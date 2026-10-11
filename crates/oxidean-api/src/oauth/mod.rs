//! OAuth2 provider (API-03) — Oxidean as an authorization server.
//!
//! Browser flow:
//!   `GET /oauth/authorize?client_id=..&redirect_uri=..&scope=..&state=..`
//!   → validates the request, then 302s to the SPA consent page
//!     `/oauth/consent` (session cookie required; anonymous users go to
//!     `/login?returnTo=` first). The consent page drives
//!     `oauthApp.authorizeInfo` / `oauthApp.authorize` RPCs, which mint the
//!     single-use authorization code and hand back the client redirect.
//!
//! Server flow:
//!   `POST /oauth/token` — `grant_type=authorization_code`, `code`,
//!   `redirect_uri`, `client_id`, `client_secret` (form-encoded per RFC 6749;
//!   JSON also accepted; HTTP Basic client auth supported).
//!   `GET /oauth/userinfo` — Bearer access token → identity JSON.
//!
//! Token seam: OAuth access tokens (`oxidean_oat_` prefix) authenticate
//! wherever a PAT does — git smart HTTP (Basic password) and the package
//! registries (Basic/Bearer) via [`authenticate_oauth_token`]. When API-02's
//! `Authorization: Bearer` support for `/api/rpc` lands, it should consult
//! [`crate::oauth::authenticate_oauth_token`] alongside the PAT lookup.

use axum::extract::{Query, RawQuery, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use oxidean_core::{
    format_oauth_scopes, oauth_scopes_or_default, parse_oauth_scopes, AppError,
    CreateOAuthAppRequest, CreateOAuthAppResponse, OAuthAppIdRequest, OAuthAppPublic,
    OAuthAuthorizeInfo, OAuthAuthorizeInfoRequest, OAuthAuthorizeRequest, OAuthAuthorizeResponse,
    OAuthGrantPublic, OAuthScope, OAuthTokenResponse, OAuthUserInfoResponse, UpdateOAuthAppRequest,
    OAUTH_ACCESS_TOKEN_PREFIX, OAUTH_CLIENT_ID_PREFIX, OAUTH_CLIENT_SECRET_PREFIX,
    OAUTH_CODE_PREFIX, OAUTH_CODE_TTL_SECS, OAUTH_TOKEN_TTL_SECS,
};
use oxidean_db::{OAuthAppRow, OAuthTokenRow, PatRow, UserRow};

use crate::app::AppState;
use crate::auth::gate::require_verified;
use crate::auth::session::{bytes_to_hex, sha256_hex};
use crate::rpc::RpcCtx;

const SECRET_BYTES: usize = 32;
const MAX_REDIRECT_URIS: usize = 10;
const MAX_REDIRECT_URI_LEN: usize = 2048;
const MAX_NAME_LEN: usize = 200;
const MAX_STATE_LEN: usize = 1024;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("oauth db error: {e}");
        AppError::new("oauth.internal", "oauth operation failed")
    }
}

fn require_session_user_id(ctx: &RpcCtx) -> Result<&str, AppError> {
    ctx.session
        .as_ref()
        .map(|s| s.user_id.as_str())
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn rfc3339_in(secs: i64) -> String {
    (Utc::now() + chrono::Duration::seconds(secs))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn rfc3339_expired(at: &str) -> bool {
    match DateTime::parse_from_rfc3339(at) {
        Ok(dt) => dt.with_timezone(&Utc) <= Utc::now(),
        Err(_) => true,
    }
}

fn mint_secret(prefix: &str) -> (String, String, String) {
    let mut bytes = [0u8; SECRET_BYTES];
    rand::fill(&mut bytes);
    let hex = bytes_to_hex(&bytes);
    let plaintext = format!("{prefix}{hex}");
    let hash = sha256_hex(plaintext.as_bytes());
    let display = format!("{prefix}{}", &hex[..8]);
    (plaintext, hash, display)
}

// --- shared validation ---

fn validate_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::new(
            "oauth.name_required",
            "A name is required for OAuth applications",
        ));
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("name must be at most {MAX_NAME_LEN} characters"),
        ));
    }
    Ok(trimmed.to_string())
}

/// Absolute http(s) URIs only — no fragments, no userinfo, exact-match later.
fn validate_redirect_uris(uris: &[String]) -> Result<Vec<String>, AppError> {
    if uris.is_empty() || uris.len() > MAX_REDIRECT_URIS {
        return Err(AppError::new(
            "oauth.invalid_redirect_uri",
            format!("provide between 1 and {MAX_REDIRECT_URIS} redirect URIs"),
        ));
    }
    let mut out: Vec<String> = Vec::with_capacity(uris.len());
    for raw in uris {
        let uri = raw.trim();
        if uri.is_empty() || uri.len() > MAX_REDIRECT_URI_LEN {
            return Err(AppError::new(
                "oauth.invalid_redirect_uri",
                "redirect URIs must be non-empty and at most 2048 characters",
            ));
        }
        let parsed = url::Url::parse(uri).map_err(|_| {
            AppError::new(
                "oauth.invalid_redirect_uri",
                "redirect URIs must be absolute http(s) URLs",
            )
        })?;
        // https, or http only for loopback dev callbacks (localhost/127.*/::1).
        let host = parsed.host_str().unwrap_or("");
        let scheme_ok = parsed.scheme() == "https"
            || (parsed.scheme() == "http"
                && (host == "localhost" || host == "::1" || host.starts_with("127.")));
        if !scheme_ok {
            return Err(AppError::new(
                "oauth.invalid_redirect_uri",
                "redirect URIs must use https (http allowed for localhost only)",
            ));
        }
        if parsed.fragment().is_some() || parsed.username() != "" || parsed.password().is_some() {
            return Err(AppError::new(
                "oauth.invalid_redirect_uri",
                "redirect URIs must not contain fragments or userinfo",
            ));
        }
        let normalized = parsed.to_string();
        if !out.iter().any(|u| u == &normalized) {
            out.push(normalized);
        }
    }
    Ok(out)
}

fn app_to_public(row: &OAuthAppRow) -> Result<OAuthAppPublic, AppError> {
    let redirect_uris: Vec<String> =
        serde_json::from_str(&row.redirect_uris_json).map_err(|e| {
            tracing::error!(error = %e, "invalid redirect_uris_json");
            AppError::new("oauth.internal", "corrupt oauth app data")
        })?;
    Ok(OAuthAppPublic {
        id: row.id.clone(),
        name: row.name.clone(),
        client_id: row.client_id.clone(),
        // Secrets are write-once: the stored prefix carries secret material
        // (first 8 hex), so the public payload only echoes the marker.
        client_secret_prefix: OAUTH_CLIENT_SECRET_PREFIX.to_string(),
        redirect_uris,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    })
}

fn redirect_uris(row: &OAuthAppRow) -> Vec<String> {
    serde_json::from_str(&row.redirect_uris_json).unwrap_or_default()
}

/// RFC 6749 redirect_uri resolution: provided value must byte-match one
/// registered URI; omitted → the single registered URI, else error.
fn resolve_redirect_uri(row: &OAuthAppRow, provided: Option<&str>) -> Result<String, AppError> {
    let registered = redirect_uris(row);
    match provided.map(str::trim).filter(|s| !s.is_empty()) {
        Some(uri) => {
            if registered.iter().any(|u| u == uri) {
                Ok(uri.to_string())
            } else {
                Err(AppError::new(
                    "oauth.redirect_uri_mismatch",
                    "redirect_uri does not match a registered callback",
                ))
            }
        }
        None => {
            if registered.len() == 1 {
                Ok(registered[0].clone())
            } else {
                Err(AppError::new(
                    "oauth.invalid_redirect_uri",
                    "redirect_uri is required when the app registers multiple callbacks",
                ))
            }
        }
    }
}

/// Append `code`/`state` or `error`/`state` to the client redirect URI.
fn build_redirect(redirect_uri: &str, pairs: &[(&str, &str)]) -> String {
    let mut url = match url::Url::parse(redirect_uri) {
        Ok(u) => u,
        Err(_) => return redirect_uri.to_string(),
    };
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in pairs {
            q.append_pair(k, v);
        }
    }
    url.to_string()
}

/// Resolve `(client_id, redirect_uri, scopes)` for authorize/consent — shared
/// between `GET /oauth/authorize`, `oauthApp.authorizeInfo`, and
/// `oauthApp.authorize`.
async fn resolve_authorize_request(
    ctx_db: &oxidean_db::Database,
    client_id: &str,
    redirect_uri: Option<&str>,
    scope: Option<&str>,
) -> Result<(OAuthAppRow, String, Vec<OAuthScope>), AppError> {
    let app = ctx_db
        .find_oauth_app_by_client_id(client_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("oauth.unknown_client", "unknown client_id"))?;
    let uri = resolve_redirect_uri(&app, redirect_uri)?;
    let scopes =
        oauth_scopes_or_default(scope).map_err(|e| AppError::new("oauth.invalid_scope", e))?;
    Ok((app, uri, scopes))
}

// ---------------------------------------------------------------------------
// HTTP: GET /oauth/authorize
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AuthorizeQuery {
    pub client_id: Option<String>,
    pub redirect_uri: Option<String>,
    pub response_type: Option<String>,
    pub scope: Option<String>,
    pub state: Option<String>,
}

fn oauth_json_error(status: StatusCode, error: &str, description: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": error,
            "error_description": description,
        })),
    )
        .into_response()
}

/// Redirect to the client's redirect_uri carrying an RFC 6749 error.
fn oauth_error_redirect(redirect_uri: &str, error: &str, state: Option<&str>) -> Response {
    let mut pairs = vec![("error", error)];
    if let Some(s) = state {
        pairs.push(("state", s));
    }
    Redirect::to(&build_redirect(redirect_uri, &pairs)).into_response()
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    crate::app::session_token_from_headers(headers)
}

/// `GET /oauth/authorize` — RFC 6749 authorization endpoint.
///
/// Validates the request, then redirects to the SPA consent page
/// (`/oauth/consent`) when signed in, or `/login?returnTo=` back here when not.
/// Errors that make the redirect_uri untrusted render as a 400 JSON body
/// instead of redirecting (never leak `code`/`error` to an unregistered URI).
pub async fn authorize(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    Query(q): Query<AuthorizeQuery>,
) -> Response {
    let client_id = q.client_id.as_deref().unwrap_or("").trim();
    if client_id.is_empty() {
        return oauth_json_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "client_id is required",
        );
    }
    let app = match state.db.find_oauth_app_by_client_id(client_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return oauth_json_error(
                StatusCode::BAD_REQUEST,
                "unauthorized_client",
                "unknown client_id",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "oauth authorize: app lookup failed");
            return oauth_json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
        }
    };
    let redirect_uri = match resolve_redirect_uri(&app, q.redirect_uri.as_deref()) {
        Ok(u) => u,
        Err(e) => {
            // Unregistered/missing redirect_uri — do not redirect (RFC §4.1.2.1).
            return oauth_json_error(StatusCode::BAD_REQUEST, "invalid_request", &e.message);
        }
    };
    if q.response_type.as_deref() != Some("code") {
        return oauth_error_redirect(
            &redirect_uri,
            "unsupported_response_type",
            q.state.as_deref(),
        );
    }
    if let Err(e) = oauth_scopes_or_default(q.scope.as_deref()) {
        tracing::warn!(error = %e, "oauth authorize: invalid scope");
        return oauth_error_redirect(&redirect_uri, "invalid_scope", q.state.as_deref());
    }

    let client = crate::rpc::ClientMeta::from_headers(&headers);
    let session = match session_token(&headers) {
        Some(token) => match state
            .sessions
            .resolve(
                &state.db,
                &token,
                client.ip_address.as_deref(),
                client.user_agent.as_deref(),
            )
            .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "oauth authorize: session resolve failed");
                None
            }
        },
        None => None,
    };

    // Preserve the original query verbatim for the SPA hop / login round-trip;
    // fall back to re-encoding the parsed fields when absent.
    let raw_query = raw_query.unwrap_or_else(|| serde_plain_query(&q));

    match session {
        Some(_) => Redirect::to(&format!("/oauth/consent?{raw_query}")).into_response(),
        None => {
            let back = format!("/oauth/authorize?{raw_query}");
            Redirect::to(&format!(
                "/login?returnTo={}",
                url::form_urlencoded::byte_serialize(back.as_bytes()).collect::<String>()
            ))
            .into_response()
        }
    }
}

/// Re-encode the parsed query — fallback when `RawQuery` is absent.
fn serde_plain_query(q: &AuthorizeQuery) -> String {
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    if let Some(v) = &q.client_id {
        ser.append_pair("client_id", v);
    }
    if let Some(v) = &q.redirect_uri {
        ser.append_pair("redirect_uri", v);
    }
    if let Some(v) = &q.response_type {
        ser.append_pair("response_type", v);
    }
    if let Some(v) = &q.scope {
        ser.append_pair("scope", v);
    }
    if let Some(v) = &q.state {
        ser.append_pair("state", v);
    }
    ser.finish()
}

// ---------------------------------------------------------------------------
// HTTP: POST /oauth/token
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TokenForm {
    grant_type: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
}

fn token_error(status: StatusCode, error: &str, description: &str) -> Response {
    let mut res = oauth_json_error(status, error, description);
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res.headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    res
}

/// Decode `Authorization: Basic <b64 client_id:client_secret>` (RFC 6749 §2.3.1).
fn client_basic_auth(headers: &HeaderMap) -> Option<(String, String)> {
    let (user, pass) = crate::packages::auth::decode_basic(headers)?;
    // Percent-decode per RFC — Basic credentials are form-url-encoded.
    let decode = |s: &str| {
        url::form_urlencoded::parse(s.as_bytes())
            .map(|(k, _)| k.into_owned())
            .next()
            .unwrap_or_else(|| s.to_string())
    };
    Some((decode(&user), decode(&pass)))
}

/// `POST /oauth/token` — authorization_code exchange (RFC 6749 §4.1.3).
///
/// Accepts `application/x-www-form-urlencoded` (standard) or
/// `application/json` bodies. Codes are single-use and expire after
/// [`OAUTH_CODE_TTL_SECS`] — the `used_at` claim is atomic.
pub async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let form: TokenForm = match headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.starts_with("application/json"))
    {
        Some(true) => match serde_json::from_slice(&body) {
            Ok(f) => f,
            Err(_) => {
                return token_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "malformed JSON body",
                )
            }
        },
        _ => {
            let mut form = TokenForm {
                grant_type: None,
                code: None,
                redirect_uri: None,
                client_id: None,
                client_secret: None,
            };
            for (k, v) in url::form_urlencoded::parse(&body) {
                match k.as_ref() {
                    "grant_type" => form.grant_type = Some(v.into_owned()),
                    "code" => form.code = Some(v.into_owned()),
                    "redirect_uri" => form.redirect_uri = Some(v.into_owned()),
                    "client_id" => form.client_id = Some(v.into_owned()),
                    "client_secret" => form.client_secret = Some(v.into_owned()),
                    _ => {}
                }
            }
            form
        }
    };

    if form.grant_type.as_deref() != Some("authorization_code") {
        return token_error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "only grant_type=authorization_code is supported",
        );
    }
    let code = form.code.as_deref().unwrap_or("").trim();
    if code.is_empty() {
        return token_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "code is required",
        );
    }

    // Client authentication: HTTP Basic wins; else client_id + client_secret fields.
    let (client_id, client_secret) = match client_basic_auth(&headers) {
        Some((id, secret)) => (id, secret),
        None => match (form.client_id, form.client_secret) {
            (Some(id), Some(secret)) => (id, secret),
            _ => {
                return token_error(
                    StatusCode::UNAUTHORIZED,
                    "invalid_client",
                    "client authentication required (Basic or client_id + client_secret)",
                )
            }
        },
    };

    let app = match state.db.find_oauth_app_by_client_id(&client_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return token_error(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "unknown client_id",
            )
        }
        Err(e) => {
            tracing::error!(error = %e, "oauth token: app lookup failed");
            return token_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
        }
    };
    if sha256_hex(client_secret.as_bytes()) != app.client_secret_hash {
        return token_error(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client_secret mismatch",
        );
    }

    // Single-use claim: mark used first so a racing second exchange fails.
    let used_at = now_rfc3339();
    let grant = match state
        .db
        .consume_oauth_code(&sha256_hex(code.as_bytes()), &used_at)
        .await
    {
        Ok(Some(c)) => c,
        Ok(None) => {
            return token_error(
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "authorization code is invalid or already used",
            )
        }
        Err(e) => {
            tracing::error!(error = %e, "oauth token: consume_code failed");
            return token_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
        }
    };
    if rfc3339_expired(&grant.expires_at) {
        return token_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "authorization code expired",
        );
    }
    if grant.application_id != app.id {
        return token_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "authorization code was not issued to this client",
        );
    }
    if let Some(uri) = form
        .redirect_uri
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if uri != grant.redirect_uri {
            return token_error(
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "redirect_uri does not match the authorization request",
            );
        }
    }

    let (plaintext, hash, prefix) = mint_secret(OAUTH_ACCESS_TOKEN_PREFIX);
    let token_id = Uuid::new_v4().to_string();
    if let Err(e) = state
        .db
        .insert_oauth_token(
            &token_id,
            &grant.application_id,
            &grant.user_id,
            &prefix,
            &hash,
            &grant.scopes,
            &rfc3339_in(OAUTH_TOKEN_TTL_SECS),
        )
        .await
    {
        tracing::error!(error = %e, "oauth token: insert failed");
        return token_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "internal error",
        );
    }

    crate::audit::record_with(
        &state.db,
        &crate::rpc::ClientMeta::from_headers(&headers),
        None,
        "oauth.token_issued",
        Some(("oauth_application", &app.id)),
        None,
    )
    .await;

    let body = OAuthTokenResponse {
        access_token: plaintext,
        token_type: "bearer".into(),
        expires_in: OAUTH_TOKEN_TTL_SECS,
        scope: grant.scopes.clone(),
    };
    let mut res = Json(body).into_response();
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res.headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    res
}

// ---------------------------------------------------------------------------
// Access-token auth seam (PAT-analogous)
// ---------------------------------------------------------------------------

/// Resolved OAuth access token + owner after Bearer/password verification.
#[derive(Debug, Clone)]
pub struct OAuthTokenIdentity {
    pub token: OAuthTokenRow,
    pub user: UserRow,
}

impl OAuthTokenIdentity {
    pub fn scopes(&self) -> Vec<OAuthScope> {
        parse_oauth_scopes(&self.token.scopes).unwrap_or_default()
    }

    pub fn has_scope(&self, scope: OAuthScope) -> bool {
        self.scopes().contains(&scope)
    }

    /// Synthesize a classic-PAT-shaped row so existing ACL helpers
    /// (`pat_allows_operation`, `pat_allows_packages`) apply unchanged.
    /// OAuth scope names intentionally equal the classic scope strings
    /// (`repo`, `package:read`, `package:write`).
    pub fn synthetic_pat(&self) -> PatRow {
        let scopes_json =
            serde_json::to_string(&self.scopes().iter().map(|s| s.as_str()).collect::<Vec<_>>())
                .unwrap_or_else(|_| "[]".to_string());
        PatRow {
            id: self.token.id.clone(),
            user_id: self.token.user_id.clone(),
            kind: "classic".into(),
            name: "oauth access token".into(),
            token_prefix: self.token.token_prefix.clone(),
            token_hash: self.token.token_hash.clone(),
            scopes_json: Some(scopes_json),
            contents_perm: None,
            repo_access: None,
            expires_at: Some(self.token.expires_at.clone()),
            revoked_at: self.token.revoked_at.clone(),
            last_used_at: self.token.last_used_at.clone(),
            last_used_ip: self.token.last_used_ip.clone(),
            created_at: self.token.created_at.clone(),
            repository_ids: Vec::new(),
        }
    }
}

/// Resolve an `oxidean_oat_` bearer/password to its owner.
/// `None` when the token is unknown, revoked, expired, or the owner vanished.
pub async fn authenticate_oauth_token(
    db: &oxidean_db::Database,
    raw_token: &str,
) -> Result<Option<OAuthTokenIdentity>, String> {
    if !raw_token.starts_with(OAUTH_ACCESS_TOKEN_PREFIX) {
        return Ok(None);
    }
    let hash = sha256_hex(raw_token.as_bytes());
    let Some(token) = db.find_oauth_token_by_hash(&hash).await? else {
        return Ok(None);
    };
    if rfc3339_expired(&token.expires_at) {
        return Ok(None);
    }
    let Some(user) = db.find_user_by_id(&token.user_id).await? else {
        return Ok(None);
    };
    if user.banned_at.is_some() {
        return Ok(None);
    }
    Ok(Some(OAuthTokenIdentity { token, user }))
}

// ---------------------------------------------------------------------------
// HTTP: GET /oauth/userinfo
// ---------------------------------------------------------------------------

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    crate::packages::auth::decode_bearer(headers)
}

fn unauthorized_bearer() -> Response {
    let mut res = oauth_json_error(
        StatusCode::UNAUTHORIZED,
        "invalid_token",
        "Bearer token required",
    );
    res.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        HeaderValue::from_static("Bearer realm=\"oxidean\""),
    );
    res
}

/// `GET /oauth/userinfo` — identity surface for "sign in with Oxidean".
/// Requires `read:user`; adds `email` when the token carries `user:email`.
pub async fn userinfo(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(raw) = bearer_token(&headers) else {
        return unauthorized_bearer();
    };
    let identity = match authenticate_oauth_token(&state.db, &raw).await {
        Ok(Some(i)) => i,
        Ok(None) => return unauthorized_bearer(),
        Err(e) => {
            tracing::error!(error = %e, "oauth userinfo: token lookup failed");
            return oauth_json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
        }
    };
    if !identity.has_scope(OAuthScope::ReadUser) {
        return oauth_json_error(
            StatusCode::FORBIDDEN,
            "insufficient_scope",
            "the read:user scope is required",
        );
    }

    let avatar_url = identity.user.avatar_path.clone();
    let email = identity
        .has_scope(OAuthScope::UserEmail)
        .then(|| identity.user.email.clone());

    let client = crate::rpc::ClientMeta::from_headers(&headers);
    let now = now_rfc3339();
    if let Err(e) = state
        .db
        .touch_oauth_token_last_used(&identity.token.id, &now, client.ip_address.as_deref())
        .await
    {
        tracing::warn!(error = %e, "oauth userinfo: touch last_used failed");
    }

    Json(OAuthUserInfoResponse {
        id: identity.user.id.clone(),
        username: identity.user.username.clone(),
        display_name: identity.user.display_name.clone(),
        avatar_url,
        email,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// RPC: oauthApp.* (session-cookie management + consent decision)
// ---------------------------------------------------------------------------

/// `oauthApp.create` — register an app; returns the one-time client_secret.
pub async fn create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CreateOAuthAppResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: CreateOAuthAppRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.create input: {e}"),
        )
    })?;
    let name = validate_name(&req.name)?;
    let uris = validate_redirect_uris(&req.redirect_uris)?;
    let uris_json = serde_json::to_string(&uris)
        .map_err(|e| AppError::new("oauth.internal", format!("uris serialize: {e}")))?;

    let (secret, secret_hash, secret_prefix) = mint_secret(OAUTH_CLIENT_SECRET_PREFIX);
    let client_id = {
        let mut bytes = [0u8; 16];
        rand::fill(&mut bytes);
        format!("{OAUTH_CLIENT_ID_PREFIX}{}", bytes_to_hex(&bytes))
    };
    let id = Uuid::new_v4().to_string();
    ctx.db
        .insert_oauth_app(
            &id,
            &user.id,
            &name,
            &client_id,
            &secret_hash,
            &secret_prefix,
            &uris_json,
        )
        .await
        .map_err(db_err)?;

    let row = ctx
        .db
        .find_oauth_app_by_id(&id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("oauth.internal", "created app missing"))?;
    Ok(CreateOAuthAppResponse {
        app: app_to_public(&row)?,
        client_secret: secret,
    })
}

/// `oauthApp.list` — apps owned by the signed-in user.
pub async fn list(ctx: &RpcCtx) -> Result<Vec<OAuthAppPublic>, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let rows = ctx
        .db
        .list_oauth_apps_for_owner(user_id)
        .await
        .map_err(db_err)?;
    rows.iter().map(app_to_public).collect()
}

/// Load `id` and check `owner_id == session user`.
async fn owned_app(ctx: &RpcCtx, id: &str) -> Result<OAuthAppRow, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let app = ctx
        .db
        .find_oauth_app_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("oauth.app_not_found", "oauth application not found"))?;
    if app.owner_id != user_id {
        return Err(AppError::new(
            "oauth.app_not_found",
            "oauth application not found",
        ));
    }
    Ok(app)
}

/// `oauthApp.update` — rename and/or replace the redirect URI set.
pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<OAuthAppPublic, AppError> {
    let req: UpdateOAuthAppRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.update input: {e}"),
        )
    })?;
    let app = owned_app(ctx, req.id.trim()).await?;
    let name = match req.name.as_deref() {
        Some(n) => validate_name(n)?,
        None => app.name.clone(),
    };
    let uris_json = match req.redirect_uris.as_ref() {
        Some(list) => serde_json::to_string(&validate_redirect_uris(list)?)
            .map_err(|e| AppError::new("oauth.internal", format!("uris serialize: {e}")))?,
        None => app.redirect_uris_json.clone(),
    };
    ctx.db
        .update_oauth_app(&app.id, &name, &uris_json, &now_rfc3339())
        .await
        .map_err(db_err)?;
    let row = ctx
        .db
        .find_oauth_app_by_id(&app.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("oauth.internal", "updated app missing"))?;
    app_to_public(&row)
}

/// `oauthApp.delete` — removes the app; codes + tokens cascade-delete.
pub async fn delete_app(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let req: OAuthAppIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.delete input: {e}"),
        )
    })?;
    let app = owned_app(ctx, req.id.trim()).await?;
    ctx.db.delete_oauth_app(&app.id).await.map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}

/// `oauthApp.regenerateSecret` — rotate the client secret (one-time reveal).
pub async fn regenerate_secret(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CreateOAuthAppResponse, AppError> {
    let _user = require_verified(ctx).await?;
    let req: OAuthAppIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.regenerateSecret input: {e}"),
        )
    })?;
    let app = owned_app(ctx, req.id.trim()).await?;
    let (secret, hash, prefix) = mint_secret(OAUTH_CLIENT_SECRET_PREFIX);
    ctx.db
        .update_oauth_app_secret(&app.id, &hash, &prefix, &now_rfc3339())
        .await
        .map_err(db_err)?;
    let row = ctx
        .db
        .find_oauth_app_by_id(&app.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("oauth.internal", "updated app missing"))?;
    Ok(CreateOAuthAppResponse {
        app: app_to_public(&row)?,
        client_secret: secret,
    })
}

/// `oauthApp.listGrants` — applications with a live token for the signed-in
/// user (the "Authorized OAuth applications" list).
pub async fn list_grants(ctx: &RpcCtx) -> Result<Vec<OAuthGrantPublic>, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let tokens = ctx
        .db
        .list_active_oauth_tokens_for_user(user_id)
        .await
        .map_err(db_err)?;
    let mut grants: Vec<OAuthGrantPublic> = Vec::new();
    for t in &tokens {
        let idx = grants
            .iter()
            .position(|g| g.application_id == t.application_id);
        match idx {
            Some(i) => {
                let g = &mut grants[i];
                for s in parse_oauth_scopes(&t.scopes).unwrap_or_default() {
                    let name = s.as_str().to_string();
                    if !g.scopes.contains(&name) {
                        g.scopes.push(name);
                    }
                }
                if t.created_at < g.granted_at {
                    g.granted_at = t.created_at.clone();
                }
                match (&g.last_used_at, &t.last_used_at) {
                    (Some(cur), Some(next)) if next > cur => g.last_used_at = Some(next.clone()),
                    (None, Some(next)) => g.last_used_at = Some(next.clone()),
                    _ => {}
                }
            }
            None => {
                let app = ctx
                    .db
                    .find_oauth_app_by_id(&t.application_id)
                    .await
                    .map_err(db_err)?;
                let Some(app) = app else { continue };
                grants.push(OAuthGrantPublic {
                    application_id: t.application_id.clone(),
                    app_name: app.name.clone(),
                    client_id: app.client_id.clone(),
                    scopes: parse_oauth_scopes(&t.scopes)
                        .unwrap_or_default()
                        .iter()
                        .map(|s| s.as_str().to_string())
                        .collect(),
                    granted_at: t.created_at.clone(),
                    last_used_at: t.last_used_at.clone(),
                });
            }
        }
    }
    Ok(grants)
}

/// `oauthApp.revoke` — revoke the signed-in user's grant to an application
/// (`id` = `oauth_applications.id`); every live token for that pair dies.
pub async fn revoke_grant(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let user_id = require_session_user_id(ctx)?.to_string();
    let req: OAuthAppIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.revoke input: {e}"),
        )
    })?;
    let id = req.id.trim();
    let app = ctx
        .db
        .find_oauth_app_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("oauth.app_not_found", "oauth application not found"))?;
    ctx.db
        .revoke_oauth_tokens_for_user_app(&user_id, &app.id, &now_rfc3339())
        .await
        .map_err(db_err)?;
    let actor_name = ctx
        .db
        .find_user_by_id(&user_id)
        .await
        .ok()
        .flatten()
        .map(|u| u.username)
        .unwrap_or_default();
    crate::audit::record_with(
        &ctx.db,
        &ctx.client,
        Some((user_id.as_str(), actor_name.as_str())),
        "oauth.grant_revoked",
        Some(("oauth_application", &app.id)),
        None,
    )
    .await;
    Ok(serde_json::json!({ "ok": true }))
}

/// `oauthApp.authorizeInfo` — consent screen payload (session required).
pub async fn authorize_info(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OAuthAuthorizeInfo, AppError> {
    require_session_user_id(ctx)?;
    let req: OAuthAuthorizeInfoRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.authorizeInfo input: {e}"),
        )
    })?;
    let (app, uri, scopes) = resolve_authorize_request(
        &ctx.db,
        req.client_id.trim(),
        req.redirect_uri.as_deref(),
        req.scope.as_deref(),
    )
    .await?;
    let owner_username = ctx
        .db
        .find_user_by_id(&app.owner_id)
        .await
        .map_err(db_err)?
        .map(|u| u.username)
        .unwrap_or_default();
    Ok(OAuthAuthorizeInfo {
        app_name: app.name.clone(),
        client_id: app.client_id.clone(),
        redirect_uri: uri,
        scopes: scopes.iter().map(|s| s.as_str().to_string()).collect(),
        owner_username,
    })
}

/// `oauthApp.authorize` — the consent decision. `approve` mints a single-use
/// code (≤ [`OAUTH_CODE_TTL_SECS`]); `deny` returns an `access_denied` redirect.
pub async fn authorize_rpc(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OAuthAuthorizeResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: OAuthAuthorizeRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.authorize input: {e}"),
        )
    })?;
    let state = req
        .state
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(s) = state {
        if s.len() > MAX_STATE_LEN {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("state must be at most {MAX_STATE_LEN} characters"),
            ));
        }
    }
    let (app, uri, scopes) = resolve_authorize_request(
        &ctx.db,
        req.client_id.trim(),
        Some(req.redirect_uri.as_str()),
        req.scope.as_deref(),
    )
    .await?;

    if !req.approve {
        let mut pairs = vec![("error", "access_denied")];
        if let Some(s) = state {
            pairs.push(("state", s));
        }
        return Ok(OAuthAuthorizeResponse {
            redirect_to: build_redirect(&uri, &pairs),
        });
    }

    let code_id = Uuid::new_v4().to_string();
    let (code, code_hash, _prefix) = mint_secret(OAUTH_CODE_PREFIX);
    ctx.db
        .insert_oauth_code(
            &code_id,
            &code_hash,
            &app.id,
            &user.id,
            &uri,
            &format_oauth_scopes(&scopes),
            &rfc3339_in(OAUTH_CODE_TTL_SECS),
        )
        .await
        .map_err(db_err)?;

    crate::audit::record_with(
        &ctx.db,
        &ctx.client,
        Some((&user.id, &user.username)),
        "oauth.grant_approved",
        Some(("oauth_application", &app.id)),
        None,
    )
    .await;

    let mut pairs = vec![("code", code.as_str())];
    if let Some(s) = state {
        pairs.push(("state", s));
    }
    Ok(OAuthAuthorizeResponse {
        redirect_to: build_redirect(&uri, &pairs),
    })
}
