//! `user.listStarred` — caller's starred repos, newest first (D-SOC-03).

use oxidean_core::{AppError, ListStarredRequest, RepoListMineResponse};

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::repo::{
    effective_capability, enrich_social, meets, resolve_owner_slug, to_public, AccessibleRepo,
    Capability,
};
use oxidean_api_core::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    tracing::error!("listStarred db error: {e}");
    AppError::new("user.internal", "could not list starred repositories")
}

pub async fn list_starred(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoListMineResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: ListStarredRequest = serde_json::from_value(input).unwrap_or(ListStarredRequest {
        offset: None,
        limit: None,
    });
    let offset = req.offset.unwrap_or(0).max(0);
    let limit = req.limit.unwrap_or(30).clamp(1, 50);

    let ids = ctx
        .db
        .list_starred_repo_ids(&user.id, offset, limit)
        .await
        .map_err(db_err)?;

    let mut repos = Vec::new();
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
        let Ok(capability) = effective_capability(&ctx.db, Some(&user.id), &row, &owner).await
        else {
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
        let enriched = enrich_social(ctx, to_public(&accessible), Some(&user.id)).await?;
        repos.push(enriched);
    }

    Ok(RepoListMineResponse { repos })
}
