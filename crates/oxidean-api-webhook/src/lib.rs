//! `oxidean-api-webhook` — `webhook.*` RPC handlers extracted from the
//! monolithic `oxidean-api` per issue #162. The delivery engine
//! (`dispatch`/`deliver`/`payloads`/`worker`) stays in `oxidean-api-core` —
//! nearly every domain emits lifecycle webhooks through it.

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "webhook.create",
    "webhook.delete",
    "webhook.deliveries.get",
    "webhook.deliveries.list",
    "webhook.get",
    "webhook.list",
    "webhook.ping",
    "webhook.redeliver",
    "webhook.update",
];
