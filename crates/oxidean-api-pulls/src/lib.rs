//! `oxidean-api-pulls` — pull-request-domain RPC handlers extracted from the
//! monolithic `oxidean-api` crate (issue #162).
//!
//! Covers `pull.*` (lifecycle, files/commits, comments, reviews +
//! review-requests, merge, branch status/update) plus the pull-owned
//! `repo.mergeSettings.*` names. `pull.create`, ACL, refs, synchronize and
//! merge-gate machinery stay in `oxidean_api_core::pull` — `repo::files`'s
//! create-PR flow and `merge_queue` lean on them cross-domain.

pub mod comments;
pub mod lifecycle;
pub mod merge_ops;
pub mod reviews;
pub mod update_branch;

mod dispatch;
pub use dispatch::dispatch;

/// RPC procedure names this crate answers. The shell aggregates every
/// domain's `PROCEDURES` into the manifest/spec surface.
pub const PROCEDURES: &[&str] = &[
    "pull.create",
    "pull.get",
    "pull.list",
    "pull.update",
    "pull.close",
    "pull.reopen",
    "pull.files",
    "pull.commits",
    "pull.comments.list",
    "pull.comments.create",
    "pull.comments.update",
    "pull.comments.delete",
    "pull.comments.history",
    "pull.comments.resolve",
    "pull.reviews.list",
    "pull.reviews.submit",
    "pull.reviews.dismiss",
    "pull.reviewRequests.list",
    "pull.reviewRequests.add",
    "pull.reviewRequests.remove",
    "pull.merge",
    "pull.branchStatus",
    "pull.updateBranch",
    "repo.mergeSettings.get",
    "repo.mergeSettings.update",
];
