//! Instance MCP server (AGT-01): streamable-HTTP Model Context Protocol endpoint
//! over the existing typed RPC domain handlers.
//!
//! - `POST /api/mcp` — a single JSON-RPC 2.0 message → `application/json`
//!   response. Notifications and client responses get `202 Accepted` (no body).
//! - `GET /api/mcp` — standalone SSE stream per the streamable-HTTP spec; this
//!   server never pushes server→client traffic, so GET returns `405`.
//!
//! Auth: `Authorization: Bearer <token>` carrying a classic (`oxidean_pat_…`) or
//! fine-grained (`oxidean_fg_…`) PAT, or the `oxidean_session` cookie. A
//! presented-but-invalid credential → `401` + `WWW-Authenticate: Bearer`. No
//! credential → anonymous context: public-readable tools still run, while
//! private or auth-required calls return `isError` content — never a
//! protocol-level failure (same `repo.not_found` anti-enumeration shape as the
//! RPC layer).
//!
//! `oxidean_oat_…` OAuth access tokens (authorization-code grants from
//! `/oauth/token`) resolve through `crate::oauth::authenticate_oauth_token`
//! and land in a synthetic classic-PAT row — granted OAuth scopes map onto the
//! same `Access` gates PATs face. Prefix dispatch keeps the three token
//! families disjoint.
//!
//! Instance gate (AGT-03): the endpoint only serves while MCP is enabled —
//! `instance_mcp_settings.enabled` (admin override) falling back to the
//! `OXIDEAN_MCP_ENABLED` env default (on when unset). Disabled → `404` with a
//! `mcp.disabled` JSON-RPC error for both POST and GET.
//!
//! Token scope mapping mirrors Smart HTTP / registry auth: classic `repo`
//! covers repository tools and `package:read`/`package:write` covers
//! `packages_list`; fine-grained `contents` plus the repository selection gate
//! repo-scoped tools (writes always require the repo to be inside the grant;
//! reads pass on public repos).

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use oxidean_core::{
    oauth_types::OAUTH_ACCESS_TOKEN_PREFIX, AppError, ClassicPatScope, ContentsPerm, FgRepoAccess,
    PatKind, RpcRequest, RpcResponse, CLASSIC_PAT_PREFIX, FINE_GRAINED_PAT_PREFIX,
    MCP_PROTOCOL_VERSIONS,
};
use oxidean_db::PatRow;
use serde_json::{json, Map, Value};

use crate::app::{
    build_rpc_ctx_with_session, resolve_session_token, session_token_from_headers, AppState,
};
use crate::auth::session::{sha256_hex, ResolvedSession};
use crate::repo::{
    fg_all_covers_repo, is_private_visibility, lookup_repo_row_or_redirect, not_found,
    resolve_owner_slug,
};
use crate::rpc::{self, ClientMeta};

/// Protocol versions this endpoint understands (newest last = advertised
/// default) — canonical list lives in `oxidean_core::MCP_PROTOCOL_VERSIONS`.
const PROTOCOL_VERSIONS: &[&str] = MCP_PROTOCOL_VERSIONS;

const WWW_AUTH: &str = r#"Bearer realm="Oxidean MCP""#;

/// `WWW-Authenticate` challenge — carries the RFC 9728 `resource_metadata`
/// pointer so OAuth-capable MCP clients can discover the authorization server
/// off a bare 401, plus the RFC 6750 `scope` hint naming the token's needed
/// capability set for clients that retry with an explicit scope. Falls back
/// to the bare realm when the resolved origin can't form a header value.
fn www_authenticate() -> HeaderValue {
    let challenge = format!(
        r#"Bearer realm="Oxidean MCP", resource_metadata="{}/.well-known/oauth-protected-resource", scope="{}""#,
        crate::public_origin::resolve_public_origin().trim_end_matches('/'),
        oxidean_core::FIRST_PARTY_SCOPES,
    );
    HeaderValue::from_str(&challenge).unwrap_or_else(|_| HeaderValue::from_static(WWW_AUTH))
}

// JSON-RPC 2.0 error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const SERVER_ERROR: i64 = -32000;

fn json_response(id: &Value, result: Value) -> Response {
    (
        StatusCode::OK,
        Json(json!({"jsonrpc": "2.0", "id": id, "result": result})),
    )
        .into_response()
}

fn json_error(id: &Value, code: i64, message: impl Into<String>) -> Response {
    (
        StatusCode::OK,
        Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": code, "message": message.into()},
        })),
    )
        .into_response()
}

fn json_error_status(
    id: &Value,
    code: i64,
    message: impl Into<String>,
    status: StatusCode,
) -> Response {
    (
        status,
        Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": code, "message": message.into()},
        })),
    )
        .into_response()
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, www_authenticate())],
        Json(json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {"code": SERVER_ERROR, "message": "unauthenticated"},
        })),
    )
        .into_response()
}

fn too_many_requests(retry_after: std::time::Duration) -> Response {
    let secs = retry_after.as_secs().max(1).to_string();
    let mut res = (
        StatusCode::TOO_MANY_REQUESTS,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {"code": SERVER_ERROR, "message": "too many failed authentication attempts"},
        })),
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(&secs) {
        res.headers_mut().insert(header::RETRY_AFTER, v);
    }
    res
}

fn internal_error_response() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {"code": SERVER_ERROR, "message": "internal error"},
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Instance gate (AGT-03) — `instance_mcp_settings` override else env default.
// ---------------------------------------------------------------------------

/// `OXIDEAN_MCP_ENABLED` parse — same truthy semantics as
/// `OXIDEAN_ACTIONS_ENABLED`: unset or anything but `0`/`false`/`no`/`off`/`""`
/// means on. Default **on** so self-host/local dev needs zero configuration.
fn mcp_env_truthy(raw: &str) -> bool {
    let t = raw.trim().to_ascii_lowercase();
    !(t.is_empty() || t == "0" || t == "false" || t == "no" || t == "off")
}

pub(crate) fn env_mcp_enabled() -> bool {
    std::env::var("OXIDEAN_MCP_ENABLED")
        .map(|v| mcp_env_truthy(&v))
        .unwrap_or(true)
}

/// Effective enable state: admin override row wins; env default otherwise.
/// Fails closed when the settings row cannot be read (a DB-less instance has
/// nothing to serve over MCP anyway).
async fn mcp_enabled(state: &AppState) -> bool {
    match state.db.get_mcp_settings().await {
        Ok(row) => row.enabled.unwrap_or(state.mcp_enabled),
        Err(e) => {
            tracing::error!(error = %e, "mcp settings read failed; endpoint disabled");
            false
        }
    }
}

/// Disabled surface — `404` (the endpoint is "not there" when off), still a
/// well-formed JSON-RPC error so clients can tell toggle-off from a bad URL.
fn mcp_disabled() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {"code": SERVER_ERROR, "message": "mcp.disabled: the MCP endpoint is disabled on this instance"},
        })),
    )
        .into_response()
}

/// `GET /api/mcp` — no standalone SSE stream (server→client pushes unsupported).
pub async fn handle_get(State(state): State<AppState>) -> Response {
    if !mcp_enabled(&state).await {
        return mcp_disabled();
    }
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, "POST")],
        Json(json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {"code": METHOD_NOT_FOUND, "message": "GET is reserved for SSE streams this server does not provide; POST JSON-RPC messages instead"},
        })),
    )
        .into_response()
}

/// `POST /api/mcp` — single JSON-RPC 2.0 message (streamable-HTTP, JSON mode).
pub async fn handle_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !mcp_enabled(&state).await {
        return mcp_disabled();
    }
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return json_error_status(
                &Value::Null,
                PARSE_ERROR,
                format!("parse error: {e}"),
                StatusCode::BAD_REQUEST,
            )
        }
    };
    let Some(obj) = msg.as_object() else {
        return json_error_status(
            &Value::Null,
            INVALID_REQUEST,
            "expected a single JSON-RPC request object",
            StatusCode::BAD_REQUEST,
        );
    };
    if let Some(v) = obj.get("jsonrpc") {
        if v.as_str() != Some("2.0") {
            return json_error_status(
                &obj.get("id").cloned().unwrap_or(Value::Null),
                INVALID_REQUEST,
                "jsonrpc must be \"2.0\"",
                StatusCode::BAD_REQUEST,
            );
        }
    }

    // Authenticate before method handling: a presented credential must be valid
    // regardless of the message type it rides on.
    let auth = match authenticate(&state, &headers).await {
        Ok(a) => a,
        Err(r) => return r,
    };

    let id = obj.get("id").cloned();
    let Some(method) = obj.get("method").and_then(|m| m.as_str()) else {
        // Client response or malformed frame — MCP requires no reply to either.
        if obj.contains_key("result") || obj.contains_key("error") {
            return StatusCode::ACCEPTED.into_response();
        }
        return json_error_status(
            &id.unwrap_or(Value::Null),
            INVALID_REQUEST,
            "missing method",
            StatusCode::BAD_REQUEST,
        );
    };
    let Some(id) = id else {
        // Notification (e.g. `notifications/initialized`) — acknowledge silently.
        return StatusCode::ACCEPTED.into_response();
    };
    let params = obj.get("params").cloned().unwrap_or_else(|| json!({}));
    let client = ClientMeta::from_headers(&headers);

    match method {
        "initialize" => json_response(&id, initialize_result(&params)),
        "ping" => json_response(&id, json!({})),
        "tools/list" => json_response(
            &id,
            json!({"tools": tool_defs().iter().map(tool_public).collect::<Vec<_>>()}),
        ),
        "tools/call" => match call_tool(&state, &auth, &client, &params).await {
            Ok(result) => json_response(&id, result),
            Err((code, message)) => json_error(&id, code, message),
        },
        "resources/list" => json_response(&id, json!({"resources": []})),
        "resources/templates/list" => {
            json_response(&id, json!({"resourceTemplates": resource_templates()}))
        }
        "resources/read" => match read_resource(&state, &auth, &client, &params).await {
            Ok(result) => json_response(&id, result),
            Err((code, message)) => json_error(&id, code, message),
        },
        "prompts/list" => json_response(&id, json!({"prompts": []})),
        _ => json_error(&id, METHOD_NOT_FOUND, format!("method not found: {method}")),
    }
}

fn initialize_result(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(|v| v.as_str());
    let version = match requested {
        Some(v) if PROTOCOL_VERSIONS.contains(&v) => v.to_string(),
        _ => PROTOCOL_VERSIONS
            .last()
            .copied()
            .unwrap_or("2025-06-18")
            .to_string(),
    };
    json!({
        "protocolVersion": version,
        "capabilities": {
            "tools": {"listChanged": false},
            "resources": {"listChanged": false},
            "prompts": {"listChanged": false},
        },
        "serverInfo": {"name": "oxidean", "version": env!("CARGO_PKG_VERSION")},
        "instructions": "Oxidean forge MCP endpoint. Tools expose repositories, issues, pull requests, Actions runs, packages, and search. Send `Authorization: Bearer oxidean_pat_…` (or `oxidean_fg_…`) for private data and writes; anonymous callers get public data only.",
    })
}

// ---------------------------------------------------------------------------
// Auth — Bearer PAT (classic + fine-grained) or session cookie.
// ---------------------------------------------------------------------------

struct McpAuth {
    /// Session handed to `RpcCtx` — real cookie session, or a synthesized one
    /// for PAT auth so domain handlers reuse their normal identity checks.
    session: Option<ResolvedSession>,
    /// Resolved PAT when Bearer auth was used — drives token-scope checks.
    pat: Option<PatRow>,
}

fn limiter_lock(
    state: &AppState,
) -> std::sync::MutexGuard<'_, crate::pat::rate_limit::FailedAuthLimiter> {
    state
        .git_auth_limiter
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Bearer credential families recognized at this endpoint. Prefix dispatch is
/// the single point that routes a token to its resolver — `oxidean_pat_`,
/// `oxidean_fg_`, and `oxidean_oat_` stay disjoint by construction.
enum BearerToken<'a> {
    /// `oxidean_pat_…` / `oxidean_fg_…` — resolved against the PAT tables.
    Pat(&'a str),
    /// `oxidean_oat_…` — OAuth2 access token minted by this instance.
    Oat(&'a str),
    /// Unrecognized prefix — failed auth.
    Unknown,
}

fn classify_bearer(token: &str) -> BearerToken<'_> {
    if token.starts_with(CLASSIC_PAT_PREFIX) || token.starts_with(FINE_GRAINED_PAT_PREFIX) {
        BearerToken::Pat(token)
    } else if token.starts_with(OAUTH_ACCESS_TOKEN_PREFIX) {
        BearerToken::Oat(token)
    } else {
        BearerToken::Unknown
    }
}

fn pat_expired(expires_at: &Option<String>) -> bool {
    let Some(raw) = expires_at.as_deref() else {
        return false;
    };
    match chrono::DateTime::parse_from_rfc3339(raw) {
        Ok(dt) => dt.with_timezone(&Utc) <= Utc::now(),
        Err(_) => true,
    }
}

async fn touch_last_used(state: &AppState, pat_id: &str, ip: Option<&str>) {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    if let Err(e) = state.db.touch_pat_last_used(pat_id, &now, ip).await {
        tracing::warn!(error = %e, pat_id, "touch_pat_last_used failed");
    }
}

async fn touch_oauth_last_used(state: &AppState, token_id: &str, ip: Option<&str>) {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    if let Err(e) = state
        .db
        .touch_oauth_token_last_used(token_id, &now, ip)
        .await
    {
        tracing::warn!(error = %e, token_id, "touch_oauth_token_last_used failed");
    }
}

#[allow(clippy::result_large_err)]
async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<McpAuth, Response> {
    if let Some(raw) = headers.get(header::AUTHORIZATION) {
        let raw = raw.to_str().map_err(|_| unauthorized())?;
        let Some(token) = raw
            .strip_prefix("Bearer ")
            .or_else(|| raw.strip_prefix("bearer "))
        else {
            return Err(unauthorized());
        };
        return resolve_bearer_identity(state, headers, token.trim()).await;
    }
    let token = session_token_from_headers(headers);
    let client = ClientMeta::from_headers(headers);
    let session = resolve_session_token(state, token.as_deref(), &client).await;
    Ok(McpAuth { session, pat: None })
}

#[allow(clippy::result_large_err)]
/// Resolve `Authorization: Bearer <token>` to an [`McpAuth`].
///
/// Extension point (AGT-03): `oxidean_pat_`/`oxidean_fg_` tokens resolve through
/// [`authenticate_pat`] today; `oxidean_oat_…` OAuth access tokens route to
/// [`resolve_oat_bearer`], the seam the OAuth provider resolver lands in. Any
/// other prefix is failed auth — recorded against the shared D-26 IP limiter.
async fn resolve_bearer_identity(
    state: &AppState,
    headers: &HeaderMap,
    token: &str,
) -> Result<McpAuth, Response> {
    let client = ClientMeta::from_headers(headers);
    let ip = client
        .ip_address
        .clone()
        .unwrap_or_else(|| "unknown".into());
    // D-26 IP bucket — shared counters with Smart HTTP failed auth.
    if let Err(retry) = limiter_lock(state).check_ip(&ip) {
        return Err(too_many_requests(retry));
    }
    match classify_bearer(token) {
        BearerToken::Pat(token) => authenticate_pat(state, &client, token).await,
        BearerToken::Oat(token) => resolve_oat_bearer(state, &client, token).await,
        BearerToken::Unknown => {
            limiter_lock(state).record_ip(&ip);
            Err(unauthorized())
        }
    }
}

#[allow(clippy::result_large_err)]
/// OAuth access-token (`oxidean_oat_…`) resolution — same hash lookup, expiry,
/// revocation, and banned-owner checks the packages/git-HTTP paths get via
/// [`crate::oauth::authenticate_oauth_token`]. The OAuth scopes land in the
/// synthetic classic-PAT row (`synthetic_pat` keeps the `repo` /
/// `package:read` / `package:write` scope strings), so the `Access` gates
/// below apply unchanged.
async fn resolve_oat_bearer(
    state: &AppState,
    client: &ClientMeta,
    token: &str,
) -> Result<McpAuth, Response> {
    let ip = client
        .ip_address
        .clone()
        .unwrap_or_else(|| "unknown".into());
    let ident = match crate::oauth::authenticate_oauth_token(&state.db, token).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            limiter_lock(state).record_ip(&ip);
            return Err(unauthorized());
        }
        Err(e) => {
            tracing::error!(error = %e, "mcp oauth token lookup failed");
            return Err(internal_error_response());
        }
    };
    touch_oauth_last_used(state, &ident.token.id, client.ip_address.as_deref()).await;
    let session = ResolvedSession {
        session_id: format!("oat:{}", ident.token.id),
        user_id: ident.user.id.clone(),
        remember_me: false,
        expires_at: Utc::now() + chrono::Duration::hours(24),
    };
    Ok(McpAuth {
        session: Some(session),
        pat: Some(ident.synthetic_pat()),
    })
}

#[allow(clippy::result_large_err)]
/// Bearer PAT → PAT row + owner. Mirrors `git_smart_http::authenticate_pat`
/// (hash lookup, expiry, banned-owner) without the username/alias dance —
/// identity comes from the token hash alone. The caller
/// (`resolve_bearer_identity`) has already classified the prefix and run the
/// IP rate-limit check.
async fn authenticate_pat(
    state: &AppState,
    client: &ClientMeta,
    token: &str,
) -> Result<McpAuth, Response> {
    let ip = client
        .ip_address
        .clone()
        .unwrap_or_else(|| "unknown".into());
    let token_hash = sha256_hex(token.as_bytes());
    let pat = match state.db.find_pat_by_token_hash(&token_hash).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            limiter_lock(state).record_ip(&ip);
            return Err(unauthorized());
        }
        Err(e) => {
            tracing::error!(error = %e, "mcp find_pat_by_token_hash failed");
            return Err(internal_error_response());
        }
    };
    if pat_expired(&pat.expires_at) {
        limiter_lock(state).record_ip(&ip);
        return Err(unauthorized());
    }
    let owner = match state.db.find_user_by_id(&pat.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            limiter_lock(state).record_ip(&ip);
            return Err(unauthorized());
        }
        Err(e) => {
            tracing::error!(error = %e, "mcp find_user_by_id failed");
            return Err(internal_error_response());
        }
    };
    if owner.banned_at.is_some() {
        limiter_lock(state).record_ip(&ip);
        return Err(unauthorized());
    }
    touch_last_used(state, &pat.id, client.ip_address.as_deref()).await;

    // Synthetic session so `RpcCtx` users see the PAT owner's identity.
    let session = ResolvedSession {
        session_id: format!("pat:{}", pat.id),
        user_id: owner.id.clone(),
        remember_me: false,
        expires_at: Utc::now() + chrono::Duration::hours(24),
    };
    Ok(McpAuth {
        session: Some(session),
        pat: Some(pat),
    })
}

// ---------------------------------------------------------------------------
// Tool surface — thin wrappers over `rpc::dispatch`.
// ---------------------------------------------------------------------------

/// Token-scope requirement for a tool (PAT enforcement; sessions are unlimited
/// beyond what the underlying handler already checks).
#[derive(Clone, Copy)]
enum Access {
    /// Public data (explore) — no token-scope gate.
    Public,
    /// Identity-level (`repo.listMine`) — any token kind; FG `selected` gets
    /// its result set filtered to granted repos.
    Identity,
    /// Repo read on `owner`+`name` args — classic `repo` / FG `contents:read`.
    RepoRead,
    /// Repo write on `owner`+`name` args — classic `repo` / FG `contents:write`.
    RepoWrite,
    /// `package:read` or `package:write` scope.
    PackageRead,
}

struct ToolDef {
    name: &'static str,
    description: &'static str,
    /// Underlying RPC procedure; empty when the tool picks one from arguments.
    procedure: &'static str,
    access: Access,
    input_schema: Value,
}

fn repo_identity_props() -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(
        "owner".into(),
        json!({"type": "string", "description": "Repository owner (user or org slug)"}),
    );
    m.insert(
        "name".into(),
        json!({"type": "string", "description": "Repository name"}),
    );
    m
}

fn repo_number_schema() -> Value {
    let mut props = repo_identity_props();
    props.insert(
        "number".into(),
        json!({"type": "integer", "description": "Issue / pull request number"}),
    );
    json!({"type": "object", "properties": props, "required": ["owner", "name", "number"]})
}

fn tool_defs() -> Vec<ToolDef> {
    let repo_props = repo_identity_props();
    let repo_schema =
        json!({"type": "object", "properties": repo_props, "required": ["owner", "name"]});
    vec![
        ToolDef {
            name: "repo_list",
            description: "List repositories. With `owner`, lists that user/org's repos visible to the caller; without it, lists the authenticated user's own repositories.",
            procedure: "",
            access: Access::Identity,
            input_schema: json!({
                "type": "object",
                "properties": {"owner": {"type": "string", "description": "User or org slug"}},
            }),
        },
        ToolDef {
            name: "repo_get",
            description: "Get repository metadata: visibility, default branch, description, stats.",
            procedure: "repo.get",
            access: Access::RepoRead,
            input_schema: repo_schema.clone(),
        },
        ToolDef {
            name: "repo_tree",
            description: "List files/directories at a path and ref (branch, tag, or SHA).",
            procedure: "repo.tree",
            access: Access::RepoRead,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "name": {"type": "string"},
                    "ref": {"type": "string", "description": "Branch/tag/SHA; empty = default branch"},
                    "path": {"type": "string", "description": "Directory path inside the repo (default root)"},
                },
                "required": ["owner", "name"],
            }),
        },
        ToolDef {
            name: "issue_list",
            description: "List issues in a repository. `state`: open (default) | closed | all; optional `q` title/body filter.",
            procedure: "issue.list",
            access: Access::RepoRead,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "name": {"type": "string"},
                    "state": {"type": "string", "enum": ["open", "closed", "all"]},
                    "q": {"type": "string"},
                    "offset": {"type": "integer"},
                    "limit": {"type": "integer"},
                },
                "required": ["owner", "name"],
            }),
        },
        ToolDef {
            name: "issue_get",
            description: "Get a single issue by number (labels, assignees, reactions, body).",
            procedure: "issue.get",
            access: Access::RepoRead,
            input_schema: repo_number_schema(),
        },
        ToolDef {
            name: "issue_create",
            description: "Create an issue. Requires write permission on the repository.",
            procedure: "issue.create",
            access: Access::RepoWrite,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "name": {"type": "string"},
                    "title": {"type": "string"},
                    "body": {"type": "string"},
                },
                "required": ["owner", "name", "title"],
            }),
        },
        ToolDef {
            name: "issue_comment",
            description: "Add a comment to an issue. Requires write permission on the repository.",
            procedure: "issue.comments.create",
            access: Access::RepoWrite,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "name": {"type": "string"},
                    "number": {"type": "integer"},
                    "body": {"type": "string"},
                },
                "required": ["owner", "name", "number", "body"],
            }),
        },
        ToolDef {
            name: "issue_list_comments",
            description: "List comments on an issue, oldest first.",
            procedure: "issue.comments.list",
            access: Access::RepoRead,
            input_schema: repo_number_schema(),
        },
        ToolDef {
            name: "pull_list",
            description: "List pull requests in a repository. `state`: open (default) | closed | merged | all.",
            procedure: "pull.list",
            access: Access::RepoRead,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "name": {"type": "string"},
                    "state": {"type": "string", "enum": ["open", "closed", "merged", "all"]},
                    "q": {"type": "string"},
                    "offset": {"type": "integer"},
                    "limit": {"type": "integer"},
                },
                "required": ["owner", "name"],
            }),
        },
        ToolDef {
            name: "pull_get",
            description: "Get a single pull request by number (head/base refs, state, body).",
            procedure: "pull.get",
            access: Access::RepoRead,
            input_schema: repo_number_schema(),
        },
        ToolDef {
            name: "actions_list_runs",
            description: "List Actions workflow runs for a repository.",
            procedure: "repo.actions.listRuns",
            access: Access::RepoRead,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "name": {"type": "string"},
                    "page": {"type": "integer"},
                    "per_page": {"type": "integer"},
                },
                "required": ["owner", "name"],
            }),
        },
        ToolDef {
            name: "actions_get_run",
            description: "Get an Actions workflow run with its jobs.",
            procedure: "repo.actions.getRun",
            access: Access::RepoRead,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "name": {"type": "string"},
                    "run_id": {"type": "string"},
                },
                "required": ["owner", "name", "run_id"],
            }),
        },
        ToolDef {
            name: "packages_list",
            description: "List packages for an owner (user or org slug) or a repository id.",
            procedure: "packages.list",
            access: Access::PackageRead,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "owner": {"type": "string", "description": "User or org slug"},
                    "repository_id": {"type": "string"},
                },
            }),
        },
        ToolDef {
            name: "search_repos",
            description: "Search public repositories on this instance (explore).",
            procedure: "repo.explore",
            access: Access::Public,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "q": {"type": "string"},
                    "offset": {"type": "integer"},
                    "limit": {"type": "integer"},
                },
            }),
        },
        ToolDef {
            name: "search_issues",
            description: "Search issues in a repository (supports `is:open`/`is:closed`, `author:`, `path:`-style qualifiers where implemented).",
            procedure: "repo.search",
            access: Access::RepoRead,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "name": {"type": "string"},
                    "q": {"type": "string"},
                    "offset": {"type": "integer"},
                    "limit": {"type": "integer"},
                },
                "required": ["owner", "name", "q"],
            }),
        },
        ToolDef {
            name: "search_pulls",
            description: "Search pull requests in a repository.",
            procedure: "repo.search",
            access: Access::RepoRead,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "name": {"type": "string"},
                    "q": {"type": "string"},
                    "offset": {"type": "integer"},
                    "limit": {"type": "integer"},
                },
                "required": ["owner", "name", "q"],
            }),
        },
        ToolDef {
            name: "search_code",
            description: "Search code in a repository (git grep on a ref; default branch when `ref` omitted).",
            procedure: "repo.search",
            access: Access::RepoRead,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "name": {"type": "string"},
                    "q": {"type": "string"},
                    "ref": {"type": "string"},
                    "offset": {"type": "integer"},
                    "limit": {"type": "integer"},
                },
                "required": ["owner", "name", "q"],
            }),
        },
    ]
}

fn tool_public(t: &ToolDef) -> Value {
    json!({
        "name": t.name,
        "description": t.description,
        "inputSchema": t.input_schema,
    })
}

/// Map tool name + arguments → (RPC procedure, RPC input).
fn build_rpc_input(tool: &ToolDef, args: &Value) -> Result<(String, Value), (i64, String)> {
    match tool.name {
        "repo_list" => {
            let owner = args
                .get("owner")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            match owner {
                Some(owner) => Ok(("repo.listByOwner".into(), json!({"owner": owner}))),
                None => Ok(("repo.listMine".into(), json!({}))),
            }
        }
        "search_issues" | "search_pulls" | "search_code" => {
            let mut input = args.clone();
            let kind = match tool.name {
                "search_issues" => "issues",
                "search_pulls" => "pulls",
                _ => "code",
            };
            input["type"] = json!(kind);
            Ok((tool.procedure.into(), input))
        }
        // The tool schema advertises `ref` as optional (empty → default branch),
        // while `repo.tree` requires the field — default it here.
        "repo_tree" => {
            let mut input = args.clone();
            if input.get("ref").is_none() {
                input["ref"] = json!("");
            }
            Ok((tool.procedure.into(), input))
        }
        _ => Ok((tool.procedure.into(), args.clone())),
    }
}

fn tool_ok(data: Value) -> Value {
    let text = serde_json::to_string(&data).unwrap_or_else(|_| "{}".into());
    let mut result = json!({
        "isError": false,
        "content": [{"type": "text", "text": text}],
    });
    if data.is_object() {
        result["structuredContent"] = data;
    }
    result
}

fn tool_error(e: &AppError) -> Value {
    json!({
        "isError": true,
        "content": [{"type": "text", "text": format!("{}: {}", e.code, e.message)}],
    })
}

fn internal_app_error() -> AppError {
    AppError::new("mcp.internal", "internal error")
}

fn scope_denied(need: &str) -> AppError {
    AppError::new(
        "mcp.insufficient_scope",
        format!("token does not grant {need} access for this operation"),
    )
}

fn pat_scopes(pat: &PatRow) -> Vec<String> {
    pat.scopes_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

fn pat_has_package_read(pat: &PatRow) -> bool {
    pat_scopes(pat)
        .iter()
        .any(|s| s == "package:read" || s == "package:write")
}

/// PAT scope gate — session-cookie callers skip this entirely (the RPC layer
/// already enforces their ACL). Denials land as `isError` tool content.
async fn enforce_pat_scope(
    state: &AppState,
    pat: &PatRow,
    access: Access,
    input: &Value,
) -> Result<(), AppError> {
    let kind = PatKind::parse(&pat.kind).map_err(|_| internal_app_error())?;
    match access {
        Access::Public | Access::Identity => Ok(()),
        Access::PackageRead => {
            if pat_has_package_read(pat) {
                Ok(())
            } else {
                Err(scope_denied("package:read"))
            }
        }
        Access::RepoRead | Access::RepoWrite => match kind {
            PatKind::Classic => {
                if pat_scopes(pat)
                    .iter()
                    .any(|s| s == ClassicPatScope::Repo.as_str())
                {
                    Ok(())
                } else {
                    Err(scope_denied("repo"))
                }
            }
            PatKind::FineGrained => {
                fg_repo_access_check(state, pat, input, matches!(access, Access::RepoWrite)).await
            }
        },
    }
}

/// Fine-grained repo grant check. Unknown repos defer to the handler (which
/// answers `repo.not_found`); uncovered private repos answer `repo.not_found`
/// here — same anti-enumeration shape either way.
async fn fg_repo_access_check(
    state: &AppState,
    pat: &PatRow,
    input: &Value,
    need_write: bool,
) -> Result<(), AppError> {
    let owner = input
        .get("owner")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let name = input
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if owner.is_empty() || name.is_empty() {
        return Ok(()); // handler validates required fields
    }
    let pair = lookup_repo_row_or_redirect(&state.db, owner, name)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "mcp repo lookup failed");
            internal_app_error()
        })?;
    let Some((row, owner_ref)) = pair else {
        return Ok(()); // missing → handler emits repo.not_found uniformly
    };
    let covered = match FgRepoAccess::parse(pat.repo_access.as_deref().unwrap_or("")) {
        Ok(FgRepoAccess::Selected) => pat.repository_ids.iter().any(|id| id == &row.id),
        Ok(FgRepoAccess::All) => fg_all_covers_repo(&state.db, &pat.user_id, &owner_ref)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "mcp fg_all_covers_repo failed");
                internal_app_error()
            })?,
        Err(_) => false,
    };
    // Reads: public repos are anonymous-readable anyway; the FG grant only
    // gates private data. Writes: the repo must sit inside the grant.
    let repo_ok = if need_write {
        covered
    } else {
        covered || !is_private_visibility(&row.visibility)
    };
    if !repo_ok {
        return Err(not_found());
    }
    let contents_ok = match ContentsPerm::parse(pat.contents_perm.as_deref().unwrap_or("")) {
        Ok(ContentsPerm::Write) => true,
        Ok(ContentsPerm::Read) => !need_write,
        Err(_) => false,
    };
    if !contents_ok {
        return Err(scope_denied("contents:write"));
    }
    Ok(())
}

/// `repo_list` narrowing for fine-grained tokens: drop repos outside the grant
/// (public repos stay — they are anonymous-readable anyway).
async fn filter_repo_list(
    state: &AppState,
    pat: &PatRow,
    args: &Value,
    data: &mut Value,
) -> Result<(), AppError> {
    if PatKind::parse(&pat.kind) != Ok(PatKind::FineGrained) {
        return Ok(());
    }
    let access = FgRepoAccess::parse(pat.repo_access.as_deref().unwrap_or(""));
    let owner_arg = args
        .get("owner")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    // FG `all` covers personal repos + orgs where the subject is Owner/Admin.
    // `repo.listMine` rows are all personal → covered. `repo.listByOwner` shares
    // one owner → evaluate the grant once against it.
    let all_owner_covered = match access {
        Ok(FgRepoAccess::All) => match owner_arg {
            Some(owner) => match resolve_owner_slug(&state.db, owner).await {
                Ok(Some(o)) => fg_all_covers_repo(&state.db, &pat.user_id, &o)
                    .await
                    .map_err(|e| {
                        tracing::error!(error = %e, "mcp fg_all_covers_repo failed");
                        internal_app_error()
                    })?,
                Ok(None) => false,
                Err(e) => {
                    tracing::error!(error = %e, "mcp resolve_owner_slug failed");
                    return Err(internal_app_error());
                }
            },
            None => true,
        },
        _ => false,
    };
    let selected = matches!(access, Ok(FgRepoAccess::Selected));
    let Some(repos) = data.get_mut("repos").and_then(|v| v.as_array_mut()) else {
        return Ok(());
    };
    repos.retain(|r| {
        let public = r
            .get("visibility")
            .and_then(|v| v.as_str())
            .map(|v| !is_private_visibility(v))
            .unwrap_or(false);
        if public || all_owner_covered {
            return true;
        }
        if selected {
            let id = r.get("id").and_then(|v| v.as_str()).unwrap_or("");
            return pat.repository_ids.iter().any(|x| x == id);
        }
        false
    });
    Ok(())
}

async fn dispatch_rpc(
    state: &AppState,
    auth: &McpAuth,
    client: &ClientMeta,
    procedure: &str,
    input: Value,
) -> Result<Value, AppError> {
    let mut ctx = build_rpc_ctx_with_session(state, auth.session.clone(), client.clone());
    let resp = rpc::dispatch(
        &mut ctx,
        RpcRequest {
            procedure: procedure.to_string(),
            input,
        },
    )
    .await;
    match resp {
        RpcResponse::Ok { data, .. } => Ok(data),
        RpcResponse::Err { error, .. } => Err(error),
    }
}

async fn call_tool(
    state: &AppState,
    auth: &McpAuth,
    client: &ClientMeta,
    params: &Value,
) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| (INVALID_PARAMS, "params.name is required".to_string()))?;
    let args = match params.get("arguments") {
        None | Some(Value::Null) => json!({}),
        Some(v) if v.is_object() => v.clone(),
        _ => {
            return Err((
                INVALID_PARAMS,
                "params.arguments must be an object".to_string(),
            ))
        }
    };
    let Some(tool) = tool_defs().into_iter().find(|t| t.name == name) else {
        return Err((INVALID_PARAMS, format!("unknown tool: {name}")));
    };
    let (procedure, input) = build_rpc_input(&tool, &args)?;

    if let Some(pat) = &auth.pat {
        if let Err(e) = enforce_pat_scope(state, pat, tool.access, &input).await {
            return Ok(tool_error(&e));
        }
    }
    match dispatch_rpc(state, auth, client, &procedure, input).await {
        Ok(mut data) => {
            if tool.name == "repo_list" {
                if let Some(pat) = &auth.pat {
                    if let Err(e) = filter_repo_list(state, pat, &args, &mut data).await {
                        return Ok(tool_error(&e));
                    }
                }
            }
            Ok(tool_ok(data))
        }
        Err(e) => Ok(tool_error(&e)),
    }
}

// ---------------------------------------------------------------------------
// Resources — repo blobs + issue/PR bodies behind the same ACL + scope seam.
// ---------------------------------------------------------------------------

fn resource_templates() -> Value {
    json!([
        {
            "uriTemplate": "oxidean://repo/{owner}/{name}/blob/{path}?ref={ref}",
            "name": "repo_file",
            "description": "File contents at a ref (default branch when `ref` is omitted)",
            "mimeType": "text/plain",
        },
        {
            "uriTemplate": "oxidean://repo/{owner}/{name}/issue/{number}",
            "name": "issue",
            "description": "Issue title, state, and body as Markdown",
            "mimeType": "text/markdown",
        },
        {
            "uriTemplate": "oxidean://repo/{owner}/{name}/pull/{number}",
            "name": "pull_request",
            "description": "Pull request title, state, and body as Markdown",
            "mimeType": "text/markdown",
        },
    ])
}

enum ResourceTarget {
    Blob { path: String, ref_name: String },
    Issue { number: i64 },
    Pull { number: i64 },
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `oxidean://repo/{owner}/{name}/blob/{path…}?ref={ref}` |
/// `…/issue/{number}` | `…/pull/{number}`
fn parse_resource_uri(uri: &str) -> Result<(String, String, ResourceTarget), (i64, String)> {
    let rest = uri
        .strip_prefix("oxidean://repo/")
        .ok_or_else(|| (INVALID_PARAMS, format!("unsupported resource uri: {uri}")))?;
    let (path_part, query) = match rest.split_once('?') {
        Some((p, q)) => (p, q),
        None => (rest, ""),
    };
    let segs: Vec<&str> = path_part.split('/').collect();
    if segs.len() < 3 || segs[0].is_empty() || segs[1].is_empty() {
        return Err((INVALID_PARAMS, format!("unsupported resource uri: {uri}")));
    }
    let owner = pct_decode(segs[0]);
    let name = pct_decode(segs[1]);
    match segs[2] {
        "blob" if segs.len() >= 4 => {
            let path = segs[3..]
                .iter()
                .map(|s| pct_decode(s))
                .collect::<Vec<_>>()
                .join("/");
            let ref_name = url::form_urlencoded::parse(query.as_bytes())
                .find(|(k, _)| k == "ref")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default();
            Ok((owner, name, ResourceTarget::Blob { path, ref_name }))
        }
        "issue" if segs.len() == 4 => {
            let number = segs[3].parse::<i64>().map_err(|_| {
                (
                    INVALID_PARAMS,
                    format!("invalid issue number in uri: {uri}"),
                )
            })?;
            Ok((owner, name, ResourceTarget::Issue { number }))
        }
        "pull" if segs.len() == 4 => {
            let number = segs[3]
                .parse::<i64>()
                .map_err(|_| (INVALID_PARAMS, format!("invalid pull number in uri: {uri}")))?;
            Ok((owner, name, ResourceTarget::Pull { number }))
        }
        _ => Err((INVALID_PARAMS, format!("unsupported resource uri: {uri}"))),
    }
}

fn resource_domain_error(e: &AppError) -> (i64, String) {
    (SERVER_ERROR, format!("{}: {}", e.code, e.message))
}

async fn read_resource(
    state: &AppState,
    auth: &McpAuth,
    client: &ClientMeta,
    params: &Value,
) -> Result<Value, (i64, String)> {
    let uri = params
        .get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| (INVALID_PARAMS, "params.uri is required".to_string()))?;
    let (owner, name, target) = parse_resource_uri(uri)?;

    if let Some(pat) = &auth.pat {
        let input = json!({"owner": owner, "name": name});
        if let Err(e) = enforce_pat_scope(state, pat, Access::RepoRead, &input).await {
            return Err(resource_domain_error(&e));
        }
    }

    match target {
        ResourceTarget::Blob { path, ref_name } => {
            let data = dispatch_rpc(
                state,
                auth,
                client,
                "repo.blob",
                json!({"owner": owner, "name": name, "ref": ref_name, "path": path}),
            )
            .await
            .map_err(|e| resource_domain_error(&e))?;
            let content = data
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let is_binary = data
                .get("is_binary")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let entry = if is_binary {
                json!({"uri": uri, "mimeType": "application/octet-stream", "blob": content})
            } else {
                json!({"uri": uri, "mimeType": "text/plain", "text": content})
            };
            Ok(json!({"contents": [entry]}))
        }
        ResourceTarget::Issue { number } | ResourceTarget::Pull { number } => {
            let (procedure, label) = match target {
                ResourceTarget::Issue { .. } => ("issue.get", "Issue"),
                _ => ("pull.get", "Pull request"),
            };
            read_doc_resource(
                state, auth, client, uri, &owner, &name, number, procedure, label,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn read_doc_resource(
    state: &AppState,
    auth: &McpAuth,
    client: &ClientMeta,
    uri: &str,
    owner: &str,
    name: &str,
    number: i64,
    procedure: &str,
    label: &str,
) -> Result<Value, (i64, String)> {
    let data = dispatch_rpc(
        state,
        auth,
        client,
        procedure,
        json!({"owner": owner, "name": name, "number": number}),
    )
    .await
    .map_err(|e| resource_domain_error(&e))?;
    let title = data.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let state_s = data.get("state").and_then(|v| v.as_str()).unwrap_or("");
    let author = data
        .get("author_username")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let body = data.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let text =
        format!("# {label} #{number}: {title}\n\nState: {state_s}\nAuthor: @{author}\n\n{body}\n");
    Ok(json!({
        "contents": [{"uri": uri, "mimeType": "text/markdown", "text": text}],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- env flag parsing ---------------------------------------------------

    #[test]
    fn mcp_env_truthy_values() {
        for v in ["", "0", "false", "no", "off", " FALSE ", "Off", "0 "] {
            assert!(!mcp_env_truthy(v), "{v:?} must be falsy");
        }
        for v in ["1", "true", "yes", "on", "anything", " enabled "] {
            assert!(mcp_env_truthy(v), "{v:?} must be truthy");
        }
    }

    // -- percent decoding ----------------------------------------------------

    #[test]
    fn pct_decode_basics() {
        assert_eq!(pct_decode("plain"), "plain");
        assert_eq!(pct_decode("a%20b"), "a b");
        assert_eq!(pct_decode("dir%2Ffile"), "dir/file");
        assert_eq!(pct_decode("%E2%82%AC"), "\u{20ac}");
        // Truncated / invalid escapes pass through literally.
        assert_eq!(pct_decode("x%4"), "x%4");
        assert_eq!(pct_decode("x%zz"), "x%zz");
        assert_eq!(pct_decode("100%"), "100%");
    }

    // -- resource URI parsing ------------------------------------------------

    #[test]
    fn parse_resource_uri_blob() {
        let (owner, name, target) =
            parse_resource_uri("oxidean://repo/octo/demo/blob/src/main.rs?ref=dev").unwrap();
        assert_eq!(owner, "octo");
        assert_eq!(name, "demo");
        match target {
            ResourceTarget::Blob { path, ref_name } => {
                assert_eq!(path, "src/main.rs");
                assert_eq!(ref_name, "dev");
            }
            _ => panic!("expected blob"),
        }
    }

    #[test]
    fn parse_resource_uri_blob_defaults_and_decoding() {
        let (owner, name, target) =
            parse_resource_uri("oxidean://repo/o%20x/d%2Fe/blob/README.md").unwrap();
        assert_eq!(owner, "o x");
        assert_eq!(name, "d/e");
        match target {
            ResourceTarget::Blob { path, ref_name } => {
                assert_eq!(path, "README.md");
                assert_eq!(ref_name, "");
            }
            _ => panic!("expected blob"),
        }
        // Percent-decoded path segments rejoin with '/'.
        let (_, _, target) =
            parse_resource_uri("oxidean://repo/o/n/blob/a%20b/c?ref=main&other=1").unwrap();
        match target {
            ResourceTarget::Blob { path, ref_name } => {
                assert_eq!(path, "a b/c");
                assert_eq!(ref_name, "main");
            }
            _ => panic!("expected blob"),
        }
    }

    #[test]
    fn parse_resource_uri_issue_pull() {
        let (_, _, target) = parse_resource_uri("oxidean://repo/o/n/issue/7").unwrap();
        assert!(matches!(target, ResourceTarget::Issue { number: 7 }));
        let (_, _, target) = parse_resource_uri("oxidean://repo/o/n/pull/42").unwrap();
        assert!(matches!(target, ResourceTarget::Pull { number: 42 }));
    }

    #[test]
    fn parse_resource_uri_rejects_bad_shapes() {
        for uri in [
            "https://example.com/x",
            "oxidean://org/o/n",
            "oxidean://repo/o",
            "oxidean://repo//n/issue/1",
            "oxidean://repo/o/n/issue",
            "oxidean://repo/o/n/issue/abc",
            "oxidean://repo/o/n/issue/1/extra",
            "oxidean://repo/o/n/pull/xyz",
            "oxidean://repo/o/n/blob",
            "oxidean://repo/o/n/unknown/1",
        ] {
            match parse_resource_uri(uri) {
                Err((code, _)) => assert_eq!(code, INVALID_PARAMS, "{uri}"),
                Ok(_) => panic!("{uri} must not parse"),
            }
        }
    }

    // -- bearer classification ----------------------------------------------

    #[test]
    fn classify_bearer_families() {
        assert!(matches!(
            classify_bearer("oxidean_pat_abcdef"),
            BearerToken::Pat(_)
        ));
        assert!(matches!(
            classify_bearer("oxidean_fg_abcdef"),
            BearerToken::Pat(_)
        ));
        assert!(matches!(
            classify_bearer("oxidean_oat_abcdef"),
            BearerToken::Oat(_)
        ));
        assert!(matches!(classify_bearer("ghp_abc"), BearerToken::Unknown));
        assert!(matches!(classify_bearer(""), BearerToken::Unknown));
        assert!(matches!(
            classify_bearer("oxidean_zzz_abcdef"),
            BearerToken::Unknown
        ));
    }

    // -- PAT expiry ----------------------------------------------------------

    #[test]
    fn pat_expired_cases() {
        assert!(!pat_expired(&None));
        let past = (Utc::now() - chrono::Duration::hours(1))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        assert!(pat_expired(&Some(past)));
        let future = (Utc::now() + chrono::Duration::hours(1))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        assert!(!pat_expired(&Some(future)));
        // Unparseable timestamps fail closed.
        assert!(pat_expired(&Some("not-a-date".into())));
    }

    // -- initialize negotiation ----------------------------------------------

    #[test]
    fn initialize_negotiates_protocol_version() {
        for v in PROTOCOL_VERSIONS {
            let out = initialize_result(&json!({"protocolVersion": v}));
            assert_eq!(out["protocolVersion"], *v);
        }
        for params in [
            json!({"protocolVersion": "1999-01-01"}),
            json!({}),
            json!({"protocolVersion": 42}),
        ] {
            let out = initialize_result(&params);
            assert_eq!(
                out["protocolVersion"],
                *PROTOCOL_VERSIONS.last().unwrap(),
                "{params}"
            );
        }
        let out = initialize_result(&json!({}));
        assert_eq!(out["serverInfo"]["name"], "oxidean");
        assert!(out["capabilities"]["tools"].is_object());
        assert!(out["capabilities"]["resources"].is_object());
        assert!(out["capabilities"]["prompts"].is_object());
        assert!(out["instructions"]
            .as_str()
            .unwrap()
            .contains("oxidean_pat_"));
    }

    // -- tool surface invariants ---------------------------------------------

    #[test]
    fn tool_defs_are_well_formed() {
        let defs = tool_defs();
        assert!(defs.len() >= 16, "tool registry shrank: {}", defs.len());
        let mut names = std::collections::HashSet::new();
        for t in &defs {
            assert!(names.insert(t.name), "duplicate tool {}", t.name);
            assert!(!t.description.is_empty(), "{} missing description", t.name);
            assert_eq!(t.input_schema["type"], "object", "{} schema", t.name);
            let props = t.input_schema["properties"].as_object();
            let required: Vec<&str> = t
                .input_schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            if let Some(props) = props {
                for r in required {
                    assert!(props.contains_key(r), "{} requires unknown {r}", t.name);
                }
            } else {
                assert!(required.is_empty(), "{} has required but no props", t.name);
            }
        }
        for expected in [
            "repo_list",
            "repo_get",
            "repo_tree",
            "issue_list",
            "issue_get",
            "issue_create",
            "issue_comment",
            "issue_list_comments",
            "pull_list",
            "pull_get",
            "actions_list_runs",
            "actions_get_run",
            "packages_list",
            "search_repos",
            "search_issues",
            "search_pulls",
            "search_code",
        ] {
            assert!(names.contains(expected), "missing tool {expected}");
        }
    }

    // -- tool → RPC input mapping --------------------------------------------

    #[test]
    fn build_rpc_input_repo_list() {
        let defs = tool_defs();
        let repo_list = defs.iter().find(|t| t.name == "repo_list").unwrap();
        let (proc, input) = build_rpc_input(repo_list, &json!({})).unwrap();
        assert_eq!(proc, "repo.listMine");
        assert_eq!(input, json!({}));
        for args in [json!({"owner": "octo"}), json!({"owner": "  octo  "})] {
            let (proc, input) = build_rpc_input(repo_list, &args).unwrap();
            assert_eq!(proc, "repo.listByOwner");
            assert_eq!(input["owner"], "octo");
        }
        // Blank owner falls back to listMine.
        let (proc, _) = build_rpc_input(repo_list, &json!({"owner": "   "})).unwrap();
        assert_eq!(proc, "repo.listMine");
    }

    #[test]
    fn build_rpc_input_search_kinds() {
        let defs = tool_defs();
        let cases = [
            ("search_issues", "issues"),
            ("search_pulls", "pulls"),
            ("search_code", "code"),
        ];
        for (name, kind) in cases {
            let tool = defs.iter().find(|t| t.name == name).unwrap();
            let (proc, input) = build_rpc_input(
                tool,
                &json!({"owner": "o", "name": "n", "q": "x", "limit": 5}),
            )
            .unwrap();
            assert_eq!(proc, "repo.search", "{name}");
            assert_eq!(input["type"], kind, "{name}");
            assert_eq!(input["q"], "x");
        }
    }

    #[test]
    fn build_rpc_input_passthrough() {
        let defs = tool_defs();
        let tool = defs.iter().find(|t| t.name == "issue_get").unwrap();
        let args = json!({"owner": "o", "name": "n", "number": 3});
        let (proc, input) = build_rpc_input(tool, &args).unwrap();
        assert_eq!(proc, "issue.get");
        assert_eq!(input, args);
    }

    #[test]
    fn build_rpc_input_repo_tree_ref_default() {
        let defs = tool_defs();
        let tool = defs.iter().find(|t| t.name == "repo_tree").unwrap();
        // Omitted → "" so the required `repo.tree` field falls back to the
        // default branch; an explicit ref passes through untouched.
        let (_, input) = build_rpc_input(tool, &json!({"owner": "o", "name": "n"})).unwrap();
        assert_eq!(input["ref"], "");
        let (_, input) = build_rpc_input(
            tool,
            &json!({"owner": "o", "name": "n", "ref": "dev", "path": "src"}),
        )
        .unwrap();
        assert_eq!(input["ref"], "dev");
        assert_eq!(input["path"], "src");
    }

    // -- tool result envelopes -------------------------------------------------

    #[test]
    fn tool_ok_envelope() {
        let out = tool_ok(json!({"name": "demo", "n": 1}));
        assert_eq!(out["isError"], false);
        assert_eq!(out["structuredContent"]["name"], "demo");
        let text = out["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"demo\""), "{text}");
        // Non-object payload: text content only, no structuredContent.
        let out = tool_ok(json!(["a", "b"]));
        assert_eq!(out["isError"], false);
        assert!(out.get("structuredContent").is_none());
        assert_eq!(out["content"][0]["text"], "[\"a\",\"b\"]");
    }

    #[test]
    fn tool_error_envelope() {
        let e = AppError::new("repo.not_found", "repository not found");
        let out = tool_error(&e);
        assert_eq!(out["isError"], true);
        let text = out["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("repo.not_found"), "{text}");
        assert!(text.contains("repository not found"), "{text}");
    }

    // -- resource templates ----------------------------------------------------

    #[test]
    fn resource_templates_shape() {
        let tpls = resource_templates();
        let arr = tpls.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        for t in arr {
            assert!(t["uriTemplate"]
                .as_str()
                .unwrap()
                .starts_with("oxidean://repo/"));
            assert!(t["name"].is_string());
            assert!(t["mimeType"].is_string());
        }
    }

    // -- scope helpers -----------------------------------------------------------

    #[test]
    fn pat_helpers() {
        let pat_row = |scopes_json: Option<&str>| PatRow {
            id: "p1".into(),
            user_id: "u1".into(),
            kind: PatKind::Classic.as_str().into(),
            name: "t".into(),
            token_prefix: String::new(),
            token_hash: String::new(),
            scopes_json: scopes_json.map(str::to_string),
            contents_perm: None,
            perms_json: None,
            repo_access: None,
            expires_at: None,
            revoked_at: None,
            last_used_at: None,
            last_used_ip: None,
            created_at: String::new(),
            repository_ids: vec![],
        };
        assert!(pat_has_package_read(&pat_row(Some(
            r#"["repo","package:read"]"#
        ))));
        assert!(
            pat_has_package_read(&pat_row(Some(r#"["package:write"]"#))),
            "write implies read"
        );
        assert!(!pat_has_package_read(&pat_row(Some(r#"["repo"]"#))));
        assert!(
            pat_scopes(&pat_row(Some("not json"))).is_empty(),
            "bad scopes_json → empty"
        );
        let e = scope_denied("repo");
        assert_eq!(e.code, "mcp.insufficient_scope");
        assert!(e.message.contains("repo"));
    }
}
