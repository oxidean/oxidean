//! Per-domain dispatch — the shell tries each domain's `dispatch` before
//! falling through to `oxidean-api-core`'s residual match. Returns `None`
//! when `procedure` is not one of this crate's names so the next dispatcher
//! can try it.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::RpcResponse;
use serde_json::Value;

use crate::{connector, session, settings, status, tools};

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
        "oxpilot.status" => arm!(status::status(ctx)),
        "oxpilot.session_create" => arm!(session::create(ctx, input)),
        "oxpilot.session_list" => arm!(session::list(ctx)),
        // U2 session-core surface — the C5 `SessionService` seam backs
        // every verb; `require_enabled` runs first inside each handler.
        "oxpilot.session_get" => arm!(session::get(ctx, input)),
        "oxpilot.session_delete" => arm!(session::delete(ctx, input)),
        "oxpilot.message_queue" => arm!(session::message_queue(ctx, input)),
        "oxpilot.queue_list" => arm!(session::queue_list(ctx, input)),
        "oxpilot.usage_get" => arm!(session::usage_get(ctx)),
        // U3 — C1's previously-orphaned proc: abort the live turn on an
        // owned session; `{aborted:bool}` reports the CAS outcome.
        "oxpilot.turn_cancel" => arm!(session::turn_cancel(ctx, input)),
        // U6 connector-gateway owner surface — the `ConnectorGateway`
        // seam backs every verb; owner scoping + `not_found`
        // anti-enumeration live inside it (BR7.1).
        "oxpilot.pairing_create" => arm!(connector::procs::pairing_create(ctx, input)),
        "oxpilot.target_list" => arm!(connector::procs::target_list(ctx)),
        "oxpilot.target_get" => arm!(connector::procs::target_get(ctx, input)),
        "oxpilot.target_disconnect" => arm!(connector::procs::target_disconnect(ctx, input)),
        "oxpilot.target_revoke" => arm!(connector::procs::target_revoke(ctx, input)),
        "oxpilot.grant_list" => arm!(connector::procs::grant_list(ctx, input)),
        "oxpilot.grant_get" => arm!(connector::procs::grant_get(ctx, input)),
        "oxpilot.grant_respond" => arm!(connector::procs::grant_respond(ctx, input)),
        // U4 admin surface — SessionOnly at the proc gate (PAT denies
        // upstream); `require_admin` inside each handler (BR1.1/BR5.1).
        "admin.oxpilot.settings_get" => arm!(settings::settings_get(ctx)),
        "admin.oxpilot.settings_set" => arm!(settings::settings_set(ctx, input)),
        "admin.oxpilot.providers_list" => arm!(settings::providers_list(ctx)),
        // U5 diff surface — `diff_apply` takes `&mut RpcCtx` because the
        // materialization chain rides the caller's own `rpc_dispatch`
        // (BR5.3); the arm macro passes `ctx` straight through.
        "oxpilot.diff_list" => arm!(tools::procs::diff_list(ctx, input)),
        "oxpilot.diff_get" => arm!(tools::procs::diff_get(ctx, input)),
        "oxpilot.diff_apply" => arm!(tools::procs::diff_apply(ctx, input)),
        _ => None,
    }
}
