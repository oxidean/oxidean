//! GitHub Pages — `repo_pages` site config (existence = enabled) and
//! `pages_builds` append-only build history. `build_type` is `legacy`
//! (branch+path, rebuilt on push) or `workflow` (Actions deploy seam).
//! Build `status` runs `building` → `built` | `errored`; serving resolves
//! the newest `built` row.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct RepoPagesRow {
    pub repo_id: String,
    /// `legacy` | `workflow`.
    pub build_type: String,
    pub source_branch: String,
    /// `/` or `/docs` — the served subtree of `source_branch`.
    pub source_path: String,
    pub cname: String,
    pub https_enforced: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct PagesBuildRow {
    pub id: String,
    pub repo_id: String,
    /// `building` | `built` | `errored`.
    pub status: String,
    /// `legacy` | `workflow` — how the build was produced.
    pub build_type: String,
    pub commit_sha: String,
    pub branch: String,
    pub path: String,
    pub url: String,
    pub error_message: String,
    pub duration_ms: i64,
    pub pusher_id: Option<String>,
    /// Actions run id for `workflow` builds; NULL for `legacy`/`request`.
    pub run_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_pages {
    ($row:expr) => {{
        let row = $row;
        RepoPagesRow {
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("repo_pages row: {e}"))?,
            build_type: row
                .try_get("build_type")
                .map_err(|e| format!("repo_pages row: {e}"))?,
            source_branch: row
                .try_get("source_branch")
                .map_err(|e| format!("repo_pages row: {e}"))?,
            source_path: row
                .try_get("source_path")
                .map_err(|e| format!("repo_pages row: {e}"))?,
            cname: row
                .try_get("cname")
                .map_err(|e| format!("repo_pages row: {e}"))?,
            https_enforced: row
                .try_get::<bool, _>("https_enforced")
                .or_else(|_| row.try_get::<i64, _>("https_enforced").map(|v| v != 0))
                .or_else(|_| row.try_get::<i8, _>("https_enforced").map(|v| v != 0))
                .map_err(|e| format!("repo_pages row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("repo_pages row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("repo_pages row: {e}"))?,
        }
    }};
}

macro_rules! map_build {
    ($row:expr) => {{
        let row = $row;
        PagesBuildRow {
            id: row.try_get("id").map_err(|e| format!("build row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("build row: {e}"))?,
            status: row
                .try_get("status")
                .map_err(|e| format!("build row: {e}"))?,
            build_type: row
                .try_get("build_type")
                .map_err(|e| format!("build row: {e}"))?,
            commit_sha: row
                .try_get("commit_sha")
                .map_err(|e| format!("build row: {e}"))?,
            branch: row
                .try_get("branch")
                .map_err(|e| format!("build row: {e}"))?,
            path: row.try_get("path").map_err(|e| format!("build row: {e}"))?,
            url: row.try_get("url").map_err(|e| format!("build row: {e}"))?,
            error_message: row
                .try_get("error_message")
                .map_err(|e| format!("build row: {e}"))?,
            duration_ms: row
                .try_get::<i64, _>("duration_ms")
                .or_else(|_| row.try_get::<i32, _>("duration_ms").map(i64::from))
                .map_err(|e| format!("build row: {e}"))?,
            pusher_id: row
                .try_get("pusher_id")
                .map_err(|e| format!("build row: {e}"))?,
            run_id: row
                .try_get("run_id")
                .map_err(|e| format!("build row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("build row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("build row: {e}"))?,
        }
    }};
}

const PAGES_SELECT_PG: &str = "SELECT p.repo_id, p.build_type, p.source_branch, p.source_path,
       p.cname, p.https_enforced,
       to_char(p.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(p.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM repo_pages p";

const PAGES_SELECT_MYSQL: &str = "SELECT p.repo_id, p.build_type, p.source_branch, p.source_path,
       p.cname, p.https_enforced,
       DATE_FORMAT(p.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(p.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM repo_pages p";

const PAGES_SELECT_SQLITE: &str = "SELECT p.repo_id, p.build_type, p.source_branch, p.source_path,
       p.cname, p.https_enforced,
       strftime('%Y-%m-%dT%H:%M:%SZ', p.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', p.updated_at) AS updated_at
FROM repo_pages p";

const BUILD_SELECT_PG: &str = "SELECT b.id, b.repo_id, b.status, b.build_type, b.commit_sha,
       b.branch, b.path, b.url, b.error_message, b.duration_ms, b.pusher_id, b.run_id,
       to_char(b.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(b.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM pages_builds b";

const BUILD_SELECT_MYSQL: &str = "SELECT b.id, b.repo_id, b.status, b.build_type, b.commit_sha,
       b.branch, b.path, b.url, b.error_message, b.duration_ms, b.pusher_id, b.run_id,
       DATE_FORMAT(b.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(b.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM pages_builds b";

const BUILD_SELECT_SQLITE: &str = "SELECT b.id, b.repo_id, b.status, b.build_type, b.commit_sha,
       b.branch, b.path, b.url, b.error_message, b.duration_ms, b.pusher_id, b.run_id,
       strftime('%Y-%m-%dT%H:%M:%SZ', b.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', b.updated_at) AS updated_at
FROM pages_builds b";

/// `updated_at` stamp per dialect — a literal folded into the SET list.
fn now_expr(pool: &DbPool) -> &'static str {
    match pool {
        DbPool::Sqlite(_) => "strftime('%Y-%m-%d %H:%M:%S','now')",
        _ => "CURRENT_TIMESTAMP",
    }
}

// ---------------------------------------------------------------------------
// Site config
// ---------------------------------------------------------------------------

pub async fn find_repo_pages(pool: &DbPool, repo_id: &str) -> Result<Option<RepoPagesRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{PAGES_SELECT_PG} WHERE p.repo_id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find repo_pages failed: {e}"))?
            {
                Some(r) => Ok(Some(map_pages!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{PAGES_SELECT_MYSQL} WHERE p.repo_id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find repo_pages failed: {e}"))?
            {
                Some(r) => Ok(Some(map_pages!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{PAGES_SELECT_SQLITE} WHERE p.repo_id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find repo_pages failed: {e}"))?
            {
                Some(r) => Ok(Some(map_pages!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Create-or-replace the site config (POST + PUT /pages share the upsert).
#[allow(clippy::too_many_arguments)]
pub async fn upsert_repo_pages(
    pool: &DbPool,
    repo_id: &str,
    build_type: &str,
    source_branch: &str,
    source_path: &str,
    cname: &str,
    https_enforced: bool,
) -> Result<RepoPagesRow, String> {
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "INSERT INTO repo_pages (repo_id, build_type, source_branch, source_path, cname, https_enforced)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (repo_id) DO UPDATE SET
                   build_type = EXCLUDED.build_type,
                   source_branch = EXCLUDED.source_branch,
                   source_path = EXCLUDED.source_path,
                   cname = EXCLUDED.cname,
                   https_enforced = EXCLUDED.https_enforced,
                   updated_at = {now}"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(build_type)
                .bind(source_branch)
                .bind(source_path)
                .bind(cname)
                .bind(https_enforced)
                .execute(p)
                .await
                .map_err(|e| format!("upsert repo_pages failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "INSERT INTO repo_pages (repo_id, build_type, source_branch, source_path, cname, https_enforced)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON DUPLICATE KEY UPDATE
                   build_type = VALUES(build_type),
                   source_branch = VALUES(source_branch),
                   source_path = VALUES(source_path),
                   cname = VALUES(cname),
                   https_enforced = VALUES(https_enforced),
                   updated_at = {now}"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(build_type)
                .bind(source_branch)
                .bind(source_path)
                .bind(cname)
                .bind(https_enforced)
                .execute(p)
                .await
                .map_err(|e| format!("upsert repo_pages failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "INSERT INTO repo_pages (repo_id, build_type, source_branch, source_path, cname, https_enforced)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT (repo_id) DO UPDATE SET
                   build_type = excluded.build_type,
                   source_branch = excluded.source_branch,
                   source_path = excluded.source_path,
                   cname = excluded.cname,
                   https_enforced = excluded.https_enforced,
                   updated_at = {now}"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(build_type)
                .bind(source_branch)
                .bind(source_path)
                .bind(cname)
                .bind(https_enforced)
                .execute(p)
                .await
                .map_err(|e| format!("upsert repo_pages failed: {e}"))?;
        }
    }
    find_repo_pages(pool, repo_id)
        .await?
        .ok_or_else(|| "repo_pages row vanished after upsert".to_string())
}

pub async fn delete_repo_pages(pool: &DbPool, repo_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM repo_pages WHERE repo_id = $1")
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete repo_pages failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM repo_pages WHERE repo_id = ?")
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete repo_pages failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM repo_pages WHERE repo_id = ?1")
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete repo_pages failed: {e}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Builds
// ---------------------------------------------------------------------------

/// Insert a `building` row — the status transition lands via
/// [`finish_pages_build`].
#[allow(clippy::too_many_arguments)]
pub async fn insert_pages_build(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    build_type: &str,
    commit_sha: &str,
    branch: &str,
    path: &str,
    pusher_id: Option<&str>,
    run_id: Option<&str>,
) -> Result<PagesBuildRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO pages_builds
                   (id, repo_id, status, build_type, commit_sha, branch, path, pusher_id, run_id)
                 VALUES ($1, $2, 'building', $3, $4, $5, $6, $7, $8)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(build_type)
            .bind(commit_sha)
            .bind(branch)
            .bind(path)
            .bind(pusher_id)
            .bind(run_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert pages_build failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO pages_builds
                   (id, repo_id, status, build_type, commit_sha, branch, path, pusher_id, run_id)
                 VALUES (?, ?, 'building', ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(build_type)
            .bind(commit_sha)
            .bind(branch)
            .bind(path)
            .bind(pusher_id)
            .bind(run_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert pages_build failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO pages_builds
                   (id, repo_id, status, build_type, commit_sha, branch, path, pusher_id, run_id)
                 VALUES (?, ?, 'building', ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(build_type)
            .bind(commit_sha)
            .bind(branch)
            .bind(path)
            .bind(pusher_id)
            .bind(run_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert pages_build failed: {e}"))?;
        }
    }
    find_pages_build(pool, id)
        .await?
        .ok_or_else(|| "pages_build row vanished after insert".to_string())
}

/// Close out a build — `status` is `built` | `errored`; `url` lands only on
/// `built`, `error_message` only on `errored`.
pub async fn finish_pages_build(
    pool: &DbPool,
    id: &str,
    status: &str,
    url: &str,
    error_message: &str,
    duration_ms: i64,
) -> Result<Option<PagesBuildRow>, String> {
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "UPDATE pages_builds SET status = $1, url = $2, error_message = $3,
                   duration_ms = $4, updated_at = {now} WHERE id = $5"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(status)
                .bind(url)
                .bind(error_message)
                .bind(duration_ms)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("finish pages_build failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "UPDATE pages_builds SET status = ?, url = ?, error_message = ?,
                   duration_ms = ?, updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(status)
                .bind(url)
                .bind(error_message)
                .bind(duration_ms)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("finish pages_build failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "UPDATE pages_builds SET status = ?, url = ?, error_message = ?,
                   duration_ms = ?, updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(status)
                .bind(url)
                .bind(error_message)
                .bind(duration_ms)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("finish pages_build failed: {e}"))?;
        }
    }
    find_pages_build(pool, id).await
}

pub async fn find_pages_build(pool: &DbPool, id: &str) -> Result<Option<PagesBuildRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{BUILD_SELECT_PG} WHERE b.id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find pages_build failed: {e}"))?
            {
                Some(r) => Ok(Some(map_build!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{BUILD_SELECT_MYSQL} WHERE b.id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find pages_build failed: {e}"))?
            {
                Some(r) => Ok(Some(map_build!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{BUILD_SELECT_SQLITE} WHERE b.id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find pages_build failed: {e}"))?
            {
                Some(r) => Ok(Some(map_build!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Newest build for the repo, optionally constrained to a status
/// (`Some("built")` resolves the served build).
pub async fn latest_pages_build(
    pool: &DbPool,
    repo_id: &str,
    status: Option<&str>,
) -> Result<Option<PagesBuildRow>, String> {
    list_pages_builds(pool, repo_id, status, 1)
        .await
        .map(|mut v| v.pop())
}

/// Builds newest-first — `status` filters (`None` lists all).
pub async fn list_pages_builds(
    pool: &DbPool,
    repo_id: &str,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<PagesBuildRow>, String> {
    // The status filter stays bound on every path — an unbound `?NNN` slot
    // under SQLite reads NULL, and `LIMIT NULL` is a datatype error.
    let status = status.unwrap_or("");
    match pool {
        DbPool::Postgres(p) => {
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(format!(
                "{BUILD_SELECT_PG} WHERE b.repo_id = $1 AND ($2 = '' OR b.status = $2) \
                   ORDER BY b.created_at DESC, b.seq DESC LIMIT $3"
            )))
            .bind(repo_id)
            .bind(status)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list pages_builds failed: {e}"))?
            {
                out.push(map_build!(&row));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(format!(
                "{BUILD_SELECT_MYSQL} WHERE b.repo_id = ? AND (? = '' OR b.status = ?) \
                   ORDER BY b.created_at DESC, b.seq DESC LIMIT ?"
            )))
            .bind(repo_id)
            .bind(status)
            .bind(status)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list pages_builds failed: {e}"))?
            {
                out.push(map_build!(&row));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(format!(
                "{BUILD_SELECT_SQLITE} WHERE b.repo_id = ?1 AND (?2 = '' OR b.status = ?2) \
                   ORDER BY b.created_at DESC, b.rowid DESC LIMIT ?3"
            )))
            .bind(repo_id)
            .bind(status)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list pages_builds failed: {e}"))?
            {
                out.push(map_build!(&row));
            }
            Ok(out)
        }
    }
}

pub async fn delete_pages_builds_for_repo(pool: &DbPool, repo_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM pages_builds WHERE repo_id = $1")
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete pages_builds failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM pages_builds WHERE repo_id = ?")
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete pages_builds failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM pages_builds WHERE repo_id = ?1")
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete pages_builds failed: {e}"))?;
        }
    }
    Ok(())
}
