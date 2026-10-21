//! `oxidean-api-admin` — instance-admin RPC handlers (`admin.*`) extracted
//! from the monolithic `oxidean-api` per issue #162. The `require_admin`
//! gate + `require_admin_user`/`reject_if_banned` helpers stay in
//! `oxidean-api-core` — `templates::handlers` and `auth::two_factor` call
//! them across domain boundaries.

mod dispatch;
pub mod invites;
pub mod settings;
pub mod users;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "admin.auth.get_settings",
    "admin.auth.update_settings",
    "admin.git.getSettings",
    "admin.git.updateSettings",
    "admin.instance.factory_reset",
    "admin.invites.create",
    "admin.invites.createLink",
    "admin.invites.list",
    "admin.invites.revoke",
    "admin.lfs.getSettings",
    "admin.lfs.getUsage",
    "admin.lfs.updateSettings",
    "admin.mcp.getSettings",
    "admin.mcp.updateSettings",
    "admin.pat.getSettings",
    "admin.pat.updateSettings",
    "admin.repos.gc",
    "admin.templates.delete",
    "admin.templates.list",
    "admin.templates.setEnabled",
    "admin.templates.update",
    "admin.users.ban",
    "admin.users.delete",
    "admin.users.getAccess",
    "admin.users.getActivity",
    "admin.users.list",
    "admin.users.listSessions",
    "admin.users.revokeSessions",
    "admin.users.unban",
    "admin.users.updateRole",
];
