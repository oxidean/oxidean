//! Auth identity linking for WorkOS / OIDC subjects.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct AuthIdentityRow {
    pub id: String,
    pub user_id: String,
    pub provider: String,
    pub provider_subject: String,
    pub provider_email: Option<String>,
}

macro_rules! map_identity {
    ($row:expr) => {{
        let row = $row;
        AuthIdentityRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("auth identity row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("auth identity row: {e}"))?,
            provider: row
                .try_get("provider")
                .map_err(|e| format!("auth identity row: {e}"))?,
            provider_subject: row
                .try_get("provider_subject")
                .map_err(|e| format!("auth identity row: {e}"))?,
            provider_email: row
                .try_get("provider_email")
                .map_err(|e| format!("auth identity row: {e}"))?,
        }
    }};
}

pub async fn upsert(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    provider: &str,
    provider_subject: &str,
    provider_email: Option<&str>,
) -> Result<AuthIdentityRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO auth_identities (id, user_id, provider, provider_subject, provider_email)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT (provider, provider_subject) DO UPDATE
SET user_id = EXCLUDED.user_id,
    provider_email = EXCLUDED.provider_email",
            )
            .bind(id)
            .bind(user_id)
            .bind(provider)
            .bind(provider_subject)
            .bind(provider_email)
            .execute(p)
            .await
            .map_err(|e| format!("upsert auth identity failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO auth_identities (id, user_id, provider, provider_subject, provider_email)
VALUES (?, ?, ?, ?, ?) AS new
ON DUPLICATE KEY UPDATE
  user_id = new.user_id,
  provider_email = new.provider_email",
            )
            .bind(id)
            .bind(user_id)
            .bind(provider)
            .bind(provider_subject)
            .bind(provider_email)
            .execute(p)
            .await
            .map_err(|e| format!("upsert auth identity failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO auth_identities (id, user_id, provider, provider_subject, provider_email)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT (provider, provider_subject) DO UPDATE SET
  user_id = excluded.user_id,
  provider_email = excluded.provider_email",
            )
            .bind(id)
            .bind(user_id)
            .bind(provider)
            .bind(provider_subject)
            .bind(provider_email)
            .execute(p)
            .await
            .map_err(|e| format!("upsert auth identity failed: {e}"))?;
        }
    }
    find(pool, provider, provider_subject)
        .await?
        .ok_or_else(|| "upsert auth identity failed: row missing after upsert".into())
}

pub async fn find(
    pool: &DbPool,
    provider: &str,
    provider_subject: &str,
) -> Result<Option<AuthIdentityRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT id, user_id, provider, provider_subject, provider_email
FROM auth_identities WHERE provider = $1 AND provider_subject = $2",
            )
            .bind(provider)
            .bind(provider_subject)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find auth identity failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_identity!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT id, user_id, provider, provider_subject, provider_email
FROM auth_identities WHERE provider = ? AND provider_subject = ?",
            )
            .bind(provider)
            .bind(provider_subject)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find auth identity failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_identity!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT id, user_id, provider, provider_subject, provider_email
FROM auth_identities WHERE provider = ?1 AND provider_subject = ?2",
            )
            .bind(provider)
            .bind(provider_subject)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find auth identity failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_identity!(&r)),
                None => None,
            })
        }
    }
}
