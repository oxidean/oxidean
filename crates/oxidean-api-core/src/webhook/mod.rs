//! Webhook delivery engine — `dispatch`/`deliver`/`payloads`/`worker` stay
//! in core: most domains emit lifecycle webhooks through them (#162). The
//! `webhook.*` RPC handlers moved to `oxidean-api-webhook`.

pub mod deliver;
pub mod dispatch;
pub mod payloads;
pub mod worker;
