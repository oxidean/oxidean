//! Reverse proxy for API-owned prefixes → `OXIDEAN_API_ORIGIN`.
//!
//! Compose puts Traefik in front and routes these paths to the API directly;
//! the proxy exists for dev parity and single-origin deploys (Railway) where
//! the web tier IS the origin. Mirrors the old vite `server.proxy` list.

use std::net::SocketAddr;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

/// Path prefixes the API owns (kept in sync with docker-compose Traefik
/// labels and the retired vite proxy table).
const API_PREFIXES: &[&str] = &[
    "/api/",
    "/uploads",
    // Package registry surfaces (D-PKG-01).
    "/v2",
    "/npm",
    "/generic",
    // CLI distribution (install.sh + ox binaries).
    "/cli",
    // OAuth2 endpoints (API-03). `/oauth/consent` stays a SPA route.
    "/oauth/authorize",
    "/oauth/token",
    "/oauth/userinfo",
];

/// Hop-by-hop headers never cross a proxy hop. `connection`/`upgrade` are
/// re-admitted only for genuine upgrade requests (websocket handshakes such
/// as `/api/rpc/ws`), where forwarding them verbatim is the handshake.
const HOP_BY_HOP: &[&str] = &[
    "host",
    "connection",
    "transfer-encoding",
    "keep-alive",
    "upgrade",
];

/// Client-settable headers that describe the request to the next hop. When
/// this tier is the edge (`behind_proxy == false`) they are attacker-controlled
/// — stripped here, then re-set from observed truth. Behind a sanitizing edge
/// the edge's values forward verbatim instead.
const FORWARDED_HEADERS: &[&str] = &[
    "forwarded",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "x-forwarded-server",
    "x-real-ip",
    "x-client-ip",
    "cf-connecting-ip",
    "true-client-ip",
];

/// Path-segment-aware prefix check (`/v2x` must not match `/v2`).
pub fn is_api_prefix(path: &str) -> bool {
    API_PREFIXES.iter().any(|p| {
        let bare = p.trim_end_matches('/');
        path == bare
            || path
                .strip_prefix(bare)
                .map(|rest| rest.starts_with('/'))
                .unwrap_or(false)
    })
}

pub struct ApiProxy {
    origin: String,
    client: reqwest::Client,
    /// True when a sanitizing edge (Traefik, Railway) sits in front of this
    /// tier — the edge's X-Forwarded-* values are authoritative and pass
    /// through verbatim. False means this tier IS the edge: client-supplied
    /// forwarding headers are stripped and re-set from observed truth so the
    /// API's rightmost-XFF reads and public-origin fallback stay honest.
    behind_proxy: bool,
}

impl ApiProxy {
    pub fn new(origin: &str, behind_proxy: bool) -> Self {
        Self {
            origin: origin.to_string(),
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                // Bound only the connect phase — a full-request timeout would
                // kill long git pushes and upgraded websocket tunnels.
                .connect_timeout(Duration::from_secs(5))
                .build()
                .expect("reqwest client"),
            behind_proxy,
        }
    }

    /// Forward the request upstream, stream the response back. `peer` is the
    /// socket that connected to this tier — the edge's address behind a
    /// proxy, the real client's address otherwise.
    pub async fn forward(&self, mut req: axum::extract::Request, peer: SocketAddr) -> Response {
        // Upgrade intent: `Upgrade` present + `Connection` listing upgrade.
        // Websocket handshakes (`/api/rpc/ws`) pass through as raw tunnels.
        let is_upgrade = req.headers().contains_key(header::UPGRADE)
            && req
                .headers()
                .get(header::CONNECTION)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.to_ascii_lowercase().contains("upgrade"));
        // Capture the client-side upgrade handle BEFORE consuming `req` —
        // hyper hands over the socket once we commit a 101 response below.
        let client_upgrade = is_upgrade.then(|| hyper::upgrade::on(&mut req));

        let (parts, body) = req.into_parts();
        let path_q = parts
            .uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");
        let url = format!("{}{path_q}", self.origin);

        let mut rb = self
            .client
            .request(parts.method.clone(), &url)
            .body(reqwest::Body::wrap_stream(body.into_data_stream()));

        let inbound_has_xfh = parts.headers.contains_key("x-forwarded-host");
        for (name, value) in parts.headers.iter() {
            let n = name.as_str();
            if HOP_BY_HOP.contains(&n) && !(is_upgrade && matches!(n, "connection" | "upgrade")) {
                continue;
            }
            if !self.behind_proxy && FORWARDED_HEADERS.contains(&n) {
                continue;
            }
            rb = rb.header(n, value.as_bytes());
        }
        let host = parts.uri.host().map(str::to_owned).or_else(|| {
            parts
                .headers
                .get(header::HOST)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        });
        if self.behind_proxy {
            // Trust the edge's chain as-is; fill in X-Forwarded-Host only if
            // the edge left it unset.
            if !inbound_has_xfh {
                if let Some(host) = host {
                    rb = rb.header("x-forwarded-host", host);
                }
            }
        } else {
            // This tier is the edge — report observed truth, not claims.
            rb = rb
                .header("x-forwarded-for", peer.ip().to_string())
                .header("x-forwarded-proto", "http");
            if let Some(host) = host {
                rb = rb.header("x-forwarded-host", host);
            }
        }

        let upstream = match rb.send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("proxy {url} failed: {e}");
                return (StatusCode::BAD_GATEWAY, "API unreachable").into_response();
            }
        };

        // Websocket handshake: upstream answered 101 — relay its handshake
        // headers verbatim (sec-websocket-accept/protocol are end-to-end),
        // then splice the two raw sockets in a detached task. `hyper`'s
        // OnUpgrade for the client resolves as soon as this 101 commits.
        if is_upgrade && upstream.status() == StatusCode::SWITCHING_PROTOCOLS {
            let Some(client_upgrade) = client_upgrade else {
                return (StatusCode::BAD_GATEWAY, "upgrade handshake lost").into_response();
            };
            let mut resp = Response::builder().status(StatusCode::SWITCHING_PROTOCOLS);
            for (name, value) in upstream.headers() {
                resp = resp.header(name.as_str(), value.as_bytes());
            }
            let mut upstream_io = match upstream.upgrade().await {
                Ok(io) => io,
                Err(e) => {
                    tracing::warn!("proxy {url} upgrade failed: {e}");
                    return (StatusCode::BAD_GATEWAY, "upgrade failed").into_response();
                }
            };
            tokio::spawn(async move {
                match client_upgrade.await {
                    Ok(client_io) => {
                        let mut c = hyper_util::rt::TokioIo::new(client_io);
                        let _ = tokio::io::copy_bidirectional(&mut c, &mut upstream_io).await;
                    }
                    Err(e) => tracing::debug!("client upgrade abandoned: {e}"),
                }
            });
            return resp
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response());
        }

        let status =
            StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let mut resp = Response::builder().status(status);
        for (name, value) in upstream.headers() {
            if matches!(
                name.as_str(),
                "connection" | "transfer-encoding" | "keep-alive"
            ) {
                continue;
            }
            if let Ok(v) = HeaderValue::from_bytes(value.as_bytes()) {
                resp = resp.header(name.as_str(), v);
            }
        }
        resp.body(Body::from_stream(upstream.bytes_stream()))
            .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Request;
    use axum::http::HeaderMap;
    use axum::routing::any;
    use tokio::net::TcpListener;

    #[test]
    fn prefix_boundaries() {
        assert!(is_api_prefix("/api/rpc"));
        assert!(is_api_prefix("/api"));
        assert!(is_api_prefix("/uploads/avatars/x.png"));
        assert!(is_api_prefix("/v2/oxidean/app/blobs/sha256:ab"));
        assert!(is_api_prefix("/oauth/authorize"));
        assert!(!is_api_prefix("/v2x"));
        assert!(!is_api_prefix("/oauth/consent"));
        assert!(!is_api_prefix("/apiary"));
        assert!(!is_api_prefix("/jesse/app"));
    }

    /// Spin up a one-shot upstream that records the headers it receives.
    async fn header_recorder() -> (u16, tokio::sync::oneshot::Receiver<HeaderMap>) {
        use axum::extract::State;
        use std::sync::{Arc, Mutex};
        let (tx, rx) = tokio::sync::oneshot::channel::<HeaderMap>();
        let tx = Arc::new(Mutex::new(Some(tx)));
        let upstream = axum::Router::new().route(
            "/{*rest}",
            any(
                |State(tx): State<Arc<Mutex<Option<tokio::sync::oneshot::Sender<HeaderMap>>>>>,
                 headers: HeaderMap| async move {
                    let _ = tx.lock().unwrap().take().unwrap().send(headers);
                    "ok"
                },
            )
            .with_state(tx),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, upstream).await });
        (port, rx)
    }

    const PEER: std::net::SocketAddr = std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
        std::net::Ipv4Addr::new(203, 0, 113, 7),
        44444,
    ));

    /// T-06-11 (moved): SSR cookie-forward now lives in the proxy — the
    /// upstream must see the user's Cookie verbatim, hop-by-hop headers must
    /// be dropped, and X-Forwarded-Host must carry the public host.
    #[tokio::test]
    async fn forwards_cookie_and_drops_hop_headers() {
        let (port, rx) = header_recorder().await;
        let proxy = ApiProxy::new(&format!("http://127.0.0.1:{port}"), true);
        let req = Request::builder()
            .uri("http://public.example/api/rpc")
            .header("cookie", "oxidean_session=sekrit")
            .header("connection", "keep-alive")
            .header("host", "public.example")
            .body(Body::empty())
            .unwrap();
        let resp = proxy.forward(req, PEER).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let headers = rx.await.unwrap();
        assert_eq!(
            headers.get("cookie").unwrap(),
            "oxidean_session=sekrit",
            "session cookie must reach the API verbatim"
        );
        assert!(headers.get("connection").is_none());
        assert_eq!(headers.get("x-forwarded-host").unwrap(), "public.example");
    }

    /// Behind a sanitizing edge the edge's forwarded chain passes through
    /// untouched (rightmost XFF stays the edge-observed client IP).
    #[tokio::test]
    async fn behind_proxy_passes_forwarded_chain_verbatim() {
        let (port, rx) = header_recorder().await;
        let proxy = ApiProxy::new(&format!("http://127.0.0.1:{port}"), true);
        let req = Request::builder()
            .uri("http://public.example/api/rpc")
            .header("x-forwarded-for", "198.51.100.9")
            .header("x-forwarded-host", "edge.example")
            .header("x-forwarded-proto", "https")
            .body(Body::empty())
            .unwrap();
        let resp = proxy.forward(req, PEER).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let headers = rx.await.unwrap();
        assert_eq!(headers.get("x-forwarded-for").unwrap(), "198.51.100.9");
        assert_eq!(headers.get("x-forwarded-host").unwrap(), "edge.example");
        assert_eq!(headers.get("x-forwarded-proto").unwrap(), "https");
    }

    /// As the edge: client-supplied forwarding headers are attacker input —
    /// they must be stripped and re-set from observed truth so the API's
    /// rightmost-XFF read can't be spoofed.
    #[tokio::test]
    async fn edge_mode_rewrites_forwarded_headers() {
        let (port, rx) = header_recorder().await;
        let proxy = ApiProxy::new(&format!("http://127.0.0.1:{port}"), false);
        let req = Request::builder()
            .uri("http://public.example/api/rpc")
            .header("x-forwarded-for", "6.6.6.6")
            .header("x-forwarded-host", "evil.example")
            .header("x-forwarded-proto", "https")
            .header("x-real-ip", "6.6.6.6")
            .header("forwarded", "for=6.6.6.6")
            .body(Body::empty())
            .unwrap();
        let resp = proxy.forward(req, PEER).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let headers = rx.await.unwrap();
        assert_eq!(
            headers.get("x-forwarded-for").unwrap(),
            "203.0.113.7",
            "XFF must carry the observed peer, not the client's claim"
        );
        assert_eq!(
            headers.get("x-forwarded-host").unwrap(),
            "public.example",
            "XFH must carry the real Host, not the spoofed one"
        );
        assert_eq!(headers.get("x-forwarded-proto").unwrap(), "http");
        assert!(headers.get("x-real-ip").is_none());
        assert!(headers.get("forwarded").is_none());
    }

    /// Dead upstream → 502, not a hang or a panic.
    #[tokio::test]
    async fn dead_upstream_is_502() {
        let proxy = ApiProxy::new("http://127.0.0.1:1", false);
        let req = Request::builder()
            .uri("http://x/api/rpc")
            .body(Body::empty())
            .unwrap();
        let resp = proxy.forward(req, PEER).await;
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }
}
