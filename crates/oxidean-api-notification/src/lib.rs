//! `oxidean-api-notification` — `notification.*` RPC handlers (#162).

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "notification.list",
    "notification.markAllRead",
    "notification.markRead",
    "notification.unreadCount",
];
