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
use axum::routing::{delete, get};
use axum::Json;
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{RespSpec, RouteDef};
use super::{call_map, ctx_for, dispatch, repo_ref, status_for_error, RepoPath};
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
