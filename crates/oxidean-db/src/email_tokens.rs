//! Email verify/reset token CRUD via `DbPool` match — hash-at-rest only (T-05-03).
//! `target_email` scopes verify tokens per address (`''` for primary/legacy + reset).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct EmailTokenRow {
    pub id: String,
    pub user_id: String,
    pub purpose: String,
    pub target_email: String,
    pub token_hash: String,
    pub otp_hash: String,
    pub expires_at: String,
    pub attempt_count: i32,
    pub issue_count: i32,
    pub created_at: String,
}

macro_rules! map_i32 {
    ($row:expr, $name:expr) => {{
        $row.try_get::<i32, _>($name)
            .or_else(|_| {
                $row.try_get::<i64, _>($name)
                    .map(|v| v as i32)
                    .or_else(|_| $row.try_get::<i8, _>($name).map(|v| i32::from(v)))
            })
            .map_err(|e| format!("email token row: {e}"))?
    }};
}

macro_rules! map_email_token {
    ($row:expr) => {{
        let row = $row;
        EmailTokenRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("email token row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("email token row: {e}"))?,
            purpose: row
                .try_get("purpose")
                .map_err(|e| format!("email token row: {e}"))?,
            target_email: row.try_get::<String, _>("target_email").unwrap_or_default(),
            token_hash: row
                .try_get("token_hash")
                .map_err(|e| format!("email token row: {e}"))?,
            otp_hash: row
                .try_get("otp_hash")
                .map_err(|e| format!("email token row: {e}"))?,
            expires_at: row
                .try_get("expires_at")
                .map_err(|e| format!("email token row: {e}"))?,
            attempt_count: map_i32!(row, "attempt_count"),
            issue_count: map_i32!(row, "issue_count"),
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("email token row: {e}"))?,
        }
    }};
}

const TOKEN_SELECT_PG: &str =
    "SELECT id, user_id, purpose, target_email, token_hash, otp_hash, attempt_count, issue_count,
       to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM auth_email_tokens";

const TOKEN_SELECT_MYSQL: &str =
    "SELECT id, user_id, purpose, target_email, token_hash, otp_hash, attempt_count, issue_count,
       DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') AS expires_at,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM auth_email_tokens";

const TOKEN_SELECT_SQLITE: &str =
    "SELECT id, user_id, purpose, target_email, token_hash, otp_hash, attempt_count, issue_count,
       strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) AS expires_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM auth_email_tokens";

/// Insert or replace the row for `(user_id, purpose, target_email)`.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_by_user_purpose(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    purpose: &str,
    target_email: &str,
    token_hash: &str,
    otp_hash: &str,
    expires_at: &str,
    issue_count: i32,
) -> Result<EmailTokenRow, String> {
    let target = target_email.trim().to_ascii_lowercase();
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO auth_email_tokens
(id, user_id, purpose, target_email, token_hash, otp_hash, expires_at, attempt_count, issue_count)
VALUES ($1, $2, $3, $4, $5, $6, $7::timestamptz, 0, $8)
ON CONFLICT (user_id, purpose, target_email) DO UPDATE
SET id = EXCLUDED.id,
    token_hash = EXCLUDED.token_hash,
    otp_hash = EXCLUDED.otp_hash,
    expires_at = EXCLUDED.expires_at,
    attempt_count = 0,
    issue_count = EXCLUDED.issue_count,
    created_at = now()",
            )
            .bind(id)
            .bind(user_id)
            .bind(purpose)
            .bind(&target)
            .bind(token_hash)
            .bind(otp_hash)
            .bind(expires_at)
            .bind(issue_count)
            .execute(p)
            .await
            .map_err(|e| format!("upsert email token failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO auth_email_tokens
(id, user_id, purpose, target_email, token_hash, otp_hash, expires_at, attempt_count, issue_count)
VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?) AS new
ON DUPLICATE KEY UPDATE
  id = new.id,
  token_hash = new.token_hash,
  otp_hash = new.otp_hash,
  expires_at = new.expires_at,
  attempt_count = 0,
  issue_count = new.issue_count,
  created_at = CURRENT_TIMESTAMP",
            )
            .bind(id)
            .bind(user_id)
            .bind(purpose)
            .bind(&target)
            .bind(token_hash)
            .bind(otp_hash)
            .bind(expires_at)
            .bind(issue_count)
            .execute(p)
            .await
            .map_err(|e| format!("upsert email token failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO auth_email_tokens
(id, user_id, purpose, target_email, token_hash, otp_hash, expires_at, attempt_count, issue_count)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)
ON CONFLICT (user_id, purpose, target_email) DO UPDATE SET
  id = excluded.id,
  token_hash = excluded.token_hash,
  otp_hash = excluded.otp_hash,
  expires_at = excluded.expires_at,
  attempt_count = 0,
  issue_count = excluded.issue_count,
  created_at = strftime('%Y-%m-%d %H:%M:%S','now')",
            )
            .bind(id)
            .bind(user_id)
            .bind(purpose)
            .bind(&target)
            .bind(token_hash)
            .bind(otp_hash)
            .bind(expires_at)
            .bind(issue_count)
            .execute(p)
            .await
            .map_err(|e| format!("upsert email token failed: {e}"))?;
        }
    }
    find_by_user_purpose(pool, user_id, purpose, &target)
        .await?
        .ok_or_else(|| "upsert email token failed: row missing after upsert".into())
}

pub async fn find_by_user_purpose(
    pool: &DbPool,
    user_id: &str,
    purpose: &str,
    target_email: &str,
) -> Result<Option<EmailTokenRow>, String> {
    let target = target_email.trim().to_ascii_lowercase();
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_PG} WHERE user_id = $1 AND purpose = $2 AND target_email = $3"
            )))
            .bind(user_id)
            .bind(purpose)
            .bind(&target)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find email token by user/purpose failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_email_token!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_MYSQL} WHERE user_id = ? AND purpose = ? AND target_email = ?"
            )))
            .bind(user_id)
            .bind(purpose)
            .bind(&target)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find email token by user/purpose failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_email_token!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_SQLITE} WHERE user_id = ?1 AND purpose = ?2 AND target_email = ?3"
            )))
            .bind(user_id)
            .bind(purpose)
            .bind(&target)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find email token by user/purpose failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_email_token!(&r)),
                None => None,
            })
        }
    }
}

pub async fn find_by_token_hash(
    pool: &DbPool,
    token_hash: &str,
) -> Result<Option<EmailTokenRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_PG} WHERE token_hash = $1"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find email token by token_hash failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_email_token!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_MYSQL} WHERE token_hash = ?"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find email token by token_hash failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_email_token!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_SQLITE} WHERE token_hash = ?1"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find email token by token_hash failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_email_token!(&r)),
                None => None,
            })
        }
    }
}

pub async fn find_by_otp_hash(
    pool: &DbPool,
    otp_hash: &str,
) -> Result<Option<EmailTokenRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_PG} WHERE otp_hash = $1"
            )))
            .bind(otp_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find email token by otp_hash failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_email_token!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_MYSQL} WHERE otp_hash = ?"
            )))
            .bind(otp_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find email token by otp_hash failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_email_token!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TOKEN_SELECT_SQLITE} WHERE otp_hash = ?1"
            )))
            .bind(otp_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find email token by otp_hash failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_email_token!(&r)),
                None => None,
            })
        }
    }
}

pub async fn increment_attempts(pool: &DbPool, id: &str) -> Result<i32, String> {
    match pool {
        DbPool::Postgres(p) => {
            let count = sqlx::query_scalar::<_, i32>(
                "UPDATE auth_email_tokens SET attempt_count = attempt_count + 1
WHERE id = $1
RETURNING attempt_count",
            )
            .bind(id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("increment email token attempts failed: {e}"))?;
            Ok(count)
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE auth_email_tokens SET attempt_count = attempt_count + 1 WHERE id = ?",
            )
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("increment email token attempts failed: {e}"))?;
            let count = sqlx::query_scalar::<_, i32>(
                "SELECT attempt_count FROM auth_email_tokens WHERE id = ?",
            )
            .bind(id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("increment email token attempts failed: {e}"))?;
            Ok(count)
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE auth_email_tokens SET attempt_count = attempt_count + 1 WHERE id = ?1",
            )
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("increment email token attempts failed: {e}"))?;
            let count = sqlx::query_scalar::<_, i32>(
                "SELECT attempt_count FROM auth_email_tokens WHERE id = ?1",
            )
            .bind(id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("increment email token attempts failed: {e}"))?;
            Ok(count)
        }
    }
}

/// Test/helper: set `created_at` for rate-limit window simulation.
///
/// Updates **all** tokens for `(user_id, purpose)` regardless of `target_email`,
/// so backdate helpers keep working after primary verify tokens use the address.
pub async fn set_created_at(
    pool: &DbPool,
    user_id: &str,
    purpose: &str,
    created_at: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE auth_email_tokens SET created_at = $3::timestamptz
WHERE user_id = $1 AND purpose = $2",
            )
            .bind(user_id)
            .bind(purpose)
            .bind(created_at)
            .execute(p)
            .await
            .map_err(|e| format!("set email token created_at failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE auth_email_tokens SET created_at = ?
WHERE user_id = ? AND purpose = ?",
            )
            .bind(created_at)
            .bind(user_id)
            .bind(purpose)
            .execute(p)
            .await
            .map_err(|e| format!("set email token created_at failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE auth_email_tokens SET created_at = ?1
WHERE user_id = ?2 AND purpose = ?3",
            )
            .bind(created_at)
            .bind(user_id)
            .bind(purpose)
            .execute(p)
            .await
            .map_err(|e| format!("set email token created_at failed: {e}"))?;
        }
    }
    Ok(())
}

pub async fn delete(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM auth_email_tokens WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete email token failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM auth_email_tokens WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete email token failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM auth_email_tokens WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete email token failed: {e}"))?;
        }
    }
    Ok(())
}
