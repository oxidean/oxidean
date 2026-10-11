//! Webhooks + deliveries persistence (Phase 18 / HOOK-01..03).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct WebhookRow {
    pub id: String,
    pub repository_id: String,
    pub url: String,
    pub secret: String,
    pub active: bool,
    pub events: String,
    pub name: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct WebhookDeliveryRow {
    pub id: String,
    pub webhook_id: String,
    pub delivery_guid: String,
    pub event: String,
    pub action: String,
    pub payload_json: String,
    pub status: String,
    pub next_attempt_at: Option<String>,
    pub attempt_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct WebhookDeliveryAttemptRow {
    pub id: String,
    pub delivery_id: String,
    pub attempt_number: i64,
    pub attempted_at: String,
    pub http_status: Option<i32>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
    pub response_snippet: Option<String>,
}

macro_rules! map_hook {
    ($row:expr) => {{
        let row = $row;
        let active_i: i64 = row
            .try_get::<i32, _>("active")
            .map(|v| i64::from(v))
            .or_else(|_| row.try_get::<i64, _>("active"))
            .or_else(|_| {
                row.try_get::<bool, _>("active")
                    .map(|b| if b { 1 } else { 0 })
            })
            .map_err(|e| format!("active: {e}"))?;
        WebhookRow {
            id: row.try_get("id").map_err(|e| format!("id: {e}"))?,
            repository_id: row
                .try_get("repository_id")
                .map_err(|e| format!("repository_id: {e}"))?,
            url: row.try_get("url").map_err(|e| format!("url: {e}"))?,
            secret: row.try_get("secret").map_err(|e| format!("secret: {e}"))?,
            active: active_i != 0,
            events: row.try_get("events").map_err(|e| format!("events: {e}"))?,
            name: row.try_get("name").map_err(|e| format!("name: {e}"))?,
            created_by: row
                .try_get("created_by")
                .map_err(|e| format!("created_by: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("created_at: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("updated_at: {e}"))?,
        }
    }};
}

macro_rules! map_delivery {
    ($row:expr) => {{
        let row = $row;
        let attempt_count: i64 = row
            .try_get::<i64, _>("attempt_count")
            .or_else(|_| row.try_get::<i32, _>("attempt_count").map(|v| i64::from(v)))
            .unwrap_or(0);
        WebhookDeliveryRow {
            id: row.try_get("id").map_err(|e| format!("id: {e}"))?,
            webhook_id: row
                .try_get("webhook_id")
                .map_err(|e| format!("webhook_id: {e}"))?,
            delivery_guid: row
                .try_get("delivery_guid")
                .map_err(|e| format!("delivery_guid: {e}"))?,
            event: row.try_get("event").map_err(|e| format!("event: {e}"))?,
            action: row.try_get("action").map_err(|e| format!("action: {e}"))?,
            payload_json: row
                .try_get("payload_json")
                .map_err(|e| format!("payload_json: {e}"))?,
            status: row.try_get("status").map_err(|e| format!("status: {e}"))?,
            next_attempt_at: row.try_get("next_attempt_at").ok(),
            attempt_count,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("created_at: {e}"))?,
        }
    }};
}

macro_rules! map_attempt {
    ($row:expr) => {{
        let row = $row;
        let attempt_number: i64 = row
            .try_get::<i64, _>("attempt_number")
            .or_else(|_| {
                row.try_get::<i32, _>("attempt_number")
                    .map(|v| i64::from(v))
            })
            .map_err(|e| format!("attempt_number: {e}"))?;
        let http_status: Option<i32> = row
            .try_get::<i32, _>("http_status")
            .ok()
            .or_else(|| row.try_get::<i64, _>("http_status").ok().map(|v| v as i32));
        let duration_ms: Option<i64> = row.try_get::<i64, _>("duration_ms").ok().or_else(|| {
            row.try_get::<i32, _>("duration_ms")
                .ok()
                .map(|v| i64::from(v))
        });
        WebhookDeliveryAttemptRow {
            id: row.try_get("id").map_err(|e| format!("id: {e}"))?,
            delivery_id: row
                .try_get("delivery_id")
                .map_err(|e| format!("delivery_id: {e}"))?,
            attempt_number,
            attempted_at: row
                .try_get("attempted_at")
                .map_err(|e| format!("attempted_at: {e}"))?,
            http_status,
            error_message: row.try_get("error_message").ok(),
            duration_ms,
            response_snippet: row.try_get("response_snippet").ok(),
        }
    }};
}

const HOOK_SEL_PG: &str = "SELECT id, repository_id, url, secret, CASE WHEN active THEN 1::bigint ELSE 0::bigint END AS active, events, name, created_by, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at, to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at FROM webhooks";
const HOOK_SEL_MY: &str = "SELECT id, repository_id, url, secret, active, events, name, created_by, DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at, DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at FROM webhooks";
const HOOK_SEL_SQ: &str = "SELECT id, repository_id, url, secret, active, events, name, created_by, strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at, strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at FROM webhooks";

const DEL_SEL_PG: &str = "SELECT id, webhook_id, delivery_guid, event, action, payload_json, status, CASE WHEN next_attempt_at IS NULL THEN NULL ELSE to_char(next_attempt_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS next_attempt_at, attempt_count, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at FROM webhook_deliveries";
const DEL_SEL_MY: &str = "SELECT id, webhook_id, delivery_guid, event, action, payload_json, status, CASE WHEN next_attempt_at IS NULL THEN NULL ELSE DATE_FORMAT(next_attempt_at, '%Y-%m-%dT%H:%i:%sZ') END AS next_attempt_at, attempt_count, DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at FROM webhook_deliveries";
const DEL_SEL_SQ: &str = "SELECT id, webhook_id, delivery_guid, event, action, payload_json, status, CASE WHEN next_attempt_at IS NULL THEN NULL ELSE strftime('%Y-%m-%dT%H:%M:%SZ', next_attempt_at) END AS next_attempt_at, attempt_count, strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at FROM webhook_deliveries";

const ATT_SEL_PG: &str = "SELECT id, delivery_id, attempt_number, to_char(attempted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS attempted_at, http_status, error_message, duration_ms, response_snippet FROM webhook_delivery_attempts";
const ATT_SEL_MY: &str = "SELECT id, delivery_id, attempt_number, DATE_FORMAT(attempted_at, '%Y-%m-%dT%H:%i:%sZ') AS attempted_at, http_status, error_message, duration_ms, response_snippet FROM webhook_delivery_attempts";
const ATT_SEL_SQ: &str = "SELECT id, delivery_id, attempt_number, strftime('%Y-%m-%dT%H:%M:%SZ', attempted_at) AS attempted_at, http_status, error_message, duration_ms, response_snippet FROM webhook_delivery_attempts";

#[allow(clippy::too_many_arguments)]
pub async fn insert_webhook(
    pool: &DbPool,
    id: &str,
    repository_id: &str,
    url: &str,
    secret: &str,
    active: bool,
    events_json: &str,
    name: &str,
    created_by: &str,
) -> Result<WebhookRow, String> {
    let a: i64 = if active { 1 } else { 0 };
    match pool {
        DbPool::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO webhooks (id, repository_id, url, secret, active, events, name, created_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(id)
            .bind(repository_id)
            .bind(url)
            .bind(secret)
            .bind(active)
            .bind(events_json)
            .bind(name)
            .bind(created_by)
            .execute(pool)
            .await
            .map_err(|e| format!("insert webhook: {e}"))?;
        }
        DbPool::MySql(pool) => {
            sqlx::query(
                "INSERT INTO webhooks (id, repository_id, url, secret, active, events, name, created_by) VALUES (?,?,?,?,?,?,?,?)",
            )
            .bind(id)
            .bind(repository_id)
            .bind(url)
            .bind(secret)
            .bind(a)
            .bind(events_json)
            .bind(name)
            .bind(created_by)
            .execute(pool)
            .await
            .map_err(|e| format!("insert webhook: {e}"))?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO webhooks (id, repository_id, url, secret, active, events, name, created_by) VALUES (?,?,?,?,?,?,?,?)",
            )
            .bind(id)
            .bind(repository_id)
            .bind(url)
            .bind(secret)
            .bind(a)
            .bind(events_json)
            .bind(name)
            .bind(created_by)
            .execute(pool)
            .await
            .map_err(|e| format!("insert webhook: {e}"))?;
        }
    }
    get_webhook(pool, id).await
}

pub async fn get_webhook(pool: &DbPool, id: &str) -> Result<WebhookRow, String> {
    match pool {
        DbPool::Postgres(pool) => {
            let q = format!("{HOOK_SEL_PG} WHERE id = $1");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("get webhook: {e}"))?
                .ok_or_else(|| "webhook not found".to_string())?;
            Ok(map_hook!(row))
        }
        DbPool::MySql(pool) => {
            let q = format!("{HOOK_SEL_MY} WHERE id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("get webhook: {e}"))?
                .ok_or_else(|| "webhook not found".to_string())?;
            Ok(map_hook!(row))
        }
        DbPool::Sqlite(pool) => {
            let q = format!("{HOOK_SEL_SQ} WHERE id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("get webhook: {e}"))?
                .ok_or_else(|| "webhook not found".to_string())?;
            Ok(map_hook!(row))
        }
    }
}

pub async fn list_webhooks_for_repo(
    pool: &DbPool,
    repository_id: &str,
) -> Result<Vec<WebhookRow>, String> {
    match pool {
        DbPool::Postgres(pool) => {
            let q = format!("{HOOK_SEL_PG} WHERE repository_id = $1 ORDER BY created_at DESC");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(repository_id)
                .fetch_all(pool)
                .await
                .map_err(|e| format!("list webhooks: {e}"))?;
            rows.into_iter().map(|r| Ok(map_hook!(r))).collect()
        }
        DbPool::MySql(pool) => {
            let q = format!("{HOOK_SEL_MY} WHERE repository_id = ? ORDER BY created_at DESC");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(repository_id)
                .fetch_all(pool)
                .await
                .map_err(|e| format!("list webhooks: {e}"))?;
            rows.into_iter().map(|r| Ok(map_hook!(r))).collect()
        }
        DbPool::Sqlite(pool) => {
            let q = format!("{HOOK_SEL_SQ} WHERE repository_id = ? ORDER BY created_at DESC");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(repository_id)
                .fetch_all(pool)
                .await
                .map_err(|e| format!("list webhooks: {e}"))?;
            rows.into_iter().map(|r| Ok(map_hook!(r))).collect()
        }
    }
}

pub async fn list_active_webhooks_for_event(
    pool: &DbPool,
    repository_id: &str,
    event: &str,
) -> Result<Vec<WebhookRow>, String> {
    let all = list_webhooks_for_repo(pool, repository_id).await?;
    Ok(all
        .into_iter()
        .filter(|h| {
            if !h.active {
                return false;
            }
            // events stored as JSON string array e.g. ["issues","push"]
            let needle = format!("\"{event}\"");
            h.events.contains(&needle) || h.events.contains("\"*\"")
        })
        .collect())
}

pub async fn update_webhook(
    pool: &DbPool,
    id: &str,
    url: Option<&str>,
    secret: Option<&str>,
    active: Option<bool>,
    events_json: Option<&str>,
    name: Option<&str>,
) -> Result<WebhookRow, String> {
    let current = get_webhook(pool, id).await?;
    let url = url.unwrap_or(&current.url);
    let secret = secret.unwrap_or(&current.secret);
    let active = active.unwrap_or(current.active);
    let events_json = events_json.unwrap_or(&current.events);
    let name = name.unwrap_or(&current.name);
    let a: i64 = if active { 1 } else { 0 };
    match pool {
        DbPool::Postgres(pool) => {
            sqlx::query(
                "UPDATE webhooks SET url=$2, secret=$3, active=$4, events=$5, name=$6, updated_at=now() WHERE id=$1",
            )
            .bind(id)
            .bind(url)
            .bind(secret)
            .bind(active)
            .bind(events_json)
            .bind(name)
            .execute(pool)
            .await
            .map_err(|e| format!("update webhook: {e}"))?;
        }
        DbPool::MySql(pool) => {
            sqlx::query(
                "UPDATE webhooks SET url=?, secret=?, active=?, events=?, name=?, updated_at=CURRENT_TIMESTAMP WHERE id=?",
            )
            .bind(url)
            .bind(secret)
            .bind(a)
            .bind(events_json)
            .bind(name)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| format!("update webhook: {e}"))?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query(
                "UPDATE webhooks SET url=?, secret=?, active=?, events=?, name=?, updated_at=strftime('%Y-%m-%d %H:%M:%S','now') WHERE id=?",
            )
            .bind(url)
            .bind(secret)
            .bind(a)
            .bind(events_json)
            .bind(name)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| format!("update webhook: {e}"))?;
        }
    }
    get_webhook(pool, id).await
}

pub async fn delete_webhook(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(pool) => {
            let r = sqlx::query("DELETE FROM webhooks WHERE id = $1")
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| format!("delete webhook: {e}"))?;
            if r.rows_affected() == 0 {
                return Err("webhook not found".into());
            }
        }
        DbPool::MySql(pool) => {
            let r = sqlx::query("DELETE FROM webhooks WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| format!("delete webhook: {e}"))?;
            if r.rows_affected() == 0 {
                return Err("webhook not found".into());
            }
        }
        DbPool::Sqlite(pool) => {
            let r = sqlx::query("DELETE FROM webhooks WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| format!("delete webhook: {e}"))?;
            if r.rows_affected() == 0 {
                return Err("webhook not found".into());
            }
        }
    }
    Ok(())
}

pub async fn insert_delivery(
    pool: &DbPool,
    id: &str,
    webhook_id: &str,
    delivery_guid: &str,
    event: &str,
    action: &str,
    payload_json: &str,
) -> Result<WebhookDeliveryRow, String> {
    match pool {
        DbPool::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO webhook_deliveries (id, webhook_id, delivery_guid, event, action, payload_json, status, next_attempt_at, attempt_count) VALUES ($1,$2,$3,$4,$5,$6,'pending',now(),0)",
            )
            .bind(id)
            .bind(webhook_id)
            .bind(delivery_guid)
            .bind(event)
            .bind(action)
            .bind(payload_json)
            .execute(pool)
            .await
            .map_err(|e| format!("insert delivery: {e}"))?;
        }
        DbPool::MySql(pool) => {
            sqlx::query(
                "INSERT INTO webhook_deliveries (id, webhook_id, delivery_guid, event, action, payload_json, status, next_attempt_at, attempt_count) VALUES (?,?,?,?,?,?,'pending',CURRENT_TIMESTAMP,0)",
            )
            .bind(id)
            .bind(webhook_id)
            .bind(delivery_guid)
            .bind(event)
            .bind(action)
            .bind(payload_json)
            .execute(pool)
            .await
            .map_err(|e| format!("insert delivery: {e}"))?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO webhook_deliveries (id, webhook_id, delivery_guid, event, action, payload_json, status, next_attempt_at, attempt_count) VALUES (?,?,?,?,?,?,'pending',strftime('%Y-%m-%d %H:%M:%S','now'),0)",
            )
            .bind(id)
            .bind(webhook_id)
            .bind(delivery_guid)
            .bind(event)
            .bind(action)
            .bind(payload_json)
            .execute(pool)
            .await
            .map_err(|e| format!("insert delivery: {e}"))?;
        }
    }
    get_delivery(pool, id).await
}

pub async fn get_delivery(pool: &DbPool, id: &str) -> Result<WebhookDeliveryRow, String> {
    match pool {
        DbPool::Postgres(pool) => {
            let q = format!("{DEL_SEL_PG} WHERE id = $1");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("get delivery: {e}"))?
                .ok_or_else(|| "delivery not found".to_string())?;
            Ok(map_delivery!(row))
        }
        DbPool::MySql(pool) => {
            let q = format!("{DEL_SEL_MY} WHERE id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("get delivery: {e}"))?
                .ok_or_else(|| "delivery not found".to_string())?;
            Ok(map_delivery!(row))
        }
        DbPool::Sqlite(pool) => {
            let q = format!("{DEL_SEL_SQ} WHERE id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("get delivery: {e}"))?
                .ok_or_else(|| "delivery not found".to_string())?;
            Ok(map_delivery!(row))
        }
    }
}

pub async fn list_deliveries_for_webhook(
    pool: &DbPool,
    webhook_id: &str,
    limit: i64,
) -> Result<Vec<WebhookDeliveryRow>, String> {
    let limit = limit.clamp(1, 100);
    match pool {
        DbPool::Postgres(pool) => {
            let q = format!("{DEL_SEL_PG} WHERE webhook_id = $1 ORDER BY created_at DESC LIMIT $2");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(webhook_id)
                .bind(limit)
                .fetch_all(pool)
                .await
                .map_err(|e| format!("list deliveries: {e}"))?;
            rows.into_iter().map(|r| Ok(map_delivery!(r))).collect()
        }
        DbPool::MySql(pool) => {
            let q = format!("{DEL_SEL_MY} WHERE webhook_id = ? ORDER BY created_at DESC LIMIT ?");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(webhook_id)
                .bind(limit)
                .fetch_all(pool)
                .await
                .map_err(|e| format!("list deliveries: {e}"))?;
            rows.into_iter().map(|r| Ok(map_delivery!(r))).collect()
        }
        DbPool::Sqlite(pool) => {
            let q = format!("{DEL_SEL_SQ} WHERE webhook_id = ? ORDER BY created_at DESC LIMIT ?");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(webhook_id)
                .bind(limit)
                .fetch_all(pool)
                .await
                .map_err(|e| format!("list deliveries: {e}"))?;
            rows.into_iter().map(|r| Ok(map_delivery!(r))).collect()
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_delivery_attempt(
    pool: &DbPool,
    id: &str,
    delivery_id: &str,
    attempt_number: i64,
    http_status: Option<i32>,
    error_message: Option<&str>,
    duration_ms: Option<i64>,
    response_snippet: Option<&str>,
) -> Result<WebhookDeliveryAttemptRow, String> {
    match pool {
        DbPool::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO webhook_delivery_attempts (id, delivery_id, attempt_number, http_status, error_message, duration_ms, response_snippet) VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(id)
            .bind(delivery_id)
            .bind(attempt_number)
            .bind(http_status)
            .bind(error_message)
            .bind(duration_ms)
            .bind(response_snippet)
            .execute(pool)
            .await
            .map_err(|e| format!("insert attempt: {e}"))?;
        }
        DbPool::MySql(pool) => {
            sqlx::query(
                "INSERT INTO webhook_delivery_attempts (id, delivery_id, attempt_number, http_status, error_message, duration_ms, response_snippet) VALUES (?,?,?,?,?,?,?)",
            )
            .bind(id)
            .bind(delivery_id)
            .bind(attempt_number)
            .bind(http_status)
            .bind(error_message)
            .bind(duration_ms)
            .bind(response_snippet)
            .execute(pool)
            .await
            .map_err(|e| format!("insert attempt: {e}"))?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO webhook_delivery_attempts (id, delivery_id, attempt_number, http_status, error_message, duration_ms, response_snippet) VALUES (?,?,?,?,?,?,?)",
            )
            .bind(id)
            .bind(delivery_id)
            .bind(attempt_number)
            .bind(http_status)
            .bind(error_message)
            .bind(duration_ms)
            .bind(response_snippet)
            .execute(pool)
            .await
            .map_err(|e| format!("insert attempt: {e}"))?;
        }
    }
    latest_attempt_for_delivery(pool, delivery_id)
        .await?
        .ok_or_else(|| "attempt not found".to_string())
}

pub async fn latest_attempt_for_delivery(
    pool: &DbPool,
    delivery_id: &str,
) -> Result<Option<WebhookDeliveryAttemptRow>, String> {
    match pool {
        DbPool::Postgres(pool) => {
            let q =
                format!("{ATT_SEL_PG} WHERE delivery_id = $1 ORDER BY attempt_number DESC LIMIT 1");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(delivery_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("latest attempt: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_attempt!(r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(pool) => {
            let q =
                format!("{ATT_SEL_MY} WHERE delivery_id = ? ORDER BY attempt_number DESC LIMIT 1");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(delivery_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("latest attempt: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_attempt!(r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(pool) => {
            let q =
                format!("{ATT_SEL_SQ} WHERE delivery_id = ? ORDER BY attempt_number DESC LIMIT 1");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(delivery_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("latest attempt: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_attempt!(r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn mark_delivery_result(
    pool: &DbPool,
    delivery_id: &str,
    status: &str,
    attempt_count: i64,
    next_attempt_at: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(pool) => {
            if let Some(next) = next_attempt_at {
                sqlx::query(
                    "UPDATE webhook_deliveries SET status=$2, attempt_count=$3, next_attempt_at=$4::timestamptz WHERE id=$1",
                )
                .bind(delivery_id)
                .bind(status)
                .bind(attempt_count)
                .bind(next)
                .execute(pool)
                .await
                .map_err(|e| format!("mark delivery: {e}"))?;
            } else {
                sqlx::query(
                    "UPDATE webhook_deliveries SET status=$2, attempt_count=$3, next_attempt_at=NULL WHERE id=$1",
                )
                .bind(delivery_id)
                .bind(status)
                .bind(attempt_count)
                .execute(pool)
                .await
                .map_err(|e| format!("mark delivery: {e}"))?;
            }
        }
        DbPool::MySql(pool) => {
            sqlx::query(
                "UPDATE webhook_deliveries SET status=?, attempt_count=?, next_attempt_at=? WHERE id=?",
            )
            .bind(status)
            .bind(attempt_count)
            .bind(next_attempt_at)
            .bind(delivery_id)
            .execute(pool)
            .await
            .map_err(|e| format!("mark delivery: {e}"))?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query(
                "UPDATE webhook_deliveries SET status=?, attempt_count=?, next_attempt_at=? WHERE id=?",
            )
            .bind(status)
            .bind(attempt_count)
            .bind(next_attempt_at)
            .bind(delivery_id)
            .execute(pool)
            .await
            .map_err(|e| format!("mark delivery: {e}"))?;
        }
    }
    Ok(())
}

/// Pending deliveries whose next_attempt_at is due (or null treated as due).
pub async fn list_pending_deliveries(
    pool: &DbPool,
    limit: i64,
) -> Result<Vec<WebhookDeliveryRow>, String> {
    let limit = limit.clamp(1, 100);
    match pool {
        DbPool::Postgres(pool) => {
            let q = format!(
                "{DEL_SEL_PG} WHERE status = 'pending' AND (next_attempt_at IS NULL OR next_attempt_at <= now()) ORDER BY created_at ASC LIMIT $1"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(limit)
                .fetch_all(pool)
                .await
                .map_err(|e| format!("list pending deliveries: {e}"))?;
            rows.into_iter().map(|r| Ok(map_delivery!(r))).collect()
        }
        DbPool::MySql(pool) => {
            let q = format!(
                "{DEL_SEL_MY} WHERE status = 'pending' AND (next_attempt_at IS NULL OR next_attempt_at <= CURRENT_TIMESTAMP) ORDER BY created_at ASC LIMIT ?"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(limit)
                .fetch_all(pool)
                .await
                .map_err(|e| format!("list pending deliveries: {e}"))?;
            rows.into_iter().map(|r| Ok(map_delivery!(r))).collect()
        }
        DbPool::Sqlite(pool) => {
            let q = format!(
                "{DEL_SEL_SQ} WHERE status = 'pending' AND (next_attempt_at IS NULL OR next_attempt_at <= strftime('%Y-%m-%d %H:%M:%S','now')) ORDER BY created_at ASC LIMIT ?"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(limit)
                .fetch_all(pool)
                .await
                .map_err(|e| format!("list pending deliveries: {e}"))?;
            rows.into_iter().map(|r| Ok(map_delivery!(r))).collect()
        }
    }
}
