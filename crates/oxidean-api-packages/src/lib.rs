//! `oxidean-api-packages` — `packages.*` RPC handlers extracted from the
//! monolithic `oxidean-api` per issue #162. The registry protocol
//! implementation (`acl`/`auth`/`quota`/`store`/`oci`/`npm`/`generic` and
//! `registry_routers`) stays in `oxidean-api-core` — the shell mounts it.

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "packages.adminSetQuota",
    "packages.adminUsage",
    "packages.delete",
    "packages.deleteVersion",
    "packages.list",
];
