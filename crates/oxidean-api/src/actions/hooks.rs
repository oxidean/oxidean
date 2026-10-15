//! Phase 12 integration hooks for Actions (D-ACT-05).
//!
//! Wired from `pull` lifecycle (create → Opened, synchronize_after_push → Synchronize,
//! reopen → Reopened) via [`notify_pull_request_actions`] (soft-fail).
//!
//! | PR lifecycle | `PullRequestAction` | Call site |
//! |--------------|---------------------|-----------|
//! | PR opened | `Opened` | `pull::create` |
//! | PR head updated | `Synchronize` | `pull::synchronize_after_push` |
//! | PR reopened | `Reopened` | `pull::reopen` |
//!
//! Do **not** expose a public HTTP endpoint for these events (T-19-13).

pub use crate::actions::events::{
    dispatch_pull_request, dispatch_pull_request_for_sha, notify_pull_request_actions,
    PullRequestAction, PullRequestEvent,
};

#[cfg(test)]
mod harness {
    use super::*;

    #[test]
    fn phase12_hook_symbols_are_invocable() {
        // Compile-time surface: Phase 12 imports these names from `actions::hooks`.
        assert_eq!(format!("{:?}", PullRequestAction::Opened), "Opened");
        assert_eq!(
            format!("{:?}", PullRequestAction::Synchronize),
            "Synchronize"
        );
        assert_eq!(format!("{:?}", PullRequestAction::Reopened), "Reopened");
        let _fn = dispatch_pull_request;
        let _fn2 = dispatch_pull_request_for_sha;
        let _ = std::mem::size_of_val(&_fn);
        let _ = std::mem::size_of_val(&_fn2);
        let _ = std::any::type_name::<PullRequestEvent>();
    }
}
