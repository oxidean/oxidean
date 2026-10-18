//! Artifact store (Phase 4) — `action_artifacts` rows + payload files under
//! `OXIDEAN_ACTIONS_ARTIFACTS_DIR` (default `var/actions-artifacts`).
//!
//! Runner-proto endpoints (runner-token authed like fetch_task/update_task):
//!   POST /artifacts/upload?job_id={id}&name={n}  (raw tar.gz body, streamed)
//!   GET  /artifacts/{run_id}/{name}              (raw tar.gz body back)
//!   POST /artifacts/list                         ({run_id} → metadata list)

use std::path::PathBuf;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::app::AppState;

/// Artifact blob root — `OXIDEAN_ACTIONS_ARTIFACTS_DIR` wins; default
/// `var/actions-artifacts` beside the other `var/` stores.
pub fn resolve_artifact_dir() -> PathBuf {
    std::env::var("OXIDEAN_ACTIONS_ARTIFACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/actions-artifacts"))
}

#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    pub job_id: String,
    pub name: String,
    /// Optional per-artifact retention override (days); absent → instance
    /// default (`OXIDEAN_ACTIONS_ARTIFACT_RETENTION_DAYS`).
    pub retention_days: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactMeta {
    id: String,
    run_id: String,
    job_id: Option<String>,
    name: String,
    size_bytes: i64,
    digest_sha256: String,
    expires_at: Option<String>,
    created_at: String,
}

fn meta(a: &oxidean_db::actions::ActionArtifactRow) -> ArtifactMeta {
    ArtifactMeta {
        id: a.id.clone(),
        run_id: a.run_id.clone(),
        job_id: a.job_id.clone(),
        name: a.name.clone(),
        size_bytes: a.size_bytes,
        digest_sha256: a.digest_sha256.clone(),
        expires_at: a.expires_at.clone(),
        created_at: a.created_at.clone(),
    }
}

/// POST /artifacts/upload — stream the tar.gz payload to disk, then record
/// it. Same (run, name) re-uploads overwrite (GitHub v4 semantics).
pub async fn upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<UploadQuery>,
    body: Body,
) -> Result<axum::Json<ArtifactMeta>, StatusCode> {
    let runner = super::runner_proto::runner_from_headers_pub(&state, &headers).await?;
    let job = state
        .db
        .find_action_job_by_id(&q.job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    // The uploading job must belong to the calling runner.
    if job.runner_id.as_deref() != Some(runner.id.as_str()) {
        return Err(StatusCode::FORBIDDEN);
    }
    if q.name.is_empty() || q.name.contains('/') || q.name.contains("..") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let run = state
        .db
        .find_action_run_by_id(&job.run_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Re-upload overwrites: reuse the artifact id when one exists so the
    // storage path stays stable.
    let existing = state
        .db
        .find_action_artifact(&job.run_id, &q.name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let artifact_id = existing
        .as_ref()
        .map(|a| a.id.clone())
        .unwrap_or_else(|| format!("art-{}", Uuid::new_v4()));
    let rel = format!("{}/{artifact_id}.tgz", job.run_id);
    let dest = state.actions_artifacts_dir.join(&rel);
    let tmp = dest.with_extension("part");
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    // Remove the previous blob once the new one lands.
    let old_path = existing
        .as_ref()
        .map(|a| state.actions_artifacts_dir.join(&a.storage_path));

    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut stream = body.into_data_stream();
    let mut hasher = Sha256::new();
    let mut size: u64 = 0;
    let max = state.actions_artifact_max_bytes;
    let mut too_big = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| StatusCode::BAD_REQUEST)?;
        size += chunk.len() as u64;
        if size > max {
            too_big = true;
            break;
        }
        hasher.update(&chunk);
        if file.write_all(&chunk).await.is_err() {
            let _ = std::fs::remove_file(&tmp);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
    if too_big {
        let _ = std::fs::remove_file(&tmp);
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    drop(file);
    std::fs::rename(&tmp, &dest).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(old) = old_path.filter(|p| p != &dest) {
        let _ = std::fs::remove_file(old);
    }

    // A `0` retention means "keep indefinitely" (no expiry row value).
    let expires_at = match q.retention_days {
        Some(0) => None,
        Some(d) => Some(chrono::Utc::now() + chrono::Duration::days(i64::from(d))),
        None => Some(
            chrono::Utc::now()
                + chrono::Duration::days(i64::from(state.actions_artifact_retention_days)),
        ),
    };
    let row = oxidean_db::actions::ActionArtifactRow {
        id: artifact_id,
        repository_id: run.repository_id.clone(),
        run_id: job.run_id.clone(),
        job_id: Some(job.id.clone()),
        name: q.name.clone(),
        storage_path: rel,
        size_bytes: size as i64,
        digest_sha256: hex::encode(hasher.finalize()),
        expires_at: expires_at.map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        created_at: String::new(),
    };
    state
        .db
        .upsert_action_artifact(&row)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let stored = state
        .db
        .find_action_artifact(&job.run_id, &q.name)
        .await
        .ok()
        .flatten()
        .unwrap_or(row);
    Ok(axum::Json(meta(&stored)))
}

/// Read-side authorization: the runner must own a job in the run, or be
/// repository-scoped to the run's repository. Mirrors GitHub's behavior where
/// `download-artifact` reaches other jobs' artifacts in the same run.
async fn authorize_run_read(
    state: &AppState,
    runner: &oxidean_db::ActionRunnerRow,
    run_id: &str,
) -> Result<(), StatusCode> {
    if let Some(repo) = runner.repository_id.as_deref() {
        let run = state
            .db
            .find_action_run_by_id(run_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
        return if run.repository_id == repo {
            Ok(())
        } else {
            Err(StatusCode::FORBIDDEN)
        };
    }
    let jobs = state
        .db
        .list_action_jobs_for_run(run_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if jobs.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    if jobs
        .iter()
        .any(|j| j.runner_id.as_deref() == Some(runner.id.as_str()))
    {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// GET /artifacts/{run_id}/{name} — stream the stored tar.gz back.
pub async fn download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((run_id, name)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let runner = super::runner_proto::runner_from_headers_pub(&state, &headers).await?;
    authorize_run_read(&state, &runner, &run_id).await?;
    let row = state
        .db
        .find_action_artifact(&run_id, &name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let path = state.actions_artifacts_dir.join(&row.storage_path);
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let mut resp = Response::new(Body::from_stream(stream));
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        "application/gzip".parse().unwrap(),
    );
    Ok(resp)
}

#[derive(Debug, Deserialize)]
pub struct ListRequest {
    pub run_id: String,
}

/// POST /artifacts/list — every artifact metadata row for a run.
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<ListRequest>,
) -> Result<axum::Json<Vec<ArtifactMeta>>, StatusCode> {
    let runner = super::runner_proto::runner_from_headers_pub(&state, &headers).await?;
    authorize_run_read(&state, &runner, &req.run_id).await?;
    let rows = state
        .db
        .list_action_artifacts_for_run(&req.run_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(axum::Json(rows.iter().map(meta).collect()))
}
