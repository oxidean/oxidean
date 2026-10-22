//! `oxidean-api-check` — `check.*` RPC handlers extracted from the monolithic
//! `oxidean-api` per issue #162. `annotation_count` stays in core
//! (`webhook::dispatch` consumes it).

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "check.annotations.list",
    "check.runs.create",
    "check.runs.get",
    "check.runs.listForRef",
    "check.runs.listForSuite",
    "check.runs.rerequest",
    "check.runs.update",
    "check.suites.create",
    "check.suites.get",
    "check.suites.list",
    "check.suites.rerequest",
];
