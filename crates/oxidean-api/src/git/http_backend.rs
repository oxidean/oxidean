//! Spawn `git-http-backend` CGI for Smart HTTP (D-18 / D-22 / T-08-07).

use std::path::{Path, PathBuf};
use std::process::Stdio;

use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::Response;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Resolve `git-http-backend` via `git --exec-path`, with well-known fallback.
pub fn resolve_git_http_backend() -> PathBuf {
    if let Ok(out) = std::process::Command::new("git")
        .args(["--exec-path"])
        .output()
    {
        if out.status.success() {
            let exec = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let candidate = PathBuf::from(&exec).join("git-http-backend");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from("/usr/lib/git-core/git-http-backend")
}

/// Inputs for a single CGI invocation.
pub struct CgiRequest<'a> {
    pub repos_dir: &'a Path,
    /// e.g. `/owner/repo.git/info/refs`
    pub path_info: &'a str,
    pub method: &'a str,
    pub query_string: &'a str,
    pub content_type: Option<&'a str>,
    pub body: &'a [u8],
    pub remote_user: Option<&'a str>,
    pub git_protocol: Option<&'a str>,
    /// Phase 13: pass-through env for bare-repo protection hooks (D-19).
    pub protection_env: Option<&'a ProtectionCgiEnv<'a>>,
}

/// Env forwarded to git-http-backend so `hooks/update` can evaluate protection.
pub struct ProtectionCgiEnv<'a> {
    pub database_url: &'a str,
    pub actor_capability: &'a str,
    pub helper_path: Option<&'a str>,
    /// Re-injected after `env_clear` so update hooks can see production vs compose (D-PKG-01).
    pub oxidean_env: Option<&'a str>,
    /// `OXIDEAN_GIT_REPO_QUOTA_BYTES` pass-through so the update hook sees the
    /// configured instance quota default (env_clear strips it otherwise, GIT-25).
    pub git_repo_quota_bytes: Option<&'a str>,
}

/// Run git-http-backend and map CGI stdout to an Axum [`Response`].
pub async fn run_git_http_backend(req: CgiRequest<'_>) -> Result<Response, String> {
    let backend = resolve_git_http_backend();
    if !backend.is_file() {
        return Err(format!(
            "git-http-backend not found at {}",
            backend.display()
        ));
    }

    let mut cmd = Command::new(&backend);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()))
        .env("GIT_PROJECT_ROOT", req.repos_dir)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("PATH_INFO", req.path_info)
        .env("REQUEST_METHOD", req.method)
        .env("QUERY_STRING", req.query_string)
        .env("CONTENT_LENGTH", req.body.len().to_string())
        .env("GATEWAY_INTERFACE", "CGI/1.1")
        .env("REMOTE_ADDR", "127.0.0.1")
        // GIT-27: advertise partial-clone filtering (`filter` capability) on
        // upload-pack. Env config (vs per-repo `git config`) applies uniformly
        // to repos created before this flag existed, and `uploadpack.*` keys
        // are inert when the CGI dispatches receive-pack instead.
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "uploadpack.allowFilter")
        .env("GIT_CONFIG_VALUE_0", "true");

    if let Some(ct) = req.content_type {
        cmd.env("CONTENT_TYPE", ct);
    }
    if let Some(user) = req.remote_user {
        cmd.env("REMOTE_USER", user);
    }
    if let Some(proto) = req.git_protocol {
        cmd.env("GIT_PROTOCOL", proto);
    }
    if let Some(pe) = req.protection_env {
        cmd.env("OXIDEAN_REPOS_DIR", req.repos_dir);
        cmd.env("OXIDEAN_DATABASE_URL", pe.database_url);
        cmd.env("OXIDEAN_ACTOR_CAPABILITY", pe.actor_capability);
        if let Some(helper) = pe.helper_path {
            cmd.env("OXIDEAN_PROTECTION_HELPER", helper);
        }
        if let Some(env_name) = pe.oxidean_env {
            cmd.env("OXIDEAN_ENV", env_name);
        }
        if let Some(quota) = pe.git_repo_quota_bytes.filter(|s| !s.is_empty()) {
            cmd.env("OXIDEAN_GIT_REPO_QUOTA_BYTES", quota);
        }
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn git-http-backend: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(req.body)
            .await
            .map_err(|e| format!("cgi stdin write failed: {e}"))?;
        drop(stdin);
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("cgi wait failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!(
            status = ?output.status,
            stderr = %stderr,
            "git-http-backend exited non-zero"
        );
        // Still try to parse CGI headers if present; otherwise 500.
    }

    parse_cgi_response(&output.stdout)
}

fn parse_cgi_response(stdout: &[u8]) -> Result<Response, String> {
    // Split headers / body on blank line (\n\n or \r\n\r\n).
    let sep = find_header_body_sep(stdout).ok_or_else(|| {
        format!(
            "cgi response missing header terminator ({} bytes)",
            stdout.len()
        )
    })?;
    let header_bytes = &stdout[..sep.0];
    let body = stdout[sep.1..].to_vec();

    let header_str = String::from_utf8_lossy(header_bytes);
    let mut status = StatusCode::OK;
    let mut headers = HeaderMap::new();

    for line in header_str.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("Status:") {
            let code = rest.trim().split_whitespace().next().unwrap_or("200");
            status = StatusCode::from_u16(code.parse().unwrap_or(200))
                .unwrap_or(StatusCode::OK);
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim();
            let value = value.trim();
            if name.eq_ignore_ascii_case("Status") {
                continue;
            }
            if let (Ok(hn), Ok(hv)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_str(value),
            ) {
                headers.insert(hn, hv);
            }
        }
    }

    let mut response = Response::new(axum::body::Body::from(body));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    Ok(response)
}

/// Returns (header_end_exclusive, body_start).
fn find_header_body_sep(buf: &[u8]) -> Option<(usize, usize)> {
    for i in 0..buf.len().saturating_sub(3) {
        if buf[i] == b'\r' && buf[i + 1] == b'\n' && buf[i + 2] == b'\r' && buf[i + 3] == b'\n' {
            return Some((i, i + 4));
        }
    }
    for i in 0..buf.len().saturating_sub(1) {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some((i, i + 2));
        }
    }
    None
}
