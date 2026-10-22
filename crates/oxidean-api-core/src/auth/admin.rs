//! Sys-admin gate for admin-scoped RPC handlers — kept in core because
//! `templates::handlers`, `auth::two_factor` and `admin/mod.rs` all call it
//! across domain boundaries (issue #162).

use oxidean_core::AppError;

use crate::rpc::RpcCtx;

pub fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("admin auth settings db error: {e}");
        AppError::new("admin.internal", "auth settings operation failed")
    }
}
pub async fn require_admin(ctx: &RpcCtx) -> Result<(), AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new("auth.unauthenticated", "not authenticated"));
    };
    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    if !user.role.is_sys_admin() {
        return Err(AppError::new(
            "admin.forbidden",
            "You need system admin access to manage auth settings.",
        ));
    }
    if user.banned_at.is_some() {
        return Err(AppError::new(
            "auth.banned",
            "This account has been suspended.",
        ));
    }
    Ok(())
}
