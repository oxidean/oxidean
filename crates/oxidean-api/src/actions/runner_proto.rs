//! Actions runner protocol mount — Register/Declare/FetchTask/UpdateTask/UpdateLog (D-ACT-07).
//! HTTP+JSON tracer compatible with pinned `proto/runner.proto` (prost types available as `pb`).

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::actions::logs::append_job_log;
use crate::actions::tokens::{accept_registration_token, hash_token};
use crate::app::AppState;

/// Generated protobuf types (Compile via build.rs).
pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/oxidean.actions.v1.rs"));
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/declare", post(declare))
        .route("/fetch_task", post(fetch_task))
        .route("/update_task", post(update_task))
        .route("/update_log", post(update_log))
        // Anonymous template-icon route (GET …/template-icons/{name}.svg) —
        // shares the /api/actions nest; matchit captures the whole segment.
        .route(
            "/template-icons/{name}",
            get(crate::actions::templates::template_icon),
        )
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let raw = auth
        .strip_prefix("Bearer ")
        .or_else(|| auth.strip_prefix("bearer "))?;
    Some(raw.trim().to_string())
}

async fn runner_from_headers(
    state: &AppState,
    headers: &HeaderMap,
    body_token: Option<&str>,
) -> Result<oxidean_db::ActionRunnerRow, StatusCode> {
    let _ = headers.get(axum::http::header::COOKIE); // D-ACT-18: ignore cookies
    let raw = bearer_token(headers)
        .or_else(|| body_token.map(str::to_string))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let hash = hash_token(&raw);
    let runner = state
        .db
        .find_action_runner_by_token_hash(&hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    // Liveness heartbeat — never fail the request on a bookkeeping error.
    if let Err(e) = state.db.touch_action_runner_online(&runner.id).await {
        tracing::warn!(error = %e, runner_id = %runner.id, "failed to bump runner last_online");
    }
    Ok(runner)
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    name: String,
    labels: Vec<String>,
    token: String,
}

#[derive(Debug, Serialize)]
struct RegisterResponse {
    runner_id: String,
    runner_token: String,
}

async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, StatusCode> {
    let _ = headers.get(axum::http::header::COOKIE);
    let ok = accept_registration_token(&state.db, &req.token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !ok {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let runner_token = format!("ort_{}", Uuid::new_v4());
    let runner_hash = hash_token(&runner_token);
    let labels = serde_json::to_string(&req.labels).unwrap_or_else(|_| "[]".into());
    let id = Uuid::new_v4().to_string();
    state
        .db
        .insert_action_runner(&id, &req.name, &runner_hash, &labels, None, false)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Touch prost types so codegen stays linked.
    let _ = pb::RegisterResponse {
        runner_id: id.clone(),
        runner_token: runner_token.clone(),
    };
    Ok(Json(RegisterResponse {
        runner_id: id,
        runner_token,
    }))
}

#[derive(Debug, Deserialize)]
struct DeclareRequest {
    labels: Vec<String>,
    runner_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct OkResponse {
    ok: bool,
}

async fn declare(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeclareRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let runner = runner_from_headers(&state, &headers, req.runner_token.as_deref()).await?;
    let labels = serde_json::to_string(&req.labels).unwrap_or_else(|_| "[]".into());
    state
        .db
        .update_action_runner_labels(&runner.id, &labels)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Debug, Deserialize)]
struct FetchTaskRequest {
    runner_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct FetchTaskResponse {
    job_id: Option<String>,
    run_id: Option<String>,
    job_key: Option<String>,
    runs_on: Option<Vec<String>>,
    workflow_path: Option<String>,
    workflow_name: Option<String>,
    head_sha: Option<String>,
    head_ref: Option<String>,
    /// Step objects for the claimed job — from the durable `action_payloads`
    /// row captured at enqueue (never re-read from the bare repo at claim).
    steps: Option<serde_json::Value>,
    secrets: Option<std::collections::HashMap<String, String>>,
    /// Repository coordinates — runners clone {origin}/{owner}/{repo}.git.
    repository_owner: Option<String>,
    repository_name: Option<String>,
    /// Triggering event name (`push`, `pull_request`, `workflow_dispatch`, …).
    event: Option<String>,
    run_number: Option<i64>,
    /// `action_runs.attempt` — bumped by `requeue_run`; GITHUB_RUN_ATTEMPT.
    run_attempt: Option<i64>,
    /// Username of `triggered_by` ("" when the run was system-triggered).
    actor: Option<String>,
    actor_id: Option<String>,
    repository_id: Option<String>,
    /// Owner user id or org id, whichever `owner_ref_for_repo` resolves.
    repository_owner_id: Option<String>,
    /// Instance public origin (OXIDEAN_PUBLIC_ORIGIN / Railway gateway).
    server_url: Option<String>,
    /// PR metadata — base ref + number for `pull_request` runs only.
    base_ref: Option<String>,
    pull_number: Option<i64>,
    /// Per-job `oxidean_job_…` bearer token (OXIDEAN_TOKEN); absent when the
    /// workflow's `contents` permission is `none` or the run was
    /// system-triggered.
    job_token: Option<String>,
    /// Granted contents level for `job_token`: `read` | `write` | "".
    job_token_permission: Option<String>,
    /// `{owner}/{repo}/{workflow_path}@{head_ref}`.
    workflow_ref: Option<String>,
    /// Job `env:` map captured at enqueue (payload) — runner-side `env`
    /// context + step env overlay.
    env: Option<serde_json::Value>,
    /// Workflow `env:` map captured at enqueue.
    workflow_env: Option<serde_json::Value>,
    /// Evaluated `timeout-minutes` deadline (RFC3339) from `action_jobs`.
    timeout_at: Option<String>,
    /// Triggering event payload (`action_runs.event_payload_json`) →
    /// `github.event` context.
    event_payload: Option<serde_json::Value>,
    /// Matrix assignment (`action_jobs.matrix_json`) → `matrix` context.
    matrix: Option<serde_json::Value>,
    /// Seconds until `timeout_at`, computed at claim — runners don't parse
    /// the timestamp or depend on wall-clock agreement with the API.
    timeout_seconds: Option<i64>,
}

impl FetchTaskResponse {
    /// Empty "no work" response — shared by the no-claim path and the
    /// loud-failure path for claimed jobs missing their payload.
    fn empty() -> Self {
        Self {
            job_id: None,
            run_id: None,
            job_key: None,
            runs_on: None,
            workflow_path: None,
            workflow_name: None,
            head_sha: None,
            head_ref: None,
            steps: None,
            secrets: None,
            repository_owner: None,
            repository_name: None,
            event: None,
            run_number: None,
            run_attempt: None,
            actor: None,
            actor_id: None,
            repository_id: None,
            repository_owner_id: None,
            server_url: None,
            base_ref: None,
            pull_number: None,
            job_token: None,
            job_token_permission: None,
            workflow_ref: None,
            env: None,
            workflow_env: None,
            timeout_at: None,
            event_payload: None,
            matrix: None,
            timeout_seconds: None,
        }
    }
}

async fn fetch_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<FetchTaskRequest>,
) -> Result<Json<FetchTaskResponse>, StatusCode> {
    let runner = runner_from_headers(&state, &headers, req.runner_token.as_deref()).await?;
    let labels: Vec<String> = serde_json::from_str(&runner.labels_json).unwrap_or_default();
    let job = state
        .db
        .claim_queued_action_job_for_labels(&runner.id, &labels)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(match job {
        Some(j) => {
            let runs_on: Vec<String> = serde_json::from_str(&j.runs_on_json).unwrap_or_default();
            if let Err(e) = state.db.recompute_action_run_status(&j.run_id).await {
                tracing::warn!("action run rollup after claim failed: {e}");
            }
            let run = state
                .db
                .find_action_run_by_id(&j.run_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            // PR provenance must resolve BEFORE secrets/token minting: a
            // `pull_request` run whose head commit cannot be traced to a
            // same-repository PR head (fork head, stale synchronize, deleted
            // PR) gets no repo secrets and a read-only job token — GitHub's
            // fork-PR model, failing closed on any lookup miss.
            let pr_ctx = pull_context_for_run(&state.db, &run).await;
            let secrets = if pr_ctx.trusted {
                crate::actions::secrets::decrypted_secrets_for_repo(&state.db, &run.repository_id)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            } else {
                std::collections::HashMap::new()
            };
            // Durable payload: steps + effective permissions captured at
            // enqueue (0053). A claimed job with no payload predates the
            // migration — fail it loudly rather than hand the runner an empty
            // step list it would report as a no-op success.
            let payload = match j.payload_sha.as_deref() {
                Some(sha) => state
                    .db
                    .find_action_payload(sha)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
                None => None,
            };
            let Some(payload) = payload else {
                tracing::error!(job_id = %j.id, run_id = %j.run_id, "claimed job has no captured payload");
                let _ = append_job_log(
                    &state.actions_log_dir,
                    &j.run_id,
                    &j.id,
                    "::error::job payload missing (enqueued before durable payloads) — re-run the workflow\n"
                        .as_bytes(),
                )
                .await;
                if let Err(e) = state.db.update_action_job_status(&j.id, "failure").await {
                    tracing::warn!(error = %e, job_id = %j.id, "failed to fail payload-less job");
                }
                if let Err(e) = state.db.recompute_action_run_status(&j.run_id).await {
                    tracing::warn!(error = %e, run_id = %j.run_id, "run rollup failed");
                }
                return Ok(Json(FetchTaskResponse::empty()));
            };
            let payload_json: serde_json::Value = serde_json::from_str(&payload.payload_json)
                .unwrap_or_else(|_| serde_json::json!({}));
            let steps = payload_json
                .get("steps")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([]));
            let workflow_name = run.workflow_name.clone();
            let mut contents_permission = match payload_json
                .get("contents_permission")
                .and_then(|v| v.as_str())
            {
                Some("write") => crate::actions::ContentsPermission::Write,
                Some("none") => crate::actions::ContentsPermission::None,
                _ => crate::actions::ContentsPermission::Read,
            };
            if !pr_ctx.trusted && contents_permission == crate::actions::ContentsPermission::Write {
                contents_permission = crate::actions::ContentsPermission::Read;
            }
            let repo = state
                .db
                .find_repository_by_id(&run.repository_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            let owner_ref = crate::repo::owner_ref_for_repo(&state.db, &repo)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            let owner_slug = owner_ref.slug().to_string();
            let owner_id = owner_ref.id().to_string();

            // Triggering actor — "" when the run was system-triggered.
            let (actor, actor_id) = match run.triggered_by.as_deref() {
                Some(uid) => {
                    let name = state
                        .db
                        .find_user_by_id(uid)
                        .await
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                        .map(|u| u.username)
                        .unwrap_or_default();
                    (name, uid.to_string())
                }
                None => (String::new(), String::new()),
            };
            let base_ref = pr_ctx.base_ref.clone();
            let pull_number = pr_ctx.pull_number;

            // Per-job OXIDEAN_TOKEN — minted at claim so the grant reflects the
            // workflow's effective `contents` permission at head_sha.
            let (job_token, job_token_permission) =
                match crate::actions::job_tokens::mint_job_token(
                    &state.db,
                    &run,
                    &repo,
                    &owner_ref,
                    &j.id,
                    contents_permission,
                )
                .await
                {
                    Ok(Some((plaintext, perm))) => (Some(plaintext), perm.as_str().to_string()),
                    Ok(None) => (None, String::new()),
                    Err(e) => {
                        tracing::error!(error = %e, job_id = %j.id, "mint_job_token failed");
                        return Err(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                };

            let workflow_ref = format!(
                "{}/{}/{}@{}",
                owner_slug, repo.name, run.workflow_path, run.head_ref
            );
            let server_url = crate::public_origin::resolve_public_origin();

            // Keep prost types linked when regenerating.
            let _ = pb::FetchTaskResponse {
                job_id: j.id.clone(),
                run_id: j.run_id.clone(),
                job_key: j.job_key.clone(),
                runs_on: runs_on.clone(),
                workflow_path: run.workflow_path.clone(),
                workflow_name: workflow_name.clone(),
                head_sha: run.head_sha.clone(),
                head_ref: run.head_ref.clone(),
                steps_json: steps.to_string(),
                secrets_json: serde_json::to_string(&secrets).unwrap_or_else(|_| "{}".into()),
                repository_owner: owner_slug.clone(),
                repository_name: repo.name.clone(),
                event: run.event.clone(),
                run_number: run.run_number,
                run_attempt: run.attempt,
                actor: actor.clone(),
                actor_id: actor_id.clone(),
                repository_id: run.repository_id.clone(),
                repository_owner_id: owner_id.clone(),
                server_url: server_url.clone(),
                base_ref: base_ref.clone(),
                pull_number,
                job_token: job_token.clone().unwrap_or_default(),
                job_token_permission: job_token_permission.clone(),
                workflow_ref: workflow_ref.clone(),
            };
            FetchTaskResponse {
                job_id: Some(j.id),
                run_id: Some(j.run_id),
                job_key: Some(j.job_key),
                runs_on: Some(runs_on),
                workflow_path: Some(run.workflow_path.clone()),
                workflow_name: Some(workflow_name),
                head_sha: Some(run.head_sha.clone()),
                head_ref: Some(run.head_ref.clone()),
                steps: Some(steps),
                secrets: Some(secrets),
                repository_owner: Some(owner_slug),
                repository_name: Some(repo.name),
                event: Some(run.event),
                run_number: Some(run.run_number),
                run_attempt: Some(run.attempt),
                actor: Some(actor),
                actor_id: Some(actor_id),
                repository_id: Some(run.repository_id),
                repository_owner_id: Some(owner_id),
                server_url: Some(server_url),
                base_ref: Some(base_ref),
                pull_number: Some(pull_number),
                job_token,
                job_token_permission: Some(job_token_permission),
                workflow_ref: Some(workflow_ref),
                env: payload_json.get("env").cloned(),
                workflow_env: payload_json.get("workflow_env").cloned(),
                timeout_at: j.timeout_at.clone(),
                event_payload: Some(
                    run.event_payload_json
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or_else(|| serde_json::json!({})),
                ),
                matrix: Some(
                    serde_json::from_str(&j.matrix_json).unwrap_or_else(|_| serde_json::json!({})),
                ),
                timeout_seconds: j.timeout_at.as_deref().and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(s).ok().map(|t| {
                        t.signed_duration_since(chrono::Utc::now())
                            .num_seconds()
                            .max(0)
                    })
                }),
            }
        }
        None => FetchTaskResponse::empty(),
    }))
}

/// PR provenance for `pull_request` runs.
///
/// `trusted` is the security boundary, decided by the `head_repo_id` recorded
/// on the run at dispatch: `true` only when it is present and equals the run's
/// `repository_id` (a same-repo branch PR). Fork heads and `pull_request` runs
/// with no recorded provenance resolve `false` — callers must treat that as
/// untrusted (no secrets, read-only token). Non-`pull_request` events are
/// always trusted.
struct PullRunContext {
    base_ref: String,
    pull_number: i64,
    trusted: bool,
}

async fn pull_context_for_run(
    db: &oxidean_db::Database,
    run: &oxidean_db::ActionRunRow,
) -> PullRunContext {
    if run.event != "pull_request" {
        return PullRunContext {
            base_ref: String::new(),
            pull_number: 0,
            trusted: true,
        };
    }
    let trusted = run.head_repo_id.as_deref() == Some(run.repository_id.as_str());
    // Cosmetic PR metadata (base ref + number) — best effort from the open
    // pulls list. Two pulls can legitimately share a head SHA (a fork and a
    // same-repo branch at the same commit), so the ref is the discriminator
    // before falling back to SHA-only and ref-only matches.
    let short = run
        .head_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(&run.head_ref);
    let pulls = db
        .list_pulls_for_repo(&run.repository_id, Some("open"), 0, 100)
        .await
        .map(|(rows, _)| rows)
        .unwrap_or_default();
    let (base_ref, pull_number) = pulls
        .iter()
        .find(|p| p.head_sha == run.head_sha && (p.head_ref == short || p.head_ref == run.head_ref))
        .or_else(|| pulls.iter().find(|p| p.head_sha == run.head_sha))
        .or_else(|| {
            pulls
                .iter()
                .find(|p| p.head_ref == short || p.head_ref == run.head_ref)
        })
        .map(|p| (p.base_ref.clone(), p.number))
        .unwrap_or_else(|| (String::new(), 0));
    PullRunContext {
        base_ref,
        pull_number,
        trusted,
    }
}

#[derive(Debug, Deserialize)]
struct UpdateTaskRequest {
    job_id: String,
    state: String,
    runner_token: Option<String>,
}

async fn update_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let runner = runner_from_headers(&state, &headers, req.runner_token.as_deref()).await?;
    let job = state
        .db
        .find_action_job_by_id(&req.job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if job.runner_id.as_deref() != Some(runner.id.as_str()) {
        return Err(StatusCode::FORBIDDEN);
    }
    let status = match req.state.as_str() {
        "in_progress" | "success" | "failure" | "cancelled" => req.state.as_str(),
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    // API-04: capture the pre-update run status so the queued→in_progress
    // transition emits `workflow_run` `in_progress` exactly once.
    let prev_run_status = state
        .db
        .find_action_run_by_id(&job.run_id)
        .await
        .ok()
        .flatten()
        .map(|r| r.status);
    state
        .db
        .update_action_job_status(&job.id, status)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Terminal state revokes the per-job OXIDEAN_TOKEN (idempotent).
    if matches!(status, "success" | "failure" | "cancelled") {
        if let Err(e) = state.db.revoke_action_job_token_for_job(&job.id).await {
            tracing::warn!(error = %e, job_id = %job.id, "failed to revoke job token");
        }
    }
    if let Err(e) = state.db.recompute_action_run_status(&job.run_id).await {
        tracing::warn!(error = %e, run_id = %job.run_id, "failed to roll up run status");
    }
    // DEBT-06: at-most-once run-completion notification routed through the
    // watch matrix (all watchers + triggering user; ignore suppresses).
    if state
        .db
        .claim_run_completion_notice(&job.run_id)
        .await
        .unwrap_or(false)
    {
        if let Ok(Some(run)) = state.db.find_action_run_by_id(&job.run_id).await {
            crate::notify::fanout_workflow_completed(&state.db, &run).await;
        }
    }
    // API-04: emit workflow_run webhook on queued→in_progress and terminal states.
    if let Ok(Some(run)) = state.db.find_action_run_by_id(&job.run_id).await {
        let action = match run.status.as_str() {
            "success" | "failure" | "cancelled" => Some("completed"),
            "in_progress" if prev_run_status.as_deref() == Some("queued") => Some("in_progress"),
            _ => None,
        };
        if let Some(action) = action {
            crate::webhook::dispatch::notify_workflow_run(
                &state.db,
                &run.id,
                action,
                &state.env_name,
            )
            .await;
        }
    }
    let origin = std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok();
    if let Err(e) = crate::actions::statuses::publish_from_job_update(
        &state.db,
        &job.id,
        status,
        origin.as_deref(),
    )
    .await
    {
        tracing::warn!(error = %e, job_id = %job.id, "failed to publish job commit status");
    }
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Debug, Deserialize)]
struct UpdateLogRequest {
    run_id: String,
    job_id: String,
    chunk: String,
    runner_token: Option<String>,
}

/// Cap per-request log appends so a stolen runner token cannot fill the disk in one call.
const MAX_LOG_CHUNK_BYTES: usize = 256 * 1024;

async fn update_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UpdateLogRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let runner = runner_from_headers(&state, &headers, req.runner_token.as_deref()).await?;
    if req.chunk.len() > MAX_LOG_CHUNK_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let job = state
        .db
        .find_action_job_by_id(&req.job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if job.runner_id.as_deref() != Some(runner.id.as_str()) {
        return Err(StatusCode::FORBIDDEN);
    }
    // Bind path to the job row — never trust client run_id for disk layout.
    if req.run_id != job.run_id {
        return Err(StatusCode::BAD_REQUEST);
    }
    append_job_log(
        &state.actions_log_dir,
        &job.run_id,
        &job.id,
        req.chunk.as_bytes(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(OkResponse { ok: true }))
}
