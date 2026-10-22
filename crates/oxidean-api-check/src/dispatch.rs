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
        "check.suites.list" => arm!(handlers::suites_list(ctx, input)),
        "check.suites.get" => arm!(handlers::suites_get(ctx, input)),
        "check.suites.create" => arm!(handlers::suites_create(ctx, input)),
        "check.suites.rerequest" => arm!(handlers::suites_rerequest(ctx, input)),
        "check.runs.listForSuite" => arm!(handlers::runs_list_for_suite(ctx, input)),
        "check.runs.listForRef" => arm!(handlers::runs_list_for_ref(ctx, input)),
        "check.runs.get" => arm!(handlers::runs_get(ctx, input)),
        "check.runs.create" => arm!(handlers::runs_create(ctx, input)),
        "check.runs.update" => arm!(handlers::runs_update(ctx, input)),
        "check.runs.rerequest" => arm!(handlers::runs_rerequest(ctx, input)),
        "check.annotations.list" => arm!(handlers::annotations_list(ctx, input)),
        _ => None,
    }
}
