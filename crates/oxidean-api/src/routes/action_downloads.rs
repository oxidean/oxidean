//! Browser-facing Actions downloads — artifact archives and raw job logs.
//!
//! These are session/PAT-authed GETs (unlike the runner-token
//! `/api/actions/artifacts/*` routes) so the web UI can offer download links
//! and `ox` can fetch logs. Capability rule mirrors `repo.actions.*` RPCs:
//! Read+ on the repository (public repos allow anonymous reads, private repos
//! 404 for unauthorized callers).

use std::io::Write as _;

use axum::extract::{Path as AxumPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app::AppState;
use crate::repo::{effective_capability, meets, resolve_owner_slug, Capability};

use super::release_assets::{resolve_session_user_id, sanitize_filename};

fn err_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "ok": false,
            "error": { "code": code, "message": message }
        })),
    )
        .into_response()
}

fn pat_err_response(e: &oxidean_core::AppError) -> Response {
    let status = if e.code.ends_with(".not_found") {
        StatusCode::NOT_FOUND
    } else if e.code == "auth.pat_scope" {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    err_response(status, &e.code, &e.message)
}

pub(crate) struct RepoLookup {
    pub repo: oxidean_db::repositories::RepositoryRow,
    pub owner_ref: crate::repo::OwnerRef,
}

/// `{owner}/{repo}` → repo row + owner ref (404-shaped errors).
pub(crate) async fn lookup_repo(
    state: &AppState,
    owner: &str,
    repo: &str,
) -> Result<RepoLookup, Box<Response>> {
    let owner_ref = resolve_owner_slug(&state.db, owner).await.map_err(|e| {
        tracing::error!(error = %e, "resolve_owner_slug");
        Box::new(err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "repo.internal",
            "lookup failed",
        ))
    })?;
    let Some(owner_ref) = owner_ref else {
        return Err(Box::new(err_response(
            StatusCode::NOT_FOUND,
            "repo.not_found",
            "repository not found",
        )));
    };
    let repo = state
        .db
        .find_repository_by_owner_name(owner_ref.id(), repo)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "find_repository_by_owner_name");
            Box::new(err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repo.internal",
                "lookup failed",
            ))
        })?;
    let Some(repo) = repo else {
        return Err(Box::new(err_response(
            StatusCode::NOT_FOUND,
            "repo.not_found",
            "repository not found",
        )));
    };
    Ok(RepoLookup { repo, owner_ref })
}

/// Session/PAT credential + Read capability gate for Actions downloads.
pub(crate) async fn authorize_read(
    state: &AppState,
    headers: &HeaderMap,
    lookup: &RepoLookup,
    pat_proc: &'static str,
) -> Result<(), Box<Response>> {
    let caller = resolve_session_user_id(state, headers).await;
    if let Some((_, Some(pat))) = &caller {
        if let Err(e) = crate::pat::bearer::authorize_pat_on_repo(
            &state.db,
            pat,
            pat_proc,
            &lookup.repo,
            &lookup.owner_ref,
        )
        .await
        {
            return Err(Box::new(pat_err_response(&e)));
        }
    }
    let capability = effective_capability(
        &state.db,
        caller.as_ref().map(|(id, _)| id.as_str()),
        &lookup.repo,
        &lookup.owner_ref,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "effective_capability actions download");
        Box::new(err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "repo.internal",
            "lookup failed",
        ))
    })?;
    if !meets(capability, Capability::Read) {
        return Err(Box::new(err_response(
            StatusCode::NOT_FOUND,
            "repo.not_found",
            "repository not found",
        )));
    }
    Ok(())
}

/// GET /api/repos/{owner}/{repo}/actions/artifacts/{artifact_id} — stream the
/// stored tar.gz archive with an attachment disposition.
pub async fn download_artifact(
    State(state): State<AppState>,
    AxumPath((owner, repo, artifact_id)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let lookup = match lookup_repo(&state, &owner, &repo).await {
        Ok(l) => l,
        Err(r) => return *r,
    };
    if let Err(r) = authorize_read(&state, &headers, &lookup, "repo.actions.getRun").await {
        return *r;
    }
    let artifact = match state.db.find_action_artifact_by_id(&artifact_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return err_response(
                StatusCode::NOT_FOUND,
                "actions.artifact_not_found",
                "Artifact not found",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "find artifact");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "actions.internal",
                "download failed",
            );
        }
    };
    if artifact.repository_id != lookup.repo.id {
        return err_response(
            StatusCode::NOT_FOUND,
            "actions.artifact_not_found",
            "Artifact not found",
        );
    }
    let path = state.actions_artifacts_dir.join(&artifact.storage_path);
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(_) => {
            return err_response(
                StatusCode::NOT_FOUND,
                "actions.artifact_not_found",
                "Artifact not found",
            );
        }
    };
    let mut res = Response::new(axum::body::Body::from(bytes));
    *res.status_mut() = StatusCode::OK;
    if let Ok(hv) = HeaderValue::from_str("application/gzip") {
        res.headers_mut().insert(header::CONTENT_TYPE, hv);
    }
    let disp = format!(
        "attachment; filename=\"{}.tar.gz\"",
        sanitize_filename(&artifact.name)
    );
    if let Ok(hv) = HeaderValue::from_str(&disp) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, hv);
    }
    res
}

/// GET /api/repos/{owner}/{repo}/actions/jobs/{job_id}/logs — the job's raw
/// log as `text/plain` (transparently serves the gzip-at-rest form for
/// finished runs).
pub async fn download_job_log(
    State(state): State<AppState>,
    AxumPath((owner, repo, job_id)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let lookup = match lookup_repo(&state, &owner, &repo).await {
        Ok(l) => l,
        Err(r) => return *r,
    };
    if let Err(r) = authorize_read(&state, &headers, &lookup, "repo.actions.getJobLog").await {
        return *r;
    }
    let job = match state.db.find_action_job_by_id(&job_id).await {
        Ok(Some(j)) => j,
        Ok(None) => {
            return err_response(
                StatusCode::NOT_FOUND,
                "actions.job_not_found",
                "Job not found",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "find job");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "actions.internal",
                "download failed",
            );
        }
    };
    let run = match state.db.find_action_run_by_id(&job.run_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return err_response(
                StatusCode::NOT_FOUND,
                "actions.job_not_found",
                "Job not found",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "find run for job");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "actions.internal",
                "download failed",
            );
        }
    };
    if run.repository_id != lookup.repo.id {
        return err_response(
            StatusCode::NOT_FOUND,
            "actions.job_not_found",
            "Job not found",
        );
    }
    let (bytes, _size) =
        crate::actions::logs::read_job_log(&state.actions_log_dir, &run.id, &job.id, 0)
            .await
            .unwrap_or_default();
    let mut res = Response::new(axum::body::Body::from(bytes));
    *res.status_mut() = StatusCode::OK;
    if let Ok(hv) = HeaderValue::from_str("text/plain; charset=utf-8") {
        res.headers_mut().insert(header::CONTENT_TYPE, hv);
    }
    let disp = format!(
        "attachment; filename=\"{}.log\"",
        sanitize_filename(&job.name)
    );
    if let Ok(hv) = HeaderValue::from_str(&disp) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, hv);
    }
    res
}

/// GET /api/repos/{owner}/{repo}/actions/runs/{run_id}/logs — every job log
/// in the run as a zip archive (GitHub's run-logs download). Entries are one
/// `{job_name}.txt` per job — GitHub zips per step, we retain per-job streams.
pub async fn download_run_logs(
    State(state): State<AppState>,
    AxumPath((owner, repo, run_id)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let lookup = match lookup_repo(&state, &owner, &repo).await {
        Ok(l) => l,
        Err(r) => return *r,
    };
    if let Err(r) = authorize_read(&state, &headers, &lookup, "repo.actions.getJobLog").await {
        return *r;
    }
    let run = match state.db.find_action_run_by_id(&run_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return err_response(
                StatusCode::NOT_FOUND,
                "actions.run_not_found",
                "Run not found",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "find run");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "actions.internal",
                "download failed",
            );
        }
    };
    if run.repository_id != lookup.repo.id {
        return err_response(
            StatusCode::NOT_FOUND,
            "actions.run_not_found",
            "Run not found",
        );
    }
    let jobs = match state.db.list_action_jobs_for_run(&run.id).await {
        Ok(j) => j,
        Err(e) => {
            tracing::error!(error = %e, "list run jobs");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "actions.internal",
                "download failed",
            );
        }
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut used = std::collections::HashSet::new();
        for job in &jobs {
            let (bytes, _size) =
                crate::actions::logs::read_job_log(&state.actions_log_dir, &run.id, &job.id, 0)
                    .await
                    .unwrap_or_default();
            // Unique, filesystem-safe entry names — matrix siblings share a name.
            let mut entry = format!("{}.txt", sanitize_filename(&job.name));
            for n in 2.. {
                if used.insert(entry.clone()) {
                    break;
                }
                entry = format!("{} ({n}).txt", sanitize_filename(&job.name));
            }
            if zip.start_file(entry, opts).is_err() || zip.write_all(&bytes).is_err() {
                return err_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "actions.internal",
                    "archive build failed",
                );
            }
        }
        if zip.finish().is_err() {
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "actions.internal",
                "archive build failed",
            );
        }
    }
    let mut res = Response::new(axum::body::Body::from(buf.into_inner()));
    *res.status_mut() = StatusCode::OK;
    if let Ok(hv) = HeaderValue::from_str("application/zip") {
        res.headers_mut().insert(header::CONTENT_TYPE, hv);
    }
    if let Ok(hv) = HeaderValue::from_str("attachment; filename=\"logs.zip\"") {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, hv);
    }
    res
}
