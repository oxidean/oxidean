//! `action_deployments` + `action_deployment_statuses` — the public
//! Deployments surface (GitHub Deployments API parity). Environment-job
//! deployments carry `run_id`/`job_id`; API-created deployments leave them
//! NULL. Status history lives in `action_deployment_statuses`; the latest
//! row projects as `latest_state` on every deployment select.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct ActionDeploymentRow {
    pub id: String,
    pub repository_id: String,
    pub environment: String,
    /// Present for environment-job deployments; NULL for API-created ones.
    pub run_id: Option<String>,
    pub job_id: Option<String>,
    pub sha: String,
    pub git_ref: String,
    pub url: Option<String>,
    pub task: String,
    pub description: String,
    /// Free-form JSON payload (serialized) — GitHub `deployment.payload`.
    pub payload: Option<String>,
    pub creator_id: Option<String>,
    pub transient_environment: bool,
    pub production_environment: bool,
    /// Live `action_jobs.status` for job-bound deployments.
    pub job_status: String,
    /// Latest `action_deployment_statuses.state` (NULL until first status).
    pub latest_state: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ActionDeploymentStatusRow {
    pub id: String,
    pub deployment_id: String,
    pub state: String,
    pub description: String,
    pub environment: Option<String>,
    pub environment_url: Option<String>,
    pub target_url: Option<String>,
    pub log_url: Option<String>,
    pub creator_id: Option<String>,
    pub created_at: String,
}

/// Insert payload — job-bound fields optional for API-created deployments.
pub struct NewDeployment<'a> {
    pub id: &'a str,
    pub repository_id: &'a str,
    pub environment: &'a str,
    pub run_id: Option<&'a str>,
    pub job_id: Option<&'a str>,
    pub sha: &'a str,
    pub git_ref: &'a str,
    pub url: Option<&'a str>,
    pub task: &'a str,
    pub description: &'a str,
    /// Serialized JSON.
    pub payload: Option<&'a str>,
    pub creator_id: Option<&'a str>,
    pub transient_environment: bool,
    pub production_environment: bool,
}

pub struct NewDeploymentStatus<'a> {
    pub id: &'a str,
    pub deployment_id: &'a str,
    pub state: &'a str,
    pub description: &'a str,
    pub environment: Option<&'a str>,
    pub environment_url: Option<&'a str>,
    pub target_url: Option<&'a str>,
    pub log_url: Option<&'a str>,
    pub creator_id: Option<&'a str>,
}

// Latest-status ordering must be monotonic: `created_at` is only
// second/millisecond-grained, so several statuses in one burst collide on it.
// Tiebreak on the insertion-ordered surrogate (`rowid` / `seq`) instead of the
// random uuid `id`.
const DEPLOYMENT_COLS_SQLITE: &str = "d.id, d.repository_id, d.environment, d.run_id, d.job_id,
       d.sha, d.ref, d.url, d.task, d.description, d.payload, d.creator_id,
       d.transient_environment, d.production_environment,
       j.status AS job_status,
       (SELECT s.state FROM action_deployment_statuses s
         WHERE s.deployment_id = d.id ORDER BY s.created_at DESC, s.rowid DESC LIMIT 1) AS latest_state,
       d.created_at, d.updated_at";

const DEPLOYMENT_COLS_MYSQL: &str = "d.id, d.repository_id, d.environment, d.run_id, d.job_id,
       d.sha, d.ref, d.url, d.task, d.description, CAST(d.payload AS CHAR) AS payload, d.creator_id,
       d.transient_environment, d.production_environment,
       j.status AS job_status,
       (SELECT s.state FROM action_deployment_statuses s
         WHERE s.deployment_id = d.id ORDER BY s.created_at DESC, s.seq DESC LIMIT 1) AS latest_state,
       CAST(d.created_at AS CHAR) AS created_at, CAST(d.updated_at AS CHAR) AS updated_at";

const DEPLOYMENT_COLS_PG: &str = "d.id, d.repository_id, d.environment, d.run_id, d.job_id,
       d.sha, d.ref, d.url, d.task, d.description, d.payload::text AS payload, d.creator_id,
       d.transient_environment, d.production_environment,
       j.status AS job_status,
       (SELECT s.state FROM action_deployment_statuses s
         WHERE s.deployment_id = d.id ORDER BY s.created_at DESC, s.seq DESC LIMIT 1) AS latest_state,
       to_char(d.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(d.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at";

const STATUS_COLS: &str = "id, deployment_id, state, description, environment, environment_url,
       target_url, log_url, creator_id, created_at";

const STATUS_COLS_MYSQL: &str =
    "id, deployment_id, state, description, environment, environment_url,
       target_url, log_url, creator_id, CAST(created_at AS CHAR) AS created_at";

const STATUS_COLS_PG: &str = "id, deployment_id, state, description, environment, environment_url,
       target_url, log_url, creator_id,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at";

macro_rules! map_deployment {
    ($row:expr) => {{
        let r = $row;
        ActionDeploymentRow {
            id: r
                .try_get("id")
                .map_err(|e| format!("deployment row: {e}"))?,
            repository_id: r
                .try_get("repository_id")
                .map_err(|e| format!("deployment row: {e}"))?,
            environment: r
                .try_get("environment")
                .map_err(|e| format!("deployment row: {e}"))?,
            run_id: r
                .try_get("run_id")
                .map_err(|e| format!("deployment row: {e}"))?,
            job_id: r
                .try_get("job_id")
                .map_err(|e| format!("deployment row: {e}"))?,
            sha: r
                .try_get("sha")
                .map_err(|e| format!("deployment row: {e}"))?,
            git_ref: r
                .try_get("ref")
                .map_err(|e| format!("deployment row: {e}"))?,
            url: r
                .try_get("url")
                .map_err(|e| format!("deployment row: {e}"))?,
            task: r
                .try_get("task")
                .map_err(|e| format!("deployment row: {e}"))?,
            description: r
                .try_get("description")
                .map_err(|e| format!("deployment row: {e}"))?,
            payload: r
                .try_get("payload")
                .map_err(|e| format!("deployment row: {e}"))?,
            creator_id: r
                .try_get("creator_id")
                .map_err(|e| format!("deployment row: {e}"))?,
            transient_environment: r
                .try_get("transient_environment")
                .map_err(|e| format!("deployment row: {e}"))?,
            production_environment: r
                .try_get("production_environment")
                .map_err(|e| format!("deployment row: {e}"))?,
            job_status: r.try_get("job_status").unwrap_or_default(),
            latest_state: r
                .try_get("latest_state")
                .map_err(|e| format!("deployment row: {e}"))?,
            created_at: r
                .try_get("created_at")
                .map_err(|e| format!("deployment row: {e}"))?,
            updated_at: r
                .try_get("updated_at")
                .map_err(|e| format!("deployment row: {e}"))?,
        }
    }};
}

macro_rules! map_status {
    ($row:expr) => {{
        let r = $row;
        ActionDeploymentStatusRow {
            id: r.try_get("id").map_err(|e| format!("status row: {e}"))?,
            deployment_id: r
                .try_get("deployment_id")
                .map_err(|e| format!("status row: {e}"))?,
            state: r.try_get("state").map_err(|e| format!("status row: {e}"))?,
            description: r
                .try_get("description")
                .map_err(|e| format!("status row: {e}"))?,
            environment: r
                .try_get("environment")
                .map_err(|e| format!("status row: {e}"))?,
            environment_url: r
                .try_get("environment_url")
                .map_err(|e| format!("status row: {e}"))?,
            target_url: r
                .try_get("target_url")
                .map_err(|e| format!("status row: {e}"))?,
            log_url: r
                .try_get("log_url")
                .map_err(|e| format!("status row: {e}"))?,
            creator_id: r
                .try_get("creator_id")
                .map_err(|e| format!("status row: {e}"))?,
            created_at: r
                .try_get("created_at")
                .map_err(|e| format!("status row: {e}"))?,
        }
    }};
}

fn status_cols_for(pool: &DbPool) -> &'static str {
    match pool {
        DbPool::Sqlite(_) => STATUS_COLS,
        DbPool::Postgres(_) => STATUS_COLS_PG,
        DbPool::MySql(_) => STATUS_COLS_MYSQL,
    }
}

async fn find_deployment_row(
    pool: &DbPool,
    column: &str,
    bind: &str,
) -> Result<Option<ActionDeploymentRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_SQLITE} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id WHERE d.{column} = ?"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(bind)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find deployment failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_deployment!(r))),
                None => Ok(None),
            }
        }
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_PG} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id WHERE d.{column} = $1"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(bind)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find deployment failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_deployment!(r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_MYSQL} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id WHERE d.{column} = ?"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(bind)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find deployment failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_deployment!(r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn insert_deployment(
    pool: &DbPool,
    d: &NewDeployment<'_>,
) -> Result<ActionDeploymentRow, String> {
    let transient = d.transient_environment;
    let production = d.production_environment;
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO action_deployments (id, repository_id, environment, run_id, job_id,
                 sha, ref, url, task, description, payload, creator_id,
                 transient_environment, production_environment)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(d.id)
            .bind(d.repository_id)
            .bind(d.environment)
            .bind(d.run_id)
            .bind(d.job_id)
            .bind(d.sha)
            .bind(d.git_ref)
            .bind(d.url)
            .bind(d.task)
            .bind(d.description)
            .bind(d.payload)
            .bind(d.creator_id)
            .bind(transient)
            .bind(production)
            .execute(p)
            .await
            .map_err(|e| format!("insert deployment failed: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO action_deployments (id, repository_id, environment, run_id, job_id,
                 sha, ref, url, task, description, payload, creator_id,
                 transient_environment, production_environment)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb, $12, $13, $14)",
            )
            .bind(d.id)
            .bind(d.repository_id)
            .bind(d.environment)
            .bind(d.run_id)
            .bind(d.job_id)
            .bind(d.sha)
            .bind(d.git_ref)
            .bind(d.url)
            .bind(d.task)
            .bind(d.description)
            .bind(d.payload)
            .bind(d.creator_id)
            .bind(transient)
            .bind(production)
            .execute(p)
            .await
            .map_err(|e| format!("insert deployment failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO action_deployments (id, repository_id, environment, run_id, job_id,
                 sha, ref, url, task, description, payload, creator_id,
                 transient_environment, production_environment)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(d.id)
            .bind(d.repository_id)
            .bind(d.environment)
            .bind(d.run_id)
            .bind(d.job_id)
            .bind(d.sha)
            .bind(d.git_ref)
            .bind(d.url)
            .bind(d.task)
            .bind(d.description)
            .bind(d.payload)
            .bind(d.creator_id)
            .bind(transient)
            .bind(production)
            .execute(p)
            .await
            .map_err(|e| format!("insert deployment failed: {e}"))?;
        }
    }
    find_deployment_by_id(pool, d.id)
        .await?
        .ok_or_else(|| "inserted deployment missing on reload".to_string())
}

pub async fn find_deployment_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<ActionDeploymentRow>, String> {
    find_deployment_row(pool, "id", id).await
}

pub async fn find_deployment_by_job_id(
    pool: &DbPool,
    job_id: &str,
) -> Result<Option<ActionDeploymentRow>, String> {
    find_deployment_row(pool, "job_id", job_id).await
}

/// Deployments for a run — `job_status` joins the live job row.
pub async fn list_deployments_for_run(
    pool: &DbPool,
    run_id: &str,
) -> Result<Vec<ActionDeploymentRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_SQLITE} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.run_id = ? ORDER BY d.created_at"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(run_id)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_PG} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.run_id = $1 ORDER BY d.created_at"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(run_id)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_MYSQL} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.run_id = ? ORDER BY d.created_at"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(run_id)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
    }
}

/// Repo-scoped deployment listing (newest first) — backs the repo
/// Environments view and the public `deployment.list` RPC. `environment`
/// narrows to one environment.
pub async fn list_deployments_for_repo(
    pool: &DbPool,
    repository_id: &str,
    environment: Option<&str>,
    limit: i64,
) -> Result<Vec<ActionDeploymentRow>, String> {
    let limit = limit.clamp(1, 500);
    match pool {
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_SQLITE} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = ? AND (? IS NULL OR d.environment = ?)
                 ORDER BY d.created_at DESC, d.rowid DESC LIMIT ?"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(environment)
                .bind(environment)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_PG} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = $1 AND ($2::text IS NULL OR d.environment = $2)
                 ORDER BY d.created_at DESC, d.seq DESC LIMIT $3"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(environment)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_MYSQL} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = ? AND (? IS NULL OR d.environment = ?)
                 ORDER BY d.created_at DESC, d.seq DESC LIMIT ?"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(environment)
                .bind(environment)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
    }
}

/// Public Deployments-API listing — all four GitHub query filters
/// (`environment`, `sha`, `ref`, `task`) as NULL-passes.
#[allow(clippy::too_many_arguments)]
pub async fn list_deployments_filtered(
    pool: &DbPool,
    repository_id: &str,
    environment: Option<&str>,
    sha: Option<&str>,
    git_ref: Option<&str>,
    task: Option<&str>,
    limit: i64,
) -> Result<Vec<ActionDeploymentRow>, String> {
    let limit = limit.clamp(1, 500);
    match pool {
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_SQLITE} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = ?
                   AND (? IS NULL OR d.environment = ?)
                   AND (? IS NULL OR d.sha = ?)
                   AND (? IS NULL OR d.ref = ?)
                   AND (? IS NULL OR d.task = ?)
                 ORDER BY d.created_at DESC, d.rowid DESC LIMIT ?"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(environment)
                .bind(environment)
                .bind(sha)
                .bind(sha)
                .bind(git_ref)
                .bind(git_ref)
                .bind(task)
                .bind(task)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_PG} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = $1
                   AND ($2::text IS NULL OR d.environment = $2)
                   AND ($3::text IS NULL OR d.sha = $3)
                   AND ($4::text IS NULL OR d.ref = $4)
                   AND ($5::text IS NULL OR d.task = $5)
                 ORDER BY d.created_at DESC, d.seq DESC LIMIT $6"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(environment)
                .bind(sha)
                .bind(git_ref)
                .bind(task)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_MYSQL} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = ?
                   AND (? IS NULL OR d.environment = ?)
                   AND (? IS NULL OR d.sha = ?)
                   AND (? IS NULL OR d.ref = ?)
                   AND (? IS NULL OR d.task = ?)
                 ORDER BY d.created_at DESC, d.seq DESC LIMIT ?"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(environment)
                .bind(environment)
                .bind(sha)
                .bind(sha)
                .bind(git_ref)
                .bind(git_ref)
                .bind(task)
                .bind(task)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
    }
}

pub async fn delete_deployment(pool: &DbPool, id: &str) -> Result<bool, String> {
    let n = match pool {
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM action_deployments WHERE id = ?")
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected(),
        DbPool::Postgres(p) => sqlx::query("DELETE FROM action_deployments WHERE id = $1")
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected(),
        DbPool::MySql(p) => sqlx::query("DELETE FROM action_deployments WHERE id = ?")
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected(),
    };
    Ok(n > 0)
}

// ---- deployment statuses ---------------------------------------------------

pub async fn insert_deployment_status(
    pool: &DbPool,
    s: &NewDeploymentStatus<'_>,
) -> Result<ActionDeploymentStatusRow, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO action_deployment_statuses (id, deployment_id, state, description,
                 environment, environment_url, target_url, log_url, creator_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(s.id)
            .bind(s.deployment_id)
            .bind(s.state)
            .bind(s.description)
            .bind(s.environment)
            .bind(s.environment_url)
            .bind(s.target_url)
            .bind(s.log_url)
            .bind(s.creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert deployment status failed: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO action_deployment_statuses (id, deployment_id, state, description,
                 environment, environment_url, target_url, log_url, creator_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(s.id)
            .bind(s.deployment_id)
            .bind(s.state)
            .bind(s.description)
            .bind(s.environment)
            .bind(s.environment_url)
            .bind(s.target_url)
            .bind(s.log_url)
            .bind(s.creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert deployment status failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO action_deployment_statuses (id, deployment_id, state, description,
                 environment, environment_url, target_url, log_url, creator_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(s.id)
            .bind(s.deployment_id)
            .bind(s.state)
            .bind(s.description)
            .bind(s.environment)
            .bind(s.environment_url)
            .bind(s.target_url)
            .bind(s.log_url)
            .bind(s.creator_id)
            .execute(p)
            .await
            .map_err(|e| format!("insert deployment status failed: {e}"))?;
        }
    }
    let cols = status_cols_for(pool);
    match pool {
        DbPool::Sqlite(p) => {
            let sql = format!("SELECT {cols} FROM action_deployment_statuses WHERE id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(s.id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("reload deployment status failed: {e}"))?;
            Ok(map_status!(row))
        }
        DbPool::Postgres(p) => {
            let sql = format!("SELECT {cols} FROM action_deployment_statuses WHERE id = $1");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(s.id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("reload deployment status failed: {e}"))?;
            Ok(map_status!(row))
        }
        DbPool::MySql(p) => {
            let sql = format!("SELECT {cols} FROM action_deployment_statuses WHERE id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(s.id)
                .fetch_one(p)
                .await
                .map_err(|e| format!("reload deployment status failed: {e}"))?;
            Ok(map_status!(row))
        }
    }
}

/// Statuses for one deployment, newest first (GitHub shape).
pub async fn list_deployment_statuses(
    pool: &DbPool,
    deployment_id: &str,
    limit: i64,
) -> Result<Vec<ActionDeploymentStatusRow>, String> {
    let limit = limit.clamp(1, 500);
    let cols = status_cols_for(pool);
    match pool {
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {cols} FROM action_deployment_statuses
                 WHERE deployment_id = ? ORDER BY created_at DESC, rowid DESC LIMIT ?"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(deployment_id)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_status!(r))).collect()
        }
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {cols} FROM action_deployment_statuses
                 WHERE deployment_id = $1 ORDER BY created_at DESC, seq DESC LIMIT $2"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(deployment_id)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_status!(r))).collect()
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "SELECT {cols} FROM action_deployment_statuses
                 WHERE deployment_id = ? ORDER BY created_at DESC, seq DESC LIMIT ?"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(deployment_id)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_status!(r))).collect()
        }
    }
}

/// Deployments in `repo + environment` whose latest status is not `inactive`
/// — `auto_inactive` marks each of these `inactive` on the new status.
pub async fn list_active_deployments_for_environment(
    pool: &DbPool,
    repository_id: &str,
    environment: &str,
    except_deployment_id: &str,
) -> Result<Vec<ActionDeploymentRow>, String> {
    let active_filter_sqlite = "(SELECT s.state FROM action_deployment_statuses s
         WHERE s.deployment_id = d.id ORDER BY s.created_at DESC, s.rowid DESC LIMIT 1)
        IS DISTINCT FROM 'inactive'";
    let active_filter_pg = "(SELECT s.state FROM action_deployment_statuses s
         WHERE s.deployment_id = d.id ORDER BY s.created_at DESC, s.seq DESC LIMIT 1)
        IS DISTINCT FROM 'inactive'";
    match pool {
        DbPool::Sqlite(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_SQLITE} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = ? AND d.environment = ? AND d.id <> ? AND {active_filter_sqlite}
                 ORDER BY d.created_at"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(environment)
                .bind(except_deployment_id)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
        DbPool::Postgres(p) => {
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_PG} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = $1 AND d.environment = $2 AND d.id <> $3 AND {active_filter_pg}
                 ORDER BY d.created_at"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(environment)
                .bind(except_deployment_id)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
        DbPool::MySql(p) => {
            // MySQL has no IS DISTINCT FROM — `<=>` is the null-safe equal.
            let sql = format!(
                "SELECT {DEPLOYMENT_COLS_MYSQL} FROM action_deployments d
                 LEFT JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = ? AND d.environment = ? AND d.id <> ?
                   AND NOT ((SELECT s.state FROM action_deployment_statuses s
                     WHERE s.deployment_id = d.id ORDER BY s.created_at DESC, s.seq DESC LIMIT 1)
                    <=> 'inactive')
                 ORDER BY d.created_at"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repository_id)
                .bind(environment)
                .bind(except_deployment_id)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            rows.into_iter().map(|r| Ok(map_deployment!(r))).collect()
        }
    }
}
