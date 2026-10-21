//! `check_suites` / `check_runs` / `check_run_annotations` — the public
//! Checks surface (GitHub Checks API parity). Actions-run jobs project a
//! check suite (`run_id` linkage) with one check run per job; API-created
//! runs share the `api`-app suite per commit. Latest rows order by `seq`
//! (PG/MySQL) or `rowid` (SQLite) so same-second writes stay deterministic.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct CheckSuiteRow {
    pub id: String,
    pub repository_id: String,
    pub head_sha: String,
    pub head_branch: Option<String>,
    pub app: String,
    pub run_id: Option<String>,
    /// `queued` | `in_progress` | `completed`.
    pub status: String,
    /// GitHub conclusion vocabulary once `status == completed`.
    pub conclusion: Option<String>,
    pub url: Option<String>,
    pub creator_id: Option<String>,
    /// Live check-run count under this suite.
    pub check_runs_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct CheckRunRow {
    pub id: String,
    pub repository_id: String,
    pub check_suite_id: String,
    pub name: String,
    pub head_sha: String,
    /// `queued` | `in_progress` | `completed` | `waiting` | `pending` |
    /// `requested` (GitHub `check_run.status`).
    pub status: String,
    /// `success`|`failure`|`neutral`|`cancelled`|`skipped`|`timed_out`|
    /// `action_required`|`startup_failure`|`stale` when completed.
    pub conclusion: Option<String>,
    pub details_url: Option<String>,
    pub external_id: Option<String>,
    pub output_title: Option<String>,
    pub output_summary: Option<String>,
    pub output_text: Option<String>,
    pub job_id: Option<String>,
    pub run_id: Option<String>,
    pub creator_id: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct CheckRunAnnotationRow {
    pub id: String,
    pub check_run_id: String,
    pub repository_id: String,
    pub path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub start_column: Option<i64>,
    pub end_column: Option<i64>,
    /// `notice` | `warning` | `failure`.
    pub annotation_level: String,
    pub message: String,
    pub title: Option<String>,
    pub raw_details: Option<String>,
    pub created_at: String,
}

pub struct NewCheckSuite<'a> {
    pub id: &'a str,
    pub repository_id: &'a str,
    pub head_sha: &'a str,
    pub head_branch: Option<&'a str>,
    pub app: &'a str,
    pub run_id: Option<&'a str>,
    pub status: &'a str,
    pub conclusion: Option<&'a str>,
    pub url: Option<&'a str>,
    pub creator_id: Option<&'a str>,
}

pub struct NewCheckRun<'a> {
    pub id: &'a str,
    pub repository_id: &'a str,
    pub check_suite_id: &'a str,
    pub name: &'a str,
    pub head_sha: &'a str,
    pub status: &'a str,
    pub conclusion: Option<&'a str>,
    pub details_url: Option<&'a str>,
    pub external_id: Option<&'a str>,
    pub output_title: Option<&'a str>,
    pub output_summary: Option<&'a str>,
    pub output_text: Option<&'a str>,
    pub job_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub creator_id: Option<&'a str>,
    pub started_at: Option<&'a str>,
    pub completed_at: Option<&'a str>,
}

/// Fields `check.runs.update` may rewrite — `None` keeps the stored value.
#[derive(Default)]
pub struct CheckRunPatch<'a> {
    pub name: Option<&'a str>,
    pub status: Option<&'a str>,
    pub conclusion: Option<Option<&'a str>>,
    pub details_url: Option<Option<&'a str>>,
    pub external_id: Option<Option<&'a str>>,
    pub output_title: Option<Option<&'a str>>,
    pub output_summary: Option<Option<&'a str>>,
    pub output_text: Option<Option<&'a str>>,
    pub started_at: Option<Option<&'a str>>,
    pub completed_at: Option<Option<&'a str>>,
}

pub struct NewCheckRunAnnotation<'a> {
    pub id: &'a str,
    pub check_run_id: &'a str,
    pub repository_id: &'a str,
    pub path: &'a str,
    pub start_line: i64,
    pub end_line: i64,
    pub start_column: Option<i64>,
    pub end_column: Option<i64>,
    pub annotation_level: &'a str,
    pub message: &'a str,
    pub title: Option<&'a str>,
    pub raw_details: Option<&'a str>,
}

const SUITE_COLS_SQLITE: &str = "s.id, s.repository_id, s.head_sha, s.head_branch, s.app,
       s.run_id, s.status, s.conclusion, s.url, s.creator_id,
       (SELECT COUNT(*) FROM check_runs r WHERE r.check_suite_id = s.id) AS check_runs_count,
       s.created_at, s.updated_at";

const SUITE_COLS_MYSQL: &str = "s.id, s.repository_id, s.head_sha, s.head_branch, s.app,
       s.run_id, s.status, s.conclusion, s.url, s.creator_id,
       (SELECT COUNT(*) FROM check_runs r WHERE r.check_suite_id = s.id) AS check_runs_count,
       CAST(s.created_at AS CHAR) AS created_at, CAST(s.updated_at AS CHAR) AS updated_at";

const SUITE_COLS_PG: &str = "s.id, s.repository_id, s.head_sha, s.head_branch, s.app,
       s.run_id, s.status, s.conclusion, s.url, s.creator_id,
       (SELECT COUNT(*) FROM check_runs r WHERE r.check_suite_id = s.id) AS check_runs_count,
       to_char(s.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(s.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at";

const RUN_COLS_SQLITE: &str = "id, repository_id, check_suite_id, name, head_sha, status,
       conclusion, details_url, external_id, output_title, output_summary, output_text,
       job_id, run_id, creator_id, started_at, completed_at, created_at, updated_at";

const RUN_COLS_MYSQL: &str = "id, repository_id, check_suite_id, name, head_sha, status,
       conclusion, details_url, external_id, output_title, output_summary, output_text,
       job_id, run_id, creator_id, started_at, completed_at,
       CAST(created_at AS CHAR) AS created_at, CAST(updated_at AS CHAR) AS updated_at";

const RUN_COLS_PG: &str = "id, repository_id, check_suite_id, name, head_sha, status,
       conclusion, details_url, external_id, output_title, output_summary, output_text,
       job_id, run_id, creator_id, started_at, completed_at,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at";

const ANNOTATION_COLS: &str = "id, check_run_id, repository_id, path, start_line, end_line,
       start_column, end_column, annotation_level, message, title, raw_details, created_at";

const ANNOTATION_COLS_MYSQL: &str = "id, check_run_id, repository_id, path, start_line, end_line,
       start_column, end_column, annotation_level, message, title, raw_details,
       CAST(created_at AS CHAR) AS created_at";

const ANNOTATION_COLS_PG: &str = "id, check_run_id, repository_id, path, start_line, end_line,
       start_column, end_column, annotation_level, message, title, raw_details,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at";

fn suite_cols_for(pool: &DbPool) -> &'static str {
    match pool {
        DbPool::Sqlite(_) => SUITE_COLS_SQLITE,
        DbPool::Postgres(_) => SUITE_COLS_PG,
        DbPool::MySql(_) => SUITE_COLS_MYSQL,
    }
}

fn run_cols_for(pool: &DbPool) -> &'static str {
    match pool {
        DbPool::Sqlite(_) => RUN_COLS_SQLITE,
        DbPool::Postgres(_) => RUN_COLS_PG,
        DbPool::MySql(_) => RUN_COLS_MYSQL,
    }
}

fn annotation_cols_for(pool: &DbPool) -> &'static str {
    match pool {
        DbPool::Sqlite(_) => ANNOTATION_COLS,
        DbPool::Postgres(_) => ANNOTATION_COLS_PG,
        DbPool::MySql(_) => ANNOTATION_COLS_MYSQL,
    }
}

macro_rules! map_suite {
    ($row:expr) => {{
        let r = $row;
        CheckSuiteRow {
            id: r.try_get("id").map_err(|e| format!("suite row: {e}"))?,
            repository_id: r
                .try_get("repository_id")
                .map_err(|e| format!("suite row: {e}"))?,
            head_sha: r
                .try_get("head_sha")
                .map_err(|e| format!("suite row: {e}"))?,
            head_branch: r
                .try_get("head_branch")
                .map_err(|e| format!("suite row: {e}"))?,
            app: r.try_get("app").map_err(|e| format!("suite row: {e}"))?,
            run_id: r.try_get("run_id").map_err(|e| format!("suite row: {e}"))?,
            status: r.try_get("status").map_err(|e| format!("suite row: {e}"))?,
            conclusion: r
                .try_get("conclusion")
                .map_err(|e| format!("suite row: {e}"))?,
            url: r.try_get("url").map_err(|e| format!("suite row: {e}"))?,
            creator_id: r
                .try_get("creator_id")
                .map_err(|e| format!("suite row: {e}"))?,
            check_runs_count: r
                .try_get::<i64, _>("check_runs_count")
                .map_err(|e| format!("suite row: {e}"))?,
            created_at: r
                .try_get("created_at")
                .map_err(|e| format!("suite row: {e}"))?,
            updated_at: r
                .try_get("updated_at")
                .map_err(|e| format!("suite row: {e}"))?,
        }
    }};
}

macro_rules! map_run {
    ($row:expr) => {{
        let r = $row;
        CheckRunRow {
            id: r.try_get("id").map_err(|e| format!("check row: {e}"))?,
            repository_id: r
                .try_get("repository_id")
                .map_err(|e| format!("check row: {e}"))?,
            check_suite_id: r
                .try_get("check_suite_id")
                .map_err(|e| format!("check row: {e}"))?,
            name: r.try_get("name").map_err(|e| format!("check row: {e}"))?,
            head_sha: r
                .try_get("head_sha")
                .map_err(|e| format!("check row: {e}"))?,
            status: r.try_get("status").map_err(|e| format!("check row: {e}"))?,
            conclusion: r
                .try_get("conclusion")
                .map_err(|e| format!("check row: {e}"))?,
            details_url: r
                .try_get("details_url")
                .map_err(|e| format!("check row: {e}"))?,
            external_id: r
                .try_get("external_id")
                .map_err(|e| format!("check row: {e}"))?,
            output_title: r
                .try_get("output_title")
                .map_err(|e| format!("check row: {e}"))?,
            output_summary: r
                .try_get("output_summary")
                .map_err(|e| format!("check row: {e}"))?,
            output_text: r
                .try_get("output_text")
                .map_err(|e| format!("check row: {e}"))?,
            job_id: r.try_get("job_id").map_err(|e| format!("check row: {e}"))?,
            run_id: r.try_get("run_id").map_err(|e| format!("check row: {e}"))?,
            creator_id: r
                .try_get("creator_id")
                .map_err(|e| format!("check row: {e}"))?,
            started_at: r
                .try_get("started_at")
                .map_err(|e| format!("check row: {e}"))?,
            completed_at: r
                .try_get("completed_at")
                .map_err(|e| format!("check row: {e}"))?,
            created_at: r
                .try_get("created_at")
                .map_err(|e| format!("check row: {e}"))?,
            updated_at: r
                .try_get("updated_at")
                .map_err(|e| format!("check row: {e}"))?,
        }
    }};
}

macro_rules! map_annotation {
    ($row:expr) => {{
        let r = $row;
        CheckRunAnnotationRow {
            id: r
                .try_get("id")
                .map_err(|e| format!("annotation row: {e}"))?,
            check_run_id: r
                .try_get("check_run_id")
                .map_err(|e| format!("annotation row: {e}"))?,
            repository_id: r
                .try_get("repository_id")
                .map_err(|e| format!("annotation row: {e}"))?,
            path: r
                .try_get("path")
                .map_err(|e| format!("annotation row: {e}"))?,
            start_line: r
                .try_get("start_line")
                .map_err(|e| format!("annotation row: {e}"))?,
            end_line: r
                .try_get("end_line")
                .map_err(|e| format!("annotation row: {e}"))?,
            start_column: r
                .try_get("start_column")
                .map_err(|e| format!("annotation row: {e}"))?,
            end_column: r
                .try_get("end_column")
                .map_err(|e| format!("annotation row: {e}"))?,
            annotation_level: r
                .try_get("annotation_level")
                .map_err(|e| format!("annotation row: {e}"))?,
            message: r
                .try_get("message")
                .map_err(|e| format!("annotation row: {e}"))?,
            title: r
                .try_get("title")
                .map_err(|e| format!("annotation row: {e}"))?,
            raw_details: r
                .try_get("raw_details")
                .map_err(|e| format!("annotation row: {e}"))?,
            created_at: r
                .try_get("created_at")
                .map_err(|e| format!("annotation row: {e}"))?,
        }
    }};
}

/// Insert a suite and return the stored row.
#[allow(clippy::too_many_arguments)]
pub async fn insert_check_suite(
    pool: &DbPool,
    s: &NewCheckSuite<'_>,
) -> Result<CheckSuiteRow, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO check_suites (id, repository_id, head_sha, head_branch, app,
                  run_id, status, conclusion, url, creator_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(s.id)
            .bind(s.repository_id)
            .bind(s.head_sha)
            .bind(s.head_branch)
            .bind(s.app)
            .bind(s.run_id)
            .bind(s.status)
            .bind(s.conclusion)
            .bind(s.url)
            .bind(s.creator_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO check_suites (id, repository_id, head_sha, head_branch, app,
                  run_id, status, conclusion, url, creator_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            )
            .bind(s.id)
            .bind(s.repository_id)
            .bind(s.head_sha)
            .bind(s.head_branch)
            .bind(s.app)
            .bind(s.run_id)
            .bind(s.status)
            .bind(s.conclusion)
            .bind(s.url)
            .bind(s.creator_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO check_suites (id, repository_id, head_sha, head_branch, app,
                  run_id, status, conclusion, url, creator_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(s.id)
            .bind(s.repository_id)
            .bind(s.head_sha)
            .bind(s.head_branch)
            .bind(s.app)
            .bind(s.run_id)
            .bind(s.status)
            .bind(s.conclusion)
            .bind(s.url)
            .bind(s.creator_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    find_check_suite(pool, s.id)
        .await?
        .ok_or_else(|| "check suite missing after insert".into())
}

/// Suite by id.
pub async fn find_check_suite(pool: &DbPool, id: &str) -> Result<Option<CheckSuiteRow>, String> {
    let cols = suite_cols_for(pool);
    match pool {
        DbPool::Sqlite(p) => {
            let sql = format!("SELECT {cols} FROM check_suites s WHERE s.id = ?");
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?
                .map(|r| Ok(map_suite!(r)))
                .transpose()
        }
        DbPool::Postgres(p) => {
            let sql = format!("SELECT {cols} FROM check_suites s WHERE s.id = $1");
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?
                .map(|r| Ok(map_suite!(r)))
                .transpose()
        }
        DbPool::MySql(p) => {
            let sql = format!("SELECT {cols} FROM check_suites s WHERE s.id = ?");
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?
                .map(|r| Ok(map_suite!(r)))
                .transpose()
        }
    }
}

/// The non-run suite for (repo, sha, app) — API callers share one suite per
/// commit like GitHub's per-app suite.
pub async fn find_check_suite_for(
    pool: &DbPool,
    repository_id: &str,
    head_sha: &str,
    app: &str,
) -> Result<Option<CheckSuiteRow>, String> {
    let cols = suite_cols_for(pool);
    let (ph1, ph2, ph3) = match pool {
        DbPool::Sqlite(_) | DbPool::MySql(_) => ("?", "?", "?"),
        DbPool::Postgres(_) => ("$1", "$2", "$3"),
    };
    let sql = format!(
        "SELECT {cols} FROM check_suites s
         WHERE s.repository_id = {ph1} AND s.head_sha = {ph2} AND s.app = {ph3}
           AND s.run_id IS NULL
         ORDER BY s.created_at DESC LIMIT 1"
    );
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(repository_id)
            .bind(head_sha)
            .bind(app)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_suite!(r)))
            .transpose(),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(repository_id)
            .bind(head_sha)
            .bind(app)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_suite!(r)))
            .transpose(),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(repository_id)
            .bind(head_sha)
            .bind(app)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_suite!(r)))
            .transpose(),
    }
}

/// The Actions-run suite (`run_id` linkage).
pub async fn find_check_suite_for_run(
    pool: &DbPool,
    run_id: &str,
) -> Result<Option<CheckSuiteRow>, String> {
    let cols = suite_cols_for(pool);
    let ph = match pool {
        DbPool::Sqlite(_) | DbPool::MySql(_) => "?",
        DbPool::Postgres(_) => "$1",
    };
    let sql = format!("SELECT {cols} FROM check_suites s WHERE s.run_id = {ph} LIMIT 1");
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(run_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_suite!(r)))
            .transpose(),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(run_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_suite!(r)))
            .transpose(),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(run_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_suite!(r)))
            .transpose(),
    }
}

/// Suites newest-first, optionally narrowed to a sha.
pub async fn list_check_suites(
    pool: &DbPool,
    repository_id: &str,
    head_sha: Option<&str>,
    limit: i64,
) -> Result<Vec<CheckSuiteRow>, String> {
    let limit = limit.clamp(1, 500);
    let cols = suite_cols_for(pool);
    match pool {
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {cols} FROM check_suites s
                 WHERE s.repository_id = ? AND (? IS NULL OR s.head_sha = ?)
                 ORDER BY s.created_at DESC, s.rowid DESC LIMIT ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(head_sha)
                .bind(head_sha)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|r| Ok(map_suite!(r)))
                .collect()
        }
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {cols} FROM check_suites s
                 WHERE s.repository_id = $1 AND ($2::text IS NULL OR s.head_sha = $2)
                 ORDER BY s.created_at DESC, s.seq DESC LIMIT $3"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(head_sha)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|r| Ok(map_suite!(r)))
                .collect()
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {cols} FROM check_suites s
                 WHERE s.repository_id = ? AND (? IS NULL OR s.head_sha = ?)
                 ORDER BY s.created_at DESC, s.seq DESC LIMIT ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(head_sha)
                .bind(head_sha)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|r| Ok(map_suite!(r)))
                .collect()
        }
    }
}

/// Rewrite a suite's status/conclusion; `updated_at` bumps on every write.
pub async fn update_check_suite_state(
    pool: &DbPool,
    id: &str,
    status: &str,
    conclusion: Option<&str>,
) -> Result<(), String> {
    let now = now_expr(pool);
    match pool {
        DbPool::Sqlite(p) => {
            let sql = format!(
                "UPDATE check_suites SET status = ?, conclusion = ?, updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(status)
                .bind(conclusion)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE check_suites SET status = $1, conclusion = $2, updated_at = now() WHERE id = $3",
            )
            .bind(status)
            .bind(conclusion)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "UPDATE check_suites SET status = ?, conclusion = ?, updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(status)
                .bind(conclusion)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// `updated_at` stamp per dialect — a literal folded into the SET list.
fn now_expr(pool: &DbPool) -> &'static str {
    match pool {
        DbPool::Sqlite(_) => "strftime('%Y-%m-%d %H:%M:%S','now')",
        _ => "CURRENT_TIMESTAMP",
    }
}

/// Recompute a suite from its runs: `completed` once every run is completed
/// (conclusion = GitHub's failure > cancelled > success rollup), otherwise
/// `in_progress` when any run is running, `queued` otherwise. Returns the
/// new (status, conclusion).
pub async fn recompute_check_suite(
    pool: &DbPool,
    suite_id: &str,
) -> Result<(String, Option<String>), String> {
    let runs = list_check_runs_for_suite(pool, suite_id, 500).await?;
    let (status, conclusion) = rollup_suite_state(&runs);
    update_check_suite_state(pool, suite_id, &status, conclusion.as_deref()).await?;
    Ok((status, conclusion))
}

/// GitHub's suite rollup: any failure-family conclusion wins, then
/// cancelled, then success when all runs completed; `in_progress` when any
/// run is mid-flight, `queued` otherwise.
pub fn rollup_suite_state(runs: &[CheckRunRow]) -> (String, Option<String>) {
    if runs.is_empty() {
        return ("queued".into(), None);
    }
    let all_completed = runs.iter().all(|r| r.status == "completed");
    if !all_completed {
        let any_running = runs
            .iter()
            .any(|r| matches!(r.status.as_str(), "in_progress" | "waiting" | "pending"));
        return (
            if any_running { "in_progress" } else { "queued" }.into(),
            None,
        );
    }
    let mut saw_failure = false;
    let mut saw_cancelled = false;
    for r in runs {
        match r.conclusion.as_deref() {
            Some("failure")
            | Some("timed_out")
            | Some("startup_failure")
            | Some("action_required") => {
                saw_failure = true;
            }
            Some("cancelled") | Some("stale") => saw_cancelled = true,
            _ => {}
        }
    }
    let conclusion = if saw_failure {
        "failure"
    } else if saw_cancelled {
        "cancelled"
    } else {
        "success"
    };
    ("completed".into(), Some(conclusion.into()))
}

/// Insert a check run and return the stored row.
#[allow(clippy::too_many_arguments)]
pub async fn insert_check_run(pool: &DbPool, r: &NewCheckRun<'_>) -> Result<CheckRunRow, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO check_runs (id, repository_id, check_suite_id, name, head_sha,
                  status, conclusion, details_url, external_id, output_title, output_summary,
                  output_text, job_id, run_id, creator_id, started_at, completed_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(r.id)
            .bind(r.repository_id)
            .bind(r.check_suite_id)
            .bind(r.name)
            .bind(r.head_sha)
            .bind(r.status)
            .bind(r.conclusion)
            .bind(r.details_url)
            .bind(r.external_id)
            .bind(r.output_title)
            .bind(r.output_summary)
            .bind(r.output_text)
            .bind(r.job_id)
            .bind(r.run_id)
            .bind(r.creator_id)
            .bind(r.started_at)
            .bind(r.completed_at)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO check_runs (id, repository_id, check_suite_id, name, head_sha,
                  status, conclusion, details_url, external_id, output_title, output_summary,
                  output_text, job_id, run_id, creator_id, started_at, completed_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
            )
            .bind(r.id)
            .bind(r.repository_id)
            .bind(r.check_suite_id)
            .bind(r.name)
            .bind(r.head_sha)
            .bind(r.status)
            .bind(r.conclusion)
            .bind(r.details_url)
            .bind(r.external_id)
            .bind(r.output_title)
            .bind(r.output_summary)
            .bind(r.output_text)
            .bind(r.job_id)
            .bind(r.run_id)
            .bind(r.creator_id)
            .bind(r.started_at)
            .bind(r.completed_at)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO check_runs (id, repository_id, check_suite_id, name, head_sha,
                  status, conclusion, details_url, external_id, output_title, output_summary,
                  output_text, job_id, run_id, creator_id, started_at, completed_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(r.id)
            .bind(r.repository_id)
            .bind(r.check_suite_id)
            .bind(r.name)
            .bind(r.head_sha)
            .bind(r.status)
            .bind(r.conclusion)
            .bind(r.details_url)
            .bind(r.external_id)
            .bind(r.output_title)
            .bind(r.output_summary)
            .bind(r.output_text)
            .bind(r.job_id)
            .bind(r.run_id)
            .bind(r.creator_id)
            .bind(r.started_at)
            .bind(r.completed_at)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    find_check_run(pool, r.id)
        .await?
        .ok_or_else(|| "check run missing after insert".into())
}

/// Check run by id.
pub async fn find_check_run(pool: &DbPool, id: &str) -> Result<Option<CheckRunRow>, String> {
    let cols = run_cols_for(pool);
    let ph = match pool {
        DbPool::Sqlite(_) | DbPool::MySql(_) => "?",
        DbPool::Postgres(_) => "$1",
    };
    let sql = format!("SELECT {cols} FROM check_runs WHERE id = {ph}");
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_run!(r)))
            .transpose(),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_run!(r)))
            .transpose(),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_run!(r)))
            .transpose(),
    }
}

/// Check run linked to an Actions job (the projection key).
pub async fn find_check_run_by_job(
    pool: &DbPool,
    job_id: &str,
) -> Result<Option<CheckRunRow>, String> {
    let cols = run_cols_for(pool);
    let ph = match pool {
        DbPool::Sqlite(_) | DbPool::MySql(_) => "?",
        DbPool::Postgres(_) => "$1",
    };
    let sql = format!("SELECT {cols} FROM check_runs WHERE job_id = {ph} LIMIT 1");
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(job_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_run!(r)))
            .transpose(),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(job_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_run!(r)))
            .transpose(),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(job_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| Ok(map_run!(r)))
            .transpose(),
    }
}

/// Runs under one suite, oldest first (GitHub list order).
pub async fn list_check_runs_for_suite(
    pool: &DbPool,
    suite_id: &str,
    limit: i64,
) -> Result<Vec<CheckRunRow>, String> {
    let limit = limit.clamp(1, 500);
    let cols = run_cols_for(pool);
    let (ph, order) = match pool {
        DbPool::Sqlite(_) => ("?", "created_at, rowid"),
        DbPool::Postgres(_) => ("$1", "created_at, seq"),
        DbPool::MySql(_) => ("?", "created_at, seq"),
    };
    let sql = format!(
        "SELECT {cols} FROM check_runs WHERE check_suite_id = {ph} ORDER BY {order} LIMIT {limit}"
    );
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(suite_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| Ok(map_run!(r)))
            .collect(),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(suite_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| Ok(map_run!(r)))
            .collect(),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(suite_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| Ok(map_run!(r)))
            .collect(),
    }
}

/// Check runs on a commit (`check_name`/`status`/`conclusion` NULL-passes),
/// newest first.
pub async fn list_check_runs_for_sha(
    pool: &DbPool,
    repository_id: &str,
    head_sha: &str,
    check_name: Option<&str>,
    status: Option<&str>,
    conclusion: Option<&str>,
    limit: i64,
) -> Result<Vec<CheckRunRow>, String> {
    let limit = limit.clamp(1, 500);
    let cols = run_cols_for(pool);
    match pool {
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {cols} FROM check_runs
                 WHERE repository_id = ? AND head_sha = ?
                   AND (? IS NULL OR name = ?)
                   AND (? IS NULL OR status = ?)
                   AND (? IS NULL OR conclusion = ?)
                 ORDER BY created_at DESC, rowid DESC LIMIT ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(head_sha)
                .bind(check_name)
                .bind(check_name)
                .bind(status)
                .bind(status)
                .bind(conclusion)
                .bind(conclusion)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|r| Ok(map_run!(r)))
                .collect()
        }
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {cols} FROM check_runs
                 WHERE repository_id = $1 AND head_sha = $2
                   AND ($3::text IS NULL OR name = $3)
                   AND ($4::text IS NULL OR status = $4)
                   AND ($5::text IS NULL OR conclusion = $5)
                 ORDER BY created_at DESC, seq DESC LIMIT $6"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(head_sha)
                .bind(check_name)
                .bind(status)
                .bind(conclusion)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|r| Ok(map_run!(r)))
                .collect()
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {cols} FROM check_runs
                 WHERE repository_id = ? AND head_sha = ?
                   AND (? IS NULL OR name = ?)
                   AND (? IS NULL OR status = ?)
                   AND (? IS NULL OR conclusion = ?)
                 ORDER BY created_at DESC, seq DESC LIMIT ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(head_sha)
                .bind(check_name)
                .bind(check_name)
                .bind(status)
                .bind(status)
                .bind(conclusion)
                .bind(conclusion)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|r| Ok(map_run!(r)))
                .collect()
        }
    }
}

/// Apply a `check.runs.update` patch; only `Some` fields are rewritten.
/// Returns the updated row.
pub async fn update_check_run(
    pool: &DbPool,
    id: &str,
    patch: &CheckRunPatch<'_>,
) -> Result<CheckRunRow, String> {
    let now = now_expr(pool);
    macro_rules! build {
        ($ty:ty) => {{
            let mut qb = sqlx::QueryBuilder::<$ty>::new("UPDATE check_runs SET ");
            {
                let mut sep = qb.separated(", ");
                if let Some(v) = patch.name {
                    sep.push("name = ").push_bind_unseparated(v);
                }
                if let Some(v) = patch.status {
                    sep.push("status = ").push_bind_unseparated(v);
                }
                if let Some(v) = patch.conclusion {
                    sep.push("conclusion = ").push_bind_unseparated(v);
                }
                if let Some(v) = patch.details_url {
                    sep.push("details_url = ").push_bind_unseparated(v);
                }
                if let Some(v) = patch.external_id {
                    sep.push("external_id = ").push_bind_unseparated(v);
                }
                if let Some(v) = patch.output_title {
                    sep.push("output_title = ").push_bind_unseparated(v);
                }
                if let Some(v) = patch.output_summary {
                    sep.push("output_summary = ").push_bind_unseparated(v);
                }
                if let Some(v) = patch.output_text {
                    sep.push("output_text = ").push_bind_unseparated(v);
                }
                if let Some(v) = patch.started_at {
                    sep.push("started_at = ").push_bind_unseparated(v);
                }
                if let Some(v) = patch.completed_at {
                    sep.push("completed_at = ").push_bind_unseparated(v);
                }
                sep.push(format!("updated_at = {now}"));
            }
            qb.push(" WHERE id = ").push_bind(id);
            qb
        }};
    }
    match pool {
        DbPool::Sqlite(p) => {
            build!(sqlx::Sqlite)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update check run failed: {e}"))?;
        }
        DbPool::Postgres(p) => {
            build!(sqlx::Postgres)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update check run failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            build!(sqlx::MySql)
                .build()
                .execute(p)
                .await
                .map_err(|e| format!("update check run failed: {e}"))?;
        }
    }
    find_check_run(pool, id)
        .await?
        .ok_or_else(|| "check run missing after update".into())
}

/// Insert an annotation row.
pub async fn insert_check_run_annotation(
    pool: &DbPool,
    a: &NewCheckRunAnnotation<'_>,
) -> Result<(), String> {
    let stmt = match pool {
        DbPool::Sqlite(_) | DbPool::MySql(_) => {
            "INSERT INTO check_run_annotations (id, check_run_id, repository_id, path,
              start_line, end_line, start_column, end_column, annotation_level,
              message, title, raw_details)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        }
        DbPool::Postgres(_) => {
            "INSERT INTO check_run_annotations (id, check_run_id, repository_id, path,
              start_line, end_line, start_column, end_column, annotation_level,
              message, title, raw_details)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
        }
    };
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(stmt)
                .bind(a.id)
                .bind(a.check_run_id)
                .bind(a.repository_id)
                .bind(a.path)
                .bind(a.start_line)
                .bind(a.end_line)
                .bind(a.start_column)
                .bind(a.end_column)
                .bind(a.annotation_level)
                .bind(a.message)
                .bind(a.title)
                .bind(a.raw_details)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(stmt)
                .bind(a.id)
                .bind(a.check_run_id)
                .bind(a.repository_id)
                .bind(a.path)
                .bind(a.start_line)
                .bind(a.end_line)
                .bind(a.start_column)
                .bind(a.end_column)
                .bind(a.annotation_level)
                .bind(a.message)
                .bind(a.title)
                .bind(a.raw_details)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(stmt)
                .bind(a.id)
                .bind(a.check_run_id)
                .bind(a.repository_id)
                .bind(a.path)
                .bind(a.start_line)
                .bind(a.end_line)
                .bind(a.start_column)
                .bind(a.end_column)
                .bind(a.annotation_level)
                .bind(a.message)
                .bind(a.title)
                .bind(a.raw_details)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    };
    Ok(())
}

/// Annotations attached to one check run, insertion order.
pub async fn list_check_run_annotations(
    pool: &DbPool,
    check_run_id: &str,
) -> Result<Vec<CheckRunAnnotationRow>, String> {
    let cols = annotation_cols_for(pool);
    let (ph, order) = match pool {
        DbPool::Sqlite(_) => ("?", "rowid"),
        DbPool::Postgres(_) => ("$1", "seq"),
        DbPool::MySql(_) => ("?", "seq"),
    };
    let sql = format!(
        "SELECT {cols} FROM check_run_annotations WHERE check_run_id = {ph} ORDER BY {order}"
    );
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(check_run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| Ok(map_annotation!(r)))
            .collect(),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(check_run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| Ok(map_annotation!(r)))
            .collect(),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(check_run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| Ok(map_annotation!(r)))
            .collect(),
    }
}
