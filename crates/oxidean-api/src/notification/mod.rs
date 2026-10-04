//! `notification.*` RPCs — list / unreadCount / markRead / markAllRead (NOTF-02 / D-12 / D-15).

use oxidean_core::{
    AppError, NotificationListRequest, NotificationListResponse, NotificationMarkAllReadResponse,
    NotificationMarkReadRequest, NotificationMarkReadResponse, NotificationPublic,
    NotificationUnreadCountResponse,
};
use oxidean_db::NotificationRow;

use crate::notify;
use crate::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!(error = %e, "notification db error");
        AppError::new("notification.internal", "notification operation failed")
    }
}

fn require_session_user_id(ctx: &RpcCtx) -> Result<&str, AppError> {
    ctx.session
        .as_ref()
        .map(|s| s.user_id.as_str())
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

async fn owner_slug_for_repo(
    ctx: &RpcCtx,
    repo_id: &str,
) -> Result<Option<(String, String)>, AppError> {
    let Some(repo) = ctx
        .db
        .find_repository_by_id(repo_id)
        .await
        .map_err(db_err)?
    else {
        return Ok(None);
    };
    let owner = if repo.owner_type == "org" {
        ctx.db
            .find_organization_by_id(&repo.owner_id)
            .await
            .map_err(db_err)?
            .map(|o| o.slug)
            .unwrap_or_default()
    } else {
        ctx.db
            .find_user_by_id(&repo.owner_id)
            .await
            .map_err(db_err)?
            .map(|u| u.username)
            .unwrap_or_default()
    };
    Ok(Some((owner, repo.name)))
}

async fn row_to_public(
    ctx: &RpcCtx,
    row: &NotificationRow,
) -> Result<Option<NotificationPublic>, AppError> {
    // A subject repo deleted between the read-time prune and this map is a
    // raced stale row — drop it rather than failing the whole list.
    let Some((owner, repo)) = owner_slug_for_repo(ctx, &row.subject_repo_id).await? else {
        return Ok(None);
    };
    let actor_username = match ctx.db.find_user_by_id(&row.actor_id).await {
        Ok(Some(u)) => u.username,
        Ok(None) => String::new(),
        Err(e) => return Err(db_err(e)),
    };
    Ok(Some(NotificationPublic {
        id: row.id.clone(),
        reason: row.reason.clone(),
        subject_kind: row.subject_kind.clone(),
        subject_repo_id: row.subject_repo_id.clone(),
        owner,
        repo,
        subject_number: row.subject_number,
        subject_title: row.subject_title.clone(),
        subject_ref: row.subject_ref.clone(),
        actor_id: row.actor_id.clone(),
        actor_username,
        created_at: row.created_at.clone(),
        read_at: row.read_at.clone(),
    }))
}

/// `notification.list` — own rows only; filter `unread` (default) | `all` (D-09 / D-15).
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<NotificationListResponse, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let req: NotificationListRequest = serde_json::from_value(input).unwrap_or(NotificationListRequest {
        filter: None,
        offset: None,
        limit: None,
    });
    let filter = req
        .filter
        .as_deref()
        .unwrap_or("unread")
        .trim()
        .to_ascii_lowercase();
    let unread_only = match filter.as_str() {
        "all" => false,
        "unread" | "" => true,
        other => {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("filter must be unread or all, got {other}"),
            ))
        }
    };
    let offset = req.offset.unwrap_or(0);
    let limit = req.limit.unwrap_or(30);
    // DEBT-06 follow-up: re-check repository read access at read time — stale
    // rows for repos the recipient can no longer read are pruned (watch row
    // dropped GitHub-style) before listing.
    notify::prune_stale_notifications(&ctx.db, user_id).await;
    let (rows, total) = ctx
        .db
        .list_notifications(user_id, unread_only, offset, limit)
        .await
        .map_err(db_err)?;
    let mut notifications = Vec::with_capacity(rows.len());
    for row in &rows {
        if let Some(n) = row_to_public(ctx, row).await? {
            notifications.push(n);
        }
    }
    Ok(NotificationListResponse {
        notifications,
        total,
    })
}

/// `notification.unreadCount` — badge count for session user (D-14 / D-15).
pub async fn unread_count(
    ctx: &RpcCtx,
    _input: serde_json::Value,
) -> Result<NotificationUnreadCountResponse, AppError> {
    let user_id = require_session_user_id(ctx)?;
    // Same read-time access re-check as `notification.list` so the badge never
    // counts rows for repos the recipient can no longer read (DEBT-06).
    notify::prune_stale_notifications(&ctx.db, user_id).await;
    let count = ctx
        .db
        .notification_unread_count(user_id)
        .await
        .map_err(db_err)?;
    Ok(NotificationUnreadCountResponse { count })
}

/// `notification.markRead` — mark own ids only; foreign ids are no-ops (T-17-01).
pub async fn mark_read(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<NotificationMarkReadResponse, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let req: NotificationMarkReadRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid notification.markRead input: {e}"),
        )
    })?;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let marked = ctx
        .db
        .mark_notifications_read(user_id, &req.ids, &now)
        .await
        .map_err(db_err)?;
    Ok(NotificationMarkReadResponse { marked })
}

/// `notification.markAllRead` — clear all unread for session user (D-12).
pub async fn mark_all_read(
    ctx: &RpcCtx,
    _input: serde_json::Value,
) -> Result<NotificationMarkAllReadResponse, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let marked = ctx
        .db
        .mark_all_notifications_read(user_id, &now)
        .await
        .map_err(db_err)?;
    Ok(NotificationMarkAllReadResponse { marked })
}
