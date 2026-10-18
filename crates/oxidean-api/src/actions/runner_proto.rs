//! Actions runner protocol mount — Register/Declare/FetchTask/UpdateTask/UpdateLog (D-ACT-07).
//! HTTP+JSON tracer compatible with pinned `proto/runner.proto` (prost types available as `pb`).

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
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
        // Gitea `runner.v1` compat surface — stock act_runner speaks
        // ConnectRPC unary (application/proto | application/json) at these
        // fixed procedure paths.
        .route(
            "/runner.v1.RunnerService/Register",
            post(crate::actions::gitea_proto::register),
        )
        .route(
            "/runner.v1.RunnerService/Declare",
            post(crate::actions::gitea_proto::declare),
        )
        .route(
            "/runner.v1.RunnerService/FetchTask",
            post(crate::actions::gitea_proto::fetch_task),
        )
        .route(
            "/runner.v1.RunnerService/UpdateTask",
            post(crate::actions::gitea_proto::update_task),
        )
        .route(
            "/runner.v1.RunnerService/UpdateLog",
            post(crate::actions::gitea_proto::update_log),
        )
        // act_runner pings the instance before registering / on daemon start.
        .route(
            "/ping.v1.PingService/Ping",
            post(crate::actions::gitea_proto::ping),
        )
        .route("/artifacts/upload", post(crate::actions::artifacts::upload))
        .route(
            "/artifacts/{run_id}/{name}",
            get(crate::actions::artifacts::download),
        )
        .route("/artifacts/list", post(crate::actions::artifacts::list))
        // Anonymous template-icon route (GET …/template-icons/{name}.svg) —
        // shares the /api/actions nest; matchit captures the whole segment.
        .route(
            "/template-icons/{name}",
            get(crate::actions::templates::template_icon),
        )
        // actions/cache service — `_apis/artifactcache` REST surface authed
        // by per-job runtime tokens (not the runner token).
        .merge(crate::actions::cache::router())
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

/// Artifact endpoints authenticate with the same runner bearer token.
pub(crate) async fn runner_from_headers_pub(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<oxidean_db::ActionRunnerRow, StatusCode> {
    runner_from_headers(state, headers, None).await
}

pub(crate) async fn runner_from_headers(
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

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Serialize)]
struct OkResponse {
    ok: bool,
    /// `update_task` only — the job was cancelled server-side (concurrency
    /// supersede, fail-fast sibling, run cancel) while the runner was still
    /// executing; the runner aborts remaining steps on `true`.
    #[serde(default, skip_serializing_if = "is_false")]
    cancelled: bool,
    /// `update_log` only — the server's next expected batch index. Returned
    /// on every outcome (accept, duplicate-drop, gap-409) so the runner can
    /// resync its cursor without another round-trip.
    #[serde(default)]
    log_index: i64,
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
    Ok(Json(OkResponse {
        ok: true,
        cancelled: false,
        log_index: 0,
    }))
}

#[derive(Debug, Deserialize)]
struct FetchTaskRequest {
    runner_token: Option<String>,
    /// Long-poll hold (Phase 9): when no job is claimable, keep the request
    /// open up to this many ms (server caps at 30s), returning as soon as a
    /// job lands. `0`/absent keeps the old immediate-empty behavior.
    #[serde(default)]
    wait_ms: Option<u64>,
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
    /// Configuration variables (`vars` context) — repo < org < environment
    /// merged at claim. Plaintext config, flows to untrusted PR runs too
    /// (variables aren't secrets).
    vars: Option<std::collections::HashMap<String, String>>,
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
    /// Per-leg strategy metadata (`action_jobs.strategy_json`) → `strategy`
    /// context — `{"job-index","job-total","fail-fast","max-parallel"}`.
    strategy: Option<serde_json::Value>,
    /// `needs.<id>.{result,outputs}` for dependents — `None` for need-less
    /// jobs.
    needs: Option<serde_json::Value>,
    /// Seconds until `timeout_at`, computed at claim — runners don't parse
    /// the timestamp or depend on wall-clock agreement with the API.
    timeout_seconds: Option<i64>,
    /// `jobs.<id>.container` spec (evaluated) — the runner creates one
    /// long-lived job container and `docker exec`s steps into it.
    container: Option<serde_json::Value>,
    /// `jobs.<id>.services` map — sibling containers on the job network.
    services: Option<serde_json::Value>,
    /// `jobs.<id>.environment` (resolved name) — surfaces in the runner's
    /// deployment context.
    environment: Option<String>,
    /// `jobs.<id>.environment.url` (resolved) — the `environment.url`
    /// expression context + deployment record.
    environment_url: Option<String>,
    /// `ACTIONS_ID_TOKEN_REQUEST_URL` — set when `id-token: write` is granted.
    id_token_request_url: Option<String>,
    /// `ACTIONS_ID_TOKEN_REQUEST_TOKEN` — bound to this job's token row.
    id_token_request_token: Option<String>,
    /// `ACTIONS_CACHE_URL` — `_apis/artifactcache` service root for this job.
    cache_url: Option<String>,
    /// `ACTIONS_RUNTIME_TOKEN` — per-job bearer for the cache endpoints.
    cache_token: Option<String>,
    /// `GITHUB_REF_PROTECTED` — whether a branch/tag protection rule matched
    /// `head_ref` at claim (same evaluation the OIDC `ref_protected` claim
    /// uses).
    ref_protected: Option<bool>,
    /// `GITHUB_RETENTION_DAYS` — the instance's actions log/artifact retention
    /// window (`OXIDEAN_ACTIONS_LOG_RETENTION_DAYS`, default 90).
    retention_days: Option<i64>,
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
            vars: None,
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
            strategy: None,
            needs: None,
            timeout_seconds: None,
            container: None,
            services: None,
            environment: None,
            environment_url: None,
            id_token_request_url: None,
            id_token_request_token: None,
            cache_url: None,
            cache_token: None,
            ref_protected: None,
            retention_days: None,
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
    // Long-poll: hold empty responses up to `wait_ms` (cap 30s), waking on
    // enqueue/approval/promotion via `task_wake` plus a 2s fallback tick —
    // bounded and stateless (no sticky runner sessions; any tick re-probes).
    let wait = req.wait_ms.unwrap_or(0).min(30_000);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait);
    let job = loop {
        // Register the waiter BEFORE the claim probe — a wake between probe
        // and await otherwise goes missing.
        let waiter = crate::actions::task_wake::waiter();
        let notified = waiter.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let job = state
            .db
            .claim_queued_action_job_for_labels(&runner.id, &labels)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if job.is_some() || wait == 0 || std::time::Instant::now() >= deadline {
            break job;
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        tokio::select! {
            _ = notified => {}
            _ = tokio::time::sleep(remaining.min(std::time::Duration::from_secs(2))) => {}
        }
    };
    Ok(Json(match job {
        Some(j) => {
            let runs_on: Vec<String> = serde_json::from_str(&j.runs_on_json).unwrap_or_default();
            if let Err(e) = state.db.recompute_action_run_status(&j.run_id).await {
                tracing::warn!("action run rollup after claim failed: {e}");
            }
            crate::webhook::dispatch::notify_workflow_job(&state.db, &j.id, "in_progress").await;
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
            // The frozen scope map drives the minted token's perms_json +
            // OIDC gating; `contents_permission` remains as the legacy
            // fallback for payloads enqueued before Phase 7.
            let mut permissions =
                crate::actions::job_tokens::permissions_from_payload(&payload_json);
            if !pr_ctx.trusted {
                crate::actions::job_tokens::downgrade_write_grants(&mut permissions);
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
            // Claim-time secrets: repo < org < environment merge + decrypt
            // audit (Phase 8). The trusted-PR gate still decides whether ANY
            // secrets leave the server — untrusted fork PRs get nothing.
            let secrets = if pr_ctx.trusted {
                crate::actions::secrets::decrypted_secrets_for_job(
                    &state.db,
                    &run.repository_id,
                    j.environment_name.as_deref(),
                    Some(crate::actions::secrets::SecretAudit {
                        actor_username: if actor.is_empty() {
                            "oxidean-actions"
                        } else {
                            &actor
                        },
                        actor_id: if actor_id.is_empty() {
                            None
                        } else {
                            Some(actor_id.as_str())
                        },
                        repository_id: &run.repository_id,
                        run_id: &run.id,
                        job_id: &j.id,
                    }),
                )
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            } else {
                std::collections::HashMap::new()
            };
            // Configuration variables — repo < org < environment merge.
            // Plaintext (not secrets): they flow to untrusted PR runs just
            // like GitHub exposes `vars.*` there. Failure degrades to an
            // empty map rather than failing the claim.
            let mut vars = crate::actions::vars::variables_for_job(
                &state.db,
                &run.repository_id,
                j.environment_name.as_deref(),
            )
            .await
            .unwrap_or_default();
            crate::actions::vars::inject_debug_logging(&mut vars, j.debug_logging);
            let base_ref = pr_ctx.base_ref.clone();
            let pull_number = pr_ctx.pull_number;

            // Per-job OXIDEAN_TOKEN — minted at claim against the frozen
            // scope map so the grant reflects the workflow's permissions at
            // head_sha (and `id-token: write` mints the OIDC request pair).
            let (job_token, job_token_permission, id_token_request_token) =
                match crate::actions::job_tokens::mint_job_token(
                    &state.db,
                    &run,
                    &repo,
                    &owner_ref,
                    &j.id,
                    &permissions,
                )
                .await
                {
                    Ok(Some(minted)) => (
                        minted.plaintext,
                        minted.contents.as_str().to_string(),
                        minted.oidc_request_token,
                    ),
                    Ok(None) => (None, String::new(), None),
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
            // `ACTIONS_ID_TOKEN_REQUEST_URL` — minted only alongside the
            // request token; the endpoint bound the hash at mint.
            let id_token_request_url = id_token_request_token
                .as_ref()
                .map(|_| format!("{server_url}{}", crate::actions::oidc::TOKEN_REQUEST_PATH));

            // Per-job `ACTIONS_RUNTIME_TOKEN` for the `actions/cache` service —
            // failures degrade to "cache unavailable" rather than failing the
            // claim (the action treats missing env as a skip).
            let cache_token =
                crate::actions::cache::mint_cache_token(&state, &j.id, &run.repository_id).await;
            let cache_url = cache_token
                .as_ref()
                .map(|_| format!("{server_url}/api/actions/_apis/artifactcache"));

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
            // `needs.<id>.{result,outputs}` for dependents — computed before
            // the response literal moves `j.run_id`/`j.needs_json`. Scoped
            // to the caller group so callee-internal needs resolve.
            let needs = {
                let all = state
                    .db
                    .list_action_jobs_for_run(&j.run_id)
                    .await
                    .unwrap_or_default();
                crate::actions::scheduler::needs_context_for_task(
                    &all,
                    &j.needs_json,
                    j.caller_job_id.as_str(),
                )
            };
            // Reusable-workflow callee legs (P13): `call_inputs` patches the
            // frozen event payload's `inputs` member — `github.event.inputs`
            // and the runner's `inputs` context both resolve from it.
            let event_payload = {
                let base: serde_json::Value = run
                    .event_payload_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_else(|| serde_json::json!({}));
                match payload_json.get("call_inputs") {
                    Some(inputs) if !inputs.is_null() => {
                        crate::actions::reusable::patch_event_inputs(&base, inputs)
                    }
                    _ => base,
                }
            };
            // `call_secrets` remaps the caller scope: `inherit` passes
            // through, a map forwards each callee name from the evaluated
            // caller-scope expr, absent = the callee sees no secrets.
            let secrets = if !j.caller_job_id.is_empty() {
                let spec = payload_json
                    .get("call_secrets")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let scope: serde_json::Map<String, serde_json::Value> = secrets
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                crate::actions::reusable::remap_call_secrets(&scope, &spec)
                    .into_iter()
                    .map(|(k, v)| (k, v.as_str().unwrap_or_default().to_string()))
                    .collect()
            } else {
                secrets
            };
            // Job timeout clock starts at claim (GitHub semantics), not at
            // enqueue — a needs-gated or held job's `timeout-minutes` must
            // not tick while it waits, and a rerun's cleared `timeout_at`
            // gets restamped from the payload's evaluated duration.
            let timeout_seconds = payload_json
                .get("timeout_seconds")
                .and_then(|v| v.as_f64())
                .map(|s| s as i64)
                .or_else(|| {
                    // Payloads enqueued before the field existed — fall back
                    // to the enqueue-stamped row deadline.
                    j.timeout_at.as_deref().and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(s).ok().map(|t| {
                            t.signed_duration_since(chrono::Utc::now())
                                .num_seconds()
                                .max(0)
                        })
                    })
                });
            let mut timeout_at = j.timeout_at.clone();
            if let Some(secs) = timeout_seconds {
                let fresh_deadline = (chrono::Utc::now() + chrono::Duration::seconds(secs))
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                if let Err(e) = state
                    .db
                    .set_action_job_timeout_at(&j.id, Some(&fresh_deadline))
                    .await
                {
                    tracing::warn!(error = %e, job_id = %j.id, "claim-time timeout restamp failed");
                } else {
                    timeout_at = Some(fresh_deadline);
                }
            }
            // `GITHUB_REF_PROTECTED` — same branch/tag rule evaluation the
            // OIDC `ref_protected` claim uses. Failure degrades to `false`
            // (unprotected), never fails the claim.
            let ref_protected = if let Some(tag) = crate::protection::tag_from_ref(&run.head_ref) {
                crate::protection::effective_for_tag(&state.db, &repo.id, tag)
                    .await
                    .map(|e| e.matched)
                    .unwrap_or(false)
            } else if let Some(branch) = crate::protection::branch_from_ref(&run.head_ref) {
                crate::protection::effective_for_branch(&state.db, &repo.id, branch)
                    .await
                    .map(|e| e.matched)
                    .unwrap_or(false)
            } else {
                false
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
                vars: Some(vars),
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
                timeout_at,
                event_payload: Some(event_payload),
                matrix: Some(
                    serde_json::from_str(&j.matrix_json).unwrap_or_else(|_| serde_json::json!({})),
                ),
                strategy: Some(
                    serde_json::from_str(&j.strategy_json)
                        .unwrap_or_else(|_| serde_json::json!({})),
                ),
                needs,
                container: payload_json.get("container").cloned(),
                services: payload_json.get("services").cloned(),
                environment: payload_json
                    .get("environment")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                environment_url: payload_json
                    .get("environment_url")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                id_token_request_url,
                id_token_request_token,
                cache_url,
                cache_token,
                ref_protected: Some(ref_protected),
                retention_days: Some(
                    crate::actions::sweeps::log_retention_days().min(i64::MAX as u64) as i64,
                ),
                timeout_seconds,
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
pub(crate) struct PullRunContext {
    pub base_ref: String,
    pub pull_number: i64,
    pub trusted: bool,
}

pub(crate) async fn pull_context_for_run(
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
    /// `steps.<id>.outputs` collected from `$GITHUB_OUTPUT` file commands —
    /// reported on terminal states so `jobs.<id>.outputs` can materialize.
    #[serde(default)]
    outputs: Option<serde_json::Value>,
    /// `$GITHUB_STEP_SUMMARY` markdown — reported once at terminal.
    #[serde(default)]
    summary: Option<String>,
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
    // Runner contact — liveness heartbeat for the stale-claim sweeper.
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let _ = state.db.touch_action_job_heartbeat(&job.id, &at).await;
    // API-04: capture the pre-update run status so the queued→in_progress
    // transition emits `workflow_run` `in_progress` exactly once.
    let prev_run_status = state
        .db
        .find_action_run_by_id(&job.run_id)
        .await
        .ok()
        .flatten()
        .map(|r| r.status);
    // Guarded transition — a job the server already cancelled (concurrency
    // supersede, fail-fast sibling, run cancel) keeps its terminal state
    // even when a runner reports success afterwards.
    let landed = state
        .db
        .transition_action_job_status(&job.id, status)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Materialize `jobs.<id>.outputs` from reported step outputs so
    // `needs.<id>.outputs.*` resolves for dependents (terminal states only).
    if matches!(status, "success" | "failure" | "cancelled") {
        if let Some(outputs) = &req.outputs {
            if let Err(e) =
                crate::actions::scheduler::materialize_outputs(&state.db, &job, outputs).await
            {
                tracing::warn!(error = %e, job_id = %job.id, "job outputs materialization failed");
            }
        }
    }
    let cancelled = finalize_task_update(
        &state,
        &job,
        prev_run_status,
        status,
        landed,
        req.summary.as_deref(),
    )
    .await;
    Ok(Json(OkResponse {
        ok: true,
        cancelled,
        log_index: 0,
    }))
}

/// Shared post-transition tail for both runner protocols — token revoke,
/// summary persist, scheduler wakeup, run rollup, completion notice,
/// `workflow_run` webhook, commit-status publish. Returns whether the job
/// was already terminal `cancelled` server-side (the runner should abort).
pub(crate) async fn finalize_task_update(
    state: &AppState,
    job: &oxidean_db::ActionJobRow,
    prev_run_status: Option<String>,
    status: &str,
    landed: bool,
    summary: Option<&str>,
) -> bool {
    // Terminal state revokes the per-job OXIDEAN_TOKEN (idempotent).
    if matches!(status, "success" | "failure" | "cancelled") {
        if let Err(e) = state.db.revoke_action_job_token_for_job(&job.id).await {
            tracing::warn!(error = %e, job_id = %job.id, "failed to revoke job token");
        }
        // `$GITHUB_STEP_SUMMARY` → `action_jobs.summary_md` for the run UI.
        if let Some(summary) = summary.filter(|s| !s.trim().is_empty()) {
            if let Err(e) = state.db.set_action_job_summary(&job.id, summary).await {
                tracing::warn!(error = %e, job_id = %job.id, "job summary persist failed");
            }
        }
        // Flip `waiting` jobs whose needs just finished. Boxed — this runs
        // inside the runner-proto handler chain on small test stacks.
        if let Err(e) = Box::pin(crate::actions::scheduler::recompute_ready_jobs(
            &state.db,
            &job.run_id,
        ))
        .await
        {
            tracing::warn!(error = %e, run_id = %job.run_id, "ready-job recompute failed");
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
            // `on: workflow_run` — the same once-per-run claim gates the
            // downstream fan-out, so a run fans out exactly once per attempt
            // lifetime (requeue clears the flag and may fan out again).
            crate::actions::events::notify_workflow_run_actions(
                &state.db,
                state.git.as_ref(),
                &state.repos_dir,
                &run,
                &state.env_name,
            )
            .await;
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
    // If the write didn't land the job was already terminal — tell the
    // runner when that's a cancel so it aborts instead of continuing.
    if landed {
        false
    } else {
        state
            .db
            .find_action_job_by_id(&job.id)
            .await
            .ok()
            .flatten()
            .is_some_and(|j| j.status == "cancelled")
    }
}

#[derive(Debug, Deserialize)]
struct UpdateLogRequest {
    run_id: String,
    job_id: String,
    chunk: String,
    runner_token: Option<String>,
    /// Monotonic batch cursor (Phase 8) — the server stores `log_index` as
    /// the next expected batch. `None` = legacy client → append unordered.
    #[serde(default)]
    log_index: Option<i64>,
}

/// Cap per-request log appends so a stolen runner token cannot fill the disk in one call.
const MAX_LOG_CHUNK_BYTES: usize = 256 * 1024;

/// Per-job total log cap — `OXIDEAN_ACTIONS_MAX_LOG_BYTES` (default 64 MiB).
/// Over-cap appends land a truncation marker once, then drop; a stolen token
/// can no longer fill the disk one 256 KiB chunk at a time.
pub(crate) fn max_job_log_bytes() -> usize {
    std::env::var("OXIDEAN_ACTIONS_MAX_LOG_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(64 * 1024 * 1024)
}

/// Per-job bounded append — over-cap writes land a truncation marker and
/// report OK so the runner keeps running (the log itself records the loss).
async fn append_job_log_capped(
    state: &AppState,
    job: &oxidean_db::ActionJobRow,
    chunk: &[u8],
) -> Result<bool, StatusCode> {
    // `##[ann]` markers carry structured annotations out-of-band — strip
    // them from the stored text and persist rows on successful append.
    let (text, anns) = crate::actions::logs::extract_annotations(chunk);
    let appended = crate::actions::logs::append_job_log_bounded(
        &state.actions_log_dir,
        &job.run_id,
        &job.id,
        &text,
        max_job_log_bytes() as u64,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if appended && !anns.is_empty() {
        crate::actions::logs::persist_annotations(&state.db, &job.run_id, &job.id, anns).await;
    }
    Ok(appended)
}

async fn update_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UpdateLogRequest>,
) -> Result<Response, StatusCode> {
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
    // Ordering CAS — a replayed batch drops idempotently; a skipped index is
    // a real gap the runner must resend from its last ack (409 carries the
    // server's next expected index).
    let mut skip_append = false;
    if let Some(index) = req.log_index {
        match state
            .db
            .advance_action_job_log_index(&job.id, index)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        {
            oxidean_db::LogIndexCas::Accept => {}
            oxidean_db::LogIndexCas::Duplicate => skip_append = true,
            oxidean_db::LogIndexCas::Gap => {
                let cursor = state.db.action_job_log_index(&job.id).await.unwrap_or(0);
                return Ok((
                    StatusCode::CONFLICT,
                    Json(OkResponse {
                        ok: false,
                        cancelled: false,
                        log_index: cursor,
                    }),
                )
                    .into_response());
            }
        }
    }
    if !skip_append {
        let appended = append_job_log_capped(&state, &job, req.chunk.as_bytes()).await?;
        if !appended {
            // Total per-job cap hit — the CAS already consumed this index so
            // a retry lands Duplicate, but signal back-pressure so a
            // flooding runner slows down instead of hammering.
            let cursor = state.db.action_job_log_index(&job.id).await.unwrap_or(0);
            return Ok((
                StatusCode::TOO_MANY_REQUESTS,
                [(axum::http::header::RETRY_AFTER, "5")],
                Json(OkResponse {
                    ok: false,
                    cancelled: false,
                    log_index: cursor,
                }),
            )
                .into_response());
        }
    }
    // Every runner contact bumps the liveness heartbeat for the stale-claim
    // sweeper — best-effort, never fails the request.
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let _ = state.db.touch_action_job_heartbeat(&job.id, &at).await;
    // Log traffic is the runner's most frequent contact — piggyback the
    // cancel signal so a superseded/fail-fast job aborts mid-run rather
    // than executing its remaining steps.
    let cancelled = state
        .db
        .find_action_job_by_id(&job.id)
        .await
        .ok()
        .flatten()
        .is_some_and(|j| j.status == "cancelled");
    let cursor = state.db.action_job_log_index(&job.id).await.unwrap_or(0);
    Ok((
        StatusCode::OK,
        Json(OkResponse {
            ok: true,
            cancelled,
            log_index: cursor,
        }),
    )
        .into_response())
}
