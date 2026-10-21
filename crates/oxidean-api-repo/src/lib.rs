//! `oxidean-api-repo` — repository-domain RPC handlers extracted from the
//! monolithic `oxidean-api` crate (issue #162).
//!
//! Covers `repo.*` except `repo.mirror.*`, `repo.deployKey.*`, `repo.actions.*`
//! and `repo.templates.{get,set}Enabled` (owned by other domains/shared infra).
//! The repo ACL layer, unit-flag machinery, signature plumbing, file-commit
//! machinery, author resolution and redirect plumbing stay in
//! `oxidean_api_core::repo` — every other domain leans on them.

pub mod activity;
pub mod branch_protection;
pub mod collaborators;
pub mod commit_status;
pub mod file_templates;
pub mod files;
pub mod insights;
pub mod invites;
pub mod language_stats;
pub mod lifecycle;
pub mod rename_transfer;
pub mod search;
pub mod social_lists;
pub mod sync_fork;
pub mod tag_protection;
pub mod templates;
pub mod units;

mod dispatch;
pub use dispatch::dispatch;

/// RPC procedure names this crate answers. The shell aggregates every
/// domain's `PROCEDURES` into the manifest/spec surface.
pub const PROCEDURES: &[&str] = &[
    "repo.listMine",
    "repo.listByOwner",
    "repo.createDefaults",
    "repo.create",
    "repo.fork",
    "repo.migrate",
    "repo.star",
    "repo.unstar",
    "repo.watch",
    "repo.unwatch",
    "repo.stargazers.list",
    "repo.watchers.list",
    "repo.forks.list",
    "repo.forkStatus",
    "repo.syncFork",
    "repo.updateMetadata",
    "repo.topicsSuggest",
    "repo.explore",
    "repo.get",
    "repo.tree",
    "repo.blob",
    "repo.refs",
    "repo.commits",
    "repo.pathLastCommits",
    "repo.commitCount",
    "repo.contributors.list",
    "repo.languages",
    "repo.activity.list",
    "repo.insights.contributors",
    "repo.insights.commitActivity",
    "repo.insights.forkNetwork",
    "repo.commit",
    "repo.compare",
    "repo.blame",
    "repo.search",
    "repo.branchCreate",
    "repo.branchRename",
    "repo.branchDelete",
    "repo.file.create",
    "repo.file.update",
    "repo.file.delete",
    "repo.file.rename",
    "repo.file.upload",
    "repo.file.mkdir",
    "repo.file.commitPolicy",
    "repo.updateVisibility",
    "repo.setArchived",
    "repo.lfs.setEnabled",
    "repo.lfs.getEnabled",
    "repo.templates.list",
    "repo.lfs.getStatus",
    "repo.lfs.getUsage",
    "repo.quota.get",
    "repo.quota.set",
    "repo.lfs.listObjects",
    "repo.lfs.download",
    "repo.softDelete",
    "repo.rename",
    "repo.transfer",
    "repo.collaborators.list",
    "repo.collaborators.add",
    "repo.collaborators.update",
    "repo.collaborators.remove",
    "repo.invites.create",
    "repo.invites.createLink",
    "repo.invites.list",
    "repo.invites.revoke",
    "repo.branchProtection.list",
    "repo.branchProtection.create",
    "repo.branchProtection.update",
    "repo.branchProtection.delete",
    "repo.tagProtection.list",
    "repo.tagProtection.create",
    "repo.tagProtection.update",
    "repo.tagProtection.delete",
    "repo.commitStatus.create",
    "repo.commitStatus.list",
    "repo.issues.getEnabled",
    "repo.issues.setEnabled",
    "repo.pulls.getEnabled",
    "repo.pulls.setEnabled",
    "repo.wiki.getEnabled",
    "repo.wiki.setEnabled",
    "repo.projects.getEnabled",
    "repo.projects.setEnabled",
    "repo.discussions.getEnabled",
    "repo.discussions.setEnabled",
];

#[cfg(test)]
mod tests {
    /// Every name in `PROCEDURES` must be answered by `dispatch` — guards
    /// against drift between the manifest surface and the match arms.
    #[test]
    fn procedures_match_dispatch() {
        for name in super::PROCEDURES {
            assert!(
                dispatch_matches(name),
                "PROCEDURES entry {name} has no dispatch arm"
            );
        }
    }

    fn dispatch_matches(name: &str) -> bool {
        let src = include_str!("dispatch.rs");
        src.contains(&format!("\"{name}\""))
    }
}
