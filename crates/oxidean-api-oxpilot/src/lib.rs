//! `oxidean-api-oxpilot` — the `/oxpilot` agentic-chat domain crate.
//!
//! Covers `oxpilot.status` (gate-exempt enablement probe), the
//! session-core procedures (`session_create` / `session_list` /
//! `session_get` / `session_delete` / `message_queue` / `queue_list` /
//! `usage_get`), the `admin.oxpilot.*` settings surface, and the two
//! `/api/oxpilot/...` SSE routes — `POST .../chat` (turn claim +
//! streamed reply) and `GET .../chat/stream` (snapshot resume + live
//! attach). Shared slot types (`LlmProvider`, `ActiveTurn`,
//! `OxpilotSlot`, `SessionService`, enablement guard) live in
//! `oxidean_api_core::oxpilot` because `AppState`/`RpcCtx` carry them;
//! this crate owns the procedures, the route handlers, the turn task,
//! and the concrete `OxpilotSessionService` installed onto the
//! runtime's `OnceLock` seam by the shell.

pub mod connector;
pub mod providers;
pub mod routes;
pub mod runtime;
pub mod services;
pub mod session;
pub mod settings;
pub mod status;
pub mod tools;

mod dispatch;
pub use dispatch::dispatch;

/// RPC procedure names this crate answers (BR1.1 — the pinned set; the
/// shell aggregates it into the manifest/spec surface).
pub const PROCEDURES: &[&str] = &[
    "oxpilot.status",
    "oxpilot.session_create",
    "oxpilot.session_list",
    "oxpilot.session_get",
    "oxpilot.session_delete",
    "oxpilot.message_queue",
    "oxpilot.queue_list",
    "oxpilot.usage_get",
    "oxpilot.turn_cancel",
    "oxpilot.pairing_create",
    "oxpilot.target_list",
    "oxpilot.target_get",
    "oxpilot.target_disconnect",
    "oxpilot.target_revoke",
    "oxpilot.grant_list",
    "oxpilot.grant_get",
    "oxpilot.grant_respond",
    "admin.oxpilot.settings_get",
    "admin.oxpilot.settings_set",
    "admin.oxpilot.providers_list",
    // U5 — the explicit-confirm materialization surface (BR5.1: user
    // procs only, no catalog entry maps here).
    "oxpilot.diff_list",
    "oxpilot.diff_get",
    "oxpilot.diff_apply",
];

#[cfg(test)]
mod tests {
    /// `PROCEDURES` must exactly mirror the `dispatch` match arms — same
    /// source-scan approach as `oxidean-api-core::rpc`'s parity test.
    #[test]
    fn procedures_list_matches_dispatch() {
        let src = include_str!("dispatch.rs");
        let mut arms: Vec<&str> = Vec::new();
        for line in src.lines() {
            let line = line.trim_start();
            if !line.starts_with('"') {
                continue;
            }
            if let Some(end) = line[1..].find('"') {
                let name = &line[1..1 + end];
                let rest = line[2 + end..].trim_start();
                if rest.starts_with("=>") {
                    arms.push(name);
                }
            }
        }
        let listed: std::collections::BTreeSet<&str> = crate::PROCEDURES.iter().copied().collect();
        for arm in &arms {
            assert!(
                listed.contains(arm),
                "dispatch arm `{arm}` missing from PROCEDURES"
            );
        }
        for name in &listed {
            assert!(
                arms.contains(name),
                "PROCEDURES entry `{name}` has no dispatch arm"
            );
        }
    }

    /// BR1.2 — every `oxpilot.*` proc is `SessionOnly` by omission from the
    /// PAT tiers; this closed-set test fails if a name ever enters one.
    #[test]
    fn all_procs_are_session_only() {
        for p in crate::PROCEDURES {
            assert_eq!(
                oxidean_core::classify_pat_proc(p),
                oxidean_core::PatProc::SessionOnly,
                "{p} must stay SessionOnly — PAT/job-token calls are denied"
            );
        }
    }
}
