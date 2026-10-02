//! Repository rename (+ transfer in 15-04) with bare-disk move and redirects (GIT-16/17).

use chrono::{Duration, Utc};
use oxidean_core::{
    validate_repo_name, AppError, OwnerType, RepoPublic, RepoRenameRequest, RepoRenameResponse,
    RepoTransferRequest, RepoTransferResponse, RepoVisibility,
};
use oxidean_db::Database;
use uuid::Uuid;

use super::acl::{
    lookup_repo_row_or_redirect, meets, AccessibleRepo, Capability,
};
use super::collaborators::resolve_repo_for_admin;
use crate::auth::gate::require_verified;
use crate::git::bare_repo_path;
use crate::rpc::RpcCtx;

/// Default redirect retention (D-REL-08 / RESEARCH).
pub const DEFAULT_REPO_REDIRECT_RETENTION_DAYS: u64 = 90;

pub fn redirect_retention_days() -> u64 {
    std::env::var("OXIDEAN_REPO_REDIRECT_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_REPO_REDIRECT_RETENTION_DAYS)
}

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new(
            "repo.name_taken",
            "A repository with this name already exists. Choose a different name.",
        )
    } else {
        tracing::error!("repo rename/transfer db error: {e}");
        AppError::new("repo.internal", "repository operation failed")
    }
}

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
        homepage: String::new(),
        topics: Vec::new(),
        fork_count: 0,
        watch_count: 0,
        viewer_is_watching: false,
        issues_enabled: flags.issues_enabled,
        pulls_enabled: flags.pulls_enabled,
        fork_network_id: None,
        forked_from: None,
    })
}

/// Resolve for ACL honoring redirects. Live path always wins.
pub async fn resolve_repo_or_redirect(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
    need: Capability,
) -> Result<AccessibleRepo, AppError> {
    use super::acl::not_found;
    use super::acl::effective_capability;

    let pair = match lookup_repo_row_or_redirect(&ctx.db, owner, name).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "lookup_repo_row_or_redirect failed");
            return Err(AppError::new("repo.internal", "repository operation failed"));
        }
    };
    let Some((row, owner_ref)) = pair else {
        return Err(not_found());
    };

    let caller_id = ctx.session.as_ref().map(|s| s.user_id.as_str());
    let capability = match effective_capability(&ctx.db, caller_id, &row, &owner_ref).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "effective_capability failed");
            return Err(AppError::new("repo.internal", "repository operation failed"));
        }
    };
    if !meets(capability, need) {
        return Err(not_found());
    }
    Ok(AccessibleRepo {
        row,
        owner_username: owner_ref.slug().to_string(),
        capability,
    })
}

async fn insert_rename_redirect(
    ctx: &RpcCtx,
    old_owner_slug: &str,
    old_name: &str,
    repo_id: &str,
) -> Result<(), AppError> {
    let days = redirect_retention_days() as i64;
    let expires = (Utc::now() + Duration::days(days))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let id = Uuid::new_v4().to_string();
    let _ = ctx
        .db
        .delete_repository_redirect(old_owner_slug, old_name)
        .await;
    ctx.db
        .insert_repository_redirect(&id, old_owner_slug, old_name, repo_id, &expires)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// `repo.rename` — Admin moves bare dir + DB name and inserts retention redirect (GIT-16).
pub async fn rename(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoRenameResponse, AppError> {
    let req: RepoRenameRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.rename input: {e}"))
    })?;

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

/// Delete redirect when a live repo occupies the slug/name (supersede).
pub async fn supersede_redirect_on_create(
    db: &Database,
    owner_slug: &str,
    name: &str,
) -> Result<(), String> {
    db.delete_repository_redirect(owner_slug, name).await
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
                    AppError::new("repo.transfer_dest_not_found", "Destination organization not found")
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
    let req: RepoTransferRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.transfer input: {e}"))
    })?;

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
            AppError::new("repo.transfer_failed", "failed to prepare destination storage")
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
