//! Sys-admin user management + instance invites (`admin.users.*`, `admin.invites.*`).

pub mod invites;
pub mod users;

pub use invites::{
    create as invites_create, list as invites_list, revoke as invites_revoke,
};
pub use users::{
    ban as users_ban, delete as users_delete, list as users_list,
    revoke_sessions as users_revoke_sessions, unban as users_unban,
    update_role as users_update_role,
};

use oxidean_core::AppError;
use oxidean_db::UserRow;

use crate::rpc::RpcCtx;

pub(crate) fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("admin users/invites db error: {e}");
        AppError::new("admin.internal", "admin operation failed")
    }
}

/// Reject when the account is soft-banned (`banned_at` set).
pub fn reject_if_banned(user: &UserRow) -> Result<(), AppError> {
    if user.banned_at.is_some() {
        return Err(AppError::new(
            "auth.banned",
            "This account has been suspended.",
        ));
    }
    Ok(())
}

/// Sys-admin gate that returns the caller row (for self-action guards).
pub async fn require_admin_user(ctx: &RpcCtx) -> Result<UserRow, AppError> {
    crate::auth::admin::require_admin(ctx).await?;
    let session = ctx.session.as_ref().ok_or_else(|| {
        AppError::new("auth.unauthenticated", "not authenticated")
    })?;
    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    reject_if_banned(&user)?;
    Ok(user)
}
