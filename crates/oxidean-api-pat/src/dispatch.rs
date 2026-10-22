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
        "pat.createClassic" => arm!(handlers::create_classic(ctx, input)),
        "pat.createFineGrained" => arm!(handlers::create_fine_grained(ctx, input)),
        "pat.list" => arm!(handlers::list(ctx)),
        "pat.revoke" => arm!(handlers::revoke(ctx, input)),
        "pat.self" => match handlers::self_info(ctx) {
            Ok(v) => Some(RpcResponse::ok(v)),
            Err(e) => Some(RpcResponse::err(e)),
        },
        "pat.policy" => arm!(handlers::effective_policy(ctx)),
        "pat.catalog" => match handlers::catalog(ctx) {
            Ok(v) => Some(RpcResponse::ok(v)),
            Err(e) => Some(RpcResponse::err(e)),
        },
        _ => None,
    }
}
