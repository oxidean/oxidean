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
        "wiki.pages.create" => arm!(handlers::page_create(ctx, input)),
        "wiki.pages.delete" => arm!(handlers::page_delete(ctx, input)),
        "wiki.pages.get" => arm!(handlers::page_get(ctx, input)),
        "wiki.pages.list" => arm!(handlers::pages_list(ctx, input)),
        "wiki.pages.update" => arm!(handlers::page_update(ctx, input)),
        "wiki.revisions.list" => arm!(handlers::revisions_list(ctx, input)),
        _ => None,
    }
}
