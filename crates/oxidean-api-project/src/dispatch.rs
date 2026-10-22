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
        "projects.list" => arm!(handlers::list(ctx, input)),
        "projects.get" => arm!(handlers::get(ctx, input)),
        "projects.create" => arm!(handlers::create(ctx, input)),
        "projects.update" => arm!(handlers::update(ctx, input)),
        "projects.delete" => arm!(handlers::delete(ctx, input)),
        "projects.columns.list" => arm!(handlers::columns_list(ctx, input)),
        "projects.columns.create" => arm!(handlers::columns_create(ctx, input)),
        "projects.columns.update" => arm!(handlers::columns_update(ctx, input)),
        "projects.columns.move" => arm!(handlers::columns_move(ctx, input)),
        "projects.columns.delete" => arm!(handlers::columns_delete(ctx, input)),
        "projects.cards.list" => arm!(handlers::cards_list(ctx, input)),
        "projects.cards.create" => arm!(handlers::cards_create(ctx, input)),
        "projects.cards.update" => arm!(handlers::cards_update(ctx, input)),
        "projects.cards.move" => arm!(handlers::cards_move(ctx, input)),
        "projects.cards.convert" => arm!(handlers::cards_convert(ctx, input)),
        "projects.cards.delete" => arm!(handlers::cards_delete(ctx, input)),
        _ => None,
    }
}
