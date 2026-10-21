//! `merge_queue.*` RPC handlers (#162); the merge-queue engine stays in core.

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "merge_queue.dequeue",
    "merge_queue.enqueue",
    "merge_queue.get",
    "merge_queue.groups",
    "merge_queue.list",
];
