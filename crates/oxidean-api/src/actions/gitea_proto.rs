//! Gitea `runner.v1` compatibility surface — stock `act_runner` connects over
//! ConnectRPC (unary POST, `application/json` or `application/proto` bodies)
//! at `/api/actions/runner.v1.RunnerService/{Register,Declare,FetchTask,
//! UpdateTask,UpdateLog}` (Phase 5). Message types come from the vendored
//! `proto/runner/v1/*.proto` (gitea.com/gitea/actions-proto-def).
//!
//! Mapping onto our execution model:
//! - Register → `action_runners` row + `ort_` bearer (same as `/register`).
//! - FetchTask → the shared atomic label-aware claim (`fetch_task`'s path)
//!   plus a `action_task_claims` row minting the never-reused numeric
//!   `task_id`; the `Task` carries a single-job workflow YAML (raw job
//!   subtree captured in `action_payloads.job_yaml` at enqueue, with
//!   `strategy.matrix` narrowed to the claimed leg).
//! - UpdateTask/UpdateLog resolve `task_id` → `(job_id, runner_id)` then run
//!   the same guarded transitions, output materialization, and rollback path
//!   as `update_task`/`update_log`.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use prost::Message;
use uuid::Uuid;

use crate::actions::runner_proto::{
    finalize_task_update, pull_context_for_run, runner_from_headers,
};
use crate::actions::tokens::{accept_registration_token, hash_token};
use crate::app::AppState;

/// Cap a gitea-protocol log batch the same way `/update_log` caps chunks.
const MAX_GITEA_LOG_BYTES: usize = 256 * 1024;

/// Generated Gitea `runner.v1` types (see `proto/runner/v1/*.proto`).
pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/runner.v1.rs"));
}

/// Generated `ping.v1` types — act_runner pings on register/daemon startup.
pub mod ping_pb {
    include!(concat!(env!("OUT_DIR"), "/ping.v1.rs"));
}

// --- Connect codec ---------------------------------------------------------

/// Response encoding, picked from the request Content-Type.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wire {
    Proto,
    Json,
}

fn wire(headers: &HeaderMap) -> Wire {
    match headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
    {
        ct if ct.starts_with("application/proto")
            || ct.starts_with("application/connect+proto") =>
        {
            Wire::Proto
        }
        _ => Wire::Json,
    }
}

/// Connect-style error body (`{"code","message"}` JSON, matching status).
fn connect_err(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        axum::Json(serde_json::json!({"code": code, "message": message})),
    )
        .into_response()
}

fn internal_err(e: impl std::fmt::Display) -> Response {
    connect_err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        &e.to_string(),
    )
}

fn unauthorized(msg: &str) -> Response {
    connect_err(StatusCode::UNAUTHORIZED, "unauthenticated", msg)
}

fn not_found(msg: &str) -> Response {
    connect_err(StatusCode::NOT_FOUND, "not_found", msg)
}

fn forbidden(msg: &str) -> Response {
    connect_err(StatusCode::FORBIDDEN, "permission_denied", msg)
}

fn respond<T: Message>(w: Wire, msg: &T, json: serde_json::Value) -> Response {
    match w {
        Wire::Proto => ([(CONTENT_TYPE, "application/proto")], msg.encode_to_vec()).into_response(),
        Wire::Json => (
            [(CONTENT_TYPE, "application/json")],
            serde_json::to_vec(&json).unwrap_or_default(),
        )
            .into_response(),
    }
}

// --- JSON decode helpers (proto3 JSON: camelCase preferred, snake accepted) --

fn json_str(v: &serde_json::Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .unwrap_or_default()
        .to_string()
}

fn json_i64(v: &serde_json::Value, keys: &[&str]) -> i64 {
    for k in keys {
        if let Some(x) = v.get(*k) {
            if let Some(n) = x.as_i64() {
                return n;
            }
            if let Some(s) = x.as_str().and_then(|s| s.parse::<i64>().ok()) {
                return s;
            }
        }
    }
    0
}

fn json_bool(v: &serde_json::Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_bool()))
        .unwrap_or(false)
}

fn json_strs(v: &serde_json::Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_array()))
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn json_str_map(v: &serde_json::Value, key: &str) -> HashMap<String, String> {
    v.get(key)
        .and_then(|x| x.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, x)| x.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Decode the request body — proto binary when the caller asked for it, else
/// hand-rolled proto3-JSON extraction (few scalar fields per message).
#[allow(clippy::result_large_err)] // Response is the deliberate early-return mechanism
fn decode_json(body: &Bytes) -> Result<serde_json::Value, Response> {
    serde_json::from_slice(body).map_err(|e| {
        connect_err(
            StatusCode::BAD_REQUEST,
            "invalid_argument",
            &format!("invalid JSON body: {e}"),
        )
    })
}

// --- serde_json ↔ prost_types::Struct --------------------------------------

fn json_to_prost(v: &serde_json::Value) -> prost_types::Value {
    use prost_types::value::Kind;
    prost_types::Value {
        kind: Some(match v {
            serde_json::Value::Null => Kind::NullValue(0),
            serde_json::Value::Bool(b) => Kind::BoolValue(*b),
            serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or_default()),
            serde_json::Value::String(s) => Kind::StringValue(s.clone()),
            serde_json::Value::Array(a) => Kind::ListValue(prost_types::ListValue {
                values: a.iter().map(json_to_prost).collect(),
            }),
            serde_json::Value::Object(m) => Kind::StructValue(prost_types::Struct {
                fields: m
                    .iter()
                    .map(|(k, x)| (k.clone(), json_to_prost(x)))
                    .collect(),
            }),
        }),
    }
}

fn json_struct(v: &serde_json::Value) -> Option<prost_types::Struct> {
    match v {
        serde_json::Value::Object(m) => Some(prost_types::Struct {
            fields: m
                .iter()
                .map(|(k, x)| (k.clone(), json_to_prost(x)))
                .collect(),
        }),
        _ => None,
    }
}

// --- result/status mapping -------------------------------------------------

fn status_for_result(r: i32) -> &'static str {
    match pb::Result::try_from(r).unwrap_or(pb::Result::Unspecified) {
        pb::Result::Unspecified => "in_progress",
        pb::Result::Success => "success",
        pb::Result::Failure => "failure",
        pb::Result::Cancelled => "cancelled",
        pb::Result::Skipped => "skipped",
    }
}

fn result_for_status(s: &str) -> i32 {
    (match s {
        "success" => pb::Result::Success,
        "failure" => pb::Result::Failure,
        "cancelled" => pb::Result::Cancelled,
        "skipped" => pb::Result::Skipped,
        _ => pb::Result::Unspecified,
    }) as i32
}

fn runner_i64_id(uuid_str: &str) -> i64 {
    Uuid::parse_str(uuid_str)
        .map(|u| (u.as_u128() as u64) as i64)
        .unwrap_or_default()
}

#[allow(deprecated)] // `agent_labels`/`custom_labels` exist for older runners
fn runner_pb(r: &oxidean_db::ActionRunnerRow, token: &str, version: &str) -> pb::Runner {
    pb::Runner {
        id: runner_i64_id(&r.id),
        uuid: r.id.clone(),
        token: token.into(),
        name: r.name.clone(),
        status: pb::RunnerStatus::Active as i32,
        agent_labels: vec![],
        custom_labels: vec![],
        version: version.into(),
        labels: serde_json::from_str(&r.labels_json).unwrap_or_default(),
        ephemeral: r.ephemeral,
    }
}

/// Stock `act_runner` authenticates every non-Register call with
/// `x-runner-uuid` + `x-runner-token` (see its connect interceptor), not a
/// bearer header. Accept that pair first, then fall back to our standard
/// `Authorization: Bearer ort_…` so both client styles work.
async fn gitea_runner_from_headers(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<oxidean_db::ActionRunnerRow, StatusCode> {
    if let (Some(uuid), Some(token)) = (
        headers.get("x-runner-uuid").and_then(|v| v.to_str().ok()),
        headers.get("x-runner-token").and_then(|v| v.to_str().ok()),
    ) {
        let runner = state
            .db
            .find_action_runner_by_id(uuid)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if !token.is_empty() && runner.token_hash == hash_token(token) {
            if let Err(e) = state.db.touch_action_runner_online(&runner.id).await {
                tracing::warn!(error = %e, runner_id = %runner.id, "failed to bump runner last_online");
            }
            return Ok(runner);
        }
        return Err(StatusCode::UNAUTHORIZED);
    }
    runner_from_headers(state, headers, None).await
}

// --- handlers ---------------------------------------------------------------

/// `ping.v1.PingService/Ping` — connectivity check act_runner runs before
/// register and on daemon start. Echoes the request payload back.
pub async fn ping(headers: HeaderMap, body: Bytes) -> Response {
    let w = wire(&headers);
    let data = match w {
        Wire::Proto => match ping_pb::PingRequest::decode(body) {
            Ok(r) => r.data,
            Err(e) => return internal_err(e),
        },
        Wire::Json => match decode_json(&body) {
            Ok(v) => json_str(&v, &["data"]),
            Err(r) => return r,
        },
    };
    respond(
        w,
        &ping_pb::PingResponse { data: data.clone() },
        serde_json::json!({"data": data}),
    )
}

#[allow(deprecated)] // `agent_labels` is the legacy labels field
pub async fn register(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let w = wire(&headers);
    let req = match w {
        Wire::Proto => match pb::RegisterRequest::decode(body) {
            Ok(r) => r,
            Err(e) => return internal_err(e),
        },
        Wire::Json => match decode_json(&body) {
            Ok(v) => pb::RegisterRequest {
                name: json_str(&v, &["name"]),
                token: json_str(&v, &["token"]),
                agent_labels: vec![],
                custom_labels: vec![],
                version: json_str(&v, &["version"]),
                labels: json_strs(&v, &["labels"]),
                ephemeral: json_bool(&v, &["ephemeral"]),
                capabilities: json_strs(&v, &["capabilities"]),
            },
            Err(r) => return r,
        },
    };
    if req.name.is_empty() || req.token.is_empty() {
        return connect_err(
            StatusCode::BAD_REQUEST,
            "invalid_argument",
            "missing runner token, name",
        );
    }
    match accept_registration_token(&state.db, &req.token).await {
        Ok(true) => {}
        Ok(false) => return unauthorized("runner registration token not found"),
        Err(e) => return internal_err(e),
    }
    let runner_token = format!("ort_{}", Uuid::new_v4());
    let runner_hash = hash_token(&runner_token);
    let labels = if req.labels.is_empty() {
        // Deprecated fields — older runners still send them.
        serde_json::to_string(&req.agent_labels).unwrap_or_else(|_| "[]".into())
    } else {
        serde_json::to_string(&req.labels).unwrap_or_else(|_| "[]".into())
    };
    let id = Uuid::new_v4().to_string();
    if let Err(e) = state
        .db
        .insert_action_runner(&id, &req.name, &runner_hash, &labels, None, req.ephemeral)
        .await
    {
        return internal_err(e);
    }
    let row = state
        .db
        .find_action_runner_by_id(&id)
        .await
        .ok()
        .flatten()
        .unwrap_or(oxidean_db::ActionRunnerRow {
            id: id.clone(),
            name: req.name.clone(),
            token_hash: String::new(),
            labels_json: labels.clone(),
            owner_type: None,
            owner_id: None,
            repository_id: None,
            ephemeral: req.ephemeral,
            last_online: None,
            created_at: String::new(),
            updated_at: String::new(),
        });
    respond(
        w,
        &pb::RegisterResponse {
            runner: Some(runner_pb(&row, &runner_token, &req.version)),
        },
        serde_json::json!({
            "runner": {
                "id": runner_i64_id(&id),
                "uuid": id,
                "token": runner_token,
                "name": req.name,
                "status": 2,
                "version": req.version,
                "labels": req.labels,
                "ephemeral": req.ephemeral,
            }
        }),
    )
}

pub async fn declare(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let w = wire(&headers);
    let runner = match gitea_runner_from_headers(&state, &headers).await {
        Ok(r) => r,
        Err(_) => return unauthorized("unknown runner token"),
    };
    let req = match w {
        Wire::Proto => match pb::DeclareRequest::decode(body) {
            Ok(r) => r,
            Err(e) => return internal_err(e),
        },
        Wire::Json => match decode_json(&body) {
            Ok(v) => pb::DeclareRequest {
                version: json_str(&v, &["version"]),
                labels: json_strs(&v, &["labels"]),
                capabilities: json_strs(&v, &["capabilities"]),
            },
            Err(r) => return r,
        },
    };
    let labels = serde_json::to_string(&req.labels).unwrap_or_else(|_| "[]".into());
    if let Err(e) = state
        .db
        .update_action_runner_labels(&runner.id, &labels)
        .await
    {
        return internal_err(e);
    }
    let mut runner = runner;
    runner.labels_json = labels;
    let mut resp = respond(
        w,
        &pb::DeclareResponse {
            runner: Some(runner_pb(&runner, "", &req.version)),
        },
        serde_json::json!({
            "runner": {
                "id": runner_i64_id(&runner.id),
                "uuid": runner.id,
                "name": runner.name,
                "status": 2,
                "version": req.version,
                "labels": req.labels,
            }
        }),
    );
    // Server capability advertisement — runners feature-negotiate off this.
    if let Ok(v) = HeaderValue::from_str("cancelling") {
        resp.headers_mut().insert("X-Gitea-Actions-Capabilities", v);
    }
    resp
}

/// The claimed job's materialized execution context — shared shape between
/// `fetch_task` (JSON protocol) and `runner.v1` FetchTask.
pub(crate) struct ClaimContext {
    pub repo_name: String,
    pub owner_slug: String,
    pub actor: String,
    pub base_ref: String,
    pub pull_number: i64,
    pub secrets: HashMap<String, String>,
    /// Configuration variables — repo < org < environment plaintext merge.
    /// Not secrets: flows regardless of PR trust (GitHub exposes `vars.*`
    /// to untrusted workflows too).
    pub vars: HashMap<String, String>,
    pub job_token: Option<String>,
    /// OIDC request credential pair — `ACTIONS_ID_TOKEN_REQUEST_*` (Some
    /// only when `id-token: write` survived provenance gating).
    pub id_token_request_url: Option<String>,
    pub id_token_request_token: Option<String>,
    /// `ACTIONS_CACHE_URL` / `ACTIONS_RUNTIME_TOKEN` pair — act_runner maps
    /// `gitea_runtime_token` onto the runtime token env for actions/cache.
    pub cache_url: Option<String>,
    pub cache_token: Option<String>,
    pub server_url: String,
    pub workflow_ref: String,
    /// Whether a branch/tag protection rule matched `head_ref` — feeds the
    /// `github.ref_protected` context (same evaluation as the OIDC claim).
    pub ref_protected: bool,
    /// `github.retention_days` — the instance actions retention window.
    pub retention_days: i64,
}

/// Resolve repo/actor/secrets/token for a claimed job — the security-bearing
/// half of `fetch_task` (PR provenance gates secrets + token grant).
pub(crate) async fn claim_context(
    state: &AppState,
    j: &oxidean_db::ActionJobRow,
    run: &oxidean_db::ActionRunRow,
    permissions: std::collections::BTreeMap<String, crate::actions::ContentsPermission>,
) -> Result<ClaimContext, StatusCode> {
    let pr_ctx = pull_context_for_run(&state.db, run).await;
    let mut permissions = permissions;
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
    let actor = match run.triggered_by.as_deref() {
        Some(uid) => state
            .db
            .find_user_by_id(uid)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .map(|u| u.username)
            .unwrap_or_default(),
        None => String::new(),
    };
    // Claim-time secrets: repo < org < environment merge + decrypt audit
    // (Phase 8); untrusted fork PRs still get an empty map.
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
                actor_id: run.triggered_by.as_deref(),
                repository_id: &run.repository_id,
                run_id: &run.id,
                job_id: &j.id,
            }),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        HashMap::new()
    };
    // Variables — plaintext config (not secrets): no trust gate.
    let mut vars = crate::actions::vars::variables_for_job(
        &state.db,
        &run.repository_id,
        j.environment_name.as_deref(),
    )
    .await
    .unwrap_or_default();
    crate::actions::vars::inject_debug_logging(&mut vars, j.debug_logging);
    let minted = match crate::actions::job_tokens::mint_job_token(
        &state.db,
        run,
        &repo,
        &owner_ref,
        &j.id,
        &permissions,
    )
    .await
    {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(error = %e, job_id = %j.id, "mint_job_token failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let owner_slug = owner_ref.slug().to_string();
    let workflow_ref = format!(
        "{}/{}/{}@{}",
        owner_slug, repo.name, run.workflow_path, run.head_ref
    );
    let server_url = crate::public_origin::resolve_public_origin();
    let (job_token, id_token_request_token) = match minted {
        Some(m) => (m.plaintext, m.oidc_request_token),
        None => (None, None),
    };
    let id_token_request_url = id_token_request_token
        .as_ref()
        .map(|_| format!("{server_url}{}", crate::actions::oidc::TOKEN_REQUEST_PATH));
    // Per-job `ACTIONS_RUNTIME_TOKEN` for the `actions/cache` service —
    // failure degrades to "cache unavailable" rather than failing the claim.
    let cache_token =
        crate::actions::cache::mint_cache_token(state, &j.id, &run.repository_id).await;
    let cache_url = cache_token
        .as_ref()
        .map(|_| format!("{server_url}/api/actions/_apis/artifactcache"));
    // `github.ref_protected` — same rule evaluation as the OIDC claim;
    // failure degrades to `false`, never fails the claim.
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
    Ok(ClaimContext {
        repo_name: repo.name.clone(),
        owner_slug,
        actor,
        base_ref: pr_ctx.base_ref,
        pull_number: pr_ctx.pull_number,
        secrets,
        vars,
        job_token,
        id_token_request_url,
        id_token_request_token,
        cache_url,
        cache_token,
        server_url,
        workflow_ref,
        ref_protected,
        retention_days: crate::actions::sweeps::log_retention_days().min(i64::MAX as u64) as i64,
    })
}

/// Assemble the single-job workflow YAML act_runner executes — the raw job
/// subtree captured at enqueue with `strategy.matrix` narrowed to this leg
/// so the runner's own matrix expansion resolves exactly our assignment.
fn build_workflow_payload(
    payload_json: &serde_json::Value,
    job: &oxidean_db::ActionJobRow,
    event: &str,
    env_extra: &std::collections::BTreeMap<String, String>,
) -> Option<Vec<u8>> {
    let job_yaml = payload_json.get("job_yaml")?.as_str()?;
    let mut job_v: serde_yaml::Value = serde_yaml::from_str(job_yaml).ok()?;
    // `ACTIONS_ID_TOKEN_REQUEST_*` — GitHub injects these into the job env;
    // the Task wire has no env surface, so they ride the job's `env:` map.
    if !env_extra.is_empty() {
        if let serde_yaml::Value::Mapping(m) = &mut job_v {
            let env_key = serde_yaml::Value::String("env".into());
            if !matches!(m.get(&env_key), Some(serde_yaml::Value::Mapping(_))) {
                m.insert(
                    env_key.clone(),
                    serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
                );
            }
            if let Some(serde_yaml::Value::Mapping(env)) = m.get_mut(&env_key) {
                for (k, v) in env_extra {
                    env.insert(
                        serde_yaml::Value::String(k.clone()),
                        serde_yaml::Value::String(v.clone()),
                    );
                }
            }
        }
    }
    let assignment: serde_json::Value =
        serde_json::from_str(&job.matrix_json).unwrap_or_else(|_| serde_json::json!({}));
    if let (serde_yaml::Value::Mapping(m), Some(assign)) =
        (&mut job_v, assignment.as_object().filter(|o| !o.is_empty()))
    {
        // Narrow `strategy` to exactly this leg — act_runner re-expands
        // matrix itself, so each key becomes a single-element list and
        // fail-fast/max-parallel/include/exclude no longer apply.
        let mut mat = serde_yaml::Mapping::new();
        for (k, v) in assign {
            mat.insert(
                serde_yaml::Value::String(k.clone()),
                serde_yaml::Value::Sequence(vec![serde_yaml::to_value(v).ok()?]),
            );
        }
        let mut strat = serde_yaml::Mapping::new();
        strat.insert(
            serde_yaml::Value::String("matrix".into()),
            serde_yaml::Value::Mapping(mat),
        );
        m.insert(
            serde_yaml::Value::String("strategy".into()),
            serde_yaml::Value::Mapping(strat),
        );
    }
    let workflow_name = payload_json
        .get("workflow_name")
        .and_then(|v| v.as_str())
        .unwrap_or("workflow");
    let mut doc = serde_yaml::Mapping::new();
    doc.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String(workflow_name.into()),
    );
    doc.insert(
        serde_yaml::Value::String("on".into()),
        serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(event.into())]),
    );
    if let Some(env) = payload_json.get("workflow_env").filter(|v| !v.is_null()) {
        if let Ok(e) = serde_yaml::to_value(env) {
            doc.insert(serde_yaml::Value::String("env".into()), e);
        }
    }
    let mut jobs = serde_yaml::Mapping::new();
    jobs.insert(serde_yaml::Value::String(job.job_key.clone()), job_v);
    doc.insert(
        serde_yaml::Value::String("jobs".into()),
        serde_yaml::Value::Mapping(jobs),
    );
    serde_yaml::to_string(&serde_yaml::Value::Mapping(doc))
        .ok()
        .map(String::into_bytes)
}

/// GitHub-shaped `github` context as a `google.protobuf.Struct` — field names
/// match Gitea's `GenerateGiteaContext` so act_runner reads what it expects.
#[allow(clippy::too_many_arguments)]
fn task_context(
    run: &oxidean_db::ActionRunRow,
    j: &oxidean_db::ActionJobRow,
    ctx: &ClaimContext,
    event_payload: serde_json::Value,
) -> prost_types::Struct {
    let ref_short = run
        .head_ref
        .strip_prefix("refs/heads/")
        .or_else(|| run.head_ref.strip_prefix("refs/tags/"))
        .unwrap_or(&run.head_ref);
    let ref_type = if run.head_ref.starts_with("refs/tags/") {
        "tag"
    } else {
        "branch"
    };
    json_struct(&serde_json::json!({
        "action": "",
        "action_path": "",
        "action_ref": "",
        "action_repository": "",
        "action_status": "",
        "actor": ctx.actor,
        "api_url": format!("{}/api/v1", ctx.server_url),
        "base_ref": ctx.base_ref,
        "env": "",
        "event": event_payload,
        "event_name": run.event,
        "event_path": "",
        "graphql_url": "",
        "head_ref": if ctx.pull_number > 0 { ref_short.to_string() } else { String::new() },
        "job": j.job_key,
        "path": "",
        "ref": run.head_ref,
        "ref_name": ref_short,
        "ref_protected": ctx.ref_protected,
        "ref_type": ref_type,
        "repository": format!("{}/{}", ctx.owner_slug, ctx.repo_name),
        "repository_owner": ctx.owner_slug,
        "repositoryUrl": format!("{}/{}/{}", ctx.server_url, ctx.owner_slug, ctx.repo_name),
        "retention_days": ctx.retention_days.to_string(),
        "run_id": run.id,
        "run_number": run.run_number.to_string(),
        "run_attempt": run.attempt.to_string(),
        "secret_source": "Actions",
        "server_url": ctx.server_url,
        "sha": run.head_sha,
        "triggering_actor": ctx.actor,
        "workflow": run.workflow_name,
        "workflow_ref": ctx.workflow_ref,
        "workspace": "",
        "token": ctx.job_token.clone().unwrap_or_default(),
        "gitea_runtime_token": ctx.cache_token.clone().unwrap_or_default(),
        "cache_url": ctx.cache_url.clone().unwrap_or_default(),
        "gitea_default_actions_url": ctx.server_url,
    }))
    .unwrap_or_else(|| prost_types::Struct {
        fields: Default::default(),
    })
}

/// proto3-JSON view of a `Task` — `workflowPayload` base64 (bytes), `context`
/// as a plain JSON object, `needs.<id>.result` as the enum number.
fn task_json(t: &pb::Task) -> serde_json::Value {
    use base64::Engine;
    let needs: serde_json::Map<String, serde_json::Value> = t
        .needs
        .iter()
        .map(|(k, n)| {
            (
                k.clone(),
                serde_json::json!({"outputs": n.outputs, "result": n.result}),
            )
        })
        .collect();
    serde_json::json!({
        "id": t.id.to_string(),
        "workflowPayload": t.workflow_payload.as_ref().map(|p| {
            base64::engine::general_purpose::STANDARD.encode(p)
        }),
        "context": t.context.as_ref().map(|s| {
            s.fields
                .iter()
                .map(|(k, v)| (k.clone(), prost_to_json(v)))
                .collect::<serde_json::Map<String, serde_json::Value>>()
        }),
        "secrets": t.secrets,
        "needs": serde_json::Value::Object(needs),
        "vars": t.vars,
    })
}

fn prost_to_json(v: &prost_types::Value) -> serde_json::Value {
    use prost_types::value::Kind;
    match v.kind.as_ref() {
        Some(Kind::NullValue(_)) | None => serde_json::Value::Null,
        Some(Kind::NumberValue(n)) => serde_json::json!(n),
        Some(Kind::StringValue(s)) => serde_json::json!(s),
        Some(Kind::BoolValue(b)) => serde_json::json!(b),
        Some(Kind::StructValue(s)) => s
            .fields
            .iter()
            .map(|(k, v)| (k.clone(), prost_to_json(v)))
            .collect::<serde_json::Map<String, serde_json::Value>>()
            .into(),
        Some(Kind::ListValue(l)) => {
            serde_json::Value::Array(l.values.iter().map(prost_to_json).collect())
        }
    }
}

fn needs_map(needs: Option<serde_json::Value>) -> HashMap<String, pb::TaskNeed> {
    let mut out = HashMap::new();
    if let Some(serde_json::Value::Object(m)) = needs {
        for (id, v) in m {
            let result = v
                .get("result")
                .and_then(|r| r.as_str())
                .map(result_for_status)
                .unwrap_or(0);
            let outputs = v
                .get("outputs")
                .and_then(|o| o.as_object())
                .map(|m| {
                    m.iter()
                        .map(|(k, x)| {
                            (
                                k.clone(),
                                x.as_str()
                                    .map(str::to_string)
                                    .unwrap_or_else(|| x.to_string()),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.insert(id, pb::TaskNeed { outputs, result });
        }
    }
    out
}

pub async fn fetch_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let w = wire(&headers);
    let runner = match gitea_runner_from_headers(&state, &headers).await {
        Ok(r) => r,
        Err(_) => return unauthorized("unknown runner token"),
    };
    let req = match w {
        Wire::Proto => match pb::FetchTaskRequest::decode(body) {
            Ok(r) => r,
            Err(e) => return internal_err(e),
        },
        Wire::Json => match decode_json(&body) {
            Ok(v) => pb::FetchTaskRequest {
                tasks_version: json_i64(&v, &["tasksVersion", "tasks_version"]),
            },
            Err(r) => return r,
        },
    };
    let latest = match state.db.action_tasks_version().await {
        Ok(v) => v,
        Err(e) => return internal_err(e),
    };
    let mut task: Option<pb::Task> = None;
    if req.tasks_version != latest {
        let labels: Vec<String> = serde_json::from_str(&runner.labels_json).unwrap_or_default();
        let job = match state
            .db
            .claim_queued_action_job_for_labels(&runner.id, &labels)
            .await
        {
            Ok(j) => j,
            Err(e) => return internal_err(e),
        };
        if let Some(j) = job {
            match build_task(&state, &j, &runner.id).await {
                Ok(t) => {
                    task = Some(t);
                    crate::webhook::dispatch::notify_workflow_job(&state.db, &j.id, "in_progress")
                        .await;
                }
                Err(resp) => return resp,
            }
        }
    }
    respond(
        w,
        &pb::FetchTaskResponse {
            task: task.clone(),
            tasks_version: latest,
        },
        serde_json::json!({
            "tasksVersion": latest,
            "task": task.as_ref().map(task_json),
        }),
    )
}

#[allow(deprecated)] // `Task.machine` is a deprecated-but-required field
#[allow(clippy::result_large_err)] // Response is the deliberate early-return mechanism
async fn build_task(
    state: &AppState,
    j: &oxidean_db::ActionJobRow,
    runner_id: &str,
) -> Result<pb::Task, Response> {
    let run = state
        .db
        .find_action_run_by_id(&j.run_id)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| internal_err("claimed job has no run"))?;
    let payload = match j.payload_sha.as_deref() {
        Some(sha) => state
            .db
            .find_action_payload(sha)
            .await
            .map_err(internal_err)?,
        None => None,
    };
    let Some(payload) = payload else {
        tracing::error!(job_id = %j.id, run_id = %j.run_id, "claimed job has no captured payload");
        return Err(internal_err("job payload missing"));
    };
    let payload_json: serde_json::Value =
        serde_json::from_str(&payload.payload_json).unwrap_or_else(|_| serde_json::json!({}));
    let permissions = crate::actions::job_tokens::permissions_from_payload(&payload_json);
    let cc = claim_context(state, j, &run, permissions)
        .await
        .map_err(|_| internal_err("claim context failed"))?;
    let mut env_extra = std::collections::BTreeMap::new();
    if let (Some(url), Some(tok)) = (&cc.id_token_request_url, &cc.id_token_request_token) {
        env_extra.insert("ACTIONS_ID_TOKEN_REQUEST_URL".into(), url.clone());
        env_extra.insert("ACTIONS_ID_TOKEN_REQUEST_TOKEN".into(), tok.clone());
    }
    let workflow_payload = build_workflow_payload(&payload_json, j, &run.event, &env_extra);
    let task_id = state
        .db
        .mint_action_task_claim(&j.id, runner_id)
        .await
        .map_err(internal_err)?;
    let event_payload = {
        let base: serde_json::Value = run
            .event_payload_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        // Callee legs: patch the call's resolved inputs into event.inputs.
        match payload_json.get("call_inputs") {
            Some(inputs) if !inputs.is_null() => {
                crate::actions::reusable::patch_event_inputs(&base, inputs)
            }
            _ => base,
        }
    };
    let needs = {
        let all = state
            .db
            .list_action_jobs_for_run(&j.run_id)
            .await
            .unwrap_or_default();
        needs_map(crate::actions::scheduler::needs_context_for_task(
            &all,
            &j.needs_json,
            j.caller_job_id.as_str(),
        ))
    };
    // Callee legs: `call_secrets` remaps the decrypted caller scope —
    // `inherit` passes through, a map forwards each callee name, absent =
    // the callee sees nothing.
    let secrets = if !j.caller_job_id.is_empty() {
        let spec = payload_json
            .get("call_secrets")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let scope: serde_json::Map<String, serde_json::Value> = cc
            .secrets
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        crate::actions::reusable::remap_call_secrets(&scope, &spec)
            .into_iter()
            .map(|(k, v)| (k, v.as_str().unwrap_or_default().to_string()))
            .collect()
    } else {
        cc.secrets.clone()
    };
    Ok(pb::Task {
        id: task_id,
        workflow_payload,
        context: Some(task_context(&run, j, &cc, event_payload)),
        secrets,
        machine: String::new(),
        needs,
        vars: cc.vars.clone(),
    })
}

pub async fn update_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let w = wire(&headers);
    let runner = match gitea_runner_from_headers(&state, &headers).await {
        Ok(r) => r,
        Err(_) => return unauthorized("unknown runner token"),
    };
    let req = match w {
        Wire::Proto => match pb::UpdateTaskRequest::decode(body) {
            Ok(r) => r,
            Err(e) => return internal_err(e),
        },
        Wire::Json => match decode_json(&body) {
            Ok(v) => {
                let s = v.get("state").cloned().unwrap_or_default();
                pb::UpdateTaskRequest {
                    state: Some(pb::TaskState {
                        id: json_i64(&s, &["id"]),
                        result: json_i64(&s, &["result"]) as i32,
                        ..Default::default()
                    }),
                    outputs: json_str_map(&v, "outputs"),
                }
            }
            Err(r) => return r,
        },
    };
    let Some(task_state) = req.state else {
        return connect_err(
            StatusCode::BAD_REQUEST,
            "invalid_argument",
            "missing task state",
        );
    };
    let claim = match state.db.find_action_task_claim(task_state.id).await {
        Ok(c) => c,
        Err(e) => return internal_err(e),
    };
    let Some((job_id, runner_id)) = claim else {
        return not_found("unknown task_id");
    };
    if runner_id != runner.id {
        return forbidden("task belongs to another runner");
    }
    let job = match state.db.find_action_job_by_id(&job_id).await {
        Ok(Some(j)) => j,
        Ok(None) => return not_found("unknown job"),
        Err(e) => return internal_err(e),
    };
    // Runner contact — liveness heartbeat for the stale-claim sweeper.
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let _ = state.db.touch_action_job_heartbeat(&job.id, &at).await;
    let status = status_for_result(task_state.result);
    let prev_run_status = state
        .db
        .find_action_run_by_id(&job.run_id)
        .await
        .ok()
        .flatten()
        .map(|r| r.status);
    let landed = match state.db.transition_action_job_status(&job.id, status).await {
        Ok(l) => l,
        Err(e) => return internal_err(e),
    };
    // act_runner reports already-evaluated job outputs ({name: value}) —
    // store directly; the runner resolved `outputs:` expressions itself.
    if matches!(status, "success" | "failure" | "cancelled") && !req.outputs.is_empty() {
        let evaluated = serde_json::json!(req
            .outputs
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect::<serde_json::Map<_, _>>());
        if let Err(e) = state
            .db
            .set_action_job_outputs(&job.id, &evaluated.to_string())
            .await
        {
            tracing::warn!(error = %e, job_id = %job.id, "job outputs persist failed");
        }
    }
    finalize_task_update(&state, &job, prev_run_status, status, landed, None).await;
    // Server-authoritative result — a job cancelled server-side reads back
    // CANCELLED so the runner aborts mid-run.
    let current = state
        .db
        .find_action_job_by_id(&job.id)
        .await
        .ok()
        .flatten()
        .map(|j| j.status)
        .unwrap_or_default();
    let result = result_for_status(&current);
    let mut sent: Vec<String> = req.outputs.keys().cloned().collect();
    sent.sort();
    respond(
        w,
        &pb::UpdateTaskResponse {
            state: Some(pb::TaskState {
                id: task_state.id,
                result,
                ..Default::default()
            }),
            sent_outputs: sent.clone(),
        },
        serde_json::json!({
            "state": {"id": task_state.id, "result": result},
            "sentOutputs": sent,
        }),
    )
}

pub async fn update_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let w = wire(&headers);
    let runner = match gitea_runner_from_headers(&state, &headers).await {
        Ok(r) => r,
        Err(_) => return unauthorized("unknown runner token"),
    };
    let req = match w {
        Wire::Proto => match pb::UpdateLogRequest::decode(body) {
            Ok(r) => r,
            Err(e) => return internal_err(e),
        },
        Wire::Json => match decode_json(&body) {
            Ok(v) => pb::UpdateLogRequest {
                task_id: json_i64(&v, &["taskId", "task_id"]),
                index: json_i64(&v, &["index"]),
                rows: v
                    .get("rows")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .map(|r| pb::LogRow {
                                time: None,
                                content: json_str(r, &["content"]),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                no_more: json_bool(&v, &["noMore", "no_more"]),
            },
            Err(r) => return r,
        },
    };
    let claim = match state.db.find_action_task_claim(req.task_id).await {
        Ok(c) => c,
        Err(e) => return internal_err(e),
    };
    let Some((job_id, runner_id)) = claim else {
        return not_found("unknown task_id");
    };
    if runner_id != runner.id {
        return forbidden("task belongs to another runner");
    }
    let job = match state.db.find_action_job_by_id(&job_id).await {
        Ok(Some(j)) => j,
        Ok(None) => return not_found("unknown job"),
        Err(e) => return internal_err(e),
    };
    // Ordering by row cursor: `index` is the client's next-row position,
    // `log_index` the server's. Below stored → partial/full replay (only the
    // unseen tail appends); above stored → a gap — ack the stored cursor so
    // act_runner resends from it instead of dropping rows.
    let stored = match state.db.action_job_log_index(&job.id).await {
        Ok(i) => i,
        Err(e) => return internal_err(e),
    };
    if req.index > stored {
        return respond(
            w,
            &pb::UpdateLogResponse { ack_index: stored },
            serde_json::json!({"ackIndex": stored}),
        );
    }
    let overlap = (stored - req.index).clamp(0, req.rows.len() as i64) as usize;
    let new_rows = &req.rows[overlap..];
    if !new_rows.is_empty() {
        // Rows are newline-less lines; append each with its terminator.
        let mut chunk = String::new();
        for r in new_rows {
            chunk.push_str(&r.content);
            chunk.push('\n');
        }
        if chunk.len() > MAX_GITEA_LOG_BYTES {
            return connect_err(
                StatusCode::PAYLOAD_TOO_LARGE,
                "resource_exhausted",
                "log batch too large",
            );
        }
        let next = req.index + req.rows.len() as i64;
        match state
            .db
            .set_action_job_log_index_if(&job.id, stored, next)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                // Cursor moved under us — treat as retryable.
                return connect_err(StatusCode::CONFLICT, "aborted", "log cursor moved");
            }
            Err(e) => return internal_err(e),
        }
        if let Err(e) = crate::actions::logs::append_job_log_bounded(
            &state.actions_log_dir,
            &job.run_id,
            &job.id,
            chunk.as_bytes(),
            crate::actions::runner_proto::max_job_log_bytes() as u64,
        )
        .await
        {
            return internal_err(e);
        }
    }
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let _ = state.db.touch_action_job_heartbeat(&job.id, &at).await;
    // ack = next row index the client should send — never behind the stored
    // cursor so a full replay converges instead of looping on dup rows.
    let ack = (req.index + req.rows.len() as i64).max(stored);
    respond(
        w,
        &pb::UpdateLogResponse { ack_index: ack },
        serde_json::json!({"ackIndex": ack}),
    )
}
