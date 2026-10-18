//! Classic project boards — repo-scoped `projects` (per-repo `#N` counters
//! like milestones), `project_columns`, and `project_cards`. Positions are
//! contiguous 0-based integers inside their parent (column per board, card
//! per column); insert appends, delete/move re-pack so list order is stable.
//! Cards are notes (`note` set) or linked entities (`content_type` +
//! `content_id`); `convert` swaps a note for an issue link.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: String,
    pub repo_id: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub creator_id: Option<String>,
    pub closed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ProjectColumnRow {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ProjectCardRow {
    pub id: String,
    pub column_id: String,
    pub note: Option<String>,
    pub content_type: Option<String>,
    pub content_id: Option<String>,
    pub position: i64,
    pub archived: bool,
    pub creator_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_project {
    ($row:expr) => {{
        let row = $row;
        ProjectRow {
            id: row.try_get("id").map_err(|e| format!("project row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("project row: {e}"))?,
            number: row
                .try_get::<i64, _>("number")
                .map_err(|e| format!("project row: {e}"))?,
            title: row
                .try_get("title")
                .map_err(|e| format!("project row: {e}"))?,
            body: row
                .try_get("body")
                .map_err(|e| format!("project row: {e}"))?,
            state: row
                .try_get("state")
                .map_err(|e| format!("project row: {e}"))?,
            creator_id: row
                .try_get("creator_id")
                .map_err(|e| format!("project row: {e}"))?,
            closed_at: row
                .try_get("closed_at")
                .map_err(|e| format!("project row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("project row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("project row: {e}"))?,
        }
    }};
}

macro_rules! map_column {
    ($row:expr) => {{
        let row = $row;
        ProjectColumnRow {
            id: row.try_get("id").map_err(|e| format!("column row: {e}"))?,
            project_id: row
                .try_get("project_id")
                .map_err(|e| format!("column row: {e}"))?,
            name: row
                .try_get("name")
                .map_err(|e| format!("column row: {e}"))?,
            position: row
                .try_get::<i64, _>("position")
                .or_else(|_| row.try_get::<i32, _>("position").map(i64::from))
                .map_err(|e| format!("column row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("column row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("column row: {e}"))?,
        }
    }};
}

macro_rules! map_card {
    ($row:expr) => {{
        let row = $row;
        ProjectCardRow {
            id: row.try_get("id").map_err(|e| format!("card row: {e}"))?,
            column_id: row
                .try_get("column_id")
                .map_err(|e| format!("card row: {e}"))?,
            note: row.try_get("note").map_err(|e| format!("card row: {e}"))?,
            content_type: row
                .try_get("content_type")
                .map_err(|e| format!("card row: {e}"))?,
            content_id: row
                .try_get("content_id")
                .map_err(|e| format!("card row: {e}"))?,
            position: row
                .try_get::<i64, _>("position")
                .or_else(|_| row.try_get::<i32, _>("position").map(i64::from))
                .map_err(|e| format!("card row: {e}"))?,
            archived: row
                .try_get::<bool, _>("archived")
                .or_else(|_| row.try_get::<i64, _>("archived").map(|v| v != 0))
                .or_else(|_| row.try_get::<i8, _>("archived").map(|v| v != 0))
                .map_err(|e| format!("card row: {e}"))?,
            creator_id: row
                .try_get("creator_id")
                .map_err(|e| format!("card row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("card row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("card row: {e}"))?,
        }
    }};
}

const PROJECT_SELECT_PG: &str = "SELECT p.id, p.repo_id, p.number, p.title, p.body, p.state,
       p.creator_id,
       CASE WHEN p.closed_at IS NULL THEN NULL
            ELSE to_char(p.closed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS closed_at,
       to_char(p.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(p.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM projects p";

const PROJECT_SELECT_MYSQL: &str = "SELECT p.id, p.repo_id, p.number, p.title, p.body, p.state,
       p.creator_id,
       DATE_FORMAT(p.closed_at, '%Y-%m-%dT%H:%i:%sZ') AS closed_at,
       DATE_FORMAT(p.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(p.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM projects p";

const PROJECT_SELECT_SQLITE: &str = "SELECT p.id, p.repo_id, p.number, p.title, p.body, p.state,
       p.creator_id,
       CASE WHEN p.closed_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', p.closed_at) END AS closed_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', p.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', p.updated_at) AS updated_at
FROM projects p";

const COLUMN_SELECT_PG: &str = "SELECT c.id, c.project_id, c.name, c.position,
       to_char(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(c.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM project_columns c";

const COLUMN_SELECT_MYSQL: &str = "SELECT c.id, c.project_id, c.name, c.position,
       DATE_FORMAT(c.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(c.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM project_columns c";

const COLUMN_SELECT_SQLITE: &str = "SELECT c.id, c.project_id, c.name, c.position,
       strftime('%Y-%m-%dT%H:%M:%SZ', c.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', c.updated_at) AS updated_at
FROM project_columns c";

const CARD_SELECT_PG: &str = "SELECT k.id, k.column_id, k.note, k.content_type, k.content_id,
       k.position, k.archived, k.creator_id,
       to_char(k.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(k.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM project_cards k";

const CARD_SELECT_MYSQL: &str = "SELECT k.id, k.column_id, k.note, k.content_type, k.content_id,
       k.position, k.archived, k.creator_id,
       DATE_FORMAT(k.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(k.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM project_cards k";

const CARD_SELECT_SQLITE: &str = "SELECT k.id, k.column_id, k.note, k.content_type, k.content_id,
       k.position, k.archived, k.creator_id,
       strftime('%Y-%m-%dT%H:%M:%SZ', k.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', k.updated_at) AS updated_at
FROM project_cards k";

/// `updated_at` stamp per dialect — a literal folded into the SET list.
fn now_expr(pool: &DbPool) -> &'static str {
    match pool {
        DbPool::Sqlite(_) => "strftime('%Y-%m-%d %H:%M:%S','now')",
        _ => "CURRENT_TIMESTAMP",
    }
}

// ---------------------------------------------------------------------------
// Boards
// ---------------------------------------------------------------------------

/// Allocate the next per-repo project number (monotonic; never reclaims).
pub async fn allocate_next_project_number(pool: &DbPool, repo_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin allocate project number tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO project_counters (repo_id, max_number) VALUES ($1, 0)
                 ON CONFLICT (repo_id) DO NOTHING",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure project_counters failed: {e}"))?;
            let number: i64 = sqlx::query_scalar(
                "UPDATE project_counters SET max_number = max_number + 1
                 WHERE repo_id = $1 RETURNING max_number",
            )
            .bind(repo_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("allocate project number failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit allocate project number tx failed: {e}"))?;
            Ok(number)
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin allocate project number tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO project_counters (repo_id, max_number) VALUES (?, 0)
                 ON DUPLICATE KEY UPDATE repo_id = repo_id",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure project_counters failed: {e}"))?;
            sqlx::query(
                "UPDATE project_counters SET max_number = max_number + 1 WHERE repo_id = ?",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("allocate project number failed: {e}"))?;
            let number: i64 =
                sqlx::query_scalar("SELECT max_number FROM project_counters WHERE repo_id = ?")
                    .bind(repo_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| format!("read project number failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit allocate project number tx failed: {e}"))?;
            Ok(number)
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin allocate project number tx failed: {e}"))?;
            sqlx::query(
                "INSERT OR IGNORE INTO project_counters (repo_id, max_number) VALUES (?1, 0)",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure project_counters failed: {e}"))?;
            sqlx::query(
                "UPDATE project_counters SET max_number = max_number + 1 WHERE repo_id = ?1",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("allocate project number failed: {e}"))?;
            let number: i64 =
                sqlx::query_scalar("SELECT max_number FROM project_counters WHERE repo_id = ?1")
                    .bind(repo_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| format!("read project number failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit allocate project number tx failed: {e}"))?;
            Ok(number)
        }
    }
}

pub async fn insert_project(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    number: i64,
    title: &str,
    body: &str,
    creator_id: Option<&str>,
) -> Result<ProjectRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO projects (id, repo_id, number, title, body, creator_id)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(body)
            .bind(creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert project failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO projects (id, repo_id, number, title, body, creator_id)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(body)
            .bind(creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert project failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO projects (id, repo_id, number, title, body, creator_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(body)
            .bind(creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert project failed: {e}"))?;
        }
    }
    find_project_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert project: row missing after insert".to_string())
}

pub async fn find_project_by_id(pool: &DbPool, id: &str) -> Result<Option<ProjectRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{PROJECT_SELECT_PG} WHERE p.id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project failed: {e}"))?
            {
                Some(r) => Ok(Some(map_project!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{PROJECT_SELECT_MYSQL} WHERE p.id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project failed: {e}"))?
            {
                Some(r) => Ok(Some(map_project!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{PROJECT_SELECT_SQLITE} WHERE p.id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project failed: {e}"))?
            {
                Some(r) => Ok(Some(map_project!(&r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn find_project_by_repo_number(
    pool: &DbPool,
    repo_id: &str,
    number: i64,
) -> Result<Option<ProjectRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{PROJECT_SELECT_PG} WHERE p.repo_id = $1 AND p.number = $2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project failed: {e}"))?
            {
                Some(r) => Ok(Some(map_project!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{PROJECT_SELECT_MYSQL} WHERE p.repo_id = ? AND p.number = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project failed: {e}"))?
            {
                Some(r) => Ok(Some(map_project!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{PROJECT_SELECT_SQLITE} WHERE p.repo_id = ?1 AND p.number = ?2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project failed: {e}"))?
            {
                Some(r) => Ok(Some(map_project!(&r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn list_projects_for_repo(
    pool: &DbPool,
    repo_id: &str,
    state: &str,
) -> Result<Vec<ProjectRow>, String> {
    let filtered = matches!(state, "open" | "closed");
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{PROJECT_SELECT_PG} WHERE p.repo_id = $1 {} ORDER BY p.number ASC",
                if filtered { "AND p.state = $2" } else { "" }
            );
            let mut qb = sqlx::query(sqlx::AssertSqlSafe(&*sql)).bind(repo_id);
            if filtered {
                qb = qb.bind(state);
            }
            let rows = qb
                .fetch_all(p)
                .await
                .map_err(|e| format!("list projects failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_project!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{PROJECT_SELECT_MYSQL} WHERE p.repo_id = ? {} ORDER BY p.number ASC",
                if filtered { "AND p.state = ?" } else { "" }
            );
            let mut qb = sqlx::query(sqlx::AssertSqlSafe(&*sql)).bind(repo_id);
            if filtered {
                qb = qb.bind(state);
            }
            let rows = qb
                .fetch_all(p)
                .await
                .map_err(|e| format!("list projects failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_project!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{PROJECT_SELECT_SQLITE} WHERE p.repo_id = ?1 {} ORDER BY p.number ASC",
                if filtered { "AND p.state = ?2" } else { "" }
            );
            let mut qb = sqlx::query(sqlx::AssertSqlSafe(&*sql)).bind(repo_id);
            if filtered {
                qb = qb.bind(state);
            }
            let rows = qb
                .fetch_all(p)
                .await
                .map_err(|e| format!("list projects failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_project!(r));
            }
            Ok(out)
        }
    }
}

/// Edit title/body. Returns the updated row, or `None` when the project
/// vanished mid-update.
pub async fn update_project(
    pool: &DbPool,
    id: &str,
    title: Option<&str>,
    body: Option<&str>,
) -> Result<Option<ProjectRow>, String> {
    let now = now_expr(pool);
    macro_rules! build {
        ($ty:ty) => {{
            let mut qb = sqlx::QueryBuilder::<$ty>::new("UPDATE projects SET ");
            {
                let mut sep = qb.separated(", ");
                if let Some(t) = title {
                    sep.push("title = ").push_bind_unseparated(t);
                }
                if let Some(b) = body {
                    sep.push("body = ").push_bind_unseparated(b);
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
                .map_err(|e| format!("update project failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            build!(sqlx::MySql)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update project failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            build!(sqlx::Sqlite)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update project failed: {e}"))?;
        }
    }
    find_project_by_id(pool, id).await
}

/// Open/close a board — closing stamps `closed_at`, reopening clears it.
pub async fn set_project_state(
    pool: &DbPool,
    id: &str,
    state: &str,
) -> Result<Option<ProjectRow>, String> {
    let now = now_expr(pool);
    let closed_expr = if state == "closed" { now } else { "NULL" };
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "UPDATE projects SET state = $1, closed_at = {closed_expr}, updated_at = {now} WHERE id = $2"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set project state failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "UPDATE projects SET state = ?, closed_at = {closed_expr}, updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set project state failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "UPDATE projects SET state = ?1, closed_at = {closed_expr}, updated_at = {now} WHERE id = ?2"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set project state failed: {e}"))?;
        }
    }
    find_project_by_id(pool, id).await
}

/// Hard-delete — columns and cards cascade (`ON DELETE CASCADE`).
/// Numbers are not reclaimed.
pub async fn delete_project(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM projects WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete project failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM projects WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete project failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM projects WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete project failed: {e}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Columns
// ---------------------------------------------------------------------------

pub async fn find_column_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<ProjectColumnRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{COLUMN_SELECT_PG} WHERE c.id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project column failed: {e}"))?
            {
                Some(r) => Ok(Some(map_column!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{COLUMN_SELECT_MYSQL} WHERE c.id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project column failed: {e}"))?
            {
                Some(r) => Ok(Some(map_column!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{COLUMN_SELECT_SQLITE} WHERE c.id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project column failed: {e}"))?
            {
                Some(r) => Ok(Some(map_column!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Column lookup scoped to a board — cross-board ids don't resolve.
pub async fn find_column_in_project(
    pool: &DbPool,
    project_id: &str,
    id: &str,
) -> Result<Option<ProjectColumnRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{COLUMN_SELECT_PG} WHERE c.id = $1 AND c.project_id = $2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .bind(project_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project column failed: {e}"))?
            {
                Some(r) => Ok(Some(map_column!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{COLUMN_SELECT_MYSQL} WHERE c.id = ? AND c.project_id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .bind(project_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project column failed: {e}"))?
            {
                Some(r) => Ok(Some(map_column!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{COLUMN_SELECT_SQLITE} WHERE c.id = ?1 AND c.project_id = ?2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .bind(project_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project column failed: {e}"))?
            {
                Some(r) => Ok(Some(map_column!(&r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn list_columns_for_project(
    pool: &DbPool,
    project_id: &str,
) -> Result<Vec<ProjectColumnRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{COLUMN_SELECT_PG} WHERE c.project_id = $1 ORDER BY c.position ASC");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(project_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list project columns failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_column!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql =
                format!("{COLUMN_SELECT_MYSQL} WHERE c.project_id = ? ORDER BY c.position ASC");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(project_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list project columns failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_column!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql =
                format!("{COLUMN_SELECT_SQLITE} WHERE c.project_id = ?1 ORDER BY c.position ASC");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(project_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list project columns failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_column!(r));
            }
            Ok(out)
        }
    }
}

/// Append a column — position lands after the current last slot.
pub async fn insert_project_column(
    pool: &DbPool,
    id: &str,
    project_id: &str,
    name: &str,
) -> Result<ProjectColumnRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO project_columns (id, project_id, name, position)
                 VALUES ($1, $2, $3,
                    (SELECT COALESCE(MAX(position) + 1, 0) FROM project_columns WHERE project_id = $2))",
            )
            .bind(id)
            .bind(project_id)
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| format!("insert project column failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO project_columns (id, project_id, name, position)
                 SELECT ?, ?, ?, COALESCE(MAX(position) + 1, 0) FROM project_columns WHERE project_id = ?",
            )
            .bind(id)
            .bind(project_id)
            .bind(name)
            .bind(project_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert project column failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO project_columns (id, project_id, name, position)
                 VALUES (?1, ?2, ?3,
                    (SELECT COALESCE(MAX(position) + 1, 0) FROM project_columns WHERE project_id = ?2))",
            )
            .bind(id)
            .bind(project_id)
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| format!("insert project column failed: {e}"))?;
        }
    }
    find_column_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert project column: row missing after insert".to_string())
}

/// Rename a column. Returns the updated row or `None` when it vanished.
pub async fn update_project_column(
    pool: &DbPool,
    id: &str,
    name: &str,
) -> Result<Option<ProjectColumnRow>, String> {
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let sql =
                format!("UPDATE project_columns SET name = $1, updated_at = {now} WHERE id = $2");
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(name)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update project column failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql =
                format!("UPDATE project_columns SET name = ?, updated_at = {now} WHERE id = ?");
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(name)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update project column failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql =
                format!("UPDATE project_columns SET name = ?1, updated_at = {now} WHERE id = ?2");
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(name)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("update project column failed: {e}"))?;
        }
    }
    find_column_by_id(pool, id).await
}

/// Re-order a column inside its board. `new_position` clamps to
/// `0..=len-1`; siblings shift to stay contiguous.
pub async fn move_project_column(
    pool: &DbPool,
    id: &str,
    new_position: i64,
) -> Result<Option<ProjectColumnRow>, String> {
    let Some(col) = find_column_by_id(pool, id).await? else {
        return Ok(None);
    };
    let siblings = list_columns_for_project(pool, &col.project_id).await?;
    let len = siblings.len() as i64;
    let target = new_position.clamp(0, len - 1);
    if target == col.position {
        return Ok(Some(col));
    }
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin move column tx failed: {e}"))?;
            if target < col.position {
                sqlx::query(
                    "UPDATE project_columns SET position = position + 1
                     WHERE project_id = $1 AND position >= $2 AND position < $3",
                )
                .bind(&col.project_id)
                .bind(target)
                .bind(col.position)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("shift project columns failed: {e}"))?;
            } else {
                sqlx::query(
                    "UPDATE project_columns SET position = position - 1
                     WHERE project_id = $1 AND position <= $2 AND position > $3",
                )
                .bind(&col.project_id)
                .bind(target)
                .bind(col.position)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("shift project columns failed: {e}"))?;
            }
            let sql = format!(
                "UPDATE project_columns SET position = $1, updated_at = {now} WHERE id = $2"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("move project column failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit move column tx failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin move column tx failed: {e}"))?;
            if target < col.position {
                sqlx::query(
                    "UPDATE project_columns SET position = position + 1
                     WHERE project_id = ? AND position >= ? AND position < ?",
                )
                .bind(&col.project_id)
                .bind(target)
                .bind(col.position)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("shift project columns failed: {e}"))?;
            } else {
                sqlx::query(
                    "UPDATE project_columns SET position = position - 1
                     WHERE project_id = ? AND position <= ? AND position > ?",
                )
                .bind(&col.project_id)
                .bind(target)
                .bind(col.position)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("shift project columns failed: {e}"))?;
            }
            let sql =
                format!("UPDATE project_columns SET position = ?, updated_at = {now} WHERE id = ?");
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("move project column failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit move column tx failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin move column tx failed: {e}"))?;
            if target < col.position {
                sqlx::query(
                    "UPDATE project_columns SET position = position + 1
                     WHERE project_id = ?1 AND position >= ?2 AND position < ?3",
                )
                .bind(&col.project_id)
                .bind(target)
                .bind(col.position)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("shift project columns failed: {e}"))?;
            } else {
                sqlx::query(
                    "UPDATE project_columns SET position = position - 1
                     WHERE project_id = ?1 AND position <= ?2 AND position > ?3",
                )
                .bind(&col.project_id)
                .bind(target)
                .bind(col.position)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("shift project columns failed: {e}"))?;
            }
            let sql = format!(
                "UPDATE project_columns SET position = ?1, updated_at = {now} WHERE id = ?2"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(target)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("move project column failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit move column tx failed: {e}"))?;
        }
    }
    find_column_by_id(pool, id).await
}

/// Delete a column — cards cascade; higher positions re-pack down.
pub async fn delete_project_column(pool: &DbPool, id: &str) -> Result<(), String> {
    let Some(col) = find_column_by_id(pool, id).await? else {
        return Ok(());
    };
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin delete column tx failed: {e}"))?;
            sqlx::query("DELETE FROM project_columns WHERE id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("delete project column failed: {e}"))?;
            sqlx::query(
                "UPDATE project_columns SET position = position - 1
                 WHERE project_id = $1 AND position > $2",
            )
            .bind(&col.project_id)
            .bind(col.position)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("repack project columns failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit delete column tx failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin delete column tx failed: {e}"))?;
            sqlx::query("DELETE FROM project_columns WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("delete project column failed: {e}"))?;
            sqlx::query(
                "UPDATE project_columns SET position = position - 1
                 WHERE project_id = ? AND position > ?",
            )
            .bind(&col.project_id)
            .bind(col.position)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("repack project columns failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit delete column tx failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin delete column tx failed: {e}"))?;
            sqlx::query("DELETE FROM project_columns WHERE id = ?1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("delete project column failed: {e}"))?;
            sqlx::query(
                "UPDATE project_columns SET position = position - 1
                 WHERE project_id = ?1 AND position > ?2",
            )
            .bind(&col.project_id)
            .bind(col.position)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("repack project columns failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit delete column tx failed: {e}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------

pub async fn find_card_by_id(pool: &DbPool, id: &str) -> Result<Option<ProjectCardRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{CARD_SELECT_PG} WHERE k.id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project card failed: {e}"))?
            {
                Some(r) => Ok(Some(map_card!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{CARD_SELECT_MYSQL} WHERE k.id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project card failed: {e}"))?
            {
                Some(r) => Ok(Some(map_card!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{CARD_SELECT_SQLITE} WHERE k.id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project card failed: {e}"))?
            {
                Some(r) => Ok(Some(map_card!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Card lookup scoped to a column — cross-column ids don't resolve.
pub async fn find_card_in_column(
    pool: &DbPool,
    column_id: &str,
    id: &str,
) -> Result<Option<ProjectCardRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{CARD_SELECT_PG} WHERE k.id = $1 AND k.column_id = $2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .bind(column_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project card failed: {e}"))?
            {
                Some(r) => Ok(Some(map_card!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{CARD_SELECT_MYSQL} WHERE k.id = ? AND k.column_id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .bind(column_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project card failed: {e}"))?
            {
                Some(r) => Ok(Some(map_card!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{CARD_SELECT_SQLITE} WHERE k.id = ?1 AND k.column_id = ?2");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .bind(column_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find project card failed: {e}"))?
            {
                Some(r) => Ok(Some(map_card!(&r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn list_cards_for_column(
    pool: &DbPool,
    column_id: &str,
    include_archived: bool,
) -> Result<Vec<ProjectCardRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{CARD_SELECT_PG} WHERE k.column_id = $1 {} ORDER BY k.position ASC",
                if include_archived {
                    ""
                } else {
                    "AND NOT k.archived"
                }
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(column_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list project cards failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_card!(r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{CARD_SELECT_MYSQL} WHERE k.column_id = ? {} ORDER BY k.position ASC",
                if include_archived {
                    ""
                } else {
                    "AND NOT k.archived"
                }
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(column_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list project cards failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_card!(r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{CARD_SELECT_SQLITE} WHERE k.column_id = ?1 {} ORDER BY k.position ASC",
                if include_archived {
                    ""
                } else {
                    "AND NOT k.archived"
                }
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(column_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list project cards failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(map_card!(r));
            }
            Ok(out)
        }
    }
}

/// Append a card — position lands after the current last slot.
#[allow(clippy::too_many_arguments)]
pub async fn insert_project_card(
    pool: &DbPool,
    id: &str,
    column_id: &str,
    note: Option<&str>,
    content_type: Option<&str>,
    content_id: Option<&str>,
    creator_id: Option<&str>,
) -> Result<ProjectCardRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO project_cards (id, column_id, note, content_type, content_id, position, creator_id)
                 VALUES ($1, $2, $3, $4, $5,
                    (SELECT COALESCE(MAX(position) + 1, 0) FROM project_cards WHERE column_id = $2), $6)",
            )
            .bind(id)
            .bind(column_id)
            .bind(note)
            .bind(content_type)
            .bind(content_id)
            .bind(creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert project card failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO project_cards (id, column_id, note, content_type, content_id, position, creator_id)
                 SELECT ?, ?, ?, ?, ?, COALESCE(MAX(position) + 1, 0), ? FROM project_cards WHERE column_id = ?",
            )
            .bind(id)
            .bind(column_id)
            .bind(note)
            .bind(content_type)
            .bind(content_id)
            .bind(creator_id)
            .bind(column_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert project card failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO project_cards (id, column_id, note, content_type, content_id, position, creator_id)
                 VALUES (?1, ?2, ?3, ?4, ?5,
                    (SELECT COALESCE(MAX(position) + 1, 0) FROM project_cards WHERE column_id = ?2), ?6)",
            )
            .bind(id)
            .bind(column_id)
            .bind(note)
            .bind(content_type)
            .bind(content_id)
            .bind(creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert project card failed: {e}"))?;
        }
    }
    find_card_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert project card: row missing after insert".to_string())
}

/// Edit note/archived flag. Returns the updated row or `None` when it
/// vanished mid-update.
pub async fn update_project_card(
    pool: &DbPool,
    id: &str,
    note: Option<&str>,
    archived: Option<bool>,
) -> Result<Option<ProjectCardRow>, String> {
    let now = now_expr(pool);
    macro_rules! build {
        ($ty:ty) => {{
            let mut qb = sqlx::QueryBuilder::<$ty>::new("UPDATE project_cards SET ");
            {
                let mut sep = qb.separated(", ");
                if let Some(n) = note {
                    sep.push("note = ").push_bind_unseparated(n);
                }
                if let Some(a) = archived {
                    sep.push("archived = ").push_bind_unseparated(a);
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
                .map_err(|e| format!("update project card failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            build!(sqlx::MySql)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update project card failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            build!(sqlx::Sqlite)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update project card failed: {e}"))?;
        }
    }
    find_card_by_id(pool, id).await
}

/// Convert a note card to an issue link — clears `note`, sets
/// `content_type`/`content_id`. Already-linked cards return unchanged.
pub async fn convert_project_card(
    pool: &DbPool,
    id: &str,
    content_type: &str,
    content_id: &str,
) -> Result<Option<ProjectCardRow>, String> {
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "UPDATE project_cards SET note = NULL, content_type = $1, content_id = $2, updated_at = {now} WHERE id = $3"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(content_type)
                .bind(content_id)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("convert project card failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "UPDATE project_cards SET note = NULL, content_type = ?, content_id = ?, updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(content_type)
                .bind(content_id)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("convert project card failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "UPDATE project_cards SET note = NULL, content_type = ?1, content_id = ?2, updated_at = {now} WHERE id = ?3"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(content_type)
                .bind(content_id)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("convert project card failed: {e}"))?;
        }
    }
    find_card_by_id(pool, id).await
}

/// Move a card — optionally across columns. `new_position` clamps inside the
/// destination; the source column re-packs, the destination shifts open.
pub async fn move_project_card(
    pool: &DbPool,
    id: &str,
    dest_column_id: &str,
    new_position: i64,
) -> Result<Option<ProjectCardRow>, String> {
    let Some(card) = find_card_by_id(pool, id).await? else {
        return Ok(None);
    };
    let same_column = card.column_id == dest_column_id;
    let dest_cards = list_cards_for_column(pool, dest_column_id, true).await?;
    let dest_len = if same_column {
        dest_cards.len() as i64
    } else {
        dest_cards.len() as i64 + 1
    };
    let target = new_position.clamp(0, dest_len - 1);
    if same_column && target == card.position {
        return Ok(Some(card));
    }
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin move card tx failed: {e}"))?;
            if same_column {
                if target < card.position {
                    sqlx::query(
                        "UPDATE project_cards SET position = position + 1
                         WHERE column_id = $1 AND position >= $2 AND position < $3",
                    )
                    .bind(dest_column_id)
                    .bind(target)
                    .bind(card.position)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("shift project cards failed: {e}"))?;
                } else {
                    sqlx::query(
                        "UPDATE project_cards SET position = position - 1
                         WHERE column_id = $1 AND position <= $2 AND position > $3",
                    )
                    .bind(dest_column_id)
                    .bind(target)
                    .bind(card.position)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("shift project cards failed: {e}"))?;
                }
            } else {
                sqlx::query(
                    "UPDATE project_cards SET position = position - 1
                     WHERE column_id = $1 AND position > $2",
                )
                .bind(&card.column_id)
                .bind(card.position)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("repack source column failed: {e}"))?;
                sqlx::query(
                    "UPDATE project_cards SET position = position + 1
                     WHERE column_id = $1 AND position >= $2",
                )
                .bind(dest_column_id)
                .bind(target)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("shift destination column failed: {e}"))?;
            }
            let sql = format!(
                "UPDATE project_cards SET column_id = $1, position = $2, updated_at = {now} WHERE id = $3"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(dest_column_id)
                .bind(target)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("move project card failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit move card tx failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin move card tx failed: {e}"))?;
            if same_column {
                if target < card.position {
                    sqlx::query(
                        "UPDATE project_cards SET position = position + 1
                         WHERE column_id = ? AND position >= ? AND position < ?",
                    )
                    .bind(dest_column_id)
                    .bind(target)
                    .bind(card.position)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("shift project cards failed: {e}"))?;
                } else {
                    sqlx::query(
                        "UPDATE project_cards SET position = position - 1
                         WHERE column_id = ? AND position <= ? AND position > ?",
                    )
                    .bind(dest_column_id)
                    .bind(target)
                    .bind(card.position)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("shift project cards failed: {e}"))?;
                }
            } else {
                sqlx::query(
                    "UPDATE project_cards SET position = position - 1
                     WHERE column_id = ? AND position > ?",
                )
                .bind(&card.column_id)
                .bind(card.position)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("repack source column failed: {e}"))?;
                sqlx::query(
                    "UPDATE project_cards SET position = position + 1
                     WHERE column_id = ? AND position >= ?",
                )
                .bind(dest_column_id)
                .bind(target)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("shift destination column failed: {e}"))?;
            }
            let sql = format!(
                "UPDATE project_cards SET column_id = ?, position = ?, updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(dest_column_id)
                .bind(target)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("move project card failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit move card tx failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin move card tx failed: {e}"))?;
            if same_column {
                if target < card.position {
                    sqlx::query(
                        "UPDATE project_cards SET position = position + 1
                         WHERE column_id = ?1 AND position >= ?2 AND position < ?3",
                    )
                    .bind(dest_column_id)
                    .bind(target)
                    .bind(card.position)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("shift project cards failed: {e}"))?;
                } else {
                    sqlx::query(
                        "UPDATE project_cards SET position = position - 1
                         WHERE column_id = ?1 AND position <= ?2 AND position > ?3",
                    )
                    .bind(dest_column_id)
                    .bind(target)
                    .bind(card.position)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("shift project cards failed: {e}"))?;
                }
            } else {
                sqlx::query(
                    "UPDATE project_cards SET position = position - 1
                     WHERE column_id = ?1 AND position > ?2",
                )
                .bind(&card.column_id)
                .bind(card.position)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("repack source column failed: {e}"))?;
                sqlx::query(
                    "UPDATE project_cards SET position = position + 1
                     WHERE column_id = ?1 AND position >= ?2",
                )
                .bind(dest_column_id)
                .bind(target)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("shift destination column failed: {e}"))?;
            }
            let sql = format!(
                "UPDATE project_cards SET column_id = ?1, position = ?2, updated_at = {now} WHERE id = ?3"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(dest_column_id)
                .bind(target)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("move project card failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit move card tx failed: {e}"))?;
        }
    }
    find_card_by_id(pool, id).await
}

/// Delete a card — higher positions in the column re-pack down.
pub async fn delete_project_card(pool: &DbPool, id: &str) -> Result<(), String> {
    let Some(card) = find_card_by_id(pool, id).await? else {
        return Ok(());
    };
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin delete card tx failed: {e}"))?;
            sqlx::query("DELETE FROM project_cards WHERE id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("delete project card failed: {e}"))?;
            sqlx::query(
                "UPDATE project_cards SET position = position - 1
                 WHERE column_id = $1 AND position > $2",
            )
            .bind(&card.column_id)
            .bind(card.position)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("repack project cards failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit delete card tx failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin delete card tx failed: {e}"))?;
            sqlx::query("DELETE FROM project_cards WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("delete project card failed: {e}"))?;
            sqlx::query(
                "UPDATE project_cards SET position = position - 1
                 WHERE column_id = ? AND position > ?",
            )
            .bind(&card.column_id)
            .bind(card.position)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("repack project cards failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit delete card tx failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin delete card tx failed: {e}"))?;
            sqlx::query("DELETE FROM project_cards WHERE id = ?1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("delete project card failed: {e}"))?;
            sqlx::query(
                "UPDATE project_cards SET position = position - 1
                 WHERE column_id = ?1 AND position > ?2",
            )
            .bind(&card.column_id)
            .bind(card.position)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("repack project cards failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit delete card tx failed: {e}"))?;
        }
    }
    Ok(())
}
