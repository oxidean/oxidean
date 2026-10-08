//! oxidean-web — the production web tier: serves the statically-built Octane
//! SPA shells and assets, applies per-request middleware (theme class, route
//! <title>, SSH advertise metas, anonymous→login gate, header-gated /health,
//! well-known discovery docs), and reverse-proxies API prefixes when
//! OXIDEAN_API_ORIGIN is set (dev parity — Compose Traefik splits them
//! upstream instead).

mod inject;
mod proxy;
mod session;
mod shells;
mod wellknown;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tracing_subscriber::EnvFilter;

/// Compose/Dockerfile probe parity with `vite-plugins/web-health.ts`:
/// `GET /health` + `Oxidean-Health-Probe: 1` → 200 JSON; anything else → 404
/// so the endpoint is not a free liveness oracle on the SPA origin.
const HEALTH_PATH: &str = "/health";
const HEALTH_PROBE_HEADER: &str = "oxidean-health-probe";
const HEALTH_PROBE_VALUE: &str = "1";

struct Shared {
    dist: PathBuf,
    meta: inject::AdvertiseMeta,
    proxy: Option<proxy::ApiProxy>,
    sessions: Option<session::SessionValidator>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let dist = std::env::var("OXIDEAN_WEB_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("dist"));
    let api_origin = std::env::var("OXIDEAN_API_ORIGIN")
        .ok()
        .or_else(|| std::env::var("OXIDEAN_E2E_API_ORIGIN").ok())
        .map(|s| s.trim_end_matches('/').to_string());

    if api_origin.is_none() {
        tracing::warn!(
            "OXIDEAN_API_ORIGIN unset — session stamp/gate fall back to cookie presence"
        );
    }
    let state = Arc::new(Shared {
        meta: inject::AdvertiseMeta {
            ssh_host: std::env::var("OXIDEAN_SSH_HOST").ok().filter(|s| !s.is_empty()),
            ssh_port: std::env::var("OXIDEAN_SSH_PORT").ok().and_then(|p| p.parse().ok()),
        },
        proxy: api_origin.as_deref().map(|o| {
            tracing::info!("proxying API prefixes to {o}");
            proxy::ApiProxy::new(o)
        }),
        sessions: api_origin
            .as_deref()
            .map(session::SessionValidator::new),
        dist: {
            let canon = dist.canonicalize().unwrap_or(dist);
            tracing::info!("serving SPA shells from {}", canon.display());
            canon
        },
    });

    let app = Router::new()
        .route(HEALTH_PATH, get(health))
        .route(wellknown::WEBMCP_PATH, get(wellknown::webmcp))
        .route(wellknown::MCP_PATH, get(wellknown::mcp))
        .fallback(spa)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind web listener");
    tracing::info!("oxidean-web listening on {addr}");
    axum::serve(listener, app).await.expect("serve web");
}

async fn health(headers: HeaderMap) -> Response {
    let ok = headers
        .get(HEALTH_PROBE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == HEALTH_PROBE_VALUE)
        .unwrap_or(false);
    if !ok {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        "{\"ok\":true}",
    )
        .into_response()
}

/// Fallback handler: API prefixes → proxy; routed paths → shell dispatch
/// (which carries the auth gate); everything else → static files, then 404.
async fn spa(State(st): State<Arc<Shared>>, headers: HeaderMap, uri: Uri, req: axum::extract::Request) -> Response {
    let path = uri.path();

    // API-owned prefixes (dev parity with the vite proxy / non-Traefik hosts).
    if let Some(px) = &st.proxy {
        if proxy::is_api_prefix(path) {
            return px.forward(req).await;
        }
    }

    // A dotted final segment on an asset-overlapping pattern (`/{owner}`,
    // `/{owner}/{repo}`) may be a real file under dist/ — prefer it, else 404
    // so missing chunks never return 200 HTML. Literal-prefix routes own
    // their dotted params: `/o/r/releases/v1.0`, `/o/r/blob/main/README.md`.
    let looks_like_file = path.rsplit('/').next().is_some_and(|s| s.contains('.'));

    // Shell dispatch FIRST — protected routes must not be reachable through
    // the raw static path (the shells exist as files under dist/).
    if let Some(m) = shells::dispatch(path) {
        if looks_like_file && m.asset_overlap {
            // e.g. /{owner}/{repo} matched "/brand/logo.png" — serve the real
            // file when present, else 404 (never a 200 HTML for a missing
            // chunk, which would poison caches and hide module failures).
            if let Some(resp) = try_static(&st.dist, path).await {
                return resp;
            }
            return not_found(&st.dist).await;
        }
        // Resolve the session signal once for both the protected gate and the
        // `data-oxidean-session` stamp. Validated upstream when a validator is
        // configured; Unknown degrades to the presence heuristic.
        let signal = resolve_signal(&st, cookie_header(&headers)).await;
        let passes_gate = match signal {
            session::SessionSignal::Valid => true,
            session::SessionSignal::Absent | session::SessionSignal::Invalid => false,
            session::SessionSignal::Unknown => inject::signed_in(cookie_header(&headers)),
        };
        if m.protected && !passes_gate {
            let target = format!("/login?returnTo={}", urlencoding(path));
            return (
                StatusCode::FOUND,
                [(header::LOCATION, target)],
            )
                .into_response();
        }
        match tokio::fs::read_to_string(st.dist.join(m.file)).await {
            Ok(html) => {
                let out = inject::inject(&html, &headers, m.title.as_deref(), &st.meta, signal);
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                        (header::CACHE_CONTROL, "no-cache"),
                    ],
                    out,
                )
                    .into_response();
            }
            Err(e) => {
                tracing::warn!("shell {} unreadable: {e}", m.file);
                return not_found(&st.dist).await;
            }
        }
    }

    // Non-routed path — asset or 404.
    if let Some(resp) = try_static(&st.dist, path).await {
        return resp;
    }
    not_found(&st.dist).await
}

fn cookie_header(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
}

/// Map the request's cookies to a session signal: `Absent` without an
/// `oxidean_session` cookie (a bare `oxidean_signed_in` hint is not a
/// credential), upstream verdict when the validator is configured, `Unknown`
/// otherwise — the failure mode that keeps presence-fallback behavior alive.
async fn resolve_signal(st: &Shared, cookie: Option<&str>) -> session::SessionSignal {
    let Some(c) = cookie else {
        return session::SessionSignal::Absent;
    };
    if inject::find_cookie(c, "oxidean_session").is_none_or(|v| v.is_empty()) {
        return session::SessionSignal::Absent;
    }
    match &st.sessions {
        Some(v) => v.check(c).await,
        None => session::SessionSignal::Unknown,
    }
}

/// Try `dist{path}` and `dist{path}/index.html`; rejects traversal.
async fn try_static(dist: &PathBuf, path: &str) -> Option<Response> {
    // Only serve GET-shaped asset paths; shell names never contain these.
    let rel = path.trim_start_matches('/');
    if rel.is_empty() || rel.contains("..") {
        return None;
    }
    for cand in [
        dist.join(rel),
        dist.join(rel).join("index.html"),
        dist.join(rel.trim_end_matches('/')).join("index.html"),
    ] {
        if cand.is_file() {
            // Shell HTML under dist is dispatch-owned — don't serve it raw at
            // asset positions (it would skip injection). `_`-dir pages are the
            // placeholder shells; still serve them directly so previews work.
            let mime = mime_guess::from_path(&cand).first_or_octet_stream();
            let cache = if rel.starts_with("_astro/") || rel.starts_with("fonts/") {
                "public, max-age=31536000, immutable"
            } else if rel.ends_with(".html") {
                "no-cache"
            } else {
                "public, max-age=3600"
            };
            let body = tokio::fs::read(&cand).await.ok()?;
            return Some(
                (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, mime.as_ref().to_string()),
                        (header::CACHE_CONTROL, cache.to_string()),
                    ],
                    Body::from(body),
                )
                    .into_response(),
            );
        }
    }
    None
}

async fn not_found(dist: &PathBuf) -> Response {
    if let Ok(body) = tokio::fs::read(dist.join("404.html")).await {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            body,
        )
            .into_response();
    }
    (StatusCode::NOT_FOUND, "Not Found").into_response()
}

/// `returnTo=` needs the path URL-encoded (`/` stays — readable and matches
/// the old `?returnTo=/settings` shape).
fn urlencoding(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
