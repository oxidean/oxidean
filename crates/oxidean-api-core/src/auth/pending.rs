//! Server-side pending external-auth store (state + PKCE verifier, TTL 10m).
//!
//! Mitigates T-04-15: reject missing/mismatched `state` on callback.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PENDING_TTL: Duration = Duration::from_secs(10 * 60);

/// Stashed between `/start` and `/callback` for WorkOS or OIDC.
#[derive(Debug, Clone)]
pub struct PendingAuth {
    pub provider: String,
    pub code_verifier: String,
    pub nonce: Option<String>,
    pub return_to: String,
    /// Redirect URI registered at authorize time (required for OIDC token exchange).
    pub redirect_uri: String,
    pub created_at: Instant,
}

/// In-memory map keyed by OAuth `state` (single-process; fine for Phase 4).
#[derive(Debug, Clone, Default)]
pub struct PendingAuthStore {
    inner: Arc<Mutex<HashMap<String, PendingAuth>>>,
}

impl PendingAuthStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, state: String, pending: PendingAuth) {
        let mut map = self.inner.lock().expect("pending auth lock");
        Self::evict_expired_locked(&mut map);
        map.insert(state, pending);
    }

    /// Take (remove) a pending entry if present and not expired.
    pub fn take(&self, state: &str) -> Option<PendingAuth> {
        let mut map = self.inner.lock().expect("pending auth lock");
        Self::evict_expired_locked(&mut map);
        map.remove(state)
            .filter(|p| p.created_at.elapsed() < PENDING_TTL)
    }

    fn evict_expired_locked(map: &mut HashMap<String, PendingAuth>) {
        map.retain(|_, p| p.created_at.elapsed() < PENDING_TTL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_removes_and_returns() {
        let store = PendingAuthStore::new();
        store.insert(
            "s1".into(),
            PendingAuth {
                provider: "workos".into(),
                code_verifier: "v".into(),
                nonce: None,
                return_to: "/".into(),
                redirect_uri: "http://localhost/cb".into(),
                created_at: Instant::now(),
            },
        );
        let p = store.take("s1").expect("present");
        assert_eq!(p.provider, "workos");
        assert!(store.take("s1").is_none());
    }
}
