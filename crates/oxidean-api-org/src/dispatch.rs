//! Per-domain dispatch — see `oxidean-api-issues` for the contract. `None`
//! means the name is not this crate's; the shell tries the next dispatcher.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::RpcResponse;
use serde_json::Value;

use crate::{handlers, invites, members, runners, secrets, vars};

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
        "org.actions.runners.createRegistrationToken" => {
            arm!(runners::create_registration_token(ctx, input))
        }
        "org.actions.runners.createRemoveToken" => arm!(runners::create_remove_token(ctx, input)),
        "org.actions.runners.delete" => arm!(runners::delete(ctx, input)),
        "org.actions.runners.get" => arm!(runners::get(ctx, input)),
        "org.actions.runners.list" => arm!(runners::list(ctx, input)),
        "org.actions.secrets.delete" => arm!(secrets::delete(ctx, input)),
        "org.actions.secrets.list" => arm!(secrets::list(ctx, input)),
        "org.actions.secrets.listRepos" => arm!(secrets::list_repos(ctx, input)),
        "org.actions.secrets.publicKey" => arm!(secrets::public_key(ctx, input)),
        "org.actions.secrets.put" => arm!(secrets::put(ctx, input)),
        "org.actions.secrets.setRepos" => arm!(secrets::set_repos(ctx, input)),
        "org.actions.variables.delete" => arm!(vars::delete(ctx, input)),
        "org.actions.variables.list" => arm!(vars::list(ctx, input)),
        "org.actions.variables.listRepos" => arm!(vars::list_repos(ctx, input)),
        "org.actions.variables.put" => arm!(vars::put(ctx, input)),
        "org.actions.variables.setRepos" => arm!(vars::set_repos(ctx, input)),
        "org.create" => arm!(handlers::create(ctx, input)),
        "org.get" => arm!(handlers::get(ctx, input)),
        "org.invites.accept" => arm!(invites::accept(ctx, input)),
        "org.invites.create" => arm!(invites::create(ctx, input)),
        "org.invites.createLink" => arm!(invites::create_link(ctx, input)),
        "org.invites.list" => arm!(invites::list(ctx, input)),
        "org.invites.revoke" => arm!(invites::revoke(ctx, input)),
        "org.listMine" => arm!(handlers::list_mine(ctx)),
        "org.members.add" => arm!(members::add(ctx, input)),
        "org.members.list" => arm!(members::list(ctx, input)),
        "org.members.remove" => arm!(members::remove(ctx, input)),
        "org.members.updateRole" => arm!(members::update_role(ctx, input)),
        "org.updateSettings" => arm!(handlers::update_settings(ctx, input)),
        _ => None,
    }
}
