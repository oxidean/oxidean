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
        "merge_queue.list" => arm!(handlers::list(ctx, input)),
        "merge_queue.get" => arm!(handlers::get(ctx, input)),
        "merge_queue.groups" => arm!(handlers::groups(ctx, input)),
        "merge_queue.enqueue" => arm!(handlers::enqueue(ctx, input)),
        "merge_queue.dequeue" => arm!(handlers::dequeue(ctx, input)),
        _ => None,
    }
}
