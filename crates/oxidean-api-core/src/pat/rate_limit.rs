//! In-memory failed Smart HTTP Basic/PAT auth counters (D-26).
//!
//! Per-process only (Compose single replica); multi-replica deferred.
//! Thresholds: 20 failures / IP and 10 / user per 15-minute sliding window.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(15 * 60);
const IP_LIMIT: usize = 20;
const USER_LIMIT: usize = 10;

/// Sliding-window counters keyed by client IP and optional user id.
#[derive(Debug, Default)]
pub struct FailedAuthLimiter {
    by_ip: HashMap<String, VecDeque<Instant>>,
    by_user: HashMap<String, VecDeque<Instant>>,
}

impl FailedAuthLimiter {
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

    fn retry_after(q: &VecDeque<Instant>, now: Instant) -> Duration {
        match q.front() {
            Some(oldest) => {
                let elapsed = now.saturating_duration_since(*oldest);
                WINDOW.saturating_sub(elapsed).max(Duration::from_secs(1))
            }
            None => Duration::from_secs(1),
        }
    }

    /// `Ok(())` if under limit; `Err(retry_after)` when blocked.
    pub fn check_ip(&mut self, ip: &str) -> Result<(), Duration> {
        let now = Instant::now();
        let q = self.by_ip.entry(ip.to_string()).or_default();
        Self::prune(q, now);
        if q.len() >= IP_LIMIT {
            Err(Self::retry_after(q, now))
        } else {
            Ok(())
        }
    }

    /// `Ok(())` if under limit; `Err(retry_after)` when blocked.
    pub fn check_user(&mut self, user_id: &str) -> Result<(), Duration> {
        let now = Instant::now();
        let q = self.by_user.entry(user_id.to_string()).or_default();
        Self::prune(q, now);
        if q.len() >= USER_LIMIT {
            Err(Self::retry_after(q, now))
        } else {
            Ok(())
        }
    }

    pub fn record_ip(&mut self, ip: &str) {
        let now = Instant::now();
        let q = self.by_ip.entry(ip.to_string()).or_default();
        Self::prune(q, now);
        q.push_back(now);
    }

    pub fn record_user(&mut self, user_id: &str) {
        let now = Instant::now();
        let q = self.by_user.entry(user_id.to_string()).or_default();
        Self::prune(q, now);
        q.push_back(now);
    }

    /// Successful auth clears the user bucket only (not IP) — D-26 / RESEARCH.
    pub fn clear_user(&mut self, user_id: &str) {
        self.by_user.remove(user_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_limit_trips_at_20() {
        let mut lim = FailedAuthLimiter::new();
        for _ in 0..20 {
            assert!(lim.check_ip("1.2.3.4").is_ok());
            lim.record_ip("1.2.3.4");
        }
        let err = lim.check_ip("1.2.3.4").unwrap_err();
        assert!(err.as_secs() >= 1);
    }

    #[test]
    fn user_limit_trips_at_10_and_clear_resets() {
        let mut lim = FailedAuthLimiter::new();
        for _ in 0..10 {
            assert!(lim.check_user("u1").is_ok());
            lim.record_user("u1");
        }
        assert!(lim.check_user("u1").is_err());
        lim.clear_user("u1");
        assert!(lim.check_user("u1").is_ok());
    }
}
