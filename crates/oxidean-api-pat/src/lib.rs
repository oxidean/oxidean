//! `pat.*` RPC handlers (#162); `pat::bearer`/`policy`/`rate_limit`/`mint_classic_pat` stay in core.

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "pat.catalog",
    "pat.createClassic",
    "pat.createFineGrained",
    "pat.list",
    "pat.policy",
    "pat.revoke",
    "pat.self",
];
