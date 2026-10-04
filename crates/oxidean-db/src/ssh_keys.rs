//! SSH public key CRUD via `DbPool` match — fingerprint UNIQUE (D-SSH-05).
//! Revoke is hard-delete (ASSUME A3); no `revoked_at` column.
//! `can_authenticate` / `can_sign` gate Git SSH auth vs commit signature verify.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct SshKeyRow {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub public_key: String,
    pub fingerprint: String,
    pub key_type: String,
    pub can_authenticate: bool,
    pub can_sign: bool,
    pub last_used_at: Option<String>,
    pub last_used_ip: Option<String>,
    pub created_at: String,
}

macro_rules! map_ssh_key {
    ($row:expr) => {{
        let row = $row;
        let can_authenticate = row
            .try_get::<i64, _>("can_authenticate")
            .or_else(|_| {
                row.try_get::<i8, _>("can_authenticate")
                    .map(|v| i64::from(v))
            })
            .or_else(|_| {
                row.try_get::<bool, _>("can_authenticate")
                    .map(|v| if v { 1 } else { 0 })
            })
            .unwrap_or(1)
            != 0;
        let can_sign = row
            .try_get::<i64, _>("can_sign")
            .or_else(|_| row.try_get::<i8, _>("can_sign").map(|v| i64::from(v)))
            .or_else(|_| {
                row.try_get::<bool, _>("can_sign")
                    .map(|v| if v { 1 } else { 0 })
            })
            .unwrap_or(1)
            != 0;
        SshKeyRow {
            id: row.try_get("id").map_err(|e| format!("ssh key row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("ssh key row: {e}"))?,
            title: row
                .try_get("title")
                .map_err(|e| format!("ssh key row: {e}"))?,
            public_key: row
                .try_get("public_key")
                .map_err(|e| format!("ssh key row: {e}"))?,
            fingerprint: row
                .try_get("fingerprint")
                .map_err(|e| format!("ssh key row: {e}"))?,
            key_type: row
                .try_get("key_type")
                .map_err(|e| format!("ssh key row: {e}"))?,
            can_authenticate,
            can_sign,
            last_used_at: row
                .try_get("last_used_at")
                .map_err(|e| format!("ssh key row: {e}"))?,
            last_used_ip: row
                .try_get("last_used_ip")
                .map_err(|e| format!("ssh key row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("ssh key row: {e}"))?,
        }
    }};
}

const SSH_SELECT_PG: &str = "SELECT id, user_id, title, public_key, fingerprint, key_type,
       CASE WHEN can_authenticate THEN 1 ELSE 0 END AS can_authenticate,
       CASE WHEN can_sign THEN 1 ELSE 0 END AS can_sign,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE to_char(last_used_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS last_used_at,
       last_used_ip,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM ssh_public_keys";

const SSH_SELECT_MYSQL: &str = "SELECT id, user_id, title, public_key, fingerprint, key_type,
       can_authenticate, can_sign,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE DATE_FORMAT(last_used_at, '%Y-%m-%dT%H:%i:%sZ') END AS last_used_at,
       last_used_ip,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM ssh_public_keys";

const SSH_SELECT_SQLITE: &str = "SELECT id, user_id, title, public_key, fingerprint, key_type,
       can_authenticate, can_sign,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', last_used_at) END AS last_used_at,
       last_used_ip,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM ssh_public_keys";

/// Insert a registered SSH public key row.
#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    title: &str,
    public_key: &str,
    fingerprint: &str,
    key_type: &str,
    can_authenticate: bool,
    can_sign: bool,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO ssh_public_keys
(id, user_id, title, public_key, fingerprint, key_type, can_authenticate, can_sign)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(id)
            .bind(user_id)
            .bind(title)
            .bind(public_key)
            .bind(fingerprint)
            .bind(key_type)
            .bind(can_authenticate)
            .bind(can_sign)
            .execute(p)
            .await
            .map_err(|e| format!("create ssh key failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO ssh_public_keys
(id, user_id, title, public_key, fingerprint, key_type, can_authenticate, can_sign)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(user_id)
            .bind(title)
            .bind(public_key)
            .bind(fingerprint)
            .bind(key_type)
            .bind(can_authenticate)
            .bind(can_sign)
            .execute(p)
            .await
            .map_err(|e| format!("create ssh key failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO ssh_public_keys
(id, user_id, title, public_key, fingerprint, key_type, can_authenticate, can_sign)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(id)
            .bind(user_id)
            .bind(title)
            .bind(public_key)
            .bind(fingerprint)
            .bind(key_type)
            .bind(if can_authenticate { 1 } else { 0 })
            .bind(if can_sign { 1 } else { 0 })
            .execute(p)
            .await
            .map_err(|e| format!("create ssh key failed: {e}"))?;
        }
    }
    Ok(())
}

/// Lookup by OpenSSH SHA256 fingerprint (`SHA256:…`).
pub async fn find_by_fingerprint(
    pool: &DbPool,
    fingerprint: &str,
) -> Result<Option<SshKeyRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(&format!("{SSH_SELECT_PG} WHERE fingerprint = $1"))
                .bind(fingerprint)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find ssh key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_ssh_key!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(&format!("{SSH_SELECT_MYSQL} WHERE fingerprint = ?"))
                .bind(fingerprint)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find ssh key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_ssh_key!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(&format!("{SSH_SELECT_SQLITE} WHERE fingerprint = ?1"))
                .bind(fingerprint)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find ssh key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_ssh_key!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Keys for a user, newest first (`created_at DESC`).
pub async fn list_for_user(pool: &DbPool, user_id: &str) -> Result<Vec<SshKeyRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(&format!(
                "{SSH_SELECT_PG} WHERE user_id = $1 ORDER BY created_at DESC, id DESC"
            ))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list ssh keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_ssh_key!(&r));
            }
            Ok(mapped)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(&format!(
                "{SSH_SELECT_MYSQL} WHERE user_id = ? ORDER BY created_at DESC, id DESC"
            ))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list ssh keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_ssh_key!(&r));
            }
            Ok(mapped)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(&format!(
                "{SSH_SELECT_SQLITE} WHERE user_id = ?1 ORDER BY created_at DESC, id DESC"
            ))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list ssh keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_ssh_key!(&r));
            }
            Ok(mapped)
        }
    }
}

/// Keys for many users in one `IN (...)` round trip, newest first
/// (commit signature keyring batching).
pub async fn list_for_users(pool: &DbPool, user_ids: &[String]) -> Result<Vec<SshKeyRow>, String> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(&format!(
                "{SSH_SELECT_PG} WHERE user_id = ANY($1) ORDER BY created_at DESC, id DESC"
            ))
            .bind(user_ids)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list ssh keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_ssh_key!(&r));
            }
            Ok(mapped)
        }
        DbPool::MySql(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::MySql, 1, user_ids.len());
            let q_str = format!(
                "{SSH_SELECT_MYSQL} WHERE user_id IN ({in_list}) ORDER BY created_at DESC, id DESC"
            );
            let q = user_ids.iter().fold(sqlx::query(&q_str), |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list ssh keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_ssh_key!(&r));
            }
            Ok(mapped)
        }
        DbPool::Sqlite(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::Sqlite, 1, user_ids.len());
            let q_str = format!(
                "{SSH_SELECT_SQLITE} WHERE user_id IN ({in_list}) ORDER BY created_at DESC, id DESC"
            );
            let q = user_ids.iter().fold(sqlx::query(&q_str), |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list ssh keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_ssh_key!(&r));
            }
            Ok(mapped)
        }
    }
}

/// Hard-delete a key by id. Idempotent when missing.
pub async fn revoke(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM ssh_public_keys WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("revoke ssh key failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM ssh_public_keys WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("revoke ssh key failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM ssh_public_keys WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("revoke ssh key failed: {e}"))?;
        }
    }
    Ok(())
}

/// Update last-used timestamp and optional client IP after successful SSH auth.
pub async fn touch_last_used(
    pool: &DbPool,
    id: &str,
    last_used_at: &str,
    last_used_ip: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE ssh_public_keys
SET last_used_at = $2::timestamptz, last_used_ip = $3
WHERE id = $1",
            )
            .bind(id)
            .bind(last_used_at)
            .bind(last_used_ip)
            .execute(p)
            .await
            .map_err(|e| format!("touch ssh key failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE ssh_public_keys SET last_used_at = ?, last_used_ip = ?
WHERE id = ?",
            )
            .bind(last_used_at)
            .bind(last_used_ip)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("touch ssh key failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE ssh_public_keys SET last_used_at = ?2, last_used_ip = ?3
WHERE id = ?1",
            )
            .bind(id)
            .bind(last_used_at)
            .bind(last_used_ip)
            .execute(p)
            .await
            .map_err(|e| format!("touch ssh key failed: {e}"))?;
        }
    }
    Ok(())
}
