//! `saved_replies` (migration 0104) — per-user saved comment replies
//! (GitHub "saved replies"). User-scoped: every row belongs to the caller;
//! there is no repo/org binding in v1.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct SavedReplyRow {
    pub id: String,
    pub user_id: String,
    /// Picker label (GitHub `name`).
    pub name: String,
    /// Markdown body inserted into a composer.
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

const SAVED_REPLY_COLS: &str = "id, user_id, name, body, created_at, updated_at";

macro_rules! map_reply {
    ($row:expr) => {{
        let row = $row;
        SavedReplyRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("saved reply row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("saved reply row: {e}"))?,
            name: row
                .try_get("name")
                .map_err(|e| format!("saved reply row: {e}"))?,
            body: row
                .try_get("body")
                .map_err(|e| format!("saved reply row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("saved reply row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("saved reply row: {e}"))?,
        }
    }};
}

/// All of a user's saved replies, most recently updated first.
pub async fn list_saved_replies(
    pool: &DbPool,
    user_id: &str,
) -> Result<Vec<SavedReplyRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {SAVED_REPLY_COLS} FROM saved_replies WHERE user_id = $1 ORDER BY updated_at DESC"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list saved replies failed: {e}"))?;
            rows.iter().map(|r| Ok(map_reply!(r))).collect()
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {SAVED_REPLY_COLS} FROM saved_replies WHERE user_id = ? ORDER BY updated_at DESC"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list saved replies failed: {e}"))?;
            rows.iter().map(|r| Ok(map_reply!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {SAVED_REPLY_COLS} FROM saved_replies WHERE user_id = ?1 ORDER BY updated_at DESC"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list saved replies failed: {e}"))?;
            rows.iter().map(|r| Ok(map_reply!(r))).collect()
        }
    }
}

/// Fetch one reply scoped to its owner — `None` when missing or not the
/// caller's (anti-enumeration: same shape either way).
pub async fn find_saved_reply(
    pool: &DbPool,
    user_id: &str,
    id: &str,
) -> Result<Option<SavedReplyRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {SAVED_REPLY_COLS} FROM saved_replies WHERE user_id = $1 AND id = $2"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find saved reply failed: {e}"))?;
            row.map(|r| Ok(map_reply!(&r))).transpose()
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {SAVED_REPLY_COLS} FROM saved_replies WHERE user_id = ? AND id = ?"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find saved reply failed: {e}"))?;
            row.map(|r| Ok(map_reply!(&r))).transpose()
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {SAVED_REPLY_COLS} FROM saved_replies WHERE user_id = ?1 AND id = ?2"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find saved reply failed: {e}"))?;
            row.map(|r| Ok(map_reply!(&r))).transpose()
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_saved_reply(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    name: &str,
    body: &str,
) -> Result<SavedReplyRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO saved_replies (id, user_id, name, body) VALUES ($1, $2, $3, $4)",
            )
            .bind(id)
            .bind(user_id)
            .bind(name)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert saved reply failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("INSERT INTO saved_replies (id, user_id, name, body) VALUES (?, ?, ?, ?)")
                .bind(id)
                .bind(user_id)
                .bind(name)
                .bind(body)
                .execute(p)
                .await
                .map_err(|e| format!("insert saved reply failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO saved_replies (id, user_id, name, body) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(id)
            .bind(user_id)
            .bind(name)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert saved reply failed: {e}"))?;
        }
    }
    find_saved_reply(pool, user_id, id)
        .await?
        .ok_or_else(|| "insert saved reply failed: row missing after insert".into())
}

pub async fn update_saved_reply(
    pool: &DbPool,
    user_id: &str,
    id: &str,
    name: &str,
    body: &str,
) -> Result<Option<SavedReplyRow>, String> {
    let now = match pool {
        DbPool::Sqlite(_) => "strftime('%Y-%m-%d %H:%M:%S','now')",
        _ => "CURRENT_TIMESTAMP",
    };
    let changed = match pool {
        DbPool::Postgres(p) => {
            sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "UPDATE saved_replies SET name = $1, body = $2, updated_at = {now} WHERE user_id = $3 AND id = $4"
            )))
            .bind(name)
            .bind(body)
            .bind(user_id)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update saved reply failed: {e}"))?
            .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "UPDATE saved_replies SET name = ?, body = ?, updated_at = {now} WHERE user_id = ? AND id = ?"
            )))
            .bind(name)
            .bind(body)
            .bind(user_id)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update saved reply failed: {e}"))?
            .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "UPDATE saved_replies SET name = ?1, body = ?2, updated_at = {now} WHERE user_id = ?3 AND id = ?4"
            )))
            .bind(name)
            .bind(body)
            .bind(user_id)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update saved reply failed: {e}"))?
            .rows_affected()
        }
    };
    if changed == 0 {
        return Ok(None);
    }
    find_saved_reply(pool, user_id, id).await
}

/// Delete scoped to the owner — returns whether a row was removed.
pub async fn delete_saved_reply(pool: &DbPool, user_id: &str, id: &str) -> Result<bool, String> {
    let changed = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM saved_replies WHERE user_id = $1 AND id = $2")
                .bind(user_id)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete saved reply failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => sqlx::query("DELETE FROM saved_replies WHERE user_id = ? AND id = ?")
            .bind(user_id)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("delete saved reply failed: {e}"))?
            .rows_affected(),
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM saved_replies WHERE user_id = ?1 AND id = ?2")
                .bind(user_id)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete saved reply failed: {e}"))?
                .rows_affected()
        }
    };
    Ok(changed > 0)
}
