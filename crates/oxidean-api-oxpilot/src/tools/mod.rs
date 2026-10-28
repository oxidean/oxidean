//! U5 — `OxpilotTools`: the model-facing tool executor for /oxpilot
//! turns.
//!
//! Invocation ladder (BR1.3/BR1.4/BR2.1 — order is load-bearing):
//!   1. resolve `call.name` against the closed catalog ⇒ `unknown_tool`
//!   2. parse + validate `call.args_json` against the entry schema
//!      ⇒ `invalid_args`
//!   3. resolve the session row (`svc.get_session`) ⇒ `not_found`
//!   4. dispatch to the class/kind handler
//!
//! Read handlers reach backing procs only through `dispatch_proc`
//! (synthesized `RpcCtx` + `state.rpc_dispatch`, BR3.1); exec/fs
//! handlers reach targets only through `ConnectorGateway` (BR7.1).

pub mod comment;
pub mod dispatch;
pub mod procs;
pub mod propose;
pub mod read;
pub mod registry;
pub mod validate;

use std::sync::Arc;

use oxidean_api_core::app::AppState;
use oxidean_api_core::oxpilot::{
    SessionService, ToolCall, ToolCtx, ToolError, ToolExecutor, ToolOutput, ToolSpec,
};
use oxidean_api_core::rpc::ClientMeta;
use oxidean_core::{RpcRequest, RpcResponse};
use oxidean_db::OxpilotSessionRow;
use serde_json::{json, Value};

/// `ToolError` ships pub fields with no ctor — one constructor keeps the
/// code/message pairing consistent at every raise site.
pub(crate) fn tool_err(code: &str, message: impl Into<String>) -> ToolError {
    ToolError {
        code: code.to_string(),
        message: message.into(),
    }
}

/// U3-shipped ceilings (tools stay under them rather than relying on
/// the runtime clamp — BR9.4).
pub(crate) const TOOL_OUTPUT_MAX: usize = 32 * 1024;
pub(crate) const OUTPUT_SUMMARY_MAX: usize = 2 * 1024;
/// `propose_diff` bounds (BR4.4).
pub(crate) const DIFF_BODY_MAX: usize = 256 * 1024;
pub(crate) const MAX_PROPOSAL_FILES: usize = 50;
pub(crate) const TITLE_MAX: usize = 200;
/// `pr_comment` body bound (BR8.3).
pub(crate) const COMMENT_MAX: usize = 4 * 1024;
/// Gateway arg-string bound (BR7.2).
pub(crate) const ACTION_ARGS_MAX: usize = oxidean_api_core::oxpilot::ACTION_ARGS_MAX;
/// Apply branch namespace — `oxpilot/proposal-<id8>` (BR5.3).
pub(crate) const APPLY_BRANCH_PREFIX: &str = "oxpilot/proposal-";

/// The executor installed on the runtime slot at shell assembly.
/// Holds `Arc<AppState>` (the clone shares `db`, `oxpilot`, and the
/// aggregated `rpc_dispatch` handle installed before cloning).
pub struct OxpilotTools {
    pub(crate) state: Arc<AppState>,
}

impl OxpilotTools {
    pub fn new(state: AppState) -> Self {
        Self {
            state: Arc::new(state),
        }
    }

    /// `svc.get_session` is the live ownership + repo-binding +
    /// liveness check for every handler (BR2.1). Absent or foreign
    /// session ⇒ `not_found` — anti-enumeration, foreign and dead ids
    /// are indistinguishable.
    pub(crate) async fn resolve_session(
        &self,
        ctx: &ToolCtx,
    ) -> Result<(Arc<dyn SessionService>, OxpilotSessionRow), ToolError> {
        let svc = self
            .state
            .oxpilot
            .sessions()
            .map_err(|e| tool_err("internal", e.message))?;
        let session = svc
            .get_session(&ctx.user_id, &ctx.session_id)
            .await
            .map_err(|_| tool_err("not_found", "session not found"))?;
        Ok((svc, session))
    }

    /// Reach a backing proc through `state.rpc_dispatch` on a
    /// synthesized-session `RpcCtx` (BR3.1) — `build_rpc_ctx_with_session`
    /// with the ctx-bound user + `ClientMeta::default()` (no fabricated
    /// IP/UA). The proc's own authorization applies verbatim.
    pub(crate) async fn dispatch_proc(
        &self,
        ctx: &ToolCtx,
        procedure: &str,
        input: Value,
    ) -> Result<Value, ToolError> {
        let session = oxidean_api_core::auth::session::ResolvedSession {
            session_id: format!("oxpilot:{}", ctx.session_id),
            user_id: ctx.user_id.clone(),
            remember_me: false,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        };
        let mut rpc_ctx = oxidean_api_core::app::build_rpc_ctx_with_session(
            &self.state,
            Some(session),
            ClientMeta::default(),
        );
        let dispatch = rpc_ctx.rpc_dispatch.clone();
        let resp = dispatch(
            &mut rpc_ctx,
            RpcRequest {
                procedure: procedure.to_string(),
                input,
            },
        )
        .await;
        match resp {
            RpcResponse::Ok { data, .. } => Ok(data),
            RpcResponse::Err { error, .. } => Err(map_proc_error(&error)),
        }
    }

    /// Emit a live-turn card frame — `turns().get` + `emit_frame`
    /// (the queued-card precedent, BR4.5). Best-effort: a terminalled
    /// turn drops silently.
    pub(crate) async fn emit_card(&self, session_id: &str, card_kind: &str, ref_id: &str) {
        if let Some(turn) = self.state.oxpilot.turns().await.get(session_id).await {
            crate::runtime::emit_frame(
                &turn,
                json!({
                    "type": "card",
                    "card_kind": card_kind,
                    "ref_id": ref_id,
                }),
            )
            .await;
        }
    }

    /// Post-commit best-effort audit (BR9.1/BR9.3) — `record_with` is
    /// the ctx-free entry point; the actor falls back to `None` when
    /// the user lookup fails rather than skipping the event. `detail`
    /// carries ids/counts/refs only. `target` is `(target_type,
    /// target_id)` — the audit feed's join key.
    pub(crate) async fn audit(
        &self,
        ctx: &ToolCtx,
        event: &str,
        target: Option<(&str, &str)>,
        detail: Value,
    ) {
        let actor = match self.state.db.find_user_by_id(&ctx.user_id).await {
            Ok(Some(u)) => Some((u.id.clone(), u.username.clone())),
            _ => None,
        };
        oxidean_api_core::audit::record_with(
            &self.state.db,
            &ClientMeta::default(),
            actor
                .as_ref()
                .map(|(id, name)| (id.as_str(), name.as_str())),
            event,
            target,
            Some(detail.to_string()),
        )
        .await;
    }
}

/// AppError→ToolError map — only the PINNED set {not_found,
/// forbidden, invalid_args} passes through verbatim (BR3.4); every
/// other code — internal/unknown, rpc.*, auth.*, db.*, repo.*,
/// oxpilot.* — collapses to `internal` with a generic message so no
/// resource-leaking implementation detail reaches the model surface.
pub(crate) fn map_proc_error(e: &oxidean_core::AppError) -> ToolError {
    match e.code.as_str() {
        "not_found" | "forbidden" | "invalid_args" => {
            let mut msg = oxidean_core::redact(&e.message);
            truncate_charsafe(&mut msg, OUTPUT_SUMMARY_MAX);
            tool_err(e.code.as_str(), msg)
        }
        _ => tool_err("internal", "internal error"),
    }
}

/// Char-boundary-safe truncation — `String::truncate` at a byte bound
/// panics mid-codepoint (NFR4.x "never a panic"), so the cut walks
/// back to the last char boundary instead.
pub(crate) fn truncate_charsafe(s: &mut String, max: usize) {
    if s.len() <= max {
        return;
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s.truncate(cut);
}

/// Bound a handler's serialized payload under TOOL_OUTPUT_MAX (BR9.4)
/// with an explicit `truncated` marker, plus the ≤2 KiB redacted
/// `output_summary`.
pub(crate) fn bound_output(content: Value) -> ToolOutput {
    const TRUNC_MARKER: &str = "\n[output truncated]";
    let mut text = serde_json::to_string(&content).unwrap_or_else(|_| "{}".to_string());
    let truncated = text.len() > TOOL_OUTPUT_MAX;
    if truncated {
        // Char-safe cut reserving room for the in-band marker BR9.4
        // pins — silently-corrupt JSON must never reach the model.
        truncate_charsafe(&mut text, TOOL_OUTPUT_MAX - TRUNC_MARKER.len());
        text.push_str(TRUNC_MARKER);
    }
    let summary = if truncated {
        "output truncated".to_string()
    } else {
        let mut s = oxidean_core::redact(&text);
        truncate_charsafe(&mut s, OUTPUT_SUMMARY_MAX);
        s
    };
    ToolOutput {
        content: text,
        output_summary: summary,
    }
}

#[async_trait::async_trait]
impl ToolExecutor for OxpilotTools {
    fn catalog(&self) -> Vec<ToolSpec> {
        registry::catalog()
    }

    async fn invoke(&self, call: ToolCall, ctx: ToolCtx) -> Result<ToolOutput, ToolError> {
        // 1. catalog resolution — absent names fail closed (BR1.3):
        // no schema validation, no handler, no dispatch, no audit.
        let entry = registry::find(&call.name)
            .ok_or_else(|| tool_err("unknown_tool", format!("unknown tool: {}", call.name)))?;

        // 2. JSON parse + schema validation BEFORE any handler sees
        // the args (BR1.4) — identity fields can never rebind
        // (`additionalProperties: false` everywhere, BR2.3).
        let args: Value = serde_json::from_str(&call.args_json)
            .map_err(|_| tool_err("invalid_args", "args_json: malformed JSON"))?;
        validate::validate(&(entry.parameters_json)(), &args)
            .map_err(|e| tool_err("invalid_args", e))?;

        // 3. session re-resolution — ownership/binding/liveness (BR2.1).
        let (_svc, session) = self.resolve_session(&ctx).await?;

        // 4. dispatch by kind — class is fixed catalog data (BR2.4).
        match entry.kind {
            registry::EntryKind::Proc(proc_name) => {
                read::handle(self, &ctx, proc_name, entry.name, &args, &session).await
            }
            registry::EntryKind::ProposalGet => {
                propose::proposal_get(self, &ctx, &args, &session).await
            }
            registry::EntryKind::ProposeDiff => {
                propose::propose_diff(self, &ctx, &args, &session).await
            }
            registry::EntryKind::TargetsList => dispatch::targets_list(self, &ctx).await,
            registry::EntryKind::ActionStatus => dispatch::action_status(self, &ctx, &args).await,
            registry::EntryKind::Action => {
                dispatch::dispatch_action(self, &ctx, entry.name, &args, &session).await
            }
            registry::EntryKind::PrComment => {
                comment::pr_comment(self, &ctx, &args, &session).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_error_codes_map_verbatim() {
        let e = oxidean_core::AppError::new("not_found", "no such issue");
        let t = map_proc_error(&e);
        assert_eq!(t.code, "not_found");
        assert_eq!(t.message, "no such issue");
        let e = oxidean_core::AppError::new("forbidden", "denied");
        assert_eq!(map_proc_error(&e).code, "forbidden");
        let e = oxidean_core::AppError::new("invalid_args", "path required");
        assert_eq!(map_proc_error(&e).code, "invalid_args");
    }

    #[test]
    fn internal_codes_collapse() {
        // The pinned verbatim set is exactly three — every other code,
        // including domain-shaped ones (repo.*, auth.*, rpc.bad_input,
        // oxpilot.*), collapses so no resource detail reaches the model.
        for code in [
            "internal",
            "rpc.internal",
            "rpc.bad_input",
            "db.query_failed",
            "repo.git_failed",
            "auth.unauthenticated",
            "oxpilot.action_denied",
            "diff_not_pending",
            "apply_failed:repo.branchCreate",
            "",
        ] {
            let e = oxidean_core::AppError::new(code, "secret detail");
            let t = map_proc_error(&e);
            assert_eq!(t.code, "internal", "{code} must collapse");
            assert_eq!(t.message, "internal error");
        }
    }

    #[test]
    fn bound_output_truncates_with_marker() {
        let big = json!({"data": "x".repeat(TOOL_OUTPUT_MAX + 100)});
        let out = bound_output(big);
        assert!(out.content.len() <= TOOL_OUTPUT_MAX);
        assert_eq!(out.output_summary, "output truncated");
    }
}
