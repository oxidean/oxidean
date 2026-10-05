//! REST facade over the typed RPC domain (API-01).
//!
//! `/api/v1/**` is a companion surface to `POST /api/rpc` — every REST route
//! translates its path/query/body into the matching RPC procedure's input and
//! runs through [`rpc::dispatch`], so ACLs, PAT scope gates (`authorize_rpc`),
//! the bootstrap lock, and notification/webhook side effects all behave
//! exactly as they do for RPC callers. No business logic is duplicated here.
//!
//! Response conventions differ from the RPC envelope:
//! - Success → the procedure's `data` payload directly (no `{ok,data}` wrap).
//!   `POST` create endpoints return 201.
//! - Failure → the `AppError` object (`{code, message, data?}`) with a mapped
//!   HTTP status (see [`status_for_error`]).
//!
//! Auth: the `oxidean_session` cookie wins; `Authorization: Bearer <pat>`
//! applies when no cookie is present — same as `/api/rpc` (API-02).

mod admin;
mod issues;
pub mod openapi;
mod orgs;
mod pulls;
mod releases;
mod repos;
pub mod spec;
mod statuses;
mod users;

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use oxidean_core::{AppError, HealthResponse, RpcRequest, RpcResponse};
use schemars::SchemaGenerator;
use serde_json::{Map, Value};

use crate::app::{build_rpc_ctx, edge_credential, AppState};
use crate::pat::bearer::BearerRejection;
use crate::rpc::{self, RpcCtx};

use self::spec::{RespSpec, RouteDef};

/// `/api/v1` route group — mounted in `app::router_with_state`, built from the
/// same `RouteDef` table that generates `openapi.json` (single source).
pub fn router() -> Router<AppState> {
    spec::router_from_defs().merge(openapi::router())
}

/// Meta routes owned by `rest::mod` — just health for now.
const META_ROUTES: &[RouteDef] = &[RouteDef {
    method: "GET",
    path: "/health",
    tags: &["meta"],
    operation_id: "getHealth",
    summary: "Instance health/version (system.health)",
    procedure: "system.health",
    ok: StatusCode::OK,
    anonymous: true,
    path_fields: &[],
    query: None,
    body: None,
    response: Some(RespSpec::Schema(
        SchemaGenerator::subschema_for::<HealthResponse>,
    )),
    mount: || get(health),
}];

/// Instance health/version — thin alias over `system.health`.
async fn health(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call(
        &state,
        &headers,
        "system.health",
        Value::Object(Map::new()),
        StatusCode::OK,
    )
    .await
}

/// Bearer auth failures surface as REST errors (401 + `WWW-Authenticate`, or
/// 429 + `Retry-After`) — same contract as the RPC edge, different body shape.
fn bearer_rejection_response(rejection: BearerRejection) -> Response {
    let status = rejection.status();
    let mut res = (status, Json(rejection.error)).into_response();
    if status == StatusCode::UNAUTHORIZED {
        if let Ok(v) = HeaderValue::from_str(r#"Bearer realm="Oxidean REST API""#) {
            res.headers_mut().insert(header::WWW_AUTHENTICATE, v);
        }
    }
    if let Some(secs) = rejection.retry_after {
        if let Ok(v) = HeaderValue::from_str(&secs.to_string()) {
            res.headers_mut().insert(header::RETRY_AFTER, v);
        }
    }
    res
}

/// Resolve the edge credential (session cookie or PAT Bearer) into an `RpcCtx`
/// — identical to the `/api/rpc` HTTP edge.
async fn ctx_for(state: &AppState, headers: &HeaderMap) -> Result<RpcCtx, Response> {
    build_rpc_ctx(
        state,
        edge_credential(headers),
        rpc::ClientMeta::from_headers(headers),
    )
    .await
    .map_err(bearer_rejection_response)
}

/// Run one RPC procedure and render the REST response.
async fn call(
    state: &AppState,
    headers: &HeaderMap,
    procedure: &str,
    input: Value,
    ok_status: StatusCode,
) -> Response {
    let mut ctx = match ctx_for(state, headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(&mut ctx, procedure, input).await;
    respond(resp, ok_status)
}

/// `call` variant that transforms the success payload before responding
/// (e.g. filtering `repo.refs` down to branches only).
async fn call_map<F>(
    state: &AppState,
    headers: &HeaderMap,
    procedure: &str,
    input: Value,
    ok_status: StatusCode,
    f: F,
) -> Response
where
    F: FnOnce(Value) -> Value,
{
    let mut ctx = match ctx_for(state, headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(&mut ctx, procedure, input).await;
    match resp {
        RpcResponse::Ok { data, .. } => (ok_status, Json(f(data))).into_response(),
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

async fn dispatch(ctx: &mut RpcCtx, procedure: &str, input: Value) -> RpcResponse {
    rpc::dispatch(
        ctx,
        RpcRequest {
            procedure: procedure.to_string(),
            input,
        },
    )
    .await
}

fn respond(resp: RpcResponse, ok_status: StatusCode) -> Response {
    match resp {
        RpcResponse::Ok { data, .. } => (ok_status, Json(data)).into_response(),
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

/// Map `AppError.code` to a REST status. The RPC edge's `rpc_status` enumerates
/// specific codes; the REST surface maps by convention so every domain error
/// gets a sane status without a per-procedure table.
fn status_for_error(err: &AppError) -> StatusCode {
    let code = err.code.as_str();
    match code {
        "auth.unauthenticated" => StatusCode::UNAUTHORIZED,
        "auth.rate_limited" => StatusCode::TOO_MANY_REQUESTS,
        "auth.setup_required" | "auth.email_unverified" | "auth.pat_scope" => StatusCode::FORBIDDEN,
        "db.not_configured" => StatusCode::SERVICE_UNAVAILABLE,
        "rpc.payload_too_large" => StatusCode::PAYLOAD_TOO_LARGE,
        "pull.merge_conflict" | "release.tag_taken" | "auth.taken" | "email.taken" => {
            StatusCode::CONFLICT
        }
        "repo.branch_protection" => StatusCode::FORBIDDEN,
        _ => {
            if code.ends_with("forbidden") {
                StatusCode::FORBIDDEN
            } else if code.contains("not_found") || code == "rpc.unknown_procedure" {
                StatusCode::NOT_FOUND
            } else if code.ends_with("internal") || code.ends_with("_failed") {
                StatusCode::INTERNAL_SERVER_ERROR
            } else {
                StatusCode::BAD_REQUEST
            }
        }
    }
}

/// Overlay identity/path fields onto a request body. Path fields always win —
/// the URL is the canonical identity for the resource.
fn merge_fields(fields: Value, body: Value) -> Value {
    let mut obj = match body {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    if let Value::Object(extra) = fields {
        obj.extend(extra);
    }
    Value::Object(obj)
}

/// `{owner}/{repo}` path params → the RPC `{owner, name}` input convention.
fn repo_ref(owner: &str, repo: &str) -> Value {
    serde_json::json!({ "owner": owner, "name": repo })
}

/// Path params for `/{owner}/{repo}` routes.
#[derive(serde::Deserialize)]
struct RepoPath {
    owner: String,
    repo: String,
}

/// Path params for `/{owner}/{repo}/(issues|pulls)/{number}` routes.
#[derive(serde::Deserialize)]
struct RepoNumberPath {
    owner: String,
    repo: String,
    number: i64,
}

impl RepoNumberPath {
    /// `{owner, name, number}` — the shared `*RefRequest` shape.
    fn ref_input(&self) -> Value {
        serde_json::json!({
            "owner": self.owner,
            "name": self.repo,
            "number": self.number,
        })
    }
}

/// Path params for issue-comment routes (`{number}` is required because the
/// underlying RPC inputs key comments by issue number + comment id).
#[derive(serde::Deserialize)]
struct CommentPath {
    owner: String,
    repo: String,
    number: i64,
    comment_id: String,
}
