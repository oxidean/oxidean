//! `/api/v1/repos/{o}/{r}/{check-runs,check-suites,commits/{ref}/check-*}` —
//! GitHub-shaped Checks REST over the `check.*` RPC domain.
//!
//! Mapping notes:
//! - GitHub's create/update body calls the check's display name `name`; our
//!   RPC calls it `check_name` (`name` is the repo name at the RPC layer).
//!   Handlers lift `name` → `check_name` before merging path fields.
//! - List responses gain GitHub's `total_count` envelope; the suites list key
//!   becomes `check_suites`.
//! - The annotations list answers a bare array (GitHub's shape), not the
//!   RPC's `{annotations}` wrapper.
//! - `app`/`check_suite` project as nested objects alongside the flat
//!   `check_suite_id`/app-label fields the RPC returns.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, patch, post};
use axum::Json;
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call_map, merge_fields, repo_ref};
use oxidean_api_core::app::AppState;

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/check-runs",
        tags: &["checks"],
        operation_id: "checks/create",
        summary: "Create a check run (check.runs.create)",
        procedure: "check.runs.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo"],
        query: None,
        body: Some(BodySpec::Json(check_run_create_body_schema)),
        response: Some(RespSpec::Json(check_run_schema)),
        mount: || post(create_check_run),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/check-runs/{id}",
        tags: &["checks"],
        operation_id: "checks/get",
        summary: "Get a check run (check.runs.get)",
        procedure: "check.runs.get",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo", "id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(check_run_schema)),
        mount: || get(get_check_run),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/check-runs/{id}",
        tags: &["checks"],
        operation_id: "checks/update",
        summary: "Update a check run (check.runs.update)",
        procedure: "check.runs.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo", "id"],
        query: None,
        body: Some(BodySpec::Json(check_run_update_body_schema)),
        response: Some(RespSpec::Json(check_run_schema)),
        mount: || patch(update_check_run),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/check-runs/{id}/rerequest",
        tags: &["checks"],
        operation_id: "checks/rerequestRun",
        summary: "Rerequest a check run (check.runs.rerequest)",
        procedure: "check.runs.rerequest",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo", "id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(check_run_schema)),
        mount: || post(rerequest_check_run),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/check-runs/{id}/annotations",
        tags: &["checks"],
        operation_id: "checks/listAnnotations",
        summary: "List a check run's annotations (check.annotations.list)",
        procedure: "check.annotations.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo", "id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(annotations_schema)),
        mount: || get(list_annotations),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/check-suites/{id}/check-runs",
        tags: &["checks"],
        operation_id: "checks/listForSuite",
        summary: "List check runs in a suite (check.runs.listForSuite)",
        procedure: "check.runs.listForSuite",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo", "id"],
        query: Some(SchemaGenerator::into_root_schema_for::<CheckRunsListQuery>),
        body: None,
        response: Some(RespSpec::Json(check_runs_list_schema)),
        mount: || get(list_suite_runs),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/commits/{ref}/check-runs",
        tags: &["checks"],
        operation_id: "checks/listForRef",
        summary: "List check runs for a git ref (check.runs.listForRef)",
        procedure: "check.runs.listForRef",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo", "ref"],
        query: Some(SchemaGenerator::into_root_schema_for::<CheckRunsListQuery>),
        body: None,
        response: Some(RespSpec::Json(check_runs_list_schema)),
        mount: || get(list_ref_runs),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/check-suites",
        tags: &["checks"],
        operation_id: "checks/createSuite",
        summary: "Create a check suite (check.suites.create)",
        procedure: "check.suites.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo"],
        query: None,
        body: Some(BodySpec::Json(check_suite_create_body_schema)),
        response: Some(RespSpec::Json(check_suite_schema)),
        mount: || post(create_suite),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/check-suites/{id}",
        tags: &["checks"],
        operation_id: "checks/getSuite",
        summary: "Get a check suite (check.suites.get)",
        procedure: "check.suites.get",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo", "id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(check_suite_schema)),
        mount: || get(get_suite),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/check-suites/{id}/rerequest",
        tags: &["checks"],
        operation_id: "checks/rerequestSuite",
        summary: "Rerequest a check suite (check.suites.rerequest)",
        procedure: "check.suites.rerequest",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo", "id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(check_suite_schema)),
        mount: || post(rerequest_suite),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/commits/{ref}/check-suites",
        tags: &["checks"],
        operation_id: "checks/listSuitesForRef",
        summary: "List check suites for a git ref (check.suites.list)",
        procedure: "check.suites.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo", "ref"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(check_suites_list_schema)),
        mount: || get(list_ref_suites),
    },
];

// ---- extractors -----------------------------------------------------------

#[derive(Deserialize)]
pub struct RepoIdPath {
    owner: String,
    repo: String,
    id: String,
}

#[derive(Deserialize)]
pub struct RepoRefPath {
    owner: String,
    repo: String,
    #[serde(rename = "ref")]
    git_ref: String,
}

/// `check_name`/`status`/`conclusion` filters plus `per_page` → `limit`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct CheckRunsListQuery {
    #[serde(default)]
    check_name: Option<String>,
    #[serde(default)]
    status: Option<String>,
    /// Server-side extra over GitHub: fold a conclusion filter in directly.
    #[serde(default)]
    conclusion: Option<String>,
    #[serde(default)]
    per_page: Option<u32>,
}

fn check_run_create_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["name", "head_sha"],
        "properties": {
            "name": {"type": "string"},
            "head_sha": {"type": "string"},
            "details_url": {"type": "string"},
            "external_id": {"type": "string"},
            "status": {"type": "string"},
            "conclusion": {"type": "string"},
            "started_at": {"type": "string"},
            "completed_at": {"type": "string"},
            "output": {"type": "object"},
        },
    })
}

fn check_run_update_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "details_url": {"type": ["string", "null"]},
            "external_id": {"type": ["string", "null"]},
            "status": {"type": "string"},
            "conclusion": {"type": ["string", "null"]},
            "started_at": {"type": ["string", "null"]},
            "completed_at": {"type": ["string", "null"]},
            "output": {"type": "object"},
        },
    })
}

fn check_suite_create_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["head_sha"],
        "properties": {"head_sha": {"type": "string"}},
    })
}

// ---- GitHub reshaping -----------------------------------------------------

/// `CheckRunPublic` → GitHub's shape: nest `check_suite`/`app` beside the
/// flat fields (additive — RPC consumers still see the flat columns).
fn gh_check_run(r: Value) -> Value {
    let mut v = r;
    v["check_suite"] = json!({"id": v["check_suite_id"]});
    v["app"] = json!({"slug": v["app"], "name": v["app"]});
    v
}

fn gh_check_suite(s: Value) -> Value {
    let mut v = s;
    v["app"] = json!({"slug": v["app"], "name": v["app"]});
    v
}

fn check_run_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn check_suite_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn check_runs_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object", "required": ["total_count", "check_runs"]})
}

fn check_suites_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object", "required": ["total_count", "check_suites"]})
}

fn annotations_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

// ---- handlers ---------------------------------------------------------------

async fn create_check_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<super::RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let mut body = body;
    if let Some(name) = body.get("name").and_then(Value::as_str) {
        body["check_name"] = json!(name);
    }
    call_map(
        &state,
        &headers,
        "check.runs.create",
        merge_fields(repo_ref(&p.owner, &p.repo), body),
        StatusCode::CREATED,
        gh_check_run,
    )
    .await
}

async fn get_check_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoIdPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "check.runs.get",
        merge_fields(repo_ref(&p.owner, &p.repo), json!({"id": p.id})),
        StatusCode::OK,
        gh_check_run,
    )
    .await
}

async fn update_check_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoIdPath>,
    Json(body): Json<Value>,
) -> Response {
    let mut body = body;
    if let Some(name) = body.get("name").and_then(Value::as_str) {
        body["check_name"] = json!(name);
        body.as_object_mut().map(|m| m.remove("name"));
    }
    call_map(
        &state,
        &headers,
        "check.runs.update",
        merge_fields(
            repo_ref(&p.owner, &p.repo),
            merge_fields(json!({"id": p.id}), body),
        ),
        StatusCode::OK,
        gh_check_run,
    )
    .await
}

async fn rerequest_check_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoIdPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "check.runs.rerequest",
        merge_fields(repo_ref(&p.owner, &p.repo), json!({"id": p.id})),
        StatusCode::CREATED,
        gh_check_run,
    )
    .await
}

/// GitHub answers a bare annotations array — unwrap the RPC's
/// `{annotations}` envelope.
async fn list_annotations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoIdPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "check.annotations.list",
        merge_fields(repo_ref(&p.owner, &p.repo), json!({"check_run_id": p.id})),
        StatusCode::OK,
        |d| d["annotations"].clone(),
    )
    .await
}

async fn list_suite_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoIdPath>,
    Query(q): Query<CheckRunsListQuery>,
) -> Response {
    let mut input = merge_fields(repo_ref(&p.owner, &p.repo), json!({"suite_id": p.id}));
    if let Some(v) = q.check_name {
        input["check_name"] = json!(v);
    }
    if let Some(v) = q.status {
        input["status"] = json!(v);
    }
    if let Some(v) = q.per_page {
        input["limit"] = json!(v);
    }
    call_map(
        &state,
        &headers,
        "check.runs.listForSuite",
        input,
        StatusCode::OK,
        |d| {
            let runs = d["check_runs"].as_array().cloned().unwrap_or_default();
            json!({"total_count": runs.len(), "check_runs": runs.into_iter().map(gh_check_run).collect::<Vec<_>>()})
        },
    )
    .await
}

async fn list_ref_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoRefPath>,
    Query(q): Query<CheckRunsListQuery>,
) -> Response {
    let mut input = merge_fields(repo_ref(&p.owner, &p.repo), json!({"ref": p.git_ref}));
    if let Some(v) = q.check_name {
        input["check_name"] = json!(v);
    }
    if let Some(v) = q.status {
        input["status"] = json!(v);
    }
    if let Some(v) = q.conclusion {
        input["conclusion"] = json!(v);
    }
    if let Some(v) = q.per_page {
        input["limit"] = json!(v);
    }
    call_map(
        &state,
        &headers,
        "check.runs.listForRef",
        input,
        StatusCode::OK,
        |d| {
            let runs = d["check_runs"].as_array().cloned().unwrap_or_default();
            json!({"total_count": runs.len(), "check_runs": runs.into_iter().map(gh_check_run).collect::<Vec<_>>()})
        },
    )
    .await
}

async fn create_suite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<super::RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    call_map(
        &state,
        &headers,
        "check.suites.create",
        merge_fields(repo_ref(&p.owner, &p.repo), body),
        StatusCode::CREATED,
        gh_check_suite,
    )
    .await
}

async fn get_suite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoIdPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "check.suites.get",
        merge_fields(repo_ref(&p.owner, &p.repo), json!({"id": p.id})),
        StatusCode::OK,
        gh_check_suite,
    )
    .await
}

async fn rerequest_suite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoIdPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "check.suites.rerequest",
        merge_fields(repo_ref(&p.owner, &p.repo), json!({"id": p.id})),
        StatusCode::CREATED,
        gh_check_suite,
    )
    .await
}

async fn list_ref_suites(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoRefPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "check.suites.list",
        merge_fields(repo_ref(&p.owner, &p.repo), json!({"ref": p.git_ref})),
        StatusCode::OK,
        |d| {
            let suites = d["suites"].as_array().cloned().unwrap_or_default();
            json!({"total_count": suites.len(), "check_suites": suites.into_iter().map(gh_check_suite).collect::<Vec<_>>()})
        },
    )
    .await
}
