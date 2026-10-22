//! `pages.*` RPC handlers (#162); the build engine + `resolve_pages_dir`/`rebuild_after_push` stay in core.

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "pages.builds.get",
    "pages.builds.list",
    "pages.builds.request",
    "pages.deploys.create",
    "pages.disable",
    "pages.enable",
    "pages.get",
    "pages.update",
];
