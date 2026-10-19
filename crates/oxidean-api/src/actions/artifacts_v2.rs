//! `upload-artifact@v4` / `download-artifact@v4` wire protocol — the v4
//! artifact backend speaks Twirp over
//! `{ACTIONS_RESULTS_URL}/twirp/github.actions.results.api.v1.ArtifactService/*`.
//! Like the cache v2 surface the toolkit's `ArtifactServiceClientJSON` posts
//! `application/json` with proto field names (snake_case); protobuf is never
//! exercised.
//!
//!   POST /twirp/github.actions.results.api.v1.ArtifactService/CreateArtifact
//!   POST /twirp/github.actions.results.api.v1.ArtifactService/FinalizeArtifact
//!   POST /twirp/github.actions.results.api.v1.ArtifactService/ListArtifacts
//!   POST /twirp/github.actions.results.api.v1.ArtifactService/GetSignedArtifactURL
//!   POST /twirp/github.actions.results.api.v1.ArtifactService/DeleteArtifact
//!
//! Rows share `action_artifacts` with the builtin runner-proto endpoints —
//! a builtin upload and a Twirp upload land in the same store and list the
//! same way, matching GitHub's single logical store.
//!
//! Backend ids: requests carry `workflow_run_backend_id` /
//! `workflow_job_run_backend_id`, which the toolkit decodes from the
//! `ACTIONS_RUNTIME_TOKEN` JWT's `scp` claim (`Actions.Results:<run>:<job>` —
//! minted in `cache::mint_cache_token`). Writes authorize job-exact, reads
//! run-scoped (any job in the run may fetch its siblings' artifacts, like
//! `download-artifact` same-run).
//!
//! Blob bytes never cross the Twirp plane: `CreateArtifact` returns a
//! `signed_upload_url` the Azure SDK writes (`PUT`, `?comp=block`, then
//! `?comp=blocklist`), and `GetSignedArtifactURL` returns a `signed_url`
//! fetched with a plain GET. Both are self-authenticating `?t=` URLs under
//! `/_apis/artifactblobs/{id}` — SAS semantics, no Authorization header.

use std::collections::HashMap;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{post, put};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::app::AppState;

use super::cache::{
    cache_scope_from_headers, cache_scope_from_query, reserve_ttl_minutes, CacheScope,
};
use super::cache_v2::{
    block_file, blocks_dir, de_i64_maybe_string, parse_block_list, twirp_err, twirp_internal,
    twirp_status, write_body_to,
};

/// Fields shared by all five RPCs — the backend ids echo the runtime token's
/// `scp` claim; `expires_at`/`mime_type`/`hash`/`name_filter`/`id_filter`
/// arrive as proto3-JSON scalars (wrappers unwrap to bare values server-side).
#[derive(Debug, Deserialize)]
struct ArtifactRef {
    #[serde(default)]
    workflow_run_backend_id: String,
    #[serde(default)]
    workflow_job_run_backend_id: String,
    #[serde(default)]
    name: String,
    // `version` (the client's protocol marker, currently 7) is accepted and
    // ignored — unknown fields deserialize freely.
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    mime_type: Option<String>,
    /// `FinalizeArtifact` — declared archive size (int64 → JSON string).
    #[serde(default, deserialize_with = "de_i64_maybe_string")]
    size: i64,
    /// `FinalizeArtifact` — `"sha256:<hex>"` integrity check.
    #[serde(default)]
    hash: Option<String>,
    /// `ListArtifacts` — exact-name filter.
    #[serde(default)]
    name_filter: Option<String>,
    /// `ListArtifacts` — `database_id` filter (numeric string).
    #[serde(default)]
    id_filter: Option<String>,
}

/// Job-exact write gate — CreateArtifact/FinalizeArtifact must name the same
/// job the runtime token was minted for. `Some` = the denial response.
fn require_job_scope(scope: &CacheScope, req: &ArtifactRef) -> Option<Response> {
    if req.workflow_run_backend_id == scope.run_id
        && req.workflow_job_run_backend_id == scope.job_id
    {
        None
    } else {
        Some(twirp_err(
            StatusCode::FORBIDDEN,
            "permission_denied",
            "backend ids do not match the runtime token",
        ))
    }
}

/// Run-scoped read gate — List/GetSignedURL cover any artifact in the run.
fn require_run_scope(scope: &CacheScope, req: &ArtifactRef) -> Option<Response> {
    if req.workflow_run_backend_id == scope.run_id {
        None
    } else {
        Some(twirp_err(
            StatusCode::FORBIDDEN,
            "permission_denied",
            "backend ids do not match the runtime token",
        ))
    }
}

/// Artifact-name validation shared with the builtin upload path.
fn name_invalid(name: &str) -> bool {
    name.is_empty() || name.len() > 255 || name.contains('/') || name.contains("..")
}

/// `{run_id}/{artifact_id}.zip` under the artifacts dir — same layout the
/// builtin upload uses (`.tgz` there, `.zip` here; the v4 client uploads a
/// zip unless `skipArchive` sends a raw file, mime_type records which).
fn storage_rel(run_id: &str, artifact_id: &str) -> String {
    format!("{run_id}/{artifact_id}.zip")
}

/// Absolute `?t=` SAS URL for the artifact blob plane.
fn blob_url(artifact_id: &str, token: &str) -> String {
    let origin = crate::public_origin::resolve_public_origin();
    format!("{origin}/api/actions/_apis/artifactblobs/{artifact_id}?t={token}")
}

/// POST `…/ArtifactService/CreateArtifact` — reserve `name` under the run and
/// hand back the upload URL. A row already occupying (run, name) — pending or
/// committed — is `already_exists`, matching GitHub's create-time name
/// reservation (immutable artifact names per run).
async fn create_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ArtifactRef>,
) -> Response {
    let scope = match cache_scope_from_headers(&state, &headers).await {
        Ok(s) => s,
        Err(e) => return twirp_status(e),
    };
    if let Some(resp) = require_job_scope(&scope, &req) {
        return resp;
    }
    if name_invalid(&req.name) {
        return twirp_err(
            StatusCode::BAD_REQUEST,
            "invalid_argument",
            "artifact name is required and must be ≤ 255 chars without path separators",
        );
    }
    let expires_at = match &req.expires_at {
        Some(raw) => match chrono::DateTime::parse_from_rfc3339(raw) {
            Ok(ts) => Some(ts.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
            Err(_) => {
                return twirp_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_argument",
                    "expires_at must be an RFC3339 timestamp",
                );
            }
        },
        // A `0` retention means "keep indefinitely" (no expiry row value) —
        // same convention as the builtin upload path.
        None => match state.actions_artifact_retention_days {
            0 => None,
            days => Some(
                (chrono::Utc::now() + chrono::Duration::days(i64::from(days)))
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            ),
        },
    };
    match state
        .db
        .find_action_artifact(&scope.run_id, &req.name)
        .await
    {
        Ok(Some(_)) => {
            return twirp_err(
                StatusCode::CONFLICT,
                "already_exists",
                format!("artifact {} already exists in this run", req.name),
            );
        }
        Ok(None) => {}
        Err(_) => return twirp_internal(),
    }
    // Numeric id: `FinalizeArtifact`'s `artifact_id`/`database_id` feed the
    // client's BigInt (int64 wire fields) — millis + 6-digit suffix stays in
    // i64 range and sorts chronologically.
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let artifact_id = format!("{}{:06}", millis, Uuid::new_v4().as_u128() % 1_000_000);
    let row = oxidean_db::actions::ActionArtifactRow {
        id: artifact_id.clone(),
        repository_id: scope.repository_id.clone(),
        run_id: scope.run_id.clone(),
        job_id: Some(scope.job_id.clone()),
        name: req.name.clone(),
        storage_path: storage_rel(&scope.run_id, &artifact_id),
        size_bytes: 0,
        digest_sha256: String::new(),
        expires_at,
        state: "pending".into(),
        mime_type: req
            .mime_type
            .filter(|m| !m.is_empty() && m.len() <= 255)
            .unwrap_or_else(|| "application/zip".into()),
        created_at: String::new(),
    };
    match state.db.upsert_action_artifact(&row).await {
        Ok(()) => Json(serde_json::json!({
            "ok": true,
            "signed_upload_url": blob_url(&artifact_id, &scope.token),
        }))
        .into_response(),
        Err(_) => twirp_err(
            StatusCode::CONFLICT,
            "already_exists",
            format!("artifact {} already exists in this run", req.name),
        ),
    }
}

/// POST `…/ArtifactService/FinalizeArtifact` — the pending row is found by
/// (run, name); the staged archive must match `size` and the declared
/// `sha256:` digest before `.part` becomes the committed blob.
async fn finalize_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ArtifactRef>,
) -> Response {
    let scope = match cache_scope_from_headers(&state, &headers).await {
        Ok(s) => s,
        Err(e) => return twirp_status(e),
    };
    if let Some(resp) = require_job_scope(&scope, &req) {
        return resp;
    }
    let row = match state
        .db
        .find_action_artifact(&scope.run_id, &req.name)
        .await
    {
        Ok(r) => r,
        Err(_) => return twirp_internal(),
    };
    let Some(row) = row.filter(|r| r.state == "pending") else {
        return Json(serde_json::json!({
            "ok": false,
            "message": "no pending artifact for this name",
        }))
        .into_response();
    };
    let dest = state.actions_artifacts_dir.join(&row.storage_path);
    let tmp = dest.with_extension("part");
    let mut on_disk = tokio::fs::metadata(&tmp)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    // A zero-byte artifact may legitimately land without any blob PUT — an
    // absent `.part` is the committed empty payload, not a missing upload.
    if on_disk == 0 && req.size == 0 && !tmp.exists() {
        if let Some(parent) = tmp.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&tmp, b"");
        on_disk = 0;
    }
    if req.size < 0 || on_disk != req.size as u64 {
        return Json(serde_json::json!({
            "ok": false,
            "message": format!(
                "uploaded size {on_disk} does not match declared size {}",
                req.size
            ),
        }))
        .into_response();
    }
    // `hash` arrives as `sha256:<hex>` — verify against the staged bytes.
    if let Some(declared) = req.hash.as_deref().and_then(|h| h.strip_prefix("sha256:")) {
        let mut file = match tokio::fs::File::open(&tmp).await {
            Ok(f) => f,
            Err(_) => {
                return Json(
                    serde_json::json!({ "ok": false, "message": "uploaded payload missing" }),
                )
                .into_response();
            }
        };
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            match file.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => hasher.update(&buf[..n]),
                Err(_) => return twirp_internal(),
            }
        }
        if hex::encode(hasher.finalize()) != declared {
            return Json(serde_json::json!({
                "ok": false,
                "message": "uploaded payload sha256 does not match declared hash",
            }))
            .into_response();
        }
    }
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::rename(&tmp, &dest).is_err() {
        return twirp_internal();
    }
    let digest = req
        .hash
        .as_deref()
        .and_then(|h| h.strip_prefix("sha256:"))
        .unwrap_or("")
        .to_string();
    match state
        .db
        .commit_action_artifact(&row.id, req.size, &digest)
        .await
    {
        Ok(true) => Json(serde_json::json!({ "ok": true, "artifact_id": row.id })).into_response(),
        Ok(false) => {
            let _ = std::fs::remove_file(&dest);
            Json(serde_json::json!({ "ok": false, "message": "artifact already committed" }))
                .into_response()
        }
        Err(_) => twirp_internal(),
    }
}

/// `MonolithArtifact` wire shape — int64 fields serialize as strings.
fn artifact_json(row: &oxidean_db::actions::ActionArtifactRow) -> serde_json::Value {
    serde_json::json!({
        "workflow_run_backend_id": row.run_id,
        "workflow_job_run_backend_id": row.job_id.clone().unwrap_or_default(),
        // `database_id` is an int64 wire field — builtin rows carry `art-*`
        // ids, which cannot round-trip `BigInt`; omit rather than lie.
        "database_id": row.id.parse::<i64>().ok().map(|n| n.to_string()),
        "name": row.name,
        "size": row.size_bytes.to_string(),
        "created_at": row.created_at,
        "digest": (!row.digest_sha256.is_empty())
            .then(|| format!("sha256:{}", row.digest_sha256)),
    })
}

/// POST `…/ArtifactService/ListArtifacts` — committed rows for the run,
/// optionally narrowed by `name_filter` (exact) or `id_filter` (database_id).
async fn list_artifacts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ArtifactRef>,
) -> Response {
    let scope = match cache_scope_from_headers(&state, &headers).await {
        Ok(s) => s,
        Err(e) => return twirp_status(e),
    };
    if let Some(resp) = require_run_scope(&scope, &req) {
        return resp;
    }
    let rows = match state.db.list_action_artifacts_for_run(&scope.run_id).await {
        Ok(r) => r,
        Err(_) => return twirp_internal(),
    };
    let artifacts: Vec<serde_json::Value> = rows
        .into_iter()
        .filter(|r| {
            req.name_filter.as_deref().is_none_or(|n| r.name == n)
                && req.id_filter.as_deref().is_none_or(|id| {
                    r.id.parse::<i64>().ok().map(|n| n.to_string()).as_deref() == Some(id)
                })
        })
        .map(|r| artifact_json(&r))
        .collect();
    Json(serde_json::json!({ "artifacts": artifacts })).into_response()
}

/// POST `…/ArtifactService/GetSignedArtifactURL` — run-scoped name lookup →
/// self-authenticating `?t=` blob URL.
async fn get_signed_url(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ArtifactRef>,
) -> Response {
    let scope = match cache_scope_from_headers(&state, &headers).await {
        Ok(s) => s,
        Err(e) => return twirp_status(e),
    };
    if let Some(resp) = require_run_scope(&scope, &req) {
        return resp;
    }
    let row = match state
        .db
        .find_action_artifact(&scope.run_id, &req.name)
        .await
    {
        Ok(r) => r,
        Err(_) => return twirp_internal(),
    };
    match row.filter(|r| r.state == "committed") {
        Some(row) => Json(serde_json::json!({
            "signed_url": blob_url(&row.id, &scope.token),
        }))
        .into_response(),
        None => twirp_err(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("artifact {} not found", req.name),
        ),
    }
}

/// POST `…/ArtifactService/DeleteArtifact` — drops the row and any staged or
/// committed payload files.
async fn delete_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ArtifactRef>,
) -> Response {
    let scope = match cache_scope_from_headers(&state, &headers).await {
        Ok(s) => s,
        Err(e) => return twirp_status(e),
    };
    if let Some(resp) = require_run_scope(&scope, &req) {
        return resp;
    }
    if let Ok(Some(row)) = state
        .db
        .find_action_artifact(&scope.run_id, &req.name)
        .await
    {
        let _ = state.db.delete_action_artifact(&row.id).await;
        remove_payload_files(&state, &row.storage_path);
    }
    Json(serde_json::json!({ "ok": true })).into_response()
}

/// Drop the committed blob + any staged upload residue for `storage_path`.
fn remove_payload_files(state: &AppState, storage_path: &str) {
    let dest = state.actions_artifacts_dir.join(storage_path);
    let _ = std::fs::remove_file(&dest);
    let _ = std::fs::remove_file(dest.with_extension("part"));
    let _ = std::fs::remove_dir_all(dest.with_extension("blocks"));
}

// ---------------------------------------------------------------------------
// Blob plane — same Azure block-blob shape as the cache v2 plane.
// ---------------------------------------------------------------------------

/// Look the blob's artifact row up and gate on the token's run.
async fn blob_row(
    state: &AppState,
    artifact_id: &str,
    scope: &CacheScope,
    pending_only: bool,
) -> Result<oxidean_db::actions::ActionArtifactRow, StatusCode> {
    let row = state
        .db
        .find_action_artifact_by_id(artifact_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if row.repository_id != scope.repository_id || row.run_id != scope.run_id {
        return Err(StatusCode::NOT_FOUND);
    }
    let expected = if pending_only { "pending" } else { "committed" };
    if row.state != expected {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(row)
}

/// PUT `/_apis/artifactblobs/{id}?t=…` — PutBlob single-shot, `?comp=block`
/// staging, `?comp=blocklist` commit into `.part` (same verbs as cache v2).
async fn put_blob(
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    body: Body,
) -> Result<StatusCode, StatusCode> {
    let scope = cache_scope_from_query(&state, q.get("t").map(String::as_str)).await?;
    let row = blob_row(&state, &artifact_id, &scope, true).await?;
    let max = state.actions_artifact_max_bytes;
    match q.get("comp").map(String::as_str) {
        Some("block") => {
            let block_id = q
                .get("blockid")
                .filter(|b| !b.is_empty())
                .ok_or(StatusCode::BAD_REQUEST)?;
            let dir = blocks_dir(&state.actions_artifacts_dir, &row.storage_path);
            write_body_to(&block_file(&dir, block_id), body, max)
                .await
                .map(|_| StatusCode::CREATED)
        }
        Some("blocklist") => {
            let bytes = body
                .into_data_stream()
                .fold(Vec::new(), |mut acc, c| async move {
                    if let Ok(c) = c {
                        acc.extend_from_slice(&c);
                    }
                    acc
                })
                .await;
            let ids = parse_block_list(&String::from_utf8_lossy(&bytes));
            if ids.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            let dir = blocks_dir(&state.actions_artifacts_dir, &row.storage_path);
            let dest = state
                .actions_artifacts_dir
                .join(&row.storage_path)
                .with_extension("part");
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            let mut out = tokio::fs::File::create(&dest)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut total: u64 = 0;
            for id in &ids {
                let data = tokio::fs::read(block_file(&dir, id))
                    .await
                    .map_err(|_| StatusCode::BAD_REQUEST)?;
                total += data.len() as u64;
                if total > max * 2 {
                    let _ = std::fs::remove_file(&dest);
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                out.write_all(&data)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            drop(out);
            let _ = std::fs::remove_dir_all(&dir);
            Ok(StatusCode::CREATED)
        }
        _ => {
            let dest = state
                .actions_artifacts_dir
                .join(&row.storage_path)
                .with_extension("part");
            write_body_to(&dest, body, max * 2)
                .await
                .map(|_| StatusCode::CREATED)
        }
    }
}

/// GET `/_apis/artifactblobs/{id}?t=…` — committed payload back with the
/// stored mime type + a `Content-Disposition` filename (the v4 client's
/// zip-vs-raw extraction decision reads both).
async fn get_blob(
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let scope = cache_scope_from_query(&state, q.get("t").map(String::as_str)).await?;
    let row = blob_row(&state, &artifact_id, &scope, false).await?;
    let path = state.actions_artifacts_dir.join(&row.storage_path);
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let safe_name: String = row
        .name
        .chars()
        .map(|c| if c == '"' || c == '\\' { '_' } else { c })
        .collect();
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", row.mime_type)
        .header("content-length", row.size_bytes.to_string())
        .header(
            "content-disposition",
            format!("attachment; filename=\"{safe_name}.zip\""),
        )
        .body(Body::from_stream(tokio_util::io::ReaderStream::new(file)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Twirp control plane — merged into the `/twirp` nest next to CacheService.
pub fn twirp_router() -> Router<AppState> {
    Router::new()
        .route(
            "/github.actions.results.api.v1.ArtifactService/CreateArtifact",
            post(create_artifact),
        )
        .route(
            "/github.actions.results.api.v1.ArtifactService/FinalizeArtifact",
            post(finalize_artifact),
        )
        .route(
            "/github.actions.results.api.v1.ArtifactService/ListArtifacts",
            post(list_artifacts),
        )
        .route(
            "/github.actions.results.api.v1.ArtifactService/GetSignedArtifactURL",
            post(get_signed_url),
        )
        .route(
            "/github.actions.results.api.v1.ArtifactService/DeleteArtifact",
            post(delete_artifact),
        )
}

/// Blob plane — merged into the `/api/actions` runner-proto mount next to
/// the cache v2 `_apis/artifactcache2/blobs` routes.
pub fn blob_router() -> Router<AppState> {
    Router::new().route("/_apis/artifactblobs/{id}", put(put_blob).get(get_blob))
}

/// Periodic maintenance — drop 'pending' reservations past the reservation
/// TTL (abandoned between CreateArtifact and FinalizeArtifact) together with
/// their staged `.part`/`.blocks` payload files. Runs on the actions sweep
/// tick next to the cache reservation sweep.
pub async fn sweep_pending_artifacts(
    db: &oxidean_db::Database,
    artifacts_dir: &std::path::Path,
) -> Result<u64, String> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(reserve_ttl_minutes()))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let stale = db.list_stale_pending_artifacts(&cutoff).await?;
    let mut n = 0u64;
    for row in stale {
        let dest = artifacts_dir.join(&row.storage_path);
        let _ = std::fs::remove_file(&dest);
        let _ = std::fs::remove_file(dest.with_extension("part"));
        let _ = std::fs::remove_dir_all(dest.with_extension("blocks"));
        if matches!(db.delete_action_artifact(&row.id).await, Ok(true)) {
            n += 1;
        }
    }
    Ok(n)
}
