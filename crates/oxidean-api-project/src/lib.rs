//! `oxidean-api-project` — `projects.*` RPC handlers extracted from the
//! monolithic `oxidean-api` per issue #162. The module had zero core
//! callers; it consumes shared helpers (`repo` ACL, `actions::events`,
//! `issue::notify_actions_issue`, `webhook::dispatch`).

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "projects.cards.convert",
    "projects.cards.create",
    "projects.cards.delete",
    "projects.cards.list",
    "projects.cards.move",
    "projects.cards.update",
    "projects.columns.create",
    "projects.columns.delete",
    "projects.columns.list",
    "projects.columns.move",
    "projects.columns.update",
    "projects.create",
    "projects.delete",
    "projects.get",
    "projects.list",
    "projects.update",
];
