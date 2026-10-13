//! Singleton instance PAT expiration policy (id = 1).
//!
//! NULL columns mean "no admin override stored": the API layer falls back to
//! the `OXIDEAN_PAT_*` env defaults — the same env-default/override split as
//! `instance_mcp_settings` / `instance_lfs_settings`.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone, Default)]
pub struct PatSettingsRow {
    /// NULL = env `OXIDEAN_PAT_DEFAULT_TTL_DAYS` / built-in default.
    pub default_ttl_days: Option<i64>,
    /// NULL = env `OXIDEAN_PAT_MAX_TTL_DAYS` / built-in default.
    pub max_ttl_days: Option<i64>,
    /// NULL = env `OXIDEAN_PAT_ALLOW_NO_EXPIRY` / built-in default (false).
    pub allow_never_expire: Option<bool>,
}

/// Dialect-tolerant decode: nullable ints are i64/i32/i8 across drivers;
/// BOOLEAN is bool on Postgres, TINYINT on MySQL, INTEGER on SQLite.
macro_rules! get_int {
    ($row:expr, $col:literal) => {
        $row.try_get::<Option<i64>, _>($col)
            .or_else(|_| {
                $row.try_get::<Option<i32>, _>($col)
                    .map(|v| v.map(i64::from))
            })
            .or_else(|_| {
                $row.try_get::<Option<i8>, _>($col)
                    .map(|v| v.map(i64::from))
            })
            .map_err(|e| format!("pat settings row: {e}"))
    };
}

macro_rules! map_settings {
    ($row:expr) => {{
        let row = $row;
        let allow_never_expire = row
            .try_get::<Option<bool>, _>("allow_never_expire")
            .or_else(|_| {
                row.try_get::<Option<i64>, _>("allow_never_expire")
                    .map(|v| v.map(|n| n != 0))
            })
            .or_else(|_| {
                row.try_get::<Option<i8>, _>("allow_never_expire")
                    .map(|v| v.map(|n| n != 0))
            })
            .map_err(|e| format!("pat settings row: {e}"))?;
        PatSettingsRow {
            default_ttl_days: get_int!(row, "default_ttl_days")?,
            max_ttl_days: get_int!(row, "max_ttl_days")?,
            allow_never_expire,
        }
    }};
}

const SELECT: &str =
    "SELECT default_ttl_days, max_ttl_days, allow_never_expire FROM instance_pat_settings WHERE id = 1";

pub async fn get_pat_settings(pool: &DbPool) -> Result<PatSettingsRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(SELECT)
                .fetch_one(p)
                .await
                .map_err(|e| format!("get_pat_settings: {e}"))?;
            Ok(map_settings!(&row))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(SELECT)
                .fetch_one(p)
                .await
                .map_err(|e| format!("get_pat_settings: {e}"))?;
            Ok(map_settings!(&row))
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(SELECT)
                .fetch_one(p)
                .await
                .map_err(|e| format!("get_pat_settings: {e}"))?;
            Ok(map_settings!(&row))
        }
    }
}

pub async fn update_pat_settings(
    pool: &DbPool,
    default_ttl_days: Option<i64>,
    max_ttl_days: Option<i64>,
    allow_never_expire: Option<bool>,
) -> Result<PatSettingsRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE instance_pat_settings
SET default_ttl_days = $1, max_ttl_days = $2, allow_never_expire = $3, updated_at = NOW()
WHERE id = 1",
            )
            .bind(default_ttl_days)
            .bind(max_ttl_days)
            .bind(allow_never_expire)
            .execute(p)
            .await
            .map_err(|e| format!("update_pat_settings: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE instance_pat_settings
SET default_ttl_days = ?, max_ttl_days = ?, allow_never_expire = ?, updated_at = CURRENT_TIMESTAMP(3)
WHERE id = 1",
            )
            .bind(default_ttl_days)
            .bind(max_ttl_days)
            .bind(allow_never_expire)
            .execute(p)
            .await
            .map_err(|e| format!("update_pat_settings: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE instance_pat_settings
SET default_ttl_days = ?, max_ttl_days = ?, allow_never_expire = ?, updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = 1",
            )
            .bind(default_ttl_days)
            .bind(max_ttl_days)
            .bind(allow_never_expire)
            .execute(p)
            .await
            .map_err(|e| format!("update_pat_settings: {e}"))?;
        }
    }
    get_pat_settings(pool).await
}
