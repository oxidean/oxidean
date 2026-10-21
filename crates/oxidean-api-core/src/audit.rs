//! Durable audit events — write-through to `audit_events` (oxidean-db).
//!
//! Audit failures are logged and swallowed: the log must never break the
//! operation it records. `detail` is a small JSON object — never secrets,
//! tokens, or password material.

use uuid::Uuid;

use crate::rpc::{ClientMeta, RpcCtx};

/// Record one audit event. `actor` is `(user_id, username)`; `target` is
/// `(target_type, target_id)`. Pass the real actor when known so
/// `admin.users.getActivity` can join the feed.
pub async fn record(
    ctx: &RpcCtx,
    actor: Option<(&str, &str)>,
    event_type: &str,
    target: Option<(&str, &str)>,
    detail: Option<String>,
) {
    record_with(&ctx.db, &ctx.client, actor, event_type, target, detail).await;
}

/// Variant for call sites without an `RpcCtx` (SSO HTTP callback routes).
pub async fn record_with(
    db: &oxidean_db::Database,
    client: &ClientMeta,
    actor: Option<(&str, &str)>,
    event_type: &str,
    target: Option<(&str, &str)>,
    detail: Option<String>,
) {
    let (actor_id, actor_username) = match actor {
        Some((id, name)) => (Some(id), name),
        None => (None, ""),
    };
    let (target_type, target_id) = match target {
        Some((t, id)) => (Some(t), Some(id)),
        None => (None, None),
    };
    if let Err(e) = db
        .insert_audit_event(
            &Uuid::new_v4().to_string(),
            actor_id,
            actor_username,
            event_type,
            target_type,
            target_id,
            detail.as_deref(),
            client.ip_address.as_deref(),
            client.user_agent.as_deref(),
        )
        .await
    {
        tracing::warn!(error = %e, event_type, "audit event write failed");
    }
}
