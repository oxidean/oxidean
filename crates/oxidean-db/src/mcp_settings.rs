//! Singleton instance MCP settings (id = 1) — AGT-03.
//!
//! `enabled` NULL means "no admin override stored": the API layer falls back to
//! the `OXIDEAN_MCP_ENABLED` env default — the same env-default/override split
//! as `instance_lfs_settings`.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone, Default)]
pub struct McpSettingsRow {
    /// NULL = use the `OXIDEAN_MCP_ENABLED` env default (true when unset).
    pub enabled: Option<bool>,
}

/// Dialect-tolerant nullable bool decode: Postgres BOOLEAN, MySQL TINYINT(1),
/// SQLite INTEGER.
macro_rules! map_settings {
    ($row:expr) => {{
        let row = $row;
        let enabled = row
            .try_get::<Option<bool>, _>("enabled")
            .or_else(|_| {
                row.try_get::<Option<i64>, _>("enabled")
                    .map(|v| v.map(|n| n != 0))
            })
            .or_else(|_| {
                row.try_get::<Option<i8>, _>("enabled")
                    .map(|v| v.map(|n| n != 0))
            })
            .map_err(|e| format!("mcp settings row: {e}"))?;
        McpSettingsRow { enabled }
    }};
}

pub async fn get_mcp_settings(pool: &DbPool) -> Result<McpSettingsRow, String> {
    let sql = "SELECT enabled FROM instance_mcp_settings WHERE id = 1";
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sql)
                .fetch_one(p)
                .await
                .map_err(|e| format!("get_mcp_settings: {e}"))?;
            Ok(map_settings!(&row))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sql)
                .fetch_one(p)
                .await
                .map_err(|e| format!("get_mcp_settings: {e}"))?;
            Ok(map_settings!(&row))
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sql)
                .fetch_one(p)
                .await
                .map_err(|e| format!("get_mcp_settings: {e}"))?;
            Ok(map_settings!(&row))
        }
    }
}

pub async fn update_mcp_settings(
    pool: &DbPool,
    enabled: Option<bool>,
) -> Result<McpSettingsRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE instance_mcp_settings SET enabled = $1, updated_at = NOW() WHERE id = 1",
            )
            .bind(enabled)
            .execute(p)
            .await
            .map_err(|e| format!("update_mcp_settings: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE instance_mcp_settings SET enabled = ?, updated_at = CURRENT_TIMESTAMP(3) WHERE id = 1",
            )
            .bind(enabled)
            .execute(p)
            .await
            .map_err(|e| format!("update_mcp_settings: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE instance_mcp_settings SET enabled = ?, updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = 1",
            )
            .bind(enabled)
            .execute(p)
            .await
            .map_err(|e| format!("update_mcp_settings: {e}"))?;
        }
    }
    get_mcp_settings(pool).await
}
