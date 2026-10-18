//! Milestones + per-repo `#N` counters — mirrors `issues.rs` conventions.
//! `due_on` is a `YYYY-MM-DD` calendar date on the wire (GitHub's milestone
//! API shape); stored as TEXT on sqlite, TIMESTAMPTZ/DATETIME elsewhere and
//! normalized back to `YYYY-MM-DD` on read.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct MilestoneRow {
    pub id: String,
    pub repo_id: String,
    pub number: i64,
    pub title: String,
    pub description: String,
    pub state: String,
    /// `YYYY-MM-DD` when set.
    pub due_on: Option<String>,
    pub creator_id: Option<String>,
    pub closed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Derived `issues` counts carried by every `MILESTONE_SELECT_*` query.
    pub open_issues: i64,
    pub closed_issues: i64,
}

macro_rules! map_milestone {
    ($row:expr) => {{
        let row = $row;
        MilestoneRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("milestone row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("milestone row: {e}"))?,
            number: row
                .try_get::<i64, _>("number")
                .map_err(|e| format!("milestone row: {e}"))?,
            title: row
                .try_get("title")
                .map_err(|e| format!("milestone row: {e}"))?,
            description: row
                .try_get("description")
                .map_err(|e| format!("milestone row: {e}"))?,
            state: row
                .try_get("state")
                .map_err(|e| format!("milestone row: {e}"))?,
            due_on: row
                .try_get("due_on")
                .map_err(|e| format!("milestone row: {e}"))?,
            creator_id: row
                .try_get("creator_id")
                .map_err(|e| format!("milestone row: {e}"))?,
            closed_at: row
                .try_get("closed_at")
                .map_err(|e| format!("milestone row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("milestone row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("milestone row: {e}"))?,
            open_issues: row
                .try_get::<i64, _>("open_issues")
                .or_else(|_| row.try_get::<i32, _>("open_issues").map(i64::from))
                .map_err(|e| format!("milestone row: {e}"))?,
            closed_issues: row
                .try_get::<i64, _>("closed_issues")
                .or_else(|_| row.try_get::<i32, _>("closed_issues").map(i64::from))
                .map_err(|e| format!("milestone row: {e}"))?,
        }
    }};
}

const MILESTONE_SELECT_PG: &str = "SELECT m.id, m.repo_id, m.number, m.title, m.description, m.state,
       CASE WHEN m.due_on IS NULL THEN NULL
            ELSE to_char(m.due_on AT TIME ZONE 'UTC', 'YYYY-MM-DD') END AS due_on,
       m.creator_id,
       CASE WHEN m.closed_at IS NULL THEN NULL
            ELSE to_char(m.closed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS closed_at,
       to_char(m.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(m.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at,
       (SELECT COUNT(*)::bigint FROM issues i WHERE i.milestone_id = m.id AND i.state = 'open') AS open_issues,
       (SELECT COUNT(*)::bigint FROM issues i WHERE i.milestone_id = m.id AND i.state = 'closed') AS closed_issues
FROM milestones m";

const MILESTONE_SELECT_MYSQL: &str = "SELECT m.id, m.repo_id, m.number, m.title, m.description, m.state,
       DATE_FORMAT(m.due_on, '%Y-%m-%d') AS due_on,
       m.creator_id,
       DATE_FORMAT(m.closed_at, '%Y-%m-%dT%H:%i:%sZ') AS closed_at,
       DATE_FORMAT(m.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(m.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at,
       (SELECT COUNT(*) FROM issues i WHERE i.milestone_id = m.id AND i.state = 'open') AS open_issues,
       (SELECT COUNT(*) FROM issues i WHERE i.milestone_id = m.id AND i.state = 'closed') AS closed_issues
FROM milestones m";

const MILESTONE_SELECT_SQLITE: &str = "SELECT m.id, m.repo_id, m.number, m.title, m.description, m.state,
       CASE WHEN m.due_on IS NULL THEN NULL ELSE substr(m.due_on, 1, 10) END AS due_on,
       m.creator_id,
       CASE WHEN m.closed_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', m.closed_at) END AS closed_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', m.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', m.updated_at) AS updated_at,
       (SELECT COUNT(*) FROM issues i WHERE i.milestone_id = m.id AND i.state = 'open') AS open_issues,
       (SELECT COUNT(*) FROM issues i WHERE i.milestone_id = m.id AND i.state = 'closed') AS closed_issues
FROM milestones m";

/// Allocate the next per-repo milestone number (monotonic; never reclaims).
pub async fn allocate_next_number(pool: &DbPool, repo_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin allocate milestone number tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO milestone_counters (repo_id, max_number) VALUES ($1, 0)
                 ON CONFLICT (repo_id) DO NOTHING",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure milestone_counters failed: {e}"))?;
            let number: i64 = sqlx::query_scalar(
                "UPDATE milestone_counters SET max_number = max_number + 1
                 WHERE repo_id = $1 RETURNING max_number",
            )
            .bind(repo_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("allocate milestone number failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit allocate milestone number tx failed: {e}"))?;
            Ok(number)
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin allocate milestone number tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO milestone_counters (repo_id, max_number) VALUES (?, 0)
                 ON DUPLICATE KEY UPDATE repo_id = repo_id",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure milestone_counters failed: {e}"))?;
            sqlx::query(
                "UPDATE milestone_counters SET max_number = max_number + 1 WHERE repo_id = ?",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("allocate milestone number failed: {e}"))?;
            let number: i64 =
                sqlx::query_scalar("SELECT max_number FROM milestone_counters WHERE repo_id = ?")
                    .bind(repo_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| format!("read allocated milestone number failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit allocate milestone number tx failed: {e}"))?;
            Ok(number)
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin allocate milestone number tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO milestone_counters (repo_id, max_number) VALUES (?1, 0)
                 ON CONFLICT (repo_id) DO NOTHING",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure milestone_counters failed: {e}"))?;
            let number: i64 = sqlx::query_scalar(
                "UPDATE milestone_counters SET max_number = max_number + 1
                 WHERE repo_id = ?1 RETURNING max_number",
            )
            .bind(repo_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("allocate milestone number failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit allocate milestone number tx failed: {e}"))?;
            Ok(number)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_milestone(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    number: i64,
    title: &str,
    description: &str,
    due_on: Option<&str>,
    creator_id: Option<&str>,
) -> Result<MilestoneRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO milestones (id, repo_id, number, title, description, due_on, creator_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(description)
            .bind(due_on)
            .bind(creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert milestone failed: {e}"))?;
            let sql = format!("{MILESTONE_SELECT_PG} WHERE m.id = $1");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("reload milestone failed: {e}"))?;
            Ok(map_milestone!(&row))
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO milestones (id, repo_id, number, title, description, due_on, creator_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(description)
            .bind(due_on)
            .bind(creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert milestone failed: {e}"))?;
            let sql = format!("{MILESTONE_SELECT_MYSQL} WHERE m.id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("reload milestone failed: {e}"))?;
            Ok(map_milestone!(&row))
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO milestones (id, repo_id, number, title, description, due_on, creator_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(description)
            .bind(due_on)
            .bind(creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert milestone failed: {e}"))?;
            let sql = format!("{MILESTONE_SELECT_SQLITE} WHERE m.id = ?1");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("reload milestone failed: {e}"))?;
            Ok(map_milestone!(&row))
        }
    }
}

pub async fn find_milestone_by_id(pool: &DbPool, id: &str) -> Result<Option<MilestoneRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{MILESTONE_SELECT_PG} WHERE m.id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find milestone failed: {e}"))?
            {
                Some(r) => Ok(Some(map_milestone!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{MILESTONE_SELECT_MYSQL} WHERE m.id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find milestone failed: {e}"))?
            {
                Some(r) => Ok(Some(map_milestone!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{MILESTONE_SELECT_SQLITE} WHERE m.id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find milestone failed: {e}"))?
            {
                Some(r) => Ok(Some(map_milestone!(&r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn find_milestone_by_repo_number(
    pool: &DbPool,
    repo_id: &str,
    number: i64,
) -> Result<Option<MilestoneRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{MILESTONE_SELECT_PG} WHERE m.repo_id = $1 AND m.number = $2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find milestone failed: {e}"))?
            {
                Some(r) => Ok(Some(map_milestone!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{MILESTONE_SELECT_MYSQL} WHERE m.repo_id = ? AND m.number = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find milestone failed: {e}"))?
            {
                Some(r) => Ok(Some(map_milestone!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{MILESTONE_SELECT_SQLITE} WHERE m.repo_id = ?1 AND m.number = ?2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find milestone failed: {e}"))?
            {
                Some(r) => Ok(Some(map_milestone!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Case-insensitive title match within a repo — backs the create/update
/// duplicate check (the unique index enforces races).
pub async fn find_milestone_by_repo_title(
    pool: &DbPool,
    repo_id: &str,
    title: &str,
) -> Result<Option<MilestoneRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{MILESTONE_SELECT_PG} WHERE m.repo_id = $1 AND lower(m.title) = lower($2)"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(title)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find milestone by title failed: {e}"))?
            {
                Some(r) => Ok(Some(map_milestone!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{MILESTONE_SELECT_MYSQL} WHERE m.repo_id = ? AND m.title_lower = lower(?)"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(title)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find milestone by title failed: {e}"))?
            {
                Some(r) => Ok(Some(map_milestone!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{MILESTONE_SELECT_SQLITE} WHERE m.repo_id = ?1 AND lower(m.title) = lower(?2)"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(title)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find milestone by title failed: {e}"))?
            {
                Some(r) => Ok(Some(map_milestone!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// List milestones — `state` is `open`/`closed`/`all`. GitHub's default sort is
/// soonest due date (milestones without a due date last), then number.
pub async fn list_milestones_for_repo(
    pool: &DbPool,
    repo_id: &str,
    state: &str,
) -> Result<Vec<MilestoneRow>, String> {
    let filtered = matches!(state, "open" | "closed");
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{MILESTONE_SELECT_PG} WHERE m.repo_id = $1 {} ORDER BY (m.due_on IS NULL), m.due_on ASC, m.number ASC",
                if filtered { "AND m.state = $2" } else { "" }
            );
            let mut qb = sqlx::query(sqlx::AssertSqlSafe(&*sql)).bind(repo_id);
            if filtered {
                qb = qb.bind(state);
            }
            let rows = qb
                .fetch_all(p)
                .await
                .map_err(|e| format!("list milestones failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_milestone!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{MILESTONE_SELECT_MYSQL} WHERE m.repo_id = ? {} ORDER BY (m.due_on IS NULL), m.due_on ASC, m.number ASC",
                if filtered { "AND m.state = ?" } else { "" }
            );
            let mut qb = sqlx::query(sqlx::AssertSqlSafe(&*sql)).bind(repo_id);
            if filtered {
                qb = qb.bind(state);
            }
            let rows = qb
                .fetch_all(p)
                .await
                .map_err(|e| format!("list milestones failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_milestone!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{MILESTONE_SELECT_SQLITE} WHERE m.repo_id = ?1 {} ORDER BY (m.due_on IS NULL), m.due_on ASC, m.number ASC",
                if filtered { "AND m.state = ?2" } else { "" }
            );
            let mut qb = sqlx::query(sqlx::AssertSqlSafe(&*sql)).bind(repo_id);
            if filtered {
                qb = qb.bind(state);
            }
            let rows = qb
                .fetch_all(p)
                .await
                .map_err(|e| format!("list milestones failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_milestone!(r));
            }
            Ok(out)
        }
    }
}

/// Batch lookup for issue enrichment — returns rows keyed by milestone id.
pub async fn list_milestones_by_ids(
    pool: &DbPool,
    ids: &[String],
) -> Result<Vec<MilestoneRow>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    match pool {
        DbPool::Postgres(p) => {
            let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(MILESTONE_SELECT_PG);
            qb.push(" WHERE m.id IN (");
            let mut sep = qb.separated(", ");
            for id in ids {
                sep.push_bind(id);
            }
            qb.push(")");
            let rows = qb
                .build()
                .fetch_all(p)
                .await
                .map_err(|e| format!("list milestones by ids failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_milestone!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let mut qb = sqlx::QueryBuilder::<sqlx::MySql>::new(MILESTONE_SELECT_MYSQL);
            qb.push(" WHERE m.id IN (");
            let mut sep = qb.separated(", ");
            for id in ids {
                sep.push_bind(id);
            }
            qb.push(")");
            let rows = qb
                .build()
                .fetch_all(p)
                .await
                .map_err(|e| format!("list milestones by ids failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_milestone!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(MILESTONE_SELECT_SQLITE);
            qb.push(" WHERE m.id IN (");
            let mut sep = qb.separated(", ");
            for id in ids {
                sep.push_bind(id);
            }
            qb.push(")");
            let rows = qb
                .build()
                .fetch_all(p)
                .await
                .map_err(|e| format!("list milestones by ids failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_milestone!(r));
            }
            Ok(out)
        }
    }
}

/// `updated_at` stamp per dialect — a literal folded into the SET list.
fn now_expr(pool: &DbPool) -> &'static str {
    match pool {
        DbPool::Sqlite(_) => "strftime('%Y-%m-%d %H:%M:%S','now')",
        _ => "CURRENT_TIMESTAMP",
    }
}

/// Edit title/description/due_on — `due_on` `Some(None)` clears the date.
/// Returns the updated row, or `None` when the milestone vanished mid-update.
pub async fn update_milestone(
    pool: &DbPool,
    id: &str,
    title: Option<&str>,
    description: Option<&str>,
    due_on: Option<Option<&str>>,
) -> Result<Option<MilestoneRow>, String> {
    let now = now_expr(pool);
    macro_rules! build {
        ($ty:ty) => {{
            let mut qb = sqlx::QueryBuilder::<$ty>::new("UPDATE milestones SET ");
            {
                let mut sep = qb.separated(", ");
                if let Some(t) = title {
                    sep.push("title = ").push_bind_unseparated(t);
                }
                if let Some(d) = description {
                    sep.push("description = ").push_bind_unseparated(d);
                }
                if let Some(d) = due_on {
                    sep.push("due_on = ").push_bind_unseparated(d);
                }
                sep.push(format!("updated_at = {now}"));
            }
            qb.push(" WHERE id = ").push_bind(id);
            qb
        }};
    }
    match pool {
        DbPool::Postgres(p) => {
            build!(sqlx::Postgres)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update milestone failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            build!(sqlx::MySql)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update milestone failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            build!(sqlx::Sqlite)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update milestone failed: {e}"))?;
        }
    }
    find_milestone_by_id(pool, id).await
}

/// Open/close a milestone — closing stamps `closed_at`, reopening clears it.
/// Returns the updated row, or `None` when the milestone vanished mid-update.
pub async fn set_milestone_state(
    pool: &DbPool,
    id: &str,
    state: &str,
) -> Result<Option<MilestoneRow>, String> {
    let now = now_expr(pool);
    let closed_expr = if state == "closed" { now } else { "NULL" };
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "UPDATE milestones SET state = $1, closed_at = {closed_expr}, updated_at = {now} WHERE id = $2"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set milestone state failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "UPDATE milestones SET state = ?, closed_at = {closed_expr}, updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set milestone state failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "UPDATE milestones SET state = ?1, closed_at = {closed_expr}, updated_at = {now} WHERE id = ?2"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set milestone state failed: {e}"))?;
        }
    }
    find_milestone_by_id(pool, id).await
}

/// Hard-delete — `issues.milestone_id` clears via `ON DELETE SET NULL`.
/// Numbers are not reclaimed.
pub async fn delete_milestone(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM milestones WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete milestone failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM milestones WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete milestone failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM milestones WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete milestone failed: {e}"))?;
        }
    }
    Ok(())
}

/// Set/clear an issue's milestone (`None` clears).
pub async fn set_issue_milestone(
    pool: &DbPool,
    issue_id: &str,
    milestone_id: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE issues SET milestone_id = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2",
            )
            .bind(milestone_id)
            .bind(issue_id)
            .execute(p)
            .await
            .map_err(|e| format!("set issue milestone failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE issues SET milestone_id = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(milestone_id)
            .bind(issue_id)
            .execute(p)
            .await
            .map_err(|e| format!("set issue milestone failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE issues SET milestone_id = ?1, updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?2",
            )
            .bind(milestone_id)
            .bind(issue_id)
            .execute(p)
            .await
            .map_err(|e| format!("set issue milestone failed: {e}"))?;
        }
    }
    Ok(())
}
