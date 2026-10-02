//! User follow graph + watch matrix RPCs (DEBT-06):
//! `user.follow` / `user.unfollow` — idempotent edge writes;
//! `user.followers.list` / `user.following.list` — public paginated lists;
//! `user.listWatched` — caller's repo subscriptions for the settings matrix.

use oxidean_core::{
    AppError, ListWatchedRequest, PublicUserProfile, RepoListMineResponse, UserFollowListRequest,
    UserFollowListResponse, UserFollowPublic, UserFollowRequest,
};

use crate::auth::gate::require_verified;
use crate::auth::profile::public_profile_for;
use crate::repo::{
    effective_capability, enrich_social, meets, resolve_owner_slug, to_public, AccessibleRepo,
    Capability,
};
use crate::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!(error = %e, "user follow db error");
        AppError::new("user.internal", "user operation failed")
    }
}

fn clamp_page(offset: Option<i64>, limit: Option<i64>) -> (i64, i64) {
    let offset = offset.unwrap_or(0).max(0);
    let limit = limit.unwrap_or(30).clamp(1, 100);
    (offset, limit)
}

fn avatar_url(avatar_path: &Option<String>, user_id: &str) -> Option<String> {
    if avatar_path.is_some() {
        Some(format!("/uploads/avatars/{user_id}.webp"))
    } else {
        None
    }
}

fn parse_follow_request(input: serde_json::Value, proc: &str) -> Result<UserFollowRequest, AppError> {
    serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid {proc} input: {e}")))
}

async fn resolve_target(ctx: &RpcCtx, username: &str) -> Result<oxidean_db::UserRow, AppError> {
    let username = username.trim();
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
    Ok(user)
}

/// `user.follow` — idempotent edge create; returns the target's public profile
/// with refreshed counts and `viewer_is_following` (DEBT-06).
pub async fn follow(ctx: &RpcCtx, input: serde_json::Value) -> Result<PublicUserProfile, AppError> {
    let user = require_verified(ctx).await?;
    let req = parse_follow_request(input, "user.follow")?;
    let target = resolve_target(ctx, &req.username).await?;
    if target.id == user.id {
        return Err(AppError::new(
            "user.self_follow",
            "You cannot follow yourself",
        ));
    }
    ctx.db
        .follow_user(&user.id, &target.id)
        .await
        .map_err(db_err)?;
    public_profile_for(ctx, &target, Some(&user.id)).await
}

/// `user.unfollow` — idempotent edge delete; returns the target's profile (DEBT-06).
pub async fn unfollow(ctx: &RpcCtx, input: serde_json::Value) -> Result<PublicUserProfile, AppError> {
    let user = require_verified(ctx).await?;
    let req = parse_follow_request(input, "user.unfollow")?;
    let target = resolve_target(ctx, &req.username).await?;
    ctx.db
        .unfollow_user(&user.id, &target.id)
        .await
        .map_err(db_err)?;
    public_profile_for(ctx, &target, Some(&user.id)).await
}

fn parse_list_request(
    input: serde_json::Value,
    proc: &str,
) -> Result<UserFollowListRequest, AppError> {
    serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid {proc} input: {e}")))
}

async fn follow_list(
    ctx: &RpcCtx,
    req: UserFollowListRequest,
    followers: bool,
) -> Result<UserFollowListResponse, AppError> {
    let user = resolve_target(ctx, &req.username).await?;
    let (offset, limit) = clamp_page(req.offset, req.limit);
    let q = req.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let (total, rows) = if followers {
        let total = ctx
            .db
            .count_user_followers(&user.id, q)
            .await
            .map_err(db_err)?;
        let rows = ctx
            .db
            .list_user_followers(&user.id, q, offset, limit)
            .await
            .map_err(db_err)?;
        (total, rows)
    } else {
        let total = ctx
            .db
            .count_user_following(&user.id, q)
            .await
            .map_err(db_err)?;
        let rows = ctx
            .db
            .list_user_following(&user.id, q, offset, limit)
            .await
            .map_err(db_err)?;
        (total, rows)
    };
    let users = rows
        .into_iter()
        .map(|r| {
            let display_name = if r.display_name.trim().is_empty() {
                r.username.clone()
            } else {
                r.display_name
            };
            UserFollowPublic {
                user_id: r.user_id.clone(),
                username: r.username,
                display_name,
                avatar_url: avatar_url(&r.avatar_path, &r.user_id),
                followed_at: r.followed_at,
            }
        })
        .collect();
    Ok(UserFollowListResponse { users, total })
}

/// `user.followers.list` — public, paginated (DEBT-06).
pub async fn followers_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<UserFollowListResponse, AppError> {
    let req = parse_list_request(input, "user.followers.list")?;
    follow_list(ctx, req, true).await
}

/// `user.following.list` — public, paginated (DEBT-06).
pub async fn following_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<UserFollowListResponse, AppError> {
    let req = parse_list_request(input, "user.following.list")?;
    follow_list(ctx, req, false).await
}

/// `user.listWatched` — caller's subscription rows at any level (including
/// `ignore` so the settings matrix can manage them), repos they can still
/// read, newest first. Each `RepoPublic` carries `viewer_watch_level` (DEBT-06).
pub async fn list_watched(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoListMineResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: ListWatchedRequest = serde_json::from_value(input).unwrap_or(ListWatchedRequest {
        offset: None,
        limit: None,
    });
    let offset = req.offset.unwrap_or(0).max(0);
    let limit = req.limit.unwrap_or(30).clamp(1, 50);

    let ids = ctx
        .db
        .list_watched_repo_ids(&user.id, offset, limit)
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
