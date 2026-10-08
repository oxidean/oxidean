//! Session validation for the per-request stamp + protected gate.
//!
//! `inject`'s `data-oxidean-session` stamp drives pre-hydration skeleton
//! selection, so it should reflect a *live* session, not just cookie presence.
//! When `OXIDEAN_API_ORIGIN` is configured the tier probes
//! `GET {api}/api/auth/session-check` (read-only — one SELECT, no session
//! touch); when it isn't, or the probe fails, callers fall back to the
//! cookie-presence heuristic so an API hiccup can't degrade first paint.

use std::time::Duration;

/// What the request's credentials resolve to — feeds the protected gate and
/// the `data-oxidean-session` stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSignal {
    /// No `oxidean_session` cookie presented (a bare `oxidean_signed_in` hint
    /// counts as absent — the hint is a companion, never the credential).
    Absent,
    /// Upstream confirmed a live session.
    Valid,
    /// Upstream answered definitively: expired / revoked / unknown token.
    Invalid,
    /// Couldn't ask upstream (no origin configured, transport error, timeout,
    /// 5xx, malformed body) — callers fall back to presence-based behavior.
    Unknown,
}

/// `GET {api}/api/auth/session-check` probe client.
pub struct SessionValidator {
    client: reqwest::Client,
    url: String,
}

impl SessionValidator {
    pub fn new(api_origin: &str) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(750))
            .build()
            .expect("session validator client");
        Self {
            client,
            url: format!("{api_origin}/api/auth/session-check"),
        }
    }

    /// `cookie` is the raw Cookie header value — forwarded verbatim so the API
    /// parses the session token exactly as an edge-routed request would.
    pub async fn check(&self, cookie: &str) -> SessionSignal {
        let resp = self
            .client
            .get(&self.url)
            .header(reqwest::header::COOKIE, cookie)
            .send()
            .await;
        let Ok(resp) = resp else {
            return SessionSignal::Unknown;
        };
        if !resp.status().is_success() {
            return SessionSignal::Unknown;
        }
        let Ok(bytes) = resp.bytes().await else {
            return SessionSignal::Unknown;
        };
        match serde_json::from_slice::<SessionCheckBody>(&bytes) {
            Ok(SessionCheckBody { valid: true }) => SessionSignal::Valid,
            Ok(_) => SessionSignal::Invalid,
            Err(_) => SessionSignal::Unknown,
        }
    }
}

#[derive(serde::Deserialize)]
struct SessionCheckBody {
    valid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Request;
    use axum::http::HeaderMap;
    use axum::routing::any;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    async fn upstream(
        f: impl Fn(&HeaderMap) -> axum::response::Response + Send + Sync + 'static,
    ) -> (String, Arc<Mutex<Option<HeaderMap>>>) {
        let seen = Arc::new(Mutex::new(None::<HeaderMap>));
        let seen2 = seen.clone();
        let f = Arc::new(f);
        let app = axum::Router::new().route(
            "/api/auth/session-check",
            any(move |headers: HeaderMap, _req: Request| {
                let f = f.clone();
                let seen2 = seen2.clone();
                async move {
                    let out = f(&headers);
                    *seen2.lock().unwrap() = Some(headers);
                    out
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await });
        (format!("http://127.0.0.1:{port}"), seen)
    }

    #[tokio::test]
    async fn valid_cookie_maps_to_valid() {
        let (origin, seen) = upstream(|_| {
            axum::response::Response::builder()
                .header("content-type", "application/json")
                .body("{\"valid\":true}".into())
                .unwrap()
        })
        .await;
        let v = SessionValidator::new(&origin);
        assert_eq!(
            v.check("oxidean_session=abc; other=1").await,
            SessionSignal::Valid
        );
        let headers = seen.lock().unwrap().clone().unwrap();
        assert_eq!(
            headers.get("cookie").unwrap(),
            "oxidean_session=abc; other=1",
            "cookie header must reach the probe verbatim"
        );
    }

    #[tokio::test]
    async fn invalid_and_error_mapping() {
        let (origin, _) = upstream(|_| {
            axum::response::Response::builder()
                .header("content-type", "application/json")
                .body("{\"valid\":false}".into())
                .unwrap()
        })
        .await;
        let v = SessionValidator::new(&origin);
        assert_eq!(
            v.check("oxidean_session=dead").await,
            SessionSignal::Invalid
        );

        let (origin, _) = upstream(|_| {
            axum::response::Response::builder()
                .status(500)
                .body("oops".into())
                .unwrap()
        })
        .await;
        let v = SessionValidator::new(&origin);
        assert_eq!(v.check("oxidean_session=abc").await, SessionSignal::Unknown);

        // Dead upstream → Unknown (presence fallback), never Invalid.
        let v = SessionValidator::new("http://127.0.0.1:1");
        assert_eq!(
            v.check("oxidean_session=abc").await,
            SessionSignal::Unknown
        );
    }
}
