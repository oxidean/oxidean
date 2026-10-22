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
        "discussions.list" => arm!(handlers::list(ctx, input)),
        "discussions.get" => arm!(handlers::get(ctx, input)),
        "discussions.create" => arm!(handlers::create(ctx, input)),
        "discussions.update" => arm!(handlers::update(ctx, input)),
        "discussions.delete" => arm!(handlers::delete(ctx, input)),
        "discussions.pin" => arm!(handlers::pin(ctx, input)),
        "discussions.lock" => arm!(handlers::lock(ctx, input)),
        "discussions.transfer" => arm!(handlers::transfer(ctx, input)),
        "discussions.labels.set" => arm!(handlers::labels_set(ctx, input)),
        "discussions.answer.set" => arm!(handlers::answer_set(ctx, input)),
        "discussions.answer.clear" => arm!(handlers::answer_clear(ctx, input)),
        "discussions.categories.list" => arm!(handlers::categories_list(ctx, input)),
        "discussions.categories.create" => arm!(handlers::categories_create(ctx, input)),
        "discussions.categories.update" => arm!(handlers::categories_update(ctx, input)),
        "discussions.categories.delete" => arm!(handlers::categories_delete(ctx, input)),
        "discussions.comments.list" => arm!(handlers::comments_list(ctx, input)),
        "discussions.comments.create" => arm!(handlers::comments_create(ctx, input)),
        "discussions.comments.update" => arm!(handlers::comments_update(ctx, input)),
        "discussions.comments.delete" => arm!(handlers::comments_delete(ctx, input)),
        _ => None,
    }
}
