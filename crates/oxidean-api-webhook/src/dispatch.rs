//! Per-domain dispatch — see `oxidean-api-issues` for the contract. `None`
//! means the name is not this crate's; the shell tries the next dispatcher.

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
        "webhook.create" => arm!(handlers::create(ctx, input)),
        "webhook.list" => arm!(handlers::list(ctx, input)),
        "webhook.get" => arm!(handlers::get(ctx, input)),
        "webhook.update" => arm!(handlers::update(ctx, input)),
        "webhook.delete" => arm!(handlers::delete(ctx, input)),
        "webhook.deliveries.list" => arm!(handlers::deliveries_list(ctx, input)),
        "webhook.deliveries.get" => arm!(handlers::deliveries_get(ctx, input)),
        "webhook.ping" => arm!(handlers::ping(ctx, input)),
        "webhook.redeliver" => arm!(handlers::redeliver(ctx, input)),
        _ => None,
    }
}
