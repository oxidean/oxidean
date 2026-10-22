//! `oxidean-api-deployment` — `deployment.*` RPC handlers (#162).

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "deployment.create",
    "deployment.delete",
    "deployment.get",
    "deployment.list",
    "deployment.status.create",
    "deployment.status.list",
];
