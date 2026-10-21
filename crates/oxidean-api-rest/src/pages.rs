//! `/api/v1/repos/{o}/{r}/pages/**` — GitHub-shaped Pages REST over the
//! `pages.*` RPC domain.
//!
//! Mapping notes:
//! - `RepoPagesPublic`/`PagesBuildPublic` are already GitHub-shaped; the
//!   builds list unwraps the RPC's `{builds}` envelope into a bare array.
//! - `POST /pages/deployment` is the `actions/deploy-pages` seam: GitHub's
//!   body carries `artifact_url`; we translate the embedded artifact row id
//!   into `artifact_id` (which `pages.deploys.create` resolves and re-pins
//!   to the calling job token's run). `{artifact}` passes through as the
//!   artifact name for callers that know it.
//! - Read routes stay anonymous-capable; the ACL inside each proc decides.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{delete, get, post, put};
use axum::Json;
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, call_map, merge_fields, repo_ref, RepoPath};
use oxidean_api_core::app::AppState;

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/pages",
        tags: &["pages"],
        operation_id: "pages/getPagesSite",
        summary: "Get the repo's Pages site (pages.get)",
        procedure: "pages.get",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(pages_site_schema)),
        mount: || get(get_site),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/pages",
        tags: &["pages"],
        operation_id: "pages/createPagesSite",
        summary: "Enable Pages for the repo (pages.enable)",
        procedure: "pages.enable",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo"],
        query: None,
        body: Some(BodySpec::Json(pages_enable_body_schema)),
        response: Some(RespSpec::Json(pages_site_schema)),
        mount: || post(enable_site),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/pages",
        tags: &["pages"],
        operation_id: "pages/updatePagesSite",
        summary: "Update Pages settings (pages.update)",
        procedure: "pages.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo"],
        query: None,
        body: Some(BodySpec::Json(pages_update_body_schema)),
        response: None,
        mount: || put(update_site),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/pages",
        tags: &["pages"],
        operation_id: "pages/deletePagesSite",
        summary: "Disable Pages for the repo (pages.disable)",
        procedure: "pages.disable",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "repo"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_site),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/pages/builds",
        tags: &["pages"],
        operation_id: "pages/listBuilds",
        summary: "List Pages builds (pages.builds.list)",
        procedure: "pages.builds.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo"],
        query: Some(SchemaGenerator::into_root_schema_for::<PagesBuildsQuery>),
        body: None,
        response: Some(RespSpec::Json(pages_builds_list_schema)),
        mount: || get(list_builds),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/pages/builds",
        tags: &["pages"],
        operation_id: "pages/requestBuild",
        summary: "Request a legacy Pages rebuild (pages.builds.request)",
        procedure: "pages.builds.request",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(pages_build_schema)),
        mount: || post(request_build),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/pages/builds/latest",
        tags: &["pages"],
        operation_id: "pages/getLatestBuild",
        summary: "Get the newest Pages build (pages.builds.get)",
        procedure: "pages.builds.get",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(pages_build_schema)),
        mount: || get(latest_build),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/pages/builds/{id}",
        tags: &["pages"],
        operation_id: "pages/getBuild",
        summary: "Get one Pages build (pages.builds.get)",
        procedure: "pages.builds.get",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo", "id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(pages_build_schema)),
        mount: || get(get_build),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/pages/deployment",
        tags: &["pages"],
        operation_id: "pages/createDeployment",
        summary: "Publish a Pages build from an Actions artifact (pages.deploys.create)",
        procedure: "pages.deploys.create",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo"],
        query: None,
        body: Some(BodySpec::Json(pages_deploy_body_schema)),
        response: Some(RespSpec::Json(pages_build_schema)),
        mount: || post(create_deployment),
    },
];

// ---- extractors -----------------------------------------------------------

#[derive(Deserialize)]
pub struct BuildPath {
    owner: String,
    repo: String,
    id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct PagesBuildsQuery {
    /// `built` | `errored` | `building`.
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    per_page: Option<i64>,
}

// ---- body schemas ----------------------------------------------------------

fn pages_enable_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "build_type": {"type": "string", "enum": ["legacy", "workflow"]},
            "source": {
                "type": "object",
                "properties": {
                    "branch": {"type": "string"},
                    "path": {"type": "string", "enum": ["/", "/docs"]},
                },
            },
        },
    })
}

fn pages_update_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "build_type": {"type": "string", "enum": ["legacy", "workflow"]},
            "source": {
                "type": "object",
                "properties": {
                    "branch": {"type": "string"},
                    "path": {"type": "string"},
                },
            },
            "cname": {"type": "string"},
            "https_enforced": {"type": "boolean"},
        },
    })
}

fn pages_deploy_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "artifact": {"type": "string", "description": "artifact name (default github-pages)"},
            "artifact_url": {"type": "string", "description": "GitHub field — the embedded artifact id is translated to artifact_id"},
        },
    })
}

fn pages_site_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn pages_build_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn pages_builds_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

// ---- handlers --------------------------------------------------------------

/// Add GitHub's `url` self-link beside the already-shaped site object.
fn gh_site(owner: &str, repo: &str) -> impl FnOnce(Value) -> Value {
    let api = format!(
        "{}/api/v1/repos/{owner}/{repo}/pages",
        oxidean_api_core::public_origin::resolve_public_origin()
    );
    move |mut v| {
        v["url"] = json!(api);
        v
    }
}

async fn get_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "pages.get",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        gh_site(&p.owner, &p.repo),
    )
    .await
}

async fn enable_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    call_map(
        &state,
        &headers,
        "pages.enable",
        merge_fields(repo_ref(&p.owner, &p.repo), body),
        StatusCode::CREATED,
        gh_site(&p.owner, &p.repo),
    )
    .await
}

async fn update_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    call(
        &state,
        &headers,
        "pages.update",
        merge_fields(repo_ref(&p.owner, &p.repo), body),
        StatusCode::OK,
    )
    .await
}

async fn delete_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call(
        &state,
        &headers,
        "pages.disable",
        repo_ref(&p.owner, &p.repo),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// Bare array, like GitHub's list — unwrap the RPC's `{builds}` envelope.
async fn list_builds(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<PagesBuildsQuery>,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    if let Some(s) = q.status {
        input["status"] = json!(s);
    }
    if let Some(n) = q.per_page {
        input["limit"] = json!(n);
    }
    call_map(
        &state,
        &headers,
        "pages.builds.list",
        input,
        StatusCode::OK,
        |d| d["builds"].clone(),
    )
    .await
}

async fn request_build(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call(
        &state,
        &headers,
        "pages.builds.request",
        repo_ref(&p.owner, &p.repo),
        StatusCode::CREATED,
    )
    .await
}

async fn latest_build(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    get_build_inner(&state, &headers, &p.owner, &p.repo, "latest").await
}

async fn get_build(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<BuildPath>,
) -> Response {
    get_build_inner(&state, &headers, &p.owner, &p.repo, &p.id).await
}

async fn get_build_inner(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    build: &str,
) -> Response {
    call(
        state,
        headers,
        "pages.builds.get",
        merge_fields(repo_ref(owner, repo), json!({"build": build})),
        StatusCode::OK,
    )
    .await
}

/// Extract the artifact row id embedded in a GitHub `artifact_url` — covers
/// `/actions/artifacts/{id}[/zip]` and the `/_apis/artifactblobs/{id}` plane.
fn artifact_id_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    for w in segs.windows(2) {
        if matches!(w[0], "artifacts" | "artifactblobs") {
            return Some(w[1].to_string());
        }
    }
    None
}

async fn create_deployment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    if let Some(name) = body.get("artifact").and_then(Value::as_str) {
        input["artifact"] = json!(name);
    }
    if let Some(url) = body.get("artifact_url").and_then(Value::as_str) {
        if let Some(id) = artifact_id_from_url(url) {
            input["artifact_id"] = json!(id);
        }
    }
    call(
        &state,
        &headers,
        "pages.deploys.create",
        input,
        StatusCode::OK,
    )
    .await
}
