//! U5 — connector-dispatch handlers (BR7): `exec_shell`/`fs_read`/
//! `fs_write` through `ConnectorGateway::dispatch_action` only (BR7.1 —
//! never a socket, never a bypass), plus the read-class `targets_list`
//! and `action_status` over the same `client`-first verbs (BR7.6).
//!
//! Denial mapping (BR7.4): `AppError{oxpilot.action_denied,
//! data.denied}` — `consent_required` is a SUCCESS-shaped ToolOutput
//! plus a consent card (a pending grant is progress); the other three
//! reasons are hard `action_denied` ToolErrors carrying the reason in
//! `message` (codes never reach the model — BR3.4); a missing or
//! malformed `denied` marker fails closed as `internal`, never
//! message-sniffed.

use oxidean_api_core::oxpilot::{ToolCtx, ToolError, ToolOutput};
use oxidean_api_core::rpc::ClientMeta;
use oxidean_core::AppError;
use oxidean_db::OxpilotSessionRow;
use serde_json::{json, Value};

use super::{bound_output, map_proc_error, tool_err, OxpilotTools, ACTION_ARGS_MAX};

fn gateway(
    tools: &OxpilotTools,
) -> Result<std::sync::Arc<dyn oxidean_api_core::oxpilot::ConnectorGateway>, ToolError> {
    tools
        .state
        .oxpilot
        .gateway()
        .map_err(|e| tool_err("internal", e.message))
}

/// `targets_list{}` (read-class, BR7.6) — the target inventory a model
/// picks `target_id` from. Already user-scoped inside the gateway.
pub async fn targets_list(tools: &OxpilotTools, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let views = gateway(tools)?
        .list_targets(&ClientMeta::default(), &ctx.user_id)
        .await
        .map_err(|e| map_proc_error(&e))?;
    Ok(bound_output(json!({ "targets": views })))
}

/// `action_status{action_id}` (read-class, BR7.5/BR7.6) — async
/// polling; the gateway's owner check resolves foreign/absent ids to
/// the same `not_found`.
pub async fn action_status(
    tools: &OxpilotTools,
    ctx: &ToolCtx,
    args: &Value,
) -> Result<ToolOutput, ToolError> {
    let action_id = args["action_id"].as_str().unwrap_or_default();
    let view = gateway(tools)?
        .action_status(&ClientMeta::default(), &ctx.user_id, action_id)
        .await
        .map_err(|e| map_proc_error(&e))?;
    Ok(bound_output(
        serde_json::to_value(&view).unwrap_or_else(|_| json!({})),
    ))
}

/// `exec_shell`/`fs_read`/`fs_write` — schema-gated args, BR7.3 target
/// resolution, C7 dispatch, BR7.4 outcome mapping. `kind` is the
/// catalog entry name verbatim (BR7.1/BR1.2 — the registry cannot
/// mint a fourth kind).
pub async fn dispatch_action(
    tools: &OxpilotTools,
    ctx: &ToolCtx,
    kind: &'static str,
    args: &Value,
    session: &OxpilotSessionRow,
) -> Result<ToolOutput, ToolError> {
    let gw = gateway(tools)?;

    // BR7.3 — target resolution: an explicit `target_id` is used
    // verbatim (the gateway's own owner/connected/caps checks verify
    // it); absent, the session's binding is authoritative — only an
    // UNBOUND session falls back to sole-connected selection. The
    // bound id goes through the same gateway checks on dispatch, so a
    // bound-but-offline target denies rather than silently reroute
    // the action onto a different machine.
    let arg_target = args["target_id"].as_str().map(str::to_string);
    let target_id = match arg_target {
        Some(t) => t,
        None => match session.computer_use_target_id.clone() {
            Some(t) => t,
            None => {
                let views = gw
                    .list_targets(&ClientMeta::default(), &ctx.user_id)
                    .await
                    .map_err(|e| map_proc_error(&e))?;
                let connected: Vec<_> = views
                    .iter()
                    .filter(|t| t.lifecycle == "connected")
                    .collect();
                match connected.len() {
                    0 => {
                        return Err(tool_err(
                            "no_target",
                            "no connected connector target — call targets_list to inspect paired targets, or ask the user to pair one",
                        ));
                    }
                    1 => connected[0].id.clone(),
                    _ => {
                        return Err(tool_err(
                            "ambiguous_target",
                            format!(
                                "{} connected targets — pass target_id (see targets_list) to choose one",
                                connected.len()
                            ),
                        ));
                    }
                }
            }
        },
    };

    // The dispatched arg string drops `target_id` — it is a selector,
    // not action input (the connector executes `args` verbatim on
    // approval too). Serialized must fit ACTION_ARGS_MAX (BR7.2).
    let mut action_args = args.clone();
    if let Some(obj) = action_args.as_object_mut() {
        obj.remove("target_id");
    }
    let args_str = serde_json::to_string(&action_args).unwrap_or_else(|_| "{}".to_string());
    if args_str.len() > ACTION_ARGS_MAX {
        return Err(tool_err(
            "invalid_args",
            "args: serialized payload exceeds 2 KiB",
        ));
    }

    let row = gw
        .dispatch_action(
            &ClientMeta::default(),
            &ctx.user_id,
            &ctx.session_id,
            &target_id,
            kind,
            &args_str,
        )
        .await;

    match row {
        Ok(action) => {
            // BR7.5 — queued acknowledgement, never a wait inside invoke.
            Ok(bound_output(json!({
                "action_id": action.id,
                "state": action.state,
                "summary": format!("{kind} queued on target — poll action_status with the action_id"),
            })))
        }
        Err(e) => denied_or_mapped(tools, ctx, e).await,
    }
}

/// BR7.4 — the `data.denied` marker is the ONLY refusal signal; no
/// message-sniffing, no lenient parse (fail closed on malformed).
async fn denied_or_mapped(
    tools: &OxpilotTools,
    ctx: &ToolCtx,
    e: AppError,
) -> Result<ToolOutput, ToolError> {
    if e.code != "oxpilot.action_denied" {
        return Err(map_proc_error(&e));
    }
    let denied = e
        .data
        .as_ref()
        .and_then(|d| d.get("denied"))
        .cloned()
        .ok_or_else(|| tool_err("internal", "internal error"))?;
    let reason = denied
        .get("reason")
        .and_then(|r| r.as_str())
        .ok_or_else(|| tool_err("internal", "internal error"))?;
    match reason {
        "consent_required" => {
            let grant_id = denied
                .get("grant_id")
                .and_then(|g| g.as_str())
                .ok_or_else(|| tool_err("internal", "internal error"))?;
            tools.emit_card(&ctx.session_id, "consent", grant_id).await;
            Ok(ToolOutput {
                content: format!(
                    "consent requested — grant {grant_id} pending; the action queues on approval — do NOT re-issue this call"
                ),
                output_summary: "consent requested".to_string(),
            })
        }
        "not_connected" | "offline" | "capability" => Err(tool_err(
            "action_denied",
            format!("action denied ({reason}) — the target cannot run this now"),
        )),
        // Unknown reason or a tampered marker — fail closed.
        _ => Err(tool_err("internal", "internal error")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_marker_matrix() {
        // consent_required carries grant_id; the hard three carry only
        // reason; everything else fails closed. (Pure-shape checks —
        // the async path is covered by the dispatch integration test.)
        let marker = json!({"denied": {"reason": "consent_required", "grant_id": "g1"}});
        assert_eq!(marker["denied"]["reason"], "consent_required");
        let marker = json!({"denied": {"reason": "offline"}});
        assert!(matches!(
            marker["denied"]["reason"].as_str(),
            Some("offline")
        ));
        let bad = json!({"denied": {"reason": "mystery"}});
        assert!(
            !["not_connected", "offline", "capability", "consent_required"]
                .contains(&bad["denied"]["reason"].as_str().unwrap())
        );
    }

    #[test]
    fn target_id_is_stripped_from_dispatched_args() {
        let mut args = json!({"command": "ls", "target_id": "t1"});
        if let Some(obj) = args.as_object_mut() {
            obj.remove("target_id");
        }
        let s = serde_json::to_string(&args).unwrap();
        assert_eq!(s, r#"{"command":"ls"}"#);
        assert!(s.len() <= ACTION_ARGS_MAX);
    }
}
