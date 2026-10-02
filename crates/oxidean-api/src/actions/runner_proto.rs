//! Actions runner protocol mount — Register/Declare/FetchTask/UpdateTask/UpdateLog (D-ACT-07).
//! HTTP+JSON tracer compatible with pinned `proto/runner.proto` (prost types available as `pb`).

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
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
    /// Step objects for the claimed job (re-read from workflow YAML at head_sha).
    steps: Option<serde_json::Value>,
    secrets: Option<std::collections::HashMap<String, String>>,
    /// Repository coordinates — runners clone {origin}/{owner}/{repo}.git.
    repository_owner: Option<String>,
    repository_name: Option<String>,
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
            let secrets =
                crate::actions::secrets::decrypted_secrets_for_repo(&state.db, &run.repository_id)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let (steps, workflow_name) = job_steps_at_head(&state, &run, &j.job_key)
                .await
                .unwrap_or((serde_json::Value::Array(vec![]), run.workflow_name.clone()));
            let repo = state
                .db
                .find_repository_by_id(&run.repository_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            let owner_slug = crate::repo::owner_ref_for_repo(&state.db, &repo)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|o| o.slug().to_string())
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
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
            };
            FetchTaskResponse {
                job_id: Some(j.id),
                run_id: Some(j.run_id),
                job_key: Some(j.job_key),
                runs_on: Some(runs_on),
                workflow_path: Some(run.workflow_path),
                workflow_name: Some(workflow_name),
                head_sha: Some(run.head_sha),
                head_ref: Some(run.head_ref),
                steps: Some(steps),
                secrets: Some(secrets),
                repository_owner: Some(owner_slug),
                repository_name: Some(repo.name),
            }
        }
        None => FetchTaskResponse {
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
        },
    }))
}

/// Re-read workflow YAML at the run's head and extract steps for `job_key`.
async fn job_steps_at_head(
    state: &AppState,
    run: &oxidean_db::ActionRunRow,
    job_key: &str,
) -> Result<(serde_json::Value, String), StatusCode> {
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
    let bare = state
        .repos_dir
        .join(owner_ref.slug())
        .join(format!("{}.git", repo.name));
    let discovered = crate::actions::discover_workflows(state.git.as_ref(), &bare, &run.head_sha)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let wf = discovered
        .into_iter()
        .find(|w| w.path == run.workflow_path)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let job = wf
        .document
        .jobs
        .iter()
        .find(|j| j.id == job_key)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let steps = serde_json::to_value(&job.steps).unwrap_or_else(|_| serde_json::json!([]));
    Ok((steps, wf.document.name))
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
    if let Err(e) = state.db.recompute_action_run_status(&job.run_id).await {
        tracing::warn!(error = %e, run_id = %job.run_id, "failed to roll up run status");
    }
    if let Ok(Some(run)) = state.db.find_action_run_by_id(&job.run_id).await {
        let action = match run.status.as_str() {
            "success" | "failure" | "cancelled" => Some("completed"),
            "in_progress" if prev_run_status.as_deref() == Some("queued") => {
                Some("in_progress")
            }
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
