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
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;
use tower_http::compression::predicate;
use tower_http::compression::{CompressionLayer, Predicate};
use tower_http::services::ServeFile;
use tracing_subscriber::EnvFilter;

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
    // `OXIDEAN_WEB_BEHIND_PROXY=1` declares a sanitizing edge (Traefik,
    // Railway, nginx) in front whose X-Forwarded-* values the API proxy may
    // forward verbatim. Unset means this tier IS the edge, so the proxy
    // strips client-supplied forwarding headers and re-sets them from
    // observed truth (peer IP, Host) instead.
    let behind_proxy = std::env::var("OXIDEAN_WEB_BEHIND_PROXY")
        .ok()
        .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"));

    if api_origin.is_none() {
        tracing::warn!(
            "OXIDEAN_API_ORIGIN unset — session stamp/gate fall back to cookie presence"
        );
    }
    let state = Arc::new(Shared {
        meta: inject::AdvertiseMeta {
            ssh_host: std::env::var("OXIDEAN_SSH_HOST")
                .ok()
                .filter(|s| !s.is_empty()),
            ssh_port: std::env::var("OXIDEAN_SSH_PORT")
                .ok()
                .and_then(|p| p.parse().ok()),
        },
        proxy: api_origin.as_deref().map(|o| {
            tracing::info!("proxying API prefixes to {o} (behind_proxy={behind_proxy})");
            proxy::ApiProxy::new(o, behind_proxy)
        }),
        sessions: api_origin.as_deref().map(session::SessionValidator::new),
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
        // Compress only worth-it bodies: >64 bytes and not SSE (streamed
        // JSON-RPC over /api/mcp must not be buffered), not an archive/image
        // payload, and not already-encoded upstream. Upgrade responses (101)
        // fail the size check so websocket handshakes pass through untouched.
        .layer(CompressionLayer::new().compress_when(
            predicate::SizeAbove::new(64)
                .and(predicate::NotForContentType::new("text/event-stream"))
                .and(predicate::NotForContentType::new("application/grpc"))
                .and(predicate::NotForContentType::new("application/octet-stream"))
                .and(predicate::NotForContentType::new("application/zip"))
                .and(predicate::NotForContentType::new("image/")),
        ))
        // Log method + path only: full URIs would put verify/reset tokens
        // and OAuth params from query strings into the logs.
        .layer(tower_http::trace::TraceLayer::new_for_http().make_span_with(
            |req: &axum::extract::Request| {
                tracing::info_span!("request", method = %req.method(), path = %req.uri().path())
            },
        ))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind web listener");
    tracing::info!("oxidean-web listening on {addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("serve web");
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
async fn spa(
    State(st): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
    req: axum::extract::Request,
) -> Response {
    let path = uri.path();

    // Dot segments (literal or %-encoded) normalize before reaching the
    // upstream URL parser or the filesystem join — `/api/../x` would escape
    // the API prefix to an arbitrary API path, `/static/../../etc/passwd`
    // escapes dist/. Reject outright.
    if has_dot_segments(path) {
        return not_found(&st.dist, &headers, &st.meta).await;
    }

    // API-owned prefixes (dev parity / single-origin deploys without Traefik).
    if let Some(px) = &st.proxy {
        if proxy::is_api_prefix(path) {
            return px.forward(req, peer).await;
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
            if let Some(resp) = try_static(&st.dist, path, req).await {
                return resp;
            }
            return not_found(&st.dist, &headers, &st.meta).await;
        }
        // Shells are documents: GET/HEAD only. POST to a route path is almost
        // always a stray form/action or a confused crawler — 405 it rather
        // than serve 200 HTML that pretends the mutation worked.
        if req.method() != Method::GET && req.method() != Method::HEAD {
            return (
                StatusCode::METHOD_NOT_ALLOWED,
                [(header::ALLOW, "GET, HEAD")],
            )
                .into_response();
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
            return (StatusCode::FOUND, [(header::LOCATION, target)]).into_response();
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
                return not_found(&st.dist, &headers, &st.meta).await;
            }
        }
    }

    // Non-routed path — asset or 404.
    if let Some(resp) = try_static(&st.dist, path, req).await {
        return resp;
    }
    not_found(&st.dist, &headers, &st.meta).await
}

fn cookie_header(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::COOKIE).and_then(|v| v.to_str().ok())
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

/// Does `path` contain a `..` segment after folding `%2e`/`%2E` to `.`?
/// Both consumers that treat the path structurally — reqwest's URL parser
/// (WHATWG normalization) and `Path::join` — resolve dot segments, so the
/// check must run on the decoded form, not just the raw bytes.
fn has_dot_segments(path: &str) -> bool {
    path.split('/').any(|seg| {
        // `..` encoded any way still folds back to `..` — other %-escapes
        // can't produce a dot byte, so folding `%2e` alone is complete.
        let decoded = seg.replace("%2e", ".").replace("%2E", ".");
        decoded == ".."
    })
}

/// Try `dist{path}` for literal asset files only; rejects traversal and all
/// `.html` — shell HTML under dist is dispatch-owned (gate + per-request
/// injection), so serving it raw at asset positions would bypass both.
///
/// Serves through `ServeFile` so Range, If-Modified-Since/If-Unmodified-Since,
/// and HEAD work correctly and large assets stream rather than buffering.
async fn try_static(
    dist: &std::path::Path,
    path: &str,
    req: axum::extract::Request,
) -> Option<Response> {
    let rel = path.trim_start_matches('/');
    if rel.is_empty() || rel.ends_with(".html") {
        return None;
    }
    let cand = dist.join(rel);
    if !cand.is_file() {
        return None;
    }
    let cache = if rel.starts_with("_astro/") || rel.starts_with("fonts/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    };
    // ServeFile honors Range/conditional headers carried in `req` and returns
    // 405 for non-GET/HEAD methods on its own.
    let mut resp = ServeFile::new(cand).oneshot(req).await.ok()?;
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    Some(resp.map(Body::new))
}

/// 404 responses inject theme + advertise metas too (dark-theme users
/// shouldn't get a light flash on a missing page) but never the session
/// stamp — a missing route must not imply a signed-in browser.
async fn not_found(
    dist: &std::path::Path,
    headers: &HeaderMap,
    meta: &inject::AdvertiseMeta,
) -> Response {
    if let Ok(html) = tokio::fs::read_to_string(dist.join("404.html")).await {
        let out = inject::inject(&html, headers, None, meta, session::SessionSignal::Absent);
        return (
            StatusCode::NOT_FOUND,
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            out,
        )
            .into_response();
    }
    (
        StatusCode::NOT_FOUND,
        [(header::CACHE_CONTROL, "no-store")],
        "Not Found",
    )
        .into_response()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_segments_detected_raw_and_encoded() {
        assert!(has_dot_segments("/api/../internal"));
        assert!(has_dot_segments("/api/%2e%2e/internal"));
        assert!(has_dot_segments("/api/%2E./x"));
        assert!(has_dot_segments("/static/%2e%2E/etc/passwd"));
        assert!(has_dot_segments("/a/../.."));
        assert!(!has_dot_segments("/o/r/releases/v1.0"));
        assert!(!has_dot_segments("/api/rpc"));
        assert!(!has_dot_segments("/brand/logo.png"));
        // `.` alone can't escape a prefix — leave it alone.
        assert!(!has_dot_segments("/api/./rpc"));
    }

    /// Scratch dist/ for try_static — unique per test via the test name.
    async fn dist_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxidean-web-test-{name}"));
        for (rel, body) in files {
            let p = dir.join(rel);
            tokio::fs::create_dir_all(p.parent().unwrap())
                .await
                .unwrap();
            tokio::fs::write(&p, body).await.unwrap();
        }
        dir
    }

    #[tokio::test]
    async fn try_static_serves_asset_with_cache_header() {
        let dist = dist_with("asset", &[("app.js", "console.log(1)")]).await;
        let req = axum::extract::Request::builder()
            .uri("/app.js")
            .body(Body::empty())
            .unwrap();
        let resp = try_static(&dist, "/app.js", req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=3600"
        );
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/javascript"
        );
    }

    #[tokio::test]
    async fn try_static_never_serves_html_shells() {
        let dist = dist_with(
            "shell-guard",
            &[("settings/general/index.html", "<html>shell</html>")],
        )
        .await;
        let req = axum::extract::Request::builder()
            .uri("/settings/general/index.html")
            .body(Body::empty())
            .unwrap();
        assert!(
            try_static(&dist, "/settings/general/index.html", req)
                .await
                .is_none(),
            "raw shell HTML must not be reachable via the static path"
        );
    }

    #[tokio::test]
    async fn try_static_honors_range() {
        let dist = dist_with("range", &[("big.bin", "0123456789")]).await;
        let req = axum::extract::Request::builder()
            .uri("/big.bin")
            .header(header::RANGE, "bytes=2-4")
            .body(Body::empty())
            .unwrap();
        let resp = try_static(&dist, "/big.bin", req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            resp.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 2-4/10"
        );
    }

    /// End-to-end websocket passthrough: raw TCP client → this tier's real
    /// `spa` fallback → upstream that answers 101 and echoes. Asserts the
    /// handshake reaches the upstream, the 101 comes back, and bytes tunnel
    /// in both directions after the upgrade.
    #[tokio::test]
    async fn websocket_upgrade_tunnels_bytes() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::{TcpListener, TcpStream};

        // Upstream: accept, read the HTTP head, 101, then echo forever.
        let up = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let up_port = up.local_addr().unwrap().port();
        let (saw_upgrade_tx, saw_upgrade_rx) = tokio::sync::oneshot::channel::<bool>();
        tokio::spawn(async move {
            let (mut sock, _) = up.accept().await.unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                sock.read_exact(&mut byte).await.unwrap();
                head.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&head).to_lowercase();
            let _ = saw_upgrade_tx.send(head.contains("upgrade: websocket"));
            sock.write_all(
                b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: upgrade\r\n\r\n",
            )
            .await
            .unwrap();
            let mut buf = [0u8; 64];
            loop {
                match sock.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if sock.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let st = Arc::new(Shared {
            dist: PathBuf::from("."),
            meta: inject::AdvertiseMeta {
                ssh_host: None,
                ssh_port: None,
            },
            proxy: Some(proxy::ApiProxy::new(
                &format!("http://127.0.0.1:{up_port}"),
                true,
            )),
            sessions: None,
        });
        let app = Router::new().fallback(spa).with_state(st);
        let web = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let web_port = web.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(web, app.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .unwrap();
        });

        let mut sock = TcpStream::connect(("127.0.0.1", web_port)).await.unwrap();
        sock.write_all(
            b"GET /api/rpc/ws HTTP/1.1\r\nHost: test\r\nConnection: upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
        )
        .await
        .unwrap();

        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            sock.read_exact(&mut byte).await.unwrap();
            head.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&head);
        assert!(
            head.starts_with("HTTP/1.1 101"),
            "expected 101, got: {head}"
        );
        assert!(
            saw_upgrade_rx.await.unwrap(),
            "upgrade header must reach the upstream verbatim"
        );

        sock.write_all(b"ping-payload").await.unwrap();
        let mut echo = [0u8; 12];
        sock.read_exact(&mut echo).await.unwrap();
        assert_eq!(&echo, b"ping-payload", "bytes must tunnel after upgrade");
    }
}
