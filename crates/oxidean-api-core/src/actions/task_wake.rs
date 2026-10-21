//! Task-availability wake for `fetch_task` long-polls (Phase 9).
//!
//! Runners holding a `wait_ms` `fetch_task` register against this `Notify`
//! before their claim probe; anything that makes a job claimable —
//! enqueue, approval release, promotion sweeps, stale requeue — calls
//! [`wake`], so claims land sub-second instead of at poll cadence. A 2 s
//! fallback tick inside `fetch_task` keeps correctness when a promotion
//! path forgets to wake (multi-instance deployments never share this
//! `Notify` — the tick is the floor).

use std::sync::Arc;
use tokio::sync::Notify;

static WAKE: std::sync::OnceLock<Arc<Notify>> = std::sync::OnceLock::new();

/// The shared notify — callers pin [`Notify::notified`] then
/// [`tokio::sync::futures::Notified::enable`] BEFORE probing the queue so a
/// wake between probe and await can't be lost.
pub fn waiter() -> Arc<Notify> {
    WAKE.get_or_init(|| Arc::new(Notify::new())).clone()
}

/// Wake every registered long-poll waiter — call after any state change
/// that could make a `queued` job claimable.
pub fn wake() {
    waiter().notify_waiters();
}
