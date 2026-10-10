//! LFS PAT Basic auth + ACL helpers (D-LFS-09, D-LFS-11).
//!
//! Mirrors Smart HTTP: Cookie never authorizes; classic `repo` / FG contents scopes;
//! Capability Read for download, Write + verified email for upload.

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use oxidean_core::{
    ClassicPatScope, ContentsPerm, FgRepoAccess, PatKind, CLASSIC_PAT_PREFIX, FINE_GRAINED_PAT_PREFIX,
};
use oxidean_db::{PatRow, RepositoryRow, UserRow};

use crate::app::AppState;
use crate::auth::session::sha256_hex;
use crate::lfs::batch::LfsErrorBody;
use crate::repo::{
    effective_capability, fg_all_covers_repo, is_private_visibility, meets, Capability, OwnerRef,
};

const LFS_AUTHENTICATE: &str = r#"Basic realm="Git LFS""#;
const LFS_JSON: &str = "application/vnd.git-lfs+json";

const USERNAME_ALIASES: &[&str] = &["git", "oxidean"];

fn lfs_json_headers() -> [(header::HeaderName, HeaderValue); 1] {
    [(header::CONTENT_TYPE, HeaderValue::from_static(LFS_JSON))]
}

pub fn unauthorized_lfs() -> Response {
    let mut res = (
        StatusCode::UNAUTHORIZED,
        lfs_json_headers(),
        Json(LfsErrorBody {
            message: "Credentials needed to access LFS".into(),
            request_id: None,
        }),
    )
        .into_response();
    res.headers_mut().insert(
        header::HeaderName::from_static("lfs-authenticate"),
        HeaderValue::from_static(LFS_AUTHENTICATE),
    );
    res
}

pub fn forbidden_lfs(msg: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        lfs_json_headers(),
        Json(LfsErrorBody {
            message: msg.into(),
            request_id: None,
        }),
    )
        .into_response()
}

pub fn email_unverified_lfs() -> Response {
    (
        StatusCode::FORBIDDEN,
        lfs_json_headers(),
        Json(serde_json::json!({
            "message": "verify your email to continue",
            "request_id": null,
            "code": "auth.email_unverified"
        })),
    )
        .into_response()
}

fn parse_basic(headers: &HeaderMap) -> Option<(String, String)> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = raw
        .strip_prefix("Basic ")
        .or_else(|| raw.strip_prefix("basic "))?;
    let bytes = base64_lite_decode(encoded.trim())?;
    let decoded = String::from_utf8(bytes).ok()?;
    let (user, pass) = decoded.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

fn base64_lite_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = [0u8; 4];
    let mut n = 0;
    for &b in bytes {
        if b == b'=' {
            break;
        }
        buf[n] = val(b)?;
        n += 1;
        if n == 4 {
            out.push((buf[0] << 2) | (buf[1] >> 4));
            out.push((buf[1] << 4) | (buf[2] >> 2));
            out.push((buf[2] << 6) | buf[3]);
            n = 0;
        }
    }
    if n == 3 {
        out.push((buf[0] << 2) | (buf[1] >> 4));
        out.push((buf[1] << 4) | (buf[2] >> 2));
    } else if n == 2 {
        out.push((buf[0] << 2) | (buf[1] >> 4));
    }
    Some(out)
}

fn looks_like_pat(password: &str) -> bool {
    password.starts_with(CLASSIC_PAT_PREFIX) || password.starts_with(FINE_GRAINED_PAT_PREFIX)
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

pub struct AuthedPat {
    pub pat: PatRow,
    pub owner: UserRow,
}

#[allow(clippy::result_large_err)]
/// Resolve Basic credentials to a PAT. Cookies are never consulted (D-LFS-09).
pub async fn authenticate_pat(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<AuthedPat>, Response> {
    // D-LFS-09: Cookie / oxidean_session must not authenticate LFS.
    let _cookie_ignored = headers.get(header::COOKIE);

    let Some((username, password)) = parse_basic(headers) else {
        return Ok(None);
    };
    if username.is_empty() {
        return Err(unauthorized_lfs());
    }
    if !looks_like_pat(&password) {
        return Err(unauthorized_lfs());
    }
    let token_hash = sha256_hex(password.as_bytes());
    let pat = match state.db.find_pat_by_token_hash(&token_hash).await {
        Ok(Some(p)) if p.revoked_at.is_none() => p,
        Ok(_) => return Err(unauthorized_lfs()),
        Err(e) => {
            tracing::error!(error = %e, "find_pat_by_token_hash");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };
    if pat_expired(&pat.expires_at) {
        return Err(unauthorized_lfs());
    }
    let owner = match state.db.find_user_by_id(&pat.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => return Err(unauthorized_lfs()),
        Err(e) => {
            tracing::error!(error = %e, "find_user_by_id");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };
    if owner.banned_at.is_some() {
        return Err(unauthorized_lfs());
    }
    let user_ok = USERNAME_ALIASES
        .iter()
        .any(|a| username.eq_ignore_ascii_case(a))
        || username.eq_ignore_ascii_case(&owner.username);
    if !user_ok {
        return Err(unauthorized_lfs());
    }
    Ok(Some(AuthedPat { pat, owner }))
}

#[allow(clippy::result_large_err)]
/// Classic `repo` / FG contents+selection — insufficient → caller maps to 403 (D-LFS-11).
pub async fn pat_allows_operation(
    state: &AppState,
    pat: &PatRow,
    repo: &RepositoryRow,
    owner: &OwnerRef,
    upload: bool,
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
                ContentsPerm::Read => !upload,
                ContentsPerm::Write => true,
            })
        }
    }
}

#[allow(clippy::result_large_err)]
/// Authorize download or upload against ACL + PAT scopes (D-LFS-09/11).
///
/// Returns the authenticated PAT when present and authorized. `None` means
/// anonymous access is allowed (public download only).
pub async fn authorize_lfs(
    state: &AppState,
    headers: &HeaderMap,
    repo: &RepositoryRow,
    owner: &OwnerRef,
    upload: bool,
) -> Result<Option<AuthedPat>, Response> {
    let authed = authenticate_pat(state, headers).await?;
    let is_private = is_private_visibility(&repo.visibility);

    if upload {
        let Some(auth) = authed else {
            return Err(unauthorized_lfs());
        };
        let capability = match effective_capability(
            &state.db,
            Some(auth.owner.id.as_str()),
            repo,
            owner,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "effective_capability");
                return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        };
        if !meets(capability, Capability::Write) {
            return Err(unauthorized_lfs());
        }
        if auth.owner.email_verified_at.is_none() {
            return Err(email_unverified_lfs());
        }
        // GIT-20: archived repositories are read-only — downloads stay open.
        if repo.archived {
            return Err(forbidden_lfs("Repository is archived (read-only)"));
        }
        match pat_allows_operation(state, &auth.pat, repo, owner, true).await {
            Ok(true) => {}
            Ok(false) => {
                return Err(forbidden_lfs(
                    "Insufficient personal access token scope for LFS upload",
                ));
            }
            Err(r) => return Err(r),
        }
        return Ok(Some(auth));
    }

    // Download
    if is_private {
        let Some(auth) = authed else {
            return Err(unauthorized_lfs());
        };
        let capability = match effective_capability(
            &state.db,
            Some(auth.owner.id.as_str()),
            repo,
            owner,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "effective_capability");
                return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        };
        if !meets(capability, Capability::Read) {
            return Err(unauthorized_lfs());
        }
        match pat_allows_operation(state, &auth.pat, repo, owner, false).await {
            Ok(true) => {}
            Ok(false) => {
                return Err(forbidden_lfs(
                    "Insufficient personal access token scope for LFS download",
                ));
            }
            Err(r) => return Err(r),
        }
        return Ok(Some(auth));
    }

    // Public download: anon OK; if PAT present, still validate scope when provided.
    if let Some(auth) = authed {
        match pat_allows_operation(state, &auth.pat, repo, owner, false).await {
            Ok(true) => return Ok(Some(auth)),
            Ok(false) => {
                return Err(forbidden_lfs(
                    "Insufficient personal access token scope for LFS download",
                ));
            }
            Err(r) => return Err(r),
        }
    }
    Ok(None)
}
