//! Reverse proxy for API-owned prefixes → `OXIDEAN_API_ORIGIN`.
//!
//! Compose puts Traefik in front and routes these paths to the API directly;
//! the proxy exists for dev parity and single-origin deploys (Railway) where
//! the web tier IS the origin. Mirrors the old vite `server.proxy` list.

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
}

impl ApiProxy {
    pub fn new(origin: &str) -> Self {
        Self {
            origin: origin.to_string(),
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("reqwest client"),
        }
    }

    /// Forward the request upstream, stream the response back. Hop-by-hop
    /// headers are dropped; cookies + auth pass through untouched.
    pub async fn forward(&self, req: axum::extract::Request) -> Response {
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

        for (name, value) in parts.headers.iter() {
            if matches!(
                name.as_str(),
                "host" | "connection" | "transfer-encoding" | "keep-alive" | "upgrade"
            ) {
                continue;
            }
            rb = rb.header(name.as_str(), value.as_bytes());
        }
        // X-Forwarded-* so the API can reconstruct the public origin.
        if let Some(host) = parts
            .uri
            .host()
            .or_else(|| {
                parts
                    .headers
                    .get(header::HOST)
                    .and_then(|v| v.to_str().ok())
            })
        {
            rb = rb.header("x-forwarded-host", host);
        }

        let upstream = match rb.send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("proxy {url} failed: {e}");
                return (StatusCode::BAD_GATEWAY, "API unreachable").into_response();
            }
        };

        let status = StatusCode::from_u16(upstream.status().as_u16())
            .unwrap_or(StatusCode::BAD_GATEWAY);
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
        resp
            .body(Body::from_stream(upstream.bytes_stream()))
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

    /// T-06-11 (moved): SSR cookie-forward now lives in the proxy — the
    /// upstream must see the user's Cookie verbatim, hop-by-hop headers must
    /// be dropped, and X-Forwarded-Host must carry the public host.
    #[tokio::test]
    async fn forwards_cookie_and_drops_hop_headers() {
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

        let proxy = ApiProxy::new(&format!("http://127.0.0.1:{port}"));
        let req = Request::builder()
            .uri("http://public.example/api/rpc")
            .header("cookie", "oxidean_session=sekrit")
            .header("connection", "keep-alive")
            .header("host", "public.example")
            .body(Body::empty())
            .unwrap();
        let resp = proxy.forward(req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let headers = rx.await.unwrap();
        assert_eq!(
            headers.get("cookie").unwrap(),
            "oxidean_session=sekrit",
            "session cookie must reach the API verbatim"
        );
        assert!(headers.get("connection").is_none());
        assert_eq!(
            headers.get("x-forwarded-host").unwrap(),
            "public.example"
        );
    }

    /// Dead upstream → 502, not a hang or a panic.
    #[tokio::test]
    async fn dead_upstream_is_502() {
        let proxy = ApiProxy::new("http://127.0.0.1:1");
        let req = Request::builder()
            .uri("http://x/api/rpc")
            .body(Body::empty())
            .unwrap();
        let resp = proxy.forward(req).await;
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }
}
