//! Oxidean git forge backend.
//!
//! Phase 7 ships [`CliGitBackend`] via the system `git` CLI (2.5+).
//! A future `GixGitBackend` (gitoxide) is intentionally deferred — keep the
//! [`GitBackend`] abstraction as the swap seam (GIT-09 / GIT-10 / D-32).
//!
//! Boot version gate (`assert_git_version`) greens in plan 07-17.

pub mod backend;
pub mod cli;
pub mod version;

pub use backend::{
    ssh_host_from_remote_url, validate_remote_url, ArchiveFormat, BlameFile, BlameLine,
    CommitDetail, CommitSummary, ContributorSummary, DiffFile, DiffResult, GitBackend, GitError,
    GitRef, GrepHit, GrepResult, RemoteAuthKind, RemoteCredentials, SizedBlobEntry, TreeEntry,
    TreeEntryKind, ARCHIVE_TIMEOUT, BLAME_SOFT_MAX_LINES, DIFF_SOFT_MAX_BYTES, FORGE_NOREPLY_EMAIL,
};
pub use cli::{
    install_protection_hooks, pushed_commits, reconcile_protection_hooks, CliGitBackend,
    PushedCommit,
};
pub use version::{assert_git_version, parse_git_version};

pub fn crate_name() -> &'static str {
    "oxidean-git"
}
