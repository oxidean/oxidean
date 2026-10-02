//! Git Smart HTTP over `/{owner}/{repo}.git` — PAT Basic auth only (D-10–D-12, D-18, D-22).
//!
//! Session cookies are intentionally ignored for authorization (D-12 / T-08-02).

use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use oxidean_core::{
    ClassicPatScope, ContentsPerm, FgRepoAccess, PatKind, CLASSIC_PAT_PREFIX,
    FINE_GRAINED_PAT_PREFIX,
};
use oxidean_db::{PatRow, RepositoryRow, UserRow};
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::session::sha256_hex;
use crate::git::bare_repo_path;
use crate::git::http_backend::{self, CgiRequest};
use crate::repo::{
    effective_capability, fg_all_covers_repo, is_private_visibility, lookup_repo_row_or_redirect,
    meets, Capability, OwnerRef,
};

const WWW_AUTHENTICATE: &str = r#"Basic realm="Oxidean Git""#;
const PAT_HINT: &str =
    "Authentication failed. Use a personal access token as the password (not your account password). Create one in Settings → Personal access tokens.";

/// Username aliases accepted for Basic auth (identity still comes from the PAT) — D-10.
/// 401 responses set `WWW-Authenticate: Basic realm="Oxidean Git"`.
const USERNAME_ALIASES: &[&str] = &["git", "token", "oauth2"];

#[derive(Debug, Deserialize)]
pub struct InfoRefsQuery {
    pub service: Option<String>,
}

fn unauthorized_pat_hint() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [
            (header::WWW_AUTHENTICATE, WWW_AUTHENTICATE),
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
        ],
        PAT_HINT.to_string(),
    )
        .into_response()
}

fn unauthorized_basic() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, WWW_AUTHENTICATE)],
        "Unauthorized",
    )
        .into_response()
}

fn forbidden_insufficient_scope() -> Response {
    (
        StatusCode::FORBIDDEN,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "Insufficient personal access token scope for this git operation",
    )
        .into_response()
}

fn too_many_requests(retry_after: Duration) -> Response {
    let secs = retry_after.as_secs().max(1).to_string();
    let mut res = (
        StatusCode::TOO_MANY_REQUESTS,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "Too many failed authentication attempts. Try again later.",
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(&secs) {
        res.headers_mut().insert(header::RETRY_AFTER, v);
    }
    res
}

fn email_unverified_push() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "ok": false,
            "error": {
                "code": "auth.email_unverified",
                "message": "verify your email to continue"
            }
        })),
    )
        .into_response()
}

fn limiter_lock(
    state: &AppState,
) -> std::sync::MutexGuard<'_, crate::pat::rate_limit::FailedAuthLimiter> {
    state
        .git_auth_limiter
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Decode `Authorization: Basic …` → (username, password).
fn parse_basic(headers: &HeaderMap) -> Result<Option<(String, String)>, Response> {
    let Some(raw) = headers.get(header::AUTHORIZATION) else {
        return Ok(None);
    };
    let raw = raw.to_str().map_err(|_| unauthorized_basic())?;
    let Some(encoded) = raw
        .strip_prefix("Basic ")
        .or_else(|| raw.strip_prefix("basic "))
    else {
        return Ok(None);
    };
    let bytes = base64_lite::decode(encoded.trim()).ok_or_else(unauthorized_basic)?;
    let decoded = String::from_utf8(bytes).map_err(|_| unauthorized_basic())?;
    let (user, pass) = decoded.split_once(':').ok_or_else(unauthorized_basic)?;
    Ok(Some((user.to_string(), pass.to_string())))
}

fn looks_like_pat(password: &str) -> bool {
    password.starts_with(CLASSIC_PAT_PREFIX) || password.starts_with(FINE_GRAINED_PAT_PREFIX)
}

/// Client IP for last-used / rate-limit — rightmost `X-Forwarded-For` hop
/// (appended by a trusted proxy such as Traefik). Do not expose the API without
/// a proxy that overwrites/sanitizes forwarded headers; leftmost hops are
/// client-controlled and must not drive the failed-auth IP bucket.
fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .next_back()
        })
        .map(|s| s.to_string())
}

fn pat_expired(expires_at: &Option<String>) -> bool {
    let Some(raw) = expires_at.as_deref() else {
        return false;
    };
    match DateTime::parse_from_rfc3339(raw) {
        Ok(dt) => dt.with_timezone(&Utc) <= Utc::now(),
        Err(_) => true,
    }
}

struct AuthedPat {
    pat: PatRow,
    owner: UserRow,
}

/// Record a failed Basic/PAT attempt against IP and optional username→user bucket.
async fn record_failed_auth(state: &AppState, headers: &HeaderMap, username: Option<&str>) {
    let ip = client_ip(headers).unwrap_or_else(|| "unknown".into());
    let mut user_id: Option<String> = None;
    if let Some(name) = username {
        if !USERNAME_ALIASES
            .iter()
            .any(|a| name.eq_ignore_ascii_case(a))
        {
            if let Ok(Some(u)) = state.db.find_user_by_username(name).await {
                user_id = Some(u.id);
            }
        }
    }
    let mut lim = limiter_lock(state);
    lim.record_ip(&ip);
    if let Some(uid) = user_id {
        lim.record_user(&uid);
    }
}

/// Resolve Basic credentials to a PAT owner. Cookies are never consulted (D-12).
async fn authenticate_pat(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<AuthedPat>, Response> {
    let Some((username, password)) = parse_basic(headers)? else {
        return Ok(None);
    };
    if username.is_empty() {
        record_failed_auth(state, headers, None).await;
        return Err(unauthorized_basic());
    }

    // Per-user failed-auth gate when username maps to an account (D-26).
    if !USERNAME_ALIASES
        .iter()
        .any(|a| username.eq_ignore_ascii_case(a))
    {
        if let Ok(Some(u)) = state.db.find_user_by_username(&username).await {
            if let Err(retry) = limiter_lock(state).check_user(&u.id) {
                return Err(too_many_requests(retry));
            }
        }
    }

    if !looks_like_pat(&password) {
        record_failed_auth(state, headers, Some(&username)).await;
        return Err(unauthorized_pat_hint());
    }
    let token_hash = sha256_hex(password.as_bytes());
    let pat = match state.db.find_pat_by_token_hash(&token_hash).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            record_failed_auth(state, headers, Some(&username)).await;
            return Err(unauthorized_pat_hint());
        }
        Err(e) => {
            tracing::error!(error = %e, "find_pat_by_token_hash failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };
    if pat_expired(&pat.expires_at) {
        record_failed_auth(state, headers, Some(&username)).await;
        return Err(unauthorized_pat_hint());
    }
    let owner = match state.db.find_user_by_id(&pat.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            record_failed_auth(state, headers, Some(&username)).await;
            return Err(unauthorized_pat_hint());
        }
        Err(e) => {
            tracing::error!(error = %e, "find_user_by_id failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };
    if owner.banned_at.is_some() {
        record_failed_auth(state, headers, Some(&username)).await;
        return Err(unauthorized_pat_hint());
    }
    let user_ok = USERNAME_ALIASES
        .iter()
        .any(|a| username.eq_ignore_ascii_case(a))
        || username.eq_ignore_ascii_case(&owner.username);
    if !user_ok {
        record_failed_auth(state, headers, Some(&username)).await;
        return Err(unauthorized_basic());
    }
    // Successful auth clears user bucket only (D-26).
    limiter_lock(state).clear_user(&owner.id);
    Ok(Some(AuthedPat { pat, owner }))
}

struct ResolvedRepo {
    row: RepositoryRow,
    owner: OwnerRef,
    /// Disk path segments for CGI (current owner/name after redirect).
    disk_owner: String,
    disk_name: String,
}

async fn resolve_repo(
    state: &AppState,
    owner: &str,
    name: &str,
) -> Result<ResolvedRepo, Response> {
    let pair = match lookup_repo_row_or_redirect(&state.db, owner, name).await {
        Ok(Some(p)) => p,
        Ok(None) => return Err(unauthorized_basic()),
        Err(e) => {
            tracing::error!(error = %e, "lookup_repo_row_or_redirect");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };
    let (row, owner_ref) = pair;
    let disk_owner = owner_ref.slug().to_string();
    let disk_name = row.name.clone();
    Ok(ResolvedRepo {
        row,
        owner: owner_ref,
        disk_owner,
        disk_name,
    })
}

/// Classic `repo` / FG contents+selection checks — insufficient → 403 (D-23).
/// ACL capability is checked separately via [`effective_capability`]; this only
/// validates PAT scope/contents (T-10-13 / D-ORG-05). Classic push no longer
/// requires `pat.user_id == repositories.owner_id`.
///
/// FG All (ASSUME A4): personal-owned + org repos where subject is Owner/Admin.
async fn pat_allows_operation(
    state: &AppState,
    pat: &PatRow,
    repo: &RepositoryRow,
    owner: &OwnerRef,
    receive: bool,
) -> Result<bool, Response> {
    let kind = match PatKind::parse(&pat.kind) {
        Ok(k) => k,
        Err(_) => return Ok(false),
    };
    match kind {
        PatKind::Classic => {
            let scopes: Vec<String> =
                match pat.scopes_json.as_deref().map(serde_json::from_str) {
                    Some(Ok(v)) => v,
                    Some(Err(_)) | None => return Ok(false),
                };
            Ok(scopes
                .iter()
                .any(|s| ClassicPatScope::parse(s).ok() == Some(ClassicPatScope::Repo)))
        }
        PatKind::FineGrained => {
            let access = match pat
                .repo_access
                .as_deref()
                .and_then(|s| FgRepoAccess::parse(s).ok())
            {
                Some(a) => a,
                None => return Ok(false),
            };
            let repo_ok = match access {
                FgRepoAccess::All => match fg_all_covers_repo(&state.db, &pat.user_id, owner).await
                {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::error!(error = %e, "fg_all_covers_repo");
                        return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
                    }
                },
                FgRepoAccess::Selected => pat.repository_ids.iter().any(|id| id == &repo.id),
            };
            if !repo_ok {
                return Ok(false);
            }
            let contents = match pat
                .contents_perm
                .as_deref()
                .and_then(|s| ContentsPerm::parse(s).ok())
            {
                Some(c) => c,
                None => return Ok(false),
            };
            Ok(match contents {
                ContentsPerm::Read => !receive,
                ContentsPerm::Write => true,
            })
        }
    }
}

fn is_upload_pack_service(service: Option<&str>, path_tail: &str) -> bool {
    service == Some("git-upload-pack") || path_tail == "git-upload-pack"
}

fn is_receive_pack_service(service: Option<&str>, path_tail: &str) -> bool {
    service == Some("git-receive-pack") || path_tail == "git-receive-pack"
}

fn strip_git_suffix(repo_git: &str) -> Option<&str> {
    repo_git.strip_suffix(".git").filter(|n| !n.is_empty())
}

async fn touch_last_used(state: &AppState, pat_id: &str, headers: &HeaderMap) {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let ip = client_ip(headers);
    if let Err(e) = state
        .db
        .touch_pat_last_used(pat_id, &now, ip.as_deref())
        .await
    {
        tracing::warn!(error = %e, pat_id, "touch_pat_last_used failed");
    }
}

#[allow(clippy::too_many_arguments)]
async fn authorize_and_cgi(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo_git: &str,
    path_tail: &str,
    method: &str,
    query_string: &str,
    service: Option<&str>,
    body: &[u8],
) -> Response {
    // D-12: Cookie / oxidean_session must not authenticate Smart HTTP.
    // Reference the header so scanners see an explicit ignore pattern.
    let _oxidean_session_cookie_ignored = headers.get(header::COOKIE);

    let Some(repo_name) = strip_git_suffix(repo_git) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if bare_repo_path(&state.repos_dir, owner, repo_name).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }

    // D-26: IP failed-auth gate before Basic/PAT work.
    let ip = client_ip(headers).unwrap_or_else(|| "unknown".into());
    if let Err(retry) = limiter_lock(state).check_ip(&ip) {
        return too_many_requests(retry);
    }

    let authed = match authenticate_pat(state, headers).await {
        Ok(a) => a,
        Err(r) => return r,
    };

    let resolved = match resolve_repo(state, owner, repo_name).await {
        Ok(r) => r,
        Err(r) => return r,
    };

    let is_private = is_private_visibility(&resolved.row.visibility);
    let upload = is_upload_pack_service(service, path_tail);
    let receive = is_receive_pack_service(service, path_tail);

    if path_tail == "info/refs" && !upload && !receive {
        return (StatusCode::FORBIDDEN, "unsupported git service").into_response();
    }

    // receive-pack always requires a PAT (D-20).
    if receive && authed.is_none() {
        return unauthorized_basic();
    }
    // Private / no-access unauth → 401 (D-21), not web not_found.
    if is_private && authed.is_none() {
        return unauthorized_basic();
    }

    let mut actor_capability_label = "read";
    if let Some(ref auth) = authed {
        let caller_id = auth.owner.id.as_str();
        let capability = match effective_capability(
            &state.db,
            Some(caller_id),
            &resolved.row,
            &resolved.owner,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "effective_capability");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
        actor_capability_label = match capability {
            Some(Capability::Admin) => "admin",
            Some(Capability::Write) => "write",
            Some(Capability::Read) | None => "read",
        };
        // Private + no Read → 401 (D-21 / T-10-01), not web not_found.
        if is_private && !meets(capability, Capability::Read) {
            return unauthorized_basic();
        }
        // Push needs Write capability (ORG-04 / D-ORG-05).
        if receive && !meets(capability, Capability::Write) {
            return unauthorized_basic();
        }
        // D-24 / Open Q2: unverified may fetch; push denied with email_unverified.
        if receive && auth.owner.email_verified_at.is_none() {
            return email_unverified_push();
        }
        match pat_allows_operation(state, &auth.pat, &resolved.row, &resolved.owner, receive).await
        {
            Ok(true) => {}
            Ok(false) => return forbidden_insufficient_scope(),
            Err(r) => return r,
        }
        touch_last_used(state, &auth.pat.id, headers).await;
    }

    let path_info = format!(
        "/{}/{}.git/{}",
        resolved.disk_owner, resolved.disk_name, path_tail
    );
    let git_protocol = headers
        .get("Git-Protocol")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let remote_user = authed.as_ref().map(|a| a.owner.username.as_str());

    let db_url = std::env::var("OXIDEAN_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_default();
    let helper = crate::protection::resolve_protection_helper();
    let oxidean_env = std::env::var("OXIDEAN_ENV").ok();
    let git_repo_quota = std::env::var(crate::git::quota::REPO_QUOTA_ENV).ok();
    let protection = if receive && !db_url.is_empty() {
        Some(http_backend::ProtectionCgiEnv {
            database_url: &db_url,
            actor_capability: actor_capability_label,
            helper_path: helper.as_deref(),
            oxidean_env: oxidean_env.as_deref(),
            git_repo_quota_bytes: git_repo_quota.as_deref(),
        })
    } else {
        None
    };

    match http_backend::run_git_http_backend(CgiRequest {
        repos_dir: &state.repos_dir,
        path_info: &path_info,
        method,
        query_string,
        content_type: content_type.as_deref(),
        body,
        remote_user,
        git_protocol: git_protocol.as_deref(),
        protection_env: protection.as_ref(),
    })
    .await
    {
        Ok(resp) => {
            if receive && resp.status().is_success() {
                if let Some(auth) = &authed {
                    let updates = crate::webhook::payloads::parse_receive_ref_updates(body);
                    // Only emit when the client sent at least one ref update command.
                    if !updates.is_empty() {
                        let db = state.db.clone();
                        let repos_dir = state.repos_dir.clone();
                        let repo_id = resolved.row.id.clone();
                        let owner_slug = resolved.disk_owner.clone();
                        let repo_name = resolved.disk_name.clone();
                        let login = auth.owner.username.clone();
                        let uid = auth.owner.id.clone();
                        let env_name = state.env_name.clone();
                        let updates_wh = updates.clone();
                        let updates_pull = updates.clone();
                        let updates_act = updates.clone();
                        let git_act = state.git.clone();
                        tokio::spawn(async move {
                            let bare = repos_dir
                                .join(&owner_slug)
                                .join(format!("{repo_name}.git"));
                            // GIT-25: refresh cached size_bytes for UI/admin surfaces.
                            if let Err(e) = crate::git::quota::refresh_repo_size_bytes(
                                &db, &repo_id, &bare,
                            )
                            .await
                            {
                                tracing::warn!(error = %e, "refresh repo size_bytes failed");
                            }
                            crate::repo::record_ref_updates(
                                &db,
                                &repo_id,
                                &uid,
                                &updates_act,
                                Some(git_act),
                                Some(bare.as_path()),
                            )
                            .await;
                            crate::webhook::dispatch::notify_push(
                                &db,
                                &repo_id,
                                &owner_slug,
                                &repo_name,
                                &login,
                                &uid,
                                &updates_wh,
                                &env_name,
                            )
                            .await;
                            crate::pull::synchronize_after_push(
                                &db,
                                &repos_dir,
                                &repo_id,
                                &owner_slug,
                                &repo_name,
                                &login,
                                &uid,
                                &updates_pull,
                                &env_name,
                            )
                            .await;
                        });
                        let db2 = state.db.clone();
                        let git2 = state.git.clone();
                        let repos_dir2 = state.repos_dir.clone();
                        let repo_id2 = resolved.row.id.clone();
                        let owner2 = resolved.disk_owner.clone();
                        let name2 = resolved.disk_name.clone();
                        let uid2 = auth.owner.id.clone();
                        let updates2 = updates;
                        let actions_on = state.actions_enabled;
                        tokio::spawn(async move {
                            crate::actions::notify_push_actions(
                                &db2,
                                git2,
                                &repos_dir2,
                                &repo_id2,
                                &owner2,
                                &name2,
                                Some(&uid2),
                                &updates2,
                                actions_on,
                            )
                            .await;
                        });
                        let db3 = state.db.clone();
                        let git3 = state.git.clone();
                        let repos3 = state.repos_dir.clone();
                        let rid3 = resolved.row.id.clone();
                        crate::mirror::notify_mirror_after_local_mutation(
                            db3, git3, repos3, rid3,
                        );
                    }
                }
            }
            resp
        }
        Err(e) => {
            tracing::error!(error = %e, "git-http-backend failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": { "code": "git.cgi_failed", "message": "git smart http failed" }
                })),
            )
                .into_response()
        }
    }
}

/// `GET /{owner}/{repo}.git/info/refs?service=…`
pub async fn info_refs(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((owner, repo_git)): AxumPath<(String, String)>,
    Query(query): Query<InfoRefsQuery>,
) -> Response {
    let qs = match query.service.as_deref() {
        Some(s) => format!("service={s}"),
        None => String::new(),
    };
    authorize_and_cgi(
        &state,
        &headers,
        &owner,
        &repo_git,
        "info/refs",
        "GET",
        &qs,
        query.service.as_deref(),
        &[],
    )
    .await
}

/// `POST /{owner}/{repo}.git/git-upload-pack`
pub async fn upload_pack(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((owner, repo_git)): AxumPath<(String, String)>,
    body: Bytes,
) -> Response {
    authorize_and_cgi(
        &state,
        &headers,
        &owner,
        &repo_git,
        "git-upload-pack",
        "POST",
        "",
        Some("git-upload-pack"),
        &body,
    )
    .await
}

/// `POST /{owner}/{repo}.git/git-receive-pack`
pub async fn receive_pack(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((owner, repo_git)): AxumPath<(String, String)>,
    body: Bytes,
) -> Response {
    authorize_and_cgi(
        &state,
        &headers,
        &owner,
        &repo_git,
        "git-receive-pack",
        "POST",
        "",
        Some("git-receive-pack"),
        &body,
    )
    .await
}

/// Tiny base64 decoder — no new crates.io dep (T-08-SC).
mod base64_lite {
    pub fn decode(input: &str) -> Option<Vec<u8>> {
        let bytes: Vec<u8> = input
            .bytes()
            .filter(|&b| !b.is_ascii_whitespace())
            .collect();
        if bytes.is_empty() {
            return Some(Vec::new());
        }
        let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
        let mut buf: u32 = 0;
        let mut bits: i32 = 0;
        for &b in &bytes {
            if b == b'=' {
                break;
            }
            let val = match b {
                b'A'..=b'Z' => b - b'A',
                b'a'..=b'z' => b - b'a' + 26,
                b'0'..=b'9' => b - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return None,
            } as u32;
            buf = (buf << 6) | val;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
                buf &= (1 << bits) - 1;
            }
        }
        Some(out)
    }
}
