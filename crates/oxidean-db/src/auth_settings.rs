//! Singleton instance auth settings (id = 1).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct AuthSettingsRow {
    pub provider_mode: String,
    pub email_provider: String,
    pub from_address: Option<String>,
    pub oidc_issuer: Option<String>,
    pub oidc_client_id: Option<String>,
    pub workos_client_id: Option<String>,
    pub allow_signup: bool,
    /// Instance default visibility for new repos (D-08); defaults to `public`.
    pub default_visibility: String,
    pub updated_at: String,
}

macro_rules! map_settings {
    ($row:expr) => {{
        let row = $row;
        let allow_signup = row
            .try_get::<bool, _>("allow_signup")
            .or_else(|_| {
                row.try_get::<i64, _>("allow_signup")
                    .map(|v| v != 0)
                    .or_else(|_| row.try_get::<i8, _>("allow_signup").map(|v| v != 0))
            })
            .map_err(|e| format!("auth settings row: {e}"))?;
        AuthSettingsRow {
            provider_mode: row
                .try_get("provider_mode")
                .map_err(|e| format!("auth settings row: {e}"))?,
            email_provider: row
                .try_get("email_provider")
                .map_err(|e| format!("auth settings row: {e}"))?,
            from_address: row
                .try_get("from_address")
                .map_err(|e| format!("auth settings row: {e}"))?,
            oidc_issuer: row
                .try_get("oidc_issuer")
                .map_err(|e| format!("auth settings row: {e}"))?,
            oidc_client_id: row
                .try_get("oidc_client_id")
                .map_err(|e| format!("auth settings row: {e}"))?,
            workos_client_id: row
                .try_get("workos_client_id")
                .map_err(|e| format!("auth settings row: {e}"))?,
            allow_signup,
            default_visibility: row
                .try_get("default_visibility")
                .map_err(|e| format!("auth settings row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("auth settings row: {e}"))?,
        }
    }};
}

const SETTINGS_SELECT_PG: &str = "SELECT provider_mode, email_provider, from_address, oidc_issuer, oidc_client_id, workos_client_id, allow_signup, default_visibility,
       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM instance_auth_settings WHERE id = 1";

const SETTINGS_SELECT_MYSQL: &str = "SELECT provider_mode, email_provider, from_address, oidc_issuer, oidc_client_id, workos_client_id, allow_signup, default_visibility,
       DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM instance_auth_settings WHERE id = 1";

const SETTINGS_SELECT_SQLITE: &str = "SELECT provider_mode, email_provider, from_address, oidc_issuer, oidc_client_id, workos_client_id, allow_signup, default_visibility,
       strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
FROM instance_auth_settings WHERE id = 1";

pub async fn get(pool: &DbPool) -> Result<AuthSettingsRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(SETTINGS_SELECT_PG)
                .fetch_one(p)
                .await
                .map_err(|e| format!("get auth settings failed: {e}"))?;
            Ok(map_settings!(&row))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(SETTINGS_SELECT_MYSQL)
                .fetch_one(p)
                .await
                .map_err(|e| format!("get auth settings failed: {e}"))?;
            Ok(map_settings!(&row))
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(SETTINGS_SELECT_SQLITE)
                .fetch_one(p)
                .await
                .map_err(|e| format!("get auth settings failed: {e}"))?;
            Ok(map_settings!(&row))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn update(
    pool: &DbPool,
    provider_mode: &str,
    email_provider: &str,
    from_address: Option<&str>,
    oidc_issuer: Option<&str>,
    oidc_client_id: Option<&str>,
    workos_client_id: Option<&str>,
    allow_signup: bool,
    default_visibility: &str,
) -> Result<AuthSettingsRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE instance_auth_settings
SET provider_mode = $1, email_provider = $2, from_address = $3,
    oidc_issuer = $4, oidc_client_id = $5, workos_client_id = $6, allow_signup = $7,
    default_visibility = $8, updated_at = now()
WHERE id = 1",
            )
            .bind(provider_mode)
            .bind(email_provider)
            .bind(from_address)
            .bind(oidc_issuer)
            .bind(oidc_client_id)
            .bind(workos_client_id)
            .bind(allow_signup)
            .bind(default_visibility)
            .execute(p)
            .await
            .map_err(|e| format!("update auth settings failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE instance_auth_settings
SET provider_mode = ?, email_provider = ?, from_address = ?,
    oidc_issuer = ?, oidc_client_id = ?, workos_client_id = ?, allow_signup = ?,
    default_visibility = ?, updated_at = NOW()
WHERE id = 1",
            )
            .bind(provider_mode)
            .bind(email_provider)
            .bind(from_address)
            .bind(oidc_issuer)
            .bind(oidc_client_id)
            .bind(workos_client_id)
            .bind(allow_signup)
            .bind(default_visibility)
            .execute(p)
            .await
            .map_err(|e| format!("update auth settings failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE instance_auth_settings
SET provider_mode = ?1, email_provider = ?2, from_address = ?3,
    oidc_issuer = ?4, oidc_client_id = ?5, workos_client_id = ?6, allow_signup = ?7,
    default_visibility = ?8,
    updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = 1",
            )
            .bind(provider_mode)
            .bind(email_provider)
            .bind(from_address)
            .bind(oidc_issuer)
            .bind(oidc_client_id)
            .bind(workos_client_id)
            .bind(allow_signup)
            .bind(default_visibility)
            .execute(p)
            .await
            .map_err(|e| format!("update auth settings failed: {e}"))?;
        }
    }
    get(pool).await
}
