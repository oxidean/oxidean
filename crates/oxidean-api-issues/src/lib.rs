//! `oxidean-api-issues` — issues-domain RPC handlers extracted from the
//! monolithic `oxidean-api` crate (issue #162).
//!
//! Covers `issue.*` (lifecycle, comments, labels/assignees, reactions,
//! links), `milestone.*` + `issue.milestone.set`, and `label.*`.
//! Shared enrichment/notification helpers other domains lean on live in
//! `oxidean_api_core::{issue,label,milestone}`; this crate depends only on
//! `oxidean-api-core` + the foundational crates — never on sibling domain
//! crates.

pub mod issue;
pub mod label;
pub mod milestone;

mod dispatch;
pub use dispatch::dispatch;

/// RPC procedure names this crate answers. The shell aggregates every
/// domain's `PROCEDURES` into the manifest/spec surface.
pub const PROCEDURES: &[&str] = &[
    "issue.create",
    "issue.get",
    "issue.list",
    "issue.update",
    "issue.close",
    "issue.reopen",
    "issue.history",
    "issue.delete",
    "issue.comments.list",
    "issue.comments.create",
    "issue.comments.update",
    "issue.comments.delete",
    "issue.comments.history",
    "issue.labels.set",
    "issue.assignees.set",
    "issue.milestone.set",
    "issue.assigneeCandidates",
    "issue.reactions.toggle",
    "issue.links.list",
    "issue.links.add",
    "issue.links.remove",
    "milestone.list",
    "milestone.get",
    "milestone.create",
    "milestone.update",
    "milestone.delete",
    "label.listForRepo",
    "label.listForOrg",
    "label.create",
    "label.update",
    "label.delete",
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
}
