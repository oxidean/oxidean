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

/// Split a job log stream on `##[step]`/`##[step-done]` markers into GitHub's
/// per-step archive shape — `(preamble, steps, tail)`. Marker lines are
/// stripped; content outside any step lands in the preamble or tail.
fn split_step_sections(log: &str) -> (String, Vec<(String, String)>, String) {
    let mut preamble = String::new();
    let mut tail = String::new();
    let mut steps: Vec<(String, String)> = Vec::new();
    let mut current: Option<usize> = None;
    for line in log.lines() {
        if let Some(name) = line.strip_prefix("##[step]") {
            steps.push((name.trim().to_string(), String::new()));
            current = Some(steps.len() - 1);
            continue;
        }
        if line.starts_with("##[step-done]") {
            current = None;
            continue;
        }
        match current {
            Some(i) => {
                steps[i].1.push_str(line);
                steps[i].1.push('\n');
            }
            None if steps.is_empty() => {
                preamble.push_str(line);
                preamble.push('\n');
            }
            None => {
                tail.push_str(line);
                tail.push('\n');
            }
        }
    }
    (preamble, steps, tail)
}

/// GET /api/repos/{owner}/{repo}/actions/runs/{run_id}/logs — every job log
/// in the run as a zip archive. Entries follow GitHub's layout: one
/// `{job_name}/` folder per job holding `{seq}_{step_name}.txt` per step
/// (runner preamble → `0_setup.txt`, trailing lines → `N_teardown.txt`).
/// Logs with no step markers (skipped legs, validation failures) keep the
/// flat `{job_name}.txt` entry.
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
        let archive_failed = || {
            err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "actions.internal",
                "archive build failed",
            )
        };
        for job in &jobs {
            let (bytes, _size) =
                crate::actions::logs::read_job_log(&state.actions_log_dir, &run.id, &job.id, 0)
                    .await
                    .unwrap_or_default();
            // Unique, filesystem-safe entry names — matrix siblings share a name.
            let mut dir = sanitize_filename(&job.name);
            for n in 2.. {
                if used.insert(dir.clone()) {
                    break;
                }
                dir = format!("{} ({n})", sanitize_filename(&job.name));
            }
            let text = String::from_utf8_lossy(&bytes);
            let (preamble, steps, tail) = split_step_sections(&text);
            let mut entries: Vec<(String, &[u8])> = Vec::new();
            if steps.is_empty() {
                // No step markers — skipped/validation-failure legs keep the
                // flat `{job}.txt` entry.
                entries.push((format!("{dir}.txt"), &bytes));
            } else {
                if !preamble.trim().is_empty() {
                    entries.push((format!("{dir}/0_setup.txt"), preamble.as_bytes()));
                }
                for (idx, (name, body)) in steps.iter().enumerate() {
                    entries.push((
                        format!("{dir}/{}_{}.txt", idx + 1, sanitize_filename(name)),
                        body.as_bytes(),
                    ));
                }
                if !tail.trim().is_empty() {
                    entries.push((
                        format!("{dir}/{}_teardown.txt", steps.len() + 1),
                        tail.as_bytes(),
                    ));
                }
            }
            for (entry, content) in entries {
                if zip.start_file(entry, opts).is_err() || zip.write_all(content).is_err() {
                    return archive_failed();
                }
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
