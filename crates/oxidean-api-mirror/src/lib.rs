//! `oxidean-api-mirror` — `repo.mirror.*` RPC handlers extracted from the
//! monolithic `oxidean-api` per issue #162. The mirror engine
//! (`engine`/`queue`/`hook`) stays in `oxidean-api-core` — jobs, merge queue,
//! repo mutation hooks, and the app shell drive it.

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "repo.mirror.delete",
    "repo.mirror.fetchHostKey",
    "repo.mirror.generateSshKey",
    "repo.mirror.get",
    "repo.mirror.rotateWebhookSecret",
    "repo.mirror.syncNow",
    "repo.mirror.upsert",
];
