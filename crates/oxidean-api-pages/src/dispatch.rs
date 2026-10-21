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
        "pages.get" => arm!(handlers::get(ctx, input)),
        "pages.enable" => arm!(handlers::enable(ctx, input)),
        "pages.update" => arm!(handlers::update(ctx, input)),
        "pages.disable" => arm!(handlers::disable(ctx, input)),
        "pages.builds.list" => arm!(handlers::builds_list(ctx, input)),
        "pages.builds.get" => arm!(handlers::builds_get(ctx, input)),
        "pages.builds.request" => arm!(handlers::builds_request(ctx, input)),
        "pages.deploys.create" => arm!(handlers::deploys_create(ctx, input)),
        _ => None,
    }
}
