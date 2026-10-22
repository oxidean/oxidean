//! Account email RPC (`email.list` / `add` / `remove` / `setPrimary` / `resendVerify`).

use oxidean_core::{AddEmailRequest, AppError, EmailIdRequest, EmailListItem};
use uuid::Uuid;

use oxidean_api_core::auth::local::normalize_email;
use oxidean_api_core::auth::verify_reset;
use oxidean_api_core::rpc::RpcCtx;

const MAX_EMAILS_PER_USER: i64 = 10;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE")
        || e.contains("unique")
        || e.contains("Duplicate")
        || e.contains("constraint")
    {
        AppError::new("email.taken", "that email address is already in use")
    } else {
        tracing::error!("email db error: {e}");
        AppError::new("email.internal", "email operation failed")
    }
}

fn require_session_user_id(ctx: &RpcCtx) -> Result<&str, AppError> {
    ctx.session
        .as_ref()
        .map(|s| s.user_id.as_str())
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

fn row_to_item(row: &oxidean_db::UserEmailRow) -> EmailListItem {
    EmailListItem {
        id: row.id.clone(),
        email: row.email.clone(),
        is_primary: row.is_primary,
        verified: row.verified_at.is_some(),
        verified_at: row.verified_at.clone(),
        created_at: row.created_at.clone(),
    }
}

async fn require_session_user(ctx: &RpcCtx) -> Result<oxidean_db::UserRow, AppError> {
    let uid = require_session_user_id(ctx)?;
    ctx.db
        .find_user_by_id(uid)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

/// `email.list` — all addresses for the signed-in user.
pub async fn list(ctx: &RpcCtx) -> Result<Vec<EmailListItem>, AppError> {
    let user = require_session_user(ctx).await?;
    let mut rows = ctx.db.list_user_emails(&user.id).await.map_err(db_err)?;
    // Heal accounts where no row is marked primary (e.g. interrupted setPrimary).
    if !rows.is_empty() && !rows.iter().any(|r| r.is_primary) {
        let heal_id = rows
            .iter()
            .find(|r| r.email.eq_ignore_ascii_case(&user.email))
            .map(|r| r.id.clone())
            .unwrap_or_else(|| rows[0].id.clone());
        if let Ok(primary) = ctx.db.set_user_email_primary(&heal_id).await {
            rows = ctx.db.list_user_emails(&user.id).await.map_err(db_err)?;
            let _ = primary;
        }
    }
    Ok(rows.iter().map(row_to_item).collect())
}

/// `email.add` — add an unverified secondary address and send verify mail.
pub async fn add(ctx: &RpcCtx, input: serde_json::Value) -> Result<EmailListItem, AppError> {
    let user = require_session_user(ctx).await?;
    let req: AddEmailRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid email.add input: {e}")))?;
    let email = normalize_email(&req.email)?;

    let count = ctx.db.count_user_emails(&user.id).await.map_err(db_err)?;
    if count >= MAX_EMAILS_PER_USER {
        return Err(AppError::new(
            "email.max_emails",
            format!("maximum of {MAX_EMAILS_PER_USER} email addresses"),
        ));
    }

    if ctx
        .db
        .find_user_email_by_address(&email)
        .await
        .map_err(db_err)?
        .is_some()
        || ctx
            .db
            .find_user_by_email(&email)
            .await
            .map_err(db_err)?
            .is_some()
    {
        return Err(AppError::new(
            "email.taken",
            "that email address is already in use",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .create_user_email(&id, &user.id, &email, false, None)
        .await
        .map_err(db_err)?;

    if let Err(e) =
        verify_reset::issue_and_send_verify_for_target(ctx, &user.id, &email, &user.username).await
    {
        tracing::error!(code = %e.code, "email.add verify send failed");
    }

    Ok(row_to_item(&row))
}

/// `email.remove` — delete a non-primary address.
pub async fn remove(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let user = require_session_user(ctx).await?;
    let req: EmailIdRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid email.remove input: {e}")))?;

    let row = ctx
        .db
        .find_user_email_by_id(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("email.not_found", "email address not found"))?;
    if row.user_id != user.id {
        return Err(AppError::new("email.not_found", "email address not found"));
    }
    if row.is_primary {
        return Err(AppError::new(
            "email.cannot_remove_primary",
            "set another verified address as primary before removing this one",
        ));
    }

    ctx.db.delete_user_email(&req.id).await.map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}

/// `email.setPrimary` — promote a verified secondary to primary.
pub async fn set_primary(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<EmailListItem, AppError> {
    let user = require_session_user(ctx).await?;
    let req: EmailIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid email.setPrimary input: {e}"),
        )
    })?;

    let row = ctx
        .db
        .find_user_email_by_id(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("email.not_found", "email address not found"))?;
    if row.user_id != user.id {
        return Err(AppError::new("email.not_found", "email address not found"));
    }
    if row.verified_at.is_none() {
        return Err(AppError::new(
            "email.not_verified",
            "verify this address before making it primary",
        ));
    }

    let primary = ctx
        .db
        .set_user_email_primary(&req.id)
        .await
        .map_err(db_err)?;
    Ok(row_to_item(&primary))
}

/// `email.resendVerify` — resend verification for a specific address.
pub async fn resend_verify(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let user = require_session_user(ctx).await?;
    let req: EmailIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid email.resendVerify input: {e}"),
        )
    })?;

    let row = ctx
        .db
        .find_user_email_by_id(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("email.not_found", "email address not found"))?;
    if row.user_id != user.id {
        return Err(AppError::new("email.not_found", "email address not found"));
    }
    if row.verified_at.is_some() {
        return Ok(serde_json::json!({ "ok": true, "already_verified": true }));
    }

    verify_reset::issue_and_send_verify_for_target(ctx, &user.id, &row.email, &user.username)
        .await?;
    Ok(serde_json::json!({ "ok": true }))
}
