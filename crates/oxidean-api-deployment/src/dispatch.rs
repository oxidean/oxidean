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
        "deployment.create" => arm!(handlers::create(ctx, input)),
        "deployment.delete" => arm!(handlers::delete(ctx, input)),
        "deployment.get" => arm!(handlers::get(ctx, input)),
        "deployment.list" => arm!(handlers::list(ctx, input)),
        "deployment.status.create" => arm!(handlers::status_create(ctx, input)),
        "deployment.status.list" => arm!(handlers::status_list(ctx, input)),
        _ => None,
    }
}
