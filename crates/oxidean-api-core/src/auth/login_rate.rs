//! `auth.login_begin` rate limit (#130 identifier-first sign-in).
//!
//! `login_begin` is unauthenticated and runs before the password check: every
//! challenge it opens resets the factor-attempt budget and can trigger OTP
//! mail, so challenge churn is bounded per client IP and per identifier.
//! Per-process only (Compose single replica), same as `FailedAuthLimiter`
//! and `LookupLimiter`.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(15 * 60);
/// Begins per client IP per window — generous for shared/NAT egress.
const IP_LIMIT: usize = 30;
/// Begins per identifier per window — bounds per-account factor probing
/// (8 factor attempts × 10 challenges ≪ brute-force space).
const IDENT_LIMIT: usize = 10;

/// Sliding-window counters keyed by client IP and normalized identifier.
#[derive(Debug, Default)]
pub struct LoginBeginLimiter {
    by_ip: HashMap<String, VecDeque<Instant>>,
    by_ident: HashMap<String, VecDeque<Instant>>,
}

impl LoginBeginLimiter {
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

    fn under_limit(q: &mut VecDeque<Instant>, now: Instant, limit: usize) -> bool {
        Self::prune(q, now);
        if q.len() >= limit {
            return false;
        }
        q.push_back(now);
        true
    }

    /// `true` if this begin is under both limits (and records it); `false`
    /// when blocked — the record still counts so a hammering key stays shut.
    pub fn check_and_record(&mut self, ip: &str, identifier: &str) -> bool {
        let now = Instant::now();
        let ip_ok = {
            let q = self.by_ip.entry(ip.to_string()).or_default();
            Self::under_limit(q, now, IP_LIMIT)
        };
        let ident_ok = {
            let q = self.by_ident.entry(identifier.to_string()).or_default();
            Self::under_limit(q, now, IDENT_LIMIT)
        };
        ip_ok && ident_ok
    }

    /// `true` when both buckets are under limit — no record, for credential
    /// checks that consume budget only on failure (`auth.login`, H-1).
    pub fn check(&mut self, ip: &str, identifier: &str) -> bool {
        let now = Instant::now();
        let ip_ok = {
            let q = self.by_ip.entry(ip.to_string()).or_default();
            Self::prune(q, now);
            q.len() < IP_LIMIT
        };
        let ident_ok = {
            let q = self.by_ident.entry(identifier.to_string()).or_default();
            Self::prune(q, now);
            q.len() < IDENT_LIMIT
        };
        ip_ok && ident_ok
    }

    /// Push a failed credential attempt into both buckets.
    pub fn record_failure(&mut self, ip: &str, identifier: &str) {
        let now = Instant::now();
        {
            let q = self.by_ip.entry(ip.to_string()).or_default();
            Self::prune(q, now);
            q.push_back(now);
        }
        {
            let q = self.by_ident.entry(identifier.to_string()).or_default();
            Self::prune(q, now);
            q.push_back(now);
        }
    }

    /// Drop the newest recorded attempt from the IP bucket — refunds one
    /// in-flight reservation after a successful credential check while
    /// prior failures stay counted. The ident side is cleared by
    /// `clear_ident`.
    pub fn release_ip(&mut self, ip: &str) {
        if let Some(q) = self.by_ip.get_mut(ip) {
            q.pop_back();
            if q.is_empty() {
                self.by_ip.remove(ip);
            }
        }
    }

    /// Successful credential check clears the identifier bucket only —
    /// `FailedAuthLimiter::clear_user` parity (a proven credential resets
    /// the per-account budget; the IP's failure history stays).
    pub fn clear_ident(&mut self, identifier: &str) {
        self.by_ident.remove(identifier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ident_limit_trips() {
        let mut lim = LoginBeginLimiter::new();
        for _ in 0..IDENT_LIMIT {
            assert!(lim.check_and_record("10.0.0.1", "a@b.c"));
        }
        assert!(!lim.check_and_record("10.0.0.1", "a@b.c"));
        assert!(lim.check_and_record("10.0.0.1", "b@b.c"));
    }

    #[test]
    fn ip_limit_trips_across_idents() {
        let mut lim = LoginBeginLimiter::new();
        for i in 0..IP_LIMIT {
            assert!(lim.check_and_record("10.0.0.9", &format!("u{i}@b.c")));
        }
        assert!(!lim.check_and_record("10.0.0.9", "one-more@b.c"));
        assert!(lim.check_and_record("10.0.0.10", "u0@b.c"));
    }

    /// `auth.login` semantics: checks are free, only failures consume budget,
    /// and a proven credential resets the per-identifier bucket.
    #[test]
    fn failure_only_budget_and_clear() {
        let mut lim = LoginBeginLimiter::new();
        // Successful sign-ins (check only) never exhaust the budget.
        for _ in 0..IP_LIMIT * 2 {
            assert!(lim.check("10.0.0.1", "a@b.c"));
        }
        for _ in 0..IDENT_LIMIT {
            lim.record_failure("10.0.0.1", "a@b.c");
        }
        assert!(!lim.check("10.0.0.1", "a@b.c"));
        // A different identifier on the same IP is still under limit.
        assert!(lim.check("10.0.0.1", "b@b.c"));
        // Proven credentials reset the identifier bucket (IP stays warm).
        lim.clear_ident("a@b.c");
        assert!(lim.check("10.0.0.1", "a@b.c"));
    }
}
