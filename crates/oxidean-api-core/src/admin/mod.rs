//! Shared sys-admin helpers (`db_err`, `reject_if_banned`, `require_admin_user`).
//! The `admin.users.*` / `admin.invites.*` handlers moved to `oxidean-api-admin`
//! (#162); these helpers stay because core modules (`actions::templates`)
//! call them across domain boundaries.

use oxidean_core::AppError;
use oxidean_db::UserRow;

use crate::rpc::RpcCtx;

pub fn db_err(e: String) -> AppError {
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
    let session = ctx
        .session
        .as_ref()
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    reject_if_banned(&user)?;
    Ok(user)
}

/// Invite-create input normalization shared by `admin.invites.*`,
/// `org.invites.*` and `repo.invites.*` across domain crates (#162).
pub mod invites {
    use chrono::Utc;
    use oxidean_core::AppError;

    /// Validate + normalize a caller-supplied expiry to UTC RFC-3339 seconds.
    /// `None` -> `None` (never expires). Past timestamps are rejected.
    pub fn normalize_expiry(raw: Option<&str>) -> Result<Option<String>, AppError> {
        let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let dt = chrono::DateTime::parse_from_rfc3339(raw)
            .map(|d| d.with_timezone(&Utc))
            .or_else(|_| {
                chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map(|d| {
                    d.and_hms_opt(23, 59, 59)
                        .unwrap_or_else(|| d.and_hms_opt(0, 0, 0).unwrap_or_default())
                        .and_utc()
                })
            })
            .map_err(|_| {
                AppError::new(
                    "rpc.bad_input",
                    "expires_at must be an ISO-8601 date or timestamp",
                )
            })?;
        if dt <= Utc::now() {
            return Err(AppError::new(
                "rpc.bad_input",
                "expires_at must be in the future",
            ));
        }
        Ok(Some(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)))
    }

    /// Validate a seat cap: positive when present.
    pub fn normalize_max_uses(raw: Option<i64>) -> Result<Option<i64>, AppError> {
        match raw {
            Some(n) if n < 1 => Err(AppError::new(
                "rpc.bad_input",
                "max_uses must be a positive integer",
            )),
            other => Ok(other),
        }
    }
}
