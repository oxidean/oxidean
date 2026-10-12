//! Session validation for the per-request stamp + protected gate.
//!
//! `inject`'s `data-oxidean-session` stamp drives pre-hydration skeleton
//! selection, so it should reflect a *live* session, not just cookie presence.
//! When `OXIDEAN_API_ORIGIN` is configured the tier probes
//! `GET {api}/api/auth/session-check` (read-only — one SELECT, no session
//! touch); when it isn't, or the probe fails, callers fall back to the
//! cookie-presence heuristic so an API hiccup can't degrade first paint.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
    /// Short-lived verdict cache keyed by session token. The probe runs on
    /// every cookie-bearing shell render, so repeat navigations would double
    /// API read traffic without it. ≤10s staleness is cosmetic-only — this
    /// feeds a skeleton/login-redirect heuristic; the API stays authoritative
    /// for every data call. Concurrent misses can still stampede a single
    /// probe each — acceptable at this scale.
    cache: Mutex<HashMap<String, (SessionSignal, Instant)>>,
}

/// How long a Valid/Invalid verdict is reused before re-probing upstream.
const VERDICT_TTL: Duration = Duration::from_secs(10);
/// Cache bound — cleared on overflow (sessions churn far slower than this).
const VERDICT_CAP: usize = 4096;

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
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// `token` is the `oxidean_session` cookie value. Fresh cached verdicts
    /// return without an upstream call; `Unknown` is never cached so a
    /// transient outage doesn't pin a stale answer.
    pub async fn check_token(&self, token: &str) -> SessionSignal {
        if let Some(&(sig, at)) = self.cache.lock().unwrap().get(token) {
            if at.elapsed() < VERDICT_TTL {
                return sig;
            }
        }
        let sig = self.probe(token).await;
        if matches!(sig, SessionSignal::Valid | SessionSignal::Invalid) {
            let mut cache = self.cache.lock().unwrap();
            if cache.len() >= VERDICT_CAP {
                cache.clear();
            }
            cache.insert(token.to_string(), (sig, Instant::now()));
        }
        sig
    }

    /// Probe upstream. The API reads only `oxidean_session` from Cookie, so a
    /// narrowed header is exactly equivalent to forwarding it verbatim.
    async fn probe(&self, token: &str) -> SessionSignal {
        let resp = self
            .client
            .get(&self.url)
            .header(
                reqwest::header::COOKIE,
                format!("oxidean_session={token}"),
            )
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

    /// Upstream variant that also counts how many probes it served.
    async fn counted_upstream(
        f: impl Fn(&HeaderMap) -> axum::response::Response + Send + Sync + 'static,
    ) -> (String, Arc<Mutex<u32>>) {
        let count = Arc::new(Mutex::new(0u32));
        let count2 = count.clone();
        let (origin, _) = upstream(move |h| {
            *count2.lock().unwrap() += 1;
            f(h)
        })
        .await;
        (origin, count)
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
        assert_eq!(v.check_token("abc").await, SessionSignal::Valid);
        let headers = seen.lock().unwrap().clone().unwrap();
        assert_eq!(
            headers.get("cookie").unwrap(),
            "oxidean_session=abc",
            "the probe sends the session token as its narrowed Cookie"
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
        assert_eq!(v.check_token("dead").await, SessionSignal::Invalid);

        let (origin, _) = upstream(|_| {
            axum::response::Response::builder()
                .status(500)
                .body("oops".into())
                .unwrap()
        })
        .await;
        let v = SessionValidator::new(&origin);
        assert_eq!(v.check_token("abc").await, SessionSignal::Unknown);

        // Dead upstream → Unknown (presence fallback), never Invalid.
        let v = SessionValidator::new("http://127.0.0.1:1");
        assert_eq!(v.check_token("abc").await, SessionSignal::Unknown);
    }

    #[tokio::test]
    async fn verdicts_cache_briefly() {
        let (origin, count) = counted_upstream(|_| {
            axum::response::Response::builder()
                .header("content-type", "application/json")
                .body("{\"valid\":true}".into())
                .unwrap()
        })
        .await;
        let v = SessionValidator::new(&origin);
        assert_eq!(v.check_token("abc").await, SessionSignal::Valid);
        // Repeat checks within the TTL reuse the verdict — no new probe.
        for _ in 0..3 {
            assert_eq!(v.check_token("abc").await, SessionSignal::Valid);
        }
        assert_eq!(*count.lock().unwrap(), 1, "cached verdict must not re-probe");
        // A different token is a different cache key.
        assert_eq!(v.check_token("other").await, SessionSignal::Valid);
        assert_eq!(*count.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn unknown_is_never_cached() {
        let (origin, count) = counted_upstream(|_| {
            axum::response::Response::builder()
                .status(500)
                .body("oops".into())
                .unwrap()
        })
        .await;
        let v = SessionValidator::new(&origin);
        assert_eq!(v.check_token("abc").await, SessionSignal::Unknown);
        assert_eq!(v.check_token("abc").await, SessionSignal::Unknown);
        assert_eq!(
            *count.lock().unwrap(),
            2,
            "Unknown verdicts must re-probe, not pin a stale answer"
        );
    }
}
