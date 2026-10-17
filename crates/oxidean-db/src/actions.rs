//! Actions runners / runs / jobs / secrets persistence (Phase 19 / D-ACT-13 / D-ACT-17).

use crate::pool::DbPool;
use sqlx::Row;

#[derive(Debug, Clone)]
pub struct ActionRunnerRow {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub labels_json: String,
    pub owner_type: Option<String>,
    pub owner_id: Option<String>,
    pub repository_id: Option<String>,
    pub ephemeral: bool,
    pub last_online: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ActionRunRow {
    pub id: String,
    pub repository_id: String,
    pub workflow_path: String,
    pub workflow_name: String,
    pub event: String,
    pub head_sha: String,
    pub head_ref: String,
    pub status: String,
    pub title: String,
    /// 1-based per-workflow sequence number within the repository, computed at
    /// read time ([`RUN_NUMBER_SQL`]). 0 on insert-returned rows (never
    /// serialized — RPC paths always re-read).
    pub run_number: i64,
    pub triggered_by: Option<String>,
    /// Recorded PR head-repo provenance for `pull_request` runs (the PR's
    /// `head_repo_id`); `NULL` for every other event. Claim compares this to
    /// `repository_id` — a fork head, or a `pull_request` run with no recorded
    /// provenance, is untrusted (no secrets, read-only job token).
    pub head_repo_id: Option<String>,
    /// Re-run counter — 1 for the first attempt, bumped by `requeue_run`.
    /// Surfaces to runners as `run_attempt` (GITHUB_RUN_ATTEMPT).
    pub attempt: i64,
    /// Full triggering-event webhook payload (JSON); populated by the
    /// triggers phase for `${{ github.event.* }}` evaluation. NULL on runs
    /// dispatched before payloads were captured.
    pub event_payload_json: Option<String>,
    /// `concurrency:` group name when the workflow sets one (Phase 3).
    pub concurrency_group: Option<String>,
    /// Concurrency handoff flag (Phase 3): set when a superseding run is
    /// waiting for this run's jobs to finish.
    pub concurrency_cancel_pending: bool,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

/// GitHub-style optional filters for [`list_runs_for_repo`] /
/// [`count_runs_for_repo`]. `None` fields are not applied.
///
/// `branch` matches `head_ref` against both the bare name and the
/// `refs/heads/` form (dispatch stores short names, push events store full
/// refs). `triggered_by` carries the resolved `users.id` — the RPC layer
/// resolves the username first. `title_like` is a pre-wrapped `%pattern%`.
#[derive(Debug, Clone, Default)]
pub struct ActionRunFilter {
    pub status: Option<String>,
    pub event: Option<String>,
    pub branch: Option<String>,
    pub workflow_path: Option<String>,
    pub triggered_by: Option<String>,
    pub title_like: Option<String>,
}

/// Per-workflow 1-based run sequence computed at read time — the oldest run
/// of the same `(repository_id, workflow_path)` is 1. A correlated COUNT
/// avoids a schema migration/backfill; same-second `created_at` ties break
/// by `id` so numbers are stable.
pub const RUN_NUMBER_SQL: &str = "(SELECT COUNT(*) FROM action_runs rn2 WHERE rn2.repository_id = action_runs.repository_id AND rn2.workflow_path = action_runs.workflow_path AND (rn2.created_at < action_runs.created_at OR (rn2.created_at = action_runs.created_at AND rn2.id <= action_runs.id))) AS run_number";

#[derive(Debug, Clone)]
pub struct ActionJobRow {
    pub id: String,
    pub run_id: String,
    pub job_key: String,
    /// Matrix leg discriminator — `''` for non-matrix jobs (part of the
    /// `UNIQUE(run_id, job_key, matrix_key)` identity).
    pub matrix_key: String,
    pub name: String,
    pub runs_on_json: String,
    pub status: String,
    /// `action_payloads.sha` — steps/env/permissions captured at enqueue.
    /// `NULL` only for rows written before 0053 (claim fails them loudly).
    pub payload_sha: Option<String>,
    /// `["job-key", …]` upstream deps (Phase 3 `needs:` scheduling).
    pub needs_json: String,
    /// Expanded matrix values for this leg (Phase 3).
    pub matrix_json: String,
    /// `needs.*.outputs` materialized for downstream evaluation (Phase 3).
    pub outputs_json: String,
    /// `concurrency:` group at job level (Phase 3).
    pub concurrency_group: Option<String>,
    /// Claim counter — bumped by `requeue_run`.
    pub attempt: i64,
    /// Wall-clock deadline from `timeout-minutes` (Phase 3 enforcer).
    pub timeout_at: Option<String>,
    pub runner_id: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Insert-time job shape — the enqueue payload expansion writes these
/// columns; the matrix/needs/outputs fields default for non-scheduled jobs.
#[derive(Debug, Clone)]
pub struct JobInsert {
    pub id: String,
    pub run_id: String,
    pub job_key: String,
    pub matrix_key: String,
    pub name: String,
    /// Serialized `runs_on` array — also exploded into `action_job_labels`.
    pub runs_on_json: String,
    pub payload_sha: Option<String>,
    /// JSON array of upstream job keys (`needs:`); `'[]'` until Phase 3.
    pub needs_json: String,
    /// JSON object of this leg's matrix values; `'{}'` until Phase 3.
    pub matrix_json: String,
    /// JSON object of declared `outputs:` exprs; `'{}'` until Phase 3.
    pub outputs_json: String,
    pub concurrency_group: Option<String>,
    /// Initial status — `'queued'` normally, `'skipped'`/`'failure'` when
    /// enqueue-time expression evaluation decides the job never runs.
    pub status: String,
    /// `timeout-minutes` materialized to an absolute deadline at enqueue.
    pub timeout_at: Option<String>,
}

impl JobInsert {
    /// Pre-Phase-3 shape: a plain queued job with no scheduling metadata.
    pub fn basic(id: &str, run_id: &str, job_key: &str, name: &str, runs_on_json: &str) -> Self {
        Self {
            id: id.into(),
            run_id: run_id.into(),
            job_key: job_key.into(),
            matrix_key: String::new(),
            name: name.into(),
            runs_on_json: runs_on_json.into(),
            payload_sha: None,
            needs_json: "[]".into(),
            matrix_json: "{}".into(),
            outputs_json: "{}".into(),
            concurrency_group: None,
            status: "queued".into(),
            timeout_at: None,
        }
    }
}

/// `action_payloads` row — enqueue-captured job definition (dedup'd by sha).
#[derive(Debug, Clone)]
pub struct ActionPayloadRow {
    pub sha: String,
    pub repository_id: String,
    pub workflow_path: String,
    pub head_sha: String,
    pub payload_json: String,
    pub created_at: String,
}

/// Job SELECT-list per dialect — only timestamp casts differ. Keeping the
/// list in one const per dialect prevents column drift between
/// `find_job_by_id`, `list_jobs_for_run`, and the claim path.
const JOB_COLS_SQLITE: &str = "id, run_id, job_key, matrix_key, name, runs_on_json, status, payload_sha, needs_json, matrix_json, outputs_json, concurrency_group, attempt, timeout_at, runner_id, started_at, finished_at";
const JOB_COLS_PG: &str = "id, run_id, job_key, matrix_key, name, runs_on_json, status, payload_sha, needs_json, matrix_json, outputs_json, concurrency_group, attempt, to_char(timeout_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS timeout_at, runner_id, started_at::text AS started_at, finished_at::text AS finished_at";
const JOB_COLS_MYSQL: &str = "id, run_id, job_key, matrix_key, name, runs_on_json, status, payload_sha, needs_json, matrix_json, outputs_json, concurrency_group, attempt, DATE_FORMAT(timeout_at, '%Y-%m-%dT%H:%i:%sZ') AS timeout_at, runner_id, CAST(started_at AS CHAR) AS started_at, CAST(finished_at AS CHAR) AS finished_at";

macro_rules! map_job_row {
    ($row:expr) => {{
        let r = $row;
        ActionJobRow {
            id: r.get("id"),
            run_id: r.get("run_id"),
            job_key: r.get("job_key"),
            matrix_key: r.try_get("matrix_key").unwrap_or_default(),
            name: r.get("name"),
            runs_on_json: r.get("runs_on_json"),
            status: r.get("status"),
            payload_sha: r.try_get("payload_sha").unwrap_or(None),
            needs_json: r.try_get("needs_json").unwrap_or_else(|_| "[]".into()),
            matrix_json: r.try_get("matrix_json").unwrap_or_else(|_| "{}".into()),
            outputs_json: r.try_get("outputs_json").unwrap_or_else(|_| "{}".into()),
            concurrency_group: r.try_get("concurrency_group").unwrap_or(None),
            attempt: r
                .try_get::<i64, _>("attempt")
                .or_else(|_| r.try_get::<i32, _>("attempt").map(i64::from))
                .unwrap_or(1),
            timeout_at: r.try_get("timeout_at").unwrap_or(None),
            runner_id: r.get("runner_id"),
            started_at: r.try_get("started_at").unwrap_or(None),
            finished_at: r.try_get("finished_at").unwrap_or(None),
            created_at: r.try_get("created_at").unwrap_or_default(),
            updated_at: r.try_get("updated_at").unwrap_or_default(),
        }
    }};
}

/// Run SELECT-list body (before `RUN_NUMBER_SQL` and the dialect timestamp
/// casts) — shared across all three dialects since every added column is a
/// plain TEXT/INTEGER/BOOLEAN read.
const RUN_COLS: &str = "id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, status, title, triggered_by, head_repo_id, attempt, event_payload_json, concurrency_group, concurrency_cancel_pending";

macro_rules! map_run_row {
    ($row:expr) => {{
        let r = $row;
        ActionRunRow {
            id: r.get("id"),
            repository_id: r.get("repository_id"),
            workflow_path: r.get("workflow_path"),
            workflow_name: r.get("workflow_name"),
            event: r.get("event"),
            head_sha: r.get("head_sha"),
            head_ref: r.get("head_ref"),
            status: r.get("status"),
            title: r.get("title"),
            run_number: r.try_get("run_number").unwrap_or(0),
            triggered_by: r.get("triggered_by"),
            head_repo_id: r.get("head_repo_id"),
            attempt: r
                .try_get::<i64, _>("attempt")
                .or_else(|_| r.try_get::<i32, _>("attempt").map(i64::from))
                .unwrap_or(1),
            event_payload_json: r.try_get("event_payload_json").unwrap_or(None),
            concurrency_group: r.try_get("concurrency_group").unwrap_or(None),
            concurrency_cancel_pending: r
                .try_get::<bool, _>("concurrency_cancel_pending")
                .ok()
                .or_else(|| {
                    r.try_get::<i64, _>("concurrency_cancel_pending")
                        .ok()
                        .map(|v| v != 0)
                })
                .or_else(|| {
                    r.try_get::<i8, _>("concurrency_cancel_pending")
                        .ok()
                        .map(|v| v != 0)
                })
                .unwrap_or(false),
            created_at: r.try_get("created_at").unwrap_or_default(),
            updated_at: r.try_get("updated_at").unwrap_or_default(),
            finished_at: r.try_get("finished_at").unwrap_or(None),
        }
    }};
}

#[derive(Debug, Clone)]
pub struct ActionSecretMetaRow {
    pub id: String,
    pub repository_id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ActionSecretCipherRow {
    pub name: String,
    pub ciphertext: String,
}

pub async fn insert_runner(
    pool: &DbPool,
    id: &str,
    name: &str,
    token_hash: &str,
    labels_json: &str,
    repository_id: Option<&str>,
    ephemeral: bool,
) -> Result<ActionRunnerRow, String> {
    let eph_i = if ephemeral { 1i64 } else { 0 };
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO action_runners
                   (id, name, token_hash, labels_json, repository_id, ephemeral)
                   VALUES ($1, $2, $3, $4, $5, $6)"#,
            )
            .bind(id)
            .bind(name)
            .bind(token_hash)
            .bind(labels_json)
            .bind(repository_id)
            .bind(ephemeral)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT INTO action_runners
                   (id, name, token_hash, labels_json, repository_id, ephemeral)
                   VALUES (?, ?, ?, ?, ?, ?)"#,
            )
            .bind(id)
            .bind(name)
            .bind(token_hash)
            .bind(labels_json)
            .bind(repository_id)
            .bind(eph_i as i8)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"INSERT INTO action_runners
                   (id, name, token_hash, labels_json, repository_id, ephemeral)
                   VALUES (?, ?, ?, ?, ?, ?)"#,
            )
            .bind(id)
            .bind(name)
            .bind(token_hash)
            .bind(labels_json)
            .bind(repository_id)
            .bind(eph_i)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(ActionRunnerRow {
        id: id.into(),
        name: name.into(),
        token_hash: token_hash.into(),
        labels_json: labels_json.into(),
        owner_type: None,
        owner_id: None,
        repository_id: repository_id.map(str::to_string),
        ephemeral,
        last_online: None,
        created_at: String::new(),
        updated_at: String::new(),
    })
}

pub async fn find_runner_by_id(pool: &DbPool, id: &str) -> Result<Option<ActionRunnerRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral,
                          to_char(last_online AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_online,
                          to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                          to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                   FROM action_runners WHERE id = $1"#,
            ).bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"),
                name: r.get("name"),
                token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                repository_id: r.get("repository_id"),
                ephemeral: r.get("ephemeral"),
                last_online: r.get("last_online"),
                created_at: r.try_get("created_at").unwrap_or_default(),
                updated_at: r.try_get("updated_at").unwrap_or_default(),
            }))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral,
                          DATE_FORMAT(last_online, '%Y-%m-%dT%H:%i:%sZ') AS last_online,
                          DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
                          DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
                   FROM action_runners WHERE id = ?"#,
            ).bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"),
                name: r.get("name"),
                token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                repository_id: r.get("repository_id"),
                ephemeral: r
                    .try_get::<i8, _>("ephemeral")
                    .map(|i| i != 0)
                    .unwrap_or(false),
                last_online: r.get("last_online"),
                created_at: r.try_get("created_at").unwrap_or_default(),
                updated_at: r.try_get("updated_at").unwrap_or_default(),
            }))
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id,
                          ephemeral, last_online,
                          strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
                          strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
                   FROM action_runners WHERE id = ?"#,
            )
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"),
                name: r.get("name"),
                token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                repository_id: r.get("repository_id"),
                ephemeral: r
                    .try_get::<i64, _>("ephemeral")
                    .map(|i| i != 0)
                    .unwrap_or(false),
                last_online: r.get("last_online"),
                created_at: r.try_get("created_at").unwrap_or_default(),
                updated_at: r.try_get("updated_at").unwrap_or_default(),
            }))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_run(
    pool: &DbPool,
    id: &str,
    repository_id: &str,
    workflow_path: &str,
    workflow_name: &str,
    event: &str,
    head_sha: &str,
    head_ref: &str,
    title: &str,
    triggered_by: Option<&str>,
    head_repo_id: Option<&str>,
) -> Result<ActionRunRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(r#"INSERT INTO action_runs
                (id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, title, triggered_by, head_repo_id)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#)
                .bind(id).bind(repository_id).bind(workflow_path).bind(workflow_name)
                .bind(event).bind(head_sha).bind(head_ref).bind(title).bind(triggered_by)
                .bind(head_repo_id)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(r#"INSERT INTO action_runs
                (id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, title, triggered_by, head_repo_id)
                VALUES (?,?,?,?,?,?,?,?,?,?)"#)
                .bind(id).bind(repository_id).bind(workflow_path).bind(workflow_name)
                .bind(event).bind(head_sha).bind(head_ref).bind(title).bind(triggered_by)
                .bind(head_repo_id)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(r#"INSERT INTO action_runs
                (id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, title, triggered_by, head_repo_id)
                VALUES (?,?,?,?,?,?,?,?,?,?)"#)
                .bind(id).bind(repository_id).bind(workflow_path).bind(workflow_name)
                .bind(event).bind(head_sha).bind(head_ref).bind(title).bind(triggered_by)
                .bind(head_repo_id)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(ActionRunRow {
        id: id.into(),
        repository_id: repository_id.into(),
        workflow_path: workflow_path.into(),
        workflow_name: workflow_name.into(),
        event: event.into(),
        head_sha: head_sha.into(),
        head_ref: head_ref.into(),
        status: "queued".into(),
        title: title.into(),
        run_number: 0,
        triggered_by: triggered_by.map(str::to_string),
        head_repo_id: head_repo_id.map(str::to_string),
        attempt: 1,
        event_payload_json: None,
        concurrency_group: None,
        concurrency_cancel_pending: false,
        created_at: String::new(),
        updated_at: String::new(),
        finished_at: None,
    })
}

pub async fn find_run_by_id(pool: &DbPool, id: &str) -> Result<Option<ActionRunRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {RUN_COLS}, finished_at::text AS finished_at, {RUN_NUMBER_SQL} FROM action_runs WHERE id = $1")))
                .bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_run_row!(r)))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {RUN_COLS}, CAST(finished_at AS CHAR) AS finished_at, {RUN_NUMBER_SQL} FROM action_runs WHERE id = ?")))
                .bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_run_row!(r)))
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {RUN_COLS}, finished_at, {RUN_NUMBER_SQL} FROM action_runs WHERE id = ?"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_run_row!(r)))
        }
    }
}

/// Label match key — `labels_match` semantics: `have_base == base || have ==
/// need` reduces to `base(need) IN base(runner_labels)` since `have == need`
/// implies equal bases. `action_job_labels.base` stores this value.
fn label_base(label: &str) -> &str {
    label.split(':').next().unwrap_or(label)
}

/// Insert a job row plus its normalized claim labels in one transaction so a
/// crash between writes cannot leave a queued job invisible to claim (empty
/// label set would match every runner) — or an unclaimable labelled one.
pub async fn insert_job(pool: &DbPool, job: &JobInsert) -> Result<ActionJobRow, String> {
    let labels: Vec<String> = serde_json::from_str(&job.runs_on_json).unwrap_or_default();
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            sqlx::query(r#"INSERT INTO action_jobs
                (id, run_id, job_key, matrix_key, name, runs_on_json, payload_sha, needs_json, matrix_json, outputs_json, concurrency_group, status, timeout_at)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13::timestamptz)"#)
                .bind(&job.id).bind(&job.run_id).bind(&job.job_key).bind(&job.matrix_key)
                .bind(&job.name).bind(&job.runs_on_json).bind(&job.payload_sha)
                .bind(&job.needs_json).bind(&job.matrix_json).bind(&job.outputs_json)
                .bind(&job.concurrency_group).bind(&job.status).bind(&job.timeout_at)
                .execute(&mut *tx).await.map_err(|e| e.to_string())?;
            for label in &labels {
                sqlx::query("INSERT INTO action_job_labels (job_id, label, base) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
                    .bind(&job.id).bind(label).bind(label_base(label))
                    .execute(&mut *tx).await.map_err(|e| e.to_string())?;
            }
            tx.commit().await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            sqlx::query(r#"INSERT INTO action_jobs
                (id, run_id, job_key, matrix_key, name, runs_on_json, payload_sha, needs_json, matrix_json, outputs_json, concurrency_group, status, timeout_at)
                VALUES (?,?,?,?,?,?,?,?,?,?,?,?,STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'))"#)
                .bind(&job.id).bind(&job.run_id).bind(&job.job_key).bind(&job.matrix_key)
                .bind(&job.name).bind(&job.runs_on_json).bind(&job.payload_sha)
                .bind(&job.needs_json).bind(&job.matrix_json).bind(&job.outputs_json)
                .bind(&job.concurrency_group).bind(&job.status).bind(&job.timeout_at)
                .execute(&mut *tx).await.map_err(|e| e.to_string())?;
            for label in &labels {
                sqlx::query(
                    "INSERT IGNORE INTO action_job_labels (job_id, label, base) VALUES (?,?,?)",
                )
                .bind(&job.id)
                .bind(label)
                .bind(label_base(label))
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            }
            tx.commit().await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            sqlx::query(r#"INSERT INTO action_jobs
                (id, run_id, job_key, matrix_key, name, runs_on_json, payload_sha, needs_json, matrix_json, outputs_json, concurrency_group, status, timeout_at)
                VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)"#)
                .bind(&job.id).bind(&job.run_id).bind(&job.job_key).bind(&job.matrix_key)
                .bind(&job.name).bind(&job.runs_on_json).bind(&job.payload_sha)
                .bind(&job.needs_json).bind(&job.matrix_json).bind(&job.outputs_json)
                .bind(&job.concurrency_group).bind(&job.status).bind(&job.timeout_at)
                .execute(&mut *tx).await.map_err(|e| e.to_string())?;
            for label in &labels {
                sqlx::query(
                    "INSERT OR IGNORE INTO action_job_labels (job_id, label, base) VALUES (?,?,?)",
                )
                .bind(&job.id)
                .bind(label)
                .bind(label_base(label))
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            }
            tx.commit().await.map_err(|e| e.to_string())?;
        }
    }
    Ok(ActionJobRow {
        id: job.id.clone(),
        run_id: job.run_id.clone(),
        job_key: job.job_key.clone(),
        matrix_key: job.matrix_key.clone(),
        name: job.name.clone(),
        runs_on_json: job.runs_on_json.clone(),
        status: job.status.clone(),
        payload_sha: job.payload_sha.clone(),
        needs_json: job.needs_json.clone(),
        matrix_json: job.matrix_json.clone(),
        outputs_json: job.outputs_json.clone(),
        concurrency_group: job.concurrency_group.clone(),
        attempt: 1,
        timeout_at: job.timeout_at.clone(),
        runner_id: None,
        started_at: None,
        finished_at: None,
        created_at: String::new(),
        updated_at: String::new(),
    })
}

/// Record the workflow-level `concurrency:` group on the run (evaluated at
/// enqueue; the scheduler phase enforces cancellation against it).
pub async fn set_run_concurrency(pool: &DbPool, run_id: &str, group: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE action_runs SET concurrency_group = $1, updated_at = now() WHERE id = $2",
            )
            .bind(group)
            .bind(run_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_runs SET concurrency_group = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(group).bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_runs SET concurrency_group = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(group).bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Upsert a captured job payload — content-addressed by sha, so identical
/// job definitions across runs/reruns share one row.
pub async fn insert_payload(
    pool: &DbPool,
    sha: &str,
    repository_id: &str,
    workflow_path: &str,
    head_sha: &str,
    payload_json: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("INSERT INTO action_payloads (sha, repository_id, workflow_path, head_sha, payload_json) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (sha) DO NOTHING")
                .bind(sha).bind(repository_id).bind(workflow_path).bind(head_sha).bind(payload_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("INSERT IGNORE INTO action_payloads (sha, repository_id, workflow_path, head_sha, payload_json) VALUES (?,?,?,?,?)")
                .bind(sha).bind(repository_id).bind(workflow_path).bind(head_sha).bind(payload_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT OR IGNORE INTO action_payloads (sha, repository_id, workflow_path, head_sha, payload_json) VALUES (?,?,?,?,?)")
                .bind(sha).bind(repository_id).bind(workflow_path).bind(head_sha).bind(payload_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn find_payload(pool: &DbPool, sha: &str) -> Result<Option<ActionPayloadRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT sha, repository_id, workflow_path, head_sha, payload_json, created_at::text AS created_at FROM action_payloads WHERE sha = $1")
                .bind(sha).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionPayloadRow {
                sha: r.get("sha"),
                repository_id: r.get("repository_id"),
                workflow_path: r.get("workflow_path"),
                head_sha: r.get("head_sha"),
                payload_json: r.get("payload_json"),
                created_at: r.try_get("created_at").unwrap_or_default(),
            }))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT sha, repository_id, workflow_path, head_sha, payload_json, CAST(created_at AS CHAR) AS created_at FROM action_payloads WHERE sha = ?")
                .bind(sha).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionPayloadRow {
                sha: r.get("sha"),
                repository_id: r.get("repository_id"),
                workflow_path: r.get("workflow_path"),
                head_sha: r.get("head_sha"),
                payload_json: r.get("payload_json"),
                created_at: r.try_get("created_at").unwrap_or_default(),
            }))
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT sha, repository_id, workflow_path, head_sha, payload_json, created_at FROM action_payloads WHERE sha = ?")
                .bind(sha).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionPayloadRow {
                sha: r.get("sha"),
                repository_id: r.get("repository_id"),
                workflow_path: r.get("workflow_path"),
                head_sha: r.get("head_sha"),
                payload_json: r.get("payload_json"),
                created_at: r.try_get("created_at").unwrap_or_default(),
            }))
        }
    }
}

pub async fn find_job_by_id(pool: &DbPool, id: &str) -> Result<Option<ActionJobRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {JOB_COLS_PG} FROM action_jobs WHERE id = $1"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_job_row!(r)))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {JOB_COLS_MYSQL} FROM action_jobs WHERE id = ?"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_job_row!(r)))
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {JOB_COLS_SQLITE} FROM action_jobs WHERE id = ?"
            )))
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_job_row!(r)))
        }
    }
}

pub async fn insert_secret(
    pool: &DbPool,
    id: &str,
    repository_id: &str,
    name: &str,
    ciphertext: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(r#"INSERT INTO action_secrets (id, repository_id, name, ciphertext) VALUES ($1,$2,$3,$4)"#)
                .bind(id).bind(repository_id).bind(name).bind(ciphertext)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(r#"INSERT INTO action_secrets (id, repository_id, name, ciphertext) VALUES (?,?,?,?)"#)
                .bind(id).bind(repository_id).bind(name).bind(ciphertext)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(r#"INSERT INTO action_secrets (id, repository_id, name, ciphertext) VALUES (?,?,?,?)"#)
                .bind(id).bind(repository_id).bind(name).bind(ciphertext)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn list_secret_ciphertexts(
    pool: &DbPool,
    repository_id: &str,
) -> Result<Vec<ActionSecretCipherRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT name, ciphertext FROM action_secrets WHERE repository_id = $1 ORDER BY name",
            )
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|row| ActionSecretCipherRow {
                    name: row.get("name"),
                    ciphertext: row.get("ciphertext"),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT name, ciphertext FROM action_secrets WHERE repository_id = ? ORDER BY name",
            )
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|row| ActionSecretCipherRow {
                    name: row.get("name"),
                    ciphertext: row.get("ciphertext"),
                })
                .collect())
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT name, ciphertext FROM action_secrets WHERE repository_id = ? ORDER BY name",
            )
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|row| ActionSecretCipherRow {
                    name: row.get("name"),
                    ciphertext: row.get("ciphertext"),
                })
                .collect())
        }
    }
}

pub async fn list_secret_names(
    pool: &DbPool,
    repository_id: &str,
) -> Result<Vec<ActionSecretMetaRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT id, repository_id, name,
                          to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                          to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                   FROM action_secrets WHERE repository_id = $1 ORDER BY name"#,
            ).bind(repository_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|row| ActionSecretMetaRow {
                    id: row.get("id"),
                    repository_id: row.get("repository_id"),
                    name: row.get("name"),
                    created_at: row.try_get("created_at").unwrap_or_default(),
                    updated_at: row.try_get("updated_at").unwrap_or_default(),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT id, repository_id, name,
                          DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
                          DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
                   FROM action_secrets WHERE repository_id = ? ORDER BY name"#,
            )
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|row| ActionSecretMetaRow {
                    id: row.get("id"),
                    repository_id: row.get("repository_id"),
                    name: row.get("name"),
                    created_at: row.try_get("created_at").unwrap_or_default(),
                    updated_at: row.try_get("updated_at").unwrap_or_default(),
                })
                .collect())
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT id, repository_id, name,
                          strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
                          strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
                   FROM action_secrets WHERE repository_id = ? ORDER BY name"#,
            )
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|row| ActionSecretMetaRow {
                    id: row.get("id"),
                    repository_id: row.get("repository_id"),
                    name: row.get("name"),
                    created_at: row.try_get("created_at").unwrap_or_default(),
                    updated_at: row.try_get("updated_at").unwrap_or_default(),
                })
                .collect())
        }
    }
}

pub async fn delete_secret_by_name(
    pool: &DbPool,
    repository_id: &str,
    name: &str,
) -> Result<bool, String> {
    match pool {
        DbPool::Postgres(p) => {
            let r =
                sqlx::query("DELETE FROM action_secrets WHERE repository_id = $1 AND name = $2")
                    .bind(repository_id)
                    .bind(name)
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(r.rows_affected() > 0)
        }
        DbPool::MySql(p) => {
            let r = sqlx::query("DELETE FROM action_secrets WHERE repository_id = ? AND name = ?")
                .bind(repository_id)
                .bind(name)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(r.rows_affected() > 0)
        }
        DbPool::Sqlite(p) => {
            let r = sqlx::query("DELETE FROM action_secrets WHERE repository_id = ? AND name = ?")
                .bind(repository_id)
                .bind(name)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(r.rows_affected() > 0)
        }
    }
}

pub async fn list_runners(pool: &DbPool) -> Result<Vec<ActionRunnerRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral,
                          to_char(last_online AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_online,
                          to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                          to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                   FROM action_runners ORDER BY name"#,
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|r| ActionRunnerRow {
                    id: r.get("id"),
                    name: r.get("name"),
                    token_hash: r.get("token_hash"),
                    labels_json: r.get("labels_json"),
                    owner_type: r.try_get("owner_type").ok(),
                    owner_id: r.try_get("owner_id").ok(),
                    repository_id: r.try_get("repository_id").ok(),
                    ephemeral: r.try_get::<bool, _>("ephemeral").unwrap_or(false),
                    last_online: r.try_get("last_online").ok(),
                    created_at: r.try_get("created_at").unwrap_or_default(),
                    updated_at: r.try_get("updated_at").unwrap_or_default(),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral,
                          DATE_FORMAT(last_online, '%Y-%m-%dT%H:%i:%sZ') AS last_online,
                          DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
                          DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
                   FROM action_runners ORDER BY name"#,
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|r| {
                    let eph: i8 = r.try_get("ephemeral").unwrap_or(0);
                    ActionRunnerRow {
                        id: r.get("id"),
                        name: r.get("name"),
                        token_hash: r.get("token_hash"),
                        labels_json: r.get("labels_json"),
                        owner_type: r.try_get("owner_type").ok(),
                        owner_id: r.try_get("owner_id").ok(),
                        repository_id: r.try_get("repository_id").ok(),
                        ephemeral: eph != 0,
                        last_online: r.try_get("last_online").ok(),
                        created_at: r.try_get("created_at").unwrap_or_default(),
                        updated_at: r.try_get("updated_at").unwrap_or_default(),
                    }
                })
                .collect())
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral,
                          strftime('%Y-%m-%dT%H:%M:%SZ', last_online) AS last_online,
                          strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
                          strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
                   FROM action_runners ORDER BY name"#,
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|r| {
                    let eph: i64 = r.try_get("ephemeral").unwrap_or(0);
                    ActionRunnerRow {
                        id: r.get("id"),
                        name: r.get("name"),
                        token_hash: r.get("token_hash"),
                        labels_json: r.get("labels_json"),
                        owner_type: r.try_get("owner_type").ok(),
                        owner_id: r.try_get("owner_id").ok(),
                        repository_id: r.try_get("repository_id").ok(),
                        ephemeral: eph != 0,
                        last_online: r.try_get("last_online").ok(),
                        created_at: r.try_get("created_at").unwrap_or_default(),
                        updated_at: r.try_get("updated_at").unwrap_or_default(),
                    }
                })
                .collect())
        }
    }
}

pub async fn insert_runner_token(
    pool: &DbPool,
    id: &str,
    token_hash: &str,
    scope_type: &str,
    scope_id: Option<&str>,
    active: bool,
) -> Result<(), String> {
    let active_i = if active { 1i64 } else { 0 };
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(r#"INSERT INTO action_runner_tokens (id, token_hash, scope_type, scope_id, active) VALUES ($1,$2,$3,$4,$5)"#)
                .bind(id).bind(token_hash).bind(scope_type).bind(scope_id).bind(active)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(r#"INSERT INTO action_runner_tokens (id, token_hash, scope_type, scope_id, active) VALUES (?,?,?,?,?)"#)
                .bind(id).bind(token_hash).bind(scope_type).bind(scope_id).bind(active_i as i8)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(r#"INSERT INTO action_runner_tokens (id, token_hash, scope_type, scope_id, active) VALUES (?,?,?,?,?)"#)
                .bind(id).bind(token_hash).bind(scope_type).bind(scope_id).bind(active_i)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn wipe_actions_domain(pool: &DbPool) -> Result<(), String> {
    for sql in [
        "DELETE FROM action_job_tokens",
        "DELETE FROM action_job_labels",
        "DELETE FROM action_jobs",
        "DELETE FROM action_payloads",
        "DELETE FROM action_runs",
        "DELETE FROM action_secrets",
        "DELETE FROM action_runners",
        "DELETE FROM action_runner_tokens",
    ] {
        match pool {
            DbPool::Postgres(p) => {
                sqlx::query(sql)
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            DbPool::MySql(p) => {
                sqlx::query(sql)
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            DbPool::Sqlite(p) => {
                sqlx::query(sql)
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

pub async fn get_actions_enabled(pool: &DbPool, repo_id: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT actions_enabled FROM repositories WHERE id = ?")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| r.try_get::<i64, _>("actions_enabled").unwrap_or(1) != 0)
                .unwrap_or(true))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT actions_enabled FROM repositories WHERE id = $1")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| r.try_get::<bool, _>("actions_enabled").unwrap_or(true))
                .unwrap_or(true))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT actions_enabled FROM repositories WHERE id = ?")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| r.try_get::<i8, _>("actions_enabled").unwrap_or(1) != 0)
                .unwrap_or(true))
        }
    }
}

pub async fn set_actions_enabled(
    pool: &DbPool,
    repo_id: &str,
    enabled: bool,
) -> Result<(), String> {
    let v = if enabled { 1i64 } else { 0 };
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE repositories SET actions_enabled = ? WHERE id = ?")
                .bind(v)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE repositories SET actions_enabled = $1 WHERE id = $2")
                .bind(enabled)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE repositories SET actions_enabled = ? WHERE id = ?")
                .bind(v as i8)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn consume_registration_token(pool: &DbPool, token_hash: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let res = sqlx::query(
                "UPDATE action_runner_tokens SET active = 0 WHERE token_hash = ? AND active = 1",
            )
            .bind(token_hash)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::Postgres(p) => {
            let res = sqlx::query(
                "UPDATE action_runner_tokens SET active = false WHERE token_hash = $1 AND active = true",
            )
            .bind(token_hash)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::MySql(p) => {
            let res = sqlx::query(
                "UPDATE action_runner_tokens SET active = 0 WHERE token_hash = ? AND active = 1",
            )
            .bind(token_hash)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
    }
}

pub async fn find_runner_by_token_hash(
    pool: &DbPool,
    token_hash: &str,
) -> Result<Option<ActionRunnerRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral, strftime('%Y-%m-%dT%H:%M:%SZ', last_online) AS last_online FROM action_runners WHERE token_hash = ?",
            )
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"),
                name: r.get("name"),
                token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                repository_id: r.get("repository_id"),
                ephemeral: r
                    .try_get::<i64, _>("ephemeral")
                    .map(|i| i != 0)
                    .unwrap_or(false),
                last_online: r.get("last_online"),
                created_at: String::new(),
                updated_at: String::new(),
            }))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral, to_char(last_online AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_online FROM action_runners WHERE token_hash = $1",
            )
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"),
                name: r.get("name"),
                token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                repository_id: r.get("repository_id"),
                ephemeral: r.get("ephemeral"),
                last_online: r.get("last_online"),
                created_at: String::new(),
                updated_at: String::new(),
            }))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral, DATE_FORMAT(last_online, '%Y-%m-%dT%H:%i:%sZ') AS last_online FROM action_runners WHERE token_hash = ?",
            )
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"),
                name: r.get("name"),
                token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                repository_id: r.get("repository_id"),
                ephemeral: r
                    .try_get::<i8, _>("ephemeral")
                    .map(|i| i != 0)
                    .unwrap_or(false),
                last_online: r.get("last_online"),
                created_at: String::new(),
                updated_at: String::new(),
            }))
        }
    }
}

/// Claim the oldest queued job whose labels are satisfied by `runner_labels`.
///
/// Atomic per dialect — no Rust-side scan-then-CAS window:
/// - Postgres: one `UPDATE … WHERE id = (SELECT … FOR UPDATE SKIP LOCKED)`
///   so contending runners hand off distinct jobs without retrying.
/// - SQLite: single-writer anyway; one `UPDATE … WHERE id = (SELECT … LIMIT 1)
///   AND status='queued' … RETURNING` keeps the same single-statement shape.
/// - MySQL (no `UPDATE … RETURNING` / `SKIP LOCKED`): select a small candidate
///   set, CAS each row (`WHERE id = ? AND status='queued'`), re-select on a
///   lost race. Bounded — a lost candidate set returns `None` and the runner
///   retries on its next `fetch_task` poll.
///
/// Label predicate: `labels_match`'s `have_base == base || have == need`
/// reduces to `base(need) IN base(runner_labels)` (equality implies equal
/// bases), so claim compares `action_job_labels.base` against the runner's
/// label bases — precomputed here once per call.
pub async fn claim_queued_job_for_labels(
    pool: &DbPool,
    runner_id: &str,
    runner_labels: &[String],
) -> Result<Option<ActionJobRow>, String> {
    let bases: Vec<String> = runner_labels
        .iter()
        .map(|l| label_base(l).to_string())
        .collect();
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = $1, \
                 started_at = COALESCE(started_at, now()), updated_at = now() \
                 WHERE id = (SELECT j.id FROM action_jobs j \
                   WHERE j.status = 'queued' AND NOT EXISTS ( \
                     SELECT 1 FROM action_job_labels l WHERE l.job_id = j.id \
                     AND NOT (l.base = ANY($2::text[]))) \
                   ORDER BY j.created_at LIMIT 1 FOR UPDATE SKIP LOCKED) \
                 AND status = 'queued' RETURNING {JOB_COLS_PG}"
            )))
            .bind(runner_id)
            .bind(&bases)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_job_row!(r)))
        }
        DbPool::MySql(p) => {
            let bases_json = serde_json::to_string(&bases).unwrap_or_else(|_| "[]".into());
            for _ in 0..4 {
                let candidates: Vec<String> = sqlx::query_scalar(
                    "SELECT j.id FROM action_jobs j \
                     WHERE j.status = 'queued' AND NOT EXISTS ( \
                       SELECT 1 FROM action_job_labels l WHERE l.job_id = j.id \
                       AND l.base NOT IN (SELECT value FROM JSON_TABLE(?, '$[*]' \
                         COLUMNS (value VARCHAR(255) PATH '$')) AS lb)) \
                     ORDER BY j.created_at LIMIT 5",
                )
                .bind(&bases_json)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
                if candidates.is_empty() {
                    return Ok(None);
                }
                for id in candidates {
                    if assign_job_runner(pool, &id, runner_id).await? {
                        return find_job_by_id(pool, &id).await;
                    }
                }
            }
            Ok(None)
        }
        DbPool::Sqlite(p) => {
            let bases_json = serde_json::to_string(&bases).unwrap_or_else(|_| "[]".into());
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = ?1, \
                 started_at = COALESCE(started_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP \
                 WHERE id = (SELECT j.id FROM action_jobs j \
                   WHERE j.status = 'queued' AND NOT EXISTS ( \
                     SELECT 1 FROM action_job_labels l WHERE l.job_id = j.id \
                     AND l.base NOT IN (SELECT value FROM json_each(?2))) \
                   ORDER BY j.created_at LIMIT 1) \
                 AND status = 'queued' RETURNING {JOB_COLS_SQLITE}"
            )))
            .bind(runner_id)
            .bind(bases_json)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_job_row!(r)))
        }
    }
}

/// CAS a single queued job to in_progress for `runner_id` — the MySQL claim
/// leg and any future targeted claim (per-id) share this.
async fn assign_job_runner(pool: &DbPool, job_id: &str, runner_id: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let res = sqlx::query(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = ?, started_at = COALESCE(started_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status = 'queued'",
            )
            .bind(runner_id)
            .bind(job_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::Postgres(p) => {
            let res = sqlx::query(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = $1, started_at = COALESCE(started_at, now()), updated_at = now() WHERE id = $2 AND status = 'queued'",
            )
            .bind(runner_id)
            .bind(job_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::MySql(p) => {
            let res = sqlx::query(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = ?, started_at = COALESCE(started_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status = 'queued'",
            )
            .bind(runner_id)
            .bind(job_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
    }
}

/// Shared run-list filter shape — `(param IS NULL OR col = param)` so a single
/// statement serves filtered and unfiltered reads (same shape as issues).
/// Macros (not fns) so `query`/`query_scalar`/`query_as` all bind cleanly and
/// each dialect keeps its own placeholder style (`?` positional vs `$N`).
macro_rules! bind_run_filter {
    ($q:expr, $f:expr, $branch_ref:expr $(,)?) => {{
        let f = $f;
        let br = $branch_ref;
        $q.bind(f.status.as_deref())
            .bind(f.status.as_deref())
            .bind(f.event.as_deref())
            .bind(f.event.as_deref())
            .bind(f.branch.as_deref())
            .bind(f.branch.as_deref())
            .bind(br.as_deref())
            .bind(f.workflow_path.as_deref())
            .bind(f.workflow_path.as_deref())
            .bind(f.triggered_by.as_deref())
            .bind(f.triggered_by.as_deref())
            .bind(f.title_like.as_deref())
            .bind(f.title_like.as_deref())
    }};
}

/// Postgres variant — numbered params let each filter value bind once.
macro_rules! bind_run_filter_pg {
    ($q:expr, $f:expr, $branch_ref:expr $(,)?) => {{
        let f = $f;
        let br = $branch_ref;
        $q.bind(f.status.as_deref())
            .bind(f.event.as_deref())
            .bind(f.branch.as_deref())
            .bind(br.as_deref())
            .bind(f.workflow_path.as_deref())
            .bind(f.triggered_by.as_deref())
            .bind(f.title_like.as_deref())
    }};
}

const RUNS_FILTER_SQLITE: &str = " AND (? IS NULL OR status = ?) AND (? IS NULL OR event = ?) AND (? IS NULL OR head_ref = ? OR head_ref = ?) AND (? IS NULL OR workflow_path = ?) AND (? IS NULL OR triggered_by = ?) AND (? IS NULL OR title LIKE ? COLLATE NOCASE)";
const RUNS_FILTER_MYSQL: &str = " AND (? IS NULL OR status = ?) AND (? IS NULL OR event = ?) AND (? IS NULL OR head_ref = ? OR head_ref = ?) AND (? IS NULL OR workflow_path = ?) AND (? IS NULL OR triggered_by = ?) AND (? IS NULL OR title LIKE ?)";
const RUNS_FILTER_PG: &str = " AND ($2::text IS NULL OR status = $2) AND ($3::text IS NULL OR event = $3) AND ($4::text IS NULL OR head_ref = $4 OR head_ref = $5) AND ($6::text IS NULL OR workflow_path = $6) AND ($7::text IS NULL OR triggered_by = $7) AND ($8::text IS NULL OR title ILIKE $8)";

/// `refs/heads/` variant of `filter.branch` for the third branch bind.
fn branch_ref_of(f: &ActionRunFilter) -> Option<String> {
    f.branch.as_ref().map(|b| {
        let b = b.strip_prefix("refs/heads/").unwrap_or(b);
        format!("refs/heads/{b}")
    })
}

pub async fn list_runs_for_repo(
    pool: &DbPool,
    repository_id: &str,
    filter: &ActionRunFilter,
    limit: i64,
    offset: i64,
) -> Result<Vec<ActionRunRow>, String> {
    let branch_ref = branch_ref_of(filter);
    match pool {
        DbPool::Sqlite(p) => {
            let rows = bind_run_filter!(
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "SELECT {RUN_COLS}, created_at, updated_at, finished_at, {RUN_NUMBER_SQL} FROM action_runs WHERE repository_id = ?{RUNS_FILTER_SQLITE} ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?"
                )))
                .bind(repository_id),
                filter,
                &branch_ref,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_run_row!(r)).collect())
        }
        DbPool::Postgres(p) => {
            let rows = bind_run_filter_pg!(
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "SELECT {RUN_COLS}, created_at::text AS created_at, updated_at::text AS updated_at, finished_at::text AS finished_at, {RUN_NUMBER_SQL} FROM action_runs WHERE repository_id = $1{RUNS_FILTER_PG} ORDER BY created_at DESC, id DESC LIMIT $9 OFFSET $10"
                )))
                .bind(repository_id),
                filter,
                &branch_ref,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_run_row!(r)).collect())
        }
        DbPool::MySql(p) => {
            let rows = bind_run_filter!(
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "SELECT {RUN_COLS}, CAST(created_at AS CHAR) AS created_at, CAST(updated_at AS CHAR) AS updated_at, CAST(finished_at AS CHAR) AS finished_at, {RUN_NUMBER_SQL} FROM action_runs WHERE repository_id = ?{RUNS_FILTER_MYSQL} ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?"
                )))
                .bind(repository_id),
                filter,
                &branch_ref,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_run_row!(r)).collect())
        }
    }
}

pub async fn list_jobs_for_run(pool: &DbPool, run_id: &str) -> Result<Vec<ActionJobRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {JOB_COLS_SQLITE} FROM action_jobs WHERE run_id = ?"
            )))
            .bind(run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_job_row!(r)).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {JOB_COLS_PG} FROM action_jobs WHERE run_id = $1"
            )))
            .bind(run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_job_row!(r)).collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {JOB_COLS_MYSQL} FROM action_jobs WHERE run_id = ?"
            )))
            .bind(run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_job_row!(r)).collect())
        }
    }
}

pub async fn update_job_status(pool: &DbPool, job_id: &str, status: &str) -> Result<(), String> {
    let in_progress = status == "in_progress";
    let terminal = matches!(status, "success" | "failure" | "cancelled" | "skipped");
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_jobs SET status = ?, started_at = CASE WHEN ? AND started_at IS NULL THEN CURRENT_TIMESTAMP ELSE started_at END, finished_at = CASE WHEN ? THEN CURRENT_TIMESTAMP ELSE finished_at END, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(status).bind(in_progress).bind(terminal).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_jobs SET status = $1, started_at = CASE WHEN $2 AND started_at IS NULL THEN now() ELSE started_at END, finished_at = CASE WHEN $3 THEN now() ELSE finished_at END, updated_at = now() WHERE id = $4")
                .bind(status).bind(in_progress).bind(terminal).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_jobs SET status = ?, started_at = CASE WHEN ? AND started_at IS NULL THEN CURRENT_TIMESTAMP ELSE started_at END, finished_at = CASE WHEN ? THEN CURRENT_TIMESTAMP ELSE finished_at END, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(status).bind(in_progress).bind(terminal).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
/// Recompute a run's status from its jobs.
///
/// - any job `in_progress` → run `in_progress`
/// - all jobs terminal → `failure` if any job failed, else `cancelled` if
///   any job cancelled, else `success`
/// - otherwise → keep current status (jobs still queued/claimed)
///
/// `finished_at` is set exactly when the run first reaches a terminal status.
pub async fn recompute_run_status(pool: &DbPool, run_id: &str) -> Result<(), String> {
    const ROLLUP: &str = "UPDATE action_runs SET \
        status = CASE \
          WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'in_progress') THEN 'in_progress' \
          WHEN NOT EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status NOT IN ('success','failure','cancelled','skipped')) THEN \
            CASE \
              WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'failure') THEN 'failure' \
              WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'cancelled') THEN 'cancelled' \
              ELSE 'success' \
            END \
          ELSE status END, \
        finished_at = CASE \
          WHEN finished_at IS NULL AND NOT EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status NOT IN ('success','failure','cancelled','skipped')) THEN CURRENT_TIMESTAMP \
          ELSE finished_at END, \
        updated_at = CURRENT_TIMESTAMP \
        WHERE id = ?";
    const ROLLUP_PG: &str = "UPDATE action_runs SET \
        status = CASE \
          WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'in_progress') THEN 'in_progress' \
          WHEN NOT EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status NOT IN ('success','failure','cancelled','skipped')) THEN \
            CASE \
              WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'failure') THEN 'failure' \
              WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'cancelled') THEN 'cancelled' \
              ELSE 'success' \
            END \
          ELSE status END, \
        finished_at = CASE \
          WHEN finished_at IS NULL AND NOT EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status NOT IN ('success','failure','cancelled','skipped')) THEN now() \
          ELSE finished_at END, \
        updated_at = now() \
        WHERE id = $1";
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(ROLLUP)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(ROLLUP_PG)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(ROLLUP)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Runner liveness heartbeat — bumps `last_online`/`updated_at`. Best-effort;
/// callers should log, not fail, on error.
pub async fn touch_runner_online(pool: &DbPool, runner_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_runners SET last_online = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(runner_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE action_runners SET last_online = now(), updated_at = now() WHERE id = $1",
            )
            .bind(runner_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_runners SET last_online = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(runner_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn update_runner_labels(
    pool: &DbPool,
    runner_id: &str,
    labels_json: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_runners SET labels_json = ? WHERE id = ?")
                .bind(labels_json)
                .bind(runner_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_runners SET labels_json = $1 WHERE id = $2")
                .bind(labels_json)
                .bind(runner_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_runners SET labels_json = ? WHERE id = ?")
                .bind(labels_json)
                .bind(runner_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Total run count for a repository (pagination companion to [`list_runs_for_repo`]).
pub async fn count_runs_for_repo(
    pool: &DbPool,
    repository_id: &str,
    filter: &ActionRunFilter,
) -> Result<i64, String> {
    let branch_ref = branch_ref_of(filter);
    match pool {
        DbPool::Sqlite(p) => {
            let n: i64 = bind_run_filter!(
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                    "SELECT COUNT(*) FROM action_runs WHERE repository_id = ?{RUNS_FILTER_SQLITE}"
                )))
                .bind(repository_id),
                filter,
                &branch_ref,
            )
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(n)
        }
        DbPool::Postgres(p) => {
            let n: i64 = bind_run_filter_pg!(
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                    "SELECT COUNT(*) FROM action_runs WHERE repository_id = $1{RUNS_FILTER_PG}"
                )))
                .bind(repository_id),
                filter,
                &branch_ref,
            )
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(n)
        }
        DbPool::MySql(p) => {
            let n: i64 = bind_run_filter!(
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                    "SELECT COUNT(*) FROM action_runs WHERE repository_id = ?{RUNS_FILTER_MYSQL}"
                )))
                .bind(repository_id),
                filter,
                &branch_ref,
            )
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(n)
        }
    }
}

/// Requeue a run for re-run: reset the run to queued and requeue its jobs so a
/// runner can claim them again. `job_id` requeues a single job (GitHub
/// "Re-run this job"); `failed_only` limits the reset to failed/cancelled
/// jobs (GitHub "Re-run failed jobs"); otherwise every job resets.
pub async fn requeue_run(
    pool: &DbPool,
    run_id: &str,
    failed_only: bool,
    job_id: Option<&str>,
) -> Result<(), String> {
    const JOB_RESET_SQLITE: &str = "UPDATE action_jobs SET status = 'queued', runner_id = NULL, started_at = NULL, finished_at = NULL, timeout_at = NULL, attempt = attempt + 1, updated_at = CURRENT_TIMESTAMP WHERE run_id = ?";
    const JOB_RESET_PG: &str = "UPDATE action_jobs SET status = 'queued', runner_id = NULL, started_at = NULL, finished_at = NULL, timeout_at = NULL, attempt = attempt + 1, updated_at = now() WHERE run_id = $1";
    const JOB_RESET_MYSQL: &str = "UPDATE action_jobs SET status = 'queued', runner_id = NULL, started_at = NULL, finished_at = NULL, timeout_at = NULL, attempt = attempt + 1, updated_at = CURRENT_TIMESTAMP WHERE run_id = ?";
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE action_runs SET status = 'queued', finished_at = NULL, completion_notified = 0, attempt = attempt + 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            if let Some(j) = job_id {
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "{JOB_RESET_SQLITE} AND id = ?"
                )))
                .bind(run_id)
                .bind(j)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            } else if failed_only {
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "{JOB_RESET_SQLITE} AND status IN ('failure','cancelled')"
                )))
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            } else {
                sqlx::query(JOB_RESET_SQLITE)
                    .bind(run_id)
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE action_runs SET status = 'queued', finished_at = NULL, completion_notified = false, attempt = attempt + 1, updated_at = now() WHERE id = $1",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            if let Some(j) = job_id {
                sqlx::query(sqlx::AssertSqlSafe(format!("{JOB_RESET_PG} AND id = $2")))
                    .bind(run_id)
                    .bind(j)
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
            } else if failed_only {
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "{JOB_RESET_PG} AND status IN ('failure','cancelled')"
                )))
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            } else {
                sqlx::query(JOB_RESET_PG)
                    .bind(run_id)
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE action_runs SET status = 'queued', finished_at = NULL, completion_notified = 0, attempt = attempt + 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            if let Some(j) = job_id {
                sqlx::query(sqlx::AssertSqlSafe(format!("{JOB_RESET_MYSQL} AND id = ?")))
                    .bind(run_id)
                    .bind(j)
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
            } else if failed_only {
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "{JOB_RESET_MYSQL} AND status IN ('failure','cancelled')"
                )))
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            } else {
                sqlx::query(JOB_RESET_MYSQL)
                    .bind(run_id)
                    .execute(p)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// Atomically claim the right to emit the "run completed" in-app notification
/// for a finished run (DEBT-06). `finished_at` is set exactly once per run
/// lifetime (requeue clears it), so a successful claim means this caller is the
/// single emitter for that completion.
pub async fn claim_run_completion_notice(pool: &DbPool, run_id: &str) -> Result<bool, String> {
    let n = match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE action_runs SET completion_notified = 1
             WHERE id = ? AND finished_at IS NOT NULL AND completion_notified = 0",
        )
        .bind(run_id)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE action_runs SET completion_notified = true
             WHERE id = $1 AND finished_at IS NOT NULL AND completion_notified = false",
        )
        .bind(run_id)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE action_runs SET completion_notified = 1
             WHERE id = ? AND finished_at IS NOT NULL AND completion_notified = 0",
        )
        .bind(run_id)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
    };
    Ok(n > 0)
}

/// Cancel a run and any unfinished jobs (queued/in_progress). Finished jobs
/// keep their conclusion, matching per-job cancellation semantics.
/// Returns `true` when the run was actually cancelled — `false` when it had
/// already finished (`finished_at` guard keeps cancel from clobbering a
/// terminal status, DEBT-06 review).
pub async fn cancel_run(pool: &DbPool, run_id: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let res = sqlx::query(
                "UPDATE action_runs SET status = 'cancelled', finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND finished_at IS NULL",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            if res.rows_affected() == 0 {
                return Ok(false);
            }
            sqlx::query(
                "UPDATE action_jobs SET status = 'cancelled', finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE run_id = ? AND status IN ('queued', 'waiting', 'in_progress')",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            let res = sqlx::query(
                "UPDATE action_runs SET status = 'cancelled', finished_at = now(), updated_at = now() WHERE id = $1 AND finished_at IS NULL",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            if res.rows_affected() == 0 {
                return Ok(false);
            }
            sqlx::query(
                "UPDATE action_jobs SET status = 'cancelled', finished_at = now(), updated_at = now() WHERE run_id = $1 AND status IN ('queued', 'waiting', 'in_progress')",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            let res = sqlx::query(
                "UPDATE action_runs SET status = 'cancelled', finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND finished_at IS NULL",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            if res.rows_affected() == 0 {
                return Ok(false);
            }
            sqlx::query(
                "UPDATE action_jobs SET status = 'cancelled', finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE run_id = ? AND status IN ('queued', 'waiting', 'in_progress')",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(true)
}

// ---------------------------------------------------------------------------
// Per-job bearer tokens (OXIDEAN_TOKEN) — minted at claim, revoked on terminal
// status / run cancel; `job_id` UNIQUE so a rerun re-mints onto the same row.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ActionJobTokenRow {
    pub id: String,
    pub job_id: String,
    pub run_id: String,
    pub repository_id: String,
    pub user_id: String,
    /// `read` | `write` (validated by CHECK / Rust callers).
    pub permission: String,
    pub token_hash: String,
    pub created_at: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
}

macro_rules! map_job_token {
    ($row:expr) => {{
        let row = $row;
        ActionJobTokenRow {
            id: row.get("id"),
            job_id: row.get("job_id"),
            run_id: row.get("run_id"),
            repository_id: row.get("repository_id"),
            user_id: row.get("user_id"),
            permission: row.get("permission"),
            token_hash: row.get("token_hash"),
            created_at: row.try_get("created_at").unwrap_or_default(),
            expires_at: row.try_get("expires_at").unwrap_or_default(),
            revoked_at: row.try_get("revoked_at").unwrap_or(None),
        }
    }};
}

/// Insert the job token row — `ON CONFLICT (job_id)` re-mints in place so a
/// requeued job's next claim issues a fresh token and the old hash stops
/// resolving (the row's plaintext is never stored).
#[allow(clippy::too_many_arguments)]
pub async fn insert_action_job_token(
    pool: &DbPool,
    id: &str,
    job_id: &str,
    run_id: &str,
    repository_id: &str,
    user_id: &str,
    permission: &str,
    token_hash: &str,
    expires_at: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO action_job_tokens (id, job_id, run_id, repository_id, user_id, permission, token_hash, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (job_id) DO UPDATE SET token_hash = excluded.token_hash,
                     permission = excluded.permission, expires_at = excluded.expires_at,
                     revoked_at = NULL",
            )
            .bind(id).bind(job_id).bind(run_id).bind(repository_id).bind(user_id)
            .bind(permission).bind(token_hash).bind(expires_at)
            .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO action_job_tokens (id, job_id, run_id, repository_id, user_id, permission, token_hash, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8::timestamptz)
                 ON CONFLICT (job_id) DO UPDATE SET token_hash = excluded.token_hash,
                     permission = excluded.permission, expires_at = excluded.expires_at,
                     revoked_at = NULL",
            )
            .bind(id).bind(job_id).bind(run_id).bind(repository_id).bind(user_id)
            .bind(permission).bind(token_hash).bind(expires_at)
            .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO action_job_tokens (id, job_id, run_id, repository_id, user_id, permission, token_hash, expires_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'))
                 ON DUPLICATE KEY UPDATE token_hash = VALUES(token_hash),
                     permission = VALUES(permission), expires_at = VALUES(expires_at),
                     revoked_at = NULL",
            )
            .bind(id).bind(job_id).bind(run_id).bind(repository_id).bind(user_id)
            .bind(permission).bind(token_hash).bind(expires_at)
            .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Token-hash lookup — returns revoked/expired rows too so the bearer edge can
/// distinguish "revoked" from "never existed" (same rule as PAT resolution).
pub async fn find_action_job_token_by_hash(
    pool: &DbPool,
    token_hash: &str,
) -> Result<Option<ActionJobTokenRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT id, job_id, run_id, repository_id, user_id, permission, token_hash,
                        strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
                        expires_at,
                        strftime('%Y-%m-%dT%H:%M:%SZ', revoked_at) AS revoked_at
                 FROM action_job_tokens WHERE token_hash = ?",
            )
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_job_token!(r)))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT id, job_id, run_id, repository_id, user_id, permission, token_hash,
                        to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
                        to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at,
                        to_char(revoked_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS revoked_at
                 FROM action_job_tokens WHERE token_hash = $1",
            )
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_job_token!(r)))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT id, job_id, run_id, repository_id, user_id, permission, token_hash,
                        DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
                        DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') AS expires_at,
                        DATE_FORMAT(revoked_at, '%Y-%m-%dT%H:%i:%sZ') AS revoked_at
                 FROM action_job_tokens WHERE token_hash = ?",
            )
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_job_token!(r)))
        }
    }
}

/// Revoke a job's token (terminal status or re-mint replacement). Idempotent —
/// already-revoked rows are untouched.
pub async fn revoke_action_job_token_for_job(pool: &DbPool, job_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_job_tokens SET revoked_at = CURRENT_TIMESTAMP WHERE job_id = ? AND revoked_at IS NULL")
                .bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_job_tokens SET revoked_at = now() WHERE job_id = $1 AND revoked_at IS NULL")
                .bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_job_tokens SET revoked_at = CURRENT_TIMESTAMP WHERE job_id = ? AND revoked_at IS NULL")
                .bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Revoke every token minted for a run's jobs (run cancel).
pub async fn revoke_action_job_tokens_for_run(pool: &DbPool, run_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_job_tokens SET revoked_at = CURRENT_TIMESTAMP WHERE run_id = ? AND revoked_at IS NULL")
                .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_job_tokens SET revoked_at = now() WHERE run_id = $1 AND revoked_at IS NULL")
                .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_job_tokens SET revoked_at = CURRENT_TIMESTAMP WHERE run_id = ? AND revoked_at IS NULL")
                .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
