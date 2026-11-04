//! `org.listPinnedRepos` / `org.setPinnedRepos` — GitHub-style pinned
//! repositories on `/{org}` (COL-04 org half). The list is anonymous-safe:
//! private pins only resolve for viewers who can read the repo — the same
//! ACL filter `repo.listByOwner` applies. Only org Admin+ may edit, and only
//! org-owned repos can be pinned.

use oxidean_core::{
    AppError, OrgRole, OrgSlugRequest, RepoListMineResponse, RepoPublic, SetOrgPinnedReposRequest,
    MAX_PINNED_REPOS,
};

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::org::{load_org_by_slug, require_org_role};
use oxidean_api_core::repo::{
    effective_capability, enrich_social, meets, resolve_owner_slug, to_public, AccessibleRepo,
    Capability,
};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_db::organizations::OrganizationRow;

fn db_err(e: String) -> AppError {
    tracing::error!("org pinned repos db error: {e}");
    AppError::new("org.internal", "could not load pinned repositories")
}

/// Resolve the pinned list for `org`, ACL-filtered for `viewer_id`.
async fn pinned_repos_for(
    ctx: &RpcCtx,
    org: &OrganizationRow,
    viewer_id: Option<&str>,
) -> Result<Vec<RepoPublic>, AppError> {
    let ids = ctx
        .db
        .list_org_pinned_repo_ids(&org.id)
        .await
        .map_err(db_err)?;

    let mut repos = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(row) = ctx.db.find_repository_by_id(&id).await.map_err(db_err)? else {
            continue;
        };
        if row.deleted_at.is_some() {
            continue;
        }
        // Pins stay org-owned; a transfer can leave a stale pin row.
        if row.owner_type != "org" || row.owner_id != org.id {
            continue;
        }
        let owner = resolve_owner_slug(&ctx.db, &org.slug)
            .await
            .map_err(db_err)?;
        let Some(owner) = owner else { continue };
        let Ok(capability) = effective_capability(&ctx.db, viewer_id, &row, &owner).await else {
            continue;
        };
        if !meets(capability, Capability::Read) {
            continue;
        }
        let accessible = AccessibleRepo {
            row,
            owner_username: org.slug.clone(),
            capability,
        };
        repos.push(enrich_social(ctx, to_public(&accessible), viewer_id).await?);
    }
    Ok(repos)
}

/// `org.listPinnedRepos` — public profile pins for `/{org}`.
pub async fn list_pinned(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoListMineResponse, AppError> {
    let req: OrgSlugRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.listPinnedRepos input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let viewer_id = ctx.session.as_ref().map(|s| s.user_id.as_str());
    let repos = pinned_repos_for(ctx, &org, viewer_id).await?;
    Ok(RepoListMineResponse { repos })
}

/// `org.setPinnedRepos` — replace the org's pins (ordered, ≤
/// `MAX_PINNED_REPOS`, org-owned repos). Org Admin+ only.
pub async fn set_pinned(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoListMineResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: SetOrgPinnedReposRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.setPinnedRepos input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    match require_org_role(ctx, &org.id, &user.id).await? {
        OrgRole::Owner | OrgRole::Admin => {}
        OrgRole::Member => {
            return Err(AppError::new(
                "org.forbidden",
                "only organization owners and admins can edit pinned repositories",
            ));
        }
    }

    // Dedupe, preserving order — the pin list is a set with a display order.
    let mut ids: Vec<String> = Vec::new();
    for id in &req.repo_ids {
        let id = id.trim();
        if !id.is_empty() && !ids.iter().any(|i| i == id) {
            ids.push(id.to_string());
        }
    }
    if ids.len() > MAX_PINNED_REPOS {
        return Err(AppError::new(
            "org.too_many_pins",
            format!("at most {MAX_PINNED_REPOS} repositories can be pinned"),
        ));
    }

    for id in &ids {
        let row = ctx
            .db
            .find_repository_by_id(id)
            .await
            .map_err(db_err)?
            .filter(|r| r.deleted_at.is_none())
            .ok_or_else(|| AppError::new("repo.not_found", "Repository not found"))?;
        if row.owner_type != "org" || row.owner_id != org.id {
            return Err(AppError::new(
                "repo.forbidden",
                "only repositories owned by this organization can be pinned",
            ));
        }
    }

    ctx.db
        .set_org_pinned_repos(&org.id, &ids)
        .await
        .map_err(db_err)?;

    let repos = pinned_repos_for(ctx, &org, Some(&user.id)).await?;
    Ok(RepoListMineResponse { repos })
}
