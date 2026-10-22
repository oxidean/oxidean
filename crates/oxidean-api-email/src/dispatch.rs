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
        "email.add" => arm!(handlers::add(ctx, input)),
        "email.list" => arm!(handlers::list(ctx)),
        "email.remove" => arm!(handlers::remove(ctx, input)),
        "email.resendVerify" => arm!(handlers::resend_verify(ctx, input)),
        "email.setPrimary" => arm!(handlers::set_primary(ctx, input)),
        _ => None,
    }
}
