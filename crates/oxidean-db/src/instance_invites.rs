//! Instance invite helpers via `DbPool` match — token_hash at rest only.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct InstanceInviteRow {
    pub id: String,
    /// `None` marks a shareable invite link (no bound email).
    pub email: Option<String>,
    pub token_hash: String,
    /// `None` = never expires (link invites may opt out of expiry).
    pub expires_at: Option<String>,
    pub invited_by: String,
    pub created_at: String,
    /// `accepted_at` doubles as the "fully consumed" marker for seat-limited links.
    pub accepted_at: Option<String>,
    pub revoked_at: Option<String>,
    /// `None` = unlimited seats (links); email invites are created with `Some(1)`.
    pub max_uses: Option<i64>,
    pub use_count: i64,
}

macro_rules! map_opt_str {
    ($row:expr, $name:expr) => {{
        $row.try_get::<Option<String>, _>($name)
            .map_err(|e| format!("instance invite row: {e}"))?
    }};
}

macro_rules! map_invite {
    ($row:expr) => {{
        let row = $row;
        InstanceInviteRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("instance invite row: {e}"))?,
            email: map_opt_str!(row, "email"),
            token_hash: row
                .try_get("token_hash")
                .map_err(|e| format!("instance invite row: {e}"))?,
            expires_at: map_opt_str!(row, "expires_at"),
            invited_by: row
                .try_get("invited_by")
                .map_err(|e| format!("instance invite row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("instance invite row: {e}"))?,
            accepted_at: map_opt_str!(row, "accepted_at"),
            revoked_at: map_opt_str!(row, "revoked_at"),
            max_uses: row
                .try_get::<Option<i64>, _>("max_uses")
                .or_else(|_| {
                    row.try_get::<Option<i32>, _>("max_uses")
                        .map(|o| o.map(i64::from))
                })
                .map_err(|e| format!("instance invite row: {e}"))?,
            use_count: row
                .try_get::<i64, _>("use_count")
                .or_else(|_| row.try_get::<i32, _>("use_count").map(i64::from))
                .map_err(|e| format!("instance invite row: {e}"))?,
        }
    }};
}

const INVITE_SELECT_PG: &str = "SELECT id, email, token_hash, invited_by, max_uses, use_count,
       CASE WHEN expires_at IS NULL THEN NULL ELSE
         to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS expires_at,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       CASE WHEN accepted_at IS NULL THEN NULL ELSE
         to_char(accepted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS accepted_at,
       CASE WHEN revoked_at IS NULL THEN NULL ELSE
         to_char(revoked_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS revoked_at
FROM instance_invites";

const INVITE_SELECT_MYSQL: &str = "SELECT id, email, token_hash, invited_by, max_uses, use_count,
       CASE WHEN expires_at IS NULL THEN NULL ELSE
         DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') END AS expires_at,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       CASE WHEN accepted_at IS NULL THEN NULL ELSE
         DATE_FORMAT(accepted_at, '%Y-%m-%dT%H:%i:%sZ') END AS accepted_at,
       CASE WHEN revoked_at IS NULL THEN NULL ELSE
         DATE_FORMAT(revoked_at, '%Y-%m-%dT%H:%i:%sZ') END AS revoked_at
FROM instance_invites";

const INVITE_SELECT_SQLITE: &str = "SELECT id, email, token_hash, invited_by, max_uses, use_count,
       CASE WHEN expires_at IS NULL THEN NULL ELSE
         strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) END AS expires_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
       CASE WHEN accepted_at IS NULL THEN NULL ELSE
         strftime('%Y-%m-%dT%H:%M:%SZ', accepted_at) END AS accepted_at,
       CASE WHEN revoked_at IS NULL THEN NULL ELSE
         strftime('%Y-%m-%dT%H:%M:%SZ', revoked_at) END AS revoked_at
FROM instance_invites";

/// Insert a pending instance invite (hash-at-rest only).
/// `email` None = shareable link; `expires_at` None = never; `max_uses` None = unlimited seats.
pub async fn insert_invite(
    pool: &DbPool,
    id: &str,
    email: Option<&str>,
    token_hash: &str,
    expires_at: Option<&str>,
    invited_by: &str,
    max_uses: Option<i64>,
) -> Result<InstanceInviteRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO instance_invites
(id, email, token_hash, expires_at, invited_by, max_uses)
VALUES ($1, $2, $3, $4::timestamptz, $5, $6)",
            )
            .bind(id)
            .bind(email)
            .bind(token_hash)
            .bind(expires_at)
            .bind(invited_by)
            .bind(max_uses)
            .execute(p)
            .await
            .map_err(|e| format!("insert instance invite failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO instance_invites
(id, email, token_hash, expires_at, invited_by, max_uses)
VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(email)
            .bind(token_hash)
            .bind(expires_at)
            .bind(invited_by)
            .bind(max_uses)
            .execute(p)
            .await
            .map_err(|e| format!("insert instance invite failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO instance_invites
(id, email, token_hash, expires_at, invited_by, max_uses)
VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(id)
            .bind(email)
            .bind(token_hash)
            .bind(expires_at)
            .bind(invited_by)
            .bind(max_uses)
            .execute(p)
            .await
            .map_err(|e| format!("insert instance invite failed: {e}"))?;
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert instance invite failed: row missing after insert".into())
}

pub async fn find_by_id(pool: &DbPool, id: &str) -> Result<Option<InstanceInviteRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let q = format!("{INVITE_SELECT_PG} WHERE id = $1");
            let row = sqlx::query(&q).bind(id).fetch_optional(p).await;
            match row {
                Ok(Some(r)) => Ok(Some(map_invite!(r))),
                Ok(None) => Ok(None),
                Err(e) => Err(format!("find instance invite failed: {e}")),
            }
        }
        DbPool::MySql(p) => {
            let q = format!("{INVITE_SELECT_MYSQL} WHERE id = ?");
            let row = sqlx::query(&q).bind(id).fetch_optional(p).await;
            match row {
                Ok(Some(r)) => Ok(Some(map_invite!(r))),
                Ok(None) => Ok(None),
                Err(e) => Err(format!("find instance invite failed: {e}")),
            }
        }
        DbPool::Sqlite(p) => {
            let q = format!("{INVITE_SELECT_SQLITE} WHERE id = ?1");
            let row = sqlx::query(&q).bind(id).fetch_optional(p).await;
            match row {
                Ok(Some(r)) => Ok(Some(map_invite!(r))),
                Ok(None) => Ok(None),
                Err(e) => Err(format!("find instance invite failed: {e}")),
            }
        }
    }
}

pub async fn find_by_token_hash(
    pool: &DbPool,
    token_hash: &str,
) -> Result<Option<InstanceInviteRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let q = format!("{INVITE_SELECT_PG} WHERE token_hash = $1");
            let row = sqlx::query(&q).bind(token_hash).fetch_optional(p).await;
            match row {
                Ok(Some(r)) => Ok(Some(map_invite!(r))),
                Ok(None) => Ok(None),
                Err(e) => Err(format!("find instance invite by token failed: {e}")),
            }
        }
        DbPool::MySql(p) => {
            let q = format!("{INVITE_SELECT_MYSQL} WHERE token_hash = ?");
            let row = sqlx::query(&q).bind(token_hash).fetch_optional(p).await;
            match row {
                Ok(Some(r)) => Ok(Some(map_invite!(r))),
                Ok(None) => Ok(None),
                Err(e) => Err(format!("find instance invite by token failed: {e}")),
            }
        }
        DbPool::Sqlite(p) => {
            let q = format!("{INVITE_SELECT_SQLITE} WHERE token_hash = ?1");
            let row = sqlx::query(&q).bind(token_hash).fetch_optional(p).await;
            match row {
                Ok(Some(r)) => Ok(Some(map_invite!(r))),
                Ok(None) => Ok(None),
                Err(e) => Err(format!("find instance invite by token failed: {e}")),
            }
        }
    }
}

/// Pending (not accepted, not revoked) invite for email, if any.
pub async fn find_pending_by_email(
    pool: &DbPool,
    email: &str,
) -> Result<Option<InstanceInviteRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let q = format!(
                "{INVITE_SELECT_PG}
WHERE lower(email) = lower($1)
  AND accepted_at IS NULL AND revoked_at IS NULL
ORDER BY created_at DESC
LIMIT 1"
            );
            let row = sqlx::query(&q).bind(email).fetch_optional(p).await;
            match row {
                Ok(Some(r)) => Ok(Some(map_invite!(r))),
                Ok(None) => Ok(None),
                Err(e) => Err(format!("find pending instance invite failed: {e}")),
            }
        }
        DbPool::MySql(p) => {
            let q = format!(
                "{INVITE_SELECT_MYSQL}
WHERE lower(email) = lower(?)
  AND accepted_at IS NULL AND revoked_at IS NULL
ORDER BY created_at DESC
LIMIT 1"
            );
            let row = sqlx::query(&q).bind(email).fetch_optional(p).await;
            match row {
                Ok(Some(r)) => Ok(Some(map_invite!(r))),
                Ok(None) => Ok(None),
                Err(e) => Err(format!("find pending instance invite failed: {e}")),
            }
        }
        DbPool::Sqlite(p) => {
            let q = format!(
                "{INVITE_SELECT_SQLITE}
WHERE lower(email) = lower(?1)
  AND accepted_at IS NULL AND revoked_at IS NULL
ORDER BY created_at DESC
LIMIT 1"
            );
            let row = sqlx::query(&q).bind(email).fetch_optional(p).await;
            match row {
                Ok(Some(r)) => Ok(Some(map_invite!(r))),
                Ok(None) => Ok(None),
                Err(e) => Err(format!("find pending instance invite failed: {e}")),
            }
        }
    }
}

/// Pending instance invites (list UI — no token).
pub async fn list_pending(pool: &DbPool) -> Result<Vec<InstanceInviteRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let q = format!(
                "{INVITE_SELECT_PG}
WHERE accepted_at IS NULL AND revoked_at IS NULL
ORDER BY created_at DESC"
            );
            let rows = sqlx::query(&q)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list instance invites failed: {e}"))?;
            rows.into_iter().map(|r| Ok(map_invite!(r))).collect()
        }
        DbPool::MySql(p) => {
            let q = format!(
                "{INVITE_SELECT_MYSQL}
WHERE accepted_at IS NULL AND revoked_at IS NULL
ORDER BY created_at DESC"
            );
            let rows = sqlx::query(&q)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list instance invites failed: {e}"))?;
            rows.into_iter().map(|r| Ok(map_invite!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let q = format!(
                "{INVITE_SELECT_SQLITE}
WHERE accepted_at IS NULL AND revoked_at IS NULL
ORDER BY created_at DESC"
            );
            let rows = sqlx::query(&q)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list instance invites failed: {e}"))?;
            rows.into_iter().map(|r| Ok(map_invite!(r))).collect()
        }
    }
}

/// Soft-revoke a pending invite.
pub async fn revoke(pool: &DbPool, id: &str, revoked_at: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            let n = sqlx::query(
                "UPDATE instance_invites
SET revoked_at = $2::timestamptz
WHERE id = $1 AND accepted_at IS NULL AND revoked_at IS NULL",
            )
            .bind(id)
            .bind(revoked_at)
            .execute(p)
            .await
            .map_err(|e| format!("revoke instance invite failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("instance invite not found".into());
            }
        }
        DbPool::MySql(p) => {
            let n = sqlx::query(
                "UPDATE instance_invites
SET revoked_at = ?
WHERE id = ? AND accepted_at IS NULL AND revoked_at IS NULL",
            )
            .bind(revoked_at)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("revoke instance invite failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("instance invite not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let n = sqlx::query(
                "UPDATE instance_invites
SET revoked_at = ?1
WHERE id = ?2 AND accepted_at IS NULL AND revoked_at IS NULL",
            )
            .bind(revoked_at)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("revoke instance invite failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("instance invite not found".into());
            }
        }
    }
    Ok(())
}

/// Consume one seat atomically: increments `use_count` and stamps `accepted_at`
/// when the last seat is taken. Fails ("not found") when the invite is revoked,
/// fully consumed, or missing — this is the over-grant guard.
pub async fn consume(pool: &DbPool, id: &str, accepted_at: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            let n = sqlx::query(
                "UPDATE instance_invites
SET use_count = use_count + 1,
    accepted_at = CASE
      WHEN max_uses IS NOT NULL AND use_count + 1 >= max_uses THEN $2::timestamptz
      ELSE accepted_at END
WHERE id = $1 AND accepted_at IS NULL AND revoked_at IS NULL
  AND (max_uses IS NULL OR use_count < max_uses)",
            )
            .bind(id)
            .bind(accepted_at)
            .execute(p)
            .await
            .map_err(|e| format!("consume instance invite failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("instance invite not found".into());
            }
        }
        DbPool::MySql(p) => {
            let n = sqlx::query(
                "UPDATE instance_invites
SET use_count = use_count + 1,
    accepted_at = CASE
      WHEN max_uses IS NOT NULL AND use_count + 1 >= max_uses THEN ?
      ELSE accepted_at END
WHERE id = ? AND accepted_at IS NULL AND revoked_at IS NULL
  AND (max_uses IS NULL OR use_count < max_uses)",
            )
            .bind(accepted_at)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("consume instance invite failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("instance invite not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let n = sqlx::query(
                "UPDATE instance_invites
SET use_count = use_count + 1,
    accepted_at = CASE
      WHEN max_uses IS NOT NULL AND use_count + 1 >= max_uses THEN ?1
      ELSE accepted_at END
WHERE id = ?2 AND accepted_at IS NULL AND revoked_at IS NULL
  AND (max_uses IS NULL OR use_count < max_uses)",
            )
            .bind(accepted_at)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("consume instance invite failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("instance invite not found".into());
            }
        }
    }
    Ok(())
}

/// Count invites created by a user since `since` (rate limit).
pub async fn count_created_by_since(
    pool: &DbPool,
    invited_by: &str,
    since: &str,
) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*)::bigint FROM instance_invites
WHERE invited_by = $1 AND created_at >= $2::timestamptz",
            )
            .bind(invited_by)
            .bind(since)
            .fetch_one(p)
            .await
            .map_err(|e| format!("count instance invites failed: {e}"))?;
            Ok(row)
        }
        DbPool::MySql(p) => {
            let row: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM instance_invites
WHERE invited_by = ? AND created_at >= ?",
            )
            .bind(invited_by)
            .bind(since)
            .fetch_one(p)
            .await
            .map_err(|e| format!("count instance invites failed: {e}"))?;
            Ok(row)
        }
        DbPool::Sqlite(p) => {
            let row: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM instance_invites
WHERE invited_by = ?1 AND created_at >= ?2",
            )
            .bind(invited_by)
            .bind(since)
            .fetch_one(p)
            .await
            .map_err(|e| format!("count instance invites failed: {e}"))?;
            Ok(row)
        }
    }
}

/// Test helper: backdate expires_at.
pub async fn set_expires_at(pool: &DbPool, id: &str, expires_at: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE instance_invites SET expires_at = $2::timestamptz WHERE id = $1",
            )
            .bind(id)
            .bind(expires_at)
            .execute(p)
            .await
            .map_err(|e| format!("set instance invite expires_at failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE instance_invites SET expires_at = ? WHERE id = ?")
                .bind(expires_at)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set instance invite expires_at failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE instance_invites SET expires_at = ?1 WHERE id = ?2")
                .bind(expires_at)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set instance invite expires_at failed: {e}"))?;
        }
    }
    Ok(())
}
