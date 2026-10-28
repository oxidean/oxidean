//! `OxpilotSessionService` — the concrete C5 `SessionService` port (U2).
//!
//! Every verb is `user_id`-scoped: session-scoped work opens with an
//! ownership check and cross-user/missing resolves `not_found` —
//! identical to an absent row (BR1.1/BR3.3 anti-enumeration). Every
//! body-bearing write secrets-redacts at this boundary (BR3.1); the row
//! structs pass through unchanged on reads — no DTO copy layer.
//!
//! C8 delete ordering: registered `SessionDeleteHook`s run BEFORE the
//! cascade, in registration order (U3 abort-live-turns first, U6
//! revoke-grants second); a hook failure aborts the delete and the
//! `hook_index` rides the `AppError.data` so the proc can audit it.
//!
//! Installed once at shell assembly (`OxpilotRuntime::install_sessions`)
//! so U3/U6/U8 consume the seam without a crate cycle.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use oxidean_api_core::oxpilot::{
    AppendMessage, BudgetCaps, CreateSession, Enqueue, ProposalWrite, QueuedReceipt,
    SessionDeleteHook, SessionService, ToolCallStatusUpdate, ToolCallWrite, UsageDelta, UsageView,
};
use oxidean_core::AppError;
use oxidean_db::{
    Database, NewOxpilotMessage, NewOxpilotProposal, NewOxpilotQueued, NewOxpilotSession,
    NewOxpilotToolCall, OxpilotMessageRow, OxpilotProposalRow, OxpilotQueuedRow, OxpilotSessionRow,
    OxpilotToolCallRow, OxpilotUsageCounterRow,
};
use uuid::Uuid;

use crate::settings::EffectiveProviderConfig;

/// `From<&EffectiveProviderConfig> for BudgetCaps` — the C5 amendment:
/// budget values live on U4's resolved config; every slot-constructor
/// path stamps them through this conversion so `check_budget` callers
/// read `slot.budget_caps` and never reach back into the config.
impl From<&EffectiveProviderConfig> for BudgetCaps {
    fn from(cfg: &EffectiveProviderConfig) -> Self {
        BudgetCaps {
            user: cfg.user_budget,
            instance: cfg.instance_budget,
        }
    }
}

/// Bounded queue-mint attempts (BR4.1) — a `(session_id, queue_seq)`
/// collision under a concurrent enqueue re-reads `MAX+1`; eight tries is
/// far past any realistic collision depth and still bounded.
const ENQUEUE_MAX_ATTEMPTS: usize = 8;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!(error = %e, "oxpilot session service db error");
        AppError::new("oxpilot.internal", "session operation failed")
    }
}

/// Session-scoped anti-enumeration error — absent and foreign resolve
/// identically (BR3.3).
fn not_found() -> AppError {
    AppError::new("not_found", "session not found")
}

/// Dialect-tolerant unique-violation sniff — the db layer surfaces sqlx
/// errors as strings; a constraint trip on `(session_id, client_msg_id)`
/// or `(session_id, queue_seq)` is the only enqueue outcome that re-reads
/// instead of failing.
fn is_unique_violation(e: &str) -> bool {
    e.contains("UNIQUE constraint failed")
        || e.contains("duplicate key value")
        || e.contains("Duplicate entry")
}

/// The concrete session-core service — stateless over `Database` plus
/// the C8 hook registry. Hooks register behind a `Mutex<Vec>` so
/// registration order IS call order, and a registrar can attach at
/// install time without `&mut` access to the Arc'd service.
pub struct OxpilotSessionService {
    db: Database,
    hooks: Mutex<Vec<Arc<dyn SessionDeleteHook>>>,
}

impl OxpilotSessionService {
    pub fn new(db: Database) -> Self {
        OxpilotSessionService {
            db,
            hooks: Mutex::new(Vec::new()),
        }
    }

    /// C8 — attach a delete hook; hooks run in registration order.
    /// Idempotent-registration is the registrar's business (the C8
    /// contract makes the HOOK idempotent, not the registry).
    pub fn register_delete_hook(&self, hook: Arc<dyn SessionDeleteHook>) {
        self.hooks
            .lock()
            .expect("oxpilot hook registry poisoned")
            .push(hook);
    }

    /// The ownership gate — every session-scoped verb opens here.
    async fn owned(&self, session_id: &str, user_id: &str) -> Result<OxpilotSessionRow, AppError> {
        match self
            .db
            .find_owned_oxpilot_session(session_id, user_id)
            .await
            .map_err(db_err)?
        {
            Some(row) if row.status != "deleted" => Ok(row),
            _ => Err(not_found()),
        }
    }

    /// Ownership via a message row — message-scoped verbs fetch the row
    /// then validate its `session_id` against the caller (fetch-then-check,
    /// same anti-enumeration shape as `owned`).
    async fn owned_message(
        &self,
        message_id: &str,
        user_id: &str,
    ) -> Result<OxpilotMessageRow, AppError> {
        let row = self
            .db
            .find_oxpilot_message_by_id(message_id)
            .await
            .map_err(db_err)?
            .ok_or_else(not_found)?;
        self.owned(&row.session_id, user_id).await?;
        Ok(row)
    }

    /// Ownership via a proposal row — same fetch-then-check shape;
    /// foreign proposals resolve `not_found` exactly like absent ones.
    async fn owned_proposal(
        &self,
        proposal_id: &str,
        user_id: &str,
    ) -> Result<OxpilotProposalRow, AppError> {
        let row = self
            .db
            .find_oxpilot_proposal(proposal_id)
            .await
            .map_err(db_err)?
            .ok_or_else(not_found)?;
        self.owned(&row.session_id, user_id).await?;
        Ok(row)
    }

    /// The C1-pinned receipt ordinal — `COUNT(queue_seq < mine)+1`
    /// computed off the freshly read queue (the row's raw `queue_seq`
    /// is never the answer: a drained queue leaves minted-but-delivered
    /// seqs below the next enqueue's).
    async fn queue_position(
        &self,
        session_id: &str,
        queued_id: &str,
    ) -> Result<Option<i64>, AppError> {
        let rows = self
            .db
            .list_oxpilot_queued_for_session(session_id)
            .await
            .map_err(db_err)?;
        Ok(rows
            .iter()
            .position(|r| r.id == queued_id)
            .map(|i| i as i64 + 1))
    }

    /// Queue-half dedupe → receipt for the already-queued row.
    async fn queued_receipt(
        &self,
        session_id: &str,
        client_msg_id: &str,
    ) -> Result<Option<QueuedReceipt>, AppError> {
        let Some(row) = self
            .db
            .find_oxpilot_queued_by_cmid(session_id, client_msg_id)
            .await
            .map_err(db_err)?
        else {
            return Ok(None);
        };
        let position = self.queue_position(session_id, &row.id).await?;
        Ok(Some(QueuedReceipt {
            queued_id: Some(row.id),
            position,
        }))
    }

    /// The UTC `YYYY-MM-DD` rollup window (BR6.1) — both scopes share the
    /// caller-computed day so a midnight boundary can't split one delta
    /// across two windows.
    fn utc_window() -> String {
        chrono::Utc::now().format("%Y-%m-%d").to_string()
    }
}

#[async_trait]
impl SessionService for OxpilotSessionService {
    // ---- sessions ----

    async fn create_session(
        &self,
        user_id: &str,
        input: CreateSession,
    ) -> Result<OxpilotSessionRow, AppError> {
        // Redact-on-write for the free-text pair — same paste surface as
        // message content (BR3.1/BR3.5); repo/branch stay identifiers.
        let goal = input.goal.map(|g| oxidean_core::redact(&g));
        let title = oxidean_core::redact(&input.title);
        self.db
            .insert_oxpilot_session(&NewOxpilotSession {
                id: &input.id,
                user_id,
                repo_owner: &input.repo_owner,
                repo_name: &input.repo_name,
                repo_id: &input.repo_id,
                branch: &input.branch,
                goal: goal.as_deref(),
                title: &title,
                // U8 — the proc-validated target binding rides the
                // INSERT itself (atomic; no post-write stamp).
                computer_use_target_id: input.computer_use_target_id.as_deref(),
            })
            .await
            .map_err(db_err)
    }

    async fn get_session(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<OxpilotSessionRow, AppError> {
        self.owned(session_id, user_id).await
    }

    async fn list_sessions(&self, user_id: &str) -> Result<Vec<OxpilotSessionRow>, AppError> {
        self.db
            .list_oxpilot_sessions_for_user(user_id)
            .await
            .map_err(db_err)
    }

    async fn delete_session(&self, user_id: &str, session_id: &str) -> Result<(), AppError> {
        // Ownership FIRST — a foreign/absent session never reaches the
        // hooks (no hook may learn that a foreign session id exists).
        self.owned(session_id, user_id).await?;
        // Snapshot the registry so a concurrent registrar can't shift
        // ordering mid-delete; hooks run in registration order (C8).
        let hooks = self
            .hooks
            .lock()
            .expect("oxpilot hook registry poisoned")
            .clone();
        for (idx, hook) in hooks.iter().enumerate() {
            hook.on_session_delete(session_id, user_id)
                .await
                .map_err(|e| {
                    // MERGE, not rewrite — `with_data` replaces `data`
                    // wholesale, so `hook_index` inserts into the hook's
                    // own object (else wraps it); the hook's `code` and
                    // message reach the caller unchanged (C8).
                    let prior = e.data.clone();
                    let data = match prior {
                        Some(serde_json::Value::Object(mut m)) => {
                            m.insert("hook_index".into(), idx.into());
                            serde_json::Value::Object(m)
                        }
                        other => {
                            serde_json::json!({"hook_index": idx, "hook_error": other})
                        }
                    };
                    e.with_data(data)
                })
                .inspect_err(|e| {
                    tracing::warn!(
                        session_id = %session_id,
                        hook_index = idx,
                        err.code = %e.code,
                        "oxpilot session-delete hook aborted the delete"
                    );
                })?;
        }
        self.db
            .delete_oxpilot_session_cascade(session_id, user_id)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    // ---- transcript + tool cards ----

    async fn append_message(
        &self,
        user_id: &str,
        input: AppendMessage,
    ) -> Result<OxpilotMessageRow, AppError> {
        self.owned(&input.session_id, user_id).await?;
        let seq = self
            .db
            .next_oxpilot_message_seq(&input.session_id)
            .await
            .map_err(db_err)?;
        let content = oxidean_core::redact(&input.content);
        let row = self
            .db
            .insert_oxpilot_message(&NewOxpilotMessage {
                id: &input.id,
                session_id: &input.session_id,
                seq,
                role: &input.role,
                content: &content,
                client_msg_id: input.client_msg_id.as_deref(),
                status: &input.status,
            })
            .await
            .map_err(db_err)?;
        self.db
            .touch_oxpilot_session_activity(&input.session_id)
            .await
            .map_err(db_err)?;
        Ok(row)
    }

    async fn transcript(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<OxpilotMessageRow>, AppError> {
        self.owned(session_id, user_id).await?;
        let rows = self
            .db
            .list_oxpilot_messages(session_id)
            .await
            .map_err(db_err)?;
        // BR2.3 — the transcript is the persisted record; `streaming`
        // rows are in-flight partials, never replay material.
        Ok(rows
            .into_iter()
            .filter(|m| m.status == "complete" || m.status == "error")
            .collect())
    }

    async fn list_messages(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<OxpilotMessageRow>, AppError> {
        self.owned(session_id, user_id).await?;
        // Unfiltered on purpose — the SSE snapshot's message array needs
        // the in-flight `streaming` row so a remount can overlay the live
        // tail; prompt-side callers use `transcript` (BR2.3 filtered).
        self.db
            .list_oxpilot_messages(session_id)
            .await
            .map_err(db_err)
    }

    async fn find_message_by_client_msg_id(
        &self,
        user_id: &str,
        session_id: &str,
        client_msg_id: &str,
    ) -> Result<Option<OxpilotMessageRow>, AppError> {
        self.owned(session_id, user_id).await?;
        self.db
            .find_oxpilot_message_by_client_msg_id(session_id, client_msg_id)
            .await
            .map_err(db_err)
    }

    async fn set_message_content(
        &self,
        user_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<(), AppError> {
        let msg = self.owned_message(message_id, user_id).await?;
        self.db
            .set_oxpilot_message_content(message_id, &oxidean_core::redact(content))
            .await
            .map_err(db_err)?;
        self.db
            .touch_oxpilot_session_activity(&msg.session_id)
            .await
            .map_err(db_err)
    }

    async fn set_message_status(
        &self,
        user_id: &str,
        message_id: &str,
        status: &str,
    ) -> Result<(), AppError> {
        let msg = self.owned_message(message_id, user_id).await?;
        self.db
            .set_oxpilot_message_status(message_id, status)
            .await
            .map_err(db_err)?;
        self.db
            .touch_oxpilot_session_activity(&msg.session_id)
            .await
            .map_err(db_err)
    }

    // ---- follow-up queue (US1.8) ----

    async fn enqueue(&self, user_id: &str, input: Enqueue) -> Result<QueuedReceipt, AppError> {
        self.owned(&input.session_id, user_id).await?;
        let content = oxidean_core::redact(&input.content);

        // BR2.2 union dedupe — TRANSCRIPT first (the message already
        // landed; nothing to queue), then the queue itself (idempotent
        // resend returns the SAME receipt — same row, same position).
        if let Some(cmid) = input.client_msg_id.as_deref() {
            if self
                .db
                .find_oxpilot_message_by_client_msg_id(&input.session_id, cmid)
                .await
                .map_err(db_err)?
                .is_some()
            {
                tracing::debug!(
                    session_id = %input.session_id,
                    arm = "transcript",
                    "oxpilot enqueue dedupe hit"
                );
                return Ok(QueuedReceipt {
                    queued_id: None,
                    position: None,
                });
            }
            if let Some(receipt) = self.queued_receipt(&input.session_id, cmid).await? {
                tracing::debug!(
                    session_id = %input.session_id,
                    arm = "queue",
                    "oxpilot enqueue dedupe hit"
                );
                return Ok(receipt);
            }
        }

        // Mint-then-insert with bounded retry — a `queue_seq` collision
        // under a concurrent enqueue re-reads `MAX+1`; a `client_msg_id`
        // collision resolves through the queue dedupe (the concurrent
        // insert won — return ITS receipt, preserving idempotency).
        for _attempt in 0..ENQUEUE_MAX_ATTEMPTS {
            let queue_seq = self
                .db
                .next_oxpilot_queue_seq(&input.session_id)
                .await
                .map_err(db_err)?;
            let id = Uuid::new_v4().to_string();
            match self
                .db
                .enqueue_oxpilot_queued(&NewOxpilotQueued {
                    id: &id,
                    session_id: &input.session_id,
                    queue_seq,
                    content: &content,
                    client_msg_id: input.client_msg_id.as_deref(),
                })
                .await
            {
                Ok(row) => {
                    let position = self.queue_position(&input.session_id, &row.id).await?;
                    self.db
                        .touch_oxpilot_session_activity(&input.session_id)
                        .await
                        .map_err(db_err)?;
                    return Ok(QueuedReceipt {
                        queued_id: Some(row.id),
                        position,
                    });
                }
                Err(e) if is_unique_violation(&e) => {
                    if let Some(cmid) = input.client_msg_id.as_deref() {
                        if let Some(receipt) = self.queued_receipt(&input.session_id, cmid).await? {
                            return Ok(receipt);
                        }
                    }
                    // Genuine seq collision — loop re-mints.
                    continue;
                }
                Err(e) => return Err(db_err(e)),
            }
        }
        tracing::error!(
            session_id = %input.session_id,
            "oxpilot enqueue retries exhausted"
        );
        Err(AppError::new(
            "oxpilot.internal",
            "enqueue could not allocate a queue position",
        ))
    }

    async fn drain_queue(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<OxpilotMessageRow>, AppError> {
        self.owned(session_id, user_id).await?;
        let appended = self
            .db
            .drain_oxpilot_queue_tx(session_id, || Uuid::new_v4().to_string())
            .await
            .map_err(db_err)?;
        tracing::debug!(
            session_id = %session_id,
            drained_count = appended.len(),
            "oxpilot queue drain completed"
        );
        if !appended.is_empty() {
            self.db
                .touch_oxpilot_session_activity(session_id)
                .await
                .map_err(db_err)?;
        }
        Ok(appended)
    }

    async fn list_queue(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<OxpilotQueuedRow>, AppError> {
        self.owned(session_id, user_id).await?;
        self.db
            .list_oxpilot_queued_for_session(session_id)
            .await
            .map_err(db_err)
    }

    async fn delete_queued(
        &self,
        user_id: &str,
        session_id: &str,
        queued_id: &str,
    ) -> Result<(), AppError> {
        // Same owner read `list_queue` takes — foreign/absent sessions
        // fold into `not_found` before any row is touched (BR3.3).
        self.owned(session_id, user_id).await?;
        let deleted = self
            .db
            .delete_oxpilot_queued(session_id, queued_id)
            .await
            .map_err(db_err)?;
        // Stale-ref honesty: an already-drained/foreign queued id under
        // the owned session answers `not_found` (BR9.1).
        if deleted == 0 {
            return Err(AppError::new("not_found", "queued message not found"));
        }
        Ok(())
    }

    // ---- tool-call cards (BR7.3) ----

    async fn append_tool_call(
        &self,
        user_id: &str,
        input: ToolCallWrite,
    ) -> Result<OxpilotToolCallRow, AppError> {
        self.owned(&input.session_id, user_id).await?;
        // Row-membership check — the tool card must point at a message
        // that belongs to THIS session, else the snapshot merge orphans
        // it (groups by message_id).
        let msg = self
            .db
            .find_oxpilot_message_by_id(&input.message_id)
            .await
            .map_err(db_err)?
            .ok_or_else(not_found)?;
        if msg.session_id != input.session_id {
            return Err(not_found());
        }
        let id = Uuid::new_v4().to_string();
        let row = self
            .db
            .insert_oxpilot_tool_call(&NewOxpilotToolCall {
                id: &id,
                session_id: &input.session_id,
                message_id: &input.message_id,
                call_id: &input.call_id,
                seq: input.seq,
                call_index: input.call_index,
                tool: &input.tool,
                args_summary: &oxidean_core::redact(&input.args_summary),
                status: &input.status,
            })
            .await
            .map_err(db_err)?;
        self.db
            .touch_oxpilot_session_activity(&input.session_id)
            .await
            .map_err(db_err)?;
        Ok(row)
    }

    async fn list_tool_calls(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<OxpilotToolCallRow>, AppError> {
        self.owned(session_id, user_id).await?;
        self.db
            .list_oxpilot_tool_calls_for_session(session_id)
            .await
            .map_err(db_err)
    }

    async fn set_tool_call_status(
        &self,
        user_id: &str,
        input: ToolCallStatusUpdate,
    ) -> Result<(), AppError> {
        self.owned(&input.session_id, user_id).await?;
        // Existence first — MySQL counts CHANGED rows, so an idempotent
        // rewrite of an identical status reports 0 and must not surface
        // as `not_found` (the absent-key check is the honest predicate).
        self.db
            .find_oxpilot_tool_call_by_call_id(&input.session_id, &input.call_id)
            .await
            .map_err(db_err)?
            .ok_or_else(not_found)?;
        let output = input.output_summary.map(|s| oxidean_core::redact(&s));
        self.db
            .set_oxpilot_tool_call_status(
                &input.session_id,
                &input.call_id,
                &input.status,
                output.as_deref(),
            )
            .await
            .map_err(db_err)?;
        self.db
            .touch_oxpilot_session_activity(&input.session_id)
            .await
            .map_err(db_err)
    }

    // ---- proposals (WF8) ----

    async fn save_proposal(
        &self,
        user_id: &str,
        input: ProposalWrite,
    ) -> Result<OxpilotProposalRow, AppError> {
        self.owned(&input.session_id, user_id).await?;
        let id = Uuid::new_v4().to_string();
        let row = self
            .db
            .insert_oxpilot_proposal(&NewOxpilotProposal {
                id: &id,
                session_id: &input.session_id,
                title: &oxidean_core::redact(&input.title),
                base: &input.base,
                diff_body: &oxidean_core::redact(&input.diff_body),
                ops_json: &input.ops_json,
            })
            .await
            .map_err(db_err)?;
        self.db
            .touch_oxpilot_session_activity(&input.session_id)
            .await
            .map_err(db_err)?;
        Ok(row)
    }

    async fn proposal(
        &self,
        user_id: &str,
        proposal_id: &str,
    ) -> Result<OxpilotProposalRow, AppError> {
        self.owned_proposal(proposal_id, user_id).await
    }

    async fn mark_applied(
        &self,
        user_id: &str,
        proposal_id: &str,
        pr_ref: &str,
    ) -> Result<OxpilotProposalRow, AppError> {
        let row = self.owned_proposal(proposal_id, user_id).await?;
        if row.status != "pending" {
            return Err(AppError::new("diff_not_pending", "proposal is not pending"));
        }
        self.db
            .mark_oxpilot_proposal_applied(proposal_id, pr_ref)
            .await
            .map_err(db_err)?;
        self.db
            .touch_oxpilot_session_activity(&row.session_id)
            .await
            .map_err(db_err)?;
        self.db
            .find_oxpilot_proposal(proposal_id)
            .await
            .map_err(db_err)?
            .ok_or_else(not_found)
    }

    // ---- usage + budget (US5.2) ----

    async fn record_usage(&self, user_id: &str, delta: UsageDelta) -> Result<(), AppError> {
        self.db
            .upsert_oxpilot_usage(
                user_id,
                &Self::utc_window(),
                delta.requests,
                delta.tokens.unwrap_or(0),
            )
            .await
            .map_err(db_err)
    }

    async fn check_budget(&self, user_id: &str, caps: BudgetCaps) -> Result<(), AppError> {
        let window = Self::utc_window();
        let (user_row, instance_row) = self
            .db
            .read_oxpilot_usage_counters(user_id, &window)
            .await
            .map_err(db_err)?;
        let breach = |cap: Option<i64>, row: &Option<OxpilotUsageCounterRow>| -> bool {
            let Some(cap) = cap else { return false };
            let row = row.as_ref();
            let requests = row.map(|r| r.requests).unwrap_or(0);
            let tokens = row.map(|r| r.tokens).unwrap_or(0);
            // One configured value caps BOTH axes — at/over is the line
            // (BR6.2: the NEXT spend is refused once either axis reaches
            // the cap).
            requests >= cap || tokens >= cap
        };
        // BR6.2 — the user's own breach reports first; the instance cap is
        // the outer blast bound.
        if breach(caps.user, &user_row) {
            tracing::debug!(scope = "user", "oxpilot budget refusal");
            return Err(AppError::new("budget_exceeded", "user budget exceeded")
                .with_data(serde_json::json!({ "scope": "user" })));
        }
        if breach(caps.instance, &instance_row) {
            tracing::debug!(scope = "instance", "oxpilot budget refusal");
            return Err(AppError::new("budget_exceeded", "instance budget exceeded")
                .with_data(serde_json::json!({ "scope": "instance" })));
        }
        Ok(())
    }

    async fn usage_view(&self, user_id: &str, caps: BudgetCaps) -> Result<UsageView, AppError> {
        let window = Self::utc_window();
        let (user_row, _instance_row) = self
            .db
            .read_oxpilot_usage_counters(user_id, &window)
            .await
            .map_err(db_err)?;
        // The C1 payload is the user's own spend view — caps mirror the
        // configured USER axes (null ⇒ unbounded); it is the spend
        // surface itself so it NEVER refuses when over budget.
        Ok(UsageView {
            period: window,
            tokens_used: user_row.as_ref().map(|r| r.tokens).unwrap_or(0),
            tokens_cap: caps.user,
            requests_used: user_row.as_ref().map(|r| r.requests).unwrap_or(0),
            requests_cap: caps.user,
        })
    }
}

#[cfg(test)]
mod tests {
    //! U2 service tests — a REAL SQLite `Database` under
    //! `OxpilotSessionService`, never a mock (the behavior under test IS
    //! the store composition: ownership gates, dedupe union, hook order,
    //! budget scope order, redaction-on-write).

    use super::*;
    use oxidean_api_core::oxpilot::{
        AppendMessage, BudgetCaps, CreateSession, Enqueue, ProposalWrite, SessionDeleteHook,
        ToolCallStatusUpdate, ToolCallWrite, UsageDelta,
    };

    async fn test_db() -> Database {
        let db = Database::connect("sqlite::memory:").await.expect("connect");
        db.migrate().await.expect("migrate");
        db
    }

    /// User + repo fixtures (FK parents for `oxpilot_sessions`).
    async fn seed(db: &Database, tag: &str) -> (String, String) {
        let uid = format!("u-svc-{tag}");
        db.create_user(
            &uid,
            &format!("svc-{tag}@example.com"),
            &format!("svc{tag}"),
            None,
            "Svc Seed",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("seed user");
        let repo = format!("repo-svc-{tag}");
        db.insert_repository(&repo, &uid, "user", &repo, "private", "", "main")
            .await
            .expect("seed repo");
        (uid, repo)
    }

    fn create(id: &str, repo: &str, goal: Option<&str>, title: &str) -> CreateSession {
        CreateSession {
            id: id.to_string(),
            repo_owner: "oxpilot".to_string(),
            repo_name: repo.to_string(),
            repo_id: repo.to_string(),
            branch: "main".to_string(),
            goal: goal.map(str::to_string),
            title: title.to_string(),
            computer_use_target_id: None,
        }
    }

    async fn new_session(
        svc: &OxpilotSessionService,
        uid: &str,
        repo: &str,
        id: &str,
    ) -> OxpilotSessionRow {
        svc.create_session(uid, create(id, repo, Some("g"), "t"))
            .await
            .expect("create session")
    }

    fn append(
        id: &str,
        session_id: &str,
        role: &str,
        content: &str,
        cmid: Option<&str>,
    ) -> AppendMessage {
        AppendMessage {
            id: id.to_string(),
            session_id: session_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            client_msg_id: cmid.map(str::to_string),
            status: "complete".to_string(),
        }
    }

    fn enqueue(session_id: &str, content: &str, cmid: Option<&str>) -> Enqueue {
        Enqueue {
            session_id: session_id.to_string(),
            content: content.to_string(),
            client_msg_id: cmid.map(str::to_string),
        }
    }

    // ---------- ownership + sessions ----------

    #[tokio::test]
    async fn session_crud_and_cross_user_not_found() {
        let db = test_db().await;
        let (uid_a, repo) = seed(&db, "a").await;
        let (uid_b, _) = seed(&db, "b").await;
        let svc = OxpilotSessionService::new(db.clone());
        let row = new_session(&svc, &uid_a, &repo, "s1").await;
        assert_eq!(row.user_id, uid_a);
        assert_eq!(row.branch, "main");

        // Owned read + list.
        assert_eq!(svc.get_session(&uid_a, "s1").await.unwrap().id, "s1");
        assert_eq!(svc.list_sessions(&uid_a).await.unwrap().len(), 1);
        assert_eq!(svc.list_sessions(&uid_b).await.unwrap().len(), 0);

        // Every session-scoped verb answers not_found cross-user.
        for e in [
            svc.get_session(&uid_b, "s1").await.unwrap_err(),
            svc.delete_session(&uid_b, "s1").await.unwrap_err(),
            svc.transcript(&uid_b, "s1").await.unwrap_err(),
            svc.enqueue(&uid_b, enqueue("s1", "x", None))
                .await
                .unwrap_err(),
            svc.list_queue(&uid_b, "s1").await.unwrap_err(),
            svc.list_tool_calls(&uid_b, "s1").await.unwrap_err(),
            svc.drain_queue(&uid_b, "s1").await.unwrap_err(),
        ] {
            assert_eq!(e.code, "not_found", "{e:?}");
        }
    }

    #[tokio::test]
    async fn delete_cascades_and_then_not_found() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "c").await;
        let svc = OxpilotSessionService::new(db.clone());
        new_session(&svc, &uid, &repo, "s1").await;
        svc.append_message(&uid, append("m1", "s1", "user", "hi", Some("c1")))
            .await
            .unwrap();
        svc.enqueue(&uid, enqueue("s1", "later", Some("c2")))
            .await
            .unwrap();
        svc.append_tool_call(
            &uid,
            ToolCallWrite {
                session_id: "s1".to_string(),
                message_id: "m1".to_string(),
                seq: 1,
                call_index: 0,
                call_id: "call-1".to_string(),
                tool: "fs_read".to_string(),
                args_summary: "{}".to_string(),
                status: "running".to_string(),
            },
        )
        .await
        .unwrap();
        svc.save_proposal(
            &uid,
            ProposalWrite {
                session_id: "s1".to_string(),
                title: "t".to_string(),
                base: "main".to_string(),
                diff_body: "d".to_string(),
                ops_json: "[]".to_string(),
            },
        )
        .await
        .unwrap();

        svc.delete_session(&uid, "s1").await.expect("delete");
        assert_eq!(
            svc.get_session(&uid, "s1").await.unwrap_err().code,
            "not_found"
        );
        // Cascade removed every child row.
        assert!(db.list_oxpilot_messages("s1").await.unwrap().is_empty());
        assert!(db
            .list_oxpilot_queued_for_session("s1")
            .await
            .unwrap()
            .is_empty());
        assert!(db
            .list_oxpilot_tool_calls_for_session("s1")
            .await
            .unwrap()
            .is_empty());
    }

    // ---------- transcript ----------

    #[tokio::test]
    async fn append_mints_seq_and_transcript_excludes_streaming() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "t").await;
        let svc = OxpilotSessionService::new(db);
        new_session(&svc, &uid, &repo, "s1").await;

        let m1 = svc
            .append_message(&uid, append("m1", "s1", "user", "one", None))
            .await
            .unwrap();
        let mut streaming = append("m2", "s1", "assistant", "partial", None);
        streaming.status = "streaming".to_string();
        let m2 = svc.append_message(&uid, streaming).await.unwrap();
        let m3 = svc
            .append_message(&uid, append("m3", "s1", "user", "two", None))
            .await
            .unwrap();
        assert_eq!((m1.seq, m2.seq, m3.seq), (1, 2, 3));

        let t = svc.transcript(&uid, "s1").await.unwrap();
        assert_eq!(
            t.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["m1", "m3"],
            "streaming rows stay out of the transcript"
        );
    }

    // ---------- enqueue / dedupe / position ----------

    #[tokio::test]
    async fn enqueue_transcript_hit_returns_null_receipt() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "e").await;
        let svc = OxpilotSessionService::new(db);
        new_session(&svc, &uid, &repo, "s1").await;
        svc.append_message(&uid, append("m1", "s1", "user", "hi", Some("cm-1")))
            .await
            .unwrap();

        let receipt = svc
            .enqueue(&uid, enqueue("s1", "hi again", Some("cm-1")))
            .await
            .unwrap();
        assert_eq!(receipt.queued_id, None);
        assert_eq!(receipt.position, None);
        assert!(svc.list_queue(&uid, "s1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn enqueue_queued_hit_returns_same_receipt() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "e2").await;
        let svc = OxpilotSessionService::new(db);
        new_session(&svc, &uid, &repo, "s1").await;

        let first = svc
            .enqueue(&uid, enqueue("s1", "follow-up", Some("cm-9")))
            .await
            .unwrap();
        let second = svc
            .enqueue(&uid, enqueue("s1", "follow-up", Some("cm-9")))
            .await
            .unwrap();
        assert!(first.queued_id.is_some());
        assert_eq!(first, second, "idempotent resend returns the same receipt");
        assert_eq!(svc.list_queue(&uid, "s1").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn enqueue_position_is_ordinal_after_drain() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "e3").await;
        let svc = OxpilotSessionService::new(db.clone());
        new_session(&svc, &uid, &repo, "s1").await;

        let r1 = svc.enqueue(&uid, enqueue("s1", "a", None)).await.unwrap();
        let r2 = svc.enqueue(&uid, enqueue("s1", "b", None)).await.unwrap();
        assert_eq!(r1.position, Some(1));
        assert_eq!(r2.position, Some(2));

        // Drain delivers in order and empties the queue.
        let delivered = svc.drain_queue(&uid, "s1").await.unwrap();
        assert_eq!(delivered.len(), 2);
        assert_eq!(delivered[0].content, "a");
        assert_eq!(delivered[1].content, "b");
        assert_eq!(delivered[0].role, "user");
        assert!(svc.list_queue(&uid, "s1").await.unwrap().is_empty());
        // Exactly-once — a second drain is empty.
        assert!(svc.drain_queue(&uid, "s1").await.unwrap().is_empty());

        // The next enqueue re-mints 1 (queue_seq is MAX+1 over LIVE rows)
        // — and its position is still the ordinal 1.
        let r3 = svc.enqueue(&uid, enqueue("s1", "c", None)).await.unwrap();
        assert_eq!(r3.position, Some(1));

        // Prove position is ordinal, not queue_seq: a stray row at a
        // high seq (out-of-band insert, e.g. a replayed write) leaves
        // the next enqueue's queue_seq far above its queue position.
        db.enqueue_oxpilot_queued(&oxidean_db::NewOxpilotQueued {
            id: "stray",
            session_id: "s1",
            queue_seq: 50,
            content: "stray",
            client_msg_id: None,
        })
        .await
        .expect("stray insert");
        let r4 = svc.enqueue(&uid, enqueue("s1", "d", None)).await.unwrap();
        assert_eq!(r4.position, Some(3), "ordinal position, not queue_seq");
        let row = svc
            .list_queue(&uid, "s1")
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.id == r4.queued_id.as_deref().unwrap())
            .unwrap();
        assert_eq!(row.queue_seq, 51);
    }

    #[tokio::test]
    async fn enqueue_concurrent_pair_both_land() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "e4").await;
        let svc = Arc::new(OxpilotSessionService::new(db));
        new_session(&svc, &uid, &repo, "s1").await;

        // A concurrent pair can collide on queue_seq — the bounded
        // re-mint path must still land both rows, ordered.
        let (a, b) = {
            let s1 = svc.clone();
            let s2 = svc.clone();
            let u = uid.clone();
            tokio::join!(
                s1.enqueue(&u, enqueue("s1", "first", None)),
                s2.enqueue(&u, enqueue("s1", "second", None)),
            )
        };
        let ra = a.expect("enqueue a");
        let rb = b.expect("enqueue b");
        assert_ne!(ra.queued_id, rb.queued_id);
        let mut positions = [ra.position.unwrap(), rb.position.unwrap()];
        positions.sort();
        assert_eq!(positions, [1, 2]);
        assert_eq!(svc.list_queue(&uid, "s1").await.unwrap().len(), 2);
    }

    // ---------- hooks (C8) ----------

    struct ProbeHook {
        calls: Arc<Mutex<Vec<(usize, String)>>>,
        index: usize,
        fail: bool,
    }

    #[async_trait]
    impl SessionDeleteHook for ProbeHook {
        async fn on_session_delete(
            &self,
            session_id: &str,
            _user_id: &str,
        ) -> Result<(), AppError> {
            self.calls
                .lock()
                .unwrap()
                .push((self.index, session_id.to_string()));
            if self.fail {
                Err(AppError::new("probe.boom", "hook exploded"))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn hooks_run_in_order_and_failure_aborts_delete() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "h").await;
        let svc = OxpilotSessionService::new(db.clone());
        new_session(&svc, &uid, &repo, "s1").await;
        svc.append_message(&uid, append("m1", "s1", "user", "keep", None))
            .await
            .unwrap();

        let calls: Arc<Mutex<Vec<(usize, String)>>> = Arc::new(Mutex::new(Vec::new()));
        svc.register_delete_hook(Arc::new(ProbeHook {
            calls: calls.clone(),
            index: 0,
            fail: false,
        }));
        svc.register_delete_hook(Arc::new(ProbeHook {
            calls: calls.clone(),
            index: 1,
            fail: true,
        }));

        let err = svc.delete_session(&uid, "s1").await.unwrap_err();
        // The hook's own code survives — `hook_index` merges into `data`
        // (the proc keys its `hook_failed` audit label off its presence).
        assert_eq!(err.code, "probe.boom");
        assert_eq!(err.data.as_ref().unwrap()["hook_index"], 1);
        // Registration order held; the cascade never ran.
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[(0, "s1".to_string()), (1, "s1".to_string())]
        );
        assert!(svc.get_session(&uid, "s1").await.is_ok());
        assert!(!db.list_oxpilot_messages("s1").await.unwrap().is_empty());
    }

    // ---------- budget / usage (US5.2) ----------

    #[tokio::test]
    async fn budget_scope_order_user_before_instance() {
        let db = test_db().await;
        let (uid, _repo) = seed(&db, "u").await;
        let svc = OxpilotSessionService::new(db);
        svc.record_usage(
            &uid,
            UsageDelta {
                requests: 5,
                tokens: Some(3),
            },
        )
        .await
        .unwrap();

        // User breach reports first even when instance is also over.
        let err = svc
            .check_budget(
                &uid,
                BudgetCaps {
                    user: Some(3),
                    instance: Some(3),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, "budget_exceeded");
        assert_eq!(err.data.as_ref().unwrap()["scope"], "user");

        // Instance-only breach.
        let err = svc
            .check_budget(
                &uid,
                BudgetCaps {
                    user: None,
                    instance: Some(3),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, "budget_exceeded");
        assert_eq!(err.data.as_ref().unwrap()["scope"], "instance");

        // Unbounded caps never refuse (BR6.3).
        svc.check_budget(&uid, BudgetCaps::default()).await.unwrap();
        // Caps above the counter pass.
        svc.check_budget(
            &uid,
            BudgetCaps {
                user: Some(10),
                instance: None,
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn usage_view_reports_over_cap_without_refusing() {
        let db = test_db().await;
        let (uid, _repo) = seed(&db, "u2").await;
        let svc = OxpilotSessionService::new(db);
        svc.record_usage(
            &uid,
            UsageDelta {
                requests: 7,
                tokens: Some(11),
            },
        )
        .await
        .unwrap();

        // Over every cap — the spend surface still answers.
        let view = svc
            .usage_view(
                &uid,
                BudgetCaps {
                    user: Some(1),
                    instance: Some(1),
                },
            )
            .await
            .expect("usage_view never refuses");
        assert_eq!(view.requests_used, 7);
        assert_eq!(view.tokens_used, 11);
        assert_eq!(view.requests_cap, Some(1));
        assert_eq!(view.tokens_cap, Some(1));
        assert_eq!(view.period.chars().count(), "YYYY-MM-DD".len());
    }

    // ---------- proposals ----------

    #[tokio::test]
    async fn proposal_ownership_fetch_then_check_and_transition() {
        let db = test_db().await;
        let (uid_a, repo) = seed(&db, "pa").await;
        let (uid_b, _) = seed(&db, "pb").await;
        let svc = OxpilotSessionService::new(db);
        new_session(&svc, &uid_a, &repo, "s1").await;
        new_session(&svc, &uid_b, &repo, "s2").await;

        let p = svc
            .save_proposal(
                &uid_a,
                ProposalWrite {
                    session_id: "s1".to_string(),
                    title: "t".to_string(),
                    base: "main".to_string(),
                    diff_body: "d".to_string(),
                    ops_json: "[]".to_string(),
                },
            )
            .await
            .unwrap();
        assert_eq!(p.status, "pending");

        // Fetch-then-check: a foreign proposal is not_found, not forbidden.
        assert_eq!(
            svc.proposal(&uid_b, &p.id).await.unwrap_err().code,
            "not_found"
        );
        assert_eq!(
            svc.mark_applied(&uid_b, &p.id, "pr-1")
                .await
                .unwrap_err()
                .code,
            "not_found"
        );
        // Absent ids read the same.
        assert_eq!(
            svc.proposal(&uid_a, "missing").await.unwrap_err().code,
            "not_found"
        );

        let applied = svc
            .mark_applied(&uid_a, &p.id, "pr-1")
            .await
            .expect("apply");
        assert_eq!(applied.status, "applied");
        assert_eq!(applied.pr_ref.as_deref(), Some("pr-1"));
        // Terminal — a second apply refuses.
        assert_eq!(
            svc.mark_applied(&uid_a, &p.id, "pr-2")
                .await
                .unwrap_err()
                .code,
            "diff_not_pending"
        );
    }

    // ---------- tool calls ----------

    #[tokio::test]
    async fn tool_call_write_and_status_transition() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "tc").await;
        let svc = OxpilotSessionService::new(db);
        new_session(&svc, &uid, &repo, "s1").await;
        svc.append_message(&uid, append("m1", "s1", "assistant", "", None))
            .await
            .unwrap();

        svc.append_tool_call(
            &uid,
            ToolCallWrite {
                session_id: "s1".to_string(),
                message_id: "m1".to_string(),
                seq: 1,
                call_index: 0,
                call_id: "call-1".to_string(),
                tool: "fs_read".to_string(),
                args_summary: "{\"path\":\"/a\"}".to_string(),
                status: "running".to_string(),
            },
        )
        .await
        .unwrap();
        svc.set_tool_call_status(
            &uid,
            ToolCallStatusUpdate {
                session_id: "s1".to_string(),
                call_id: "call-1".to_string(),
                status: "done".to_string(),
                output_summary: Some("read 3 files".to_string()),
            },
        )
        .await
        .unwrap();
        let cards = svc.list_tool_calls(&uid, "s1").await.unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].status, "done");
        assert_eq!(cards[0].output_summary, "read 3 files");

        // Unknown call_id on an owned session ⇒ not_found.
        assert_eq!(
            svc.set_tool_call_status(
                &uid,
                ToolCallStatusUpdate {
                    session_id: "s1".to_string(),
                    call_id: "nope".to_string(),
                    status: "done".to_string(),
                    output_summary: None,
                },
            )
            .await
            .unwrap_err()
            .code,
            "not_found"
        );
    }

    // ---------- redaction boundary ----------

    #[tokio::test]
    async fn free_text_redacts_ids_do_not() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "r").await;
        let svc = OxpilotSessionService::new(db);
        let secret = "ghp_ab...mnop";
        let session = svc
            .create_session(
                &uid,
                create(
                    "s1",
                    &repo,
                    Some(&format!("leak {secret} here")),
                    &format!("title {secret}"),
                ),
            )
            .await
            .unwrap();
        assert!(session.goal.as_deref().unwrap().contains("***"));
        assert!(!session.goal.as_deref().unwrap().contains(secret));
        assert!(session.title.contains("***"));
        // Identifiers ride through unmasked (ID carve-out).
        assert_eq!(session.repo_owner, "oxpilot");
        assert_eq!(session.branch, "main");

        // Message + queue content.
        let m = svc
            .append_message(
                &uid,
                append("m1", "s1", "user", &format!("key {secret}"), Some("cm")),
            )
            .await
            .unwrap();
        assert!(m.content.contains("***"));
        assert_eq!(m.client_msg_id.as_deref(), Some("cm"));
        svc.enqueue(&uid, enqueue("s1", &format!("q {secret}"), None))
            .await
            .unwrap();
        assert!(svc.list_queue(&uid, "s1").await.unwrap()[0]
            .content
            .contains("***"));

        // Tool summaries redact; the tool name is an identifier.
        svc.append_tool_call(
            &uid,
            ToolCallWrite {
                session_id: "s1".to_string(),
                message_id: "m1".to_string(),
                seq: 1,
                call_index: 0,
                call_id: "call-1".to_string(),
                tool: "fs_read".to_string(),
                args_summary: format!("args {secret}"),
                status: "running".to_string(),
            },
        )
        .await
        .unwrap();
        svc.set_tool_call_status(
            &uid,
            ToolCallStatusUpdate {
                session_id: "s1".to_string(),
                call_id: "call-1".to_string(),
                status: "done".to_string(),
                output_summary: Some(format!("out {secret}")),
            },
        )
        .await
        .unwrap();
        let card = &svc.list_tool_calls(&uid, "s1").await.unwrap()[0];
        assert!(card.args_summary.contains("***"));
        assert!(card.output_summary.contains("***"));
        assert_eq!(card.tool, "fs_read");

        // Proposal title/diff redact; base is an identifier.
        let p = svc
            .save_proposal(
                &uid,
                ProposalWrite {
                    session_id: "s1".to_string(),
                    title: format!("t {secret}"),
                    base: "main".to_string(),
                    diff_body: format!("d {secret}"),
                    ops_json: "[]".to_string(),
                },
            )
            .await
            .unwrap();
        assert!(p.title.contains("***"));
        assert!(p.diff_body.contains("***"));
        assert_eq!(p.base, "main");
    }

    // ---------- drain ordering ----------

    #[tokio::test]
    async fn drain_carries_client_msg_id_and_seq_order() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "d").await;
        let svc = OxpilotSessionService::new(db);
        new_session(&svc, &uid, &repo, "s1").await;
        svc.append_message(&uid, append("m0", "s1", "user", "first", None))
            .await
            .unwrap();
        svc.enqueue(&uid, enqueue("s1", "q1", Some("cq1")))
            .await
            .unwrap();
        svc.enqueue(&uid, enqueue("s1", "q2", Some("cq2")))
            .await
            .unwrap();

        let delivered = svc.drain_queue(&uid, "s1").await.unwrap();
        assert_eq!(delivered.len(), 2);
        // seqs continue the transcript's monotonic run.
        assert_eq!((delivered[0].seq, delivered[1].seq), (2, 3));
        assert_eq!(delivered[0].client_msg_id.as_deref(), Some("cq1"));
        assert_eq!(delivered[1].client_msg_id.as_deref(), Some("cq2"));
        // A drained cmid dedupes against the TRANSCRIPT now.
        let receipt = svc
            .enqueue(&uid, enqueue("s1", "q1 again", Some("cq1")))
            .await
            .unwrap();
        assert_eq!(receipt.queued_id, None);
        assert_eq!(receipt.position, None);
    }

    #[tokio::test]
    async fn message_scoped_verbs_check_ownership() {
        let db = test_db().await;
        let (uid_a, repo) = seed(&db, "ma").await;
        let (uid_b, _) = seed(&db, "mb").await;
        let svc = OxpilotSessionService::new(db);
        new_session(&svc, &uid_a, &repo, "s1").await;
        let m = svc
            .append_message(&uid_a, append("m1", "s1", "assistant", "x", None))
            .await
            .unwrap();

        // The foreign caller can't reach the row through its message id.
        for e in [
            svc.set_message_content(&uid_b, &m.id, "y")
                .await
                .unwrap_err(),
            svc.set_message_status(&uid_b, &m.id, "complete")
                .await
                .unwrap_err(),
        ] {
            assert_eq!(e.code, "not_found", "{e:?}");
        }
        // Owner can.
        svc.set_message_status(&uid_a, &m.id, "complete")
            .await
            .unwrap();
    }
}
