//! Durable audit events (auth / admin / invite actions) via `DbPool` match.
//!
//! `actor_id` uses `ON DELETE SET NULL`; `actor_username` is a snapshot so the
//! log survives user deletion. `detail` is a small free-form JSON string.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct AuditEventRow {
    pub id: String,
    /// `None` when the actor row is gone — use `actor_username` for display.
    pub actor_id: Option<String>,
    pub actor_username: String,
    pub event_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub detail: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: String,
}

macro_rules! map_opt_str {
    ($row:expr, $name:expr) => {{
        $row.try_get::<Option<String>, _>($name)
            .map_err(|e| format!("audit event row: {e}"))?
    }};
}

macro_rules! map_event {
    ($row:expr) => {{
        let row = $row;
        AuditEventRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("audit event row: {e}"))?,
            actor_id: map_opt_str!(row, "actor_id"),
            actor_username: row
                .try_get("actor_username")
                .map_err(|e| format!("audit event row: {e}"))?,
            event_type: row
                .try_get("event_type")
                .map_err(|e| format!("audit event row: {e}"))?,
            target_type: map_opt_str!(row, "target_type"),
            target_id: map_opt_str!(row, "target_id"),
            detail: map_opt_str!(row, "detail"),
            ip_address: map_opt_str!(row, "ip_address"),
            user_agent: map_opt_str!(row, "user_agent"),
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("audit event row: {e}"))?,
        }
    }};
}

const SELECT_PG: &str = "SELECT id, actor_id, actor_username, event_type, target_type, target_id,
       detail, ip_address, user_agent,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM audit_events";

const SELECT_MYSQL: &str =
    "SELECT id, actor_id, actor_username, event_type, target_type, target_id,
       detail, ip_address, user_agent,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM audit_events";

const SELECT_SQLITE: &str =
    "SELECT id, actor_id, actor_username, event_type, target_type, target_id,
       detail, ip_address, user_agent,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM audit_events";

pub struct InsertAuditEvent<'a> {
    pub id: &'a str,
    pub actor_id: Option<&'a str>,
    pub actor_username: &'a str,
    pub event_type: &'a str,
    pub target_type: Option<&'a str>,
    pub target_id: Option<&'a str>,
    /// Small JSON object (e.g. `{"role":"admin"}`) — never secrets or tokens.
    pub detail: Option<&'a str>,
    pub ip_address: Option<&'a str>,
    pub user_agent: Option<&'a str>,
}

pub async fn insert(pool: &DbPool, event: InsertAuditEvent<'_>) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO audit_events
 (id, actor_id, actor_username, event_type, target_type, target_id,
  detail, ip_address, user_agent)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(event.id)
            .bind(event.actor_id)
            .bind(event.actor_username)
            .bind(event.event_type)
            .bind(event.target_type)
            .bind(event.target_id)
            .bind(event.detail)
            .bind(event.ip_address)
            .bind(event.user_agent)
            .execute(p)
            .await
            .map_err(|e| format!("insert audit event failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO audit_events
 (id, actor_id, actor_username, event_type, target_type, target_id,
  detail, ip_address, user_agent)
 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(event.id)
            .bind(event.actor_id)
            .bind(event.actor_username)
            .bind(event.event_type)
            .bind(event.target_type)
            .bind(event.target_id)
            .bind(event.detail)
            .bind(event.ip_address)
            .bind(event.user_agent)
            .execute(p)
            .await
            .map_err(|e| format!("insert audit event failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO audit_events
 (id, actor_id, actor_username, event_type, target_type, target_id,
  detail, ip_address, user_agent)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(event.id)
            .bind(event.actor_id)
            .bind(event.actor_username)
            .bind(event.event_type)
            .bind(event.target_type)
            .bind(event.target_id)
            .bind(event.detail)
            .bind(event.ip_address)
            .bind(event.user_agent)
            .execute(p)
            .await
            .map_err(|e| format!("insert audit event failed: {e}"))?;
        }
    }
    Ok(())
}

/// Events for one actor, newest first. `event_type` filters on an exact match.
/// `actor_id` here is the *subject* user's id (the user whose log an admin is
/// reading) — it also matches rows where `actor_id IS NULL` cannot match, which
/// is fine: deleted-user rows keep `actor_id = NULL` and are unreachable.
pub async fn list_for_actor(
    pool: &DbPool,
    actor_id: &str,
    event_type: Option<&str>,
    limit: i64,
) -> Result<Vec<AuditEventRow>, String> {
    let limit = limit.clamp(1, 500);
    match pool {
        DbPool::Postgres(p) => {
            let q = match event_type {
                Some(_) => format!(
                    "{SELECT_PG}
 WHERE actor_id = $1 AND event_type = $2
 ORDER BY created_at DESC, id DESC
 LIMIT $3"
                ),
                None => format!(
                    "{SELECT_PG}
 WHERE actor_id = $1
 ORDER BY created_at DESC, id DESC
 LIMIT $2"
                ),
            };
            let mut query = sqlx::query(sqlx::AssertSqlSafe(&*q)).bind(actor_id);
            if let Some(et) = event_type {
                query = query.bind(et);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list audit events failed: {e}"))?;
            rows.into_iter().map(|r| Ok(map_event!(r))).collect()
        }
        DbPool::MySql(p) => {
            let q = match event_type {
                Some(_) => format!(
                    "{SELECT_MYSQL}
 WHERE actor_id = ? AND event_type = ?
 ORDER BY created_at DESC, id DESC
 LIMIT ?"
                ),
                None => format!(
                    "{SELECT_MYSQL}
 WHERE actor_id = ?
 ORDER BY created_at DESC, id DESC
 LIMIT ?"
                ),
            };
            let mut query = sqlx::query(sqlx::AssertSqlSafe(&*q)).bind(actor_id);
            if let Some(et) = event_type {
                query = query.bind(et);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list audit events failed: {e}"))?;
            rows.into_iter().map(|r| Ok(map_event!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let q = match event_type {
                Some(_) => format!(
                    "{SELECT_SQLITE}
 WHERE actor_id = ?1 AND event_type = ?2
 ORDER BY created_at DESC, id DESC
 LIMIT ?3"
                ),
                None => format!(
                    "{SELECT_SQLITE}
 WHERE actor_id = ?1
 ORDER BY created_at DESC, id DESC
 LIMIT ?2"
                ),
            };
            let mut query = sqlx::query(sqlx::AssertSqlSafe(&*q)).bind(actor_id);
            if let Some(et) = event_type {
                query = query.bind(et);
            }
            let rows = query
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list audit events failed: {e}"))?;
            rows.into_iter().map(|r| Ok(map_event!(r))).collect()
        }
    }
}

/// Distinct event types seen for one actor (filter dropdown in the admin UI).
pub async fn list_event_types_for_actor(
    pool: &DbPool,
    actor_id: &str,
) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT DISTINCT event_type FROM audit_events WHERE actor_id = $1 ORDER BY event_type",
        )
        .bind(actor_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list audit event types failed: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT DISTINCT event_type FROM audit_events WHERE actor_id = ? ORDER BY event_type",
        )
        .bind(actor_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list audit event types failed: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT DISTINCT event_type FROM audit_events WHERE actor_id = ?1 ORDER BY event_type",
        )
        .bind(actor_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list audit event types failed: {e}")),
    }
}
