//! `oxidean-api-email` — `email.*` RPC handlers (#162).

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "email.add",
    "email.list",
    "email.remove",
    "email.resendVerify",
    "email.setPrimary",
];
