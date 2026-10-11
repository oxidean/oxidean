//! Per-session rate limit for `user.lookup` (T-10-03 anti-enumeration).
//!
//! Per-process only (Compose single replica); multi-replica deferred.
//! Threshold: 60 lookups / session per 60-second sliding window.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(60);
const SESSION_LIMIT: usize = 60;

/// Sliding-window counters keyed by session id.
#[derive(Debug, Default)]
pub struct LookupLimiter {
    by_session: HashMap<String, VecDeque<Instant>>,
}

impl LookupLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    fn prune(q: &mut VecDeque<Instant>, now: Instant) {
        while let Some(front) = q.front() {
            if now.saturating_duration_since(*front) > WINDOW {
                q.pop_front();
            } else {
                break;
            }
        }
    }

    /// `true` if under limit; `false` when blocked.
    pub fn check_and_record(&mut self, session_id: &str) -> bool {
        let now = Instant::now();
        let q = self.by_session.entry(session_id.to_string()).or_default();
        Self::prune(q, now);
        if q.len() >= SESSION_LIMIT {
            return false;
        }
        q.push_back(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_limit_trips_at_60() {
        let mut lim = LookupLimiter::new();
        for _ in 0..60 {
            assert!(lim.check_and_record("sess-1"));
        }
        assert!(!lim.check_and_record("sess-1"));
        assert!(lim.check_and_record("sess-2"));
    }
}
