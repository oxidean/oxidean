//! Require-verified gate for privileged RPCs (D-07, D-09, D-10).

use oxidean_core::AppError;
use oxidean_db::UserRow;

use crate::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("auth gate db error: {e}");
        AppError::new("auth.internal", "authentication failed")
    }
}

/// Require a signed-in user whose email is verified.
///
/// Unauthenticated → `auth.unauthenticated`; unverified → `auth.email_unverified`.
pub async fn require_verified(ctx: &RpcCtx) -> Result<UserRow, AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new(
            "auth.unauthenticated",
            "not authenticated",
        ));
    };
    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    if user.email_verified_at.is_none() {
        return Err(AppError::new(
            "auth.email_unverified",
            "verify your email to continue",
        ));
    }
    if user.banned_at.is_some() {
        return Err(AppError::new(
            "auth.banned",
            "This account has been suspended.",
        ));
    }
    Ok(user)
}
