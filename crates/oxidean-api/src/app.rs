use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::DefaultBodyLimit;
use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::{from_fn, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use oxidean_core::{AppError, RpcRequest, RpcResponse};
use oxidean_db::Database;
use std::net::{IpAddr, SocketAddr};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

// Facade: AppState, credential resolution, and the RpcCtx builders moved to
// `oxidean-api-core` (issue #162). Re-export so `crate::app::*` call sites keep
// resolving until the domain crates land.
pub use oxidean_api_core::app::*;

use crate::routes::{
    action_badge, auth_callbacks, avatar, cli_dist, feeds, git_lfs, git_smart_http, pages_serve,
    release_assets, repo_raw, template_packs,
};
use crate::rpc::{self, VERSION_HEADER};

/// Build router with default email sender from ENV and `OXIDEAN_ENV`.
pub fn router(db: Database, cors: CorsLayer) -> Router {
    let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
    let email = crate::email::build_email_sender_from_env();
    router_with_state(AppState::new(db, email, env_name), cors)
}

pub fn router_with_state(mut state: AppState, cors: CorsLayer) -> Router {
    // Every transport resolves procedures through the same assembled
    // dispatch — REST handlers call `ctx.rpc_dispatch` (issue #162).
    state.rpc_dispatch = std::sync::Arc::new(|ctx, req| Box::pin(crate::rpc::dispatch(ctx, req)));
    let asset_body_limit = state.release_asset_max_bytes.max(1024);
    Router::new()
        .route("/health", get(health))
        .route("/api/rpc", post(rpc_http))
        .route("/api/rpc/ws", get(rpc_ws))
        // REST facade over the RPC domain (API-01). `/api/v1/` (trailing
        // slash) falls outside the nest — mount the GitHub root-meta handler
        // explicitly, the same paired-spelling pattern as `/v2` below.
        .nest("/api/v1", crate::rest::router())
        .route("/api/v1/", get(crate::rest::github::meta_root))
        .route(
            "/api/mcp",
            post(crate::mcp::handle_post).get(crate::mcp::handle_get),
        )
        .nest("/api/actions", crate::actions::runner_proto::router())
        // actions/cache@v4 Twirp surface — `CacheServiceClientJSON` joins
        // `new URL("/twirp/<svc>/<m>", ACTIONS_RESULTS_URL)`, so the service
        // lives at the origin root regardless of the env var's path. Bearer
        // is the same per-job runtime token as the v1 surface.
        .nest(
            "/twirp",
            crate::actions::cache_v2::twirp_router()
                .merge(crate::actions::artifacts_v2::twirp_router()),
        )
        // OIDC issuer for `id-token: write` jobs — the request token itself
        // is the credential, so these paths carry no session middleware.
        .nest("/api/actions/oidc", crate::actions::oidc::router())
        // Oxpilot SSE chat + C3 connector subtree — session-cookie auth on
        // `sessions/*`, verifier-Bearer on `connector/*`; enablement fails
        // closed inside each handler.
        .nest("/api/oxpilot", oxidean_api_oxpilot::routes::router())
        .route("/api/auth/workos/start", get(auth_callbacks::workos_start))
        .route(
            "/api/auth/workos/callback",
            get(auth_callbacks::workos_callback),
        )
        .route("/api/auth/oidc/start", get(auth_callbacks::oidc_start))
        .route(
            "/api/auth/oidc/callback",
            get(auth_callbacks::oidc_callback),
        )
        // Web-tier session probe (`oxidean-web` stamps `data-oxidean-session`
        // and gates protected shells on the result): read-only, one SELECT,
        // no session touch on the document path.
        .route("/api/auth/session-check", get(session_check))
        // Web-tier setup probe (`oxidean-web` stamps `data-oxidean-setup`):
        // read-only COUNT, deployment-global so no cookie is needed — the SPA
        // drops its per-page auth.bootstrapStatus fetch in favor of the stamp.
        .route("/api/auth/setup-check", get(setup_check))
        // OAuth2 provider surface (API-03). The consent screen itself is the
        // SPA route /oauth/consent; these three paths are API-owned and must be
        // routed to the API at the edge (Caddyfile / Traefik / vite proxy).
        .route("/oauth/authorize", get(crate::oauth::authorize))
        .route("/oauth/token", post(crate::oauth::token))
        .route("/oauth/userinfo", get(crate::oauth::userinfo))
        .route("/oauth/device/code", post(crate::oauth::device_code))
        // RFC 8414 + RFC 9728 discovery — OAuth/MCP clients resolve these via
        // the `resource_metadata` pointer on /api/mcp 401s. Edge proxies must
        // forward `/.well-known/oauth*` alongside the OAuth routes.
        .route(
            "/.well-known/oauth-authorization-server",
            get(crate::oauth::authorization_server_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(crate::oauth::protected_resource_metadata),
        )
        .route(
            "/api/user/avatar",
            post(avatar::upload_avatar)
                .layer(DefaultBodyLimit::max(avatar::AVATAR_MAX_BYTES))
                .delete(avatar::delete_avatar),
        )
        .route("/uploads/avatars/{file}", get(avatar::serve_avatar))
        .route(
            "/api/repos/{owner}/{repo}/releases/{release_id}/assets",
            post(release_assets::upload_asset).layer(DefaultBodyLimit::max(asset_body_limit)),
        )
        .route(
            "/api/releases/assets/{asset_id}",
            get(release_assets::download_asset),
        )
        // Browser-facing Actions downloads — session/PAT-authed, unlike the
        // runner-token /api/actions/* surface.
        .route(
            "/api/repos/{owner}/{repo}/actions/artifacts/{artifact_id}",
            get(oxidean_api_rest::action_downloads::download_artifact),
        )
        .route(
            "/api/repos/{owner}/{repo}/actions/jobs/{job_id}/logs",
            get(oxidean_api_rest::action_downloads::download_job_log),
        )
        .route(
            "/api/repos/{owner}/{repo}/actions/jobs/{job_id}/logs/stream",
            get(oxidean_api_rest::action_downloads::stream_job_log),
        )
        .route(
            "/api/repos/{owner}/{repo}/actions/runs/{run_id}/logs",
            get(oxidean_api_rest::action_downloads::download_run_logs),
        )
        .route(
            "/api/admin/templates",
            post(template_packs::upload_pack).layer(DefaultBodyLimit::max(
                crate::templates::store::DEFAULT_MAX_PACK_BYTES as usize,
            )),
        )
        .route(
            "/api/repos/{owner}/{repo}/raw/{ref}/{*path}",
            get(repo_raw::serve_raw),
        )
        .route(
            "/api/repos/{owner}/{repo}/archive/{*archive_file}",
            get(repo_raw::serve_archive),
        )
        .route(
            "/api/repos/{owner}/{repo}/mirror/hook",
            axum::routing::post(crate::mirror::mirror_hook),
        )
        // Atom feeds (API-05) — /api prefix keeps them on this service at the edge.
        .route(
            "/api/repos/{owner}/{repo}/activity.atom",
            get(feeds::repo_activity_feed),
        )
        .route(
            "/api/repos/{owner}/{repo}/releases.atom",
            get(feeds::repo_releases_feed),
        )
        .route(
            "/api/users/{username}/activity.atom",
            get(feeds::user_activity_feed),
        )
        // GitHub-shaped workflow status badge — /{owner}/{repo}/actions/
        // workflows/{file}/badge.svg (web tier proxies this path here); also
        // mounted under /api/repos for direct API consumers.
        .route(
            "/{owner}/{repo}/actions/workflows/{workflow}/badge.svg",
            get(action_badge::workflow_badge),
        )
        .route(
            "/api/repos/{owner}/{repo}/actions/workflows/{workflow}/badge.svg",
            get(action_badge::workflow_badge),
        )
        // Pages sites — /pages/{owner}/{repo}/{*path} serves the materialized
        // tree (edge routes the reserved `/pages` prefix here; the web tier
        // proxies it too). Same-origin user HTML is sandboxed via CSP inside
        // the handler.
        .route("/pages/{owner}/{repo}", get(pages_serve::redirect_root))
        .route("/pages/{owner}/{repo}/", get(pages_serve::serve_index))
        .route("/pages/{owner}/{repo}/{*path}", get(pages_serve::serve))
        // Smart HTTP — D-18/D-22: only on /{owner}/{repo}.git (segment includes .git suffix)
        .route(
            "/{owner}/{repo_git}/info/refs",
            get(git_smart_http::info_refs),
        )
        .route(
            "/{owner}/{repo_git}/git-upload-pack",
            axum::routing::post(git_smart_http::upload_pack),
        )
        .route(
            "/{owner}/{repo_git}/git-receive-pack",
            axum::routing::post(git_smart_http::receive_pack),
        )
        // Git LFS — Batch + basic transfer (D-LFS-06 / D-LFS-07)
        .route(
            "/{owner}/{repo_git}/info/lfs/objects/batch",
            axum::routing::post(git_lfs::batch),
        )
        .route(
            "/{owner}/{repo_git}/info/lfs/objects/verify",
            axum::routing::post(git_lfs::verify_object),
        )
        .route(
            "/{owner}/{repo_git}/info/lfs/objects/{oid}",
            axum::routing::get(git_lfs::get_object)
                .put(git_lfs::put_object)
                .layer(DefaultBodyLimit::max(2 * 1024 * 1024 * 1024)),
        )
        // Agentic VCS object transport (contract C2) — Bearer/PAT only, never
        // the session cookie. The 512 MiB body limit sits above the 256 MiB
        // per-object cap so the typed `vcs.quota_exceeded` answers before
        // Axum's untyped 413.
        .route(
            "/api/vcs/{owner}/{name}/objects/{hash}",
            get(crate::vcs_routes::object_get),
        )
        .route(
            "/api/vcs/{owner}/{name}/objects",
            axum::routing::post(crate::vcs_routes::object_put)
                .layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        // CLI distribution — install script + ox binaries; must route to the
        // API at the edge (unauthenticated; ox self-update consumes it).
        .route("/cli/install.sh", get(cli_dist::install_script))
        .route("/cli/install.ps1", get(cli_dist::install_ps1))
        .route("/cli/skill.md", get(cli_dist::skill_md))
        .route("/cli/latest", get(cli_dist::latest))
        .route("/cli/bin/{*file}", get(cli_dist::binary))
        // Package registry (D-PKG-01) — path prefixes must outrank SPA at the edge.
        .route("/v2", get(crate::packages::oci::discovery))
        .route("/v2/", get(crate::packages::oci::discovery))
        .nest("/v2", crate::packages::oci::router())
        .nest("/npm", crate::packages::npm::router())
        .nest("/generic", crate::packages::generic::router())
        // Branded status pages (shared crate with the web tier): unmatched
        // paths → themed 404, method mismatches → themed 405 — but only for
        // `Accept: text/html`; API/CLI clients keep the JSON error envelope.
        .fallback(fallback_not_found)
        .method_not_allowed_fallback(fallback_method_not_allowed)
        // Bare/plain-text route errors (avatars, cli assets, raw files) get
        // the same themed page for browsers — structured bodies untouched.
        .layer(from_fn(oxidean_status_pages::theme_errors))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        // Outermost: stamp ConnectInfo into an internal header so every
        // `ClientMeta::from_headers` path (RPC, REST, packages, …) sees the
        // peer without per-handler extractors. Client-supplied values are
        // stripped first so the header is never attacker-controlled.
        .layer(from_fn(stamp_connect_ip))
        .with_state(state)
}

/// Strip any inbound `x-oxidean-connect-ip`, then re-set it from
/// `ConnectInfo` when the server was started with
/// `into_make_service_with_connect_info`.
async fn stamp_connect_ip(mut req: Request<axum::body::Body>, next: Next) -> Response {
    req.headers_mut()
        .remove(oxidean_api_core::rpc::CONNECT_IP_HEADER);
    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        if let Ok(v) = HeaderValue::from_str(&addr.ip().to_string()) {
            req.headers_mut()
                .insert(oxidean_api_core::rpc::CONNECT_IP_HEADER, v);
        }
    }
    next.run(req).await
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

/// `GET /api/auth/session-check` — reports whether the presented
/// `oxidean_session` cookie maps to a live session. Store failures surface as
/// 500 (never `valid:false`) so the web tier can tell "definitively signed
/// out" from "couldn't ask" and keep its presence fallback on errors.
async fn session_check(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    // no-store — a cached probe result would pin a stale validity verdict.
    let no_store = [(header::CACHE_CONTROL, "no-store")];
    let Some(token) = session_token_from_headers(&headers) else {
        return (no_store, Json(serde_json::json!({ "valid": false }))).into_response();
    };
    match state.sessions.peek(&state.db, &token).await {
        Ok(valid) => (no_store, Json(serde_json::json!({ "valid": valid }))).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "session-check probe failed");
            (no_store, StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

/// `GET /api/auth/setup-check` — reports whether the instance still needs
/// first-run setup. Store failures surface as 500 so the web tier stamps
/// nothing (fail-open) rather than pinning a dead-API instance into /setup.
async fn setup_check(State(state): State<AppState>) -> impl IntoResponse {
    let no_store = [(header::CACHE_CONTROL, "no-store")];
    match crate::auth::bootstrap::needs_setup(&state.db).await {
        Ok(needs_setup) => (
            no_store,
            Json(serde_json::json!({ "needs_setup": needs_setup })),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(error = ?e, "setup-check probe failed");
            (no_store, StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

/// Router fallback — no route matched the path. Browsers (`Accept:
/// text/html`) get the shared branded page; anything else gets the RPC
/// error envelope a mistyped API path deserves.
async fn fallback_not_found(headers: HeaderMap) -> Response {
    status_response(
        StatusCode::NOT_FOUND,
        "route.not_found",
        "route not found",
        &headers,
    )
}

/// Method mismatch on a matched path (axum re-stamps `Allow` on top of our
/// response) — same negotiation as the 404 fallback.
async fn fallback_method_not_allowed(headers: HeaderMap) -> Response {
    status_response(
        StatusCode::METHOD_NOT_ALLOWED,
        "route.method_not_allowed",
        "method not allowed",
        &headers,
    )
}

/// Content-negotiated status response: shared themed page for document
/// requests, JSON error envelope otherwise.
fn status_response(
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    headers: &HeaderMap,
) -> Response {
    let accept = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok());
    if oxidean_status_pages::accepts_html(accept) {
        // Same cookie/CH chain the web tier resolves — `/api/*` rides the
        // browser origin, so the theme cookies are already on the request.
        let dark = oxidean_status_pages::color_scheme(
            headers.get(header::COOKIE).and_then(|v| v.to_str().ok()),
            headers
                .get("sec-ch-prefers-color-scheme")
                .and_then(|v| v.to_str().ok()),
        );
        return (
            status,
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            oxidean_status_pages::render(oxidean_status_pages::theme(), status, dark),
        )
            .into_response();
    }
    (
        status,
        Json(serde_json::json!({
            "ok": false,
            "error": { "code": code, "message": message }
        })),
    )
        .into_response()
}

/// Peer IP when the server was started with
/// `into_make_service_with_connect_info`; `None` under `Router::oneshot`
/// tests that do not inject `ConnectInfo`.
struct PeerIp(Option<IpAddr>);

impl<S> FromRequestParts<S> for PeerIp
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(PeerIp(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|c| c.0.ip()),
        ))
    }
}

async fn rpc_http(
    State(state): State<AppState>,
    PeerIp(peer): PeerIp,
    headers: HeaderMap,
    Json(body): Json<RpcRequest>,
) -> impl IntoResponse {
    let version = headers.get(VERSION_HEADER).and_then(|v| v.to_str().ok());
    if let Err(err) = rpc::check_version_header(version) {
        return (StatusCode::BAD_REQUEST, Json(RpcResponse::err(err))).into_response();
    }

    let credential = edge_credential(&headers);
    let bearer_auth = matches!(credential, RpcCredential::Bearer(_));
    let challenge_id = two_factor_id_from_headers(&headers);
    let mut ctx = match build_rpc_ctx(
        &state,
        credential,
        rpc::ClientMeta::from_headers_and_peer(&headers, peer),
        challenge_id,
    )
    .await
    {
        Ok(ctx) => ctx,
        Err(rejection) => return bearer_rejection_response(rejection),
    };
    let resp = rpc::dispatch(&mut ctx, body).await;
    let status = rpc_status(&resp);
    // PAT Bearer calls never mint or mutate cookies (API-02).
    let set_cookie = if bearer_auth {
        None
    } else {
        ctx.set_cookie.take()
    };
    let env_name = ctx.env_name.clone();
    let response = (status, Json(resp)).into_response();
    attach_set_cookie(response, set_cookie, &env_name)
}

async fn rpc_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    PeerIp(peer): PeerIp,
    headers: HeaderMap,
) -> impl IntoResponse {
    let version = headers.get(VERSION_HEADER).and_then(|v| v.to_str().ok());
    if let Err(err) = rpc::check_version_header(version) {
        return (StatusCode::BAD_REQUEST, Json(RpcResponse::err(err))).into_response();
    }
    let credential = edge_credential(&headers);
    let client = rpc::ClientMeta::from_headers_and_peer(&headers, peer);
    let challenge_id = two_factor_id_from_headers(&headers);
    ws.on_upgrade(move |socket| handle_socket(socket, state, credential, client, challenge_id))
}

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    credential: RpcCredential,
    client: rpc::ClientMeta,
    challenge_id: Option<String>,
) {
    let (mut sender, mut receiver) = socket.split();
    while let Some(Ok(msg)) = receiver.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };
        let resp = match serde_json::from_str::<RpcRequest>(&text) {
            Ok(req) => {
                // Bearer tokens re-resolve per frame, matching session-cookie
                // semantics (revocation/expiry takes effect on the next frame).
                match build_rpc_ctx(
                    &state,
                    credential.clone(),
                    client.clone(),
                    challenge_id.clone(),
                )
                .await
                {
                    Ok(mut ctx) => rpc::dispatch(&mut ctx, req).await,
                    Err(rejection) => RpcResponse::err(rejection.error),
                }
            }
            Err(e) => RpcResponse::err(AppError::new(
                "rpc.bad_input",
                format!("invalid rpc frame: {e}"),
            )),
        };
        let payload = serde_json::to_string(&resp).unwrap_or_else(|_| {
            r#"{"ok":false,"error":{"code":"rpc.internal","message":"serialize failed"}}"#.into()
        });
        if sender.send(Message::Text(payload.into())).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fallback_negotiates_branded_page_or_json() {
        let mut html = HeaderMap::new();
        html.insert(header::ACCEPT, HeaderValue::from_static("text/html"));
        let resp = fallback_not_found(html).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains("Page not found"));

        let resp = fallback_not_found(HeaderMap::new()).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "route.not_found");
    }
}
