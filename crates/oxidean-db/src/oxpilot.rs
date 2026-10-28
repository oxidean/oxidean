//! Oxpilot agentic-chat persistence — `oxpilot_sessions` (user-owned,
//! repo-bound conversations) and `oxpilot_messages` (per-session ordered
//! transcript). All reads/writes take the caller's `user_id` so cross-user
//! access resolves `None`/`not_found` at the API layer — the queries
//! themselves never leak rows across ownership. Message content arrives
//! already secrets-redacted; this module stores it verbatim.
//!
//! `seq` is per-session monotonic and unique with `(session_id, seq)`;
//! `client_msg_id` is the sender-supplied idempotency key, unique per
//! session when present (both dialects treat multiple NULLs as distinct).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct OxpilotSessionRow {
    pub id: String,
    pub user_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub repo_id: String,
    pub branch: String,
    pub goal: Option<String>,
    pub title: String,
    pub status: String,
    pub created_at: String,
    pub last_activity_at: String,
    /// The U8 session→target binding (`0090`); `None` is the shipped
    /// "no target" default.
    pub computer_use_target_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OxpilotMessageRow {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub role: String,
    pub content: String,
    pub client_msg_id: Option<String>,
    pub status: String,
    pub created_at: String,
}

macro_rules! map_session {
    ($row:expr) => {{
        let row = $row;
        OxpilotSessionRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
            repo_owner: row
                .try_get("repo_owner")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
            repo_name: row
                .try_get("repo_name")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
            branch: row
                .try_get("branch")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
            goal: row
                .try_get("goal")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
            title: row
                .try_get("title")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
            status: row
                .try_get("status")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
            last_activity_at: row
                .try_get("last_activity_at")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
            computer_use_target_id: row
                .try_get("computer_use_target_id")
                .map_err(|e| format!("oxpilot session row: {e}"))?,
        }
    }};
}

macro_rules! map_message {
    ($row:expr) => {{
        let row = $row;
        OxpilotMessageRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oxpilot message row: {e}"))?,
            session_id: row
                .try_get("session_id")
                .map_err(|e| format!("oxpilot message row: {e}"))?,
            seq: row
                .try_get::<i64, _>("seq")
                .or_else(|_| row.try_get::<i32, _>("seq").map(i64::from))
                .map_err(|e| format!("oxpilot message row: {e}"))?,
            role: row
                .try_get("role")
                .map_err(|e| format!("oxpilot message row: {e}"))?,
            content: row
                .try_get("content")
                .map_err(|e| format!("oxpilot message row: {e}"))?,
            client_msg_id: row
                .try_get("client_msg_id")
                .map_err(|e| format!("oxpilot message row: {e}"))?,
            status: row
                .try_get("status")
                .map_err(|e| format!("oxpilot message row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oxpilot message row: {e}"))?,
        }
    }};
}

const SESSION_SELECT_PG: &str = "SELECT s.id, s.user_id, s.repo_owner, s.repo_name, s.repo_id,
       s.branch, s.goal, s.title, s.status,
       to_char(s.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(s.last_activity_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_activity_at,
       s.computer_use_target_id
FROM oxpilot_sessions s";

const SESSION_SELECT_MYSQL: &str = "SELECT s.id, s.user_id, s.repo_owner, s.repo_name, s.repo_id,
       s.branch, s.goal, s.title, s.status,
       DATE_FORMAT(s.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(s.last_activity_at, '%Y-%m-%dT%H:%i:%sZ') AS last_activity_at,
       s.computer_use_target_id
FROM oxpilot_sessions s";

const SESSION_SELECT_SQLITE: &str = "SELECT s.id, s.user_id, s.repo_owner, s.repo_name, s.repo_id,
       s.branch, s.goal, s.title, s.status,
       strftime('%Y-%m-%dT%H:%M:%SZ', s.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', s.last_activity_at) AS last_activity_at,
       s.computer_use_target_id
FROM oxpilot_sessions s";

const MESSAGE_SELECT_PG: &str = "SELECT m.id, m.session_id, m.seq, m.role, m.content,
       m.client_msg_id, m.status,
       to_char(m.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM oxpilot_messages m";

const MESSAGE_SELECT_MYSQL: &str = "SELECT m.id, m.session_id, m.seq, m.role, m.content,
       m.client_msg_id, m.status,
       DATE_FORMAT(m.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM oxpilot_messages m";

const MESSAGE_SELECT_SQLITE: &str = "SELECT m.id, m.session_id, m.seq, m.role, m.content,
       m.client_msg_id, m.status,
       strftime('%Y-%m-%dT%H:%M:%SZ', m.created_at) AS created_at
FROM oxpilot_messages m";

pub struct NewOxpilotSession<'a> {
    pub id: &'a str,
    pub user_id: &'a str,
    pub repo_owner: &'a str,
    pub repo_name: &'a str,
    pub repo_id: &'a str,
    pub branch: &'a str,
    pub goal: Option<&'a str>,
    pub title: &'a str,
    /// U8 — the optional `oxpilot_targets` binding (`0090` column);
    /// `None` persists NULL, the "no target" shipped default.
    pub computer_use_target_id: Option<&'a str>,
}

pub async fn insert_session(
    pool: &DbPool,
    s: &NewOxpilotSession<'_>,
) -> Result<OxpilotSessionRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_sessions
                     (id, user_id, repo_owner, repo_name, repo_id, branch, goal, title,
                      computer_use_target_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(s.id)
            .bind(s.user_id)
            .bind(s.repo_owner)
            .bind(s.repo_name)
            .bind(s.repo_id)
            .bind(s.branch)
            .bind(s.goal)
            .bind(s.title)
            .bind(s.computer_use_target_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot session failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_sessions
                     (id, user_id, repo_owner, repo_name, repo_id, branch, goal, title,
                      computer_use_target_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(s.id)
            .bind(s.user_id)
            .bind(s.repo_owner)
            .bind(s.repo_name)
            .bind(s.repo_id)
            .bind(s.branch)
            .bind(s.goal)
            .bind(s.title)
            .bind(s.computer_use_target_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot session failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_sessions
                     (id, user_id, repo_owner, repo_name, repo_id, branch, goal, title,
                      computer_use_target_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(s.id)
            .bind(s.user_id)
            .bind(s.repo_owner)
            .bind(s.repo_name)
            .bind(s.repo_id)
            .bind(s.branch)
            .bind(s.goal)
            .bind(s.title)
            .bind(s.computer_use_target_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot session failed: {e}"))?;
        }
    }
    find_session_by_id(pool, s.id)
        .await?
        .ok_or_else(|| "insert oxpilot session: row missing after insert".to_string())
}

/// Ownership-filtered find — a session id that isn't the caller's returns
/// `None`, identical to a missing row (anti-enumeration).
pub async fn find_owned_session(
    pool: &DbPool,
    session_id: &str,
    user_id: &str,
) -> Result<Option<OxpilotSessionRow>, String> {
    find_session_where(pool, session_id, Some(user_id)).await
}

pub async fn find_session_by_id(
    pool: &DbPool,
    session_id: &str,
) -> Result<Option<OxpilotSessionRow>, String> {
    find_session_where(pool, session_id, None).await
}

async fn find_session_where(
    pool: &DbPool,
    session_id: &str,
    user_id: Option<&str>,
) -> Result<Option<OxpilotSessionRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = if user_id.is_some() {
                format!("{SESSION_SELECT_PG} WHERE s.id = $1 AND s.user_id = $2")
            } else {
                format!("{SESSION_SELECT_PG} WHERE s.id = $1")
            };
            let mut q = sqlx::query(sqlx::AssertSqlSafe(&*sql)).bind(session_id);
            if let Some(uid) = user_id {
                q = q.bind(uid);
            }
            match q
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot session failed: {e}"))?
            {
                Some(r) => Ok(Some(map_session!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = if user_id.is_some() {
                format!("{SESSION_SELECT_MYSQL} WHERE s.id = ? AND s.user_id = ?")
            } else {
                format!("{SESSION_SELECT_MYSQL} WHERE s.id = ?")
            };
            let mut q = sqlx::query(sqlx::AssertSqlSafe(&*sql)).bind(session_id);
            if let Some(uid) = user_id {
                q = q.bind(uid);
            }
            match q
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot session failed: {e}"))?
            {
                Some(r) => Ok(Some(map_session!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = if user_id.is_some() {
                format!("{SESSION_SELECT_SQLITE} WHERE s.id = ?1 AND s.user_id = ?2")
            } else {
                format!("{SESSION_SELECT_SQLITE} WHERE s.id = ?1")
            };
            let mut q = sqlx::query(sqlx::AssertSqlSafe(&*sql)).bind(session_id);
            if let Some(uid) = user_id {
                q = q.bind(uid);
            }
            match q
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot session failed: {e}"))?
            {
                Some(r) => Ok(Some(map_session!(&r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn list_sessions_for_user(
    pool: &DbPool,
    user_id: &str,
) -> Result<Vec<OxpilotSessionRow>, String> {
    let suffix = " WHERE s.user_id = {p} AND s.status <> 'deleted'
       ORDER BY s.last_activity_at DESC, s.id";
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{SESSION_SELECT_PG}{}", suffix.replace("{p}", "$1"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot sessions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_session!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!("{SESSION_SELECT_MYSQL}{}", suffix.replace("{p}", "?"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot sessions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_session!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{SESSION_SELECT_SQLITE}{}", suffix.replace("{p}", "?1"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot sessions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_session!(r));
            }
            Ok(out)
        }
    }
}

/// Bump `last_activity_at` after a message append so the index page ordering
/// tracks recency.
pub async fn touch_session_activity(pool: &DbPool, session_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE oxpilot_sessions SET last_activity_at = now() WHERE id = $1")
                .bind(session_id)
                .execute(p)
                .await
                .map_err(|e| format!("touch oxpilot session failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE oxpilot_sessions SET last_activity_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(session_id)
            .execute(p)
            .await
            .map_err(|e| format!("touch oxpilot session failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE oxpilot_sessions
                 SET last_activity_at = strftime('%Y-%m-%d %H:%M:%S','now')
                 WHERE id = ?1",
            )
            .bind(session_id)
            .execute(p)
            .await
            .map_err(|e| format!("touch oxpilot session failed: {e}"))?;
        }
    }
    Ok(())
}

/// U8 — stamp the optional session→target binding (`0090` column) on an
/// existing session row. The caller resolves ownership + the target's
/// `connected` lifecycle first; `None` re-clears the binding. Returns
/// rows affected (0 ⇒ the session id was absent).
pub async fn set_session_computer_use_target(
    pool: &DbPool,
    session_id: &str,
    target_id: Option<&str>,
) -> Result<u64, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE oxpilot_sessions SET computer_use_target_id = $2 WHERE id = $1")
                .bind(session_id)
                .bind(target_id)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
                .map_err(|e| format!("set oxpilot session target failed: {e}"))
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE oxpilot_sessions SET computer_use_target_id = ? WHERE id = ?")
                .bind(target_id)
                .bind(session_id)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
                .map_err(|e| format!("set oxpilot session target failed: {e}"))
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE oxpilot_sessions SET computer_use_target_id = ?2 WHERE id = ?1")
                .bind(session_id)
                .bind(target_id)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
                .map_err(|e| format!("set oxpilot session target failed: {e}"))
        }
    }
}

pub struct NewOxpilotMessage<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub seq: i64,
    pub role: &'a str,
    pub content: &'a str,
    pub client_msg_id: Option<&'a str>,
    pub status: &'a str,
}

pub async fn insert_message(
    pool: &DbPool,
    m: &NewOxpilotMessage<'_>,
) -> Result<OxpilotMessageRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_messages
                     (id, session_id, seq, role, content, client_msg_id, status)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(m.id)
            .bind(m.session_id)
            .bind(m.seq)
            .bind(m.role)
            .bind(m.content)
            .bind(m.client_msg_id)
            .bind(m.status)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot message failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_messages
                     (id, session_id, seq, role, content, client_msg_id, status)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(m.id)
            .bind(m.session_id)
            .bind(m.seq)
            .bind(m.role)
            .bind(m.content)
            .bind(m.client_msg_id)
            .bind(m.status)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot message failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_messages
                     (id, session_id, seq, role, content, client_msg_id, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(m.id)
            .bind(m.session_id)
            .bind(m.seq)
            .bind(m.role)
            .bind(m.content)
            .bind(m.client_msg_id)
            .bind(m.status)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot message failed: {e}"))?;
        }
    }
    find_message_by_id(pool, m.id)
        .await?
        .ok_or_else(|| "insert oxpilot message: row missing after insert".to_string())
}

/// Next free `seq` for the session — `MAX(seq)+1`, 1 when empty.
pub async fn next_message_seq(pool: &DbPool, session_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(seq) + 1, 1) FROM oxpilot_messages WHERE session_id = $1",
        )
        .bind(session_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("next oxpilot seq failed: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(seq) + 1, 1) FROM oxpilot_messages WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("next oxpilot seq failed: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(seq) + 1, 1) FROM oxpilot_messages WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("next oxpilot seq failed: {e}")),
    }
}

pub async fn find_message_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<OxpilotMessageRow>, String> {
    find_message_where(pool, "m.id", id).await
}

/// Idempotency lookup — the user row persisted for `client_msg_id`, if any.
pub async fn find_message_by_client_msg_id(
    pool: &DbPool,
    session_id: &str,
    client_msg_id: &str,
) -> Result<Option<OxpilotMessageRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql =
                format!("{MESSAGE_SELECT_PG} WHERE m.session_id = $1 AND m.client_msg_id = $2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .bind(client_msg_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_message!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql =
                format!("{MESSAGE_SELECT_MYSQL} WHERE m.session_id = ? AND m.client_msg_id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .bind(client_msg_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_message!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql =
                format!("{MESSAGE_SELECT_SQLITE} WHERE m.session_id = ?1 AND m.client_msg_id = ?2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .bind(client_msg_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_message!(&r))),
                None => Ok(None),
            }
        }
    }
}

async fn find_message_where(
    pool: &DbPool,
    col: &str,
    val: &str,
) -> Result<Option<OxpilotMessageRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{MESSAGE_SELECT_PG} WHERE {col} = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(val)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_message!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{MESSAGE_SELECT_MYSQL} WHERE {col} = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(val)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_message!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{MESSAGE_SELECT_SQLITE} WHERE {col} = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(val)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_message!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Transcript snapshot — one indexed `(session_id, seq)` scan.
pub async fn list_messages(
    pool: &DbPool,
    session_id: &str,
) -> Result<Vec<OxpilotMessageRow>, String> {
    let suffix = " WHERE m.session_id = {p} ORDER BY m.seq";
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{MESSAGE_SELECT_PG}{}", suffix.replace("{p}", "$1"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot messages failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_message!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!("{MESSAGE_SELECT_MYSQL}{}", suffix.replace("{p}", "?"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot messages failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_message!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{MESSAGE_SELECT_SQLITE}{}", suffix.replace("{p}", "?1"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot messages failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_message!(r));
            }
            Ok(out)
        }
    }
}

/// Streaming accumulation — the turn task rewrites `content` with the
/// redacted running text so a mid-turn reader sees partial output.
pub async fn set_message_content(pool: &DbPool, id: &str, content: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE oxpilot_messages SET content = $1 WHERE id = $2")
                .bind(content)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update oxpilot message failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE oxpilot_messages SET content = ? WHERE id = ?")
                .bind(content)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update oxpilot message failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE oxpilot_messages SET content = ?1 WHERE id = ?2")
                .bind(content)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update oxpilot message failed: {e}"))?;
        }
    }
    Ok(())
}

pub async fn set_message_status(pool: &DbPool, id: &str, status: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE oxpilot_messages SET status = $1 WHERE id = $2")
                .bind(status)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update oxpilot message status failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE oxpilot_messages SET status = ? WHERE id = ?")
                .bind(status)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update oxpilot message status failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE oxpilot_messages SET status = ?1 WHERE id = ?2")
                .bind(status)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update oxpilot message status failed: {e}"))?;
        }
    }
    Ok(())
}

/// Boot-time orphan sweep — a turn task that died with the process leaves
/// rows at `status='streaming'`; they become `error` so a reopened transcript
/// renders them as interrupted rather than forever-live.
pub async fn sweep_streaming_messages(pool: &DbPool) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE oxpilot_messages SET status = 'error' WHERE status = 'streaming'")
                .execute(p)
                .await
                .map_err(|e| format!("sweep oxpilot streaming messages failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE oxpilot_messages SET status = 'error' WHERE status = 'streaming'")
                .execute(p)
                .await
                .map_err(|e| format!("sweep oxpilot streaming messages failed: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE oxpilot_messages SET status = 'error' WHERE status = 'streaming'")
                .execute(p)
                .await
                .map_err(|e| format!("sweep oxpilot streaming messages failed: {e}"))?
                .rows_affected()
        }
    };
    Ok(n)
}

// ---------- 0087 session-core rows (U2) ----------

/// One durable tool-call card — the persisted form of the C2 `tool` frame,
/// keyed `(session_id, call_id)` and merged into the snapshot by
/// `message_id` ordered by `call_index`.
#[derive(Debug, Clone)]
pub struct OxpilotToolCallRow {
    pub id: String,
    pub session_id: String,
    pub message_id: String,
    pub call_id: String,
    /// Positional copy of the owning assistant message's `seq` — minted by
    /// the caller, never allocated here.
    pub seq: i64,
    /// Intra-message ordinal — repeated same-tool calls under one message
    /// sort by this, not `call_id`.
    pub call_index: i64,
    pub tool: String,
    pub args_summary: String,
    pub output_summary: String,
    pub status: String,
    pub created_at: String,
}

/// One pending follow-up (US1.8) — presence in the table IS the queued
/// state; `drain_queue_tx` delivers rows in `queue_seq` order atomically.
#[derive(Debug, Clone)]
pub struct OxpilotQueuedRow {
    pub id: String,
    pub session_id: String,
    pub queue_seq: i64,
    pub content: String,
    pub client_msg_id: Option<String>,
    pub created_at: String,
}

/// One proposed diff (WF8) — `status` is `pending|applied`; `applied` is
/// terminal and `pr_ref` records the materialized pull reference.
#[derive(Debug, Clone)]
pub struct OxpilotProposalRow {
    pub id: String,
    pub session_id: String,
    pub title: String,
    /// Base ref/sha the diff applies against (C1 `ProposedDiff.base`).
    pub base: String,
    /// Canonical unified-diff body — `files[{path,patch}]` derives by
    /// splitting on 'diff --git' headers at read time.
    pub diff_body: String,
    /// Serialized ordered `DiffFileOp` list the apply path executes
    /// verbatim (migration 0089; `[]` on pre-column rows).
    pub ops_json: String,
    pub status: String,
    pub pr_ref: Option<String>,
    pub created_at: String,
}

/// One UTC-day usage rollup row (US5.2) — standalone by design (no FK):
/// counters survive session and user deletion. `(scope, scope_key, window)`
/// is the identity; `scope` ∈ `user|instance`, `window` is `YYYY-MM-DD`.
#[derive(Debug, Clone)]
pub struct OxpilotUsageCounterRow {
    pub id: String,
    pub scope: String,
    pub scope_key: String,
    pub window: String,
    pub requests: i64,
    pub tokens: i64,
    pub updated_at: String,
}

macro_rules! map_queued {
    ($row:expr) => {{
        let row = $row;
        OxpilotQueuedRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oxpilot queued row: {e}"))?,
            session_id: row
                .try_get("session_id")
                .map_err(|e| format!("oxpilot queued row: {e}"))?,
            queue_seq: row
                .try_get::<i64, _>("queue_seq")
                .or_else(|_| row.try_get::<i32, _>("queue_seq").map(i64::from))
                .map_err(|e| format!("oxpilot queued row: {e}"))?,
            content: row
                .try_get("content")
                .map_err(|e| format!("oxpilot queued row: {e}"))?,
            client_msg_id: row
                .try_get("client_msg_id")
                .map_err(|e| format!("oxpilot queued row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oxpilot queued row: {e}"))?,
        }
    }};
}

macro_rules! map_tool_call {
    ($row:expr) => {{
        let row = $row;
        OxpilotToolCallRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oxpilot tool-call row: {e}"))?,
            session_id: row
                .try_get("session_id")
                .map_err(|e| format!("oxpilot tool-call row: {e}"))?,
            message_id: row
                .try_get("message_id")
                .map_err(|e| format!("oxpilot tool-call row: {e}"))?,
            call_id: row
                .try_get("call_id")
                .map_err(|e| format!("oxpilot tool-call row: {e}"))?,
            seq: row
                .try_get::<i64, _>("seq")
                .or_else(|_| row.try_get::<i32, _>("seq").map(i64::from))
                .map_err(|e| format!("oxpilot tool-call row: {e}"))?,
            call_index: row
                .try_get::<i64, _>("call_index")
                .or_else(|_| row.try_get::<i32, _>("call_index").map(i64::from))
                .map_err(|e| format!("oxpilot tool-call row: {e}"))?,
            tool: row
                .try_get("tool")
                .map_err(|e| format!("oxpilot tool-call row: {e}"))?,
            args_summary: row
                .try_get("args_summary")
                .map_err(|e| format!("oxpilot tool-call row: {e}"))?,
            output_summary: row
                .try_get("output_summary")
                .map_err(|e| format!("oxpilot tool-call row: {e}"))?,
            status: row
                .try_get("status")
                .map_err(|e| format!("oxpilot tool-call row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oxpilot tool-call row: {e}"))?,
        }
    }};
}

macro_rules! map_proposal {
    ($row:expr) => {{
        let row = $row;
        OxpilotProposalRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oxpilot proposal row: {e}"))?,
            session_id: row
                .try_get("session_id")
                .map_err(|e| format!("oxpilot proposal row: {e}"))?,
            title: row
                .try_get("title")
                .map_err(|e| format!("oxpilot proposal row: {e}"))?,
            base: row
                .try_get("base")
                .map_err(|e| format!("oxpilot proposal row: {e}"))?,
            diff_body: row
                .try_get("diff_body")
                .map_err(|e| format!("oxpilot proposal row: {e}"))?,
            ops_json: row
                .try_get("ops_json")
                .map_err(|e| format!("oxpilot proposal row: {e}"))?,
            status: row
                .try_get("status")
                .map_err(|e| format!("oxpilot proposal row: {e}"))?,
            pr_ref: row
                .try_get("pr_ref")
                .map_err(|e| format!("oxpilot proposal row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oxpilot proposal row: {e}"))?,
        }
    }};
}

macro_rules! map_usage {
    ($row:expr) => {{
        let row = $row;
        OxpilotUsageCounterRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oxpilot usage row: {e}"))?,
            scope: row
                .try_get("scope")
                .map_err(|e| format!("oxpilot usage row: {e}"))?,
            scope_key: row
                .try_get("scope_key")
                .map_err(|e| format!("oxpilot usage row: {e}"))?,
            window: row
                .try_get("window")
                .map_err(|e| format!("oxpilot usage row: {e}"))?,
            requests: row
                .try_get::<i64, _>("requests")
                .or_else(|_| row.try_get::<i32, _>("requests").map(i64::from))
                .map_err(|e| format!("oxpilot usage row: {e}"))?,
            tokens: row
                .try_get::<i64, _>("tokens")
                .or_else(|_| row.try_get::<i32, _>("tokens").map(i64::from))
                .map_err(|e| format!("oxpilot usage row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("oxpilot usage row: {e}"))?,
        }
    }};
}

const QUEUED_SELECT_PG: &str = "SELECT q.id, q.session_id, q.queue_seq, q.content,
       q.client_msg_id,
       to_char(q.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM oxpilot_queued_messages q";

const QUEUED_SELECT_MYSQL: &str = "SELECT q.id, q.session_id, q.queue_seq, q.content,
       q.client_msg_id,
       DATE_FORMAT(q.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM oxpilot_queued_messages q";

const QUEUED_SELECT_SQLITE: &str = "SELECT q.id, q.session_id, q.queue_seq, q.content,
       q.client_msg_id,
       strftime('%Y-%m-%dT%H:%M:%SZ', q.created_at) AS created_at
FROM oxpilot_queued_messages q";

const TOOL_CALL_SELECT_PG: &str = "SELECT t.id, t.session_id, t.message_id, t.call_id,
       t.seq, t.call_index, t.tool, t.args_summary, t.output_summary, t.status,
       to_char(t.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM oxpilot_tool_calls t";

const TOOL_CALL_SELECT_MYSQL: &str = "SELECT t.id, t.session_id, t.message_id, t.call_id,
       t.seq, t.call_index, t.tool, t.args_summary, t.output_summary, t.status,
       DATE_FORMAT(t.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM oxpilot_tool_calls t";

const TOOL_CALL_SELECT_SQLITE: &str = "SELECT t.id, t.session_id, t.message_id, t.call_id,
       t.seq, t.call_index, t.tool, t.args_summary, t.output_summary, t.status,
       strftime('%Y-%m-%dT%H:%M:%SZ', t.created_at) AS created_at
FROM oxpilot_tool_calls t";

const PROPOSAL_SELECT_PG: &str = "SELECT p.id, p.session_id, p.title, p.base, p.diff_body,
       p.ops_json, p.status, p.pr_ref,
       to_char(p.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM oxpilot_proposals p";

const PROPOSAL_SELECT_MYSQL: &str = "SELECT p.id, p.session_id, p.title, p.base, p.diff_body,
       p.ops_json, p.status, p.pr_ref,
       DATE_FORMAT(p.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM oxpilot_proposals p";

const PROPOSAL_SELECT_SQLITE: &str = "SELECT p.id, p.session_id, p.title, p.base, p.diff_body,
       p.ops_json, p.status, p.pr_ref,
       strftime('%Y-%m-%dT%H:%M:%SZ', p.created_at) AS created_at
FROM oxpilot_proposals p";

const USAGE_SELECT_PG: &str = "SELECT u.id, u.scope, u.scope_key, u.window, u.requests, u.tokens,
       to_char(u.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM oxpilot_usage_counters u";

const USAGE_SELECT_MYSQL: &str =
    "SELECT u.id, u.scope, u.scope_key, u.window, u.requests, u.tokens,
       DATE_FORMAT(u.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM oxpilot_usage_counters u";

const USAGE_SELECT_SQLITE: &str =
    "SELECT u.id, u.scope, u.scope_key, u.window, u.requests, u.tokens,
       strftime('%Y-%m-%dT%H:%M:%SZ', u.updated_at) AS updated_at
FROM oxpilot_usage_counters u";

/// The C8 hook-then-cascade delete — one `DELETE` whose FK cascade removes
/// messages, tool_calls, queued rows, and proposals (BR5.3). Usage counters
/// carry no FK and survive by design. `user_id` rides the predicate as the
/// belt-and-braces second check even though the service pre-verifies
/// ownership. Returns rows deleted (0/1).
pub async fn delete_session_cascade(
    pool: &DbPool,
    session_id: &str,
    user_id: &str,
) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM oxpilot_sessions WHERE id = $1 AND user_id = $2")
                .bind(session_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete oxpilot session failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM oxpilot_sessions WHERE id = ? AND user_id = ?")
                .bind(session_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete oxpilot session failed: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM oxpilot_sessions WHERE id = ?1 AND user_id = ?2")
                .bind(session_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete oxpilot session failed: {e}"))?
                .rows_affected()
        }
    };
    Ok(n)
}

// ---------- follow-up queue (US1.8) ----------

/// Next free `queue_seq` for the session — `MAX(queue_seq)+1`, 1 when empty.
/// Callers insert under the retry bound; the unique index is the arbiter.
pub async fn next_queue_seq(pool: &DbPool, session_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(queue_seq) + 1, 1) FROM oxpilot_queued_messages WHERE session_id = $1",
        )
        .bind(session_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("next oxpilot queue_seq failed: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(queue_seq) + 1, 1) FROM oxpilot_queued_messages WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("next oxpilot queue_seq failed: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(queue_seq) + 1, 1) FROM oxpilot_queued_messages WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("next oxpilot queue_seq failed: {e}")),
    }
}

pub struct NewOxpilotQueued<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub queue_seq: i64,
    /// Already secrets-redacted by the service boundary.
    pub content: &'a str,
    pub client_msg_id: Option<&'a str>,
}

/// Insert a queued row — unique `(session_id, client_msg_id)` and
/// `(session_id, queue_seq)` violations surface to the caller so the
/// service can re-read (idempotent resend) or re-mint `queue_seq`.
pub async fn enqueue_queued(
    pool: &DbPool,
    q: &NewOxpilotQueued<'_>,
) -> Result<OxpilotQueuedRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_queued_messages
                     (id, session_id, queue_seq, content, client_msg_id)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(q.id)
            .bind(q.session_id)
            .bind(q.queue_seq)
            .bind(q.content)
            .bind(q.client_msg_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot queued message failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_queued_messages
                     (id, session_id, queue_seq, content, client_msg_id)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(q.id)
            .bind(q.session_id)
            .bind(q.queue_seq)
            .bind(q.content)
            .bind(q.client_msg_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot queued message failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_queued_messages
                     (id, session_id, queue_seq, content, client_msg_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(q.id)
            .bind(q.session_id)
            .bind(q.queue_seq)
            .bind(q.content)
            .bind(q.client_msg_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot queued message failed: {e}"))?;
        }
    }
    find_queued_by_id(pool, q.id)
        .await?
        .ok_or_else(|| "insert oxpilot queued message: row missing after insert".to_string())
}

pub async fn find_queued_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<OxpilotQueuedRow>, String> {
    let suffix = " WHERE q.id = {p}";
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{QUEUED_SELECT_PG}{}", suffix.replace("{p}", "$1"));
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot queued message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_queued!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{QUEUED_SELECT_MYSQL}{}", suffix.replace("{p}", "?"));
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot queued message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_queued!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{QUEUED_SELECT_SQLITE}{}", suffix.replace("{p}", "?1"));
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot queued message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_queued!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Queue-half of the BR2.2 union dedupe — the queued row holding
/// `client_msg_id` for this session, if any.
pub async fn find_queued_by_cmid(
    pool: &DbPool,
    session_id: &str,
    client_msg_id: &str,
) -> Result<Option<OxpilotQueuedRow>, String> {
    let suffix = " WHERE q.session_id = {p} AND q.client_msg_id = {q}";
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{QUEUED_SELECT_PG}{}",
                suffix.replace("{p}", "$1").replace("{q}", "$2")
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .bind(client_msg_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot queued message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_queued!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{QUEUED_SELECT_MYSQL}{}",
                suffix.replace("{p}", "?").replace("{q}", "?")
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .bind(client_msg_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot queued message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_queued!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{QUEUED_SELECT_SQLITE}{}",
                suffix.replace("{p}", "?1").replace("{q}", "?2")
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .bind(client_msg_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot queued message failed: {e}"))?
            {
                Some(r) => Ok(Some(map_queued!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// The pending queue in `queue_seq` order — `queue_list` + snapshot
/// `pending_queue` read (non-consuming).
pub async fn list_queued_for_session(
    pool: &DbPool,
    session_id: &str,
) -> Result<Vec<OxpilotQueuedRow>, String> {
    let suffix = " WHERE q.session_id = {p} ORDER BY q.queue_seq";
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{QUEUED_SELECT_PG}{}", suffix.replace("{p}", "$1"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot queued messages failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_queued!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!("{QUEUED_SELECT_MYSQL}{}", suffix.replace("{p}", "?"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot queued messages failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_queued!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{QUEUED_SELECT_SQLITE}{}", suffix.replace("{p}", "?1"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot queued messages failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_queued!(r));
            }
            Ok(out)
        }
    }
}

/// U8 (`queue_delete`) — drop ONE queued row scoped `(session_id, id)`.
/// Session ownership is resolved by the caller first, so the pair
/// filter alone can't reach across users. Returns rows affected
/// (0 ⇒ already absent, the proc's `not_found` arm).
pub async fn delete_queued(
    pool: &DbPool,
    session_id: &str,
    queued_id: &str,
) -> Result<u64, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM oxpilot_queued_messages WHERE session_id = $1 AND id = $2")
                .bind(session_id)
                .bind(queued_id)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
                .map_err(|e| format!("delete oxpilot queued message failed: {e}"))
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM oxpilot_queued_messages WHERE session_id = ? AND id = ?")
                .bind(session_id)
                .bind(queued_id)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
                .map_err(|e| format!("delete oxpilot queued message failed: {e}"))
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM oxpilot_queued_messages WHERE session_id = ?1 AND id = ?2")
                .bind(session_id)
                .bind(queued_id)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
                .map_err(|e| format!("delete oxpilot queued message failed: {e}"))
        }
    }
}

/// The atomic drain (BR4.2): claim+delete the session's queued rows in
/// `queue_seq` order and insert each as a `role='user'` `oxpilot_messages`
/// row minting `seq` inside the SAME transaction — a crash mid-drain rolls
/// back so nothing is lost, and a committed drain can't double-deliver.
/// `client_msg_id` is carried so a drained resend still dedupes. `mint_id`
/// allocates message ids (the service owns id minting). Returns the
/// appended transcript rows in delivery order.
///
/// Per dialect: Postgres/SQLite `DELETE…RETURNING` (order by `queue_seq`
/// in Rust — DELETE can't ORDER); MySQL `SELECT…FOR UPDATE ORDER BY` then
/// delete-by-ids, same transaction.
pub async fn drain_queue_tx(
    pool: &DbPool,
    session_id: &str,
    mut mint_id: impl FnMut() -> String,
) -> Result<Vec<OxpilotMessageRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot queue drain tx failed: {e}"))?;
            let rows = sqlx::query(
                "DELETE FROM oxpilot_queued_messages WHERE session_id = $1
                 RETURNING id, session_id, queue_seq, content, client_msg_id,
                     to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at",
            )
            .bind(session_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("drain oxpilot queue failed: {e}"))?;
            let mut queued: Vec<OxpilotQueuedRow> = Vec::with_capacity(rows.len());
            for r in &rows {
                queued.push(map_queued!(r));
            }
            queued.sort_by_key(|q| q.queue_seq);
            let mut out = Vec::with_capacity(queued.len());
            for q in &queued {
                let seq: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(MAX(seq) + 1, 1) FROM oxpilot_messages WHERE session_id = $1",
                )
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| format!("drain oxpilot queue seq mint failed: {e}"))?;
                let r = sqlx::query(
                    "INSERT INTO oxpilot_messages
                         (id, session_id, seq, role, content, client_msg_id, status)
                     VALUES ($1, $2, $3, 'user', $4, $5, 'complete')
                     RETURNING id, session_id, seq, role, content, client_msg_id, status,
                         to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at",
                )
                .bind(mint_id())
                .bind(session_id)
                .bind(seq)
                .bind(&q.content)
                .bind(&q.client_msg_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| format!("drain oxpilot queue message insert failed: {e}"))?;
                out.push(map_message!(&r));
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot queue drain failed: {e}"))?;
            Ok(out)
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot queue drain tx failed: {e}"))?;
            // Claim rows in order — SELECT…FOR UPDATE inside the tx holds
            // them until delete-by-ids, so a concurrent enqueue can't
            // interleave into the drain set.
            let rows = sqlx::query(
                "SELECT id, session_id, queue_seq, content, client_msg_id,
                        DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
                 FROM oxpilot_queued_messages
                 WHERE session_id = ? ORDER BY queue_seq FOR UPDATE",
            )
            .bind(session_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("drain oxpilot queue failed: {e}"))?;
            let mut queued: Vec<OxpilotQueuedRow> = Vec::with_capacity(rows.len());
            for r in &rows {
                queued.push(map_queued!(r));
            }
            for q in &queued {
                sqlx::query("DELETE FROM oxpilot_queued_messages WHERE id = ?")
                    .bind(&q.id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("drain oxpilot queue delete failed: {e}"))?;
            }
            let mut out = Vec::with_capacity(queued.len());
            for q in &queued {
                let seq: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(MAX(seq) + 1, 1) FROM oxpilot_messages WHERE session_id = ?",
                )
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| format!("drain oxpilot queue seq mint failed: {e}"))?;
                let mid = mint_id();
                sqlx::query(
                    "INSERT INTO oxpilot_messages
                         (id, session_id, seq, role, content, client_msg_id, status)
                     VALUES (?, ?, ?, 'user', ?, ?, 'complete')",
                )
                .bind(&mid)
                .bind(session_id)
                .bind(seq)
                .bind(&q.content)
                .bind(&q.client_msg_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("drain oxpilot queue message insert failed: {e}"))?;
                // No RETURNING on MySQL — re-read the row inside the tx.
                let r = sqlx::query(
                    "SELECT id, session_id, seq, role, content, client_msg_id, status,
                            DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
                     FROM oxpilot_messages WHERE id = ?",
                )
                .bind(&mid)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| format!("drain oxpilot queue message read failed: {e}"))?;
                out.push(map_message!(&r));
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot queue drain failed: {e}"))?;
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot queue drain tx failed: {e}"))?;
            let rows = sqlx::query(
                "DELETE FROM oxpilot_queued_messages WHERE session_id = ?1
                 RETURNING id, session_id, queue_seq, content, client_msg_id,
                     strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at",
            )
            .bind(session_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("drain oxpilot queue failed: {e}"))?;
            let mut queued: Vec<OxpilotQueuedRow> = Vec::with_capacity(rows.len());
            for r in &rows {
                queued.push(map_queued!(r));
            }
            queued.sort_by_key(|q| q.queue_seq);
            let mut out = Vec::with_capacity(queued.len());
            for q in &queued {
                let seq: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(MAX(seq) + 1, 1) FROM oxpilot_messages WHERE session_id = ?1",
                )
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| format!("drain oxpilot queue seq mint failed: {e}"))?;
                let r = sqlx::query(
                    "INSERT INTO oxpilot_messages
                         (id, session_id, seq, role, content, client_msg_id, status)
                     VALUES (?1, ?2, ?3, 'user', ?4, ?5, 'complete')
                     RETURNING id, session_id, seq, role, content, client_msg_id, status,
                         strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at",
                )
                .bind(mint_id())
                .bind(session_id)
                .bind(seq)
                .bind(&q.content)
                .bind(&q.client_msg_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| format!("drain oxpilot queue message insert failed: {e}"))?;
                out.push(map_message!(&r));
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot queue drain failed: {e}"))?;
            Ok(out)
        }
    }
}

// ---------- tool-call cards (BR7.3) ----------

pub struct NewOxpilotToolCall<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub message_id: &'a str,
    pub call_id: &'a str,
    /// Positional copy of the owning assistant message's `seq`.
    pub seq: i64,
    /// Intra-message ordinal (the snapshot's sort key).
    pub call_index: i64,
    pub tool: &'a str,
    /// Already secrets-redacted by the service boundary.
    pub args_summary: &'a str,
    pub status: &'a str,
}

/// Insert a tool-call card — `output_summary` starts empty (the running
/// call hasn't produced output); `set_tool_call_status` fills it on the
/// terminal transition.
pub async fn insert_tool_call(
    pool: &DbPool,
    t: &NewOxpilotToolCall<'_>,
) -> Result<OxpilotToolCallRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_tool_calls
                     (id, session_id, message_id, call_id, seq, call_index, tool,
                      args_summary, status)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(t.id)
            .bind(t.session_id)
            .bind(t.message_id)
            .bind(t.call_id)
            .bind(t.seq)
            .bind(t.call_index)
            .bind(t.tool)
            .bind(t.args_summary)
            .bind(t.status)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot tool call failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_tool_calls
                     (id, session_id, message_id, call_id, seq, call_index, tool,
                      args_summary, status)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(t.id)
            .bind(t.session_id)
            .bind(t.message_id)
            .bind(t.call_id)
            .bind(t.seq)
            .bind(t.call_index)
            .bind(t.tool)
            .bind(t.args_summary)
            .bind(t.status)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot tool call failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_tool_calls
                     (id, session_id, message_id, call_id, seq, call_index, tool,
                      args_summary, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(t.id)
            .bind(t.session_id)
            .bind(t.message_id)
            .bind(t.call_id)
            .bind(t.seq)
            .bind(t.call_index)
            .bind(t.tool)
            .bind(t.args_summary)
            .bind(t.status)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot tool call failed: {e}"))?;
        }
    }
    find_tool_call_by_id(pool, t.id)
        .await?
        .ok_or_else(|| "insert oxpilot tool call: row missing after insert".to_string())
}

pub async fn find_tool_call_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<OxpilotToolCallRow>, String> {
    let suffix = " WHERE t.id = {p}";
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{TOOL_CALL_SELECT_PG}{}", suffix.replace("{p}", "$1"));
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot tool call failed: {e}"))?
            {
                Some(r) => Ok(Some(map_tool_call!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{TOOL_CALL_SELECT_MYSQL}{}", suffix.replace("{p}", "?"));
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot tool call failed: {e}"))?
            {
                Some(r) => Ok(Some(map_tool_call!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{TOOL_CALL_SELECT_SQLITE}{}", suffix.replace("{p}", "?1"));
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot tool call failed: {e}"))?
            {
                Some(r) => Ok(Some(map_tool_call!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// The `(session_id, call_id)` lookup — existence checks for status
/// transitions (MySQL `rows_affected` counts *changed* rows, so a matched
/// row can't be distinguished from an absent one by the update count).
pub async fn find_tool_call_by_call_id(
    pool: &DbPool,
    session_id: &str,
    call_id: &str,
) -> Result<Option<OxpilotToolCallRow>, String> {
    let suffix = " WHERE t.session_id = {p1} AND t.call_id = {p2}";
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{TOOL_CALL_SELECT_PG}{}",
                suffix.replace("{p1}", "$1").replace("{p2}", "$2")
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .bind(call_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot tool call failed: {e}"))?
            {
                Some(r) => Ok(Some(map_tool_call!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{TOOL_CALL_SELECT_MYSQL}{}",
                suffix.replace("{p1}", "?").replace("{p2}", "?")
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .bind(call_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot tool call failed: {e}"))?
            {
                Some(r) => Ok(Some(map_tool_call!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{TOOL_CALL_SELECT_SQLITE}{}",
                suffix.replace("{p1}", "?1").replace("{p2}", "?2")
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .bind(call_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot tool call failed: {e}"))?
            {
                Some(r) => Ok(Some(map_tool_call!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Every card for the session in snapshot-merge order — `(seq, call_index)`
/// orders cards inside one message and across the transcript.
pub async fn list_tool_calls_for_session(
    pool: &DbPool,
    session_id: &str,
) -> Result<Vec<OxpilotToolCallRow>, String> {
    let suffix = " WHERE t.session_id = {p} ORDER BY t.seq, t.call_index";
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{TOOL_CALL_SELECT_PG}{}", suffix.replace("{p}", "$1"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot tool calls failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_tool_call!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!("{TOOL_CALL_SELECT_MYSQL}{}", suffix.replace("{p}", "?"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot tool calls failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_tool_call!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{TOOL_CALL_SELECT_SQLITE}{}", suffix.replace("{p}", "?1"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot tool calls failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_tool_call!(r));
            }
            Ok(out)
        }
    }
}

/// Terminal transition `running → done|error`; `output_summary` writes when
/// provided (already redacted at the service boundary), else the prior
/// value stands. Returns rows updated — 0 means the `call_id` is unknown
/// on this session (`not_found` at the service layer).
pub async fn set_tool_call_status(
    pool: &DbPool,
    session_id: &str,
    call_id: &str,
    status: &str,
    output_summary: Option<&str>,
) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE oxpilot_tool_calls
             SET status = $1, output_summary = COALESCE($2, output_summary)
             WHERE session_id = $3 AND call_id = $4",
        )
        .bind(status)
        .bind(output_summary)
        .bind(session_id)
        .bind(call_id)
        .execute(p)
        .await
        .map_err(|e| format!("update oxpilot tool call failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE oxpilot_tool_calls
             SET status = ?, output_summary = COALESCE(?, output_summary)
             WHERE session_id = ? AND call_id = ?",
        )
        .bind(status)
        .bind(output_summary)
        .bind(session_id)
        .bind(call_id)
        .execute(p)
        .await
        .map_err(|e| format!("update oxpilot tool call failed: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE oxpilot_tool_calls
             SET status = ?1, output_summary = COALESCE(?2, output_summary)
             WHERE session_id = ?3 AND call_id = ?4",
        )
        .bind(status)
        .bind(output_summary)
        .bind(session_id)
        .bind(call_id)
        .execute(p)
        .await
        .map_err(|e| format!("update oxpilot tool call failed: {e}"))?
        .rows_affected(),
    };
    Ok(n)
}

// ---------- proposals (WF8) ----------

pub struct NewOxpilotProposal<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    /// Already secrets-redacted by the service boundary.
    pub title: &'a str,
    /// Base ref/sha — an identifier, never redacted.
    pub base: &'a str,
    /// Already secrets-redacted by the service boundary.
    pub diff_body: &'a str,
    /// Serialized `DiffFileOp[]` — validated op-set only, never redacted
    /// (paths are identifiers; content rides base64 verbatim).
    pub ops_json: &'a str,
}

/// Insert a `pending` proposal.
pub async fn insert_proposal(
    pool: &DbPool,
    p: &NewOxpilotProposal<'_>,
) -> Result<OxpilotProposalRow, String> {
    match pool {
        DbPool::Postgres(pp) => {
            sqlx::query(
                "INSERT INTO oxpilot_proposals
                     (id, session_id, title, base, diff_body, ops_json)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(p.id)
            .bind(p.session_id)
            .bind(p.title)
            .bind(p.base)
            .bind(p.diff_body)
            .bind(p.ops_json)
            .execute(pp)
            .await
            .map_err(|e| format!("insert oxpilot proposal failed: {e}"))?;
        }
        DbPool::MySql(pp) => {
            sqlx::query(
                "INSERT INTO oxpilot_proposals
                     (id, session_id, title, base, diff_body, ops_json)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(p.id)
            .bind(p.session_id)
            .bind(p.title)
            .bind(p.base)
            .bind(p.diff_body)
            .bind(p.ops_json)
            .execute(pp)
            .await
            .map_err(|e| format!("insert oxpilot proposal failed: {e}"))?;
        }
        DbPool::Sqlite(pp) => {
            sqlx::query(
                "INSERT INTO oxpilot_proposals
                     (id, session_id, title, base, diff_body, ops_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(p.id)
            .bind(p.session_id)
            .bind(p.title)
            .bind(p.base)
            .bind(p.diff_body)
            .bind(p.ops_json)
            .execute(pp)
            .await
            .map_err(|e| format!("insert oxpilot proposal failed: {e}"))?;
        }
    }
    find_proposal(pool, p.id)
        .await?
        .ok_or_else(|| "insert oxpilot proposal: row missing after insert".to_string())
}

/// Id-keyed read — ownership is validated by the service against the
/// returned row's `session_id` (fetch-then-check).
pub async fn find_proposal(pool: &DbPool, id: &str) -> Result<Option<OxpilotProposalRow>, String> {
    let suffix = " WHERE p.id = {p}";
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{PROPOSAL_SELECT_PG}{}", suffix.replace("{p}", "$1"));
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot proposal failed: {e}"))?
            {
                Some(r) => Ok(Some(map_proposal!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{PROPOSAL_SELECT_MYSQL}{}", suffix.replace("{p}", "?"));
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot proposal failed: {e}"))?
            {
                Some(r) => Ok(Some(map_proposal!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{PROPOSAL_SELECT_SQLITE}{}", suffix.replace("{p}", "?1"));
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot proposal failed: {e}"))?
            {
                Some(r) => Ok(Some(map_proposal!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Session-scoped list, newest-first — `diff_list` + the ArtifactScope
/// rebuild both read through this (ownership is validated upstream by the
/// session row's fetch-then-check, never here).
pub async fn find_proposals_by_session(
    pool: &DbPool,
    session_id: &str,
) -> Result<Vec<OxpilotProposalRow>, String> {
    let suffix = " WHERE p.session_id = {p} ORDER BY p.created_at DESC, p.id DESC";
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{PROPOSAL_SELECT_PG}{}", suffix.replace("{p}", "$1"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot proposals failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_proposal!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!("{PROPOSAL_SELECT_MYSQL}{}", suffix.replace("{p}", "?"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot proposals failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_proposal!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{PROPOSAL_SELECT_SQLITE}{}", suffix.replace("{p}", "?1"));
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(session_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot proposals failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_proposal!(r));
            }
            Ok(out)
        }
    }
}

/// `pending → applied` with the materialized pull reference — returns rows
/// updated (the service resolves `not_found` from the fetch-then-check).
pub async fn mark_proposal_applied(pool: &DbPool, id: &str, pr_ref: &str) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE oxpilot_proposals SET status = 'applied', pr_ref = $1 WHERE id = $2",
        )
        .bind(pr_ref)
        .bind(id)
        .execute(p)
        .await
        .map_err(|e| format!("apply oxpilot proposal failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => {
            sqlx::query("UPDATE oxpilot_proposals SET status = 'applied', pr_ref = ? WHERE id = ?")
                .bind(pr_ref)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("apply oxpilot proposal failed: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE oxpilot_proposals SET status = 'applied', pr_ref = ?1 WHERE id = ?2",
        )
        .bind(pr_ref)
        .bind(id)
        .execute(p)
        .await
        .map_err(|e| format!("apply oxpilot proposal failed: {e}"))?
        .rows_affected(),
    };
    Ok(n)
}

// ---------- usage counters (US5.2) ----------

/// The `__instance__` scope key — the instance-wide rollup shares the row
/// shape with per-user rows; a literal sentinel key keeps it addressable.
pub const INSTANCE_SCOPE_KEY: &str = "__instance__";

/// One UTC-day rollup, both scopes, ONE transaction (BR6.1): `(user,
/// user_id)` + `(instance, '__instance__')` upsert-increment by the same
/// delta. `id` derives deterministically from the unique key — the row's
/// identity IS the key, so a deterministic pk can never collide. The
/// service computes `window` (UTC `YYYY-MM-DD`) — this module owns no clock.
pub async fn upsert_usage(
    pool: &DbPool,
    user_id: &str,
    window: &str,
    requests: i64,
    tokens: i64,
) -> Result<(), String> {
    let keys = [("user", user_id), ("instance", INSTANCE_SCOPE_KEY)];
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot usage upsert tx failed: {e}"))?;
            for (scope, scope_key) in keys {
                let id = format!("{scope}|{scope_key}|{window}");
                sqlx::query(
                    "INSERT INTO oxpilot_usage_counters
                         (id, scope, scope_key, window, requests, tokens)
                     VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT (scope, scope_key, window) DO UPDATE SET
                       requests = oxpilot_usage_counters.requests + EXCLUDED.requests,
                       tokens = oxpilot_usage_counters.tokens + EXCLUDED.tokens,
                       updated_at = now()",
                )
                .bind(&id)
                .bind(scope)
                .bind(scope_key)
                .bind(window)
                .bind(requests)
                .bind(tokens)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("upsert oxpilot usage failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot usage upsert failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot usage upsert tx failed: {e}"))?;
            for (scope, scope_key) in keys {
                let id = format!("{scope}|{scope_key}|{window}");
                sqlx::query(
                    "INSERT INTO oxpilot_usage_counters
                         (id, scope, scope_key, window, requests, tokens)
                     VALUES (?, ?, ?, ?, ?, ?)
                     ON DUPLICATE KEY UPDATE
                       requests = requests + VALUES(requests),
                       tokens = tokens + VALUES(tokens),
                       updated_at = CURRENT_TIMESTAMP",
                )
                .bind(&id)
                .bind(scope)
                .bind(scope_key)
                .bind(window)
                .bind(requests)
                .bind(tokens)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("upsert oxpilot usage failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot usage upsert failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot usage upsert tx failed: {e}"))?;
            for (scope, scope_key) in keys {
                let id = format!("{scope}|{scope_key}|{window}");
                sqlx::query(
                    "INSERT INTO oxpilot_usage_counters
                         (id, scope, scope_key, window, requests, tokens)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT (scope, scope_key, window) DO UPDATE SET
                       requests = oxpilot_usage_counters.requests + excluded.requests,
                       tokens = oxpilot_usage_counters.tokens + excluded.tokens,
                       updated_at = strftime('%Y-%m-%d %H:%M:%S','now')",
                )
                .bind(&id)
                .bind(scope)
                .bind(scope_key)
                .bind(window)
                .bind(requests)
                .bind(tokens)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("upsert oxpilot usage failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot usage upsert failed: {e}"))?;
        }
    }
    Ok(())
}

/// Today's `(user, user_id)` and `(instance, '__instance__')` rows —
/// `(None, None)`-able, absent reads as zero (BR6.3 posture).
pub async fn read_counters(
    pool: &DbPool,
    user_id: &str,
    window: &str,
) -> Result<
    (
        Option<OxpilotUsageCounterRow>,
        Option<OxpilotUsageCounterRow>,
    ),
    String,
> {
    let suffix = " WHERE u.window = {w} AND u.scope = {s} AND u.scope_key = {k}";
    async fn fetch(
        pool: &DbPool,
        suffix: &str,
        window: &str,
        scope: &str,
        scope_key: &str,
    ) -> Result<Option<OxpilotUsageCounterRow>, String> {
        match pool {
            DbPool::Postgres(p) => {
                let sql = format!(
                    "{USAGE_SELECT_PG}{}",
                    suffix
                        .replace("{w}", "$1")
                        .replace("{s}", "$2")
                        .replace("{k}", "$3")
                );
                match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                    .bind(window)
                    .bind(scope)
                    .bind(scope_key)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| format!("read oxpilot usage failed: {e}"))?
                {
                    Some(r) => Ok(Some(map_usage!(&r))),
                    None => Ok(None),
                }
            }
            DbPool::MySql(p) => {
                let sql = format!(
                    "{USAGE_SELECT_MYSQL}{}",
                    suffix
                        .replace("{w}", "?")
                        .replace("{s}", "?")
                        .replace("{k}", "?")
                );
                match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                    .bind(window)
                    .bind(scope)
                    .bind(scope_key)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| format!("read oxpilot usage failed: {e}"))?
                {
                    Some(r) => Ok(Some(map_usage!(&r))),
                    None => Ok(None),
                }
            }
            DbPool::Sqlite(p) => {
                let sql = format!(
                    "{USAGE_SELECT_SQLITE}{}",
                    suffix
                        .replace("{w}", "?1")
                        .replace("{s}", "?2")
                        .replace("{k}", "?3")
                );
                match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                    .bind(window)
                    .bind(scope)
                    .bind(scope_key)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| format!("read oxpilot usage failed: {e}"))?
                {
                    Some(r) => Ok(Some(map_usage!(&r))),
                    None => Ok(None),
                }
            }
        }
    }
    let user = fetch(pool, suffix, window, "user", user_id).await?;
    let instance = fetch(pool, suffix, window, "instance", INSTANCE_SCOPE_KEY).await?;
    Ok((user, instance))
}

// ---------- connector gateway (0088, U6) ----------
//
// The connector-gateway store: pairing mint/redeem, the connector-owned
// target rows, consent grants, and the durable action queue. All
// timestamp BINDS arrive as RFC3339 `YYYY-MM-DDTHH:MM:SSZ` strings and
// convert per dialect (Postgres `::timestamptz`, MySQL `STR_TO_DATE`,
// SQLite raw TEXT) exactly like `sessions.rs` — the caller computes
// `now`/expiry so state-conditional predicates and stamps always share
// one clock. Reads reformat every timestamp to RFC3339 so impl-side
// lexicographic comparisons (`expires_at > now`, liveness windows) are
// format-stable on all three dialects.

#[derive(Debug, Clone)]
pub struct OxpilotPairingRow {
    pub id: String,
    pub user_id: String,
    /// SHA-256 hex of the shown-once code — the only persisted form.
    pub code_hash: String,
    pub target_name: String,
    /// Stored JSON capability array the owner minted.
    pub requested_caps: String,
    /// `pending | claimed | expired`.
    pub state: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct OxpilotTargetRow {
    pub id: String,
    pub user_id: String,
    pub pairing_id: String,
    pub name: String,
    /// Stored JSON capability array granted at redeem.
    pub caps: String,
    /// `pending_consent | connected | disconnected | revoked`.
    pub status: String,
    /// SHA-256 hex of the connector Bearer verifier — NULL after
    /// disconnect tombstone; retained on revoke (heartbeat still
    /// authenticates so the connector learns `revoked:true`).
    pub verifier_hash: Option<String>,
    pub last_seen_at: Option<String>,
    pub created_at: String,
    pub disconnected_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OxpilotGrantRow {
    pub id: String,
    pub target_id: String,
    pub user_id: String,
    /// NULL ⇒ connect grant (survives session delete, BR6.6); a set value
    /// FK-cascades the grant with its session.
    pub session_id: Option<String>,
    /// `connect | action | action_once | action_session` — pending action
    /// grants carry `action` until respond stamps the flavor.
    pub kind: String,
    /// The action kind a pending/allowed action grant covers
    /// (`exec_shell|fs_read|fs_write`); NULL on connect grants.
    pub action_kind: Option<String>,
    /// Bounded, already-redacted proposed args snapshot (consent card).
    pub action_args: Option<String>,
    pub action_summary: Option<String>,
    /// `pending | allowed | denied | expired | cancelled | revoked`.
    pub state: String,
    pub expires_at: String,
    pub resolved_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct OxpilotActionRow {
    pub id: String,
    pub target_id: String,
    pub user_id: String,
    pub session_id: String,
    pub grant_id: String,
    pub kind: String,
    /// Bounded, already-redacted args the connector executes.
    pub args: String,
    pub args_summary: String,
    /// `queued | claimed | running | done | failed | cancel_requested | cancelled`.
    pub state: String,
    pub result_summary: Option<String>,
    pub created_at: String,
    pub claimed_at: Option<String>,
    pub finished_at: Option<String>,
}

/// Dequeue candidate — the action row plus its covering grant's
/// `state`/`expires_at` so the per-row liveness re-check needs no extra
/// query inside the per-target lock (BR5.1).
#[derive(Debug, Clone)]
pub struct OxpilotQueuedCandidate {
    pub action: OxpilotActionRow,
    pub grant_state: String,
    pub grant_expires_at: String,
}

/// Lifecycle-kill outcome — the rows a disconnect (all `cancelled`) or
/// revoke (`revoked` grants + `cancelled` queued + `cancel_requested`
/// in-flight) terminalled, for post-commit per-row audit (BR7.2).
#[derive(Debug, Clone, Default)]
pub struct OxpilotTargetLifecycleOutcome {
    pub grants: Vec<OxpilotGrantRow>,
    pub cancelled_actions: Vec<OxpilotActionRow>,
    pub cancel_requested_actions: Vec<OxpilotActionRow>,
}

/// BR6.4 session-grant revoke outcome — the revoked grant plus the
/// action rows it moved (`queued→cancelled`, `claimed|running→
/// cancel_requested`).
#[derive(Debug, Clone)]
pub struct OxpilotSessionGrantRevokeOutcome {
    pub grant: OxpilotGrantRow,
    pub cancelled_actions: Vec<OxpilotActionRow>,
    pub cancel_requested_actions: Vec<OxpilotActionRow>,
}

pub struct NewOxpilotPairing<'a> {
    pub id: &'a str,
    pub user_id: &'a str,
    pub code_hash: &'a str,
    pub target_name: &'a str,
    /// Stored JSON array of the minted capability set.
    pub requested_caps: &'a str,
    pub expires_at: &'a str,
}

pub struct NewOxpilotTarget<'a> {
    pub id: &'a str,
    pub user_id: &'a str,
    pub pairing_id: &'a str,
    pub name: &'a str,
    /// Stored JSON capability array.
    pub caps: &'a str,
    pub verifier_hash: &'a str,
}

pub struct NewOxpilotGrant<'a> {
    pub id: &'a str,
    pub target_id: &'a str,
    pub user_id: &'a str,
    /// `None` ⇒ connect grant.
    pub session_id: Option<&'a str>,
    pub kind: &'a str,
    pub action_kind: Option<&'a str>,
    /// Already-redacted, bounded args snapshot.
    pub action_args: Option<&'a str>,
    pub action_summary: Option<&'a str>,
    pub expires_at: &'a str,
}

pub struct NewOxpilotAction<'a> {
    pub id: &'a str,
    pub target_id: &'a str,
    pub user_id: &'a str,
    pub session_id: &'a str,
    pub grant_id: &'a str,
    pub kind: &'a str,
    /// Already-redacted, bounded args the connector executes.
    pub args: &'a str,
    pub args_summary: &'a str,
}

macro_rules! map_pairing {
    ($row:expr) => {{
        let row = $row;
        OxpilotPairingRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oxpilot pairing row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("oxpilot pairing row: {e}"))?,
            code_hash: row
                .try_get("code_hash")
                .map_err(|e| format!("oxpilot pairing row: {e}"))?,
            target_name: row
                .try_get("target_name")
                .map_err(|e| format!("oxpilot pairing row: {e}"))?,
            requested_caps: row
                .try_get("requested_caps")
                .map_err(|e| format!("oxpilot pairing row: {e}"))?,
            state: row
                .try_get("state")
                .map_err(|e| format!("oxpilot pairing row: {e}"))?,
            expires_at: row
                .try_get("expires_at")
                .map_err(|e| format!("oxpilot pairing row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oxpilot pairing row: {e}"))?,
        }
    }};
}

macro_rules! map_target {
    ($row:expr) => {{
        let row = $row;
        OxpilotTargetRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oxpilot target row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("oxpilot target row: {e}"))?,
            pairing_id: row
                .try_get("pairing_id")
                .map_err(|e| format!("oxpilot target row: {e}"))?,
            name: row
                .try_get("name")
                .map_err(|e| format!("oxpilot target row: {e}"))?,
            caps: row
                .try_get("caps")
                .map_err(|e| format!("oxpilot target row: {e}"))?,
            status: row
                .try_get("status")
                .map_err(|e| format!("oxpilot target row: {e}"))?,
            verifier_hash: row
                .try_get("verifier_hash")
                .map_err(|e| format!("oxpilot target row: {e}"))?,
            last_seen_at: row
                .try_get("last_seen_at")
                .map_err(|e| format!("oxpilot target row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oxpilot target row: {e}"))?,
            disconnected_at: row
                .try_get("disconnected_at")
                .map_err(|e| format!("oxpilot target row: {e}"))?,
            revoked_at: row
                .try_get("revoked_at")
                .map_err(|e| format!("oxpilot target row: {e}"))?,
        }
    }};
}

macro_rules! map_grant {
    ($row:expr) => {{
        let row = $row;
        OxpilotGrantRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
            target_id: row
                .try_get("target_id")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
            session_id: row
                .try_get("session_id")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
            kind: row
                .try_get("kind")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
            action_kind: row
                .try_get("action_kind")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
            action_args: row
                .try_get("action_args")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
            action_summary: row
                .try_get("action_summary")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
            state: row
                .try_get("state")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
            expires_at: row
                .try_get("expires_at")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
            resolved_at: row
                .try_get("resolved_at")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oxpilot grant row: {e}"))?,
        }
    }};
}

macro_rules! map_action {
    ($row:expr) => {{
        let row = $row;
        OxpilotActionRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            target_id: row
                .try_get("target_id")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            session_id: row
                .try_get("session_id")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            grant_id: row
                .try_get("grant_id")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            kind: row
                .try_get("kind")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            args: row
                .try_get("args")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            args_summary: row
                .try_get("args_summary")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            state: row
                .try_get("state")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            result_summary: row
                .try_get("result_summary")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            claimed_at: row
                .try_get("claimed_at")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
            finished_at: row
                .try_get("finished_at")
                .map_err(|e| format!("oxpilot action row: {e}"))?,
        }
    }};
}

const PAIRING_COLS_PG: &str = "id, user_id, code_hash, target_name, requested_caps, state,
    to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at,
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at";
const PAIRING_COLS_MYSQL: &str = "id, user_id, code_hash, target_name, requested_caps, state,
    DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') AS expires_at,
    DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at";
const PAIRING_COLS_SQLITE: &str = "id, user_id, code_hash, target_name, requested_caps, state,
    strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) AS expires_at,
    strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at";

const TARGET_COLS_PG: &str = "id, user_id, pairing_id, name, caps, status, verifier_hash,
    to_char(last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_seen_at,
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
    to_char(disconnected_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS disconnected_at,
    to_char(revoked_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS revoked_at";
const TARGET_COLS_MYSQL: &str = "id, user_id, pairing_id, name, caps, status, verifier_hash,
    DATE_FORMAT(last_seen_at, '%Y-%m-%dT%H:%i:%sZ') AS last_seen_at,
    DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
    DATE_FORMAT(disconnected_at, '%Y-%m-%dT%H:%i:%sZ') AS disconnected_at,
    DATE_FORMAT(revoked_at, '%Y-%m-%dT%H:%i:%sZ') AS revoked_at";
const TARGET_COLS_SQLITE: &str = "id, user_id, pairing_id, name, caps, status, verifier_hash,
    strftime('%Y-%m-%dT%H:%M:%SZ', last_seen_at) AS last_seen_at,
    strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
    strftime('%Y-%m-%dT%H:%M:%SZ', disconnected_at) AS disconnected_at,
    strftime('%Y-%m-%dT%H:%M:%SZ', revoked_at) AS revoked_at";

const GRANT_COLS_PG: &str = "id, target_id, user_id, session_id, kind, action_kind, action_args,
    action_summary, state,
    to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at,
    to_char(resolved_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS resolved_at,
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at";
const GRANT_COLS_MYSQL: &str = "id, target_id, user_id, session_id, kind, action_kind, action_args,
    action_summary, state,
    DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') AS expires_at,
    DATE_FORMAT(resolved_at, '%Y-%m-%dT%H:%i:%sZ') AS resolved_at,
    DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at";
const GRANT_COLS_SQLITE: &str =
    "id, target_id, user_id, session_id, kind, action_kind, action_args,
    action_summary, state,
    strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) AS expires_at,
    strftime('%Y-%m-%dT%H:%M:%SZ', resolved_at) AS resolved_at,
    strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at";

const ACTION_COLS_PG: &str =
    "id, target_id, user_id, session_id, grant_id, kind, args, args_summary,
    state, result_summary,
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
    to_char(claimed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS claimed_at,
    to_char(finished_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS finished_at";
const ACTION_COLS_MYSQL: &str =
    "id, target_id, user_id, session_id, grant_id, kind, args, args_summary,
    state, result_summary,
    DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
    DATE_FORMAT(claimed_at, '%Y-%m-%dT%H:%i:%sZ') AS claimed_at,
    DATE_FORMAT(finished_at, '%Y-%m-%dT%H:%i:%sZ') AS finished_at";
const ACTION_COLS_SQLITE: &str =
    "id, target_id, user_id, session_id, grant_id, kind, args, args_summary,
    state, result_summary,
    strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
    strftime('%Y-%m-%dT%H:%M:%SZ', claimed_at) AS claimed_at,
    strftime('%Y-%m-%dT%H:%M:%SZ', finished_at) AS finished_at";

// `a.`-prefixed variants for the dequeue JOIN — output aliases match
// ACTION_COLS_* so `map_action!` reads either shape.
const ACTION_COLS_PG_PREFIXED: &str =
    "a.id, a.target_id, a.user_id, a.session_id, a.grant_id, a.kind, a.args, a.args_summary,
    a.state, a.result_summary,
    to_char(a.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
    to_char(a.claimed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS claimed_at,
    to_char(a.finished_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS finished_at";
const ACTION_COLS_MYSQL_PREFIXED: &str =
    "a.id, a.target_id, a.user_id, a.session_id, a.grant_id, a.kind, a.args, a.args_summary,
    a.state, a.result_summary,
    DATE_FORMAT(a.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
    DATE_FORMAT(a.claimed_at, '%Y-%m-%dT%H:%i:%sZ') AS claimed_at,
    DATE_FORMAT(a.finished_at, '%Y-%m-%dT%H:%i:%sZ') AS finished_at";
const ACTION_COLS_SQLITE_PREFIXED: &str =
    "a.id, a.target_id, a.user_id, a.session_id, a.grant_id, a.kind, a.args, a.args_summary,
    a.state, a.result_summary,
    strftime('%Y-%m-%dT%H:%M:%SZ', a.created_at) AS created_at,
    strftime('%Y-%m-%dT%H:%M:%SZ', a.claimed_at) AS claimed_at,
    strftime('%Y-%m-%dT%H:%M:%SZ', a.finished_at) AS finished_at";

// -- pairings --

/// Insert a minted pairing (state `pending`). Re-reads by PK so the
/// returned row carries the server `created_at` — identical on all
/// dialects without RETURNING divergence.
pub async fn insert_pairing(
    pool: &DbPool,
    p: &NewOxpilotPairing<'_>,
) -> Result<OxpilotPairingRow, String> {
    match pool {
        DbPool::Postgres(pg) => {
            sqlx::query(
                "INSERT INTO oxpilot_pairings
                     (id, user_id, code_hash, target_name, requested_caps, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6::timestamptz)",
            )
            .bind(p.id)
            .bind(p.user_id)
            .bind(p.code_hash)
            .bind(p.target_name)
            .bind(p.requested_caps)
            .bind(p.expires_at)
            .execute(pg)
            .await
            .map_err(|e| format!("insert oxpilot pairing failed: {e}"))?;
        }
        DbPool::MySql(pg) => {
            sqlx::query(
                "INSERT INTO oxpilot_pairings
                     (id, user_id, code_hash, target_name, requested_caps, expires_at)
                 VALUES (?, ?, ?, ?, ?, STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'))",
            )
            .bind(p.id)
            .bind(p.user_id)
            .bind(p.code_hash)
            .bind(p.target_name)
            .bind(p.requested_caps)
            .bind(p.expires_at)
            .execute(pg)
            .await
            .map_err(|e| format!("insert oxpilot pairing failed: {e}"))?;
        }
        DbPool::Sqlite(pg) => {
            sqlx::query(
                "INSERT INTO oxpilot_pairings
                     (id, user_id, code_hash, target_name, requested_caps, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(p.id)
            .bind(p.user_id)
            .bind(p.code_hash)
            .bind(p.target_name)
            .bind(p.requested_caps)
            .bind(p.expires_at)
            .execute(pg)
            .await
            .map_err(|e| format!("insert oxpilot pairing failed: {e}"))?;
        }
    }
    match find_pairing_by_id(pool, p.id).await? {
        Some(row) => Ok(row),
        None => Err("inserted oxpilot pairing not found".to_string()),
    }
}

async fn find_pairing_by_id(pool: &DbPool, id: &str) -> Result<Option<OxpilotPairingRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("SELECT {PAIRING_COLS_PG} FROM oxpilot_pairings WHERE id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot pairing failed: {e}"))?
            {
                Some(r) => Ok(Some(map_pairing!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("SELECT {PAIRING_COLS_MYSQL} FROM oxpilot_pairings WHERE id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot pairing failed: {e}"))?
            {
                Some(r) => Ok(Some(map_pairing!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("SELECT {PAIRING_COLS_SQLITE} FROM oxpilot_pairings WHERE id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot pairing failed: {e}"))?
            {
                Some(r) => Ok(Some(map_pairing!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// The redeem-time lookup — `code_hash` is SHA-256 hex, never the code.
pub async fn find_pairing_by_code_hash(
    pool: &DbPool,
    code_hash: &str,
) -> Result<Option<OxpilotPairingRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql =
                format!("SELECT {PAIRING_COLS_PG} FROM oxpilot_pairings WHERE code_hash = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(code_hash)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot pairing by hash failed: {e}"))?
            {
                Some(r) => Ok(Some(map_pairing!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql =
                format!("SELECT {PAIRING_COLS_MYSQL} FROM oxpilot_pairings WHERE code_hash = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(code_hash)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot pairing by hash failed: {e}"))?
            {
                Some(r) => Ok(Some(map_pairing!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql =
                format!("SELECT {PAIRING_COLS_SQLITE} FROM oxpilot_pairings WHERE code_hash = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(code_hash)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot pairing by hash failed: {e}"))?
            {
                Some(r) => Ok(Some(map_pairing!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Lazy `pending → expired` stamp — returns rows updated (1 = stamped;
/// 0 = already resolved/missing).
pub async fn expire_pairing(pool: &DbPool, id: &str) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE oxpilot_pairings SET state = 'expired' WHERE id = $1 AND state = 'pending'",
        )
        .bind(id)
        .execute(p)
        .await
        .map_err(|e| format!("expire oxpilot pairing failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE oxpilot_pairings SET state = 'expired' WHERE id = ? AND state = 'pending'",
        )
        .bind(id)
        .execute(p)
        .await
        .map_err(|e| format!("expire oxpilot pairing failed: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE oxpilot_pairings SET state = 'expired' WHERE id = ?1 AND state = 'pending'",
        )
        .bind(id)
        .execute(p)
        .await
        .map_err(|e| format!("expire oxpilot pairing failed: {e}"))?
        .rows_affected(),
    };
    Ok(n)
}

/// Atomic redeem (BR1.2): one transaction CASes the pairing
/// `pending → claimed` gated on `expires_at > now`, then inserts the
/// `pending_consent` target (verifier material minted by the caller) and
/// the first `pending` connect grant. `Ok(None)` means the CAS lost —
/// the code was expired, already claimed, or never existed.
pub async fn claim_pairing_tx(
    pool: &DbPool,
    code_hash: &str,
    now: &str,
    target: &NewOxpilotTarget<'_>,
    grant: &NewOxpilotGrant<'_>,
) -> Result<Option<OxpilotTargetRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot pairing claim tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_pairings SET state = 'claimed'
                 WHERE code_hash = $1 AND state = 'pending' AND expires_at > $2::timestamptz",
            )
            .bind(code_hash)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("claim oxpilot pairing failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            sqlx::query(
                "INSERT INTO oxpilot_targets
                     (id, user_id, pairing_id, name, caps, verifier_hash)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(target.id)
            .bind(target.user_id)
            .bind(target.pairing_id)
            .bind(target.name)
            .bind(target.caps)
            .bind(target.verifier_hash)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert oxpilot target failed: {e}"))?;
            sqlx::query(
                "INSERT INTO oxpilot_grants
                     (id, target_id, user_id, session_id, kind, action_kind, action_args, action_summary, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::timestamptz)",
            )
            .bind(grant.id)
            .bind(grant.target_id)
            .bind(grant.user_id)
            .bind(grant.session_id)
            .bind(grant.kind)
            .bind(grant.action_kind)
            .bind(grant.action_args)
            .bind(grant.action_summary)
            .bind(grant.expires_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert oxpilot connect grant failed: {e}"))?;
            let row = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {TARGET_COLS_PG} FROM oxpilot_targets WHERE id = $1"
            )))
            .bind(target.id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("read oxpilot target failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot pairing claim failed: {e}"))?;
            Ok(Some(map_target!(&row)))
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot pairing claim tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_pairings SET state = 'claimed'
                 WHERE code_hash = ? AND state = 'pending' AND expires_at > STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')",
            )
            .bind(code_hash)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("claim oxpilot pairing failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            sqlx::query(
                "INSERT INTO oxpilot_targets
                     (id, user_id, pairing_id, name, caps, verifier_hash)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(target.id)
            .bind(target.user_id)
            .bind(target.pairing_id)
            .bind(target.name)
            .bind(target.caps)
            .bind(target.verifier_hash)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert oxpilot target failed: {e}"))?;
            sqlx::query(
                "INSERT INTO oxpilot_grants
                     (id, target_id, user_id, session_id, kind, action_kind, action_args, action_summary, expires_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'))",
            )
            .bind(grant.id)
            .bind(grant.target_id)
            .bind(grant.user_id)
            .bind(grant.session_id)
            .bind(grant.kind)
            .bind(grant.action_kind)
            .bind(grant.action_args)
            .bind(grant.action_summary)
            .bind(grant.expires_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert oxpilot connect grant failed: {e}"))?;
            let row = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {TARGET_COLS_MYSQL} FROM oxpilot_targets WHERE id = ?"
            )))
            .bind(target.id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("read oxpilot target failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot pairing claim failed: {e}"))?;
            Ok(Some(map_target!(&row)))
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot pairing claim tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_pairings SET state = 'claimed'
                 WHERE code_hash = ?1 AND state = 'pending' AND expires_at > ?2",
            )
            .bind(code_hash)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("claim oxpilot pairing failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            sqlx::query(
                "INSERT INTO oxpilot_targets
                     (id, user_id, pairing_id, name, caps, verifier_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(target.id)
            .bind(target.user_id)
            .bind(target.pairing_id)
            .bind(target.name)
            .bind(target.caps)
            .bind(target.verifier_hash)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert oxpilot target failed: {e}"))?;
            sqlx::query(
                "INSERT INTO oxpilot_grants
                     (id, target_id, user_id, session_id, kind, action_kind, action_args, action_summary, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(grant.id)
            .bind(grant.target_id)
            .bind(grant.user_id)
            .bind(grant.session_id)
            .bind(grant.kind)
            .bind(grant.action_kind)
            .bind(grant.action_args)
            .bind(grant.action_summary)
            .bind(grant.expires_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert oxpilot connect grant failed: {e}"))?;
            let row = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {TARGET_COLS_SQLITE} FROM oxpilot_targets WHERE id = ?1"
            )))
            .bind(target.id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("read oxpilot target failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot pairing claim failed: {e}"))?;
            Ok(Some(map_target!(&row)))
        }
    }
}

// -- targets --

pub async fn find_target(pool: &DbPool, id: &str) -> Result<Option<OxpilotTargetRow>, String> {
    find_target_where(pool, "id", id).await
}

/// The Bearer-verifier lookup — tombstoned (NULL) rows never match, so a
/// disconnected target's old verifier answers unauthorized (BR6.1).
pub async fn find_target_by_verifier_hash(
    pool: &DbPool,
    verifier_hash: &str,
) -> Result<Option<OxpilotTargetRow>, String> {
    find_target_where(pool, "verifier_hash", verifier_hash).await
}

/// Shared single-column target lookup — `column` is a fixed literal from the
/// two call sites above, never request input.
async fn find_target_where(
    pool: &DbPool,
    column: &str,
    value: &str,
) -> Result<Option<OxpilotTargetRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("SELECT {TARGET_COLS_PG} FROM oxpilot_targets WHERE {column} = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(value)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot target failed: {e}"))?
            {
                Some(r) => Ok(Some(map_target!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("SELECT {TARGET_COLS_MYSQL} FROM oxpilot_targets WHERE {column} = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(value)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot target failed: {e}"))?
            {
                Some(r) => Ok(Some(map_target!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql =
                format!("SELECT {TARGET_COLS_SQLITE} FROM oxpilot_targets WHERE {column} = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(value)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot target failed: {e}"))?
            {
                Some(r) => Ok(Some(map_target!(&r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn list_targets_for_user(
    pool: &DbPool,
    user_id: &str,
) -> Result<Vec<OxpilotTargetRow>, String> {
    let suffix = " WHERE user_id = {p} ORDER BY created_at, id";
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {TARGET_COLS_PG} FROM oxpilot_targets{}",
                suffix.replace("{p}", "$1")
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot targets failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_target!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {TARGET_COLS_MYSQL} FROM oxpilot_targets{}",
                suffix.replace("{p}", "?")
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot targets failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_target!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {TARGET_COLS_SQLITE} FROM oxpilot_targets{}",
                suffix.replace("{p}", "?1")
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot targets failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_target!(r));
            }
            Ok(out)
        }
    }
}

/// Liveness touch — any authenticated connector route piggybacks this
/// (BR5.4); `now` is the caller's RFC3339 stamp.
pub async fn touch_target_seen(pool: &DbPool, target_id: &str, now: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE oxpilot_targets SET last_seen_at = $2::timestamptz WHERE id = $1")
                .bind(target_id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| format!("touch oxpilot target failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE oxpilot_targets SET last_seen_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s') WHERE id = ?")
                .bind(now)
                .bind(target_id)
                .execute(p)
                .await
                .map_err(|e| format!("touch oxpilot target failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE oxpilot_targets SET last_seen_at = ?2 WHERE id = ?1")
                .bind(target_id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| format!("touch oxpilot target failed: {e}"))?;
        }
    }
    Ok(())
}

/// BR6.1 disconnect — one transaction tombstones the verifier, stamps
/// `disconnected`, and directly cancels every live grant and
/// non-terminal action (the channel is dead; `cancel_requested` could
/// never reach the connector). `Ok(None)` ⇒ already terminal
/// (disconnected/revoked) or absent — the idempotent arm (BR6.7).
pub async fn disconnect_target_tx(
    pool: &DbPool,
    target_id: &str,
    now: &str,
) -> Result<Option<OxpilotTargetLifecycleOutcome>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot disconnect tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_targets
                 SET status = 'disconnected', verifier_hash = NULL, disconnected_at = $2::timestamptz
                 WHERE id = $1 AND status NOT IN ('disconnected', 'revoked')",
            )
            .bind(target_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("disconnect oxpilot target failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            let outcome = cancel_live_rows_postgres(&mut tx, target_id, None, now).await?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot disconnect failed: {e}"))?;
            Ok(Some(outcome))
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot disconnect tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_targets
                 SET status = 'disconnected', verifier_hash = NULL, disconnected_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
                 WHERE id = ? AND status NOT IN ('disconnected', 'revoked')",
            )
            .bind(now)
            .bind(target_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("disconnect oxpilot target failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            let outcome = cancel_live_rows_mysql(&mut tx, target_id, None, now).await?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot disconnect failed: {e}"))?;
            Ok(Some(outcome))
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot disconnect tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_targets
                 SET status = 'disconnected', verifier_hash = NULL, disconnected_at = ?2
                 WHERE id = ?1 AND status NOT IN ('disconnected', 'revoked')",
            )
            .bind(target_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("disconnect oxpilot target failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            let outcome = cancel_live_rows_sqlite(&mut tx, target_id, None, now).await?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot disconnect failed: {e}"))?;
            Ok(Some(outcome))
        }
    }
}

/// BR6.2 revoke — verifier survives (the connector learns `revoked:true`
/// via heartbeat); grants go `revoked`, `queued` actions go directly
/// `cancelled`, and `claimed|running` go `cancel_requested` for delivery
/// on the next cancel set. `Ok(None)` ⇒ already terminal/absent.
pub async fn revoke_target_tx(
    pool: &DbPool,
    target_id: &str,
    now: &str,
) -> Result<Option<OxpilotTargetLifecycleOutcome>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot revoke tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_targets
                 SET status = 'revoked', revoked_at = $2::timestamptz
                 WHERE id = $1 AND status NOT IN ('disconnected', 'revoked')",
            )
            .bind(target_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("revoke oxpilot target failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            let outcome = revoke_live_rows_postgres(&mut tx, target_id, now).await?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot revoke failed: {e}"))?;
            Ok(Some(outcome))
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot revoke tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_targets
                 SET status = 'revoked', revoked_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
                 WHERE id = ? AND status NOT IN ('disconnected', 'revoked')",
            )
            .bind(now)
            .bind(target_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("revoke oxpilot target failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            let outcome = revoke_live_rows_mysql(&mut tx, target_id, now).await?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot revoke failed: {e}"))?;
            Ok(Some(outcome))
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot revoke tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_targets
                 SET status = 'revoked', revoked_at = ?2
                 WHERE id = ?1 AND status NOT IN ('disconnected', 'revoked')",
            )
            .bind(target_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("revoke oxpilot target failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            let outcome = revoke_live_rows_sqlite(&mut tx, target_id, now).await?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot revoke failed: {e}"))?;
            Ok(Some(outcome))
        }
    }
}

// Disconnect/session-delete shared bulk-close: live grants → `cancelled`,
// every non-terminal action → `cancelled`. `session_id` narrows the set
// for the C8 hook; `None` runs the whole target (disconnect).

async fn cancel_live_rows_postgres(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    target_id: &str,
    session_id: Option<&str>,
    now: &str,
) -> Result<OxpilotTargetLifecycleOutcome, String> {
    let grant_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {GRANT_COLS_PG} FROM oxpilot_grants
         WHERE target_id = $1 AND state IN ('pending', 'allowed')
           AND ($2::text IS NULL OR session_id = $2)"
    )))
    .bind(target_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select live oxpilot grants failed: {e}"))?;
    let mut grants: Vec<OxpilotGrantRow> = Vec::with_capacity(grant_rows.len());
    for r in &grant_rows {
        grants.push(map_grant!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_grants SET state = 'cancelled', resolved_at = $3::timestamptz
         WHERE target_id = $1 AND state IN ('pending', 'allowed')
           AND ($2::text IS NULL OR session_id = $2)",
    )
    .bind(target_id)
    .bind(session_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel oxpilot grants failed: {e}"))?;
    let action_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {ACTION_COLS_PG} FROM oxpilot_actions
         WHERE target_id = $1 AND state IN ('queued', 'claimed', 'running', 'cancel_requested')
           AND ($2::text IS NULL OR session_id = $2)"
    )))
    .bind(target_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select live oxpilot actions failed: {e}"))?;
    let mut actions: Vec<OxpilotActionRow> = Vec::with_capacity(action_rows.len());
    for r in &action_rows {
        actions.push(map_action!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = $3::timestamptz
         WHERE target_id = $1 AND state IN ('queued', 'claimed', 'running', 'cancel_requested')
           AND ($2::text IS NULL OR session_id = $2)",
    )
    .bind(target_id)
    .bind(session_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel oxpilot actions failed: {e}"))?;
    Ok(OxpilotTargetLifecycleOutcome {
        grants,
        cancelled_actions: actions,
        cancel_requested_actions: Vec::new(),
    })
}

async fn cancel_live_rows_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    target_id: &str,
    session_id: Option<&str>,
    now: &str,
) -> Result<OxpilotTargetLifecycleOutcome, String> {
    let grant_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {GRANT_COLS_MYSQL} FROM oxpilot_grants
         WHERE target_id = ? AND state IN ('pending', 'allowed')
           AND (? IS NULL OR session_id = ?)"
    )))
    .bind(target_id)
    .bind(session_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select live oxpilot grants failed: {e}"))?;
    let mut grants: Vec<OxpilotGrantRow> = Vec::with_capacity(grant_rows.len());
    for r in &grant_rows {
        grants.push(map_grant!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_grants SET state = 'cancelled', resolved_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
         WHERE target_id = ? AND state IN ('pending', 'allowed')
           AND (? IS NULL OR session_id = ?)",
    )
    .bind(now)
    .bind(target_id)
    .bind(session_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel oxpilot grants failed: {e}"))?;
    let action_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {ACTION_COLS_MYSQL} FROM oxpilot_actions
         WHERE target_id = ? AND state IN ('queued', 'claimed', 'running', 'cancel_requested')
           AND (? IS NULL OR session_id = ?)"
    )))
    .bind(target_id)
    .bind(session_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select live oxpilot actions failed: {e}"))?;
    let mut actions: Vec<OxpilotActionRow> = Vec::with_capacity(action_rows.len());
    for r in &action_rows {
        actions.push(map_action!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
         WHERE target_id = ? AND state IN ('queued', 'claimed', 'running', 'cancel_requested')
           AND (? IS NULL OR session_id = ?)",
    )
    .bind(now)
    .bind(target_id)
    .bind(session_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel oxpilot actions failed: {e}"))?;
    Ok(OxpilotTargetLifecycleOutcome {
        grants,
        cancelled_actions: actions,
        cancel_requested_actions: Vec::new(),
    })
}

async fn cancel_live_rows_sqlite(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    target_id: &str,
    session_id: Option<&str>,
    now: &str,
) -> Result<OxpilotTargetLifecycleOutcome, String> {
    let grant_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {GRANT_COLS_SQLITE} FROM oxpilot_grants
         WHERE target_id = ?1 AND state IN ('pending', 'allowed')
           AND (?2 IS NULL OR session_id = ?2)"
    )))
    .bind(target_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select live oxpilot grants failed: {e}"))?;
    let mut grants: Vec<OxpilotGrantRow> = Vec::with_capacity(grant_rows.len());
    for r in &grant_rows {
        grants.push(map_grant!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_grants SET state = 'cancelled', resolved_at = ?3
         WHERE target_id = ?1 AND state IN ('pending', 'allowed')
           AND (?2 IS NULL OR session_id = ?2)",
    )
    .bind(target_id)
    .bind(session_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel oxpilot grants failed: {e}"))?;
    let action_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {ACTION_COLS_SQLITE} FROM oxpilot_actions
         WHERE target_id = ?1 AND state IN ('queued', 'claimed', 'running', 'cancel_requested')
           AND (?2 IS NULL OR session_id = ?2)"
    )))
    .bind(target_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select live oxpilot actions failed: {e}"))?;
    let mut actions: Vec<OxpilotActionRow> = Vec::with_capacity(action_rows.len());
    for r in &action_rows {
        actions.push(map_action!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = ?3
         WHERE target_id = ?1 AND state IN ('queued', 'claimed', 'running', 'cancel_requested')
           AND (?2 IS NULL OR session_id = ?2)",
    )
    .bind(target_id)
    .bind(session_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel oxpilot actions failed: {e}"))?;
    Ok(OxpilotTargetLifecycleOutcome {
        grants,
        cancelled_actions: actions,
        cancel_requested_actions: Vec::new(),
    })
}

// Revoke's bulk-close: grants → `revoked`, `queued` → `cancelled`,
// `claimed|running` → `cancel_requested` (heartbeat still delivers).

async fn revoke_live_rows_postgres(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    target_id: &str,
    now: &str,
) -> Result<OxpilotTargetLifecycleOutcome, String> {
    let grant_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {GRANT_COLS_PG} FROM oxpilot_grants
         WHERE target_id = $1 AND state IN ('pending', 'allowed')"
    )))
    .bind(target_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select live oxpilot grants failed: {e}"))?;
    let mut grants: Vec<OxpilotGrantRow> = Vec::with_capacity(grant_rows.len());
    for r in &grant_rows {
        grants.push(map_grant!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_grants SET state = 'revoked', resolved_at = $2::timestamptz
         WHERE target_id = $1 AND state IN ('pending', 'allowed')",
    )
    .bind(target_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("revoke oxpilot grants failed: {e}"))?;
    let queued_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {ACTION_COLS_PG} FROM oxpilot_actions
         WHERE target_id = $1 AND state = 'queued'"
    )))
    .bind(target_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select queued oxpilot actions failed: {e}"))?;
    let mut queued: Vec<OxpilotActionRow> = Vec::with_capacity(queued_rows.len());
    for r in &queued_rows {
        queued.push(map_action!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = $2::timestamptz
         WHERE target_id = $1 AND state = 'queued'",
    )
    .bind(target_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel queued oxpilot actions failed: {e}"))?;
    let inflight_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {ACTION_COLS_PG} FROM oxpilot_actions
         WHERE target_id = $1 AND state IN ('claimed', 'running')"
    )))
    .bind(target_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select in-flight oxpilot actions failed: {e}"))?;
    let mut inflight: Vec<OxpilotActionRow> = Vec::with_capacity(inflight_rows.len());
    for r in &inflight_rows {
        inflight.push(map_action!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_actions SET state = 'cancel_requested'
         WHERE target_id = $1 AND state IN ('claimed', 'running')",
    )
    .bind(target_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel-request oxpilot actions failed: {e}"))?;
    Ok(OxpilotTargetLifecycleOutcome {
        grants,
        cancelled_actions: queued,
        cancel_requested_actions: inflight,
    })
}

async fn revoke_live_rows_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    target_id: &str,
    now: &str,
) -> Result<OxpilotTargetLifecycleOutcome, String> {
    let grant_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {GRANT_COLS_MYSQL} FROM oxpilot_grants
         WHERE target_id = ? AND state IN ('pending', 'allowed')"
    )))
    .bind(target_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select live oxpilot grants failed: {e}"))?;
    let mut grants: Vec<OxpilotGrantRow> = Vec::with_capacity(grant_rows.len());
    for r in &grant_rows {
        grants.push(map_grant!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_grants SET state = 'revoked', resolved_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
         WHERE target_id = ? AND state IN ('pending', 'allowed')",
    )
    .bind(now)
    .bind(target_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("revoke oxpilot grants failed: {e}"))?;
    let queued_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {ACTION_COLS_MYSQL} FROM oxpilot_actions
         WHERE target_id = ? AND state = 'queued'"
    )))
    .bind(target_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select queued oxpilot actions failed: {e}"))?;
    let mut queued: Vec<OxpilotActionRow> = Vec::with_capacity(queued_rows.len());
    for r in &queued_rows {
        queued.push(map_action!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
         WHERE target_id = ? AND state = 'queued'",
    )
    .bind(now)
    .bind(target_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel queued oxpilot actions failed: {e}"))?;
    let inflight_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {ACTION_COLS_MYSQL} FROM oxpilot_actions
         WHERE target_id = ? AND state IN ('claimed', 'running')"
    )))
    .bind(target_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select in-flight oxpilot actions failed: {e}"))?;
    let mut inflight: Vec<OxpilotActionRow> = Vec::with_capacity(inflight_rows.len());
    for r in &inflight_rows {
        inflight.push(map_action!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_actions SET state = 'cancel_requested'
         WHERE target_id = ? AND state IN ('claimed', 'running')",
    )
    .bind(target_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel-request oxpilot actions failed: {e}"))?;
    Ok(OxpilotTargetLifecycleOutcome {
        grants,
        cancelled_actions: queued,
        cancel_requested_actions: inflight,
    })
}

async fn revoke_live_rows_sqlite(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    target_id: &str,
    now: &str,
) -> Result<OxpilotTargetLifecycleOutcome, String> {
    let grant_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {GRANT_COLS_SQLITE} FROM oxpilot_grants
         WHERE target_id = ?1 AND state IN ('pending', 'allowed')"
    )))
    .bind(target_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select live oxpilot grants failed: {e}"))?;
    let mut grants: Vec<OxpilotGrantRow> = Vec::with_capacity(grant_rows.len());
    for r in &grant_rows {
        grants.push(map_grant!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_grants SET state = 'revoked', resolved_at = ?2
         WHERE target_id = ?1 AND state IN ('pending', 'allowed')",
    )
    .bind(target_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("revoke oxpilot grants failed: {e}"))?;
    let queued_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {ACTION_COLS_SQLITE} FROM oxpilot_actions
         WHERE target_id = ?1 AND state = 'queued'"
    )))
    .bind(target_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select queued oxpilot actions failed: {e}"))?;
    let mut queued: Vec<OxpilotActionRow> = Vec::with_capacity(queued_rows.len());
    for r in &queued_rows {
        queued.push(map_action!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = ?2
         WHERE target_id = ?1 AND state = 'queued'",
    )
    .bind(target_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel queued oxpilot actions failed: {e}"))?;
    let inflight_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
        "SELECT {ACTION_COLS_SQLITE} FROM oxpilot_actions
         WHERE target_id = ?1 AND state IN ('claimed', 'running')"
    )))
    .bind(target_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("select in-flight oxpilot actions failed: {e}"))?;
    let mut inflight: Vec<OxpilotActionRow> = Vec::with_capacity(inflight_rows.len());
    for r in &inflight_rows {
        inflight.push(map_action!(r));
    }
    sqlx::query(
        "UPDATE oxpilot_actions SET state = 'cancel_requested'
         WHERE target_id = ?1 AND state IN ('claimed', 'running')",
    )
    .bind(target_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("cancel-request oxpilot actions failed: {e}"))?;
    Ok(OxpilotTargetLifecycleOutcome {
        grants,
        cancelled_actions: queued,
        cancel_requested_actions: inflight,
    })
}

// -- grants --

/// Standalone grant insert (BR3.2 re-mint, dispatch pending mint) — the
/// callers serialize on the per-target lock so the existence-check +
/// insert pair is atomic for the target.
pub async fn insert_grant(
    pool: &DbPool,
    g: &NewOxpilotGrant<'_>,
) -> Result<OxpilotGrantRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_grants
                     (id, target_id, user_id, session_id, kind, action_kind, action_args, action_summary, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::timestamptz)",
            )
            .bind(g.id)
            .bind(g.target_id)
            .bind(g.user_id)
            .bind(g.session_id)
            .bind(g.kind)
            .bind(g.action_kind)
            .bind(g.action_args)
            .bind(g.action_summary)
            .bind(g.expires_at)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot grant failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_grants
                     (id, target_id, user_id, session_id, kind, action_kind, action_args, action_summary, expires_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'))",
            )
            .bind(g.id)
            .bind(g.target_id)
            .bind(g.user_id)
            .bind(g.session_id)
            .bind(g.kind)
            .bind(g.action_kind)
            .bind(g.action_args)
            .bind(g.action_summary)
            .bind(g.expires_at)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot grant failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_grants
                     (id, target_id, user_id, session_id, kind, action_kind, action_args, action_summary, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(g.id)
            .bind(g.target_id)
            .bind(g.user_id)
            .bind(g.session_id)
            .bind(g.kind)
            .bind(g.action_kind)
            .bind(g.action_args)
            .bind(g.action_summary)
            .bind(g.expires_at)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot grant failed: {e}"))?;
        }
    }
    match find_grant(pool, g.id).await? {
        Some(row) => Ok(row),
        None => Err("inserted oxpilot grant not found".to_string()),
    }
}

pub async fn find_grant(pool: &DbPool, id: &str) -> Result<Option<OxpilotGrantRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("SELECT {GRANT_COLS_PG} FROM oxpilot_grants WHERE id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("SELECT {GRANT_COLS_MYSQL} FROM oxpilot_grants WHERE id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("SELECT {GRANT_COLS_SQLITE} FROM oxpilot_grants WHERE id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// The owner grant surface — pending-first, then most recently created
/// resolved rows (WF6); `target_id` optionally narrows to one target.
/// Bounded at 500 rows — the consent surface never reads deeper.
pub async fn list_grants_for_user(
    pool: &DbPool,
    user_id: &str,
    target_id: Option<&str>,
) -> Result<Vec<OxpilotGrantRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_PG} FROM oxpilot_grants
                 WHERE user_id = $1 AND ($2::text IS NULL OR target_id = $2)
                 ORDER BY CASE WHEN state = 'pending' THEN 0 ELSE 1 END, created_at DESC
                 LIMIT 500"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .bind(target_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot grants failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_grant!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_MYSQL} FROM oxpilot_grants
                 WHERE user_id = ? AND (? IS NULL OR target_id = ?)
                 ORDER BY CASE WHEN state = 'pending' THEN 0 ELSE 1 END, created_at DESC
                 LIMIT 500"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .bind(target_id)
                .bind(target_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot grants failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_grant!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_SQLITE} FROM oxpilot_grants
                 WHERE user_id = ?1 AND (?2 IS NULL OR target_id = ?2)
                 ORDER BY CASE WHEN state = 'pending' THEN 0 ELSE 1 END, created_at DESC
                 LIMIT 500"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .bind(target_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot grants failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_grant!(r));
            }
            Ok(out)
        }
    }
}

/// U8 (`grant_list{session_id}`) — the session-scoped owner read: only
/// rows bound to THIS session's `session_id` column (connect grants
/// carry NULL and never match); a `target_id` sent alongside ANDs in
/// with the same optional-bind shape `list_grants_for_user` uses. Same
/// pending-first ordering + bound as the target-scoped list;
/// `0088`'s `idx_oxpilot_grants_session` covers the filter.
pub async fn list_grants_for_session(
    pool: &DbPool,
    user_id: &str,
    session_id: &str,
    target_id: Option<&str>,
) -> Result<Vec<OxpilotGrantRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_PG} FROM oxpilot_grants
                 WHERE user_id = $1 AND session_id = $2
                   AND ($3::text IS NULL OR target_id = $3)
                 ORDER BY CASE WHEN state = 'pending' THEN 0 ELSE 1 END, created_at DESC
                 LIMIT 500"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .bind(session_id)
                .bind(target_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot session grants failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_grant!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_MYSQL} FROM oxpilot_grants
                 WHERE user_id = ? AND session_id = ?
                   AND (? IS NULL OR target_id = ?)
                 ORDER BY CASE WHEN state = 'pending' THEN 0 ELSE 1 END, created_at DESC
                 LIMIT 500"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .bind(session_id)
                .bind(target_id)
                .bind(target_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot session grants failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_grant!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_SQLITE} FROM oxpilot_grants
                 WHERE user_id = ?1 AND session_id = ?2
                   AND (?3 IS NULL OR target_id = ?3)
                 ORDER BY CASE WHEN state = 'pending' THEN 0 ELSE 1 END, created_at DESC
                 LIMIT 500"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(user_id)
                .bind(session_id)
                .bind(target_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list oxpilot session grants failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_grant!(r));
            }
            Ok(out)
        }
    }
}

/// The dispatch live-grant check (BR3.1): an `allowed` `action_session`
/// grant for this `(target, session)` whose `expires_at` is still ahead.
pub async fn live_session_grant(
    pool: &DbPool,
    target_id: &str,
    session_id: &str,
    now: &str,
) -> Result<Option<OxpilotGrantRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_PG} FROM oxpilot_grants
                 WHERE target_id = $1 AND session_id = $2 AND kind = 'action_session'
                   AND state = 'allowed' AND expires_at > $3::timestamptz"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_id)
                .bind(session_id)
                .bind(now)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find live oxpilot session grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_MYSQL} FROM oxpilot_grants
                 WHERE target_id = ? AND session_id = ? AND kind = 'action_session'
                   AND state = 'allowed' AND expires_at > STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_id)
                .bind(session_id)
                .bind(now)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find live oxpilot session grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_SQLITE} FROM oxpilot_grants
                 WHERE target_id = ?1 AND session_id = ?2 AND kind = 'action_session'
                   AND state = 'allowed' AND expires_at > ?3"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_id)
                .bind(session_id)
                .bind(now)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find live oxpilot session grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Pending-action dedupe (BR3.3): an identical
/// `(target, session, action_kind, action_args)` still `pending` joins
/// the existing grant rather than minting a second consent card.
pub async fn pending_action_grant(
    pool: &DbPool,
    target_id: &str,
    session_id: &str,
    action_kind: &str,
    action_args: &str,
) -> Result<Option<OxpilotGrantRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_PG} FROM oxpilot_grants
                 WHERE target_id = $1 AND session_id = $2 AND kind = 'action' AND state = 'pending'
                   AND action_kind = $3 AND action_args = $4"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_id)
                .bind(session_id)
                .bind(action_kind)
                .bind(action_args)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find pending oxpilot action grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_MYSQL} FROM oxpilot_grants
                 WHERE target_id = ? AND session_id = ? AND kind = 'action' AND state = 'pending'
                   AND action_kind = ? AND action_args = ?"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_id)
                .bind(session_id)
                .bind(action_kind)
                .bind(action_args)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find pending oxpilot action grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_SQLITE} FROM oxpilot_grants
                 WHERE target_id = ?1 AND session_id = ?2 AND kind = 'action' AND state = 'pending'
                   AND action_kind = ?3 AND action_args = ?4"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_id)
                .bind(session_id)
                .bind(action_kind)
                .bind(action_args)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find pending oxpilot action grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// BR3.2's "live one" — a still-`pending`, still-unexpired connect grant
/// on this target. `None` means an owner read re-mints.
pub async fn pending_connect_grant(
    pool: &DbPool,
    target_id: &str,
    now: &str,
) -> Result<Option<OxpilotGrantRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_PG} FROM oxpilot_grants
                 WHERE target_id = $1 AND kind = 'connect' AND state = 'pending'
                   AND expires_at > $2::timestamptz"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_id)
                .bind(now)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find pending oxpilot connect grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_MYSQL} FROM oxpilot_grants
                 WHERE target_id = ? AND kind = 'connect' AND state = 'pending'
                   AND expires_at > STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_id)
                .bind(now)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find pending oxpilot connect grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {GRANT_COLS_SQLITE} FROM oxpilot_grants
                 WHERE target_id = ?1 AND kind = 'connect' AND state = 'pending'
                   AND expires_at > ?2"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target_id)
                .bind(now)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find pending oxpilot connect grant failed: {e}"))?
            {
                Some(r) => Ok(Some(map_grant!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Lazy `pending → expired` stamp (BR3.1/WF6) — returns rows updated.
pub async fn expire_grant(pool: &DbPool, id: &str, now: &str) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE oxpilot_grants SET state = 'expired', resolved_at = $2::timestamptz WHERE id = $1 AND state = 'pending'")
                .bind(id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| format!("expire oxpilot grant failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE oxpilot_grants SET state = 'expired', resolved_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s') WHERE id = ? AND state = 'pending'")
                .bind(now)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("expire oxpilot grant failed: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE oxpilot_grants SET state = 'expired', resolved_at = ?2 WHERE id = ?1 AND state = 'pending'")
                .bind(id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| format!("expire oxpilot grant failed: {e}"))?
                .rows_affected()
        }
    };
    Ok(n)
}

/// `pending → denied` — CAS returns rows updated (0 ⇒ already resolved).
pub async fn deny_grant(pool: &DbPool, id: &str, now: &str) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE oxpilot_grants SET state = 'denied', resolved_at = $2::timestamptz WHERE id = $1 AND state = 'pending'")
                .bind(id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| format!("deny oxpilot grant failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE oxpilot_grants SET state = 'denied', resolved_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s') WHERE id = ? AND state = 'pending'")
                .bind(now)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("deny oxpilot grant failed: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE oxpilot_grants SET state = 'denied', resolved_at = ?2 WHERE id = ?1 AND state = 'pending'")
                .bind(id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| format!("deny oxpilot grant failed: {e}"))?
                .rows_affected()
        }
    };
    Ok(n)
}

/// Allow a connect grant — `pending → allowed` + `pending_consent →
/// connected` on the covering target in ONE transaction (BR4.2/WF1).
/// `false` ⇒ the grant CAS lost (already resolved — `grant_not_pending`)
/// or the target CAS missed (already left `pending_consent` — a dead
/// target can never record `connected`; both statements roll back).
pub async fn allow_connect_grant_tx(
    pool: &DbPool,
    grant_id: &str,
    target_id: &str,
    now: &str,
) -> Result<bool, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot connect allow tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_grants SET state = 'allowed', resolved_at = $2::timestamptz
                 WHERE id = $1 AND state = 'pending'",
            )
            .bind(grant_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("allow oxpilot connect grant failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(false);
            }
            // The target CAS must land too — a dead target can never
            // record `connected` (BR6.3). `Ok(false)` drops the tx:
            // the grant flip rolls back with it.
            let t = sqlx::query(
                "UPDATE oxpilot_targets SET status = 'connected'
                 WHERE id = $1 AND status = 'pending_consent'",
            )
            .bind(target_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("connect oxpilot target failed: {e}"))?
            .rows_affected();
            if t == 0 {
                return Ok(false);
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot connect allow failed: {e}"))?;
            Ok(true)
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot connect allow tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_grants SET state = 'allowed', resolved_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
                 WHERE id = ? AND state = 'pending'",
            )
            .bind(now)
            .bind(grant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("allow oxpilot connect grant failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(false);
            }
            let t = sqlx::query(
                "UPDATE oxpilot_targets SET status = 'connected'
                 WHERE id = ? AND status = 'pending_consent'",
            )
            .bind(target_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("connect oxpilot target failed: {e}"))?
            .rows_affected();
            if t == 0 {
                return Ok(false);
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot connect allow failed: {e}"))?;
            Ok(true)
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot connect allow tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_grants SET state = 'allowed', resolved_at = ?2
                 WHERE id = ?1 AND state = 'pending'",
            )
            .bind(grant_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("allow oxpilot connect grant failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(false);
            }
            let t = sqlx::query(
                "UPDATE oxpilot_targets SET status = 'connected'
                 WHERE id = ?1 AND status = 'pending_consent'",
            )
            .bind(target_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("connect oxpilot target failed: {e}"))?
            .rows_affected();
            if t == 0 {
                return Ok(false);
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot connect allow failed: {e}"))?;
            Ok(true)
        }
    }
}

/// Allow an action grant — CAS `pending → allowed` + `kind` stamp
/// (`action_once|action_session`) + the materialized `Action{queued}`
/// INSERT in ONE transaction (BR4.2). `Ok(None)` ⇒ CAS lost.
pub async fn allow_action_grant_tx(
    pool: &DbPool,
    grant_id: &str,
    stamped_kind: &str,
    action: &NewOxpilotAction<'_>,
    now: &str,
) -> Result<Option<OxpilotActionRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot action allow tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_grants SET state = 'allowed', kind = $2, resolved_at = $3::timestamptz
                 WHERE id = $1 AND state = 'pending'",
            )
            .bind(grant_id)
            .bind(stamped_kind)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("allow oxpilot action grant failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            sqlx::query(
                "INSERT INTO oxpilot_actions
                     (id, target_id, user_id, session_id, grant_id, kind, args, args_summary)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(action.id)
            .bind(action.target_id)
            .bind(action.user_id)
            .bind(action.session_id)
            .bind(action.grant_id)
            .bind(action.kind)
            .bind(action.args)
            .bind(action.args_summary)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert oxpilot action failed: {e}"))?;
            let row = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_PG} FROM oxpilot_actions WHERE id = $1"
            )))
            .bind(action.id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("read oxpilot action failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot action allow failed: {e}"))?;
            Ok(Some(map_action!(&row)))
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot action allow tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_grants SET state = 'allowed', kind = ?, resolved_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
                 WHERE id = ? AND state = 'pending'",
            )
            .bind(stamped_kind)
            .bind(now)
            .bind(grant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("allow oxpilot action grant failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            sqlx::query(
                "INSERT INTO oxpilot_actions
                     (id, target_id, user_id, session_id, grant_id, kind, args, args_summary)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(action.id)
            .bind(action.target_id)
            .bind(action.user_id)
            .bind(action.session_id)
            .bind(action.grant_id)
            .bind(action.kind)
            .bind(action.args)
            .bind(action.args_summary)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert oxpilot action failed: {e}"))?;
            let row = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_MYSQL} FROM oxpilot_actions WHERE id = ?"
            )))
            .bind(action.id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("read oxpilot action failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot action allow failed: {e}"))?;
            Ok(Some(map_action!(&row)))
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot action allow tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_grants SET state = 'allowed', kind = ?2, resolved_at = ?3
                 WHERE id = ?1 AND state = 'pending'",
            )
            .bind(grant_id)
            .bind(stamped_kind)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("allow oxpilot action grant failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            sqlx::query(
                "INSERT INTO oxpilot_actions
                     (id, target_id, user_id, session_id, grant_id, kind, args, args_summary)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(action.id)
            .bind(action.target_id)
            .bind(action.user_id)
            .bind(action.session_id)
            .bind(action.grant_id)
            .bind(action.kind)
            .bind(action.args)
            .bind(action.args_summary)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert oxpilot action failed: {e}"))?;
            let row = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_SQLITE} FROM oxpilot_actions WHERE id = ?1"
            )))
            .bind(action.id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("read oxpilot action failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot action allow failed: {e}"))?;
            Ok(Some(map_action!(&row)))
        }
    }
}

/// BR6.4 — deny-respond on an `allowed` `action_session` grant revokes
/// the grant and moves its queued actions to `cancelled`, its in-flight
/// `claimed|running` to `cancel_requested` (delivered on the next
/// claim/heartbeat cancel set). `Ok(None)` ⇒ the grant wasn't in the
/// `allowed`+`action_session` shape (resolves `grant_not_pending`).
pub async fn revoke_session_grant_tx(
    pool: &DbPool,
    grant_id: &str,
    now: &str,
) -> Result<Option<OxpilotSessionGrantRevokeOutcome>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot grant revoke tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_grants SET state = 'revoked', resolved_at = $2::timestamptz
                 WHERE id = $1 AND state = 'allowed' AND kind = 'action_session'",
            )
            .bind(grant_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("revoke oxpilot session grant failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            let grant = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {GRANT_COLS_PG} FROM oxpilot_grants WHERE id = $1"
            )))
            .bind(grant_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("read oxpilot grant failed: {e}"))?;
            let grant = map_grant!(&grant);
            let queued_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_PG} FROM oxpilot_actions
                 WHERE grant_id = $1 AND state = 'queued'"
            )))
            .bind(grant_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select queued oxpilot actions failed: {e}"))?;
            let mut queued: Vec<OxpilotActionRow> = Vec::with_capacity(queued_rows.len());
            for r in &queued_rows {
                queued.push(map_action!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = $2::timestamptz
                 WHERE grant_id = $1 AND state = 'queued'",
            )
            .bind(grant_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel queued oxpilot actions failed: {e}"))?;
            let inflight_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_PG} FROM oxpilot_actions
                 WHERE grant_id = $1 AND state IN ('claimed', 'running')"
            )))
            .bind(grant_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select in-flight oxpilot actions failed: {e}"))?;
            let mut inflight: Vec<OxpilotActionRow> = Vec::with_capacity(inflight_rows.len());
            for r in &inflight_rows {
                inflight.push(map_action!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_actions SET state = 'cancel_requested'
                 WHERE grant_id = $1 AND state IN ('claimed', 'running')",
            )
            .bind(grant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel-request oxpilot actions failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot grant revoke failed: {e}"))?;
            Ok(Some(OxpilotSessionGrantRevokeOutcome {
                grant,
                cancelled_actions: queued,
                cancel_requested_actions: inflight,
            }))
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot grant revoke tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_grants SET state = 'revoked', resolved_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
                 WHERE id = ? AND state = 'allowed' AND kind = 'action_session'",
            )
            .bind(now)
            .bind(grant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("revoke oxpilot session grant failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            let grant = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {GRANT_COLS_MYSQL} FROM oxpilot_grants WHERE id = ?"
            )))
            .bind(grant_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("read oxpilot grant failed: {e}"))?;
            let grant = map_grant!(&grant);
            let queued_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_MYSQL} FROM oxpilot_actions
                 WHERE grant_id = ? AND state = 'queued'"
            )))
            .bind(grant_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select queued oxpilot actions failed: {e}"))?;
            let mut queued: Vec<OxpilotActionRow> = Vec::with_capacity(queued_rows.len());
            for r in &queued_rows {
                queued.push(map_action!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
                 WHERE grant_id = ? AND state = 'queued'",
            )
            .bind(now)
            .bind(grant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel queued oxpilot actions failed: {e}"))?;
            let inflight_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_MYSQL} FROM oxpilot_actions
                 WHERE grant_id = ? AND state IN ('claimed', 'running')"
            )))
            .bind(grant_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select in-flight oxpilot actions failed: {e}"))?;
            let mut inflight: Vec<OxpilotActionRow> = Vec::with_capacity(inflight_rows.len());
            for r in &inflight_rows {
                inflight.push(map_action!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_actions SET state = 'cancel_requested'
                 WHERE grant_id = ? AND state IN ('claimed', 'running')",
            )
            .bind(grant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel-request oxpilot actions failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot grant revoke failed: {e}"))?;
            Ok(Some(OxpilotSessionGrantRevokeOutcome {
                grant,
                cancelled_actions: queued,
                cancel_requested_actions: inflight,
            }))
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot grant revoke tx failed: {e}"))?;
            let n = sqlx::query(
                "UPDATE oxpilot_grants SET state = 'revoked', resolved_at = ?2
                 WHERE id = ?1 AND state = 'allowed' AND kind = 'action_session'",
            )
            .bind(grant_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("revoke oxpilot session grant failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Ok(None);
            }
            let grant = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {GRANT_COLS_SQLITE} FROM oxpilot_grants WHERE id = ?1"
            )))
            .bind(grant_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("read oxpilot grant failed: {e}"))?;
            let grant = map_grant!(&grant);
            let queued_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_SQLITE} FROM oxpilot_actions
                 WHERE grant_id = ?1 AND state = 'queued'"
            )))
            .bind(grant_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select queued oxpilot actions failed: {e}"))?;
            let mut queued: Vec<OxpilotActionRow> = Vec::with_capacity(queued_rows.len());
            for r in &queued_rows {
                queued.push(map_action!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = ?2
                 WHERE grant_id = ?1 AND state = 'queued'",
            )
            .bind(grant_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel queued oxpilot actions failed: {e}"))?;
            let inflight_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_SQLITE} FROM oxpilot_actions
                 WHERE grant_id = ?1 AND state IN ('claimed', 'running')"
            )))
            .bind(grant_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select in-flight oxpilot actions failed: {e}"))?;
            let mut inflight: Vec<OxpilotActionRow> = Vec::with_capacity(inflight_rows.len());
            for r in &inflight_rows {
                inflight.push(map_action!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_actions SET state = 'cancel_requested'
                 WHERE grant_id = ?1 AND state IN ('claimed', 'running')",
            )
            .bind(grant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel-request oxpilot actions failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit oxpilot grant revoke failed: {e}"))?;
            Ok(Some(OxpilotSessionGrantRevokeOutcome {
                grant,
                cancelled_actions: queued,
                cancel_requested_actions: inflight,
            }))
        }
    }
}

// -- actions --

/// Standalone action insert — the dispatch live-session-grant path
/// (BR4.1): the covering grant's liveness was verified under the
/// per-target lock immediately before, so a lone INSERT is atomic enough.
pub async fn insert_action(
    pool: &DbPool,
    a: &NewOxpilotAction<'_>,
) -> Result<OxpilotActionRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_actions
                     (id, target_id, user_id, session_id, grant_id, kind, args, args_summary)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(a.id)
            .bind(a.target_id)
            .bind(a.user_id)
            .bind(a.session_id)
            .bind(a.grant_id)
            .bind(a.kind)
            .bind(a.args)
            .bind(a.args_summary)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot action failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_actions
                     (id, target_id, user_id, session_id, grant_id, kind, args, args_summary)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(a.id)
            .bind(a.target_id)
            .bind(a.user_id)
            .bind(a.session_id)
            .bind(a.grant_id)
            .bind(a.kind)
            .bind(a.args)
            .bind(a.args_summary)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot action failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO oxpilot_actions
                     (id, target_id, user_id, session_id, grant_id, kind, args, args_summary)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(a.id)
            .bind(a.target_id)
            .bind(a.user_id)
            .bind(a.session_id)
            .bind(a.grant_id)
            .bind(a.kind)
            .bind(a.args)
            .bind(a.args_summary)
            .execute(p)
            .await
            .map_err(|e| format!("insert oxpilot action failed: {e}"))?;
        }
    }
    match find_action(pool, a.id).await? {
        Some(row) => Ok(row),
        None => Err("inserted oxpilot action not found".to_string()),
    }
}

pub async fn find_action(pool: &DbPool, id: &str) -> Result<Option<OxpilotActionRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("SELECT {ACTION_COLS_PG} FROM oxpilot_actions WHERE id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot action failed: {e}"))?
            {
                Some(r) => Ok(Some(map_action!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("SELECT {ACTION_COLS_MYSQL} FROM oxpilot_actions WHERE id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot action failed: {e}"))?
            {
                Some(r) => Ok(Some(map_action!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("SELECT {ACTION_COLS_SQLITE} FROM oxpilot_actions WHERE id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find oxpilot action failed: {e}"))?
            {
                Some(r) => Ok(Some(map_action!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// The dequeue candidate scan (BR5.1): FIFO `queued` rows for the
/// target, each JOINed to its covering grant's `state` + `expires_at` so
/// the per-row dead-grant/capability re-check needs no extra round-trip.
/// Rides the `(target_id, state, created_at, id)` index.
pub async fn queued_action_candidates(
    pool: &DbPool,
    target_id: &str,
    limit: i64,
) -> Result<Vec<OxpilotQueuedCandidate>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_PG_PREFIXED},
                        g.state AS grant_state,
                        to_char(g.expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS grant_expires_at
                 FROM oxpilot_actions a JOIN oxpilot_grants g ON g.id = a.grant_id
                 WHERE a.target_id = $1 AND a.state = 'queued'
                 ORDER BY a.created_at, a.id LIMIT $2"
            )))
            .bind(target_id)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("scan queued oxpilot actions failed: {e}"))?;
            rows.iter()
                .map(|r| {
                    Ok(OxpilotQueuedCandidate {
                        action: map_action!(r),
                        grant_state: r
                            .try_get("grant_state")
                            .map_err(|e| format!("queued candidate grant: {e}"))?,
                        grant_expires_at: r
                            .try_get("grant_expires_at")
                            .map_err(|e| format!("queued candidate grant: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_MYSQL_PREFIXED},
                        g.state AS grant_state,
                        DATE_FORMAT(g.expires_at, '%Y-%m-%dT%H:%i:%sZ') AS grant_expires_at
                 FROM oxpilot_actions a JOIN oxpilot_grants g ON g.id = a.grant_id
                 WHERE a.target_id = ? AND a.state = 'queued'
                 ORDER BY a.created_at, a.id LIMIT ?"
            )))
            .bind(target_id)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("scan queued oxpilot actions failed: {e}"))?;
            rows.iter()
                .map(|r| {
                    Ok(OxpilotQueuedCandidate {
                        action: map_action!(r),
                        grant_state: r
                            .try_get("grant_state")
                            .map_err(|e| format!("queued candidate grant: {e}"))?,
                        grant_expires_at: r
                            .try_get("grant_expires_at")
                            .map_err(|e| format!("queued candidate grant: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_SQLITE_PREFIXED},
                        g.state AS grant_state,
                        strftime('%Y-%m-%dT%H:%M:%SZ', g.expires_at) AS grant_expires_at
                 FROM oxpilot_actions a JOIN oxpilot_grants g ON g.id = a.grant_id
                 WHERE a.target_id = ?1 AND a.state = 'queued'
                 ORDER BY a.created_at, a.id LIMIT ?2"
            )))
            .bind(target_id)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("scan queued oxpilot actions failed: {e}"))?;
            rows.iter()
                .map(|r| {
                    Ok(OxpilotQueuedCandidate {
                        action: map_action!(r),
                        grant_state: r
                            .try_get("grant_state")
                            .map_err(|e| format!("queued candidate grant: {e}"))?,
                        grant_expires_at: r
                            .try_get("grant_expires_at")
                            .map_err(|e| format!("queued candidate grant: {e}"))?,
                    })
                })
                .collect()
        }
    }
}

/// `queued → claimed` CAS (+ `claimed_at`) — the per-row dequeue claim;
/// `0` means the row moved underneath us (skip, BR5.1).
pub async fn claim_action(pool: &DbPool, action_id: &str, now: &str) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE oxpilot_actions SET state = 'claimed', claimed_at = $2::timestamptz WHERE id = $1 AND state = 'queued'")
                .bind(action_id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| format!("claim oxpilot action failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE oxpilot_actions SET state = 'claimed', claimed_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s') WHERE id = ? AND state = 'queued'")
                .bind(now)
                .bind(action_id)
                .execute(p)
                .await
                .map_err(|e| format!("claim oxpilot action failed: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE oxpilot_actions SET state = 'claimed', claimed_at = ?2 WHERE id = ?1 AND state = 'queued'")
                .bind(action_id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| format!("claim oxpilot action failed: {e}"))?
                .rows_affected()
        }
    };
    Ok(n)
}

/// Dequeue dead-grant close — `queued → cancelled` (+ `finished_at`);
/// the `action_cancelled{grant_expired}` audit rides the caller.
pub async fn cancel_queued_action(
    pool: &DbPool,
    action_id: &str,
    now: &str,
) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE oxpilot_actions SET state = 'cancelled', finished_at = $2::timestamptz WHERE id = $1 AND state = 'queued'")
                .bind(action_id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| format!("cancel queued oxpilot action failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE oxpilot_actions SET state = 'cancelled', finished_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s') WHERE id = ? AND state = 'queued'")
                .bind(now)
                .bind(action_id)
                .execute(p)
                .await
                .map_err(|e| format!("cancel queued oxpilot action failed: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE oxpilot_actions SET state = 'cancelled', finished_at = ?2 WHERE id = ?1 AND state = 'queued'")
                .bind(action_id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| format!("cancel queued oxpilot action failed: {e}"))?
                .rows_affected()
        }
    };
    Ok(n)
}

/// Terminal report close (BR5.2): `claimed|running|cancel_requested →
/// new_state` — the WHERE carries the live-state set so a terminal row
/// or a loser-CAS answers `0` (the caller reads it as the idempotent
/// `{ack:true}` arm).
pub async fn close_action_report(
    pool: &DbPool,
    action_id: &str,
    new_state: &str,
    result_summary: Option<&str>,
    now: &str,
) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE oxpilot_actions SET state = $2, result_summary = $3, finished_at = $4::timestamptz
                 WHERE id = $1 AND state IN ('claimed', 'running', 'cancel_requested')",
            )
            .bind(action_id)
            .bind(new_state)
            .bind(result_summary)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| format!("close oxpilot action failed: {e}"))?
            .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE oxpilot_actions SET state = ?, result_summary = ?, finished_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
                 WHERE id = ? AND state IN ('claimed', 'running', 'cancel_requested')",
            )
            .bind(new_state)
            .bind(result_summary)
            .bind(now)
            .bind(action_id)
            .execute(p)
            .await
            .map_err(|e| format!("close oxpilot action failed: {e}"))?
            .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE oxpilot_actions SET state = ?2, result_summary = ?3, finished_at = ?4
                 WHERE id = ?1 AND state IN ('claimed', 'running', 'cancel_requested')",
            )
            .bind(action_id)
            .bind(new_state)
            .bind(result_summary)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| format!("close oxpilot action failed: {e}"))?
            .rows_affected()
        }
    };
    Ok(n)
}

/// The BR4.3 connector-touch proof — `claimed → running` on every
/// authenticated claim/heartbeat/report request (a delivered batch is
/// presumed executing once the connector proves it is alive again).
pub async fn flip_claimed_to_running(pool: &DbPool, target_id: &str) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE oxpilot_actions SET state = 'running' WHERE target_id = $1 AND state = 'claimed'")
                .bind(target_id)
                .execute(p)
                .await
                .map_err(|e| format!("flip oxpilot actions failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE oxpilot_actions SET state = 'running' WHERE target_id = ? AND state = 'claimed'")
                .bind(target_id)
                .execute(p)
                .await
                .map_err(|e| format!("flip oxpilot actions failed: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE oxpilot_actions SET state = 'running' WHERE target_id = ?1 AND state = 'claimed'")
                .bind(target_id)
                .execute(p)
                .await
                .map_err(|e| format!("flip oxpilot actions failed: {e}"))?
                .rows_affected()
        }
    };
    Ok(n)
}

/// The cancel set — ids of `cancel_requested` rows claim/heartbeat
/// deliver to the connector (kill-locally list).
pub async fn cancel_requested_ids(pool: &DbPool, target_id: &str) -> Result<Vec<String>, String> {
    let rows = match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar::<_, String>(
                "SELECT id FROM oxpilot_actions WHERE target_id = $1 AND state = 'cancel_requested' ORDER BY created_at, id",
            )
            .bind(target_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("read oxpilot cancel set failed: {e}"))?
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar::<_, String>(
                "SELECT id FROM oxpilot_actions WHERE target_id = ? AND state = 'cancel_requested' ORDER BY created_at, id",
            )
            .bind(target_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("read oxpilot cancel set failed: {e}"))?
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar::<_, String>(
                "SELECT id FROM oxpilot_actions WHERE target_id = ?1 AND state = 'cancel_requested' ORDER BY created_at, id",
            )
            .bind(target_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("read oxpilot cancel set failed: {e}"))?
        }
    };
    Ok(rows)
}

/// C8 hook #2's bulk close (BR6.5): in ONE transaction, the session's
/// `pending|allowed` grants and every non-terminal session action go
/// directly `cancelled` — connect grants (`session_id IS NULL`) are
/// untouched (BR6.6). Returns the resolved rows for post-commit
/// `session_deleted` audits; the subsequent session cascade removes the
/// rows. Idempotent — a re-run finds nothing live.
pub async fn cancel_session_connector_rows_tx(
    pool: &DbPool,
    session_id: &str,
    now: &str,
) -> Result<(Vec<OxpilotGrantRow>, Vec<OxpilotActionRow>), String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot session-connector cancel tx failed: {e}"))?;
            let grant_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {GRANT_COLS_PG} FROM oxpilot_grants
                 WHERE session_id = $1 AND state IN ('pending', 'allowed')"
            )))
            .bind(session_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select session oxpilot grants failed: {e}"))?;
            let mut grants: Vec<OxpilotGrantRow> = Vec::with_capacity(grant_rows.len());
            for r in &grant_rows {
                grants.push(map_grant!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_grants SET state = 'cancelled', resolved_at = $2::timestamptz
                 WHERE session_id = $1 AND state IN ('pending', 'allowed')",
            )
            .bind(session_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel session oxpilot grants failed: {e}"))?;
            let action_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_PG} FROM oxpilot_actions
                 WHERE session_id = $1 AND state IN ('queued', 'claimed', 'running', 'cancel_requested')"
            )))
            .bind(session_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select session oxpilot actions failed: {e}"))?;
            let mut actions: Vec<OxpilotActionRow> = Vec::with_capacity(action_rows.len());
            for r in &action_rows {
                actions.push(map_action!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = $2::timestamptz
                 WHERE session_id = $1 AND state IN ('queued', 'claimed', 'running', 'cancel_requested')",
            )
            .bind(session_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel session oxpilot actions failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit session-connector cancel failed: {e}"))?;
            Ok((grants, actions))
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot session-connector cancel tx failed: {e}"))?;
            let grant_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {GRANT_COLS_MYSQL} FROM oxpilot_grants
                 WHERE session_id = ? AND state IN ('pending', 'allowed')"
            )))
            .bind(session_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select session oxpilot grants failed: {e}"))?;
            let mut grants: Vec<OxpilotGrantRow> = Vec::with_capacity(grant_rows.len());
            for r in &grant_rows {
                grants.push(map_grant!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_grants SET state = 'cancelled', resolved_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
                 WHERE session_id = ? AND state IN ('pending', 'allowed')",
            )
            .bind(now)
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel session oxpilot grants failed: {e}"))?;
            let action_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_MYSQL} FROM oxpilot_actions
                 WHERE session_id = ? AND state IN ('queued', 'claimed', 'running', 'cancel_requested')"
            )))
            .bind(session_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select session oxpilot actions failed: {e}"))?;
            let mut actions: Vec<OxpilotActionRow> = Vec::with_capacity(action_rows.len());
            for r in &action_rows {
                actions.push(map_action!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')
                 WHERE session_id = ? AND state IN ('queued', 'claimed', 'running', 'cancel_requested')",
            )
            .bind(now)
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel session oxpilot actions failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit session-connector cancel failed: {e}"))?;
            Ok((grants, actions))
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin oxpilot session-connector cancel tx failed: {e}"))?;
            let grant_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {GRANT_COLS_SQLITE} FROM oxpilot_grants
                 WHERE session_id = ?1 AND state IN ('pending', 'allowed')"
            )))
            .bind(session_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select session oxpilot grants failed: {e}"))?;
            let mut grants: Vec<OxpilotGrantRow> = Vec::with_capacity(grant_rows.len());
            for r in &grant_rows {
                grants.push(map_grant!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_grants SET state = 'cancelled', resolved_at = ?2
                 WHERE session_id = ?1 AND state IN ('pending', 'allowed')",
            )
            .bind(session_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel session oxpilot grants failed: {e}"))?;
            let action_rows = sqlx::query(sqlx::AssertSqlSafe(&*format!(
                "SELECT {ACTION_COLS_SQLITE} FROM oxpilot_actions
                 WHERE session_id = ?1 AND state IN ('queued', 'claimed', 'running', 'cancel_requested')"
            )))
            .bind(session_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("select session oxpilot actions failed: {e}"))?;
            let mut actions: Vec<OxpilotActionRow> = Vec::with_capacity(action_rows.len());
            for r in &action_rows {
                actions.push(map_action!(r));
            }
            sqlx::query(
                "UPDATE oxpilot_actions SET state = 'cancelled', finished_at = ?2
                 WHERE session_id = ?1 AND state IN ('queued', 'claimed', 'running', 'cancel_requested')",
            )
            .bind(session_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("cancel session oxpilot actions failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit session-connector cancel failed: {e}"))?;
            Ok((grants, actions))
        }
    }
}
