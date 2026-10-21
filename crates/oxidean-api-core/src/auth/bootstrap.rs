//! Empty-instance bootstrap status + one-time setup wizard (AUTH-07) plus
//! ENV forced credential confirm (AUTH-06).
//!
//! The RPC handlers moved to `oxidean-api-auth` (#162); the machinery below
//! stays in core — the dispatch gate (`bootstrap::needs_setup`) and `admin`
//! settings handlers (`admin_env_configured`) consume it across domain
//! boundaries.

use std::sync::{Mutex, MutexGuard};

use oxidean_core::AppError;
use oxidean_db::Database;

/// Default username assigned by ENV seed — must be changed on confirm (D-15/D-16).
pub const SEEDED_ADMIN_USERNAME: &str = "system-administrator";

pub fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("bootstrap db error: {e}");
        AppError::new("auth.internal", "bootstrap operation failed")
    }
}

pub fn map_username_err(msg: String) -> AppError {
    if msg.contains("reserved") {
        AppError::new("auth.reserved_username", "username is reserved")
    } else {
        AppError::new("auth.invalid_username", msg)
    }
}

/// Serialize integration tests that mutate `OXIDEAN_ADMIN_*`.
pub fn lock_admin_env_for_tests() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// True when both admin ENV vars are non-empty (AUTH-06 seed path).
pub fn admin_env_configured() -> bool {
    let email_ok = std::env::var("OXIDEAN_ADMIN_EMAIL")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let password_ok = std::env::var("OXIDEAN_ADMIN_PASSWORD")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    email_ok && password_ok
}

/// `needs_setup` when the users table is empty and ENV seed is not configured.
///
/// Skipped / unconfigured databases are not empty-instance wizard targets — return
/// `false` so health/echo smoke paths and `Database::skipped()` test routers keep
/// working. Real empty SQLite/Postgres/MySQL pools still report `true` (D-11).
pub async fn needs_setup(db: &Database) -> Result<bool, AppError> {
    if admin_env_configured() {
        return Ok(false);
    }
    match db.count_users().await {
        Ok(count) => Ok(count == 0),
        Err(e) if e.contains("not configured") => Ok(false),
        Err(e) => Err(db_err(e)),
    }
}
