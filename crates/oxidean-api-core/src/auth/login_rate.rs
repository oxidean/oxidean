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
/// Tracked keys per map — bounds memory against attacker-chosen
/// identifier strings (e.g. `tok:<hash>` reset keys). Inserts first reap
/// dead keys, then evict the least-recently-active entry.
const MAX_KEYS: usize = 10_000;

/// Sliding-window counters keyed by client IP and normalized identifier.
/// Maps are capped (`MAX_KEYS`) and dead keys reaped on insert so
/// unauthenticated identifier churn cannot grow state without bound.
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

    /// Window-pruned length for `key`; drops the key entirely once its
    /// deque empties so dead keys do not accumulate.
    fn fresh_len(map: &mut HashMap<String, VecDeque<Instant>>, key: &str, now: Instant) -> usize {
        let Some(q) = map.get_mut(key) else {
            return 0;
        };
        Self::prune(q, now);
        let len = q.len();
        if len == 0 {
            map.remove(key);
        }
        len
    }

    /// Make room for one new key when the map is full: reap dead keys
    /// first, then the least-recently-active entry.
    fn evict_for_space(map: &mut HashMap<String, VecDeque<Instant>>, now: Instant) {
        if map.len() < MAX_KEYS {
            return;
        }
        map.retain(|_, q| {
            Self::prune(q, now);
            !q.is_empty()
        });
        if map.len() >= MAX_KEYS {
            if let Some(oldest) = map
                .iter()
                .min_by_key(|(_, q)| q.back().copied())
                .map(|(k, _)| k.clone())
            {
                map.remove(&oldest);
            }
        }
    }

    /// Record `now` under `key`, bounding map growth via `evict_for_space`.
    fn insert_now(map: &mut HashMap<String, VecDeque<Instant>>, key: &str, now: Instant) {
        if let Some(q) = map.get_mut(key) {
            Self::prune(q, now);
            q.push_back(now);
            return;
        }
        Self::evict_for_space(map, now);
        map.entry(key.to_string()).or_default().push_back(now);
    }

    /// `true` when both buckets are under limit — records nothing and
    /// creates no keys, for credential checks that consume budget only
    /// on failure (`auth.login`, H-1).
    pub fn check(&mut self, ip: &str, identifier: &str) -> bool {
        let now = Instant::now();
        Self::fresh_len(&mut self.by_ip, ip, now) < IP_LIMIT
            && Self::fresh_len(&mut self.by_ident, identifier, now) < IDENT_LIMIT
    }

    /// `true` when this attempt is under both limits. The record is
    /// atomic across buckets — a request rejected by either consumes
    /// neither, so a blocked IP cannot burn a victim's identifier
    /// budget (or a blocked identifier burn the IP's).
    pub fn check_and_record(&mut self, ip: &str, identifier: &str) -> bool {
        if !self.check(ip, identifier) {
            return false;
        }
        self.record_failure(ip, identifier);
        true
    }

    /// Push a failed credential attempt into both buckets.
    pub fn record_failure(&mut self, ip: &str, identifier: &str) {
        let now = Instant::now();
        Self::insert_now(&mut self.by_ip, ip, now);
        Self::insert_now(&mut self.by_ident, identifier, now);
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

    /// A request rejected by one bucket consumes no budget in the other —
    /// a rate-limited IP cannot fill a victim's identifier bucket by
    /// repeating the identifier.
    #[test]
    fn rejected_attempt_records_neither_bucket() {
        let mut lim = LoginBeginLimiter::new();
        for i in 0..IP_LIMIT {
            assert!(lim.check_and_record("10.0.0.1", &format!("spray{i}@b.c")));
        }
        for _ in 0..IP_LIMIT {
            assert!(!lim.check_and_record("10.0.0.1", "victim@b.c"));
        }
        // The victim's identifier budget is intact from a healthy IP.
        assert!(lim.check_and_record("10.0.0.2", "victim@b.c"));
        assert_eq!(lim.by_ident["victim@b.c"].len(), 1);
    }

    /// Identifier keys are attacker-chosen (reset `tok:<hash>` keys) — the
    /// map reaps dead keys and evicts least-recently-active entries
    /// instead of growing without bound.
    #[test]
    fn ident_keys_are_bounded() {
        let mut lim = LoginBeginLimiter::new();
        for i in 0..MAX_KEYS + 64 {
            let ip = format!("10.{}.{}.{}", (i >> 16) & 0xff, (i >> 8) & 0xff, i & 0xff);
            assert!(lim.check_and_record(&ip, &format!("k{i}")));
        }
        assert!(lim.by_ident.len() <= MAX_KEYS);
    }
}
