//! Per-domain dispatch — see `oxidean-api-issues` for the contract. `None`
//! means the name is not this crate's; the shell tries the next dispatcher.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::RpcResponse;
use serde_json::Value;

use crate::{invites, settings, users};

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
        "admin.auth.get_settings" => arm!(settings::get_settings(ctx)),
        "admin.auth.update_settings" => arm!(settings::update_settings(ctx, input)),
        "admin.git.getSettings" => arm!(settings::git_get_settings(ctx)),
        "admin.git.updateSettings" => arm!(settings::git_update_settings(ctx, input)),
        "admin.instance.factory_reset" => arm!(settings::factory_reset(ctx, input)),
        "admin.invites.create" => arm!(invites::create(ctx, input)),
        "admin.invites.createLink" => arm!(invites::create_link(ctx, input)),
        "admin.invites.list" => arm!(invites::list(ctx)),
        "admin.invites.revoke" => arm!(invites::revoke(ctx, input)),
        "admin.lfs.getSettings" => arm!(settings::lfs_get_settings(ctx)),
        "admin.lfs.getUsage" => arm!(settings::lfs_get_usage(ctx)),
        "admin.lfs.updateSettings" => arm!(settings::lfs_update_settings(ctx, input)),
        "admin.mcp.getSettings" => arm!(settings::mcp_get_settings(ctx)),
        "admin.mcp.updateSettings" => arm!(settings::mcp_update_settings(ctx, input)),
        "admin.pat.getSettings" => arm!(settings::pat_get_settings(ctx)),
        "admin.pat.updateSettings" => arm!(settings::pat_update_settings(ctx, input)),
        "admin.repos.gc" => arm!(settings::repo_gc(ctx, input)),
        "admin.templates.delete" => arm!(oxidean_api_core::templates::handlers::admin_delete(
            ctx, input
        )),
        "admin.templates.list" => arm!(oxidean_api_core::templates::handlers::admin_list(ctx)),
        "admin.templates.setEnabled" => arm!(
            oxidean_api_core::templates::handlers::admin_set_enabled(ctx, input)
        ),
        "admin.templates.update" => arm!(oxidean_api_core::templates::handlers::admin_update(
            ctx, input
        )),
        "admin.users.ban" => arm!(users::ban(ctx, input)),
        "admin.users.delete" => arm!(users::delete(ctx, input)),
        "admin.users.getAccess" => arm!(users::get_access(ctx, input)),
        "admin.users.getActivity" => arm!(users::get_activity(ctx, input)),
        "admin.users.list" => arm!(users::list(ctx, input)),
        "admin.users.listSessions" => arm!(users::list_sessions(ctx, input)),
        "admin.users.revokeSessions" => arm!(users::revoke_sessions(ctx, input)),
        "admin.users.unban" => arm!(users::unban(ctx, input)),
        "admin.users.updateRole" => arm!(users::update_role(ctx, input)),
        _ => None,
    }
}
