//! Per-domain dispatch — see `oxidean-api-issues` for the contract.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::RpcResponse;
use serde_json::Value;

use crate::{deploy_keys, gpg_keys, ssh_keys};

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
        "gpgKey.add" => arm!(gpg_keys::add(ctx, input)),
        "gpgKey.list" => arm!(gpg_keys::list(ctx)),
        "gpgKey.revoke" => arm!(gpg_keys::revoke(ctx, input)),
        "repo.deployKey.create" => arm!(deploy_keys::create(ctx, input)),
        "repo.deployKey.delete" => arm!(deploy_keys::delete(ctx, input)),
        "repo.deployKey.list" => arm!(deploy_keys::list(ctx, input)),
        "sshKey.add" => arm!(ssh_keys::add(ctx, input)),
        "sshKey.list" => arm!(ssh_keys::list(ctx)),
        "sshKey.revoke" => arm!(ssh_keys::revoke(ctx, input)),
        _ => None,
    }
}
