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
        "notification.list" => arm!(handlers::list(ctx, input)),
        "notification.markAllRead" => arm!(handlers::mark_all_read(ctx, input)),
        "notification.markRead" => arm!(handlers::mark_read(ctx, input)),
        "notification.unreadCount" => arm!(handlers::unread_count(ctx, input)),
        _ => None,
    }
}
