//! `user.listPinnedRepos` / `user.setPinnedRepos` — GitHub-style pinned
//! repositories on `/{username}` (COL-04 profile half). The list is
//! anonymous-safe: private pins only resolve for viewers who can read the
//! repo — the same ACL filter `repo.listByOwner` applies.

use oxidean_core::{
    AppError, ListPinnedReposRequest, RepoListMineResponse, RepoPublic, SetPinnedReposRequest,
    MAX_PINNED_REPOS,
};

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::repo::{
    effective_capability, enrich_social, meets, resolve_owner_slug, to_public, AccessibleRepo,
    Capability,
};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_db::users::UserRow;

fn db_err(e: String) -> AppError {
    tracing::error!("pinned repos db error: {e}");
    AppError::new("user.internal", "could not load pinned repositories")
}

/// Resolve the pinned list for `user`, ACL-filtered for `viewer_id`.
async fn pinned_repos_for(
    ctx: &RpcCtx,
    user: &UserRow,
    viewer_id: Option<&str>,
) -> Result<Vec<RepoPublic>, AppError> {
    let ids = ctx
        .db
        .list_pinned_repo_ids(&user.id)
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
        let owner_username = if row.owner_type == "org" {
            ctx.db
                .find_organization_by_id(&row.owner_id)
                .await
                .ok()
                .flatten()
                .map(|o| o.slug)
        } else {
            ctx.db
                .find_user_by_id(&row.owner_id)
                .await
                .ok()
                .flatten()
                .map(|u| u.username)
        };
        let Some(owner_username) = owner_username else {
            continue;
        };
        let Ok(Some(owner)) = resolve_owner_slug(&ctx.db, &owner_username).await else {
            continue;
        };
        let Ok(capability) = effective_capability(&ctx.db, viewer_id, &row, &owner).await else {
            continue;
        };
        if !meets(capability, Capability::Read) {
            continue;
        }
        let accessible = AccessibleRepo {
            row,
            owner_username,
            capability,
        };
        repos.push(enrich_social(ctx, to_public(&accessible), viewer_id).await?);
    }
    Ok(repos)
}

/// `user.listPinnedRepos` — public profile pins for `/{username}`.
pub async fn list_pinned(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoListMineResponse, AppError> {
    let req: ListPinnedReposRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid listPinnedRepos input: {e}"),
        )
    })?;
    let username = req.username.trim();
    if username.is_empty() {
        return Err(AppError::new("user.not_found", "User not found"));
    }
    let user = ctx
        .db
        .find_user_by_username(username)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("user.not_found", "User not found"))?;
    if user.banned_at.is_some() {
        return Err(AppError::new("user.not_found", "User not found"));
    }
    let viewer_id = ctx.session.as_ref().map(|s| s.user_id.as_str());
    let repos = pinned_repos_for(ctx, &user, viewer_id).await?;
    Ok(RepoListMineResponse { repos })
}

/// `user.setPinnedRepos` — replace the caller's pins (ordered, ≤
/// `MAX_PINNED_REPOS`, repos the caller owns — matching GitHub's pin dialog,
/// which only offers your own repositories).
pub async fn set_pinned(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoListMineResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: SetPinnedReposRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid setPinnedRepos input: {e}"),
        )
    })?;

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
            "user.too_many_pins",
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
        if row.owner_type != "user" || row.owner_id != user.id {
            return Err(AppError::new(
                "repo.forbidden",
                "only repositories you own can be pinned",
            ));
        }
    }

    ctx.db
        .set_pinned_repos(&user.id, &ids)
        .await
        .map_err(db_err)?;

    let repos = pinned_repos_for(ctx, &user, Some(&user.id)).await?;
    Ok(RepoListMineResponse { repos })
}
