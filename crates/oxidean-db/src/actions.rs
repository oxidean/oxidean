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
    /// Fork-approval hold (Phase 6): `waiting_approval` is a derived public
    /// status — held runs keep their natural `status` and surface the held
    /// state via this flag (the jobs/runs CHECK vocabulary predates it).
    pub approval_hold: bool,
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
    /// `$GITHUB_STEP_SUMMARY` markdown reported at terminal (Phase 4).
    pub summary_md: Option<String>,
    /// `concurrency:` group at job level (Phase 3).
    pub concurrency_group: Option<String>,
    /// Claim counter — bumped by `requeue_run`.
    pub attempt: i64,
    /// Wall-clock deadline from `timeout-minutes` (Phase 3 enforcer).
    pub timeout_at: Option<String>,
    /// Fork-approval hold (Phase 6): held jobs sit `waiting` + this flag —
    /// unclaimable and invisible to the scheduler until `approve_run` clears
    /// it. See [`ActionRunRow::approval_hold`].
    pub approval_hold: bool,
    /// Resolved `environment:` name at enqueue (Phase 7) — NULL when the job
    /// does not target a deployment environment.
    pub environment_name: Option<String>,
    /// Environment wait-timer minutes captured at enqueue; promotion of a
    /// needs-gated leg stamps `deployment_not_before = now + wait` instead of
    /// queueing directly.
    pub deployment_wait_minutes: i64,
    /// RFC3339 instant after which the leg may be queued (environment wait
    /// timer). The scheduler skips `waiting` legs whose timer hasn't lapsed.
    pub deployment_not_before: Option<String>,
    /// Reusable-workflow link (P13): callee legs carry the `uses:` job's id;
    /// NULL on top-level jobs. Scopes needs/fail-fast matching per call.
    pub caller_job_id: String,
    /// Resolved `jobs.<id>.continue-on-error` — a `failure` on this leg does
    /// not flip the run to failure and cannot fire `fail-fast`.
    pub continue_on_error: bool,
    pub runner_id: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    /// Last runner contact (Phase 8) — bumped by `update_log`/`update_task`;
    /// the sweeper requeues `in_progress` jobs whose heartbeat went silent.
    pub heartbeat_at: Option<String>,
    /// Next expected log batch index (Phase 8) — replays below it drop
    /// idempotently, gaps above it are rejected so the runner retries.
    pub log_index: i64,
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
    /// Insert held (`waiting` + flag) when the fork-approval policy applies —
    /// the job is unclaimable until `approve_run` clears the flag (Phase 6).
    pub approval_hold: bool,
    /// Resolved `environment:` name (Phase 7) — drives `action_deployments`
    /// records and protection gating.
    pub environment_name: Option<String>,
    /// Environment wait-timer minutes (0 = no timer).
    pub deployment_wait_minutes: i64,
    /// Precomputed earliest-queue instant for root legs (needs-legs get their
    /// timer stamped at needs-resolution time).
    pub deployment_not_before: Option<String>,
    /// Reusable-workflow linkage — callee legs carry the caller `uses:` job's
    /// id so the scheduler scopes needs/fail-fast within the call.
    pub caller_job_id: String,
    /// Resolved `jobs.<id>.continue-on-error` — a `failure` on this leg does
    /// not flip the run to failure and cannot fire `fail-fast`.
    pub continue_on_error: bool,
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
            approval_hold: false,
            environment_name: None,
            deployment_wait_minutes: 0,
            deployment_not_before: None,
            caller_job_id: String::new(),
            continue_on_error: false,
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
const JOB_COLS_SQLITE: &str = "id, run_id, job_key, matrix_key, name, runs_on_json, status, payload_sha, needs_json, matrix_json, outputs_json, summary_md, concurrency_group, attempt, timeout_at, approval_hold, environment_name, deployment_wait_minutes, deployment_not_before, caller_job_id, continue_on_error, runner_id, started_at, finished_at, heartbeat_at, log_index";
const JOB_COLS_PG: &str = "id, run_id, job_key, matrix_key, name, runs_on_json, status, payload_sha, needs_json, matrix_json, outputs_json, summary_md, concurrency_group, attempt, to_char(timeout_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS timeout_at, approval_hold, environment_name, deployment_wait_minutes, deployment_not_before::text AS deployment_not_before, caller_job_id, continue_on_error, runner_id, started_at::text AS started_at, finished_at::text AS finished_at, heartbeat_at, log_index";
const JOB_COLS_MYSQL: &str = "id, run_id, job_key, matrix_key, name, runs_on_json, status, payload_sha, needs_json, matrix_json, outputs_json, summary_md, concurrency_group, attempt, DATE_FORMAT(timeout_at, '%Y-%m-%dT%H:%i:%sZ') AS timeout_at, approval_hold, environment_name, deployment_wait_minutes, deployment_not_before, caller_job_id, continue_on_error, runner_id, CAST(started_at AS CHAR) AS started_at, CAST(finished_at AS CHAR) AS finished_at, heartbeat_at, log_index";

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
            summary_md: r.try_get("summary_md").unwrap_or(None),
            concurrency_group: r.try_get("concurrency_group").unwrap_or(None),
            attempt: r
                .try_get::<i64, _>("attempt")
                .or_else(|_| r.try_get::<i32, _>("attempt").map(i64::from))
                .unwrap_or(1),
            timeout_at: r.try_get("timeout_at").unwrap_or(None),
            approval_hold: r.try_get("approval_hold").unwrap_or(false),
            environment_name: r.try_get("environment_name").unwrap_or(None),
            deployment_wait_minutes: r
                .try_get::<i64, _>("deployment_wait_minutes")
                .or_else(|_| {
                    r.try_get::<i32, _>("deployment_wait_minutes")
                        .map(i64::from)
                })
                .unwrap_or(0),
            deployment_not_before: r.try_get("deployment_not_before").unwrap_or(None),
            caller_job_id: r.try_get("caller_job_id").unwrap_or_default(),
            continue_on_error: r.try_get("continue_on_error").unwrap_or(false),
            runner_id: r.get("runner_id"),
            started_at: r.try_get("started_at").unwrap_or(None),
            finished_at: r.try_get("finished_at").unwrap_or(None),
            heartbeat_at: r.try_get("heartbeat_at").unwrap_or(None),
            log_index: r
                .try_get::<i64, _>("log_index")
                .or_else(|_| r.try_get::<i32, _>("log_index").map(i64::from))
                .unwrap_or(0),
            created_at: r.try_get("created_at").unwrap_or_default(),
            updated_at: r.try_get("updated_at").unwrap_or_default(),
        }
    }};
}

/// Run SELECT-list body (before `RUN_NUMBER_SQL` and the dialect timestamp
/// casts) — shared across all three dialects since every added column is a
/// plain TEXT/INTEGER/BOOLEAN read.
const RUN_COLS: &str = "id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, status, title, triggered_by, head_repo_id, attempt, event_payload_json, concurrency_group, concurrency_cancel_pending, approval_hold";

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
            approval_hold: r
                .try_get::<bool, _>("approval_hold")
                .ok()
                .or_else(|| r.try_get::<i64, _>("approval_hold").ok().map(|v| v != 0))
                .or_else(|| r.try_get::<i8, _>("approval_hold").ok().map(|v| v != 0))
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

#[derive(Debug, Clone, sqlx::FromRow)]
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
    event_payload_json: Option<&str>,
) -> Result<ActionRunRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(r#"INSERT INTO action_runs
                (id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, title, triggered_by, head_repo_id, event_payload_json)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#)
                .bind(id).bind(repository_id).bind(workflow_path).bind(workflow_name)
                .bind(event).bind(head_sha).bind(head_ref).bind(title).bind(triggered_by)
                .bind(head_repo_id).bind(event_payload_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(r#"INSERT INTO action_runs
                (id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, title, triggered_by, head_repo_id, event_payload_json)
                VALUES (?,?,?,?,?,?,?,?,?,?,?)"#)
                .bind(id).bind(repository_id).bind(workflow_path).bind(workflow_name)
                .bind(event).bind(head_sha).bind(head_ref).bind(title).bind(triggered_by)
                .bind(head_repo_id).bind(event_payload_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(r#"INSERT INTO action_runs
                (id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, title, triggered_by, head_repo_id, event_payload_json)
                VALUES (?,?,?,?,?,?,?,?,?,?,?)"#)
                .bind(id).bind(repository_id).bind(workflow_path).bind(workflow_name)
                .bind(event).bind(head_sha).bind(head_ref).bind(title).bind(triggered_by)
                .bind(head_repo_id).bind(event_payload_json)
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
        event_payload_json: event_payload_json.map(str::to_string),
        concurrency_group: None,
        concurrency_cancel_pending: false,
        approval_hold: false,
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
                (id, run_id, job_key, matrix_key, name, runs_on_json, payload_sha, needs_json, matrix_json, outputs_json, concurrency_group, status, timeout_at, approval_hold, environment_name, deployment_wait_minutes, deployment_not_before, caller_job_id, continue_on_error)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13::timestamptz,$14,$15,$16,$17,$18,$19)"#)
                .bind(&job.id).bind(&job.run_id).bind(&job.job_key).bind(&job.matrix_key)
                .bind(&job.name).bind(&job.runs_on_json).bind(&job.payload_sha)
                .bind(&job.needs_json).bind(&job.matrix_json).bind(&job.outputs_json)
                .bind(&job.concurrency_group).bind(&job.status).bind(&job.timeout_at)
                .bind(job.approval_hold)
                .bind(&job.environment_name).bind(job.deployment_wait_minutes)
                .bind(&job.deployment_not_before)
                .bind(&job.caller_job_id).bind(job.continue_on_error)
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
                (id, run_id, job_key, matrix_key, name, runs_on_json, payload_sha, needs_json, matrix_json, outputs_json, concurrency_group, status, timeout_at, approval_hold, environment_name, deployment_wait_minutes, deployment_not_before, caller_job_id, continue_on_error)
                VALUES (?,?,?,?,?,?,?,?,?,?,?,?,STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'),?,?,?,?,?,?)"#)
                .bind(&job.id).bind(&job.run_id).bind(&job.job_key).bind(&job.matrix_key)
                .bind(&job.name).bind(&job.runs_on_json).bind(&job.payload_sha)
                .bind(&job.needs_json).bind(&job.matrix_json).bind(&job.outputs_json)
                .bind(&job.concurrency_group).bind(&job.status).bind(&job.timeout_at)
                .bind(job.approval_hold)
                .bind(&job.environment_name).bind(job.deployment_wait_minutes)
                .bind(&job.deployment_not_before)
                .bind(&job.caller_job_id).bind(job.continue_on_error)
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
                (id, run_id, job_key, matrix_key, name, runs_on_json, payload_sha, needs_json, matrix_json, outputs_json, concurrency_group, status, timeout_at, approval_hold, environment_name, deployment_wait_minutes, deployment_not_before, caller_job_id, continue_on_error)
                VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#)
                .bind(&job.id).bind(&job.run_id).bind(&job.job_key).bind(&job.matrix_key)
                .bind(&job.name).bind(&job.runs_on_json).bind(&job.payload_sha)
                .bind(&job.needs_json).bind(&job.matrix_json).bind(&job.outputs_json)
                .bind(&job.concurrency_group).bind(&job.status).bind(&job.timeout_at)
                .bind(job.approval_hold)
                .bind(&job.environment_name).bind(job.deployment_wait_minutes)
                .bind(&job.deployment_not_before)
                .bind(&job.caller_job_id).bind(job.continue_on_error)
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
    let row = ActionJobRow {
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
        summary_md: None,
        concurrency_group: job.concurrency_group.clone(),
        attempt: 1,
        timeout_at: job.timeout_at.clone(),
        approval_hold: job.approval_hold,
        environment_name: job.environment_name.clone(),
        deployment_wait_minutes: job.deployment_wait_minutes,
        deployment_not_before: job.deployment_not_before.clone(),
        caller_job_id: job.caller_job_id.clone(),
        continue_on_error: job.continue_on_error,
        runner_id: None,
        started_at: None,
        finished_at: None,
        heartbeat_at: None,
        log_index: 0,
        created_at: String::new(),
        updated_at: String::new(),
    };
    // A new job row is new claimable work — bump the tasks watermark so
    // `runner.v1` FetchTask callers retry promptly.
    bump_action_tasks_version(pool).await?;
    Ok(row)
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

/// `repositories.actions_pull_request_target` — opt-in for trusted-context
/// `pull_request_target` dispatch (default off).
pub async fn get_actions_pull_request_target(pool: &DbPool, repo_id: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row =
                sqlx::query("SELECT actions_pull_request_target FROM repositories WHERE id = ?")
                    .bind(repo_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| {
                    r.try_get::<i64, _>("actions_pull_request_target")
                        .unwrap_or(0)
                        != 0
                })
                .unwrap_or(false))
        }
        DbPool::Postgres(p) => {
            let row =
                sqlx::query("SELECT actions_pull_request_target FROM repositories WHERE id = $1")
                    .bind(repo_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| {
                    r.try_get::<bool, _>("actions_pull_request_target")
                        .unwrap_or(false)
                })
                .unwrap_or(false))
        }
        DbPool::MySql(p) => {
            let row =
                sqlx::query("SELECT actions_pull_request_target FROM repositories WHERE id = ?")
                    .bind(repo_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| {
                    r.try_get::<i8, _>("actions_pull_request_target")
                        .unwrap_or(0)
                        != 0
                })
                .unwrap_or(false))
        }
    }
}

pub async fn set_actions_pull_request_target(
    pool: &DbPool,
    repo_id: &str,
    enabled: bool,
) -> Result<(), String> {
    let v = if enabled { 1i64 } else { 0 };
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE repositories SET actions_pull_request_target = ? WHERE id = ?")
                .bind(v)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE repositories SET actions_pull_request_target = $1 WHERE id = $2")
                .bind(enabled)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE repositories SET actions_pull_request_target = ? WHERE id = ?")
                .bind(v as i8)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// `repositories.actions_fork_pr_approval` — 'never' | 'first-time' | 'always'.
/// Unknown/missing values read back as 'never' (GitHub default = run freely).
pub async fn get_actions_fork_pr_approval(pool: &DbPool, repo_id: &str) -> Result<String, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT actions_fork_pr_approval FROM repositories WHERE id = ?")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(normalize_approval_policy(row.and_then(|r| {
                r.try_get::<String, _>("actions_fork_pr_approval").ok()
            })))
        }
        DbPool::Postgres(p) => {
            let row =
                sqlx::query("SELECT actions_fork_pr_approval FROM repositories WHERE id = $1")
                    .bind(repo_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(normalize_approval_policy(row.and_then(|r| {
                r.try_get::<String, _>("actions_fork_pr_approval").ok()
            })))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT actions_fork_pr_approval FROM repositories WHERE id = ?")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(normalize_approval_policy(row.and_then(|r| {
                r.try_get::<String, _>("actions_fork_pr_approval").ok()
            })))
        }
    }
}

fn normalize_approval_policy(raw: Option<String>) -> String {
    match raw.as_deref().map(str::trim) {
        Some("first-time") => "first-time",
        Some("always") => "always",
        _ => "never",
    }
    .to_string()
}

pub async fn set_actions_fork_pr_approval(
    pool: &DbPool,
    repo_id: &str,
    policy: &str,
) -> Result<(), String> {
    let policy = match policy.trim() {
        "first-time" => "first-time",
        "always" => "always",
        _ => "never",
    };
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE repositories SET actions_fork_pr_approval = ? WHERE id = ?")
                .bind(policy)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE repositories SET actions_fork_pr_approval = $1 WHERE id = $2")
                .bind(policy)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE repositories SET actions_fork_pr_approval = ? WHERE id = ?")
                .bind(policy)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Set the approval-hold flag on a run and its not-yet-terminal jobs —
/// fork runs held for maintainer approval stay unclaimable (claim never sees
/// `waiting` rows) until [`approve_run`] clears it. The API coalesces the
/// flag into the public `waiting_approval` status.
pub async fn hold_run_for_approval(pool: &DbPool, run_id: &str) -> Result<(), String> {
    let (runs_sql, jobs_sql) = match pool {
        DbPool::Sqlite(_) | DbPool::MySql(_) => (
            "UPDATE action_runs SET approval_hold = 1 WHERE id = ?",
            "UPDATE action_jobs SET approval_hold = 1 \
                 WHERE run_id = ? AND status NOT IN ('success','failure','cancelled','skipped')",
        ),
        DbPool::Postgres(_) => (
            "UPDATE action_runs SET approval_hold = true WHERE id = $1",
            "UPDATE action_jobs SET approval_hold = true \
                 WHERE run_id = $1 AND status NOT IN ('success','failure','cancelled','skipped')",
        ),
    };
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(runs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(jobs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(runs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(jobs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(runs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(jobs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Release a held run — clears the flag on the run and its held jobs. Jobs
/// stay `waiting`; the scheduler's recompute pass then restores the right
/// state (`queued` for needs-free legs, `waiting` for still-gated ones).
pub async fn approve_run(pool: &DbPool, run_id: &str) -> Result<(), String> {
    let (runs_sql, jobs_sql) = match pool {
        DbPool::Sqlite(_) | DbPool::MySql(_) => (
            "UPDATE action_runs SET approval_hold = 0 \
                 WHERE id = ? AND approval_hold != 0",
            "UPDATE action_jobs SET approval_hold = 0 \
                 WHERE run_id = ? AND approval_hold != 0",
        ),
        DbPool::Postgres(_) => (
            "UPDATE action_runs SET approval_hold = false \
                 WHERE id = $1 AND approval_hold",
            "UPDATE action_jobs SET approval_hold = false \
                 WHERE run_id = $1 AND approval_hold",
        ),
    };
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(jobs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(runs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(jobs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(runs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(jobs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(runs_sql)
                .bind(run_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// `first-time` approval probe: true when the actor already has a run in this
/// repo that is not itself held for approval (a prior approved/auto-run).
pub async fn actor_has_prior_approved_run(
    pool: &DbPool,
    repo_id: &str,
    actor_id: &str,
) -> Result<bool, String> {
    let sql = match pool {
        DbPool::Sqlite(_) | DbPool::MySql(_) => {
            "SELECT 1 FROM action_runs \
                 WHERE repository_id = ? AND triggered_by = ? \
                   AND approval_hold = 0 LIMIT 1"
        }
        DbPool::Postgres(_) => {
            "SELECT 1 FROM action_runs \
                 WHERE repository_id = $1 AND triggered_by = $2 \
                   AND approval_hold = false LIMIT 1"
        }
    };
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sql)
                .bind(repo_id)
                .bind(actor_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(row.is_some())
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(sql)
                .bind(repo_id)
                .bind(actor_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(row.is_some())
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sql)
                .bind(repo_id)
                .bind(actor_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(row.is_some())
        }
    }
}

/// Record that (repo, workflow, cron) fired at `minute` (unix/60). Returns
/// true when the row was newly claimed this minute — false means another
/// sweeper already fired it.
pub async fn claim_schedule_tick(
    pool: &DbPool,
    repo_id: &str,
    workflow_path: &str,
    cron: &str,
    minute: i64,
) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let res = sqlx::query(
                "INSERT OR IGNORE INTO action_schedule_state \
                 (repository_id, workflow_path, cron, last_fired_minute) VALUES (?,?,?,?)",
            )
            .bind(repo_id)
            .bind(workflow_path)
            .bind(cron)
            .bind(minute)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            if res.rows_affected() == 1 {
                return Ok(true);
            }
            sqlx::query(
                "UPDATE action_schedule_state SET last_fired_minute = ? \
                 WHERE repository_id = ? AND workflow_path = ? AND cron = ? \
                   AND last_fired_minute < ?",
            )
            .bind(minute)
            .bind(repo_id)
            .bind(workflow_path)
            .bind(cron)
            .bind(minute)
            .execute(p)
            .await
            .map_err(|e| e.to_string())
            .map(|r| r.rows_affected() == 1)
        }
        DbPool::Postgres(p) => {
            let res = sqlx::query(
                "INSERT INTO action_schedule_state \
                 (repository_id, workflow_path, cron, last_fired_minute) \
                 VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING",
            )
            .bind(repo_id)
            .bind(workflow_path)
            .bind(cron)
            .bind(minute)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            if res.rows_affected() == 1 {
                return Ok(true);
            }
            sqlx::query(
                "UPDATE action_schedule_state SET last_fired_minute = $1 \
                 WHERE repository_id = $2 AND workflow_path = $3 AND cron = $4 \
                   AND last_fired_minute < $5",
            )
            .bind(minute)
            .bind(repo_id)
            .bind(workflow_path)
            .bind(cron)
            .bind(minute)
            .execute(p)
            .await
            .map_err(|e| e.to_string())
            .map(|r| r.rows_affected() == 1)
        }
        DbPool::MySql(p) => {
            let res = sqlx::query(
                "INSERT IGNORE INTO action_schedule_state \
                 (repository_id, workflow_path, cron, last_fired_minute) VALUES (?,?,?,?)",
            )
            .bind(repo_id)
            .bind(workflow_path)
            .bind(cron)
            .bind(minute)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            if res.rows_affected() == 1 {
                return Ok(true);
            }
            sqlx::query(
                "UPDATE action_schedule_state SET last_fired_minute = ? \
                 WHERE repository_id = ? AND workflow_path = ? AND cron = ? \
                   AND last_fired_minute < ?",
            )
            .bind(minute)
            .bind(repo_id)
            .bind(workflow_path)
            .bind(cron)
            .bind(minute)
            .execute(p)
            .await
            .map_err(|e| e.to_string())
            .map(|r| r.rows_affected() == 1)
        }
    }
}

/// Every repository with Actions enabled — the cron sweeper iterates these.
pub async fn list_actions_enabled_repo_ids(pool: &DbPool) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT id FROM repositories WHERE actions_enabled = 1 AND deleted_at IS NULL",
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .filter_map(|r| r.try_get::<String, _>("id").ok())
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT id FROM repositories WHERE actions_enabled = true AND deleted_at IS NULL",
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .filter_map(|r| r.try_get::<String, _>("id").ok())
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT id FROM repositories WHERE actions_enabled = 1 AND deleted_at IS NULL",
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .filter_map(|r| r.try_get::<String, _>("id").ok())
                .collect())
        }
    }
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
    let heartbeat = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = $1, \
                 started_at = COALESCE(started_at, now()), heartbeat_at = $3, updated_at = now() \
                 WHERE id = (SELECT j.id FROM action_jobs j \
                   WHERE j.status = 'queued' AND j.approval_hold = false AND NOT EXISTS ( \
                     SELECT 1 FROM action_job_labels l WHERE l.job_id = j.id \
                     AND NOT (l.base = ANY($2::text[]))) \
                   ORDER BY j.created_at, j.matrix_key LIMIT 1 FOR UPDATE SKIP LOCKED) \
                 AND status = 'queued' AND approval_hold = false RETURNING {JOB_COLS_PG}"
            )))
            .bind(runner_id)
            .bind(&bases)
            .bind(&heartbeat)
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
                     WHERE j.status = 'queued' AND j.approval_hold = 0 AND NOT EXISTS ( \
                       SELECT 1 FROM action_job_labels l WHERE l.job_id = j.id \
                       AND l.base NOT IN (SELECT value FROM JSON_TABLE(?, '$[*]' \
                         COLUMNS (value VARCHAR(255) PATH '$')) AS lb)) \
                     ORDER BY j.created_at, j.matrix_key LIMIT 5",
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
                 started_at = COALESCE(started_at, CURRENT_TIMESTAMP), heartbeat_at = ?3, updated_at = CURRENT_TIMESTAMP \
                 WHERE id = (SELECT j.id FROM action_jobs j \
                   WHERE j.status = 'queued' AND j.approval_hold = 0 AND NOT EXISTS ( \
                     SELECT 1 FROM action_job_labels l WHERE l.job_id = j.id \
                     AND l.base NOT IN (SELECT value FROM json_each(?2))) \
                   ORDER BY j.created_at, j.matrix_key LIMIT 1) \
                 AND status = 'queued' AND approval_hold = 0 RETURNING {JOB_COLS_SQLITE}"
            )))
            .bind(runner_id)
            .bind(bases_json)
            .bind(&heartbeat)
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
    let heartbeat = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    match pool {
        DbPool::Sqlite(p) => {
            let res = sqlx::query(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = ?, started_at = COALESCE(started_at, CURRENT_TIMESTAMP), heartbeat_at = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status = 'queued' AND approval_hold = 0",
            )
            .bind(runner_id)
            .bind(&heartbeat)
            .bind(job_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::Postgres(p) => {
            let res = sqlx::query(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = $1, started_at = COALESCE(started_at, now()), heartbeat_at = $3, updated_at = now() WHERE id = $2 AND status = 'queued' AND approval_hold = false",
            )
            .bind(runner_id)
            .bind(job_id)
            .bind(&heartbeat)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::MySql(p) => {
            let res = sqlx::query(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = ?, started_at = COALESCE(started_at, CURRENT_TIMESTAMP), heartbeat_at = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status = 'queued' AND approval_hold = 0",
            )
            .bind(runner_id)
            .bind(&heartbeat)
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

// The status filter coalesces `approval_hold` + non-terminal stored status
// into the public `waiting_approval` label so `?status=waiting_approval`
// matches held runs — the stored CHECK vocabulary predates that status.
// (`const &str` composition isn't available; the CASE is inlined.)
const RUNS_FILTER_SQLITE: &str = " AND (? IS NULL OR CASE WHEN approval_hold != 0 AND status NOT IN ('success','failure','cancelled','skipped') THEN 'waiting_approval' ELSE status END = ?) AND (? IS NULL OR event = ?) AND (? IS NULL OR head_ref = ? OR head_ref = ?) AND (? IS NULL OR workflow_path = ?) AND (? IS NULL OR triggered_by = ?) AND (? IS NULL OR title LIKE ? COLLATE NOCASE)";
const RUNS_FILTER_MYSQL: &str = " AND (? IS NULL OR CASE WHEN approval_hold != 0 AND status NOT IN ('success','failure','cancelled','skipped') THEN 'waiting_approval' ELSE status END = ?) AND (? IS NULL OR event = ?) AND (? IS NULL OR head_ref = ? OR head_ref = ?) AND (? IS NULL OR workflow_path = ?) AND (? IS NULL OR triggered_by = ?) AND (? IS NULL OR title LIKE ?)";
const RUNS_FILTER_PG: &str = " AND ($2::text IS NULL OR CASE WHEN approval_hold AND status NOT IN ('success','failure','cancelled','skipped') THEN 'waiting_approval' ELSE status END = $2) AND ($3::text IS NULL OR event = $3) AND ($4::text IS NULL OR head_ref = $4 OR head_ref = $5) AND ($6::text IS NULL OR workflow_path = $6) AND ($7::text IS NULL OR triggered_by = $7) AND ($8::text IS NULL OR title ILIKE $8)";

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
/// Guarded status write — only lands while the job is nonterminal
/// ('queued','waiting','in_progress'). A job the server already cancelled
/// (concurrency supersede, fail-fast sibling) keeps its terminal state
/// even when a runner reports success afterwards. Returns whether the
/// write landed.
pub async fn transition_job_status(
    pool: &DbPool,
    job_id: &str,
    status: &str,
) -> Result<bool, String> {
    let in_progress = status == "in_progress";
    let terminal = matches!(status, "success" | "failure" | "cancelled" | "skipped");
    let guard = "status IN ('queued','waiting','in_progress')";
    match pool {
        DbPool::Sqlite(p) => {
            let res = sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE action_jobs SET status = ?, started_at = CASE WHEN ? AND started_at IS NULL THEN CURRENT_TIMESTAMP ELSE started_at END, finished_at = CASE WHEN ? AND finished_at IS NULL THEN CURRENT_TIMESTAMP ELSE finished_at END, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND {guard}"
            )))
                .bind(status).bind(in_progress).bind(terminal).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::Postgres(p) => {
            let res = sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE action_jobs SET status = $1, started_at = CASE WHEN $2 AND started_at IS NULL THEN now() ELSE started_at END, finished_at = CASE WHEN $3 AND finished_at IS NULL THEN now() ELSE finished_at END, updated_at = now() WHERE id = $4 AND {guard}"
            )))
                .bind(status).bind(in_progress).bind(terminal).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::MySql(p) => {
            let res = sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE action_jobs SET status = ?, started_at = CASE WHEN ? AND started_at IS NULL THEN CURRENT_TIMESTAMP ELSE started_at END, finished_at = CASE WHEN ? AND finished_at IS NULL THEN CURRENT_TIMESTAMP ELSE finished_at END, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND {guard}"
            )))
                .bind(status).bind(in_progress).bind(terminal).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
    }
}

/// Store the materialized `outputs` map for a completed job — the values
/// dependents read through `needs.<id>.outputs.*`.
pub async fn set_job_outputs(
    pool: &DbPool,
    job_id: &str,
    outputs_json: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_jobs SET outputs_json = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(outputs_json).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE action_jobs SET outputs_json = $1, updated_at = now() WHERE id = $2",
            )
            .bind(outputs_json)
            .bind(job_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_jobs SET outputs_json = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(outputs_json).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// `$GITHUB_STEP_SUMMARY` markdown — reported at terminal, survives a
/// server-side-cancelled job (summary is still worth showing).
pub async fn set_job_summary(pool: &DbPool, job_id: &str, summary_md: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_jobs SET summary_md = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(summary_md).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_jobs SET summary_md = $1, updated_at = now() WHERE id = $2")
                .bind(summary_md)
                .bind(job_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_jobs SET summary_md = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(summary_md).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Job-level `concurrency.cancel-in-progress` — supersede every nonterminal
/// job in the same repo+group belonging to a *different* unfinished run:
/// the jobs go `cancelled` (finished_at exactly once), their job tokens are
/// revoked, and the affected runs are marked `concurrency_cancel_pending`
/// (callers roll run status up afterwards). Returns affected run ids.
pub async fn cancel_superseded_concurrency_jobs(
    pool: &DbPool,
    repository_id: &str,
    group: &str,
    new_run_id: &str,
) -> Result<Vec<String>, String> {
    const SEL_SQLITE: &str = "SELECT DISTINCT run_id FROM action_jobs WHERE concurrency_group = ? AND status IN ('queued','waiting','in_progress') AND run_id IN (SELECT id FROM action_runs WHERE repository_id = ? AND id != ? AND finished_at IS NULL)";
    const UPD_SQLITE: &str = "UPDATE action_jobs SET status = 'cancelled', finished_at = COALESCE(finished_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP WHERE concurrency_group = ? AND status IN ('queued','waiting','in_progress') AND run_id IN (SELECT id FROM action_runs WHERE repository_id = ? AND id != ? AND finished_at IS NULL)";
    const REVOKE_SQLITE: &str = "UPDATE action_job_tokens SET revoked_at = CURRENT_TIMESTAMP WHERE revoked_at IS NULL AND job_id IN (SELECT id FROM action_jobs WHERE concurrency_group = ? AND run_id IN (SELECT id FROM action_runs WHERE repository_id = ? AND id != ?))";
    const MARK_SQLITE: &str = "UPDATE action_runs SET concurrency_cancel_pending = 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?";
    const SEL_PG: &str = "SELECT DISTINCT run_id FROM action_jobs WHERE concurrency_group = $1 AND status IN ('queued','waiting','in_progress') AND run_id IN (SELECT id FROM action_runs WHERE repository_id = $2 AND id != $3 AND finished_at IS NULL)";
    const UPD_PG: &str = "UPDATE action_jobs SET status = 'cancelled', finished_at = COALESCE(finished_at, now()), updated_at = now() WHERE concurrency_group = $1 AND status IN ('queued','waiting','in_progress') AND run_id IN (SELECT id FROM action_runs WHERE repository_id = $2 AND id != $3 AND finished_at IS NULL)";
    const REVOKE_PG: &str = "UPDATE action_job_tokens SET revoked_at = now() WHERE revoked_at IS NULL AND job_id IN (SELECT id FROM action_jobs WHERE concurrency_group = $1 AND run_id IN (SELECT id FROM action_runs WHERE repository_id = $2 AND id != $3))";
    const MARK_PG: &str = "UPDATE action_runs SET concurrency_cancel_pending = true, updated_at = now() WHERE id = $1";
    const SEL_MYSQL: &str = SEL_SQLITE;
    const UPD_MYSQL: &str = UPD_SQLITE;
    const REVOKE_MYSQL: &str = REVOKE_SQLITE;
    const MARK_MYSQL: &str = MARK_SQLITE;
    match pool {
        DbPool::Sqlite(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            let ids: Vec<String> = sqlx::query_scalar(SEL_SQLITE)
                .bind(group)
                .bind(repository_id)
                .bind(new_run_id)
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            if !ids.is_empty() {
                sqlx::query(UPD_SQLITE)
                    .bind(group)
                    .bind(repository_id)
                    .bind(new_run_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query(REVOKE_SQLITE)
                    .bind(group)
                    .bind(repository_id)
                    .bind(new_run_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                for id in &ids {
                    sqlx::query(MARK_SQLITE)
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            tx.commit().await.map_err(|e| e.to_string())?;
            Ok(ids)
        }
        DbPool::Postgres(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            let ids: Vec<String> = sqlx::query_scalar(SEL_PG)
                .bind(group)
                .bind(repository_id)
                .bind(new_run_id)
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            if !ids.is_empty() {
                sqlx::query(UPD_PG)
                    .bind(group)
                    .bind(repository_id)
                    .bind(new_run_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query(REVOKE_PG)
                    .bind(group)
                    .bind(repository_id)
                    .bind(new_run_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                for id in &ids {
                    sqlx::query(MARK_PG)
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            tx.commit().await.map_err(|e| e.to_string())?;
            Ok(ids)
        }
        DbPool::MySql(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            let ids: Vec<String> = sqlx::query_scalar(SEL_MYSQL)
                .bind(group)
                .bind(repository_id)
                .bind(new_run_id)
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            if !ids.is_empty() {
                sqlx::query(UPD_MYSQL)
                    .bind(group)
                    .bind(repository_id)
                    .bind(new_run_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query(REVOKE_MYSQL)
                    .bind(group)
                    .bind(repository_id)
                    .bind(new_run_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                for id in &ids {
                    sqlx::query(MARK_MYSQL)
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            tx.commit().await.map_err(|e| e.to_string())?;
            Ok(ids)
        }
    }
}

/// Restore a job's `timeout_at` after a requeue reset — `seconds` is the
/// evaluated `timeout-minutes` stored in the payload (None clears).
pub async fn set_job_timeout_at(
    pool: &DbPool,
    job_id: &str,
    timeout_at: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_jobs SET timeout_at = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(timeout_at).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_jobs SET timeout_at = $1::timestamptz, updated_at = now() WHERE id = $2")
                .bind(timeout_at).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_jobs SET timeout_at = STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'), updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(timeout_at).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// `concurrency.cancel-in-progress` — supersede every nonterminal run in
/// the same repo+group when a new run lands: mark `concurrency_cancel_pending`,
/// cancel all its nonterminal jobs (finished_at exactly once), and let the
/// rollup carry the run to 'cancelled'. Returns superseded run ids.
pub async fn cancel_superseded_concurrency_runs(
    pool: &DbPool,
    repository_id: &str,
    group: &str,
    new_run_id: &str,
) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            let ids: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM action_runs WHERE repository_id = ? AND concurrency_group = ? AND id != ? AND finished_at IS NULL",
            )
            .bind(repository_id).bind(group).bind(new_run_id)
            .fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
            for id in &ids {
                sqlx::query(
                    "UPDATE action_runs SET concurrency_cancel_pending = 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE action_jobs SET status = 'cancelled', finished_at = COALESCE(finished_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP WHERE run_id = ? AND status IN ('queued','waiting','in_progress')",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE action_runs SET status = 'cancelled', finished_at = COALESCE(finished_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE action_job_tokens SET revoked_at = CURRENT_TIMESTAMP WHERE run_id = ? AND revoked_at IS NULL",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
            }
            tx.commit().await.map_err(|e| e.to_string())?;
            Ok(ids)
        }
        DbPool::Postgres(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            let ids: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM action_runs WHERE repository_id = $1 AND concurrency_group = $2 AND id != $3 AND finished_at IS NULL",
            )
            .bind(repository_id).bind(group).bind(new_run_id)
            .fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
            for id in &ids {
                sqlx::query(
                    "UPDATE action_runs SET concurrency_cancel_pending = true, updated_at = now() WHERE id = $1",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE action_jobs SET status = 'cancelled', finished_at = COALESCE(finished_at, now()), updated_at = now() WHERE run_id = $1 AND status IN ('queued','waiting','in_progress')",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE action_runs SET status = 'cancelled', finished_at = COALESCE(finished_at, now()), updated_at = now() WHERE id = $1",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE action_job_tokens SET revoked_at = now() WHERE run_id = $1 AND revoked_at IS NULL",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
            }
            tx.commit().await.map_err(|e| e.to_string())?;
            Ok(ids)
        }
        DbPool::MySql(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            let ids: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM action_runs WHERE repository_id = ? AND concurrency_group = ? AND id != ? AND finished_at IS NULL",
            )
            .bind(repository_id).bind(group).bind(new_run_id)
            .fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
            for id in &ids {
                sqlx::query(
                    "UPDATE action_runs SET concurrency_cancel_pending = 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE action_jobs SET status = 'cancelled', finished_at = COALESCE(finished_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP WHERE run_id = ? AND status IN ('queued','waiting','in_progress')",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE action_runs SET status = 'cancelled', finished_at = COALESCE(finished_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE action_job_tokens SET revoked_at = CURRENT_TIMESTAMP WHERE run_id = ? AND revoked_at IS NULL",
                )
                .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
            }
            tx.commit().await.map_err(|e| e.to_string())?;
            Ok(ids)
        }
    }
}

pub async fn recompute_run_status(pool: &DbPool, run_id: &str) -> Result<(), String> {
    const ROLLUP: &str = "UPDATE action_runs SET \
        status = CASE \
          WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'in_progress') THEN 'in_progress' \
          WHEN NOT EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status NOT IN ('success','failure','cancelled','skipped')) THEN \
            CASE \
              WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'failure' AND j.continue_on_error = 0) THEN 'failure' \
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
              WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'failure' AND NOT j.continue_on_error) THEN 'failure' \
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
    // Requeued needs-jobs go back to `waiting` so the scheduler re-gates
    // them; enqueue-skipped jobs (if:false, no needs) stay skipped — a
    // rerun must not resurrect a job the expression already rejected.
    // Reusable-workflow caller legs (payload carries `"call":`) re-queue as
    // `waiting` too — `queued` would make them claimable; the scheduler's
    // expansion pass re-emits their callee rows instead. Callee rows
    // (`caller_job_id` set) likewise reset to `waiting` — never claimable —
    // because expansion deletes and re-emits them wholesale.
    const JOB_RESET_SQLITE: &str = "UPDATE action_jobs SET status = CASE WHEN caller_job_id != '' THEN 'waiting' WHEN status = 'skipped' AND needs_json = '[]' THEN 'skipped' WHEN needs_json != '[]' THEN 'waiting' WHEN EXISTS (SELECT 1 FROM action_payloads p WHERE p.sha = action_jobs.payload_sha AND p.payload_json LIKE '%\"call\":{%') THEN 'waiting' ELSE 'queued' END, runner_id = NULL, started_at = NULL, finished_at = NULL, timeout_at = NULL, attempt = attempt + 1, updated_at = CURRENT_TIMESTAMP WHERE run_id = ?";
    const JOB_RESET_PG: &str = "UPDATE action_jobs SET status = CASE WHEN caller_job_id != '' THEN 'waiting' WHEN status = 'skipped' AND needs_json = '[]' THEN 'skipped' WHEN needs_json != '[]' THEN 'waiting' WHEN EXISTS (SELECT 1 FROM action_payloads p WHERE p.sha = action_jobs.payload_sha AND p.payload_json LIKE '%\"call\":{%') THEN 'waiting' ELSE 'queued' END, runner_id = NULL, started_at = NULL, finished_at = NULL, timeout_at = NULL, attempt = attempt + 1, updated_at = now() WHERE run_id = $1";
    const JOB_RESET_MYSQL: &str = "UPDATE action_jobs SET status = CASE WHEN caller_job_id != '' THEN 'waiting' WHEN status = 'skipped' AND needs_json = '[]' THEN 'skipped' WHEN needs_json != '[]' THEN 'waiting' WHEN EXISTS (SELECT 1 FROM action_payloads p WHERE p.sha = action_jobs.payload_sha AND p.payload_json LIKE '%\"call\":{%') THEN 'waiting' ELSE 'queued' END, runner_id = NULL, started_at = NULL, finished_at = NULL, timeout_at = NULL, attempt = attempt + 1, updated_at = CURRENT_TIMESTAMP WHERE run_id = ?";
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE action_runs SET status = 'queued', finished_at = NULL, completion_notified = 0, attempt = attempt + 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            if let Some(j) = job_id {
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "{JOB_RESET_SQLITE} AND (id = ? OR caller_job_id = ?)"
                )))
                .bind(run_id)
                .bind(j)
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
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "{JOB_RESET_PG} AND (id = $2 OR caller_job_id = $2)"
                )))
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
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "{JOB_RESET_MYSQL} AND (id = ? OR caller_job_id = ?)"
                )))
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
    // Requeued jobs are claimable again — wake `runner.v1` pollers.
    bump_action_tasks_version(pool).await?;
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
                "UPDATE action_jobs SET status = 'cancelled', finished_at = COALESCE(finished_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP WHERE run_id = ? AND status IN ('queued', 'waiting', 'in_progress')",
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
                "UPDATE action_jobs SET status = 'cancelled', finished_at = COALESCE(finished_at, now()), updated_at = now() WHERE run_id = $1 AND status IN ('queued', 'waiting', 'in_progress')",
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
                "UPDATE action_jobs SET status = 'cancelled', finished_at = COALESCE(finished_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP WHERE run_id = ? AND status IN ('queued', 'waiting', 'in_progress')",
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
    /// `read` | `write` (validated by CHECK / Rust callers) — the `contents`
    /// level; the full GitHub scope map lives in `perms_json` (Phase 7).
    pub permission: String,
    /// Frozen GitHub scope map minted at claim — `{scope: none|read|write}`.
    pub perms_json: Option<String>,
    /// sha256 of the per-job `ACTIONS_ID_TOKEN_REQUEST_TOKEN` (OIDC).
    pub oidc_request_hash: Option<String>,
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
            perms_json: row.try_get("perms_json").unwrap_or(None),
            oidc_request_hash: row.try_get("oidc_request_hash").unwrap_or(None),
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
    perms_json: Option<&str>,
    token_hash: &str,
    expires_at: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO action_job_tokens (id, job_id, run_id, repository_id, user_id, permission, perms_json, token_hash, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT (job_id) DO UPDATE SET token_hash = excluded.token_hash,
                     permission = excluded.permission, perms_json = excluded.perms_json,
                     expires_at = excluded.expires_at, oidc_request_hash = NULL,
                     revoked_at = NULL",
            )
            .bind(id).bind(job_id).bind(run_id).bind(repository_id).bind(user_id)
            .bind(permission).bind(perms_json).bind(token_hash).bind(expires_at)
            .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO action_job_tokens (id, job_id, run_id, repository_id, user_id, permission, perms_json, token_hash, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::timestamptz)
                 ON CONFLICT (job_id) DO UPDATE SET token_hash = excluded.token_hash,
                     permission = excluded.permission, perms_json = excluded.perms_json,
                     expires_at = excluded.expires_at, oidc_request_hash = NULL,
                     revoked_at = NULL",
            )
            .bind(id).bind(job_id).bind(run_id).bind(repository_id).bind(user_id)
            .bind(permission).bind(perms_json).bind(token_hash).bind(expires_at)
            .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO action_job_tokens (id, job_id, run_id, repository_id, user_id, permission, perms_json, token_hash, expires_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'))
                 ON DUPLICATE KEY UPDATE token_hash = VALUES(token_hash),
                     permission = VALUES(permission), perms_json = VALUES(perms_json),
                     expires_at = VALUES(expires_at), oidc_request_hash = NULL,
                     revoked_at = NULL",
            )
            .bind(id).bind(job_id).bind(run_id).bind(repository_id).bind(user_id)
            .bind(permission).bind(perms_json).bind(token_hash).bind(expires_at)
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
                "SELECT id, job_id, run_id, repository_id, user_id, permission, perms_json, oidc_request_hash, token_hash,
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
                "SELECT id, job_id, run_id, repository_id, user_id, permission, perms_json, oidc_request_hash, token_hash,
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
                "SELECT id, job_id, run_id, repository_id, user_id, permission, perms_json, oidc_request_hash, token_hash,
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

/// `action_artifacts` row (0054) — metadata for a stored artifact archive;
/// content lives on disk at `storage_path` under the artifacts dir.
#[derive(Debug, Clone)]
pub struct ActionArtifactRow {
    pub id: String,
    pub repository_id: String,
    pub run_id: String,
    pub job_id: Option<String>,
    pub name: String,
    pub storage_path: String,
    pub size_bytes: i64,
    pub digest_sha256: String,
    pub expires_at: Option<String>,
    pub created_at: String,
}

const ARTIFACT_COLS: &str = "id, repository_id, run_id, job_id, name, storage_path, size_bytes, digest_sha256, expires_at, created_at";
/// MySQL TIMESTAMP columns do not decode into String — project RFC3339 text.
const ARTIFACT_COLS_MYSQL: &str = "id, repository_id, run_id, job_id, name, storage_path, size_bytes, digest_sha256, DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') AS expires_at, DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at";

macro_rules! map_artifact_row {
    ($row:expr) => {{
        use sqlx::Row;
        let r = $row;
        ActionArtifactRow {
            id: r.get("id"),
            repository_id: r.get("repository_id"),
            run_id: r.get("run_id"),
            job_id: r.try_get("job_id").unwrap_or(None),
            name: r.get("name"),
            storage_path: r.get("storage_path"),
            size_bytes: r
                .try_get::<i64, _>("size_bytes")
                .or_else(|_| r.try_get::<i32, _>("size_bytes").map(i64::from))
                .unwrap_or(0),
            digest_sha256: r.try_get("digest_sha256").unwrap_or_default(),
            expires_at: r.try_get("expires_at").unwrap_or(None),
            created_at: r.try_get("created_at").unwrap_or_default(),
        }
    }};
}

/// Insert-or-replace an artifact record — re-uploading the same (run, name)
/// overwrites (GitHub v4 `overwrite: true` semantics).
pub async fn upsert_action_artifact(pool: &DbPool, a: &ActionArtifactRow) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "INSERT INTO action_artifacts ({ARTIFACT_COLS}) VALUES (?,?,?,?,?,?,?,?,?,strftime('%Y-%m-%d %H:%M:%S','now')) \
                 ON CONFLICT(run_id, name) DO UPDATE SET job_id=excluded.job_id, storage_path=excluded.storage_path, \
                 size_bytes=excluded.size_bytes, digest_sha256=excluded.digest_sha256, expires_at=excluded.expires_at, \
                 updated_at=strftime('%Y-%m-%d %H:%M:%S','now')"
            )))
            .bind(&a.id).bind(&a.repository_id).bind(&a.run_id).bind(&a.job_id)
            .bind(&a.name).bind(&a.storage_path).bind(a.size_bytes)
            .bind(&a.digest_sha256).bind(&a.expires_at)
            .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "INSERT INTO action_artifacts ({ARTIFACT_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9::timestamptz,now()) \
                 ON CONFLICT(run_id, name) DO UPDATE SET job_id=excluded.job_id, storage_path=excluded.storage_path, \
                 size_bytes=excluded.size_bytes, digest_sha256=excluded.digest_sha256, expires_at=excluded.expires_at, \
                 updated_at=now()"
            )))
            .bind(&a.id).bind(&a.repository_id).bind(&a.run_id).bind(&a.job_id)
            .bind(&a.name).bind(&a.storage_path).bind(a.size_bytes)
            .bind(&a.digest_sha256).bind(&a.expires_at)
            .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "INSERT INTO action_artifacts ({ARTIFACT_COLS}) VALUES (?,?,?,?,?,?,?,?,STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'),NOW()) \
                 ON DUPLICATE KEY UPDATE job_id=VALUES(job_id), storage_path=VALUES(storage_path), \
                 size_bytes=VALUES(size_bytes), digest_sha256=VALUES(digest_sha256), expires_at=VALUES(expires_at)"
            )))
            .bind(&a.id).bind(&a.repository_id).bind(&a.run_id).bind(&a.job_id)
            .bind(&a.name).bind(&a.storage_path).bind(a.size_bytes)
            .bind(&a.digest_sha256).bind(&a.expires_at)
            .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn find_action_artifact(
    pool: &DbPool,
    run_id: &str,
    name: &str,
) -> Result<Option<ActionArtifactRow>, String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ARTIFACT_COLS} FROM action_artifacts WHERE run_id = ? AND name = ?"
        )))
        .bind(run_id)
        .bind(name)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_artifact_row!(r)))
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ARTIFACT_COLS} FROM action_artifacts WHERE run_id = $1 AND name = $2"
        )))
        .bind(run_id)
        .bind(name)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_artifact_row!(r)))
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ARTIFACT_COLS_MYSQL} FROM action_artifacts WHERE run_id = ? AND name = ?"
        )))
        .bind(run_id)
        .bind(name)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_artifact_row!(r)))
        .map_err(|e| e.to_string()),
    }
}

pub async fn list_action_artifacts_for_run(
    pool: &DbPool,
    run_id: &str,
) -> Result<Vec<ActionArtifactRow>, String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ARTIFACT_COLS} FROM action_artifacts WHERE run_id = ? ORDER BY name"
        )))
        .bind(run_id)
        .fetch_all(p)
        .await
        .map(|rs| rs.into_iter().map(|r| map_artifact_row!(r)).collect())
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ARTIFACT_COLS} FROM action_artifacts WHERE run_id = $1 ORDER BY name"
        )))
        .bind(run_id)
        .fetch_all(p)
        .await
        .map(|rs| rs.into_iter().map(|r| map_artifact_row!(r)).collect())
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ARTIFACT_COLS_MYSQL} FROM action_artifacts WHERE run_id = ? ORDER BY name"
        )))
        .bind(run_id)
        .fetch_all(p)
        .await
        .map(|rs| rs.into_iter().map(|r| map_artifact_row!(r)).collect())
        .map_err(|e| e.to_string()),
    }
}

/// Artifact by primary key — the user-facing download/delete paths address
/// artifacts by id (names are only unique within a run).
pub async fn find_action_artifact_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<ActionArtifactRow>, String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ARTIFACT_COLS} FROM action_artifacts WHERE id = ?"
        )))
        .bind(id)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_artifact_row!(r)))
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ARTIFACT_COLS} FROM action_artifacts WHERE id = $1"
        )))
        .bind(id)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_artifact_row!(r)))
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ARTIFACT_COLS_MYSQL} FROM action_artifacts WHERE id = ?"
        )))
        .bind(id)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_artifact_row!(r)))
        .map_err(|e| e.to_string()),
    }
}

/// Delete an artifact record by id (row only — the caller owns the payload
/// file under the artifacts dir). Returns whether a row existed.
pub async fn delete_action_artifact(pool: &DbPool, id: &str) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM action_artifacts WHERE id = ?")
            .bind(id)
            .execute(p)
            .await
            .map(|r| r.rows_affected()),
        DbPool::Postgres(p) => sqlx::query("DELETE FROM action_artifacts WHERE id = $1")
            .bind(id)
            .execute(p)
            .await
            .map(|r| r.rows_affected()),
        DbPool::MySql(p) => sqlx::query("DELETE FROM action_artifacts WHERE id = ?")
            .bind(id)
            .execute(p)
            .await
            .map(|r| r.rows_affected()),
    }
    .map_err(|e| e.to_string())?;
    Ok(affected > 0)
}

// --- `runner.v1` task claims + tasks_version counter (0055) ---------------

/// Bump the global `tasks` watermark — every enqueue/requeue of claimable
/// work must call this so Gitea-protocol runners see `tasks_version` drift
/// and re-poll for work. Returns the new value.
pub async fn bump_action_tasks_version(pool: &DbPool) -> Result<i64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO action_counters (name, value) VALUES ('tasks', 1)
                 ON CONFLICT(name) DO UPDATE SET value = value + 1",
            )
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query("SELECT value FROM action_counters WHERE name = 'tasks'")
                .fetch_one(p)
                .await
                .map_err(|e| e.to_string())?
                .try_get::<i64, _>("value")
                .map_err(|e| e.to_string())
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO action_counters (name, value) VALUES ('tasks', 1)
                 ON CONFLICT(name) DO UPDATE SET value = action_counters.value + 1",
            )
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query("SELECT value FROM action_counters WHERE name = 'tasks'")
                .fetch_one(p)
                .await
                .map_err(|e| e.to_string())?
                .try_get::<i64, _>("value")
                .map_err(|e| e.to_string())
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO action_counters (name, value) VALUES ('tasks', 1)
                 ON DUPLICATE KEY UPDATE value = value + 1",
            )
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query("SELECT value FROM action_counters WHERE name = 'tasks'")
                .fetch_one(p)
                .await
                .map_err(|e| e.to_string())?
                .try_get::<i64, _>("value")
                .map_err(|e| e.to_string())
        }
    }
}

/// Current `tasks` watermark (0 before the first enqueue).
pub async fn action_tasks_version(pool: &DbPool) -> Result<i64, String> {
    const SQL: &str =
        "SELECT COALESCE(MAX(value), 0) AS v FROM action_counters WHERE name = 'tasks'";
    match pool {
        DbPool::Sqlite(p) => sqlx::query(SQL)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?
            .try_get::<i64, _>("v")
            .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(SQL)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?
            .try_get::<i64, _>("v")
            .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(SQL)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?
            .try_get::<i64, _>("v")
            .map_err(|e| e.to_string()),
    }
}

/// Mint a never-reused numeric `task_id` for a claimed job (Gitea
/// `runner.v1.Task.id` ↔ `action_jobs.id` mapping).
pub async fn mint_task_claim(pool: &DbPool, job_id: &str, runner_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT INTO action_task_claims (job_id, runner_id) VALUES (?, ?)")
                .bind(job_id)
                .bind(runner_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())
                .map(|r| r.last_insert_rowid())
        }
        DbPool::MySql(p) => {
            sqlx::query("INSERT INTO action_task_claims (job_id, runner_id) VALUES (?, ?)")
                .bind(job_id)
                .bind(runner_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())
                .map(|r| r.last_insert_id() as i64)
        }
        DbPool::Postgres(p) => sqlx::query(
            "INSERT INTO action_task_claims (job_id, runner_id) VALUES ($1, $2) RETURNING task_id",
        )
        .bind(job_id)
        .bind(runner_id)
        .fetch_one(p)
        .await
        .map_err(|e| e.to_string())?
        .try_get::<i64, _>("task_id")
        .map_err(|e| e.to_string()),
    }
}

/// Resolve a `task_id` back to `(job_id, runner_id)`.
pub async fn find_task_claim(
    pool: &DbPool,
    task_id: i64,
) -> Result<Option<(String, String)>, String> {
    macro_rules! run {
        ($q:expr, $p:expr) => {
            sqlx::query($q)
                .bind(task_id)
                .fetch_optional($p)
                .await
                .map_err(|e| e.to_string())
                .map(|opt| {
                    opt.map(|r| {
                        (
                            r.try_get::<String, _>("job_id").unwrap_or_default(),
                            r.try_get::<String, _>("runner_id").unwrap_or_default(),
                        )
                    })
                })
        };
    }
    match pool {
        DbPool::Sqlite(p) => run!(
            "SELECT job_id, runner_id FROM action_task_claims WHERE task_id = ?",
            p
        ),
        DbPool::Postgres(p) => run!(
            "SELECT job_id, runner_id FROM action_task_claims WHERE task_id = $1",
            p
        ),
        DbPool::MySql(p) => run!(
            "SELECT job_id, runner_id FROM action_task_claims WHERE task_id = ?",
            p
        ),
    }
}

// ---------------------------------------------------------------------------
// Phase 7 — permissions breadth, environments, OIDC issuer
// ---------------------------------------------------------------------------

/// `repositories.actions_default_permissions` — `read` (restricted) |
/// `write` (permissive). Drives [`effective_permissions`] at enqueue when a
/// workflow declares no `permissions:`.
pub async fn get_actions_default_permissions(
    pool: &DbPool,
    repo_id: &str,
) -> Result<String, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row =
                sqlx::query("SELECT actions_default_permissions FROM repositories WHERE id = ?")
                    .bind(repo_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(row
                .and_then(|r| r.try_get("actions_default_permissions").ok())
                .unwrap_or_else(|| "read".into()))
        }
        DbPool::Postgres(p) => {
            let row =
                sqlx::query("SELECT actions_default_permissions FROM repositories WHERE id = $1")
                    .bind(repo_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(row
                .and_then(|r| r.try_get("actions_default_permissions").ok())
                .unwrap_or_else(|| "read".into()))
        }
        DbPool::MySql(p) => {
            let row =
                sqlx::query("SELECT actions_default_permissions FROM repositories WHERE id = ?")
                    .bind(repo_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(row
                .and_then(|r| r.try_get("actions_default_permissions").ok())
                .unwrap_or_else(|| "read".into()))
        }
    }
}

pub async fn set_actions_default_permissions(
    pool: &DbPool,
    repo_id: &str,
    default_permissions: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE repositories SET actions_default_permissions = ? WHERE id = ?")
                .bind(default_permissions)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE repositories SET actions_default_permissions = $1 WHERE id = $2")
                .bind(default_permissions)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE repositories SET actions_default_permissions = ? WHERE id = ?")
                .bind(default_permissions)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Bind the per-job `ACTIONS_ID_TOKEN_REQUEST_TOKEN` hash (OIDC request
/// credential) — set at claim when the granted scope set carries
/// `id-token: write`. Re-mint clears it (see `insert_action_job_token`).
pub async fn set_job_token_oidc_hash(
    pool: &DbPool,
    job_id: &str,
    hash: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_job_tokens SET oidc_request_hash = ? WHERE job_id = ?")
                .bind(hash)
                .bind(job_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_job_tokens SET oidc_request_hash = $1 WHERE job_id = $2")
                .bind(hash)
                .bind(job_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_job_tokens SET oidc_request_hash = ? WHERE job_id = ?")
                .bind(hash)
                .bind(job_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Resolve an `ACTIONS_ID_TOKEN_REQUEST_TOKEN` — returns the token row bound
/// to a non-terminal job so the OIDC endpoint can mint against live context.
pub async fn find_job_token_by_oidc_hash(
    pool: &DbPool,
    hash: &str,
) -> Result<Option<ActionJobTokenRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT id, job_id, run_id, repository_id, user_id, permission, perms_json, oidc_request_hash, token_hash,
                        strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
                        expires_at, strftime('%Y-%m-%dT%H:%M:%SZ', revoked_at) AS revoked_at
                 FROM action_job_tokens WHERE oidc_request_hash = ?",
            ).bind(hash).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_job_token!(r)))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT id, job_id, run_id, repository_id, user_id, permission, perms_json, oidc_request_hash, token_hash,
                        to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
                        to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at,
                        to_char(revoked_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS revoked_at
                 FROM action_job_tokens WHERE oidc_request_hash = $1",
            ).bind(hash).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_job_token!(r)))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT id, job_id, run_id, repository_id, user_id, permission, perms_json, oidc_request_hash, token_hash,
                        DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
                        DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') AS expires_at,
                        DATE_FORMAT(revoked_at, '%Y-%m-%dT%H:%i:%sZ') AS revoked_at
                 FROM action_job_tokens WHERE oidc_request_hash = ?",
            ).bind(hash).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_job_token!(r)))
        }
    }
}

// ---- environments ---------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ActionEnvironmentRow {
    pub id: String,
    pub repository_id: String,
    pub name: String,
    /// GitHub protection shape: `required_reviewers` (user ids),
    /// `wait_timer_minutes`, `deployment_branches`.
    pub protection_json: String,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_environment {
    ($r:expr) => {{
        let r = $r;
        ActionEnvironmentRow {
            id: r.get("id"),
            repository_id: r.get("repository_id"),
            name: r.get("name"),
            protection_json: r.try_get("protection_json").unwrap_or_else(|_| "{}".into()),
            created_at: r.try_get("created_at").unwrap_or_default(),
            updated_at: r.try_get("updated_at").unwrap_or_default(),
        }
    }};
}

/// Create the environment row when absent — GitHub auto-creates environments
/// on first workflow reference. Returns the stored row either way.
pub async fn ensure_environment(
    pool: &DbPool,
    id: &str,
    repository_id: &str,
    name: &str,
) -> Result<ActionEnvironmentRow, String> {
    if let Some(row) = find_environment(pool, repository_id, name).await? {
        return Ok(row);
    }
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT OR IGNORE INTO action_environments (id, repository_id, name) VALUES (?, ?, ?)")
                .bind(id).bind(repository_id).bind(name)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("INSERT INTO action_environments (id, repository_id, name) VALUES ($1, $2, $3) ON CONFLICT (repository_id, name) DO NOTHING")
                .bind(id).bind(repository_id).bind(name)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT IGNORE INTO action_environments (id, repository_id, name) VALUES (?, ?, ?)",
            )
            .bind(id)
            .bind(repository_id)
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    find_environment(pool, repository_id, name)
        .await?
        .ok_or_else(|| "environment insert vanished".to_string())
}

pub async fn find_environment(
    pool: &DbPool,
    repository_id: &str,
    name: &str,
) -> Result<Option<ActionEnvironmentRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT id, repository_id, name, protection_json, created_at, updated_at FROM action_environments WHERE repository_id = ? AND name = ?")
                .bind(repository_id).bind(name).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_environment!(r)))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT id, repository_id, name, protection_json, created_at::text, updated_at::text FROM action_environments WHERE repository_id = $1 AND name = $2")
                .bind(repository_id).bind(name).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_environment!(r)))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT id, repository_id, name, protection_json, CAST(created_at AS CHAR), CAST(updated_at AS CHAR) FROM action_environments WHERE repository_id = ? AND name = ?")
                .bind(repository_id).bind(name).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_environment!(r)))
        }
    }
}

pub async fn list_environments(
    pool: &DbPool,
    repository_id: &str,
) -> Result<Vec<ActionEnvironmentRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query("SELECT id, repository_id, name, protection_json, created_at, updated_at FROM action_environments WHERE repository_id = ? ORDER BY name")
                .bind(repository_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_environment!(r)).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query("SELECT id, repository_id, name, protection_json, created_at::text, updated_at::text FROM action_environments WHERE repository_id = $1 ORDER BY name")
                .bind(repository_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_environment!(r)).collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query("SELECT id, repository_id, name, protection_json, CAST(created_at AS CHAR), CAST(updated_at AS CHAR) FROM action_environments WHERE repository_id = ? ORDER BY name")
                .bind(repository_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_environment!(r)).collect())
        }
    }
}

/// Upsert protection rules — `protection_json` is validated upstream.
pub async fn upsert_environment(
    pool: &DbPool,
    id: &str,
    repository_id: &str,
    name: &str,
    protection_json: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT INTO action_environments (id, repository_id, name, protection_json) VALUES (?, ?, ?, ?)
                         ON CONFLICT (repository_id, name) DO UPDATE SET protection_json = excluded.protection_json, updated_at = CURRENT_TIMESTAMP")
                .bind(id).bind(repository_id).bind(name).bind(protection_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("INSERT INTO action_environments (id, repository_id, name, protection_json) VALUES ($1, $2, $3, $4)
                         ON CONFLICT (repository_id, name) DO UPDATE SET protection_json = excluded.protection_json, updated_at = now()")
                .bind(id).bind(repository_id).bind(name).bind(protection_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("INSERT INTO action_environments (id, repository_id, name, protection_json) VALUES (?, ?, ?, ?)
                         ON DUPLICATE KEY UPDATE protection_json = VALUES(protection_json)")
                .bind(id).bind(repository_id).bind(name).bind(protection_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn delete_environment(
    pool: &DbPool,
    repository_id: &str,
    name: &str,
) -> Result<bool, String> {
    macro_rules! run {
        ($q:expr, $p:expr) => {
            sqlx::query($q)
                .bind(repository_id)
                .bind(name)
                .execute($p)
                .await
                .map(|r| r.rows_affected() > 0)
                .map_err(|e| e.to_string())
        };
    }
    match pool {
        DbPool::Sqlite(p) => run!(
            "DELETE FROM action_environments WHERE repository_id = ? AND name = ?",
            p
        ),
        DbPool::Postgres(p) => run!(
            "DELETE FROM action_environments WHERE repository_id = $1 AND name = $2",
            p
        ),
        DbPool::MySql(p) => run!(
            "DELETE FROM action_environments WHERE repository_id = ? AND name = ?",
            p
        ),
    }
}

// ---- deployments ----------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ActionDeploymentRow {
    pub id: String,
    pub repository_id: String,
    pub environment: String,
    pub run_id: String,
    pub job_id: String,
    pub sha: String,
    pub git_ref: String,
    pub url: Option<String>,
    /// Derived at read time from `action_jobs.status`.
    pub job_status: String,
    pub created_at: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_deployment(
    pool: &DbPool,
    id: &str,
    repository_id: &str,
    environment: &str,
    run_id: &str,
    job_id: &str,
    sha: &str,
    git_ref: &str,
    url: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT INTO action_deployments (id, repository_id, environment, run_id, job_id, sha, ref, url) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(id).bind(repository_id).bind(environment).bind(run_id).bind(job_id).bind(sha).bind(git_ref).bind(url)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("INSERT INTO action_deployments (id, repository_id, environment, run_id, job_id, sha, ref, url) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)")
                .bind(id).bind(repository_id).bind(environment).bind(run_id).bind(job_id).bind(sha).bind(git_ref).bind(url)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("INSERT INTO action_deployments (id, repository_id, environment, run_id, job_id, sha, ref, url) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(id).bind(repository_id).bind(environment).bind(run_id).bind(job_id).bind(sha).bind(git_ref).bind(url)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Deployments for a run — `job_status` joins the live job row.
pub async fn list_deployments_for_run(
    pool: &DbPool,
    run_id: &str,
) -> Result<Vec<ActionDeploymentRow>, String> {
    macro_rules! map {
        ($r:expr) => {{
            let r = $r;
            ActionDeploymentRow {
                id: r.get("id"),
                repository_id: r.get("repository_id"),
                environment: r.get("environment"),
                run_id: r.get("run_id"),
                job_id: r.get("job_id"),
                sha: r.get("sha"),
                git_ref: r.get("ref"),
                url: r.try_get("url").unwrap_or(None),
                job_status: r.try_get("job_status").unwrap_or_else(|_| "queued".into()),
                created_at: r.try_get("created_at").unwrap_or_default(),
            }
        }};
    }
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT d.id, d.repository_id, d.environment, d.run_id, d.job_id, d.sha, d.ref, d.url,
                        j.status AS job_status, d.created_at
                 FROM action_deployments d JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.run_id = ? ORDER BY d.created_at",
            ).bind(run_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map!(r)).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT d.id, d.repository_id, d.environment, d.run_id, d.job_id, d.sha, d.ref, d.url,
                        j.status AS job_status, d.created_at::text AS created_at
                 FROM action_deployments d JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.run_id = $1 ORDER BY d.created_at",
            ).bind(run_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map!(r)).collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT d.id, d.repository_id, d.environment, d.run_id, d.job_id, d.sha, d.ref, d.url,
                        j.status AS job_status, CAST(d.created_at AS CHAR) AS created_at
                 FROM action_deployments d JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.run_id = ? ORDER BY d.created_at",
            ).bind(run_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map!(r)).collect())
        }
    }
}

/// Repo-scoped deployment listing (newest first) — backs the repo
/// Environments view. `environment` narrows to one environment.
pub async fn list_deployments_for_repo(
    pool: &DbPool,
    repository_id: &str,
    environment: Option<&str>,
    limit: i64,
) -> Result<Vec<ActionDeploymentRow>, String> {
    macro_rules! map {
        ($r:expr) => {{
            let r = $r;
            ActionDeploymentRow {
                id: r.get("id"),
                repository_id: r.get("repository_id"),
                environment: r.get("environment"),
                run_id: r.get("run_id"),
                job_id: r.get("job_id"),
                sha: r.get("sha"),
                git_ref: r.get("ref"),
                url: r.try_get("url").unwrap_or(None),
                job_status: r.try_get("job_status").unwrap_or_else(|_| "queued".into()),
                created_at: r.try_get("created_at").unwrap_or_default(),
            }
        }};
    }
    let limit = limit.clamp(1, 500);
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT d.id, d.repository_id, d.environment, d.run_id, d.job_id, d.sha, d.ref, d.url,
                        j.status AS job_status, d.created_at
                 FROM action_deployments d JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = ? AND (? IS NULL OR d.environment = ?)
                 ORDER BY d.created_at DESC, d.id DESC LIMIT ?",
            ).bind(repository_id).bind(environment).bind(environment).bind(limit)
            .fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map!(r)).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT d.id, d.repository_id, d.environment, d.run_id, d.job_id, d.sha, d.ref, d.url,
                        j.status AS job_status, d.created_at::text AS created_at
                 FROM action_deployments d JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = $1 AND ($2::text IS NULL OR d.environment = $2)
                 ORDER BY d.created_at DESC, d.id DESC LIMIT $3",
            ).bind(repository_id).bind(environment).bind(limit)
            .fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map!(r)).collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT d.id, d.repository_id, d.environment, d.run_id, d.job_id, d.sha, d.ref, d.url,
                        j.status AS job_status, CAST(d.created_at AS CHAR) AS created_at
                 FROM action_deployments d JOIN action_jobs j ON j.id = d.job_id
                 WHERE d.repository_id = ? AND (? IS NULL OR d.environment = ?)
                 ORDER BY d.created_at DESC, d.id DESC LIMIT ?",
            ).bind(repository_id).bind(environment).bind(environment).bind(limit)
            .fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map!(r)).collect())
        }
    }
}

// ---- deployment reviews ---------------------------------------------------

/// `action_deployment_reviews` row (0061) — one approve/reject decision on an
/// environment-held job. `reviewer_username` is denormalized for display.
#[derive(Debug, Clone)]
pub struct ActionDeploymentReviewRow {
    pub id: String,
    pub repository_id: String,
    pub run_id: String,
    pub job_id: String,
    pub environment: String,
    pub reviewer_id: String,
    pub reviewer_username: String,
    /// `'approved'` | `'rejected'`.
    pub state: String,
    pub comment: Option<String>,
    pub created_at: String,
}

pub async fn insert_deployment_review(
    pool: &DbPool,
    r: &ActionDeploymentReviewRow,
) -> Result<(), String> {
    macro_rules! ins {
        ($q:expr, $p:expr) => {{
            sqlx::query($q)
                .bind(&r.id)
                .bind(&r.repository_id)
                .bind(&r.run_id)
                .bind(&r.job_id)
                .bind(&r.environment)
                .bind(&r.reviewer_id)
                .bind(&r.reviewer_username)
                .bind(&r.state)
                .bind(&r.comment)
                .execute($p)
                .await
                .map_err(|e| e.to_string())?;
        }};
    }
    match pool {
        DbPool::Sqlite(p) => ins!(
            "INSERT INTO action_deployment_reviews
             (id, repository_id, run_id, job_id, environment, reviewer_id, reviewer_username, state, comment)
             VALUES (?,?,?,?,?,?,?,?,?)",
            p
        ),
        DbPool::Postgres(p) => ins!(
            "INSERT INTO action_deployment_reviews
             (id, repository_id, run_id, job_id, environment, reviewer_id, reviewer_username, state, comment)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            p
        ),
        DbPool::MySql(p) => ins!(
            "INSERT INTO action_deployment_reviews
             (id, repository_id, run_id, job_id, environment, reviewer_id, reviewer_username, state, comment)
             VALUES (?,?,?,?,?,?,?,?,?)",
            p
        ),
    }
    Ok(())
}

/// Every review recorded against a run's jobs — feeds `getRun` serialization.
pub async fn list_deployment_reviews_for_run(
    pool: &DbPool,
    run_id: &str,
) -> Result<Vec<ActionDeploymentReviewRow>, String> {
    macro_rules! map {
        ($r:expr) => {{
            let r = $r;
            ActionDeploymentReviewRow {
                id: r.get("id"),
                repository_id: r.get("repository_id"),
                run_id: r.get("run_id"),
                job_id: r.get("job_id"),
                environment: r.get("environment"),
                reviewer_id: r.get("reviewer_id"),
                reviewer_username: r.get("reviewer_username"),
                state: r.get("state"),
                comment: r.try_get("comment").unwrap_or(None),
                created_at: r.try_get("created_at").unwrap_or_default(),
            }
        }};
    }
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT id, repository_id, run_id, job_id, environment, reviewer_id,
                        reviewer_username, state, comment, created_at
                 FROM action_deployment_reviews WHERE run_id = ? ORDER BY created_at",
            )
            .bind(run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map!(r)).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT id, repository_id, run_id, job_id, environment, reviewer_id,
                        reviewer_username, state, comment, created_at::text AS created_at
                 FROM action_deployment_reviews WHERE run_id = $1 ORDER BY created_at",
            )
            .bind(run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map!(r)).collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT id, repository_id, run_id, job_id, environment, reviewer_id,
                        reviewer_username, state, comment, CAST(created_at AS CHAR) AS created_at
                 FROM action_deployment_reviews WHERE run_id = ? ORDER BY created_at",
            )
            .bind(run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map!(r)).collect())
        }
    }
}

/// Reject a held job — terminal `failure`, hold cleared. Only fires when the
/// row is still `waiting` + held (the CAS keeps a raced approve/reject pair
/// from both landing).
pub async fn reject_action_job(pool: &DbPool, job_id: &str) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE action_jobs SET status = 'failure', approval_hold = 0, finished_at = strftime('%Y-%m-%d %H:%M:%S','now'), updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
             WHERE id = ? AND approval_hold = 1 AND status = 'waiting'",
        )
        .bind(job_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected()),
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE action_jobs SET status = 'failure', approval_hold = false, finished_at = now(), updated_at = now()
             WHERE id = $1 AND approval_hold AND status = 'waiting'",
        )
        .bind(job_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected()),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE action_jobs SET status = 'failure', approval_hold = 0, finished_at = NOW(), updated_at = NOW()
            WHERE id = ? AND approval_hold = 1 AND status = 'waiting'",
        )
        .bind(job_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected()),
    }
    .map_err(|e| e.to_string())?;
    Ok(affected > 0)
}

// ---- deployment wait-timer ------------------------------------------------

/// Stamp the wait-timer on a needs-gated leg when its upstreams resolve —
/// the leg stays `waiting` until `deployment_not_before` passes (scheduler
/// promotion calls this instead of queueing directly).
pub async fn set_deployment_timer(
    pool: &DbPool,
    job_id: &str,
    not_before: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_jobs SET deployment_not_before = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(not_before).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_jobs SET deployment_not_before = $1, updated_at = now() WHERE id = $2")
                .bind(not_before).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_jobs SET deployment_not_before = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(not_before).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Promote `waiting` legs whose wait timer lapsed — the sweeper ticks this.
/// `deployment_not_before` is only ever stamped at the ready transition
/// (enqueue for ungated legs, upstream-resolve for needs-gated ones), so a
/// non-NULL value is itself proof the leg is otherwise runnable.
/// `now` is an RFC3339 UTC string (text comparison is format-stable).
pub async fn promote_due_deployment_jobs(pool: &DbPool, now: &str) -> Result<u64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_jobs SET status = 'queued', deployment_not_before = NULL, updated_at = CURRENT_TIMESTAMP
                         WHERE status = 'waiting' AND approval_hold = 0
                           AND deployment_not_before IS NOT NULL AND deployment_not_before <= ?")
                .bind(now).execute(p).await.map(|r| r.rows_affected()).map_err(|e| e.to_string())
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_jobs SET status = 'queued', deployment_not_before = NULL, updated_at = now()
                         WHERE status = 'waiting' AND approval_hold = false
                           AND deployment_not_before IS NOT NULL AND deployment_not_before <= $1")
                .bind(now).execute(p).await.map(|r| r.rows_affected()).map_err(|e| e.to_string())
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_jobs SET status = 'queued', deployment_not_before = NULL, updated_at = CURRENT_TIMESTAMP
                         WHERE status = 'waiting' AND approval_hold = 0
                           AND deployment_not_before IS NOT NULL AND deployment_not_before <= ?")
                .bind(now).execute(p).await.map(|r| r.rows_affected()).map_err(|e| e.to_string())
        }
    }
}

// ---- OIDC signing keys ----------------------------------------------------

#[derive(Debug, Clone)]
pub struct ActionOidcKeyRow {
    pub kid: String,
    pub alg: String,
    /// PKCS#8 hex of the Ed25519 private key — legacy rows only; RS256 keys
    /// store ciphertext in `private_enc` instead.
    pub private_der: String,
    /// Actions-secrets ciphertext of the PKCS#8-hex signing key (RS256 rows).
    pub private_enc: Option<String>,
    /// Public JWK JSON ({kty, n|x, e|crv, kid, alg, use}).
    pub public_jwk_json: String,
    pub active: bool,
    pub created_at: String,
}

macro_rules! map_oidc_key {
    ($r:expr) => {{
        let r = $r;
        ActionOidcKeyRow {
            kid: r.get("kid"),
            alg: r.try_get("alg").unwrap_or_else(|_| "EdDSA".into()),
            private_der: r.get("private_der"),
            private_enc: r.try_get("private_enc").ok().flatten(),
            public_jwk_json: r.get("public_jwk_json"),
            active: r
                .try_get::<bool, _>("active")
                .or_else(|_| r.try_get::<i64, _>("active").map(|v| v != 0))
                .or_else(|_| r.try_get::<i8, _>("active").map(|v| v != 0))
                .unwrap_or(false),
            created_at: r.try_get("created_at").unwrap_or_default(),
        }
    }};
}

/// Insert a signing key (generated upstream — the DB only stores it). RS256
/// rows carry the private key as ciphertext in `private_enc` (`private_der`
/// is the legacy plaintext-hex column, left empty for new rows).
#[allow(clippy::too_many_arguments)]
pub async fn insert_oidc_key(
    pool: &DbPool,
    kid: &str,
    alg: &str,
    private_der: &str,
    private_enc: Option<&str>,
    public_jwk_json: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT INTO action_oidc_keys (kid, alg, private_der, private_enc, public_jwk_json, active) VALUES (?, ?, ?, ?, ?, 1)")
                .bind(kid).bind(alg).bind(private_der).bind(private_enc).bind(public_jwk_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("INSERT INTO action_oidc_keys (kid, alg, private_der, private_enc, public_jwk_json, active) VALUES ($1, $2, $3, $4, $5, true)")
                .bind(kid).bind(alg).bind(private_der).bind(private_enc).bind(public_jwk_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("INSERT INTO action_oidc_keys (kid, alg, private_der, private_enc, public_jwk_json, active) VALUES (?, ?, ?, ?, ?, 1)")
                .bind(kid).bind(alg).bind(private_der).bind(private_enc).bind(public_jwk_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// The active signing key for an algorithm — NULL when none has been
/// generated yet.
pub async fn find_active_oidc_key(
    pool: &DbPool,
    alg: &str,
) -> Result<Option<ActionOidcKeyRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT kid, alg, private_der, private_enc, public_jwk_json, active, created_at FROM action_oidc_keys WHERE active = 1 AND alg = ? ORDER BY created_at DESC LIMIT 1")
                .bind(alg)
                .fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_oidc_key!(r)))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT kid, alg, private_der, private_enc, public_jwk_json, active, created_at::text AS created_at FROM action_oidc_keys WHERE active AND alg = $1 ORDER BY created_at DESC LIMIT 1")
                .bind(alg)
                .fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_oidc_key!(r)))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT kid, alg, private_der, private_enc, public_jwk_json, active, created_at FROM action_oidc_keys WHERE active = 1 AND alg = ? ORDER BY created_at DESC LIMIT 1")
                .bind(alg)
                .fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_oidc_key!(r)))
        }
    }
}

/// Public halves of every key for an algorithm — JWKS keeps rotated keys of
/// the current alg verifiable while in-flight JWTs age out.
pub async fn list_oidc_public_jwks(pool: &DbPool, alg: &str) -> Result<Vec<String>, String> {
    macro_rules! run {
        ($q:expr, $p:expr, $a:expr) => {
            sqlx::query($q)
                .bind($a)
                .fetch_all($p)
                .await
                .map_err(|e| e.to_string())
                .map(|rows| {
                    rows.into_iter()
                        .filter_map(|r| r.try_get::<String, _>("public_jwk_json").ok())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<String>>()
                })
        };
    }
    match pool {
        DbPool::Sqlite(p) => run!(
            "SELECT public_jwk_json FROM action_oidc_keys WHERE alg = ?",
            p,
            alg
        ),
        DbPool::Postgres(p) => run!(
            "SELECT public_jwk_json FROM action_oidc_keys WHERE alg = $1",
            p,
            alg
        ),
        DbPool::MySql(p) => run!(
            "SELECT public_jwk_json FROM action_oidc_keys WHERE alg = ?",
            p,
            alg
        ),
    }
}

/// Release one approval-held job — deployment review approval is per-job in
/// GitHub (`approveJob`), unlike `approve_run` which releases the whole run.
/// The leg lands `waiting` for the scheduler's recompute to restore.
pub async fn approve_job(pool: &DbPool, job_id: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE action_jobs SET approval_hold = 0 WHERE id = ? AND approval_hold != 0",
        )
        .bind(job_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE action_jobs SET approval_hold = false WHERE id = $1 AND approval_hold",
        )
        .bind(job_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE action_jobs SET approval_hold = 0 WHERE id = ? AND approval_hold != 0",
        )
        .bind(job_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
    }
}

// ---- Phase 8: runner heartbeat, log ordering, org/env secrets -------------

/// Bump `heartbeat_at` for a live job — every runner contact (update_log /
/// update_task) proves the claiming runner is still alive. Best-effort.
pub async fn touch_job_heartbeat(pool: &DbPool, job_id: &str, at: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_jobs SET heartbeat_at = ? WHERE id = ?")
                .bind(at)
                .bind(job_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_jobs SET heartbeat_at = $1 WHERE id = $2")
                .bind(at)
                .bind(job_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_jobs SET heartbeat_at = ? WHERE id = ?")
                .bind(at)
                .bind(job_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Log-batch ordering CAS — the appended batch is accepted only when the
/// caller's batch index equals the stored `log_index` cursor:
///
/// - `index == stored` → append; cursor advances (`log_index += 1`), `Accept`.
/// - `index <  stored` → replay of an already-appended batch — drop it and
///   answer `Duplicate` so the caller treats the write as landed.
/// - `index >  stored` → a gap — the caller missed batches; `Gap` tells the
///   endpoint to 409 so the runner resends from the right cursor.
///
/// The increment folds into the same statement so two racing runners cannot
/// interleave a batch (claim binding already restricts this to one runner,
/// but the CAS keeps the write side self-consistent on retries).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogIndexCas {
    Accept,
    Duplicate,
    Gap,
}

pub async fn advance_job_log_index(
    pool: &DbPool,
    job_id: &str,
    index: i64,
) -> Result<LogIndexCas, String> {
    let stored = job_log_index(pool, job_id).await?;
    if index == stored {
        match pool {
            DbPool::Sqlite(p) => {
                sqlx::query(
                    "UPDATE action_jobs SET log_index = log_index + 1 WHERE id = ? AND log_index = ?",
                )
                .bind(job_id)
                .bind(index)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
            DbPool::Postgres(p) => {
                sqlx::query(
                    "UPDATE action_jobs SET log_index = log_index + 1 WHERE id = $1 AND log_index = $2",
                )
                .bind(job_id)
                .bind(index)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
            DbPool::MySql(p) => {
                sqlx::query(
                    "UPDATE action_jobs SET log_index = log_index + 1 WHERE id = ? AND log_index = ?",
                )
                .bind(job_id)
                .bind(index)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        Ok(LogIndexCas::Accept)
    } else if index < stored {
        Ok(LogIndexCas::Duplicate)
    } else {
        Ok(LogIndexCas::Gap)
    }
}

/// Guarded cursor move for the gitea row-index protocol — sets `log_index`
/// to `next` only when the stored cursor still equals `expected`.
pub async fn set_job_log_index_if(
    pool: &DbPool,
    job_id: &str,
    expected: i64,
    next: i64,
) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_jobs SET log_index = ? WHERE id = ? AND log_index = ?")
                .bind(next)
                .bind(job_id)
                .bind(expected)
                .execute(p)
                .await
                .map(|r| r.rows_affected() > 0)
                .map_err(|e| e.to_string())
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_jobs SET log_index = $1 WHERE id = $2 AND log_index = $3")
                .bind(next)
                .bind(job_id)
                .bind(expected)
                .execute(p)
                .await
                .map(|r| r.rows_affected() > 0)
                .map_err(|e| e.to_string())
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_jobs SET log_index = ? WHERE id = ? AND log_index = ?")
                .bind(next)
                .bind(job_id)
                .bind(expected)
                .execute(p)
                .await
                .map(|r| r.rows_affected() > 0)
                .map_err(|e| e.to_string())
        }
    }
}

/// The stored next-expected log batch index for a job.
pub async fn job_log_index(pool: &DbPool, job_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row: (i64,) = sqlx::query_as("SELECT log_index FROM action_jobs WHERE id = ?")
                .bind(job_id)
                .fetch_one(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(row.0)
        }
        DbPool::Postgres(p) => {
            let row: (i32,) = sqlx::query_as("SELECT log_index FROM action_jobs WHERE id = $1")
                .bind(job_id)
                .fetch_one(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(i64::from(row.0))
        }
        DbPool::MySql(p) => {
            let row: (i64,) = sqlx::query_as("SELECT log_index FROM action_jobs WHERE id = ?")
                .bind(job_id)
                .fetch_one(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(row.0)
        }
    }
}

/// A live job the sweeper may reap — surfaced so the caller can write the
/// requeue/timeout notice into the job log before mutating the row.
#[derive(Debug, Clone)]
pub struct StaleJobRow {
    pub id: String,
    pub run_id: String,
    pub job_key: String,
    pub runner_id: Option<String>,
    pub heartbeat_at: Option<String>,
    pub started_at: Option<String>,
    pub timeout_at: Option<String>,
    pub attempt: i64,
}

macro_rules! map_stale_job {
    ($r:expr) => {{
        let r = $r;
        StaleJobRow {
            id: r.get("id"),
            run_id: r.get("run_id"),
            job_key: r.get("job_key"),
            runner_id: r.try_get("runner_id").unwrap_or(None),
            heartbeat_at: r.try_get("heartbeat_at").unwrap_or(None),
            started_at: r.try_get("started_at").unwrap_or(None),
            timeout_at: r.try_get("timeout_at").unwrap_or(None),
            attempt: r
                .try_get::<i64, _>("attempt")
                .or_else(|_| r.try_get::<i32, _>("attempt").map(i64::from))
                .unwrap_or(1),
        }
    }};
}

/// All `in_progress` jobs — the sweeper decides staleness/deadline in Rust so
/// dialect timestamp formats never leak into policy. Bounded by live runners.
pub async fn list_in_progress_jobs(pool: &DbPool) -> Result<Vec<StaleJobRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT id, run_id, job_key, runner_id, heartbeat_at, started_at, timeout_at, attempt FROM action_jobs WHERE status = 'in_progress'",
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_stale_job!(r)).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT id, run_id, job_key, runner_id, heartbeat_at, \
                 started_at::text AS started_at, \
                 to_char(timeout_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS timeout_at, \
                 attempt FROM action_jobs WHERE status = 'in_progress'",
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_stale_job!(r)).collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT id, run_id, job_key, runner_id, heartbeat_at, CAST(started_at AS CHAR) AS started_at, DATE_FORMAT(timeout_at, '%Y-%m-%dT%H:%i:%sZ') AS timeout_at, attempt FROM action_jobs WHERE status = 'in_progress'",
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_stale_job!(r)).collect())
        }
    }
}

/// Requeue one stale job: back to `queued`, runner/log cursor cleared,
/// `attempt` bumped. The next claim re-mints credentials and replays the log
/// stream from `log_index = 0` (the requeue deliberately resets the cursor —
/// the new runner's stream replaces the zombie's).
pub async fn requeue_stale_job(pool: &DbPool, job_id: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE action_jobs SET status = 'queued', runner_id = NULL, \
             heartbeat_at = NULL, log_index = 0, attempt = attempt + 1, \
             updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status = 'in_progress'",
        )
        .bind(job_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE action_jobs SET status = 'queued', runner_id = NULL, \
             heartbeat_at = NULL, log_index = 0, attempt = attempt + 1, \
             updated_at = now() WHERE id = $1 AND status = 'in_progress'",
        )
        .bind(job_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE action_jobs SET status = 'queued', runner_id = NULL, \
             heartbeat_at = NULL, log_index = 0, attempt = attempt + 1, \
             updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status = 'in_progress'",
        )
        .bind(job_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
    }
}

/// Delete a caller leg's callee rows — re-expansion on rerun re-emits fresh
/// rows; the replaced legs' log files stay on disk but their rows are gone.
/// Only callee rows (`caller_job_id = ?`) are touched.
pub async fn delete_callee_jobs(pool: &DbPool, caller_job_id: &str) -> Result<u64, String> {
    const SQL: &str = "DELETE FROM action_jobs WHERE caller_job_id = ?";
    const SQL_PG: &str = "DELETE FROM action_jobs WHERE caller_job_id = $1";
    match pool {
        DbPool::Sqlite(p) => sqlx::query(SQL)
            .bind(caller_job_id)
            .execute(p)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(SQL_PG)
            .bind(caller_job_id)
            .execute(p)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(SQL)
            .bind(caller_job_id)
            .execute(p)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| e.to_string()),
    }
}

/// Fail one job past its `timeout_at` deadline — guarded on `in_progress` so
/// a terminal state the runner raced in still wins.
pub async fn fail_job_deadline(pool: &DbPool, job_id: &str, now: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE action_jobs SET status = 'failure', finished_at = COALESCE(finished_at, CURRENT_TIMESTAMP), \
             updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status = 'in_progress' AND timeout_at IS NOT NULL AND timeout_at < ?",
        )
        .bind(job_id)
        .bind(now)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE action_jobs SET status = 'failure', finished_at = COALESCE(finished_at, now()), \
             updated_at = now() WHERE id = $1 AND status = 'in_progress' AND timeout_at IS NOT NULL AND timeout_at < $2::timestamptz",
        )
        .bind(job_id)
        .bind(now)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE action_jobs SET status = 'failure', finished_at = COALESCE(finished_at, CURRENT_TIMESTAMP), \
             updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status = 'in_progress' AND timeout_at IS NOT NULL AND timeout_at < ?",
        )
        .bind(job_id)
        .bind(now)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
    }
}

// ---- org secrets ------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OrgSecretRow {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub visibility: String,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_org_secret {
    ($r:expr) => {{
        let r = $r;
        OrgSecretRow {
            id: r.get("id"),
            owner_id: r.get("owner_id"),
            name: r.get("name"),
            visibility: r.try_get("visibility").unwrap_or_else(|_| "all".into()),
            created_at: r.try_get("created_at").unwrap_or_default(),
            updated_at: r.try_get("updated_at").unwrap_or_default(),
        }
    }};
}

pub async fn upsert_org_secret(
    pool: &DbPool,
    id: &str,
    owner_id: &str,
    name: &str,
    ciphertext: &str,
    visibility: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT INTO action_org_secrets (id, owner_id, name, ciphertext, visibility) VALUES (?, ?, ?, ?, ?)
                         ON CONFLICT (owner_id, name) DO UPDATE SET ciphertext = excluded.ciphertext, visibility = excluded.visibility, updated_at = CURRENT_TIMESTAMP")
                .bind(id).bind(owner_id).bind(name).bind(ciphertext).bind(visibility)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("INSERT INTO action_org_secrets (id, owner_id, name, ciphertext, visibility) VALUES ($1, $2, $3, $4, $5)
                         ON CONFLICT (owner_id, name) DO UPDATE SET ciphertext = excluded.ciphertext, visibility = excluded.visibility, updated_at = now()")
                .bind(id).bind(owner_id).bind(name).bind(ciphertext).bind(visibility)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("INSERT INTO action_org_secrets (id, owner_id, name, ciphertext, visibility) VALUES (?, ?, ?, ?, ?)
                         ON DUPLICATE KEY UPDATE ciphertext = VALUES(ciphertext), visibility = VALUES(visibility)")
                .bind(id).bind(owner_id).bind(name).bind(ciphertext).bind(visibility)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn list_org_secrets(pool: &DbPool, owner_id: &str) -> Result<Vec<OrgSecretRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query("SELECT id, owner_id, name, visibility, created_at, updated_at FROM action_org_secrets WHERE owner_id = ? ORDER BY name")
                .bind(owner_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_org_secret!(r)).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query("SELECT id, owner_id, name, visibility, created_at::text, updated_at::text FROM action_org_secrets WHERE owner_id = $1 ORDER BY name")
                .bind(owner_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_org_secret!(r)).collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query("SELECT id, owner_id, name, visibility, CAST(created_at AS CHAR), CAST(updated_at AS CHAR) FROM action_org_secrets WHERE owner_id = ? ORDER BY name")
                .bind(owner_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_org_secret!(r)).collect())
        }
    }
}

pub async fn find_org_secret_by_name(
    pool: &DbPool,
    owner_id: &str,
    name: &str,
) -> Result<Option<OrgSecretRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT id, owner_id, name, visibility, created_at, updated_at FROM action_org_secrets WHERE owner_id = ? AND name = ?")
                .bind(owner_id).bind(name).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_org_secret!(r)))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT id, owner_id, name, visibility, created_at::text, updated_at::text FROM action_org_secrets WHERE owner_id = $1 AND name = $2")
                .bind(owner_id).bind(name).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_org_secret!(r)))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT id, owner_id, name, visibility, CAST(created_at AS CHAR), CAST(updated_at AS CHAR) FROM action_org_secrets WHERE owner_id = ? AND name = ?")
                .bind(owner_id).bind(name).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_org_secret!(r)))
        }
    }
}

pub async fn delete_org_secret(pool: &DbPool, owner_id: &str, name: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM action_org_secrets WHERE owner_id = ? AND name = ?")
                .bind(owner_id)
                .bind(name)
                .execute(p)
                .await
                .map(|r| r.rows_affected() > 0)
                .map_err(|e| e.to_string())
        }
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM action_org_secrets WHERE owner_id = $1 AND name = $2")
                .bind(owner_id)
                .bind(name)
                .execute(p)
                .await
                .map(|r| r.rows_affected() > 0)
                .map_err(|e| e.to_string())
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM action_org_secrets WHERE owner_id = ? AND name = ?")
                .bind(owner_id)
                .bind(name)
                .execute(p)
                .await
                .map(|r| r.rows_affected() > 0)
                .map_err(|e| e.to_string())
        }
    }
}

/// Replace the `visibility = 'selected'` repo set for an org secret.
pub async fn set_org_secret_repos(
    pool: &DbPool,
    secret_id: &str,
    repository_ids: &[String],
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM action_org_secret_repos WHERE secret_id = ?")
                .bind(secret_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            for repo_id in repository_ids {
                sqlx::query(
                    "INSERT OR IGNORE INTO action_org_secret_repos (secret_id, repository_id) VALUES (?, ?)",
                )
                .bind(secret_id)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM action_org_secret_repos WHERE secret_id = $1")
                .bind(secret_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            for repo_id in repository_ids {
                sqlx::query(
                    "INSERT INTO action_org_secret_repos (secret_id, repository_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                )
                .bind(secret_id)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM action_org_secret_repos WHERE secret_id = ?")
                .bind(secret_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            for repo_id in repository_ids {
                sqlx::query(
                    "INSERT IGNORE INTO action_org_secret_repos (secret_id, repository_id) VALUES (?, ?)",
                )
                .bind(secret_id)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// Repo ids in a `visibility = 'selected'` org secret's allow set.
pub async fn list_org_secret_repos(pool: &DbPool, secret_id: &str) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query_scalar(
                "SELECT repository_id FROM action_org_secret_repos WHERE secret_id = ? ORDER BY repository_id",
            )
            .bind(secret_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())
        }
        DbPool::Postgres(p) => {
            sqlx::query_scalar(
                "SELECT repository_id FROM action_org_secret_repos WHERE secret_id = $1 ORDER BY repository_id",
            )
            .bind(secret_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar(
                "SELECT repository_id FROM action_org_secret_repos WHERE secret_id = ? ORDER BY repository_id",
            )
            .bind(secret_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())
        }
    }
}

/// Org secrets visible to `repository_id` at claim time: the repo's owner is
/// resolved inside the query (`owner_type = 'org'`), then `visibility` picks
/// the reach — 'all' every repo, 'private' private repos, 'selected' the
/// explicit join list.
pub async fn list_org_secret_ciphertexts_for_repo(
    pool: &DbPool,
    repository_id: &str,
) -> Result<Vec<ActionSecretCipherRow>, String> {
    const SQL: &str = "SELECT s.name, s.ciphertext FROM action_org_secrets s
      WHERE s.owner_id = (SELECT r.owner_id FROM repositories r WHERE r.id = ? AND r.owner_type = 'org')
        AND (s.visibility = 'all'
          OR (s.visibility = 'private' AND (SELECT r2.visibility FROM repositories r2 WHERE r2.id = ?) = 'private')
          OR (s.visibility = 'selected' AND EXISTS (
                SELECT 1 FROM action_org_secret_repos x WHERE x.secret_id = s.id AND x.repository_id = ?)))
      ORDER BY s.name";
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query_as::<_, ActionSecretCipherRow>(SQL)
                .bind(repository_id)
                .bind(repository_id)
                .bind(repository_id)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(rows)
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query_as::<_, ActionSecretCipherRow>(
                "SELECT s.name, s.ciphertext FROM action_org_secrets s
                  WHERE s.owner_id = (SELECT r.owner_id FROM repositories r WHERE r.id = $1 AND r.owner_type = 'org')
                    AND (s.visibility = 'all'
                      OR (s.visibility = 'private' AND (SELECT r2.visibility FROM repositories r2 WHERE r2.id = $2) = 'private')
                      OR (s.visibility = 'selected' AND EXISTS (
                            SELECT 1 FROM action_org_secret_repos x WHERE x.secret_id = s.id AND x.repository_id = $3)))
                  ORDER BY s.name",
            )
            .bind(repository_id)
            .bind(repository_id)
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query_as::<_, ActionSecretCipherRow>(SQL)
                .bind(repository_id)
                .bind(repository_id)
                .bind(repository_id)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(rows)
        }
    }
}

// ---- environment secrets -----------------------------------------------------

#[derive(Debug, Clone)]
pub struct EnvSecretMetaRow {
    pub id: String,
    pub environment_id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn upsert_environment_secret(
    pool: &DbPool,
    id: &str,
    environment_id: &str,
    name: &str,
    ciphertext: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("INSERT INTO action_environment_secrets (id, environment_id, name, ciphertext) VALUES (?, ?, ?, ?)
                         ON CONFLICT (environment_id, name) DO UPDATE SET ciphertext = excluded.ciphertext, updated_at = CURRENT_TIMESTAMP")
                .bind(id).bind(environment_id).bind(name).bind(ciphertext)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("INSERT INTO action_environment_secrets (id, environment_id, name, ciphertext) VALUES ($1, $2, $3, $4)
                         ON CONFLICT (environment_id, name) DO UPDATE SET ciphertext = excluded.ciphertext, updated_at = now()")
                .bind(id).bind(environment_id).bind(name).bind(ciphertext)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("INSERT INTO action_environment_secrets (id, environment_id, name, ciphertext) VALUES (?, ?, ?, ?)
                         ON DUPLICATE KEY UPDATE ciphertext = VALUES(ciphertext)")
                .bind(id).bind(environment_id).bind(name).bind(ciphertext)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn list_environment_secrets(
    pool: &DbPool,
    environment_id: &str,
) -> Result<Vec<EnvSecretMetaRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query("SELECT id, environment_id, name, created_at, updated_at FROM action_environment_secrets WHERE environment_id = ? ORDER BY name")
                .bind(environment_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| EnvSecretMetaRow {
                    id: r.get("id"),
                    environment_id: r.get("environment_id"),
                    name: r.get("name"),
                    created_at: r.try_get("created_at").unwrap_or_default(),
                    updated_at: r.try_get("updated_at").unwrap_or_default(),
                })
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query("SELECT id, environment_id, name, created_at::text, updated_at::text FROM action_environment_secrets WHERE environment_id = $1 ORDER BY name")
                .bind(environment_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| EnvSecretMetaRow {
                    id: r.get("id"),
                    environment_id: r.get("environment_id"),
                    name: r.get("name"),
                    created_at: r.try_get("created_at").unwrap_or_default(),
                    updated_at: r.try_get("updated_at").unwrap_or_default(),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query("SELECT id, environment_id, name, CAST(created_at AS CHAR), CAST(updated_at AS CHAR) FROM action_environment_secrets WHERE environment_id = ? ORDER BY name")
                .bind(environment_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| EnvSecretMetaRow {
                    id: r.get("id"),
                    environment_id: r.get("environment_id"),
                    name: r.get("name"),
                    created_at: r.try_get("created_at").unwrap_or_default(),
                    updated_at: r.try_get("updated_at").unwrap_or_default(),
                })
                .collect())
        }
    }
}

pub async fn delete_environment_secret(
    pool: &DbPool,
    environment_id: &str,
    name: &str,
) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "DELETE FROM action_environment_secrets WHERE environment_id = ? AND name = ?",
        )
        .bind(environment_id)
        .bind(name)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(
            "DELETE FROM action_environment_secrets WHERE environment_id = $1 AND name = $2",
        )
        .bind(environment_id)
        .bind(name)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(
            "DELETE FROM action_environment_secrets WHERE environment_id = ? AND name = ?",
        )
        .bind(environment_id)
        .bind(name)
        .execute(p)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.to_string()),
    }
}

/// Claim-time environment secret read — ciphertexts only; the API layer
/// decrypts. Returns `(environment_id-scoped)` rows for the resolved env.
pub async fn list_environment_secret_ciphertexts(
    pool: &DbPool,
    environment_id: &str,
) -> Result<Vec<ActionSecretCipherRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query_as::<_, ActionSecretCipherRow>(
                "SELECT name, ciphertext FROM action_environment_secrets WHERE environment_id = ? ORDER BY name",
            )
            .bind(environment_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows)
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query_as::<_, ActionSecretCipherRow>(
                "SELECT name, ciphertext FROM action_environment_secrets WHERE environment_id = $1 ORDER BY name",
            )
            .bind(environment_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query_as::<_, ActionSecretCipherRow>(
                "SELECT name, ciphertext FROM action_environment_secrets WHERE environment_id = ? ORDER BY name",
            )
            .bind(environment_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows)
        }
    }
}
