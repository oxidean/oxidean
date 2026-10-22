//! Per-domain dispatch — see `oxidean-api-issues` for the contract.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::RpcResponse;
use serde_json::Value;

use crate::handlers;

macro_rules! arm {
    ($handler:expr) => {
        match $handler.await {
            Ok(v) => Some(RpcResponse::ok(v)),
            Err(e) => Some(RpcResponse::err(e)),
        }
    };
}

pub async fn dispatch(ctx: &mut RpcCtx, procedure: &str, input: Value) -> Option<RpcResponse> {
    match procedure {
        "repo.mirror.get" => arm!(handlers::get(ctx, input)),
        // `upsert`/`sync_now` are shared library calls (repo lifecycle drives
        // them on settings changes) — they stay in core (#162).
        "repo.mirror.upsert" => arm!(oxidean_api_core::mirror::upsert(ctx, input)),
        "repo.mirror.delete" => arm!(handlers::delete(ctx, input)),
        "repo.mirror.syncNow" => arm!(oxidean_api_core::mirror::sync_now(ctx, input)),
        "repo.mirror.generateSshKey" => arm!(handlers::generate_ssh_key(ctx, input)),
        "repo.mirror.rotateWebhookSecret" => arm!(handlers::rotate_webhook_secret(ctx, input)),
        "repo.mirror.fetchHostKey" => arm!(handlers::fetch_host_key(ctx, input)),
        _ => None,
    }
}
