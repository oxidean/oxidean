//! RPC composition layer — re-exports `oxidean-api-core`'s RPC machinery and
//! overrides `dispatch`/`PROCEDURES` to fan out across the per-domain crates
//! (issue #162). Entry points (HTTP, WS, MCP, REST) all call
//! `crate::rpc::dispatch`; the name resolution lands here because a local
//! item shadows the glob re-export.

pub use oxidean_api_core::rpc::*;

use oxidean_core::{RpcRequest, RpcResponse};
use std::sync::LazyLock;

/// Every procedure name the assembled server answers — the
/// `system.manifest` source and the `rest/spec` coverage oracle. Aggregated
/// across `oxidean-api-core`'s residual surface plus every domain crate.
pub static PROCEDURES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut v: Vec<&'static str> = Vec::new();
    v.extend_from_slice(oxidean_api_core::rpc::PROCEDURES);
    v.extend_from_slice(oxidean_api_actions::PROCEDURES);
    v.extend_from_slice(oxidean_api_admin::PROCEDURES);
    v.extend_from_slice(oxidean_api_auth::PROCEDURES);
    v.extend_from_slice(oxidean_api_discussion::PROCEDURES);
    v.extend_from_slice(oxidean_api_project::PROCEDURES);
    v.extend_from_slice(oxidean_api_check::PROCEDURES);
    v.extend_from_slice(oxidean_api_webhook::PROCEDURES);
    v.extend_from_slice(oxidean_api_user::PROCEDURES);
    v.extend_from_slice(oxidean_api_oauth::PROCEDURES);
    v.extend_from_slice(oxidean_api_packages::PROCEDURES);
    v.extend_from_slice(oxidean_api_mirror::PROCEDURES);
    v.extend_from_slice(oxidean_api_pat::PROCEDURES);
    v.extend_from_slice(oxidean_api_merge_queue::PROCEDURES);
    v.extend_from_slice(oxidean_api_pages::PROCEDURES);
    v.extend_from_slice(oxidean_api_release::PROCEDURES);
    v.extend_from_slice(oxidean_api_keys::PROCEDURES);
    v.extend_from_slice(oxidean_api_email::PROCEDURES);
    v.extend_from_slice(oxidean_api_wiki::PROCEDURES);
    v.extend_from_slice(oxidean_api_deployment::PROCEDURES);
    v.extend_from_slice(oxidean_api_notification::PROCEDURES);
    v.extend_from_slice(oxidean_api_vcs::PROCEDURES);
    v.extend_from_slice(oxidean_api_issues::PROCEDURES);
    v.extend_from_slice(oxidean_api_org::PROCEDURES);
    v.extend_from_slice(oxidean_api_pulls::PROCEDURES);
    v.extend_from_slice(oxidean_api_repo::PROCEDURES);
    v.sort_unstable();
    v.dedup();
    v
});

/// Gates run exactly once (`gate` owns bootstrap + PAT authorize), then each
/// domain dispatcher gets first claim on the name; unclaimed names fall back
/// to core's residual match, which owns `system.*` + unextracted domains and
/// answers `rpc.method_not_found` for anything left over.
pub async fn dispatch(ctx: &mut RpcCtx, req: RpcRequest) -> RpcResponse {
    if let Err(resp) = oxidean_api_core::rpc::gate(ctx, &req).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_actions::dispatch(ctx, &req.procedure, req.input.clone()).await
    {
        return resp;
    }
    if let Some(resp) = oxidean_api_admin::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_auth::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) =
        oxidean_api_discussion::dispatch(ctx, &req.procedure, req.input.clone()).await
    {
        return resp;
    }
    if let Some(resp) = oxidean_api_project::dispatch(ctx, &req.procedure, req.input.clone()).await
    {
        return resp;
    }
    if let Some(resp) = oxidean_api_check::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_webhook::dispatch(ctx, &req.procedure, req.input.clone()).await
    {
        return resp;
    }
    if let Some(resp) = oxidean_api_user::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_oauth::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_packages::dispatch(ctx, &req.procedure, req.input.clone()).await
    {
        return resp;
    }
    if let Some(resp) = oxidean_api_mirror::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_pat::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) =
        oxidean_api_merge_queue::dispatch(ctx, &req.procedure, req.input.clone()).await
    {
        return resp;
    }
    if let Some(resp) = oxidean_api_pages::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_release::dispatch(ctx, &req.procedure, req.input.clone()).await
    {
        return resp;
    }
    if let Some(resp) = oxidean_api_keys::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_email::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_wiki::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) =
        oxidean_api_deployment::dispatch(ctx, &req.procedure, req.input.clone()).await
    {
        return resp;
    }
    if let Some(resp) =
        oxidean_api_notification::dispatch(ctx, &req.procedure, req.input.clone()).await
    {
        return resp;
    }
    if let Some(resp) = oxidean_api_vcs::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_issues::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_org::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_pulls::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    if let Some(resp) = oxidean_api_repo::dispatch(ctx, &req.procedure, req.input.clone()).await {
        return resp;
    }
    oxidean_api_core::rpc::dispatch_ungated(ctx, req, &PROCEDURES).await
}
