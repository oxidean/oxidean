//! `oxidean-api-discussion` — `discussions.*` RPC handlers extracted from
//! the monolithic `oxidean-api` per issue #162. The module had zero core
//! callers; it consumes only shared helpers (`repo` ACL, `actions::events`,
//! `webhook::dispatch`, `label` id sets).

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "discussions.answer.clear",
    "discussions.answer.set",
    "discussions.categories.create",
    "discussions.categories.delete",
    "discussions.categories.list",
    "discussions.categories.update",
    "discussions.comments.create",
    "discussions.comments.delete",
    "discussions.comments.list",
    "discussions.comments.update",
    "discussions.create",
    "discussions.delete",
    "discussions.get",
    "discussions.labels.set",
    "discussions.list",
    "discussions.lock",
    "discussions.pin",
    "discussions.transfer",
    "discussions.update",
];
