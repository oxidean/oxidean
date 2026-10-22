//! `oxidean-api-org` — organization-domain RPC handlers (`org.*`,
//! `org.actions.*`) extracted from the monolithic `oxidean-api` per issue
//! #162. Shared org machinery (`load_org_by_slug`, `require_org_role`,
//! `to_public`, `db_err`) stays in `oxidean-api-core` — `invites`,
//! `user::lookup` and the packages ACLs consume it.

mod dispatch;
pub mod handlers;
pub mod invites;
pub mod members;
pub mod runners;
pub mod secrets;
pub mod vars;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "org.actions.runners.createRegistrationToken",
    "org.actions.runners.createRemoveToken",
    "org.actions.runners.delete",
    "org.actions.runners.get",
    "org.actions.runners.list",
    "org.actions.secrets.delete",
    "org.actions.secrets.list",
    "org.actions.secrets.listRepos",
    "org.actions.secrets.publicKey",
    "org.actions.secrets.put",
    "org.actions.secrets.setRepos",
    "org.actions.variables.delete",
    "org.actions.variables.list",
    "org.actions.variables.listRepos",
    "org.actions.variables.put",
    "org.actions.variables.setRepos",
    "org.create",
    "org.get",
    "org.invites.accept",
    "org.invites.create",
    "org.invites.createLink",
    "org.invites.list",
    "org.invites.revoke",
    "org.listMine",
    "org.members.add",
    "org.members.list",
    "org.members.remove",
    "org.members.updateRole",
    "org.updateSettings",
];
