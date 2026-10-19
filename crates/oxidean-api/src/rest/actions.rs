//! GitHub-compatible Actions artifact routes under `/api/v1` — the surface
//! `download-artifact`'s cross-run path (`github-token` + `run-id`), `gh
//! run download`, and other Octokit clients reach through `api_url`.
//!
//! JSON endpoints dispatch through the typed RPC (`repo.actions.*`) exactly
//! like every other `/api/v1` route. The `/zip` stream shares the
//! `repo.actions.getArtifact` auth check, then serves the stored blob —
//! builtin runner-proto uploads persist tar.gz and are repacked to zip on
//! the fly so `unzip`-based clients see GitHub's wire format.

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::Json;
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, call_map, ctx_for, dispatch, repo_ref, status_for_error, RepoPath};
use crate::app::AppState;
use crate::routes::release_assets::sanitize_filename;

#[derive(Deserialize, schemars::JsonSchema)]
struct ArtifactsQuery {
    /// Filter by exact artifact name (GitHub `name` query param).
    name: Option<String>,
    /// Page size — GitHub default 30, max 100.
    per_page: Option<u32>,
    /// 1-based page number.
    page: Option<u32>,
}

#[derive(Deserialize)]
struct RunArtifactsPath {
    owner: String,
    repo: String,
    run_id: String,
}

#[derive(Deserialize)]
struct ArtifactPath {
    owner: String,
    repo: String,
    artifact_id: String,
}

#[derive(Deserialize)]
struct RunPath {
    owner: String,
    repo: String,
    run_id: String,
}

impl RunPath {
    fn ref_input(&self) -> Value {
        json!({
            "owner": self.owner,
            "name": self.repo,
            "run_id": self.run_id,
        })
    }
}

#[derive(Deserialize)]
struct JobPath {
    owner: String,
    repo: String,
    job_id: String,
}

#[derive(Deserialize)]
struct WorkflowPath {
    owner: String,
    repo: String,
    /// Numeric id, workflow filename, full path, or workflow name.
    workflow_id: String,
}

/// `GET /actions/runs` filters — the subset GitHub documents plus the
/// pagination pair. `created`/`exclude_pull_requests`/`check_suite_id` are
/// accepted and ignored (no backing filter).
#[derive(Deserialize, schemars::JsonSchema)]
struct RunsQuery {
    actor: Option<String>,
    branch: Option<String>,
    event: Option<String>,
    /// `queued`/`in_progress`/`waiting`/`completed` or a conclusion name.
    status: Option<String>,
    per_page: Option<u32>,
    page: Option<u32>,
}

/// `POST …/dispatches` body — `{ref, inputs}`.
#[derive(Deserialize, schemars::JsonSchema)]
struct DispatchBody {
    #[serde(rename = "ref")]
    git_ref: String,
    #[serde(default)]
    inputs: Option<Value>,
}

impl ArtifactPath {
    fn ref_input(&self) -> Value {
        json!({
            "owner": self.owner,
            "name": self.repo,
            "artifact_id": self.artifact_id,
        })
    }
}

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/artifacts",
        tags: &["actions"],
        operation_id: "actions/listArtifacts",
        summary: "List repo artifacts, GitHub shape (repo.actions.listAllArtifacts)",
        procedure: "repo.actions.listAllArtifacts",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<ArtifactsQuery>),
        body: None,
        response: Some(RespSpec::Json(artifacts_list_schema)),
        mount: || get(list_repo_artifacts),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/artifacts",
        tags: &["actions"],
        operation_id: "actions/listWorkflowRunArtifacts",
        summary: "List a run's artifacts, GitHub shape (repo.actions.listArtifacts)",
        procedure: "repo.actions.listArtifacts",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: Some(SchemaGenerator::into_root_schema_for::<ArtifactsQuery>),
        body: None,
        response: Some(RespSpec::Json(artifacts_list_schema)),
        mount: || get(list_run_artifacts),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/artifacts/{artifact_id}",
        tags: &["actions"],
        operation_id: "actions/getArtifact",
        summary: "One artifact's metadata, GitHub shape (repo.actions.getArtifact)",
        procedure: "repo.actions.getArtifact",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "artifact_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(artifact_schema)),
        mount: || get(get_artifact),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/artifacts/{artifact_id}/zip",
        tags: &["actions"],
        operation_id: "actions/downloadArtifact",
        summary: "Stream the artifact archive as zip (repo.actions.getArtifact)",
        procedure: "repo.actions.getArtifact",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "artifact_id"],
        query: None,
        body: None,
        response: None,
        mount: || get(download_artifact_zip),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/actions/artifacts/{artifact_id}",
        tags: &["actions"],
        operation_id: "actions/deleteArtifact",
        summary: "Delete an artifact (repo.actions.deleteArtifact)",
        procedure: "repo.actions.deleteArtifact",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "artifact_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_artifact),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs",
        tags: &["actions"],
        operation_id: "actions/listWorkflowRuns",
        summary: "List workflow runs, GitHub shape (repo.actions.listRuns)",
        procedure: "repo.actions.listRuns",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<RunsQuery>),
        body: None,
        response: Some(RespSpec::Json(runs_list_schema)),
        mount: || get(list_runs),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}",
        tags: &["actions"],
        operation_id: "actions/getWorkflowRun",
        summary: "One workflow run, GitHub shape (repo.actions.getRun)",
        procedure: "repo.actions.getRun",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(run_schema)),
        mount: || get(get_run),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/jobs",
        tags: &["actions"],
        operation_id: "actions/listWorkflowRunJobs",
        summary: "List a run's jobs, GitHub shape (repo.actions.getRun)",
        procedure: "repo.actions.getRun",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(jobs_list_schema)),
        mount: || get(list_run_jobs),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/jobs/{job_id}",
        tags: &["actions"],
        operation_id: "actions/getJobForWorkflowRun",
        summary: "One job, GitHub shape (repo.actions.getJob)",
        procedure: "repo.actions.getJob",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "job_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(job_schema)),
        mount: || get(get_job),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/rerun",
        tags: &["actions"],
        operation_id: "actions/reRunWorkflow",
        summary: "Re-run a workflow run (repo.actions.rerunRun)",
        procedure: "repo.actions.rerunRun",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || post(rerun_run),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/rerun-failed",
        tags: &["actions"],
        operation_id: "actions/reRunWorkflowFailed",
        summary: "Re-run failed jobs only (repo.actions.rerunRun failed_only)",
        procedure: "repo.actions.rerunRun",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || post(rerun_failed),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/jobs/{job_id}/rerun",
        tags: &["actions"],
        operation_id: "actions/reRunJobForWorkflowRun",
        summary: "Re-run one job (repo.actions.rerunRun job_id)",
        procedure: "repo.actions.rerunRun",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "job_id"],
        query: None,
        body: None,
        response: None,
        mount: || post(rerun_job),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/cancel",
        tags: &["actions"],
        operation_id: "actions/cancelWorkflowRun",
        summary: "Cancel a workflow run (repo.actions.cancelRun)",
        procedure: "repo.actions.cancelRun",
        ok: StatusCode::ACCEPTED,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || post(cancel_run),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/approve",
        tags: &["actions"],
        operation_id: "actions/approveWorkflowRun",
        summary: "Approve a fork-held run (repo.actions.approveRun)",
        procedure: "repo.actions.approveRun",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || post(approve_run),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}",
        tags: &["actions"],
        operation_id: "actions/deleteWorkflowRun",
        summary: "Delete a workflow run (repo.actions.deleteRun)",
        procedure: "repo.actions.deleteRun",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_run),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/jobs/{job_id}/logs",
        tags: &["actions"],
        operation_id: "actions/downloadJobLogsForWorkflowRun",
        summary: "Stream a job's log (repo.actions.getJobLog)",
        procedure: "repo.actions.getJobLog",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "job_id"],
        query: None,
        body: None,
        response: None,
        mount: || get(crate::routes::action_downloads::download_job_log),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/logs",
        tags: &["actions"],
        operation_id: "actions/downloadWorkflowRunLogs",
        summary: "Download the run's logs zip (repo.actions.getJobLog)",
        procedure: "repo.actions.getJobLog",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || get(crate::routes::action_downloads::download_run_logs),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/workflows",
        tags: &["actions"],
        operation_id: "actions/listRepoWorkflows",
        summary: "List repo workflows, GitHub shape (repo.actions.listWorkflows)",
        procedure: "repo.actions.listWorkflows",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(workflows_list_schema)),
        mount: || get(list_workflows),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/workflows/{workflow_id}",
        tags: &["actions"],
        operation_id: "actions/getWorkflow",
        summary: "One workflow by id/filename/name (repo.actions.listWorkflows)",
        procedure: "repo.actions.listWorkflows",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(workflow_schema)),
        mount: || get(get_workflow),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/workflows/{workflow_id}/runs",
        tags: &["actions"],
        operation_id: "actions/listWorkflowRunsForWorkflow",
        summary: "List one workflow's runs (repo.actions.listRuns)",
        procedure: "repo.actions.listRuns",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<RunsQuery>),
        body: None,
        response: Some(RespSpec::Json(runs_list_schema)),
        mount: || get(list_workflow_runs),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/workflows/{workflow_id}/dispatches",
        tags: &["actions"],
        operation_id: "actions/createWorkflowDispatch",
        summary: "workflow_dispatch a workflow (repo.actions.dispatchWorkflow)",
        procedure: "repo.actions.dispatchWorkflow",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(dispatch_body_schema)),
        response: None,
        mount: || post(dispatch_workflow),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/actions/workflows/{workflow_id}/enable",
        tags: &["actions"],
        operation_id: "actions/enableWorkflow",
        summary: "Enable a workflow (repo.actions.setWorkflowEnabled)",
        procedure: "repo.actions.setWorkflowEnabled",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: None,
        mount: || put(enable_workflow),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/actions/workflows/{workflow_id}/disable",
        tags: &["actions"],
        operation_id: "actions/disableWorkflow",
        summary: "Disable a workflow (repo.actions.setWorkflowEnabled)",
        procedure: "repo.actions.setWorkflowEnabled",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: None,
        mount: || put(disable_workflow),
    },
];

fn artifacts_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "artifacts": {"type": "array", "items": artifact_schema_value()},
        },
        "required": ["total_count", "artifacts"],
    })
}

fn artifact_schema(_gen: &mut SchemaGenerator) -> Value {
    artifact_schema_value()
}

fn artifact_schema_value() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {},
            "name": {"type": "string"},
            "size_in_bytes": {"type": "integer"},
            "url": {"type": "string"},
            "archive_download_url": {"type": "string"},
            "expired": {"type": "boolean"},
            "created_at": {"type": "string"},
            "expires_at": {"type": ["string", "null"]},
            "digest": {"type": ["string", "null"]},
            "workflow_run": {"type": "object"},
        },
    })
}

/// SQLite artifact rows store `%Y-%m-%d %H:%M:%S` — emit GitHub's RFC3339.
fn iso(v: &Value) -> Value {
    match v.as_str() {
        Some(s) if s.len() == 19 && s.as_bytes()[10] == b' ' => {
            json!(format!("{}T{}Z", &s[..10], &s[11..]))
        }
        _ => v.clone(),
    }
}

/// `ActionArtifactPublic` JSON → the GitHub REST artifact object.
fn gh_artifact(origin: &str, owner: &str, repo: &str, a: &Value) -> Value {
    let id = a["id"].as_str().unwrap_or_default();
    let run_id = a["run_id"].as_str().unwrap_or_default();
    let id_json = match id.parse::<i64>() {
        Ok(n) => json!(n),
        Err(_) => json!(id),
    };
    let run_json = match run_id.parse::<i64>() {
        Ok(n) => json!(n),
        Err(_) => json!(run_id),
    };
    let base = format!("{origin}/api/v1/repos/{owner}/{repo}/actions/artifacts/{id}");
    let expires = a["expires_at"].as_str();
    let expired = expires
        .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
        .is_some_and(|e| e < chrono::Utc::now());
    let digest = a["digest_sha256"].as_str().unwrap_or_default();
    json!({
        "id": id_json,
        "node_id": "",
        "name": a["name"].as_str().unwrap_or_default(),
        "size_in_bytes": a["size_bytes"].as_i64().unwrap_or_default(),
        "url": base,
        "archive_download_url": format!("{base}/zip"),
        "expired": expired,
        "created_at": iso(&a["created_at"]),
        "updated_at": iso(&a["created_at"]),
        "expires_at": expires.map(|e| json!(e)).unwrap_or(Value::Null),
        "digest": if digest.is_empty() {
            Value::Null
        } else {
            json!(format!("sha256:{digest}"))
        },
        "workflow_run": { "id": run_json },
    })
}

fn page_slice<'a>(q: &ArtifactsQuery, items: &'a [Value]) -> &'a [Value] {
    let per_page = q.per_page.unwrap_or(30).clamp(1, 100) as usize;
    let page = q.page.unwrap_or(1).max(1) as usize;
    let start = ((page - 1) * per_page).min(items.len());
    &items[start..(start + per_page).min(items.len())]
}

/// Shared reshape: RPC `artifacts` array → `{total_count, artifacts:[gh]}`.
fn gh_list(owner: &str, repo: &str, q: &ArtifactsQuery, data: Value) -> Value {
    let origin = crate::public_origin::resolve_public_origin();
    let mut items = data["artifacts"].as_array().cloned().unwrap_or_default();
    if let Some(name) = q.name.as_deref() {
        items.retain(|a| a["name"].as_str() == Some(name));
    }
    let total = items.len();
    let page: Vec<Value> = page_slice(q, &items)
        .iter()
        .map(|a| gh_artifact(&origin, owner, repo, a))
        .collect();
    json!({"total_count": total, "artifacts": page})
}

/// GET /repos/{o}/{r}/actions/artifacts — repo-wide, GitHub shape.
async fn list_repo_artifacts(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    Query(q): Query<ArtifactsQuery>,
    headers: HeaderMap,
) -> Response {
    let owner = p.owner.clone();
    let repo = p.repo.clone();
    call_map(
        &state,
        &headers,
        "repo.actions.listAllArtifacts",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        move |data| gh_list(&owner, &repo, &q, data),
    )
    .await
}

/// GET /repos/{o}/{r}/actions/runs/{run_id}/artifacts — the cross-run list
/// `download-artifact` paginates.
async fn list_run_artifacts(
    State(state): State<AppState>,
    Path(p): Path<RunArtifactsPath>,
    Query(q): Query<ArtifactsQuery>,
    headers: HeaderMap,
) -> Response {
    let owner = p.owner.clone();
    let repo = p.repo.clone();
    let mut input = repo_ref(&p.owner, &p.repo);
    input["run_id"] = json!(p.run_id);
    call_map(
        &state,
        &headers,
        "repo.actions.listArtifacts",
        input,
        StatusCode::OK,
        move |data| gh_list(&owner, &repo, &q, data),
    )
    .await
}

/// GET /repos/{o}/{r}/actions/artifacts/{id} — bare GitHub artifact object.
async fn get_artifact(
    State(state): State<AppState>,
    Path(p): Path<ArtifactPath>,
    headers: HeaderMap,
) -> Response {
    let owner = p.owner.clone();
    let repo = p.repo.clone();
    call_map(
        &state,
        &headers,
        "repo.actions.getArtifact",
        p.ref_input(),
        StatusCode::OK,
        move |data| {
            gh_artifact(
                &crate::public_origin::resolve_public_origin(),
                &owner,
                &repo,
                &data["artifact"],
            )
        },
    )
    .await
}

/// GET /repos/{o}/{r}/actions/artifacts/{id}/zip — the download Octokit
/// follows. Stored zip payloads pass through; builtin tar.gz repacks.
async fn download_artifact_zip(
    State(state): State<AppState>,
    Path(p): Path<ArtifactPath>,
    headers: HeaderMap,
) -> Response {
    // Auth + existence through the real RPC seam, then the storage path.
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(&mut ctx, "repo.actions.getArtifact", p.ref_input()).await;
    if let oxidean_core::RpcResponse::Err { error, .. } = resp {
        return (status_for_error(&error), Json(error)).into_response();
    }
    let row = match state.db.find_action_artifact_by_id(&p.artifact_id).await {
        Ok(Some(r)) => r,
        _ => return err_json(StatusCode::NOT_FOUND, "Artifact not found or expired"),
    };
    // GitHub 410s downloads once `expires_at` passes, even before the backend
    // physically reaps the row.
    let expired = row
        .expires_at
        .as_deref()
        .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
        .is_some_and(|e| e < chrono::Utc::now());
    if expired {
        return err_json(StatusCode::GONE, "Artifact expired");
    }
    let path = state.actions_artifacts_dir.join(&row.storage_path);
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(_) => return err_json(StatusCode::NOT_FOUND, "Artifact not found or expired"),
    };
    let body = match targz_to_zip(&bytes, &row.mime_type) {
        Ok(b) => b,
        Err(_) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, "archive build failed"),
    };
    let mut res = Response::new(Body::from(body));
    *res.status_mut() = StatusCode::OK;
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    if let Ok(hv) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}.zip\"",
        sanitize_filename(&row.name)
    )) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, hv);
    }
    res
}

/// DELETE /repos/{o}/{r}/actions/artifacts/{id} — GitHub returns bare 204.
async fn delete_artifact(
    State(state): State<AppState>,
    Path(p): Path<ArtifactPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    match dispatch(&mut ctx, "repo.actions.deleteArtifact", p.ref_input()).await {
        oxidean_core::RpcResponse::Ok { .. } => StatusCode::NO_CONTENT.into_response(),
        oxidean_core::RpcResponse::Err { error, .. } => {
            (status_for_error(&error), Json(error)).into_response()
        }
    }
}

fn err_json(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"message": message}))).into_response()
}

/// Repo the stored blob to zip. v4 Twirp uploads are already zip;
/// builtin runner uploads are tar.gz (repack so `unzip` clients work).
fn targz_to_zip(bytes: &[u8], mime_type: &str) -> std::io::Result<Vec<u8>> {
    let is_gzip = mime_type == "application/gzip" || bytes.starts_with(&[0x1f, 0x8b]);
    if !is_gzip {
        return Ok(bytes.to_vec());
    }
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut out);
        for entry in ar.entries()? {
            let mut e = entry?;
            if !e.header().entry_type().is_file() {
                continue;
            }
            let name = e.path()?.to_string_lossy().replace('\\', "/");
            // Zip-slip guard — archive names are runner-generated, but keep
            // the repack safe against absolute/parent paths regardless.
            if name.starts_with('/') || name.split('/').any(|seg| seg == "..") {
                continue;
            }
            let mut opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            if let Ok(mode) = e.header().mode() {
                opts = opts.unix_permissions(mode);
            }
            w.start_file(&name, opts)?;
            std::io::copy(&mut e, &mut w)?;
        }
        w.finish()?;
    }
    Ok(out.into_inner())
}

// --- runs / jobs / workflows — the `gh run|workflow` surface ---------------

/// Stored status → GitHub's `(status, conclusion)` pair. Non-terminal rows
/// keep a null conclusion; `waiting_approval` reads as GitHub's `waiting`.
fn status_conclusion(status: &str) -> (Value, Value) {
    match status {
        "queued" => (json!("queued"), Value::Null),
        "in_progress" => (json!("in_progress"), Value::Null),
        "waiting" | "waiting_approval" => (json!("waiting"), Value::Null),
        s => (json!("completed"), json!(s)),
    }
}

/// `refs/heads/main` → `main`; other ref forms pass through.
fn short_branch(head_ref: &str) -> &str {
    head_ref.strip_prefix("refs/heads/").unwrap_or(head_ref)
}

fn num_or_str(s: &str) -> Value {
    s.parse::<i64>().map_or_else(|_| json!(s), |n| json!(n))
}

/// `ActionRunPublic` JSON → the GitHub workflow-run object.
fn gh_run(origin: &str, owner: &str, repo: &str, r: &Value) -> Value {
    let id = r["id"].as_str().unwrap_or_default();
    let (status, conclusion) = status_conclusion(r["status"].as_str().unwrap_or_default());
    let base = format!("{origin}/api/v1/repos/{owner}/{repo}");
    let html_repo = format!("{origin}/{owner}/{repo}");
    json!({
        "id": num_or_str(id),
        "name": r["workflow_name"].as_str().unwrap_or_default(),
        "node_id": "",
        "head_branch": short_branch(r["head_ref"].as_str().unwrap_or_default()),
        "head_sha": r["head_sha"].as_str().unwrap_or_default(),
        "path": r["workflow_path"].as_str().unwrap_or_default(),
        "run_number": r["run_number"].as_i64().unwrap_or_default(),
        "run_attempt": r["attempt"].as_i64().unwrap_or(1),
        "event": r["event"].as_str().unwrap_or_default(),
        "status": status,
        "conclusion": conclusion,
        "display_title": r["title"].as_str().unwrap_or_default(),
        "url": format!("{base}/actions/runs/{id}"),
        "html_url": format!("{html_repo}/actions/runs/{id}"),
        "created_at": iso(&r["created_at"]),
        "updated_at": iso(&r["updated_at"]),
        "actor": r["actor"].as_str().map(|a| json!({"login": a})),
        "triggering_actor": r["actor"].as_str().map(|a| json!({"login": a})),
        "repository_id": num_or_str(r["repository_id"].as_str().unwrap_or_default()),
    })
}

/// `ActionJobPublic` JSON → the GitHub job object.
fn gh_job(origin: &str, owner: &str, repo: &str, j: &Value, run_name: Option<&str>) -> Value {
    let id = j["id"].as_str().unwrap_or_default();
    let run_id = j["run_id"].as_str().unwrap_or_default();
    let (status, conclusion) = status_conclusion(j["status"].as_str().unwrap_or_default());
    let base = format!("{origin}/api/v1/repos/{owner}/{repo}");
    json!({
        "id": num_or_str(id),
        "run_id": num_or_str(run_id),
        "name": j["name"].as_str().unwrap_or_default(),
        "status": status,
        "conclusion": conclusion,
        "run_attempt": j["attempt"].as_i64().unwrap_or(1),
        "workflow_name": run_name.map(Value::from).unwrap_or(Value::Null),
        "labels": j["runs_on"].clone(),
        "url": format!("{base}/actions/jobs/{id}"),
        "html_url": format!("{origin}/{owner}/{repo}/actions/runs/{run_id}/jobs/{id}"),
        "started_at": iso(&j["started_at"]),
        "completed_at": j["finished_at"].as_str().map(|_| iso(&j["finished_at"])).unwrap_or(Value::Null),
        "steps": [],
    })
}

/// `ActionWorkflowPublic` JSON → the GitHub workflow object. The path is the
/// identity (there is no separate numeric id); GitHub's `state` maps from our
/// `disabled` flag.
fn gh_workflow(origin: &str, owner: &str, repo: &str, w: &Value) -> Value {
    let path = w["path"].as_str().unwrap_or_default();
    let base = format!("{origin}/api/v1/repos/{owner}/{repo}");
    json!({
        "id": path,
        "name": w["name"].as_str().unwrap_or_default(),
        "path": path,
        "state": if w["disabled"].as_bool().unwrap_or(false) { "disabled_manually" } else { "active" },
        "url": format!("{base}/actions/workflows/{path}"),
        "html_url": format!("{origin}/{owner}/{repo}/actions?query=workflow%3A{path}"),
        "badge_url": format!("{base}/actions/workflows/{path}/badge.svg"),
    })
}

fn runs_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "workflow_runs": {"type": "array", "items": {"type": "object"}},
        },
        "required": ["total_count", "workflow_runs"],
    })
}

fn run_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn jobs_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "jobs": {"type": "array", "items": {"type": "object"}},
        },
        "required": ["total_count", "jobs"],
    })
}

fn job_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn workflows_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "workflows": {"type": "array", "items": {"type": "object"}},
        },
        "required": ["total_count", "workflows"],
    })
}

fn workflow_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn dispatch_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "ref": {"type": "string"},
            "inputs": {"type": "object"},
        },
        "required": ["ref"],
    })
}

/// GET /actions/runs — `gh run list`. Filters pass straight through to the
/// RPC (`status` includes `completed` → any terminal row).
async fn list_runs(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    Query(q): Query<RunsQuery>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo) = (p.owner.clone(), p.repo.clone());
    let mut input = repo_ref(&p.owner, &p.repo);
    input["actor"] = q.actor.as_deref().map_or(Value::Null, |v| json!(v));
    input["branch"] = q.branch.as_deref().map_or(Value::Null, |v| json!(v));
    input["event"] = q.event.as_deref().map_or(Value::Null, |v| json!(v));
    input["status"] = q.status.as_deref().map_or(Value::Null, |v| json!(v));
    input["per_page"] = q.per_page.map_or(Value::Null, |v| json!(v));
    input["page"] = q.page.map_or(Value::Null, |v| json!(v));
    call_map(
        &state,
        &headers,
        "repo.actions.listRuns",
        input,
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            let runs: Vec<Value> = data["runs"]
                .as_array()
                .map(|rs| {
                    rs.iter()
                        .map(|r| gh_run(&origin, &owner, &repo, r))
                        .collect()
                })
                .unwrap_or_default();
            json!({"total_count": data["total_count"], "workflow_runs": runs})
        },
    )
    .await
}

/// GET /actions/runs/{run_id} — `gh run view`.
async fn get_run(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo) = (p.owner.clone(), p.repo.clone());
    call_map(
        &state,
        &headers,
        "repo.actions.getRun",
        p.ref_input(),
        StatusCode::OK,
        move |data| {
            gh_run(
                &crate::public_origin::resolve_public_origin(),
                &owner,
                &repo,
                &data["run"],
            )
        },
    )
    .await
}

/// GET /actions/runs/{run_id}/jobs — `gh run view`'s job table.
async fn list_run_jobs(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo) = (p.owner.clone(), p.repo.clone());
    call_map(
        &state,
        &headers,
        "repo.actions.getRun",
        p.ref_input(),
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            let run_name = data["run"]["workflow_name"].as_str().map(str::to_string);
            let jobs: Vec<Value> = data["jobs"]
                .as_array()
                .map(|js| {
                    js.iter()
                        .map(|j| gh_job(&origin, &owner, &repo, j, run_name.as_deref()))
                        .collect()
                })
                .unwrap_or_default();
            json!({"total_count": jobs.len(), "jobs": jobs})
        },
    )
    .await
}

/// GET /actions/jobs/{job_id} — `gh run view --job`.
async fn get_job(
    State(state): State<AppState>,
    Path(p): Path<JobPath>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo) = (p.owner.clone(), p.repo.clone());
    let input = json!({"owner": p.owner, "name": p.repo, "job_id": p.job_id});
    call_map(
        &state,
        &headers,
        "repo.actions.getJob",
        input,
        StatusCode::OK,
        move |data| {
            gh_job(
                &crate::public_origin::resolve_public_origin(),
                &owner,
                &repo,
                &data["job"],
                None,
            )
        },
    )
    .await
}

/// Shared POST-with-empty-body dispatch for run mutations.
async fn run_mutation(
    state: &AppState,
    headers: &HeaderMap,
    p: &RunPath,
    proc: &str,
    extra: Value,
    ok: StatusCode,
) -> Response {
    let mut input = p.ref_input();
    if let Some(obj) = extra.as_object() {
        for (k, v) in obj {
            input[k] = v.clone();
        }
    }
    call(state, headers, proc, input, ok).await
}

async fn rerun_run(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    run_mutation(
        &state,
        &headers,
        &p,
        "repo.actions.rerunRun",
        json!({}),
        StatusCode::CREATED,
    )
    .await
}

async fn rerun_failed(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    run_mutation(
        &state,
        &headers,
        &p,
        "repo.actions.rerunRun",
        json!({"failed_only": true}),
        StatusCode::CREATED,
    )
    .await
}

/// POST /actions/jobs/{job_id}/rerun — the per-job requeue resolves through
/// the job row to find its run first.
async fn rerun_job(
    State(state): State<AppState>,
    Path(p): Path<JobPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let input = json!({"owner": p.owner, "name": p.repo, "job_id": p.job_id});
    let resp = dispatch(&mut ctx, "repo.actions.getJob", input).await;
    let job = match resp {
        oxidean_core::RpcResponse::Ok { data, .. } => data["job"].clone(),
        oxidean_core::RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response();
        }
    };
    let run_id = job["run_id"].as_str().unwrap_or_default().to_string();
    let mut input = repo_ref(&p.owner, &p.repo);
    input["run_id"] = json!(run_id);
    input["job_id"] = json!(p.job_id);
    call(
        &state,
        &headers,
        "repo.actions.rerunRun",
        input,
        StatusCode::CREATED,
    )
    .await
}

async fn cancel_run(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    run_mutation(
        &state,
        &headers,
        &p,
        "repo.actions.cancelRun",
        json!({}),
        StatusCode::ACCEPTED,
    )
    .await
}

async fn approve_run(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    run_mutation(
        &state,
        &headers,
        &p,
        "repo.actions.approveRun",
        json!({}),
        StatusCode::CREATED,
    )
    .await
}

async fn delete_run(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    run_mutation(
        &state,
        &headers,
        &p,
        "repo.actions.deleteRun",
        json!({}),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// GET /actions/workflows — `gh workflow list`.
async fn list_workflows(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo) = (p.owner.clone(), p.repo.clone());
    call_map(
        &state,
        &headers,
        "repo.actions.listWorkflows",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            let workflows: Vec<Value> = data["workflows"]
                .as_array()
                .map(|ws| {
                    ws.iter()
                        .map(|w| gh_workflow(&origin, &owner, &repo, w))
                        .collect()
                })
                .unwrap_or_default();
            json!({"total_count": workflows.len(), "workflows": workflows})
        },
    )
    .await
}

/// `workflow_id` resolves as full path, basename, or exact name — GitHub's
/// API accepts the workflow's numeric id or filename.
fn match_workflow<'a>(workflows: &'a [Value], id: &str) -> Option<&'a Value> {
    workflows.iter().find(|w| {
        let path = w["path"].as_str().unwrap_or_default();
        path == id || path.rsplit('/').next() == Some(id) || w["name"].as_str() == Some(id)
    })
}

/// GET /actions/workflows/{workflow_id} — `gh workflow view`.
async fn get_workflow(
    State(state): State<AppState>,
    Path(p): Path<WorkflowPath>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo, wf_id) = (p.owner.clone(), p.repo.clone(), p.workflow_id.clone());
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(
        &mut ctx,
        "repo.actions.listWorkflows",
        repo_ref(&p.owner, &p.repo),
    )
    .await;
    let data = match resp {
        oxidean_core::RpcResponse::Ok { data, .. } => data,
        oxidean_core::RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response();
        }
    };
    let workflows = data["workflows"].as_array().cloned().unwrap_or_default();
    match match_workflow(&workflows, &wf_id) {
        Some(w) => Json(gh_workflow(
            &crate::public_origin::resolve_public_origin(),
            &owner,
            &repo,
            w,
        ))
        .into_response(),
        None => err_json(StatusCode::NOT_FOUND, "Workflow not found"),
    }
}

/// GET /actions/workflows/{workflow_id}/runs — `gh run list --workflow`.
async fn list_workflow_runs(
    State(state): State<AppState>,
    Path(p): Path<WorkflowPath>,
    Query(q): Query<RunsQuery>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo, wf_id) = (p.owner.clone(), p.repo.clone(), p.workflow_id.clone());
    // Resolve the id/name to the canonical path for the RPC filter.
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(
        &mut ctx,
        "repo.actions.listWorkflows",
        repo_ref(&p.owner, &p.repo),
    )
    .await;
    let data = match resp {
        oxidean_core::RpcResponse::Ok { data, .. } => data,
        oxidean_core::RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response();
        }
    };
    let workflows = data["workflows"].as_array().cloned().unwrap_or_default();
    let Some(w) = match_workflow(&workflows, &wf_id) else {
        return err_json(StatusCode::NOT_FOUND, "Workflow not found");
    };
    let path = w["path"].as_str().unwrap_or_default().to_string();
    let mut input = repo_ref(&p.owner, &p.repo);
    input["workflow"] = json!(path);
    input["actor"] = q.actor.as_deref().map_or(Value::Null, |v| json!(v));
    input["branch"] = q.branch.as_deref().map_or(Value::Null, |v| json!(v));
    input["event"] = q.event.as_deref().map_or(Value::Null, |v| json!(v));
    input["status"] = q.status.as_deref().map_or(Value::Null, |v| json!(v));
    input["per_page"] = q.per_page.map_or(Value::Null, |v| json!(v));
    input["page"] = q.page.map_or(Value::Null, |v| json!(v));
    call_map(
        &state,
        &headers,
        "repo.actions.listRuns",
        input,
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            let runs: Vec<Value> = data["runs"]
                .as_array()
                .map(|rs| {
                    rs.iter()
                        .map(|r| gh_run(&origin, &owner, &repo, r))
                        .collect()
                })
                .unwrap_or_default();
            json!({"total_count": data["total_count"], "workflow_runs": runs})
        },
    )
    .await
}

/// POST /actions/workflows/{workflow_id}/dispatches — `gh workflow run`.
async fn dispatch_workflow(
    State(state): State<AppState>,
    Path(p): Path<WorkflowPath>,
    headers: HeaderMap,
    Json(body): Json<DispatchBody>,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    input["workflow_id"] = json!(p.workflow_id);
    input["git_ref"] = json!(body.git_ref);
    if let Some(inputs) = body.inputs {
        input["inputs"] = inputs;
    }
    call(
        &state,
        &headers,
        "repo.actions.dispatchWorkflow",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

/// Shared enable/disable — `{owner, name, path, enabled}`.
async fn set_workflow_enabled(
    state: &AppState,
    headers: &HeaderMap,
    p: &WorkflowPath,
    enabled: bool,
) -> Response {
    let mut ctx = match ctx_for(state, headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(
        &mut ctx,
        "repo.actions.listWorkflows",
        repo_ref(&p.owner, &p.repo),
    )
    .await;
    let data = match resp {
        oxidean_core::RpcResponse::Ok { data, .. } => data,
        oxidean_core::RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response();
        }
    };
    let workflows = data["workflows"].as_array().cloned().unwrap_or_default();
    let Some(w) = match_workflow(&workflows, &p.workflow_id) else {
        return err_json(StatusCode::NOT_FOUND, "Workflow not found");
    };
    let mut input = repo_ref(&p.owner, &p.repo);
    input["path"] = json!(w["path"].as_str().unwrap_or_default());
    input["enabled"] = json!(enabled);
    call(
        state,
        headers,
        "repo.actions.setWorkflowEnabled",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

async fn enable_workflow(
    State(state): State<AppState>,
    Path(p): Path<WorkflowPath>,
    headers: HeaderMap,
) -> Response {
    set_workflow_enabled(&state, &headers, &p, true).await
}

async fn disable_workflow(
    State(state): State<AppState>,
    Path(p): Path<WorkflowPath>,
    headers: HeaderMap,
) -> Response {
    set_workflow_enabled(&state, &headers, &p, false).await
}
