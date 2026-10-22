//! `oxidean-api-wiki` — `wiki.*` RPC handlers (#162).

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "wiki.pages.create",
    "wiki.pages.delete",
    "wiki.pages.get",
    "wiki.pages.list",
    "wiki.pages.update",
    "wiki.revisions.list",
];
