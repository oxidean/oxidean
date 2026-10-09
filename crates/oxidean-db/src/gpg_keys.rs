//! GPG / OpenPGP public key CRUD — fingerprint UNIQUE; hard-delete on revoke.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct GpgKeyRow {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub armored_public_key: String,
    pub fingerprint: String,
    pub key_id: String,
    /// JSON array of email strings from key UIDs.
    pub uid_emails: String,
    pub created_at: String,
}

macro_rules! map_gpg_key {
    ($row:expr) => {{
        let row = $row;
        GpgKeyRow {
            id: row.try_get("id").map_err(|e| format!("gpg key row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("gpg key row: {e}"))?,
            title: row
                .try_get("title")
                .map_err(|e| format!("gpg key row: {e}"))?,
            armored_public_key: row
                .try_get("armored_public_key")
                .map_err(|e| format!("gpg key row: {e}"))?,
            fingerprint: row
                .try_get("fingerprint")
                .map_err(|e| format!("gpg key row: {e}"))?,
            key_id: row
                .try_get("key_id")
                .map_err(|e| format!("gpg key row: {e}"))?,
            uid_emails: row
                .try_get("uid_emails")
                .map_err(|e| format!("gpg key row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("gpg key row: {e}"))?,
        }
    }};
}

const GPG_SELECT_PG: &str = "SELECT id, user_id, title, armored_public_key, fingerprint, key_id,
       uid_emails,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM gpg_public_keys";

const GPG_SELECT_MYSQL: &str = "SELECT id, user_id, title, armored_public_key, fingerprint, key_id,
       uid_emails,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM gpg_public_keys";

const GPG_SELECT_SQLITE: &str =
    "SELECT id, user_id, title, armored_public_key, fingerprint, key_id,
       uid_emails,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM gpg_public_keys";

/// Insert a registered GPG public key row.
#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    title: &str,
    armored_public_key: &str,
    fingerprint: &str,
    key_id: &str,
    uid_emails_json: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO gpg_public_keys
(id, user_id, title, armored_public_key, fingerprint, key_id, uid_emails)
VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(id)
            .bind(user_id)
            .bind(title)
            .bind(armored_public_key)
            .bind(fingerprint)
            .bind(key_id)
            .bind(uid_emails_json)
            .execute(p)
            .await
            .map_err(|e| format!("create gpg key failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO gpg_public_keys
(id, user_id, title, armored_public_key, fingerprint, key_id, uid_emails)
VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(user_id)
            .bind(title)
            .bind(armored_public_key)
            .bind(fingerprint)
            .bind(key_id)
            .bind(uid_emails_json)
            .execute(p)
            .await
            .map_err(|e| format!("create gpg key failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO gpg_public_keys
(id, user_id, title, armored_public_key, fingerprint, key_id, uid_emails)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(user_id)
            .bind(title)
            .bind(armored_public_key)
            .bind(fingerprint)
            .bind(key_id)
            .bind(uid_emails_json)
            .execute(p)
            .await
            .map_err(|e| format!("create gpg key failed: {e}"))?;
        }
    }
    Ok(())
}

/// Lookup by fingerprint (uppercase hex without spaces).
pub async fn find_by_fingerprint(
    pool: &DbPool,
    fingerprint: &str,
) -> Result<Option<GpgKeyRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{GPG_SELECT_PG} WHERE fingerprint = $1"
            )))
            .bind(fingerprint)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find gpg key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_gpg_key!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{GPG_SELECT_MYSQL} WHERE fingerprint = ?"
            )))
            .bind(fingerprint)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find gpg key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_gpg_key!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{GPG_SELECT_SQLITE} WHERE fingerprint = ?1"
            )))
            .bind(fingerprint)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find gpg key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_gpg_key!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Keys for a user, newest first.
pub async fn list_for_user(pool: &DbPool, user_id: &str) -> Result<Vec<GpgKeyRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{GPG_SELECT_PG} WHERE user_id = $1 ORDER BY created_at DESC, id DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list gpg keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_gpg_key!(&r));
            }
            Ok(mapped)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{GPG_SELECT_MYSQL} WHERE user_id = ? ORDER BY created_at DESC, id DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list gpg keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_gpg_key!(&r));
            }
            Ok(mapped)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{GPG_SELECT_SQLITE} WHERE user_id = ?1 ORDER BY created_at DESC, id DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list gpg keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_gpg_key!(&r));
            }
            Ok(mapped)
        }
    }
}

/// Keys for many users in one `IN (...)` round trip, newest first
/// (commit signature keyring batching).
pub async fn list_for_users(pool: &DbPool, user_ids: &[String]) -> Result<Vec<GpgKeyRow>, String> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{GPG_SELECT_PG} WHERE user_id = ANY($1) ORDER BY created_at DESC, id DESC"
            )))
            .bind(user_ids)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list gpg keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_gpg_key!(&r));
            }
            Ok(mapped)
        }
        DbPool::MySql(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::MySql, 1, user_ids.len());
            let q_str = format!(
                "{GPG_SELECT_MYSQL} WHERE user_id IN ({in_list}) ORDER BY created_at DESC, id DESC"
            );
            let q = user_ids
                .iter()
                .fold(sqlx::query(sqlx::AssertSqlSafe(&*q_str)), |q, id| {
                    q.bind(id)
                });
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list gpg keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_gpg_key!(&r));
            }
            Ok(mapped)
        }
        DbPool::Sqlite(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::Sqlite, 1, user_ids.len());
            let q_str = format!(
                "{GPG_SELECT_SQLITE} WHERE user_id IN ({in_list}) ORDER BY created_at DESC, id DESC"
            );
            let q = user_ids
                .iter()
                .fold(sqlx::query(sqlx::AssertSqlSafe(&*q_str)), |q, id| {
                    q.bind(id)
                });
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list gpg keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_gpg_key!(&r));
            }
            Ok(mapped)
        }
    }
}

/// Hard-delete by id. Idempotent when missing.
pub async fn revoke(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM gpg_public_keys WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("revoke gpg key failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM gpg_public_keys WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("revoke gpg key failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM gpg_public_keys WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("revoke gpg key failed: {e}"))?;
        }
    }
    Ok(())
}
