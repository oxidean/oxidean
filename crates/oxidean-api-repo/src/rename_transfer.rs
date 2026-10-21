//! `repo.rename` / `repo.transfer` handlers (issue #162).

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::git::bare_repo_path;
use oxidean_api_core::repo::{
    acl::{meets, AccessibleRepo, Capability},
    rename_transfer::{db_err, insert_rename_redirect},
    resolve_repo_for_admin,
};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{
    validate_repo_name, AppError, OwnerType, RepoPublic, RepoRenameRequest, RepoRenameResponse,
    RepoTransferRequest, RepoTransferResponse, RepoVisibility,
};

async fn to_public(ctx: &RpcCtx, repo: &AccessibleRepo) -> Result<RepoPublic, AppError> {
    let flags = ctx
        .db
        .get_repo_unit_flags(&repo.row.id)
        .await
        .map_err(db_err)?;
    let visibility = RepoVisibility::parse(&repo.row.visibility).unwrap_or(RepoVisibility::Public);
    let owner_type = OwnerType::parse(&repo.row.owner_type).unwrap_or(OwnerType::User);
    Ok(RepoPublic {
        id: repo.row.id.clone(),
        owner_id: repo.row.owner_id.clone(),
        owner_type,
        owner_username: repo.owner_username.clone(),
        name: repo.row.name.clone(),
        description: repo.row.description.clone(),
        visibility,
        default_branch: repo.row.default_branch.clone(),
        updated_at: repo.row.updated_at.clone(),
        can_admin: meets(repo.capability, Capability::Admin),
        can_write: meets(repo.capability, Capability::Write),
        star_count: 0,
        open_issue_count: 0,
        open_pull_count: 0,
        viewer_has_starred: false,
        is_fork: false,
        is_template: false,
        archived: repo.row.archived,
        homepage: String::new(),
        topics: Vec::new(),
        fork_count: 0,
        watch_count: 0,
        viewer_is_watching: false,
        viewer_watch_level: None,
        issues_enabled: flags.issues_enabled,
        pulls_enabled: flags.pulls_enabled,
        wiki_enabled: flags.wiki_enabled,
        projects_enabled: flags.projects_enabled,
        discussions_enabled: flags.discussions_enabled,
        fork_network_id: None,
        forked_from: None,
    })
}

/// `repo.rename` — Admin moves bare dir + DB name and inserts retention redirect (GIT-16).
pub async fn rename(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoRenameResponse, AppError> {
    let req: RepoRenameRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid repo.rename input: {e}")))?;

    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    validate_repo_name(&req.new_name).map_err(|msg| AppError::new("repo.invalid_name", msg))?;
    let new_name = req.new_name.trim().to_string();
    if new_name == accessible.row.name {
        return Ok(RepoRenameResponse {
            repo: to_public(ctx, &accessible).await?,
        });
    }

    if ctx
        .db
        .find_repository_by_owner_name(&accessible.row.owner_id, &new_name)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "repo.name_taken",
            "A repository with this name already exists. Choose a different name.",
        ));
    }

    let old_name = accessible.row.name.clone();
    let owner_slug = accessible.owner_username.clone();
    let old_path = bare_repo_path(&ctx.repos_dir, &owner_slug, &old_name)?;
    let new_path = bare_repo_path(&ctx.repos_dir, &owner_slug, &new_name)?;

    if tokio::fs::try_exists(&new_path).await.unwrap_or(false) {
        return Err(AppError::new(
            "repo.name_taken",
            "A repository with this name already exists. Choose a different name.",
        ));
    }

    // FS-then-DB (mirror username migrate): move bare dir before committing name.
    if tokio::fs::try_exists(&old_path).await.unwrap_or(false) {
        tokio::fs::rename(&old_path, &new_path).await.map_err(|e| {
            tracing::error!(
                error = %e,
                old = %old_path.display(),
                new = %new_path.display(),
                "bare repo rename failed"
            );
            AppError::new("repo.rename_failed", "failed to move repository storage")
        })?;
    }

    // The wiki bare (`{name}.wiki.git`) rides along — best-effort like the
    // redirect insert below: a missing dir is normal (wiki never created),
    // a failed move is logged rather than reverting the primary rename.
    let wiki_old = bare_repo_path(&ctx.repos_dir, &owner_slug, &format!("{old_name}.wiki"))?;
    let wiki_new = bare_repo_path(&ctx.repos_dir, &owner_slug, &format!("{new_name}.wiki"))?;
    if tokio::fs::try_exists(&wiki_old).await.unwrap_or(false)
        && !tokio::fs::try_exists(&wiki_new).await.unwrap_or(false)
    {
        if let Err(e) = tokio::fs::rename(&wiki_old, &wiki_new).await {
            tracing::error!(error = %e, "wiki bare rename failed");
        }
    }

    let updated = match ctx
        .db
        .update_repository_name(&accessible.row.id, &new_name)
        .await
    {
        Ok(row) => row,
        Err(e) => {
            if tokio::fs::try_exists(&new_path).await.unwrap_or(false) {
                let _ = tokio::fs::rename(&new_path, &old_path).await;
            }
            if tokio::fs::try_exists(&wiki_new).await.unwrap_or(false) {
                let _ = tokio::fs::rename(&wiki_new, &wiki_old).await;
            }
            return Err(db_err(e));
        }
    };

    if let Err(e) = insert_rename_redirect(ctx, &owner_slug, &old_name, &updated.id).await {
        tracing::error!(error = %e.message, "insert repository redirect failed after rename");
    }

    let capability = accessible.capability;
    Ok(RepoRenameResponse {
        repo: to_public(
            ctx,
            &AccessibleRepo {
                row: updated,
                owner_username: owner_slug,
                capability,
            },
        )
        .await?,
    })
}

async fn resolve_transfer_destination(
    ctx: &RpcCtx,
    caller_id: &str,
    dest_owner: &str,
    dest_type: OwnerType,
) -> Result<(String, OwnerType, String), AppError> {
    let slug = dest_owner.trim();
    if slug.is_empty() {
        return Err(AppError::new("rpc.bad_input", "destOwner is required"));
    }
    match dest_type {
        OwnerType::User => {
            let user = ctx
                .db
                .find_user_by_username(slug)
                .await
                .map_err(db_err)?
                .ok_or_else(|| {
                    AppError::new("repo.transfer_dest_not_found", "Destination user not found")
                })?;
            Ok((user.id, OwnerType::User, user.username))
        }
        OwnerType::Org => {
            let org = ctx
                .db
                .find_organization_by_slug(slug)
                .await
                .map_err(db_err)?
                .ok_or_else(|| {
                    AppError::new(
                        "repo.transfer_dest_not_found",
                        "Destination organization not found",
                    )
                })?;
            let member = ctx
                .db
                .find_org_member(&org.id, caller_id)
                .await
                .map_err(db_err)?;
            let allowed = member
                .as_ref()
                .map(|m| m.role == "owner" || m.role == "admin")
                .unwrap_or(false);
            if !allowed {
                return Err(AppError::new(
                    "repo.create_forbidden",
                    "You do not have permission to create a repository under this owner.",
                ));
            }
            Ok((org.id, OwnerType::Org, org.slug))
        }
    }
}

/// `repo.transfer` — Admin moves ownership + bare dir with type-confirm (GIT-17).
pub async fn transfer(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoTransferResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: RepoTransferRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid repo.transfer input: {e}")))?;

    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let confirm = req.confirm_name.trim();
    if confirm != accessible.row.name.as_str() {
        return Err(AppError::new(
            "repo.confirm_mismatch",
            "Type the repository name exactly to confirm transfer.",
        ));
    }

    let (dest_id, dest_type, dest_slug) =
        resolve_transfer_destination(ctx, &caller.id, &req.dest_owner, req.dest_owner_type).await?;

    // No-op transfer to same owner.
    if dest_id == accessible.row.owner_id
        && dest_type.as_str() == accessible.row.owner_type.as_str()
    {
        return Ok(RepoTransferResponse {
            repo: to_public(ctx, &accessible).await?,
        });
    }

    if ctx
        .db
        .find_repository_by_owner_name(&dest_id, &accessible.row.name)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "repo.name_taken",
            "A repository with this name already exists. Choose a different name.",
        ));
    }

    let old_slug = accessible.owner_username.clone();
    let name = accessible.row.name.clone();
    let old_path = bare_repo_path(&ctx.repos_dir, &old_slug, &name)?;
    let new_path = bare_repo_path(&ctx.repos_dir, &dest_slug, &name)?;

    if tokio::fs::try_exists(&new_path).await.unwrap_or(false) {
        return Err(AppError::new(
            "repo.name_taken",
            "A repository with this name already exists. Choose a different name.",
        ));
    }

    // Ensure destination owner directory exists.
    if let Some(parent) = new_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            tracing::error!(error = %e, "create dest owner dir failed");
            AppError::new(
                "repo.transfer_failed",
                "failed to prepare destination storage",
            )
        })?;
    }

    let moved = if tokio::fs::try_exists(&old_path).await.unwrap_or(false) {
        tokio::fs::rename(&old_path, &new_path).await.map_err(|e| {
            tracing::error!(error = %e, "bare repo transfer rename failed");
            AppError::new("repo.transfer_failed", "failed to move repository storage")
        })?;
        true
    } else {
        false
    };

    // Wiki bare rides along on transfer too (`{name}.wiki.git` under the
    // destination owner dir) — best-effort, same rationale as `rename`.
    let wiki_old = bare_repo_path(&ctx.repos_dir, &old_slug, &format!("{name}.wiki"))?;
    let wiki_new = bare_repo_path(&ctx.repos_dir, &dest_slug, &format!("{name}.wiki"))?;
    let wiki_moved = if tokio::fs::try_exists(&wiki_old).await.unwrap_or(false) && moved {
        match tokio::fs::rename(&wiki_old, &wiki_new).await {
            Ok(()) => true,
            Err(e) => {
                tracing::error!(error = %e, "wiki bare transfer rename failed");
                false
            }
        }
    } else {
        false
    };

    let former_owner_id = accessible.row.owner_id.clone();
    let former_was_user = accessible.row.owner_type.eq_ignore_ascii_case("user");

    let updated = match ctx
        .db
        .update_repository_owner(&accessible.row.id, &dest_id, dest_type.as_str())
        .await
    {
        Ok(row) => row,
        Err(e) => {
            if moved {
                let _ = tokio::fs::rename(&new_path, &old_path).await;
            }
            if wiki_moved {
                let _ = tokio::fs::rename(&wiki_new, &wiki_old).await;
            }
            return Err(db_err(e));
        }
    };

    if let Err(e) = insert_rename_redirect(ctx, &old_slug, &name, &updated.id).await {
        tracing::error!(error = %e.message, "insert redirect after transfer failed");
    }

    // user→user: former personal owner becomes admin collaborator (discretion).
    if former_was_user && dest_type == OwnerType::User && former_owner_id != dest_id {
        let already = ctx
            .db
            .find_repo_collaborator(&updated.id, &former_owner_id)
            .await
            .ok()
            .flatten();
        if already.is_none() {
            if let Err(e) = ctx
                .db
                .insert_repo_collaborator(&updated.id, &former_owner_id, "admin")
                .await
            {
                tracing::warn!(error = %e, "add former owner collaborator after transfer");
            }
        }
    }

    // DEBT-06: transfer changes the owner grants — watchers who can no longer
    // read (old personal owner, old org members) are auto-unwatched and their
    // notification rows dropped.
    oxidean_api_core::notify::sweep_repo_access(&ctx.db, &updated.id).await;

    let capability = accessible.capability;
    Ok(RepoTransferResponse {
        repo: to_public(
            ctx,
            &AccessibleRepo {
                row: updated,
                owner_username: dest_slug,
                capability,
            },
        )
        .await?,
    })
}
