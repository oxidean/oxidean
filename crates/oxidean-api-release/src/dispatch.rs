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
        "release.create" => arm!(handlers::create(ctx, input)),
        "release.list" => arm!(handlers::list(ctx, input)),
        "release.get" => arm!(handlers::get(ctx, input)),
        "release.update" => arm!(handlers::update(ctx, input)),
        "release.delete" => arm!(handlers::delete(ctx, input)),
        "release.deleteAsset" => arm!(handlers::delete_asset(ctx, input)),
        _ => None,
    }
}
