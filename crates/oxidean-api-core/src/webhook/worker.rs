//! Retry drain loop for pending webhook deliveries (D-HOOK-14).

use std::time::Duration;

use oxidean_db::Database;
use tokio::time::{interval, MissedTickBehavior};

use super::deliver;

/// Spawn a background loop that retries pending deliveries.
pub fn spawn_webhook_worker(db: Database, env_name: String) {
    let interval_secs = std::env::var("OXIDEAN_WEBHOOK_WORKER_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5u64);
    if interval_secs == 0 {
        tracing::info!("webhook delivery worker disabled (interval 0)");
        return;
    }
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if let Err(e) = drain_once(&db, &env_name).await {
                tracing::warn!(error = %e, "webhook worker drain failed");
            }
        }
    });
    tracing::info!(secs = interval_secs, "webhook delivery worker scheduled");
}

async fn drain_once(db: &Database, env_name: &str) -> Result<(), String> {
    let pending = db.list_pending_webhook_deliveries(25).await?;
    for delivery in pending {
        let Ok(hook) = db.get_webhook(&delivery.webhook_id).await else {
            continue;
        };
        if !hook.active {
            continue;
        }
        let _ = deliver::deliver_once(db, &hook, &delivery, env_name).await;
    }
    Ok(())
}
