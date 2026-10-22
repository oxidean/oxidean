//! `oxidean-api-keys` — `sshKey.*`, `gpgKey.*`, and `repo.deployKey.*` RPC
//! handlers extracted from the monolithic `oxidean-api` per issue #162.

mod deploy_keys;
mod dispatch;
mod gpg_keys;
mod ssh_keys;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "gpgKey.add",
    "gpgKey.list",
    "gpgKey.revoke",
    "repo.deployKey.create",
    "repo.deployKey.delete",
    "repo.deployKey.list",
    "sshKey.add",
    "sshKey.list",
    "sshKey.revoke",
];
