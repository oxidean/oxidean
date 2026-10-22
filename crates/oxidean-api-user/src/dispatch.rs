//! Per-domain dispatch — see `oxidean-api-issues` for the contract. `None`
//! means the name is not this crate's; the shell tries the next dispatcher.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::RpcResponse;
use serde_json::Value;

use crate::{follows, list_starred, lookup, profile};

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
        "user.get_profile" => arm!(profile::get_profile(ctx)),
        "user.getPublicProfile" => arm!(profile::get_public_profile(ctx, input)),
        "user.update_profile" => arm!(profile::update_profile(ctx, input)),
        "user.lookup" => arm!(lookup::lookup(ctx, input)),
        "user.listStarred" => arm!(list_starred::list_starred(ctx, input)),
        "user.listWatched" => arm!(follows::list_watched(ctx, input)),
        "user.follow" => arm!(follows::follow(ctx, input)),
        "user.unfollow" => arm!(follows::unfollow(ctx, input)),
        "user.followers.list" => arm!(follows::followers_list(ctx, input)),
        "user.following.list" => arm!(follows::following_list(ctx, input)),
        _ => None,
    }
}
