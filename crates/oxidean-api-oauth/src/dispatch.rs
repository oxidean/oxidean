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
        "oauthApp.create" => arm!(handlers::create(ctx, input)),
        "oauthApp.list" => arm!(handlers::list(ctx)),
        "oauthApp.update" => arm!(handlers::update(ctx, input)),
        "oauthApp.delete" => arm!(handlers::delete_app(ctx, input)),
        "oauthApp.regenerateSecret" => arm!(handlers::regenerate_secret(ctx, input)),
        "oauthApp.listGrants" => arm!(handlers::list_grants(ctx)),
        "oauthApp.revoke" => arm!(handlers::revoke_grant(ctx, input)),
        "oauthApp.authorizeInfo" => arm!(handlers::authorize_info(ctx, input)),
        "oauthApp.authorize" => arm!(handlers::authorize_rpc(ctx, input)),
        "oauth.deviceInfo" => arm!(handlers::device_info(ctx, input)),
        "oauth.deviceAuthorize" => arm!(handlers::device_authorize(ctx, input)),
        _ => None,
    }
}
