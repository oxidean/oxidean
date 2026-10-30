//! C8 hook #2 — the connector half of session-delete cleanup (BR6.5).
//! Registered AFTER U3's `OxpilotTurnAbortHook` at shell assembly; the
//! registry runs hooks in registration order and a hook `Err` aborts
//! the whole delete (C8). The single bulk tx moves the session's
//! `pending|allowed` grants and every non-terminal action directly to
//! `cancelled` — never `cancel_requested`, since the rows cascade
//! away and a cancel set built from them is undeliverable. Idempotent:
//! a re-run finds no live rows and answers `Ok`.

use std::sync::Arc;

use async_trait::async_trait;
use oxidean_api_core::oxpilot::{ConnectorGateway, SessionDeleteHook};
use oxidean_api_core::rpc::ClientMeta;
use oxidean_core::AppError;

/// The connector delete hook — delegates to the same `ConnectorGateway`
/// seam everything else uses (the one-gateway rule).
pub struct ConnectorDeleteHook {
    gateway: Arc<dyn ConnectorGateway>,
}

impl ConnectorDeleteHook {
    pub fn new(gateway: Arc<dyn ConnectorGateway>) -> Self {
        ConnectorDeleteHook { gateway }
    }
}

#[async_trait]
impl SessionDeleteHook for ConnectorDeleteHook {
    async fn on_session_delete(&self, session_id: &str, user_id: &str) -> Result<(), AppError> {
        self.gateway
            .on_session_delete(&ClientMeta::default(), session_id, user_id)
            .await
    }
}
