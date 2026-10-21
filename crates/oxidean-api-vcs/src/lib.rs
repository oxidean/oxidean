//! `oxidean-api-vcs` — `vcs.*` RPC handlers extracted from the monolithic
//! `oxidean-api` per issue #162. The `VcsSlot`/`SharedVcs`/`vcs_api` slot
//! stays in `oxidean-api-core` (`AppState`/`RpcCtx`/`jobs` consume it).

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "vcs.blame",
    "vcs.changeset_create",
    "vcs.changeset_get",
    "vcs.diff",
    "vcs.file_get",
    "vcs.git_export",
    "vcs.git_import",
    "vcs.log",
    "vcs.merge",
    "vcs.merge_base",
    "vcs.path_history",
    "vcs.rebase",
    "vcs.ref_list",
    "vcs.ref_update",
    "vcs.repo_enable",
    "vcs.status",
    "vcs.tree",
];
