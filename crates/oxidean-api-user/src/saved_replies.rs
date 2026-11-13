//! `savedReplies.*` — per-user saved comment replies (GitHub "saved
//! replies"). Every procedure is session-scoped to the caller's own rows;
//! anonymous callers get 401 like other session-bound reads.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{
    AppError, SavedRepliesListResponse, SavedReplyCreateRequest, SavedReplyDeleteResponse,
    SavedReplyPublic, SavedReplyRefRequest, SavedReplyUpdateRequest,
};
use uuid::Uuid;

const NAME_MAX_CHARS: usize = 80;
const BODY_MAX_CHARS: usize = 65_536;
const MAX_REPLIES: i64 = 500;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("saved replies db error: {e}");
        AppError::new("saved_replies.internal", "saved reply operation failed")
    }
}

fn require_session(
    ctx: &RpcCtx,
) -> Result<&oxidean_api_core::auth::session::ResolvedSession, AppError> {
    ctx.session
        .as_ref()
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

fn to_public(row: oxidean_db::SavedReplyRow) -> SavedReplyPublic {
    SavedReplyPublic {
        id: row.id,
        name: row.name,
        body: row.body,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn validate_name(raw: &str) -> Result<String, AppError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(AppError::new("rpc.bad_input", "name must not be empty"));
    }
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("name exceeds {NAME_MAX_CHARS} characters"),
        ));
    }
    Ok(name.to_string())
}

fn validate_body(raw: &str) -> Result<String, AppError> {
    let body = raw.trim();
    if body.is_empty() {
        return Err(AppError::new("rpc.bad_input", "body must not be empty"));
    }
    if body.chars().count() > BODY_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("body exceeds {BODY_MAX_CHARS} characters"),
        ));
    }
    Ok(body.to_string())
}

fn reply_not_found() -> AppError {
    AppError::new("saved_replies.not_found", "Saved reply not found")
}

/// `savedReplies.list` — the caller's replies, most recently updated first.
pub async fn list(ctx: &RpcCtx) -> Result<SavedRepliesListResponse, AppError> {
    let session = require_session(ctx)?;
    let rows = ctx
        .db
        .list_saved_replies(&session.user_id)
        .await
        .map_err(db_err)?;
    Ok(SavedRepliesListResponse {
        replies: rows.into_iter().map(to_public).collect(),
    })
}

/// `savedReplies.create` — session-required; caps the per-user list so a
/// runaway client can't grow the table unboundedly.
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<SavedReplyPublic, AppError> {
    let session = require_session(ctx)?;
    let req: SavedReplyCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid savedReplies.create input: {e}"),
        )
    })?;
    let name = validate_name(&req.name)?;
    let body = validate_body(&req.body)?;
    let existing = ctx
        .db
        .list_saved_replies(&session.user_id)
        .await
        .map_err(db_err)?;
    if existing.len() as i64 >= MAX_REPLIES {
        return Err(AppError::new(
            "saved_replies.limit",
            format!("saved reply limit reached ({MAX_REPLIES})"),
        ));
    }
    let row = ctx
        .db
        .insert_saved_reply(&Uuid::new_v4().to_string(), &session.user_id, &name, &body)
        .await
        .map_err(db_err)?;
    Ok(to_public(row))
}

/// `savedReplies.update` — rename and/or rewrite the caller's reply.
pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<SavedReplyPublic, AppError> {
    let session = require_session(ctx)?;
    let req: SavedReplyUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid savedReplies.update input: {e}"),
        )
    })?;
    let current = ctx
        .db
        .find_saved_reply(&session.user_id, &req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(reply_not_found)?;
    let name = match req.name.as_deref() {
        Some(raw) => validate_name(raw)?,
        None => current.name,
    };
    let body = match req.body.as_deref() {
        Some(raw) => validate_body(raw)?,
        None => current.body,
    };
    let row = ctx
        .db
        .update_saved_reply(&session.user_id, &req.id, &name, &body)
        .await
        .map_err(db_err)?
        .ok_or_else(reply_not_found)?;
    Ok(to_public(row))
}

/// `savedReplies.delete` — owner-scoped; foreign ids 404 (anti-enumeration).
pub async fn delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<SavedReplyDeleteResponse, AppError> {
    let session = require_session(ctx)?;
    let req: SavedReplyRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid savedReplies.delete input: {e}"),
        )
    })?;
    let removed = ctx
        .db
        .delete_saved_reply(&session.user_id, &req.id)
        .await
        .map_err(db_err)?;
    if !removed {
        return Err(reply_not_found());
    }
    Ok(SavedReplyDeleteResponse { ok: true })
}
