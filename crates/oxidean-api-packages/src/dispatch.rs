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
        "packages.list" => arm!(handlers::list(ctx, input)),
        "packages.deleteVersion" => arm!(handlers::delete_version(ctx, input)),
        "packages.delete" => arm!(handlers::delete(ctx, input)),
        "packages.adminUsage" => arm!(handlers::admin_usage(ctx, input)),
        "packages.adminSetQuota" => arm!(handlers::admin_set_quota(ctx, input)),
        _ => None,
    }
}
