//! Session CRUD via `DbPool` match — opaque token hashes only (D-11, D-13).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub expires_at: String,
    pub remember_me: bool,
    pub created_at: String,
    pub last_seen_at: String,
    /// Last-known client IP (create + refreshed on resolve).
    pub ip_address: Option<String>,
    /// Last-known client User-Agent (create + refreshed on resolve).
    pub user_agent: Option<String>,
    /// `users.id` via LEFT JOIN — `None` when the owning user row is gone.
    pub joined_user_id: Option<String>,
    /// `users.banned_at` via LEFT JOIN — `Some` marks a soft-banned owner.
    pub user_banned_at: Option<String>,
}

macro_rules! map_session {
    ($row:expr) => {{
        let row = $row;
        let remember_me = row
            .try_get::<bool, _>("remember_me")
            .or_else(|_| {
                row.try_get::<i64, _>("remember_me")
                    .map(|v| v != 0)
                    .or_else(|_| row.try_get::<i8, _>("remember_me").map(|v| v != 0))
            })
            .map_err(|e| format!("session row: {e}"))?;
        SessionRow {
            id: row.try_get("id").map_err(|e| format!("session row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("session row: {e}"))?,
            token_hash: row
                .try_get("token_hash")
                .map_err(|e| format!("session row: {e}"))?,
            expires_at: row
                .try_get("expires_at")
                .map_err(|e| format!("session row: {e}"))?,
            remember_me,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("session row: {e}"))?,
            last_seen_at: row
                .try_get("last_seen_at")
                .map_err(|e| format!("session row: {e}"))?,
            ip_address: row
                .try_get::<Option<String>, _>("ip_address")
                .unwrap_or(None),
            user_agent: row
                .try_get::<Option<String>, _>("user_agent")
                .unwrap_or(None),
            joined_user_id: row
                .try_get("joined_user_id")
                .map_err(|e| format!("session row: {e}"))?,
            user_banned_at: row
                .try_get("user_banned_at")
                .map_err(|e| format!("session row: {e}"))?,
        }
    }};
}

const SESSION_SELECT_PG: &str = "SELECT s.id, s.user_id, s.token_hash, s.remember_me,
       s.ip_address, s.user_agent,
       to_char(s.expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at,
       to_char(s.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(s.last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_seen_at,
       u.id AS joined_user_id,
       to_char(u.banned_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS user_banned_at
FROM sessions s
LEFT JOIN users u ON u.id = s.user_id";

const SESSION_SELECT_MYSQL: &str = "SELECT s.id, s.user_id, s.token_hash, s.remember_me,
       s.ip_address, s.user_agent,
       DATE_FORMAT(s.expires_at, '%Y-%m-%dT%H:%i:%sZ') AS expires_at,
       DATE_FORMAT(s.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(s.last_seen_at, '%Y-%m-%dT%H:%i:%sZ') AS last_seen_at,
       u.id AS joined_user_id,
       DATE_FORMAT(u.banned_at, '%Y-%m-%dT%H:%i:%sZ') AS user_banned_at
FROM sessions s
LEFT JOIN users u ON u.id = s.user_id";

const SESSION_SELECT_SQLITE: &str = "SELECT s.id, s.user_id, s.token_hash, s.remember_me,
       s.ip_address, s.user_agent,
       strftime('%Y-%m-%dT%H:%M:%SZ', s.expires_at) AS expires_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', s.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', s.last_seen_at) AS last_seen_at,
       u.id AS joined_user_id,
       strftime('%Y-%m-%dT%H:%M:%SZ', u.banned_at) AS user_banned_at
FROM sessions s
LEFT JOIN users u ON u.id = s.user_id";

/// Create a session row. `id` is the session PK; `token_hash` is SHA-256 hex of the cookie value.
/// `ip_address`/`user_agent` capture the client that minted the session.
pub async fn create(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    token_hash: &str,
    expires_at: &str,
    remember_me: bool,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO sessions (id, user_id, token_hash, expires_at, remember_me, ip_address, user_agent)
VALUES ($1, $2, $3, $4::timestamptz, $5, $6, $7)",
            )
            .bind(id)
            .bind(user_id)
            .bind(token_hash)
            .bind(expires_at)
            .bind(remember_me)
            .bind(ip_address)
            .bind(user_agent)
            .execute(p)
            .await
            .map_err(|e| format!("create session failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO sessions (id, user_id, token_hash, expires_at, remember_me, ip_address, user_agent)
VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(user_id)
            .bind(token_hash)
            .bind(expires_at)
            .bind(remember_me)
            .bind(ip_address)
            .bind(user_agent)
            .execute(p)
            .await
            .map_err(|e| format!("create session failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO sessions (id, user_id, token_hash, expires_at, remember_me, ip_address, user_agent)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(user_id)
            .bind(token_hash)
            .bind(expires_at)
            .bind(if remember_me { 1 } else { 0 })
            .bind(ip_address)
            .bind(user_agent)
            .execute(p)
            .await
            .map_err(|e| format!("create session failed: {e}"))?;
        }
    }
    Ok(())
}

pub async fn find_by_token_hash(
    pool: &DbPool,
    token_hash: &str,
) -> Result<Option<SessionRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SESSION_SELECT_PG} WHERE s.token_hash = $1"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find session failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_session!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SESSION_SELECT_MYSQL} WHERE s.token_hash = ?"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find session failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_session!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SESSION_SELECT_SQLITE} WHERE s.token_hash = ?1"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find session failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_session!(&r)),
                None => None,
            })
        }
    }
}

/// Refresh expiry + last_seen; when the caller supplies client metadata the
/// last-known `ip_address`/`user_agent` are refreshed too.
pub async fn touch(
    pool: &DbPool,
    id: &str,
    expires_at: &str,
    last_seen_at: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE sessions SET expires_at = $2::timestamptz, last_seen_at = $3::timestamptz,
    ip_address = COALESCE($4, ip_address), user_agent = COALESCE($5, user_agent)
WHERE id = $1",
            )
            .bind(id)
            .bind(expires_at)
            .bind(last_seen_at)
            .bind(ip_address)
            .bind(user_agent)
            .execute(p)
            .await
            .map_err(|e| format!("touch session failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE sessions SET expires_at = ?, last_seen_at = ?,
    ip_address = COALESCE(?, ip_address), user_agent = COALESCE(?, user_agent)
WHERE id = ?",
            )
            .bind(expires_at)
            .bind(last_seen_at)
            .bind(ip_address)
            .bind(user_agent)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("touch session failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE sessions SET expires_at = ?2, last_seen_at = ?3,
    ip_address = COALESCE(?4, ip_address), user_agent = COALESCE(?5, user_agent)
WHERE id = ?1",
            )
            .bind(id)
            .bind(expires_at)
            .bind(last_seen_at)
            .bind(ip_address)
            .bind(user_agent)
            .execute(p)
            .await
            .map_err(|e| format!("touch session failed: {e}"))?;
        }
    }
    Ok(())
}

/// All sessions for a user, most recently seen first (admin view — includes
/// token_hash; callers must never expose it).
pub async fn list_for_user(pool: &DbPool, user_id: &str) -> Result<Vec<SessionRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SESSION_SELECT_PG} WHERE s.user_id = $1 ORDER BY s.last_seen_at DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list sessions for user failed: {e}"))?;
            rows.iter().map(|r| Ok(map_session!(r))).collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SESSION_SELECT_MYSQL} WHERE s.user_id = ? ORDER BY s.last_seen_at DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list sessions for user failed: {e}"))?;
            rows.iter().map(|r| Ok(map_session!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{SESSION_SELECT_SQLITE} WHERE s.user_id = ?1 ORDER BY s.last_seen_at DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list sessions for user failed: {e}"))?;
            rows.iter().map(|r| Ok(map_session!(r))).collect()
        }
    }
}

pub async fn delete(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM sessions WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete session failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM sessions WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete session failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM sessions WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete session failed: {e}"))?;
        }
    }
    Ok(())
}

pub async fn delete_all_for_user(pool: &DbPool, user_id: &str) -> Result<u64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let result = sqlx::query("DELETE FROM sessions WHERE user_id = $1")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete sessions for user failed: {e}"))?;
            Ok(result.rows_affected())
        }
        DbPool::MySql(p) => {
            let result = sqlx::query("DELETE FROM sessions WHERE user_id = ?")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete sessions for user failed: {e}"))?;
            Ok(result.rows_affected())
        }
        DbPool::Sqlite(p) => {
            let result = sqlx::query("DELETE FROM sessions WHERE user_id = ?1")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete sessions for user failed: {e}"))?;
            Ok(result.rows_affected())
        }
    }
}
