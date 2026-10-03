//! Notifications inbox rows (NOTF-01 / NOTF-02 / D-12). Dialect SQL only.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct NotificationRow {
    pub id: String,
    pub recipient_id: String,
    pub actor_id: String,
    pub reason: String,
    pub subject_kind: String,
    pub subject_repo_id: String,
    pub subject_number: i64,
    pub subject_title: String,
    /// Deep-link ref for non-numbered subjects: release tag, workflow run id.
    pub subject_ref: Option<String>,
    pub read_at: Option<String>,
    pub created_at: String,
}

macro_rules! map_notification {
    ($row:expr) => {{
        let row = $row;
        NotificationRow {
            id: row.try_get("id").map_err(|e| format!("notification row: {e}"))?,
            recipient_id: row
                .try_get("recipient_id")
                .map_err(|e| format!("notification row: {e}"))?,
            actor_id: row
                .try_get("actor_id")
                .map_err(|e| format!("notification row: {e}"))?,
            reason: row
                .try_get("reason")
                .map_err(|e| format!("notification row: {e}"))?,
            subject_kind: row
                .try_get("subject_kind")
                .map_err(|e| format!("notification row: {e}"))?,
            subject_repo_id: row
                .try_get("subject_repo_id")
                .map_err(|e| format!("notification row: {e}"))?,
            subject_number: row
                .try_get::<i64, _>("subject_number")
                .map_err(|e| format!("notification row: {e}"))?,
            subject_title: row
                .try_get("subject_title")
                .map_err(|e| format!("notification row: {e}"))?,
            subject_ref: row
                .try_get("subject_ref")
                .map_err(|e| format!("notification row: {e}"))?,
            read_at: row
                .try_get("read_at")
                .map_err(|e| format!("notification row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("notification row: {e}"))?,
        }
    }};
}

const NOTIF_SELECT_PG: &str = "SELECT id, recipient_id, actor_id, reason, subject_kind, subject_repo_id,
       subject_number, subject_title, subject_ref,
       CASE WHEN read_at IS NULL THEN NULL
            ELSE to_char(read_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS read_at,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM notifications";

const NOTIF_SELECT_MYSQL: &str = "SELECT id, recipient_id, actor_id, reason, subject_kind, subject_repo_id,
       subject_number, subject_title, subject_ref,
       CASE WHEN read_at IS NULL THEN NULL
            ELSE DATE_FORMAT(read_at, '%Y-%m-%dT%H:%i:%sZ') END AS read_at,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM notifications";

const NOTIF_SELECT_SQLITE: &str = "SELECT id, recipient_id, actor_id, reason, subject_kind, subject_repo_id,
       subject_number, subject_title, subject_ref,
       CASE WHEN read_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', read_at) END AS read_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM notifications";

pub async fn insert_notification(
    pool: &DbPool,
    id: &str,
    recipient_id: &str,
    actor_id: &str,
    reason: &str,
    subject_kind: &str,
    subject_repo_id: &str,
    subject_number: i64,
    subject_title: &str,
    subject_ref: Option<&str>,
) -> Result<NotificationRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO notifications
 (id, recipient_id, actor_id, reason, subject_kind, subject_repo_id, subject_number, subject_title, subject_ref)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(id)
            .bind(recipient_id)
            .bind(actor_id)
            .bind(reason)
            .bind(subject_kind)
            .bind(subject_repo_id)
            .bind(subject_number)
            .bind(subject_title)
            .bind(subject_ref)
            .execute(p)
            .await
            .map_err(|e| format!("insert notification failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO notifications
 (id, recipient_id, actor_id, reason, subject_kind, subject_repo_id, subject_number, subject_title, subject_ref)
 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(recipient_id)
            .bind(actor_id)
            .bind(reason)
            .bind(subject_kind)
            .bind(subject_repo_id)
            .bind(subject_number)
            .bind(subject_title)
            .bind(subject_ref)
            .execute(p)
            .await
            .map_err(|e| format!("insert notification failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO notifications
 (id, recipient_id, actor_id, reason, subject_kind, subject_repo_id, subject_number, subject_title, subject_ref)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(id)
            .bind(recipient_id)
            .bind(actor_id)
            .bind(reason)
            .bind(subject_kind)
            .bind(subject_repo_id)
            .bind(subject_number)
            .bind(subject_title)
            .bind(subject_ref)
            .execute(p)
            .await
            .map_err(|e| format!("insert notification failed: {e}"))?;
        }
    }
    find_notification_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert notification failed: row missing after insert".into())
}

pub async fn find_notification_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<NotificationRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let q = format!("{NOTIF_SELECT_PG} WHERE id = $1");
            let row = sqlx::query(&q)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find notification failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_notification!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let q = format!("{NOTIF_SELECT_MYSQL} WHERE id = ?");
            let row = sqlx::query(&q)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find notification failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_notification!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let q = format!("{NOTIF_SELECT_SQLITE} WHERE id = ?1");
            let row = sqlx::query(&q)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find notification failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_notification!(&r)),
                None => None,
            })
        }
    }
}

/// List notifications for a recipient. `unread_only` filters `read_at IS NULL`.
/// Newest-first with offset pagination (D-09).
pub async fn list_notifications(
    pool: &DbPool,
    recipient_id: &str,
    unread_only: bool,
    offset: i64,
    limit: i64,
) -> Result<(Vec<NotificationRow>, i64), String> {
    let offset = offset.max(0);
    let limit = limit.clamp(1, 100);
    match pool {
        DbPool::Postgres(p) => {
            let (count_sql, list_sql) = if unread_only {
                (
                    "SELECT count(*) FROM notifications WHERE recipient_id = $1 AND read_at IS NULL",
                    format!(
                        "{NOTIF_SELECT_PG} WHERE recipient_id = $1 AND read_at IS NULL
ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3"
                    ),
                )
            } else {
                (
                    "SELECT count(*) FROM notifications WHERE recipient_id = $1",
                    format!(
                        "{NOTIF_SELECT_PG} WHERE recipient_id = $1
ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3"
                    ),
                )
            };
            let total: i64 = sqlx::query_scalar(count_sql)
                .bind(recipient_id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("count notifications failed: {e}"))?;
            let rows = sqlx::query(&list_sql)
                .bind(recipient_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list notifications failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_notification!(&r));
            }
            Ok((out, total))
        }
        DbPool::MySql(p) => {
            let (count_sql, list_sql) = if unread_only {
                (
                    "SELECT count(*) FROM notifications WHERE recipient_id = ? AND read_at IS NULL",
                    format!(
                        "{NOTIF_SELECT_MYSQL} WHERE recipient_id = ? AND read_at IS NULL
ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?"
                    ),
                )
            } else {
                (
                    "SELECT count(*) FROM notifications WHERE recipient_id = ?",
                    format!(
                        "{NOTIF_SELECT_MYSQL} WHERE recipient_id = ?
ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?"
                    ),
                )
            };
            let total: i64 = sqlx::query_scalar(count_sql)
                .bind(recipient_id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("count notifications failed: {e}"))?;
            let rows = sqlx::query(&list_sql)
                .bind(recipient_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list notifications failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_notification!(&r));
            }
            Ok((out, total))
        }
        DbPool::Sqlite(p) => {
            let (count_sql, list_sql) = if unread_only {
                (
                    "SELECT count(*) FROM notifications WHERE recipient_id = ?1 AND read_at IS NULL",
                    format!(
                        "{NOTIF_SELECT_SQLITE} WHERE recipient_id = ?1 AND read_at IS NULL
ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3"
                    ),
                )
            } else {
                (
                    "SELECT count(*) FROM notifications WHERE recipient_id = ?1",
                    format!(
                        "{NOTIF_SELECT_SQLITE} WHERE recipient_id = ?1
ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3"
                    ),
                )
            };
            let total: i64 = sqlx::query_scalar(count_sql)
                .bind(recipient_id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("count notifications failed: {e}"))?;
            let rows = sqlx::query(&list_sql)
                .bind(recipient_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list notifications failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_notification!(&r));
            }
            Ok((out, total))
        }
    }
}

pub async fn unread_count(pool: &DbPool, recipient_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT count(*) FROM notifications WHERE recipient_id = $1 AND read_at IS NULL",
        )
        .bind(recipient_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("unread_count failed: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT count(*) FROM notifications WHERE recipient_id = ? AND read_at IS NULL",
        )
        .bind(recipient_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("unread_count failed: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT count(*) FROM notifications WHERE recipient_id = ?1 AND read_at IS NULL",
        )
        .bind(recipient_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("unread_count failed: {e}")),
    }
}

/// Mark specific ids read for recipient only (T-17-01). Returns rows updated.
pub async fn mark_read(
    pool: &DbPool,
    recipient_id: &str,
    ids: &[String],
    read_at: &str,
) -> Result<i64, String> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut marked: i64 = 0;
    for id in ids {
        let n = match pool {
            DbPool::Postgres(p) => sqlx::query(
                "UPDATE notifications SET read_at = $1
 WHERE id = $2 AND recipient_id = $3 AND read_at IS NULL",
            )
            .bind(read_at)
            .bind(id)
            .bind(recipient_id)
            .execute(p)
            .await
            .map_err(|e| format!("mark_read failed: {e}"))?
            .rows_affected(),
            DbPool::MySql(p) => sqlx::query(
                "UPDATE notifications SET read_at = ?
 WHERE id = ? AND recipient_id = ? AND read_at IS NULL",
            )
            .bind(read_at)
            .bind(id)
            .bind(recipient_id)
            .execute(p)
            .await
            .map_err(|e| format!("mark_read failed: {e}"))?
            .rows_affected(),
            DbPool::Sqlite(p) => sqlx::query(
                "UPDATE notifications SET read_at = ?1
 WHERE id = ?2 AND recipient_id = ?3 AND read_at IS NULL",
            )
            .bind(read_at)
            .bind(id)
            .bind(recipient_id)
            .execute(p)
            .await
            .map_err(|e| format!("mark_read failed: {e}"))?
            .rows_affected(),
        };
        marked += n as i64;
    }
    Ok(marked)
}

/// Mark all unread for recipient (D-12).
pub async fn mark_all_read(
    pool: &DbPool,
    recipient_id: &str,
    read_at: &str,
) -> Result<i64, String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE notifications SET read_at = $1
 WHERE recipient_id = $2 AND read_at IS NULL",
        )
        .bind(read_at)
        .bind(recipient_id)
        .execute(p)
        .await
        .map_err(|e| format!("mark_all_read failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE notifications SET read_at = ?
 WHERE recipient_id = ? AND read_at IS NULL",
        )
        .bind(read_at)
        .bind(recipient_id)
        .execute(p)
        .await
        .map_err(|e| format!("mark_all_read failed: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE notifications SET read_at = ?1
 WHERE recipient_id = ?2 AND read_at IS NULL",
        )
        .bind(read_at)
        .bind(recipient_id)
        .execute(p)
        .await
        .map_err(|e| format!("mark_all_read failed: {e}"))?
        .rows_affected(),
    };
    Ok(n as i64)
}

/// Delete every notification row a recipient has for one repository. Used by
/// access-loss pruning (DEBT-06): once read access is gone, stale inbox rows
/// must not linger.
pub async fn delete_notifications_for_repo_recipient(
    pool: &DbPool,
    recipient_id: &str,
    subject_repo_id: &str,
) -> Result<i64, String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "DELETE FROM notifications WHERE recipient_id = $1 AND subject_repo_id = $2",
        )
        .bind(recipient_id)
        .bind(subject_repo_id)
        .execute(p)
        .await
        .map_err(|e| format!("delete notifications for repo failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "DELETE FROM notifications WHERE recipient_id = ? AND subject_repo_id = ?",
        )
        .bind(recipient_id)
        .bind(subject_repo_id)
        .execute(p)
        .await
        .map_err(|e| format!("delete notifications for repo failed: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "DELETE FROM notifications WHERE recipient_id = ?1 AND subject_repo_id = ?2",
        )
        .bind(recipient_id)
        .bind(subject_repo_id)
        .execute(p)
        .await
        .map_err(|e| format!("delete notifications for repo failed: {e}"))?
        .rows_affected(),
    };
    Ok(n as i64)
}

/// Distinct repositories a recipient has notifications for — the sweep set for
/// the read-time access re-check (DEBT-06).
pub async fn list_notification_repo_ids_for_recipient(
    pool: &DbPool,
    recipient_id: &str,
) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT DISTINCT subject_repo_id FROM notifications WHERE recipient_id = $1",
        )
        .bind(recipient_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list notification repo ids failed: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT DISTINCT subject_repo_id FROM notifications WHERE recipient_id = ?",
        )
        .bind(recipient_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list notification repo ids failed: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT DISTINCT subject_repo_id FROM notifications WHERE recipient_id = ?1",
        )
        .bind(recipient_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list notification repo ids failed: {e}")),
    }
}

/// Distinct recipients holding notifications for one repository — joined with
/// the watch list, this is the affected-user set when a repo-wide ACL change
/// happens (DEBT-06).
pub async fn list_notification_recipient_ids_for_repo(
    pool: &DbPool,
    subject_repo_id: &str,
) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT DISTINCT recipient_id FROM notifications WHERE subject_repo_id = $1",
        )
        .bind(subject_repo_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list notification recipient ids failed: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT DISTINCT recipient_id FROM notifications WHERE subject_repo_id = ?",
        )
        .bind(subject_repo_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list notification recipient ids failed: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT DISTINCT recipient_id FROM notifications WHERE subject_repo_id = ?1",
        )
        .bind(subject_repo_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list notification recipient ids failed: {e}")),
    }
}
