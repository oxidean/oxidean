//! Paginated social lists: stargazers (Write+), watchers / forks (Read+).

use oxidean_core::{
    AppError, RepoForkPublic, RepoForksListRequest, RepoForksListResponse, RepoForksSort,
    RepoStargazerPublic, RepoStargazersListRequest, RepoStargazersListResponse, RepoWatcherPublic,
    RepoWatchersListRequest, RepoWatchersListResponse,
};
use oxidean_db::ForkListSort;

use oxidean_api_core::repo::{acl, meets, resolve_repo_for_read, Capability};
use oxidean_api_core::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!(error = %e, "social list db error");
        AppError::new("repo.internal", "repository operation failed")
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

fn fork_sort(raw: Option<&str>) -> ForkListSort {
    match RepoForksSort::parse(raw.unwrap_or("stars")) {
        RepoForksSort::Updated => ForkListSort::Updated,
        RepoForksSort::Created => ForkListSort::Created,
        RepoForksSort::Stars => ForkListSort::Stars,
    }
}

/// `repo.stargazers.list` — Write+ only (soft `repo.not_found` otherwise).
pub async fn stargazers_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoStargazersListResponse, AppError> {
    let req: RepoStargazersListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.stargazers.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(acl::not_found());
    }
    let (offset, limit) = clamp_page(req.offset, req.limit);
    let q = req.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let total = ctx
        .db
        .count_repo_stargazers(&accessible.row.id, q)
        .await
        .map_err(db_err)?;
    let rows = ctx
        .db
        .list_repo_stargazers(&accessible.row.id, q, offset, limit)
        .await
        .map_err(db_err)?;
    let stargazers = rows
        .into_iter()
        .map(|r| {
            let display_name = if r.display_name.trim().is_empty() {
                r.username.clone()
            } else {
                r.display_name
            };
            RepoStargazerPublic {
                avatar_url: avatar_url(&r.avatar_path, &r.user_id),
                user_id: r.user_id,
                username: r.username,
                display_name,
                starred_at: r.starred_at,
            }
        })
        .collect();
    Ok(RepoStargazersListResponse { stargazers, total })
}

/// `repo.watchers.list` — Read+ (public anonymous OK).
pub async fn watchers_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoWatchersListResponse, AppError> {
    let req: RepoWatchersListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.watchers.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let (offset, limit) = clamp_page(req.offset, req.limit);
    let q = req.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let total = ctx
        .db
        .count_repo_watchers(&accessible.row.id, q)
        .await
        .map_err(db_err)?;
    let rows = ctx
        .db
        .list_repo_watchers(&accessible.row.id, q, offset, limit)
        .await
        .map_err(db_err)?;
    let watchers = rows
        .into_iter()
        .map(|r| {
            let display_name = if r.display_name.trim().is_empty() {
                r.username.clone()
            } else {
                r.display_name
            };
            RepoWatcherPublic {
                avatar_url: avatar_url(&r.avatar_path, &r.user_id),
                user_id: r.user_id,
                username: r.username,
                display_name,
                watched_at: r.watched_at,
            }
        })
        .collect();
    Ok(RepoWatchersListResponse { watchers, total })
}

/// `repo.forks.list` — Read+ (public anonymous OK).
pub async fn forks_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoForksListResponse, AppError> {
    let req: RepoForksListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.forks.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let network_id = ctx
        .db
        .get_repo_fork_network_id(&accessible.row.id)
        .await
        .map_err(db_err)?
        .unwrap_or_else(|| accessible.row.id.clone());
    let (offset, limit) = clamp_page(req.offset, req.limit);
    let q = req.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let sort = fork_sort(req.sort.as_deref());
    let total = ctx
        .db
        .count_network_forks(&network_id, q)
        .await
        .map_err(db_err)?;
    let rows = ctx
        .db
        .list_network_forks(&network_id, q, sort, offset, limit)
        .await
        .map_err(db_err)?;
    let forks = rows
        .into_iter()
        .map(|r| {
            let owner_avatar_url = match (&r.owner_user_id, r.has_owner_avatar) {
                (Some(uid), true) => Some(format!("/uploads/avatars/{uid}.webp")),
                _ => None,
            };
            RepoForkPublic {
                id: r.id,
                owner_username: r.owner_username,
                name: r.name,
                description: r.description,
                star_count: r.star_count,
                fork_count: r.fork_count,
                created_at: r.created_at,
                updated_at: r.updated_at,
                owner_avatar_url,
            }
        })
        .collect();
    Ok(RepoForksListResponse { forks, total })
}
