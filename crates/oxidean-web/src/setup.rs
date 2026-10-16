//! Setup-status probe for the per-request `data-oxidean-setup` stamp.
//!
//! `needs_setup` is deployment-global (empty users table, no ENV seed) and
//! flips once in an instance's lifetime — exactly the kind of fact that
//! doesn't belong in a per-page SPA query. The web tier probes
//! `GET {api}/api/auth/setup-check` and stamps `<html>` so chrome and the
//! access gate mount already-resolved instead of waiting on
//! `auth.bootstrapStatus`. Probe failures stamp nothing (fail-open), matching
//! the old query-error path, which treated the flag as false.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a `false` verdict (setup done) is reused before re-probing.
/// `true` verdicts are never cached: the needs-setup window is the one place
/// a stale stamp can wedge the app (`/setup` completion → home stamped "1" →
/// access gate bounces back to /setup), so every shell request re-probes
/// while the instance still needs setup.
const VERDICT_TTL: Duration = Duration::from_secs(10);

/// `GET {api}/api/auth/setup-check` probe client — a single global verdict
/// (the flag is not per-session), so the cache is one slot, not a map.
pub struct SetupChecker {
    client: reqwest::Client,
    url: String,
    /// When setup was last confirmed done; `true` verdicts and failures are
    /// never cached (see VERDICT_TTL).
    done_at: Mutex<Option<Instant>>,
}

impl SetupChecker {
    pub fn new(api_origin: &str) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(750))
            .build()
            .expect("setup checker client");
        Self {
            client,
            url: format!("{api_origin}/api/auth/setup-check"),
            done_at: Mutex::new(None),
        }
    }

    /// Latest verdict. `None` on probe failure — callers stamp nothing, so a
    /// dead API can't wedge the instance into /setup.
    pub async fn needs_setup(&self) -> Option<bool> {
        if let Some(at) = *self.done_at.lock().unwrap() {
            if at.elapsed() < VERDICT_TTL {
                return Some(false);
            }
        }
        let v = self.probe().await;
        if v == Some(false) {
            *self.done_at.lock().unwrap() = Some(Instant::now());
        }
        v
    }

    async fn probe(&self) -> Option<bool> {
        let resp = self.client.get(&self.url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let bytes = resp.bytes().await.ok()?;
        serde_json::from_slice::<SetupCheckBody>(&bytes)
            .ok()
            .map(|b| b.needs_setup)
    }
}

#[derive(serde::Deserialize)]
struct SetupCheckBody {
    needs_setup: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Request;
    use axum::routing::any;
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::net::TcpListener;

    async fn upstream(body: &'static str, status: u16) -> (String, Arc<StdMutex<u32>>) {
        let count = Arc::new(StdMutex::new(0u32));
        let count2 = count.clone();
        let app = axum::Router::new().route(
            "/api/auth/setup-check",
            any(move |_req: Request| {
                let count2 = count2.clone();
                async move {
                    *count2.lock().unwrap() += 1;
                    axum::response::Response::builder()
                        .status(status)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(body))
                        .unwrap()
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await });
        (format!("http://127.0.0.1:{port}"), count)
    }

    #[tokio::test]
    async fn true_and_false_map() {
        let (origin, _) = upstream("{\"needs_setup\":true}", 200).await;
        let c = SetupChecker::new(&origin);
        assert_eq!(c.needs_setup().await, Some(true));

        let (origin, _) = upstream("{\"needs_setup\":false}", 200).await;
        let c = SetupChecker::new(&origin);
        assert_eq!(c.needs_setup().await, Some(false));
    }

    #[tokio::test]
    async fn verdicts_cache_briefly() {
        let (origin, count) = upstream("{\"needs_setup\":false}", 200).await;
        let c = SetupChecker::new(&origin);
        for _ in 0..3 {
            assert_eq!(c.needs_setup().await, Some(false));
        }
        assert_eq!(
            *count.lock().unwrap(),
            1,
            "cached verdict must not re-probe"
        );
    }

    #[tokio::test]
    async fn true_verdicts_are_not_cached() {
        // A stale "1" stamp after /setup completion bounces the access gate —
        // the needs-setup window must re-probe every shell request.
        let (origin, count) = upstream("{\"needs_setup\":true}", 200).await;
        let c = SetupChecker::new(&origin);
        assert_eq!(c.needs_setup().await, Some(true));
        assert_eq!(c.needs_setup().await, Some(true));
        assert_eq!(
            *count.lock().unwrap(),
            2,
            "needs-setup verdicts must re-probe"
        );
    }

    #[tokio::test]
    async fn failures_are_not_cached() {
        let (origin, count) = upstream("oops", 500).await;
        let c = SetupChecker::new(&origin);
        assert_eq!(c.needs_setup().await, None);
        assert_eq!(c.needs_setup().await, None);
        assert_eq!(*count.lock().unwrap(), 2, "failed probes must retry");
    }
}
