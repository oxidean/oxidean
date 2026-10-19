//! Pages site serving — `GET /pages/{owner}/{repo}/{*path}` reads the
//! materialized site tree under `OXIDEAN_PAGES_DIR/{repo_id}`.
//!
//! ACL: `repo.resolve_for_read` — public sites are anonymous-readable, private
//! repos require a session with Read+. This is deliberately stricter than
//! GitHub (which serves private-repo sites publicly) — a private repo must
//! not leak its site.
//!
//! Same-origin hardening (GitHub serves Pages off a separate `*.github.io`
//! origin): `text/html` responses carry `Content-Security-Policy: sandbox
//! allow-scripts` — the document renders in an opaque origin so user HTML/JS
//! cannot ride session cookies to `/api/rpc` or touch parent-origin storage.

use std::path::{Component, Path, PathBuf};

use axum::extract::{Path as AxumPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::app::AppState;
use crate::auth::session::SESSION_COOKIE_NAME;
use crate::repo::resolve_repo_for_read;
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
                tracing::warn!(error = %e, "session resolve failed on pages");
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
        actions_artifacts_dir: state.actions_artifacts_dir.clone(),
        actions_cache_dir: state.actions_cache_dir.clone(),
        pages_dir: state.pages_dir.clone(),
        git: state.git.clone(),
        env_name: state.env_name.clone(),
        session,
        pat: None,
        client,
        set_cookie: None,
        two_factor: crate::auth::two_factor::TwoFactorCtx {
            store: state.two_factor_store.clone(),
            challenge_id: None,
        },
        login_begin_limiter: state.login_begin_limiter.clone(),
        lookup_limiter: state.lookup_limiter.clone(),
        search_timeout_ms: state.search_timeout_ms,
        search_max_matches: state.search_max_matches,
        search_max_files: state.search_max_files,
    }
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Not Found").into_response()
}

/// `rel` → a path inside `root`, or `None` on `..`/absolute escapes.
fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute()
        || rel_path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    Some(root.join(rel_path))
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("xml") => "application/xml",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        Some("csv") => "text/csv; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("gz" | "tgz") => "application/gzip",
        Some("mp4" | "m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mp3") => "audio/mpeg",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

async fn serve_file(path: &Path, status: StatusCode) -> Response {
    let file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return not_found(),
    };
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file));
    let ct = content_type(path);
    let mut resp = (status, body).into_response();
    let headers = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(ct) {
        headers.insert(header::CONTENT_TYPE, v);
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    if ct.starts_with("text/html")
        || ct.starts_with("image/svg")
        || ct.starts_with("application/xml")
    {
        // Opaque-origin sandbox — see the module doc. `allow-scripts` keeps
        // GitHub Pages' JS-support parity; `allow-same-origin` stays OFF so
        // the scripts cannot reach app cookies/storage. SVG/XML get it too —
        // a directly-navigated XML doc can execute embedded script.
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox allow-scripts"),
        );
    }
    resp
}

/// `Redirect` panics on control chars (a `%0a` path segment reaches it
/// decoded) — fall back to 404 instead of unwinding a public request.
fn redirect_or_404(location: String) -> Response {
    match HeaderValue::from_str(&location) {
        Ok(v) => {
            let mut resp = StatusCode::PERMANENT_REDIRECT.into_response();
            resp.headers_mut().insert(header::LOCATION, v);
            resp
        }
        Err(_) => not_found(),
    }
}

/// `GET /pages/{owner}/{repo}` — GitHub Pages redirects the bare site URL to
/// the trailing-slash root.
pub async fn redirect_root(
    State(_state): State<AppState>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
) -> Response {
    redirect_or_404(format!("/pages/{owner}/{repo}/"))
}

/// `GET /pages/{owner}/{repo}/{*path}` — serve a file off the built site.
/// Empty/dir paths resolve `index.html`; misses fall back to `404.html`
/// (GitHub's custom-404 behavior) before a bare 404.
pub async fn serve(
    State(state): State<AppState>,
    AxumPath((owner, repo, path)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    serve_path(state, owner, repo, path, headers).await
}

/// `GET /pages/{owner}/{repo}/` — axum's `{*path}` does not match the empty
/// remainder, so the site root gets its own route.
pub async fn serve_index(
    State(state): State<AppState>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
    headers: HeaderMap,
) -> Response {
    serve_path(state, owner, repo, String::new(), headers).await
}

async fn serve_path(
    state: AppState,
    owner: String,
    repo: String,
    path: String,
    headers: HeaderMap,
) -> Response {
    let ctx = build_ctx(&state, &headers).await;
    let accessible = match resolve_repo_for_read(&ctx, &owner, &repo).await {
        Ok(a) => a,
        Err(_) => return not_found(),
    };
    let Ok(site) = ctx.db.find_repo_pages(&accessible.row.id).await else {
        return not_found();
    };
    if site.is_none() {
        return not_found();
    }
    let root = state.pages_dir.join(&accessible.row.id);
    let rel = path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    let Some(candidate) = safe_join(&root, rel) else {
        return not_found();
    };
    // Directory URL without a trailing slash → redirect like GitHub Pages.
    if candidate.is_dir() && !path.ends_with('/') {
        return redirect_or_404(format!("/pages/{owner}/{repo}/{path}/"));
    }
    let file = if candidate.is_dir() {
        candidate.join("index.html")
    } else {
        candidate
    };
    if file.is_file() {
        return serve_file(&file, StatusCode::OK).await;
    }
    let custom_404 = root.join("404.html");
    if custom_404.is_file() {
        return serve_file(&custom_404, StatusCode::NOT_FOUND).await;
    }
    not_found()
}
