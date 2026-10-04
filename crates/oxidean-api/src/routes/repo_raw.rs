//! Raw blob + source archive HTTP — GIT-05 / GIT-07 / D-17 / D-29.
//!
//! Cookie session ACL via [`crate::repo::resolve_repo_for_read`]. Not RPC JSON (large bytes).

use std::path::{Component, Path};

use axum::extract::{Path as AxumPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use oxidean_git::ArchiveFormat;

use crate::app::AppState;
use crate::auth::session::SESSION_COOKIE_NAME;
use crate::git::bare_repo_path;
use crate::repo::{self, BLOB_SOFT_MAX_BYTES};
use crate::rpc::RpcCtx;

fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let part = part.trim();
        let prefix = format!("{SESSION_COOKIE_NAME}=");
        if let Some(value) = part.strip_prefix(prefix.as_str()) {
            return Some(value.to_string());
        }
    }
    None
}

fn err_json(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "ok": false,
            "error": { "code": code, "message": message }
        })),
    )
        .into_response()
}

fn validate_ref(ref_name: &str) -> Result<&str, Response> {
    let t = ref_name.trim();
    // Allow `/` for hierarchical branches (WR-02); reject leading `-` / `..` / NUL / metachar (CR-01).
    if t.is_empty() || t.contains('\0') || t.contains("..") || t.starts_with('-') {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "repo.invalid_ref",
            "invalid ref",
        ));
    }
    if t.chars().any(|c| {
        matches!(
            c,
            ';' | '|' | '&' | '`' | '$' | '(' | ')' | '<' | '>' | '\n' | '\r' | ' '
        )
    }) {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "repo.invalid_ref",
            "invalid ref",
        ));
    }
    Ok(t)
}

/// Treeish for archives — allow `/` in branch names (e.g. `feature/x`), reject `..` / NUL / leading `-` (CR-01).
fn validate_archive_treeish(treeish: &str) -> Result<&str, Response> {
    let t = treeish.trim();
    if t.is_empty() || t.contains('\0') || t.contains("..") || t.starts_with('-') {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "repo.invalid_ref",
            "invalid ref",
        ));
    }
    if t.chars().any(|c| {
        matches!(
            c,
            ';' | '|' | '&' | '`' | '$' | '(' | ')' | '<' | '>' | '\n' | '\r' | ' '
        )
    }) {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "repo.invalid_ref",
            "invalid ref",
        ));
    }
    Ok(t)
}

fn validate_blob_path(path: &str) -> Result<String, Response> {
    let rel = path.trim().trim_start_matches('/');
    if rel.is_empty() || rel.contains('\0') {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "repo.invalid_path",
            "invalid path",
        ));
    }
    let candidate = Path::new(rel);
    if candidate.is_absolute()
        || candidate.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "repo.invalid_path",
            "path must not contain ..",
        ));
    }
    Ok(rel.to_string())
}

/// Parse `main.zip` / `main.tar.gz` / `feature/x.tar.gz` into (treeish, format).
fn parse_archive_filename(name: &str) -> Result<(&str, ArchiveFormat), Response> {
    let name = name.trim().trim_start_matches('/');
    if name.is_empty() {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "repo.invalid_archive",
            "invalid archive path",
        ));
    }
    if let Some(stem) = name.strip_suffix(".tar.gz") {
        let treeish = validate_archive_treeish(stem)?;
        return Ok((treeish, ArchiveFormat::TarGz));
    }
    if let Some(stem) = name.strip_suffix(".zip") {
        let treeish = validate_archive_treeish(stem)?;
        return Ok((treeish, ArchiveFormat::Zip));
    }
    Err(err_json(
        StatusCode::BAD_REQUEST,
        "repo.invalid_archive",
        "archive must end in .zip or .tar.gz",
    ))
}

async fn build_ctx(state: &AppState, headers: &HeaderMap) -> RpcCtx {
    let token = session_token_from_headers(headers);
    let client = crate::rpc::ClientMeta::from_headers(headers);
    let session = match token.as_deref() {
        Some(t) => match state
            .sessions
            .resolve(
                &state.db,
                t,
                client.ip_address.as_deref(),
                client.user_agent.as_deref(),
            )
            .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "session resolve failed on raw");
                None
            }
        },
        None => None,
    };
    let email = state.current_email();
    RpcCtx {
        db: state.db.clone(),
        email: email.clone(),
        email_slot: state.email.clone(),
        sessions: state.sessions.clone(),
        uploads_dir: state.uploads_dir.clone(),
        repos_dir: state.repos_dir.clone(),
        lfs_dir: state.lfs_dir.clone(),
        release_assets_dir: state.release_assets_dir.clone(),
        template_packs_dir: state.template_packs_dir.clone(),
        actions_log_dir: state.actions_log_dir.clone(),
        git: state.git.clone(),
        env_name: state.env_name.clone(),
        session,
        pat: None,
        client,
        set_cookie: None,
        lookup_limiter: state.lookup_limiter.clone(),
        search_timeout_ms: state.search_timeout_ms,
        search_max_matches: state.search_max_matches,
        search_max_files: state.search_max_files,
    }
}

/// `GET /api/repos/{owner}/{repo}/raw/{ref}/{*path}`
pub async fn serve_raw(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((owner, repo_name, ref_name, path)): AxumPath<(String, String, String, String)>,
) -> Response {
    let ref_name = match validate_ref(&ref_name) {
        Ok(r) => r.to_string(),
        Err(r) => return r,
    };
    let path = match validate_blob_path(&path) {
        Ok(p) => p,
        Err(r) => return r,
    };

    let ctx = build_ctx(&state, &headers).await;
    let accessible = match repo::resolve_repo_for_read(&ctx, &owner, &repo_name).await {
        Ok(a) => a,
        Err(e) if e.code == "repo.not_found" => {
            return err_json(StatusCode::NOT_FOUND, &e.code, &e.message);
        }
        Err(e) => {
            return err_json(StatusCode::BAD_REQUEST, &e.code, &e.message);
        }
    };

    let bare = match bare_repo_path(
        &state.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    ) {
        Ok(p) => p,
        Err(e) => return err_json(StatusCode::BAD_REQUEST, &e.code, &e.message),
    };

    let bytes = match state.git.cat_blob(&bare, &ref_name, &path).await {
        Ok(b) => b,
        Err(oxidean_git::GitError::NotFound(_)) => {
            return err_json(
                StatusCode::NOT_FOUND,
                "repo.path_not_found",
                "file not found",
            );
        }
        Err(oxidean_git::GitError::InvalidArg(msg)) => {
            return err_json(StatusCode::BAD_REQUEST, "repo.invalid_ref", &msg);
        }
        Err(e) => {
            tracing::error!(error = %e, "raw cat_blob failed");
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repo.git_failed",
                "git operation failed",
            );
        }
    };

    let truncated = bytes.len() > BLOB_SOFT_MAX_BYTES;
    let body = if truncated {
        bytes[..BLOB_SOFT_MAX_BYTES].to_vec()
    } else {
        bytes
    };

    let mut res = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/octet-stream")],
        body,
    )
        .into_response();

    if truncated {
        if let Ok(v) = HeaderValue::from_str(&BLOB_SOFT_MAX_BYTES.to_string()) {
            res.headers_mut()
                .insert("x-oxidean-blob-truncated", HeaderValue::from_static("1"));
            res.headers_mut().insert("x-oxidean-blob-soft-max", v);
        }
    }
    res
}

/// `GET /api/repos/{owner}/{repo}/archive/{*archive_file}` — e.g. `main.zip`, `main.tar.gz`.
pub async fn serve_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((owner, repo_name, archive_file)): AxumPath<(String, String, String)>,
) -> Response {
    let (treeish, format) = match parse_archive_filename(&archive_file) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let treeish = treeish.to_string();

    let ctx = build_ctx(&state, &headers).await;
    let accessible = match repo::resolve_repo_for_read(&ctx, &owner, &repo_name).await {
        Ok(a) => a,
        Err(e) if e.code == "repo.not_found" => {
            return err_json(StatusCode::NOT_FOUND, &e.code, &e.message);
        }
        Err(e) => {
            return err_json(StatusCode::BAD_REQUEST, &e.code, &e.message);
        }
    };

    let bare = match bare_repo_path(
        &state.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    ) {
        Ok(p) => p,
        Err(e) => return err_json(StatusCode::BAD_REQUEST, &e.code, &e.message),
    };

    let prefix = accessible.row.name.as_str();
    let bytes = match state.git.archive(&bare, &treeish, format, prefix).await {
        Ok(b) => b,
        Err(oxidean_git::GitError::NotFound(_)) => {
            return err_json(
                StatusCode::NOT_FOUND,
                "repo.archive_unavailable",
                "This repository has no commits yet. Push a commit before downloading an archive.",
            );
        }
        Err(oxidean_git::GitError::InvalidArg(msg)) => {
            return err_json(StatusCode::BAD_REQUEST, "repo.invalid_ref", &msg);
        }
        Err(e) => {
            tracing::error!(error = %e, "git archive failed");
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repo.git_failed",
                "git archive failed",
            );
        }
    };

    let filename = format!(
        "{}-{}.{}",
        accessible.row.name,
        treeish.replace('/', "-"),
        format.extension()
    );
    let disposition = format!("attachment; filename=\"{filename}\"");

    let mut res = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, format.content_type())
        .body(axum::body::Body::from(bytes))
        .unwrap_or_else(|_| err_json(StatusCode::INTERNAL_SERVER_ERROR, "repo.git_failed", "response build failed"));

    if let Ok(v) = HeaderValue::from_str(&disposition) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    res
}

#[cfg(test)]
mod validate_ref_tests {
    use super::validate_ref;

    #[test]
    fn validate_ref_rejects_leading_hyphen() {
        assert!(
            validate_ref("-D").is_err(),
            "leading hyphen must be repo.invalid_ref (CR-01 defense-in-depth)"
        );
        assert!(validate_ref("  --output=/tmp/x  ").is_err());
    }

    #[test]
    fn validate_ref_allows_hierarchical_slashy_names() {
        let ok = validate_ref("feature/x").expect("slashy branch names must be allowed (WR-02)");
        assert_eq!(ok, "feature/x");
    }

    #[test]
    fn validate_ref_still_rejects_dotdot_and_empty() {
        assert!(validate_ref("").is_err());
        assert!(validate_ref("a..b").is_err());
        assert!(validate_ref("main\0evil").is_err());
    }
}
