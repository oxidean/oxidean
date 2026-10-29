//! `oxpilot.*` shared seam — the `LlmProvider` trait + request/stream
//! payload types, the per-process `ActiveTurn` map, the rebuildable
//! [`OxpilotRuntime`] slot, and the `EnablementGuard` resolution.
//! `AppState`/`RpcCtx` hold [`SharedOxpilot`] so the domain crate
//! (`oxidean-api-oxpilot`), the SSE routes, and U4's settings writer all
//! see the same handles.
//!
//! Enablement = flag AND provider slot (BR2.1): `OXIDEAN_OXPILOT_ENABLED`
//! (default OFF — fail-closed for a spend-bearing feature) feeds the
//! resolved `enabled` bit; U4's `oxpilot_settings` singleton row can
//! override it, and the resolved reason is stamped into the slot at every
//! rebuild (`flag_off | no_provider | misconfigured`).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use oxidean_core::AppError;
use oxidean_db::{
    OxpilotActionRow, OxpilotMessageRow, OxpilotProposalRow, OxpilotQueuedRow, OxpilotSessionRow,
    OxpilotToolCallRow,
};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::rpc::{ClientMeta, RpcCtx};

// ---------- provider trait (C4) ----------

/// One inbound message for the provider — role-tagged content.
/// `role` ∈ `user | assistant | system | tool`; `"tool"` is the
/// result role the turn loop feeds back (C4 amendment, U3).
#[derive(Debug, Clone)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
    /// Set when `role == "tool"` — the call this result answers. Never
    /// empty on the wire: the turn loop mints
    /// `call_{message_id}_{call_index}` when the provider omits an id
    /// (the durable card keys `UNIQUE(session_id, call_id)`).
    pub tool_call_id: Option<String>,
    /// Set when `role == "tool"` — Ollama's tool-result message is
    /// name-keyed; also the fallback for adapters that can't resolve
    /// the name from `tool_calls_json`.
    pub tool_name: Option<String>,
    /// Set on the assistant's round message when it emitted calls —
    /// the serialized call array the adapter maps to provider-native
    /// `tool_use`/`tool_calls` (`[{"id","name","args_json"}]`).
    pub tool_calls_json: Option<String>,
}

/// One tool the model may invoke — the provider-facing copy of the
/// executor's catalog entry (U3 populates `ChatRequest::tools`; U4's
/// adapters serialize it into each provider's tool schema shape).
#[derive(Debug, Clone)]
pub struct ToolSpec {
    /// Tool id the model invokes — round-trips into `ToolCall::name`.
    pub name: String,
    /// One-line description surfaced to the model.
    pub description: String,
    /// JSON Schema for the tool's arguments object.
    pub parameters_json: Value,
}

// ---------- tool-executor port (C6, U3-provided / U5-consumed) ----------

/// One provider-emitted tool invocation — `call_id` is the effective id
/// the turn loop resolved (the provider's id, or the minted
/// `call_{message_id}_{call_index}` when it omitted one).
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub call_id: String,
    /// Tool id — matches a `ToolSpec::name` from the executor's catalog.
    pub name: String,
    /// Raw JSON arguments payload, verbatim from the provider wire.
    pub args_json: String,
}

/// The owned-session context an invocation runs under — minted from the
/// claim's captured owner context (chained turns carry it forward), so
/// provider args can never rebind the tool's scope (NFR1.2/NFR1.4).
#[derive(Debug, Clone)]
pub struct ToolCtx {
    pub session_id: String,
    pub user_id: String,
    pub repo_id: String,
    /// Set when a tool binds work to an existing proposal (U5).
    pub proposal_id: Option<String>,
}

/// Successful tool result — `content` is the full output the model sees
/// (`role:"tool"` body); `output_summary` is the bounded form the card
/// row + `tool` frame carry (≤ `OUTPUT_SUMMARY_MAX`).
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: String,
    pub output_summary: String,
}

/// Failed tool result — still feeds back as a `role:"tool"` message so
/// the model sees every emitted call answered. `message` is
/// secrets-redacted and bounded to `OUTPUT_SUMMARY_MAX` when it lands
/// in provider content + the card summary.
#[derive(Debug, Clone)]
pub struct ToolError {
    pub code: String,
    pub message: String,
}

/// The tool-execution port (C6) — PROVIDED by U3 as the slot seam,
/// CONSUMED by U5's concrete registry. The runtime routes every
/// provider-emitted call through it and never executes or pre-filters
/// itself.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// The provider-facing catalog — serialized onto each `ChatRequest`.
    fn catalog(&self) -> Vec<ToolSpec>;
    /// Execute one call under the caller's timeout/cancel race.
    async fn invoke(&self, call: ToolCall, ctx: ToolCtx) -> Result<ToolOutput, ToolError>;
}

/// The empty executor — ships as the slot's `None` mapping at
/// `turn_parts()` time so `TurnParts.executor` stays non-Option. A
/// model that emits calls anyway gets `no_tools` errors fed back.
pub struct NullTools;

#[async_trait]
impl ToolExecutor for NullTools {
    fn catalog(&self) -> Vec<ToolSpec> {
        Vec::new()
    }

    async fn invoke(&self, _call: ToolCall, _ctx: ToolCtx) -> Result<ToolOutput, ToolError> {
        Err(ToolError {
            code: "no_tools".to_string(),
            message: "no tool executor installed".to_string(),
        })
    }
}

/// Token accounting carried on the `Done` stream item — `None` when a
/// provider wire doesn't report usage (Ollama reports it; the stub
/// fabricates it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Provider request — model plus the redacted transcript tail.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<ChatMsg>,
    /// Empty while no tools are wired (U3 owns the catalog); the C4
    /// contract requires adapters to serialize whatever arrives.
    pub tools: Vec<ToolSpec>,
}

/// One streamed item from a provider call (C4).
#[derive(Debug, Clone)]
pub enum ChatStreamItem {
    Delta(String),
    /// `args_json` is the raw JSON arguments payload — adapters
    /// serialize Anthropic's `input`/Ollama's `arguments` object into it.
    ToolCall {
        id: String,
        name: String,
        args_json: String,
    },
    Done {
        usage: Option<Usage>,
    },
    Error {
        code: String,
        msg: String,
    },
}

/// Stream type returned by `LlmProvider::stream` — a boxed async iterator
/// of [`ChatStreamItem`].
pub type ChatStream =
    std::pin::Pin<Box<dyn futures_util::Stream<Item = ChatStreamItem> + Send + 'static>>;

/// Provider failure — `code` maps to the SSE terminal frame's `code` field.
#[derive(Debug)]
pub struct ProviderError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ProviderError {}

/// The LLM adapter seam (C4). U1 ships [`StubProvider`]; U4's registry
/// (`oxidean-api-oxpilot::providers`) adds `openai|anthropic|ollama` and
/// rebuilds the slot under the runtime lock.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Stable provider id — `stub`, `openai`, `anthropic`, `ollama`.
    fn id(&self) -> &'static str;
    /// Open a streamed chat call.
    async fn stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError>;
}

/// Deterministic canned-reply provider — no network, no key, fixed-delta
/// output for the walking skeleton (BR4.5). `fail` is test-only failure
/// injection so provider errors prove the terminal-error path.
pub struct StubProvider {
    /// When `true`, `stream` answers `ProviderError` immediately.
    pub fail: bool,
}

#[async_trait]
impl LlmProvider for StubProvider {
    fn id(&self) -> &'static str {
        "stub"
    }

    async fn stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        if self.fail {
            return Err(ProviderError {
                code: "provider_error".to_string(),
                message: "stub provider injected failure".to_string(),
            });
        }
        let last_user = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let reply = format!("Oxpilot (stub): received \"{}\".", last_user);
        // Fixed-size deltas so frame counts are deterministic in tests.
        let deltas: Vec<String> = reply
            .as_bytes()
            .chunks(16)
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect();
        Ok(Box::pin(futures_util::stream::iter(
            deltas
                .into_iter()
                .map(ChatStreamItem::Delta)
                .chain(std::iter::once(ChatStreamItem::Done {
                    usage: Some(Usage {
                        input_tokens: last_user.len() as u64,
                        output_tokens: reply.len() as u64,
                    }),
                })),
        )))
    }
}

// ---------- session service port (C5, U2) ----------

/// `oxpilot.session_create` input — the caller resolves repo binding and
/// mints the row id; the service redacts `goal`/`title` before write.
#[derive(Debug, Clone)]
pub struct CreateSession {
    /// Caller-minted row id (uuid at the proc/route boundary).
    pub id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub repo_id: String,
    pub branch: String,
    pub goal: Option<String>,
    pub title: String,
    /// U8 — the optional computer-use target binding (`0090` column);
    /// the proc validates ownership + `connected` lifecycle up front so
    /// the id lands on the INSERT itself. `None` keeps the shipped
    /// NULL ("no target").
    pub computer_use_target_id: Option<String>,
}

/// `append_message` input — caller-minted `id` (the turn's assistant id or
/// a fresh user id) and `status` (`streaming` opens an assistant row,
/// `complete` lands a user row); the service mints `seq`.
#[derive(Debug, Clone)]
pub struct AppendMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub client_msg_id: Option<String>,
    pub status: String,
}

/// `enqueue` input — the `oxpilot.message_queue` payload.
#[derive(Debug, Clone)]
pub struct Enqueue {
    pub session_id: String,
    pub content: String,
    pub client_msg_id: Option<String>,
}

/// The C1-pinned receipt — `position` is the ordinal
/// `COUNT(queue_seq < mine)+1`, not `queue_seq`. A transcript-side dedupe
/// hit returns `{null, null}` (already persisted, nothing queued).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueuedReceipt {
    pub queued_id: Option<String>,
    pub position: Option<i64>,
}

/// `append_tool_call` input — `seq` is the caller-supplied positional copy
/// of the owning assistant message; `call_index` is the per-message
/// ordinal so repeated same-tool calls persist distinctly (BR7.3).
#[derive(Debug, Clone)]
pub struct ToolCallWrite {
    pub session_id: String,
    pub message_id: String,
    pub seq: i64,
    pub call_index: i64,
    pub call_id: String,
    pub tool: String,
    pub args_summary: String,
    pub status: String,
}

/// `set_tool_call_status` input — `done|error` plus the optional output
/// summary written on the terminal transition.
#[derive(Debug, Clone)]
pub struct ToolCallStatusUpdate {
    pub session_id: String,
    pub call_id: String,
    pub status: String,
    pub output_summary: Option<String>,
}

/// `save_proposal` input — `base` is the diff's base ref/sha (an
/// identifier, never redacted); `title`/`diff_body` redact on write.
/// `ops_json` is the serialized ordered `DiffFileOp[]` the apply path
/// executes verbatim (U5) — validated op-set only, never redacted.
/// `base_sha` is `base` resolved to its tip commit at propose time —
/// the apply path branches from it so a later push can't move the
/// tree out from under the generated patch; `""` when `base` listed
/// no head (already a commit-ish).
#[derive(Debug, Clone)]
pub struct ProposalWrite {
    pub session_id: String,
    pub title: String,
    pub base: String,
    pub base_sha: String,
    pub diff_body: String,
    pub ops_json: String,
}

/// `record_usage` delta — `requests` accumulates every exchange;
/// `tokens` accumulates only when the provider reported usage (None ⇒ +0).
#[derive(Debug, Clone, Copy)]
pub struct UsageDelta {
    pub requests: i64,
    pub tokens: Option<i64>,
}

/// The resolved budget axes (US5.2) — one configured value caps requests
/// AND tokens for that scope; `None` means unbounded. Lives on
/// [`OxpilotRuntimeSlot`] so callers read `slot.budget_caps` (the
/// stateless service can't reach `EffectiveProviderConfig`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BudgetCaps {
    pub user: Option<i64>,
    pub instance: Option<i64>,
}

/// The `oxpilot.usage_get` payload — the C1 five-field view; `*_cap`
/// mirrors the configured cap or `null` (unbounded).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsageView {
    /// UTC day `YYYY-MM-DD` the counters roll under.
    pub period: String,
    pub tokens_used: i64,
    pub tokens_cap: Option<i64>,
    pub requests_used: i64,
    pub requests_cap: Option<i64>,
}

/// The C8 delete-hook seam — registered hooks run BEFORE the cascade
/// delete, in registration order (U3 abort-live-turns first, U6
/// revoke-grants second); a hook error aborts the delete. Hooks are
/// idempotent — re-entry is a no-op.
#[async_trait]
pub trait SessionDeleteHook: Send + Sync {
    async fn on_session_delete(&self, session_id: &str, user_id: &str) -> Result<(), AppError>;
}

/// The session-core persistence and integrity port (C5) — 22 verbs,
/// `user_id`-scoped throughout: every session-scoped verb validates
/// ownership first and cross-user/missing resolves `not_found` (BR1.1);
/// body-bearing writes secrets-redact at this boundary (BR3.1). Reads
/// return the [`oxidean_db`] row structs — no DTO copy layer.
///
/// The concrete `OxpilotSessionService` lives in `oxidean-api-oxpilot`
/// and installs on [`OxpilotRuntime`] at shell assembly — a trait object
/// here so U3/U6/U8 consume the seam without a crate cycle.
#[async_trait]
pub trait SessionService: Send + Sync {
    // ---- sessions (US1.1/US1.2/US1.5) ----
    async fn create_session(
        &self,
        user_id: &str,
        input: CreateSession,
    ) -> Result<OxpilotSessionRow, AppError>;
    async fn get_session(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<OxpilotSessionRow, AppError>;
    async fn list_sessions(&self, user_id: &str) -> Result<Vec<OxpilotSessionRow>, AppError>;
    /// C8: ordered hooks then one cascade delete; a hook error aborts.
    async fn delete_session(&self, user_id: &str, session_id: &str) -> Result<(), AppError>;

    // ---- transcript + tool cards ----
    /// Mints `seq` inside the insert (BR2.1), touches `last_activity_at`.
    async fn append_message(
        &self,
        user_id: &str,
        input: AppendMessage,
    ) -> Result<OxpilotMessageRow, AppError>;
    /// Full transcript for the session — `{complete, error}` rows only
    /// (BR2.3); `streaming` stays out (in-flight).
    async fn transcript(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<OxpilotMessageRow>, AppError>;
    /// The UNFILTERED message list (`streaming` included) — the SSE
    /// snapshot's message array needs the in-flight assistant row so a
    /// remount overlays the live tail onto it (BR4.4/C2). Prompt-side
    /// consumers call [`SessionService::transcript`] instead.
    async fn list_messages(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<OxpilotMessageRow>, AppError>;
    /// Transcript half of the BR2.2 union dedupe — no status filter, a
    /// `streaming` row still counts as the dedupe hit.
    async fn find_message_by_client_msg_id(
        &self,
        user_id: &str,
        session_id: &str,
        client_msg_id: &str,
    ) -> Result<Option<OxpilotMessageRow>, AppError>;
    /// Streaming accumulation — the turn task rewrites the assistant row.
    async fn set_message_content(
        &self,
        user_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<(), AppError>;
    async fn set_message_status(
        &self,
        user_id: &str,
        message_id: &str,
        status: &str,
    ) -> Result<(), AppError>;

    // ---- follow-up queue (US1.8) ----
    /// Union dedupe transcript-first-then-queue (BR2.2); a transcript hit
    /// returns `{null, null}`, a queued hit returns the existing receipt.
    async fn enqueue(&self, user_id: &str, input: Enqueue) -> Result<QueuedReceipt, AppError>;
    /// Read-only resend probe — the same union dedupe `enqueue` runs,
    /// exposed so `message_queue` can answer a resend WITHOUT admitting
    /// budget first (BR2.2 + BR6.2 take two: the admission reservation
    /// mutates counters, so probing after `admit_budget` would charge
    /// every idempotent resend). `None` ⇒ no prior receipt, the caller
    /// admits and enqueues. Runs under the session admission lock so
    /// the probe serializes with concurrent enqueues.
    async fn peek_queued_receipt(
        &self,
        user_id: &str,
        session_id: &str,
        client_msg_id: &str,
    ) -> Result<Option<QueuedReceipt>, AppError>;
    /// Atomic claim+delete+append (BR4.2) — the appended transcript rows
    /// in delivery order.
    async fn drain_queue(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<OxpilotMessageRow>, AppError>;
    /// Non-consuming queue read — `queue_list` + snapshot `pending_queue`.
    async fn list_queue(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<OxpilotQueuedRow>, AppError>;
    /// U8's inline queued-message cancel (`queue_delete`): the owned
    /// session read runs first (foreign/absent ⇒ `not_found`, BR3.3),
    /// then the `(session_id, id)` pair delete; an already-drained row
    /// is `not_found` too (stale-ref honesty — the drained row resolves
    /// on the client's next `queue_list`, BR9.1).
    async fn delete_queued(
        &self,
        user_id: &str,
        session_id: &str,
        queued_id: &str,
    ) -> Result<(), AppError>;

    // ---- tool-call cards (BR7.3) ----
    async fn append_tool_call(
        &self,
        user_id: &str,
        input: ToolCallWrite,
    ) -> Result<OxpilotToolCallRow, AppError>;
    /// The C2 snapshot merge source — `(seq, call_index)` order.
    async fn list_tool_calls(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<OxpilotToolCallRow>, AppError>;
    /// `running → done|error` (+ output summary); unknown `call_id` ⇒ `not_found`.
    async fn set_tool_call_status(
        &self,
        user_id: &str,
        input: ToolCallStatusUpdate,
    ) -> Result<(), AppError>;

    // ---- proposals (WF8 — the U5 diff surface's store) ----
    async fn save_proposal(
        &self,
        user_id: &str,
        input: ProposalWrite,
    ) -> Result<OxpilotProposalRow, AppError>;
    /// Id-keyed owned-scope read — `not_found` cross-user (fetch-then-check).
    async fn proposal(
        &self,
        user_id: &str,
        proposal_id: &str,
    ) -> Result<OxpilotProposalRow, AppError>;
    /// `pending → applied` + `pr_ref`; fetch-then-check on `session_id`.
    async fn mark_applied(
        &self,
        user_id: &str,
        proposal_id: &str,
        pr_ref: &str,
    ) -> Result<OxpilotProposalRow, AppError>;

    // ---- usage + budget (US5.2) ----
    /// Dual-scope UTC-day rollup — `(user, user_id)` +
    /// `(instance, '__instance__')` in one transaction (BR6.1).
    async fn record_usage(&self, user_id: &str, delta: UsageDelta) -> Result<(), AppError>;
    /// Either axis at/over its configured cap ⇒ `budget_exceeded` with
    /// `data:{scope}` — user breach reported first (BR6.2); absent caps
    /// are unbounded (BR6.3). Read-only — the atomic admission is
    /// [`admit_budget`](Self::admit_budget).
    async fn check_budget(&self, user_id: &str, caps: BudgetCaps) -> Result<(), AppError>;
    /// Atomic budget admission — reserves one request on both scopes
    /// under `caps` in a single transaction (BR6.2 take two: the
    /// read-then-spend gap in `check_budget` let concurrent senders
    /// overspend). Called only at the true admission points — the chat
    /// POST after every release-armed exit, and `message_queue` inside
    /// the admission lock — so a refused or deduped send never burns
    /// budget and turn-end accounting stays token-only. Same
    /// `budget_exceeded`/`data:{scope}` refusal shape as `check_budget`.
    async fn admit_budget(&self, user_id: &str, caps: BudgetCaps) -> Result<(), AppError>;
    /// Token-axis-only budget read for the post-usage chain gates —
    /// every queued row already reserved its request at admission
    /// ([`admit_budget`](Self::admit_budget)), so re-applying the
    /// request axis there would strand an admitted follow-up behind
    /// its own reservation. Same `budget_exceeded`/`data:{scope}`
    /// refusal shape as `check_budget`.
    async fn check_token_budget(&self, user_id: &str, caps: BudgetCaps) -> Result<(), AppError>;
    /// The `usage_get` backing read — fills all five C1 fields and NEVER
    /// refuses: it is the spend surface itself.
    async fn usage_view(&self, user_id: &str, caps: BudgetCaps) -> Result<UsageView, AppError>;
}

// ---------- connector gateway port (C3/C7, U6) ----------

/// Spec pins (functional-spec §Pins) — the TTLs, bounds, and liveness
/// window every connector decision shares. The impl computes all
/// `now`/expiry stamps from these; nothing else mints connector
/// timespans.
pub const PAIRING_CODE_TTL_SECS: i64 = 10 * 60;
pub const ACTION_GRANT_TTL_SECS: i64 = 5 * 60;
pub const CONNECT_GRANT_TTL_SECS: i64 = 10 * 60;
/// `/claim`'s `wait_seconds` clamp (C3 — `>60` clamps, never rejects).
pub const CLAIM_WAIT_MAX_SECS: u64 = 60;
/// `TargetView.status: online` iff `last_seen_at` is inside this window
/// (BR5.4/NFR3.8 — ≈3× the documented client heartbeat interval).
pub const HEARTBEAT_LIVENESS_WINDOW_SECS: i64 = 90;
/// Dequeue batch cap — FIFO `(created_at, action_id)` scan bound (BR5.1).
pub const CLAIM_DEQUEUE_MAX: i64 = 50;
/// Field bounds, all redacted BEFORE write (BR7.4).
pub const ACTION_ARGS_MAX: usize = 2 * 1024;
pub const SUMMARY_MAX: usize = 512;
pub const RESULT_MAX: usize = 2 * 1024;

/// The closed capability vocabulary (BR1.1's `⊆` bound) — a minted
/// pairing can never carry a GUI cap.
pub const CONNECTOR_CAPABILITIES: [&str; 3] = ["exec_shell", "fs_read", "fs_write"];

/// The C7-pinned denial reasons (`data:{denied:{reason, grant_id?}}`).
pub mod denied_reason {
    /// Target is `pending_consent`/`disconnected`/`revoked` (or absent
    /// for the owner) — no live channel exists.
    pub const NOT_CONNECTED: &str = "not_connected";
    /// `connected` but `last_seen_at` is outside the liveness window.
    pub const OFFLINE: &str = "offline";
    /// `kind ∉ target.caps` — the granted capability set won't cover it.
    pub const CAPABILITY: &str = "capability";
    /// No live `action_session` grant covers the dispatch — `grant_id`
    /// names the pending action grant the consent card resolves.
    pub const CONSENT_REQUIRED: &str = "consent_required";
}

/// Mint the C7-pinned dispatch refusal — an `AppError` whose `data`
/// carries `{denied:{reason, grant_id?}}`. U5 distinguishes consent
/// refusals from hard errors by the `denied` marker, never by type or
/// message sniffing (BR4.4); C5 passthroughs (`not_found`/`forbidden`)
/// ride the same channel as their own codes.
pub fn denied(reason: &str, grant_id: Option<&str>) -> AppError {
    let mut body = serde_json::Map::new();
    body.insert("reason".to_string(), json!(reason));
    if let Some(g) = grant_id {
        body.insert("grant_id".to_string(), json!(g));
    }
    AppError::new("oxpilot.action_denied", format!("action denied: {reason}"))
        .with_data(json!({ "denied": Value::Object(body) }))
}

/// C3's resolved connector credential scope (entities.md
/// `ConnectorIdentity`, BR2.4) — the SOLE scoping source on connector
/// routes; a body's `target_id` is validated against it, never trusted.
#[derive(Debug, Clone)]
pub struct ConnectorIdentity {
    pub target_id: String,
    /// The owning user — every row write/audit stamps this, never the
    /// request body (AC5.5.4).
    pub user_id: String,
    /// The target's granted caps (stored JSON array) — claim
    /// intersects these, never widens.
    pub caps: String,
    /// `pending_consent | connected | disconnected | revoked` — carried
    /// so heartbeat can answer `revoked:true` on a still-authenticating
    /// revoked credential (BR2.2).
    pub status: String,
}

/// `oxpilot.pairing_create` payload (BR1.1) — the ONLY response that
/// ever carries plaintext code material; the hash alone persists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PairingView {
    /// The mint's id — the pairing→target correlation handle the
    /// connect dialog polls `target_list`/`target_get` against.
    pub pairing_id: String,
    /// Shown ONCE — never stored, logged, or returned again (BR7.3).
    pub pairing_code: String,
    pub expires_at: String,
}

/// `oxpilot.target_list`/`target_get` payload member (C1 + the
/// `lifecycle`/`pairing_id` additive amendment). Never carries
/// `verifier_hash`, raw verifier material, or `last_seen_at`
/// (NFR1.11/BR7.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TargetView {
    pub id: String,
    /// The target's display name (`device_label` fallback
    /// `target_name` from the pairing).
    pub label: String,
    /// `online | offline | revoked` — derived at read (NFR3.8):
    /// `revoked` lifecycle → `revoked`; `connected` + inside the
    /// liveness window → `online`; every other shape → `offline`.
    pub status: String,
    /// The persisted lifecycle verbatim:
    /// `pending_consent | connected | disconnected | revoked`.
    pub lifecycle: String,
    /// The mint that created it — the dialog's correlation handle.
    pub pairing_id: String,
    /// The granted capability set (parsed from the stored JSON array).
    pub granted_caps: Vec<String>,
}

/// `oxpilot.grant_*` payload member (C1). `action_once`/
/// `action_session` collapse to `action` on the wire; `action_args`
/// NEVER serializes — the consent card renders `args_summary` only
/// (NFR1.15/BR7.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrantView {
    pub id: String,
    /// `connect | action` — C1's collapsed two-member enum.
    pub kind: String,
    /// `connect` → the target's caps summary; `action` → `action_kind`.
    pub capability: String,
    /// The bounded redacted consent-card excerpt (action grants only).
    pub args_summary: Option<String>,
    /// The grant's bound session — NULL on connect grants (target-scoped,
    /// BR6.6). Lets a session-scoped reader verify the response's scope
    /// instead of trusting an unamended server to refuse `{session_id}`.
    pub session_id: Option<String>,
    /// `pending | allowed | denied | expired | cancelled | revoked`.
    pub state: String,
    pub expires_at: String,
}

/// The C7 `action_status` return — U5's public trim; connector
/// credentials, target refs, and claim cursors never present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionView {
    pub action_id: String,
    pub kind: String,
    /// `queued | claimed | running | done | failed |
    /// cancel_requested | cancelled`.
    pub state: String,
    pub created_at: String,
    /// The bounded redacted args excerpt stored at dispatch.
    pub args_summary: String,
    /// The bounded redacted report excerpt — `null` until terminal
    /// (C7 additive amendment).
    pub result_summary: Option<String>,
}

/// `/pair` 200 body (BR1.2) — the verifier's single custody surface.
#[derive(Debug, Clone, Serialize)]
pub struct RedeemedPairing {
    /// `connector_id == target_id` in v1 (the C3 sketch carries both
    /// names — U7 emits one value for both).
    pub connector_id: String,
    pub target_id: String,
    /// Raw verifier — shown once, stored hashed, never returned again.
    pub verifier: String,
}

/// One dequeued action on a `/claim` 200 — `args` is the stored
/// already-redacted form delivered verbatim (BR5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClaimAction {
    pub action_id: String,
    pub kind: String,
    pub args: String,
    pub grant_id: String,
}

/// `/claim` outcome — `Batch` serializes to the C3 200 body;
/// `Timeout` maps to `204` + a `Retry-After` header (no body).
#[derive(Debug, Clone)]
pub enum ClaimOutcome {
    Batch {
        actions: Vec<ClaimAction>,
        /// `cancel_requested` action ids — re-announced until closed.
        cancel: Vec<String>,
        retry_after: u32,
    },
    Timeout {
        retry_after: u32,
    },
}

/// `/heartbeat` 200 body (BR5.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HeartbeatOutcome {
    pub revoked: bool,
    /// All `cancel_requested` action ids on this target — re-announced
    /// every heartbeat until they close (kill semantics idempotent).
    pub cancel: Vec<String>,
}

/// The connector-gateway seam (C7 surface + the C3/proc backing
/// verbs) — ONE trait object installed beside `sessions` on
/// [`OxpilotRuntime`] (set-once `OnceLock`, survives slot rebuilds).
/// U5's `ToolExecutor` consumes `dispatch_action`/`action_status`; the
/// owner procs and C3 routes consume the rest — one impl, one state
/// machine (the one-gateway rule).
///
/// Error conventions: owner reads answer `not_found` on cross-user OR
/// absent ids (BR7.1 anti-enumeration); connector auth failures are
/// mapped at the route layer; `dispatch_action`'s refusals are
/// [`denied`]-shaped `AppError`s while session checks pass through
/// `not_found`/`forbidden` as their own codes (BR4.4).
///
/// Every verb takes `client: &ClientMeta` — the request metadata the
/// post-commit `audit::record_with` rows stamp. Server-side callers
/// (U5 dispatch, the C8 hook) pass a default-constructed meta — the
/// events they emit are process-internal, so IP/UA stay absent rather
/// than fabricated.
#[async_trait]
pub trait ConnectorGateway: Send + Sync {
    // ---- U5 dispatch (C7) ----
    /// BR4.1 ordered checks under the per-target lock: session
    /// ownership → target lifecycle+liveness → caps → live grant →
    /// pending-grant dedupe → INSERT `queued` + audit. Consent
    /// refusals return [`denied`]-shaped errors.
    async fn dispatch_action(
        &self,
        client: &ClientMeta,
        user_id: &str,
        session_id: &str,
        target_id: &str,
        kind: &str,
        args: &str,
    ) -> Result<OxpilotActionRow, AppError>;
    /// Owner-checked read — `not_found` on cross-user (BR4.4).
    async fn action_status(
        &self,
        client: &ClientMeta,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionView, AppError>;

    // ---- owner verbs (C1 procs) ----
    /// BR1.1 mint — `caps ⊆ CONNECTOR_CAPABILITIES` enforced, code ≥
    /// 160-bit entropy, `sha256(code)` alone persists, per-user mint
    /// limiter (BR1.5).
    async fn mint_pairing(
        &self,
        client: &ClientMeta,
        user_id: &str,
        target_name: &str,
        caps: &[String],
    ) -> Result<PairingView, AppError>;
    /// Owner reads — BR3.2's lazy pending-connect re-mint rides any
    /// read of a `pending_consent` target.
    async fn list_targets(
        &self,
        client: &ClientMeta,
        user_id: &str,
    ) -> Result<Vec<TargetView>, AppError>;
    async fn get_target(
        &self,
        client: &ClientMeta,
        user_id: &str,
        target_id: &str,
    ) -> Result<TargetView, AppError>;
    /// BR6.1 — verifier tombstone + direct `cancelled` closes (dead
    /// channel). Idempotent on already-terminal targets (BR6.7).
    async fn disconnect_target(
        &self,
        client: &ClientMeta,
        user_id: &str,
        target_id: &str,
    ) -> Result<(), AppError>;
    /// BR6.2 — verifier preserved (`revoked:true` still authenticates),
    /// grants `revoked`, in-flight `cancel_requested` for heartbeat
    /// delivery. Idempotent (BR6.7).
    async fn revoke_target(
        &self,
        client: &ClientMeta,
        user_id: &str,
        target_id: &str,
    ) -> Result<(), AppError>;
    /// Pending-first then recent resolved, optionally narrowed to one
    /// target (WF6); lazy expiry stamps on read. U8 adds an optional
    /// `session_id` — when present the read first resolves session
    /// ownership (`not_found` for foreign/absent ids alike, BR3.3),
    /// then narrows to that session's bound grants; `target_id` ANDs
    /// alongside it when both arrive.
    async fn list_grants(
        &self,
        client: &ClientMeta,
        user_id: &str,
        target_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<Vec<GrantView>, AppError>;
    async fn get_grant(
        &self,
        client: &ClientMeta,
        user_id: &str,
        grant_id: &str,
    ) -> Result<GrantView, AppError>;
    /// BR3.5 — `pending` → `allowed` (scope stamps `action_once`/
    /// `action_session` + materializes the Action, BR4.2) or `denied`;
    /// `allowed action_session + deny` is the BR6.4 revoke arm; every
    /// other state/kind → `grant_not_pending`.
    async fn respond_grant(
        &self,
        client: &ClientMeta,
        user_id: &str,
        grant_id: &str,
        allow: bool,
        scope: Option<&str>,
    ) -> Result<GrantView, AppError>;

    // ---- connector verbs (C3 routes) ----
    /// BR1.2 redeem — expiry checked FIRST (`pairing_expired`), then
    /// `state≠pending` (`pairing_used`), caps subset (`forbidden`); one
    /// transaction claims + mints target + verifier + connect grant.
    async fn redeem_pairing(
        &self,
        client: &ClientMeta,
        pairing_code: &str,
        device_label: &str,
        requested_caps: &[String],
    ) -> Result<RedeemedPairing, AppError>;
    /// Bearer → sha256 → `verifier_hash` lookup →
    /// [`ConnectorIdentity`]. Absent/tombstoned ⇒ `unauthorized`;
    /// `revoked` still resolves (BR2.2). The impl owns the hash — raw
    /// material never reaches storage. The gateway owns the
    /// connector-auth IP bucket: check BEFORE the hash lookup,
    /// record+audit (`connector_auth_failed`) on a miss.
    async fn resolve_verifier(
        &self,
        client: &ClientMeta,
        verifier: &str,
    ) -> Result<ConnectorIdentity, AppError>;
    /// BR5.1 long-poll — arms the notify waiter BEFORE dequeue, FIFO
    /// scan capped at [`CLAIM_DEQUEUE_MAX`], grant+caps re-checked
    /// per row, dead-grant rows cancel+skip, `claimed→running` touch
    /// proof + `last_seen_at` on every call.
    async fn claim(
        &self,
        client: &ClientMeta,
        identity: &ConnectorIdentity,
        wait_seconds: u64,
        capabilities: &[String],
    ) -> Result<ClaimOutcome, AppError>;
    /// BR5.2 — idempotent terminal close by `action_id`; cross-target
    /// or unknown ids answer `not_found` (kill-locally signal);
    /// revoked targets may ONLY take this close path (BR2.2).
    async fn report(
        &self,
        client: &ClientMeta,
        identity: &ConnectorIdentity,
        action_id: &str,
        status: &str,
        exit_code: Option<i64>,
        output: &str,
    ) -> Result<(), AppError>;
    /// BR5.3 — `last_seen_at` touch + `claimed→running` flip + the
    /// re-announced cancel set; `{revoked:true}` on revoked.
    async fn heartbeat(
        &self,
        client: &ClientMeta,
        identity: &ConnectorIdentity,
    ) -> Result<HeartbeatOutcome, AppError>;

    // ---- C8 hook backing ----
    /// BR6.5 — session-scoped grants `pending|allowed` → `cancelled`
    /// and every non-terminal action → `cancelled` directly (rows
    /// cascade-delete; `cancel_requested` could never deliver).
    /// Connect grants (`session_id NULL`) survive (BR6.6). Idempotent;
    /// `Err` aborts the delete (C8).
    async fn on_session_delete(
        &self,
        client: &ClientMeta,
        session_id: &str,
        user_id: &str,
    ) -> Result<(), AppError>;
}

/// Per-turn broadcast ring capacity (NFR3).
pub const TURN_FANOUT_CAPACITY: usize = 256;

/// Wire frame on a turn's fan-out channel — the full frame JSON including
/// `type`, so readers serialize it verbatim into the SSE `data:` line.
pub type TurnFrame = Value;

/// In-memory, per-process turn marker (entities.md `ActiveTurn`) — the
/// owner of the one-live-turn-per-session invariant. Never persisted; a
/// process-death orphan is swept to `error` at boot
/// (`Database::sweep_oxpilot_streaming_messages`).
pub struct ActiveTurn {
    /// Map key — the session this turn streams on.
    pub session_id: String,
    /// Fresh id minted at claim — distinct from `message_id`.
    pub turn_id: String,
    /// The streaming assistant row id.
    pub message_id: String,
    /// `seq` of the user row that opened the turn. Minted after the claim
    /// wins (WF3.1) so a lost race can never leak a stale seq into the
    /// unique index — `0` until the winner stores its minted value.
    pub started_seq: std::sync::atomic::AtomicI64,
    /// The `client_msg_id` the claiming POST carried — lets a resend that
    /// lands in the claim→insert window recognize its own turn.
    pub pending_client_msg_id: Option<String>,
    /// The redacted user content the claiming POST carried — same purpose.
    pub pending_content: String,
    /// Frame publisher — POST task publishes, GET readers subscribe.
    pub fanout: broadcast::Sender<TurnFrame>,
    /// Buffered `append` frames plus the `terminal` frame — replayed to
    /// mid-turn attachers so exactly-once holds, and so an attach landing
    /// after the turn's terminal still ends on a terminal frame (C2).
    pub emitted_deltas: Mutex<Vec<TurnFrame>>,
    /// Fires on explicit abort (U3), session-delete hook, or shutdown —
    /// NOT on transport close (detach-to-complete).
    pub cancel: CancellationToken,
    /// Per-session admission mutex — `message_queue`'s live-check+enqueue
    /// and the wrapper's `release`→recheck-drain serialize on it so no
    /// row can commit inside the release→recheck blind window (BR3.4,
    /// R-20's airtight seam). Stored on the ENTRY and dropped with it.
    pub admission_lock: Arc<Mutex<()>>,
    /// First-wins terminal guard — `emit_terminal` CASes this inside the
    /// `emitted_deltas` lock so exactly one terminal frame ever enters
    /// buffer + fanout (BR5.4).
    pub terminal_sent: std::sync::atomic::AtomicBool,
    /// Chain depth — `0` on the POST-minted turn; `drain_and_chain`
    /// passes `generation + 1` into the successor claim and refuses past
    /// `MAX_CHAIN_DEPTH` (BR4.3).
    pub generation: u32,
}

/// The concurrent map keyed `session_id` — insert-if-absent under one lock
/// is the atomic 409 claim (BR4.2).
#[derive(Default)]
pub struct ActiveTurns {
    map: Mutex<HashMap<String, Arc<ActiveTurn>>>,
}

impl ActiveTurns {
    /// Atomic insert-if-absent claim. Returns `None` when a turn is
    /// already live on the session (callers answer 409 `turn_active`).
    /// `mint` builds the entry while the lock is held so a mid-window
    /// `snapshot.active_turn` is always fully formed.
    pub async fn claim(
        &self,
        session_id: &str,
        mint: impl FnOnce() -> ActiveTurn,
    ) -> Option<Arc<ActiveTurn>> {
        let mut map = self.map.lock().await;
        if map.contains_key(session_id) {
            return None;
        }
        let turn = Arc::new(mint());
        map.insert(session_id.to_string(), turn.clone());
        Some(turn)
    }

    /// Read the live turn for a session (snapshot `active_turn` source).
    pub async fn get(&self, session_id: &str) -> Option<Arc<ActiveTurn>> {
        self.map.lock().await.get(session_id).cloned()
    }

    /// The `(entry, admission lock)` pair for a session — the PAIR
    /// returns so a stale guard re-verifies the entry's identity under
    /// the lock: a turn released out from under a waiting holder can
    /// never mint a false liveness read (R-13c). `None` ⇒ no live
    /// turn — `message_queue` answers `session_active_turn_absent`.
    pub async fn admission_lock(
        &self,
        session_id: &str,
    ) -> Option<(Arc<ActiveTurn>, Arc<Mutex<()>>)> {
        let turn = self.map.lock().await.get(session_id).cloned()?;
        Some((turn.clone(), turn.admission_lock.clone()))
    }

    /// Release the claim when the turn task ends. Guards against removing a
    /// *different* turn that claimed the same session later.
    pub async fn release(&self, session_id: &str, turn_id: &str) {
        let mut map = self.map.lock().await;
        if map.get(session_id).is_some_and(|t| t.turn_id == turn_id) {
            map.remove(session_id);
        }
    }
}

// ---------- runtime slot (U4) ----------

/// Where the effective API key came from for a built provider — part of
/// the build-origin tuple so a custody change (env→db, db→cleared) counts
/// as an identity change and never carries a stale Arc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// `OXIDEAN_OXPILOT_API_KEY` supplied it.
    Env,
    /// The settings row's `key_ciphertext` decrypted at build time.
    Db,
    /// No key material (stub/ollama, or a key-less misconfig).
    None,
}

/// The resolved tuple a slot's provider was built from — the carry test
/// for the no-build write path (NFR4.3): a flag/budget-only write keeps
/// the installed Arc ONLY while the row still describes this exact build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderIdentity {
    pub provider_id: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub key_source: KeySource,
}

/// One live provider configuration — swapped as a unit under the
/// runtime's write lock. Invariant: `provider: None` is always paired
/// with `disabled_reason` (the gate and `oxpilot.status` read the same
/// field); a `flag_off` slot may still hold a live Arc so a flag-on write
/// carries it without rebuilding (NFR4.5 carry semantics).
#[derive(Clone)]
pub struct OxpilotRuntimeSlot {
    pub provider: Option<Arc<dyn LlmProvider>>,
    pub model: Option<String>,
    /// `flag_off | no_provider | misconfigured` — `None` only on a live,
    /// enabled provider slot.
    pub disabled_reason: Option<String>,
    /// The one-live-turn-per-session map — the SAME Arc rides across
    /// rebuilds so in-flight turns survive provider swaps (NFR4.5).
    pub turns: Arc<ActiveTurns>,
    /// The resolved tuple `provider` was built from — `None` on
    /// never-built or dropped-provider slots.
    pub built_from: Option<ProviderIdentity>,
    /// The resolved budget axes (US5.2) — populated at EVERY constructor
    /// (boot env parse, resolve/carry/build/disabled paths) so
    /// `check_budget` callers read `slot.budget_caps` without reaching
    /// back into `EffectiveProviderConfig` (C5 amendment).
    pub budget_caps: BudgetCaps,
    /// The C6 tool-execution port (U3-provided, U5-consumed) — `None`
    /// maps to [`NullTools`] at `turn_parts()` time so the pinned
    /// executor triple stays non-Option. Carried like `turns`: a
    /// provider rebuild or flag-off must not drop a live executor.
    pub tools: Option<Arc<dyn ToolExecutor>>,
}

/// Reason-bearing disabled slot — the no-build path's terminal shape for
/// `no_provider`/`misconfigured` (reliability-design `disabled_slot`).
/// `built_from` clears with the provider so a later carry can never
/// resurrect it. `caps` is the resolved budget pair — a disabled slot
/// still carries the configured posture (unbounded callers pass
/// [`BudgetCaps::default`]). `tools` carries independently of the
/// provider — a disabled slot keeps whatever executor was installed.
pub fn disabled_slot(
    turns: Arc<ActiveTurns>,
    reason: &str,
    caps: BudgetCaps,
    tools: Option<Arc<dyn ToolExecutor>>,
) -> OxpilotRuntimeSlot {
    OxpilotRuntimeSlot {
        provider: None,
        model: None,
        disabled_reason: Some(reason.to_string()),
        turns,
        built_from: None,
        budget_caps: caps,
        tools,
    }
}

/// The mutable runtime handle — one RwLock'd slot plus ONE rebuild mutex
/// (NFR4.2: `settings::rebuild`/`settings_set` serialize their
/// resolve → validate → build → persist → swap → audit section on
/// `rebuild_lock`). Request-path readers take `slot.read()` alone — the
/// hot-path gate never touches the DB, and the only lock order is
/// `rebuild_lock` → `slot.write()` for a single assignment.
///
/// `sessions` is the U2 service seam — the RUNTIME owns it (not the
/// rebuildable slot): a settings swap never reinstalls the persistence
/// port. Installed once at shell assembly via [`OxpilotRuntime::install_sessions`].
pub struct OxpilotRuntime {
    pub slot: RwLock<OxpilotRuntimeSlot>,
    pub rebuild_lock: Mutex<()>,
    /// The `SessionService` trait object — set-once `OnceLock` so a late
    /// `install_sessions` can't swap the seam under in-flight callers.
    /// Private: all access flows through [`OxpilotRuntime::install_sessions`]
    /// /[`OxpilotRuntime::sessions`] (the `oxpilot.internal` mapping lives
    /// there and must not be bypassed).
    sessions: OnceLock<Arc<dyn SessionService>>,
    /// The `ConnectorGateway` trait object (C7, U6) — same set-once
    /// shape as `sessions`: NOT the rebuildable slot, so a settings
    /// rebuild can never drop the seam under in-flight dispatchers.
    gateway: OnceLock<Arc<dyn ConnectorGateway>>,
}

/// `Arc<OxpilotRuntime>` for `AppState`/`RpcCtx` — shared across ctx
/// clones because both fields are handle-only state.
pub type SharedOxpilot = Arc<OxpilotRuntime>;

impl OxpilotRuntime {
    /// Sync boot construction from `OXIDEAN_OXPILOT_*` env (U1 parity —
    /// `AppState::new` can't await a DB read or provider build):
    /// `stub` installs [`StubProvider`]; an env provider value api-core
    /// can't build lands `misconfigured` until `settings::rebuild`
    /// (unconditional, right after `AppState::new`) resolves it through
    /// the api-oxpilot registry; unset ⇒ `no_provider`. The flag env is
    /// stamped into `disabled_reason` so the reason invariant holds even
    /// before the first rebuild.
    pub fn new() -> SharedOxpilot {
        let turns = Arc::new(ActiveTurns::default());
        let flag_on = flag_env_on();
        let env = std::env::var("OXIDEAN_OXPILOT_PROVIDER")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let model_env = std::env::var("OXIDEAN_OXPILOT_MODEL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        // US5.2 — boot-time budget envs; U4's rebuild overwrites them
        // with the row-over-env resolved caps.
        let parse_budget = |name: &str| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.trim().parse::<i64>().ok())
        };
        let budget_caps = BudgetCaps {
            user: parse_budget("OXIDEAN_OXPILOT_USER_BUDGET"),
            instance: parse_budget("OXIDEAN_OXPILOT_INSTANCE_BUDGET"),
        };
        let (provider, model, mut misconfigured) = match env.as_deref() {
            Some("stub") => (
                Some(Arc::new(StubProvider { fail: false }) as Arc<dyn LlmProvider>),
                Some(model_env.unwrap_or_else(|| "stub-1".to_string())),
                false,
            ),
            Some(_) => (None, None, true),
            None => (None, None, false),
        };
        let built_from = provider.as_ref().map(|_| ProviderIdentity {
            provider_id: "stub".to_string(),
            model: model.clone(),
            base_url: None,
            key_source: KeySource::None,
        });
        if !flag_on {
            // flag_off wins the reason (BR2.1 precedence) even when the
            // env provider is also unusable.
            misconfigured = false;
        }
        let disabled_reason = if !flag_on {
            Some("flag_off".to_string())
        } else if misconfigured {
            tracing::warn!("oxpilot disabled: unrecognized OXIDEAN_OXPILOT_PROVIDER value");
            Some("misconfigured".to_string())
        } else if provider.is_none() {
            Some("no_provider".to_string())
        } else {
            None
        };
        Arc::new(OxpilotRuntime {
            slot: RwLock::new(OxpilotRuntimeSlot {
                provider,
                model,
                disabled_reason,
                turns,
                built_from,
                budget_caps,
                tools: None,
            }),
            rebuild_lock: Mutex::new(()),
            sessions: OnceLock::new(),
            gateway: OnceLock::new(),
        })
    }

    /// Test/dev seam — install a slot directly (`with_oxpilot` hook).
    pub fn from_slot(slot: OxpilotRuntimeSlot) -> SharedOxpilot {
        Arc::new(OxpilotRuntime {
            slot: RwLock::new(slot),
            rebuild_lock: Mutex::new(()),
            sessions: OnceLock::new(),
            gateway: OnceLock::new(),
        })
    }

    /// One-time install of the U2 `SessionService` seam — set-once
    /// semantics: the first call wins and a later call is a no-op, so
    /// the port can never swap under in-flight callers (C5/C8).
    pub fn install_sessions(&self, svc: Arc<dyn SessionService>) {
        let _ = self.sessions.set(svc);
    }

    /// The installed `SessionService` — `oxpilot.internal` until shell
    /// assembly calls [`OxpilotRuntime::install_sessions`].
    pub fn sessions(&self) -> Result<Arc<dyn SessionService>, AppError> {
        self.sessions.get().cloned().ok_or_else(|| {
            AppError::new("oxpilot.internal", "oxpilot session service not installed")
        })
    }

    /// One-time install of the U6 `ConnectorGateway` seam — identical
    /// set-once semantics to `sessions` (first call wins; the port can
    /// never swap under in-flight dispatchers).
    pub fn install_gateway(&self, gw: Arc<dyn ConnectorGateway>) {
        let _ = self.gateway.set(gw);
    }

    /// The installed `ConnectorGateway` — `oxpilot.internal` until
    /// shell assembly calls [`OxpilotRuntime::install_gateway`].
    pub fn gateway(&self) -> Result<Arc<dyn ConnectorGateway>, AppError> {
        self.gateway.get().cloned().ok_or_else(|| {
            AppError::new(
                "oxpilot.internal",
                "oxpilot connector gateway not installed",
            )
        })
    }

    /// Install the U5 `ToolExecutor` onto the runtime slot — carried
    /// like `turns`: a later provider rebuild or flag-off must not
    /// drop the live executor (`disabled_slot` carries it through).
    /// Unlike `sessions`/`gateway` this is a slot field (the U3 seam),
    /// so it rides `slot.write()` rather than a `OnceLock`.
    pub async fn install_tools(&self, tools: Arc<dyn ToolExecutor>) {
        self.slot.write().await.tools = Some(tools);
    }

    /// The shared turn map — the same Arc across every slot rebuild.
    pub async fn turns(&self) -> Arc<ActiveTurns> {
        self.slot.read().await.turns.clone()
    }

    /// Capture the `(provider, model, executor)` triple for a turn start
    /// — ONE slot-lock read so a mid-turn settings swap can't tear the
    /// set the in-flight turn is running on (NFR4.5 detach-to-complete
    /// + the C6 pinning amendment). `tools: None` maps to `NullTools`.
    ///
    /// A disabled slot supplies NO provider: chained successors (queue
    /// drain → `spawn_turn_task`) start without the POST route gate, so
    /// the check lives here — `run_turn` maps the missing provider to
    /// the `oxpilot.disabled` terminal either way.
    pub async fn turn_parts(&self) -> TurnParts {
        let slot = self.slot.read().await;
        TurnParts {
            provider: if slot.disabled_reason.is_none() {
                slot.provider.clone()
            } else {
                None
            },
            model: slot.model.clone(),
            executor: slot.tools.clone().unwrap_or_else(|| Arc::new(NullTools)),
        }
    }
}

/// The pinned per-turn triple (`turn_parts()` — Q4): the provider Arc +
/// model the turn streams on and the executor its tool calls route to,
/// captured atomically at turn start.
pub struct TurnParts {
    pub provider: Option<Arc<dyn LlmProvider>>,
    /// `None` ⇒ terminal `{result:"error", code:"no_model"}` before any
    /// stream opens — a provider with no resolved model is a typed
    /// refusal, never a fabricated request (NFR4.9).
    pub model: Option<String>,
    /// The C6 port — `Arc<dyn ToolExecutor>` non-Option; the slot's
    /// `None` already mapped to `NullTools`.
    pub executor: Arc<dyn ToolExecutor>,
}

// ---------- enablement ----------

/// Request-time enablement view (entities.md `EnablementState`) — read
/// off the current slot, never cached beyond the read.
#[derive(Debug, Clone)]
pub struct EnablementState {
    pub enabled: bool,
    pub flag_on: bool,
    /// `{id, model}` when a provider occupies the slot — `model` is
    /// `None` for no-advertised-list providers without a configured
    /// model (never fabricated).
    pub provider: Option<(String, Option<String>)>,
    /// `flag_off | misconfigured | no_provider` — `None` when enabled.
    pub reason: Option<String>,
}

/// Env flag read — `OXIDEAN_OXPILOT_ENABLED` defaults OFF when unset.
/// Only the boot/`rebuild` resolvers consume this; request-path
/// resolution reads the slot's stamped reason.
fn flag_env_on() -> bool {
    std::env::var("OXIDEAN_OXPILOT_ENABLED")
        .map(|v| {
            let t = v.trim().to_ascii_lowercase();
            !(t.is_empty() || t == "0" || t == "false" || t == "no" || t == "off")
        })
        .unwrap_or(false)
}

/// Resolve `EnablementState` for a request — an async slot read; a `None`
/// runtime (test-built ctx without the field) resolves `no_provider`.
pub async fn resolve_enablement(ctx: &RpcCtx) -> EnablementState {
    match ctx.oxpilot.as_ref() {
        Some(runtime) => resolve_enablement_slot(&*runtime.slot.read().await),
        None => EnablementState {
            enabled: false,
            flag_on: flag_env_on(),
            provider: None,
            reason: Some("no_provider".to_string()),
        },
    }
}

/// Slot-level resolution — the SSE edge routes hold `AppState`, not an
/// `RpcCtx`, so they resolve against a slot read guard directly. The
/// reason precedence `flag_off > misconfigured > no_provider` is already
/// encoded in `disabled_reason` by whoever stamped the slot.
pub fn resolve_enablement_slot(slot: &OxpilotRuntimeSlot) -> EnablementState {
    let provider = slot
        .provider
        .as_ref()
        .map(|p| (p.id().to_string(), slot.model.clone()));
    // Defensive backstop — the invariant says provider:None always pairs
    // with a reason; a hand-built slot that breaks it still answers
    // something closed-set.
    let reason = slot
        .disabled_reason
        .clone()
        .or_else(|| provider.is_none().then(|| "no_provider".to_string()));
    EnablementState {
        enabled: slot.disabled_reason.is_none() && provider.is_some(),
        flag_on: slot.disabled_reason.as_deref() != Some("flag_off"),
        provider,
        reason,
    }
}

/// The gated-surface check — everything except `oxpilot.status` and the
/// gate-exempt `admin.oxpilot.*` procs calls this first (BR2.3). Disabled
/// ⇒ typed `oxpilot.disabled`.
pub async fn require_enabled(ctx: &RpcCtx) -> Result<EnablementState, AppError> {
    let state = resolve_enablement(ctx).await;
    if !state.enabled {
        return Err(AppError::new(
            "oxpilot.disabled",
            state
                .reason
                .as_deref()
                .map(|r| format!("oxpilot is disabled ({r})"))
                .unwrap_or_else(|| "oxpilot is disabled".to_string()),
        ));
    }
    Ok(state)
}

/// Wire shape for one queued message — shared by the snapshot
/// `pending_queue` array and the `oxpilot.queue_list` item shape so a
/// queued card reads identically on either surface (C1/C2).
pub fn queued_message_json(row: &OxpilotQueuedRow) -> Value {
    json!({
        "id": row.id,
        "queue_seq": row.queue_seq,
        "content": row.content,
        "client_msg_id": row.client_msg_id,
        "created_at": row.created_at,
    })
}

/// `snapshot` frame payload for `GET .../chat/stream` — persisted messages
/// (each merged with its durable tool-call cards by `message_id`, cards
/// ordered by `call_index`), the pending follow-up queue in `queue_seq`
/// order, and the live-turn view (BR4.4, C2 U2 extension).
pub fn snapshot_frame(
    mut messages: Vec<Value>,
    pending_queue: Vec<OxpilotQueuedRow>,
    tool_calls: Vec<OxpilotToolCallRow>,
    active_turn: Option<ActiveTurnView>,
) -> Value {
    // Merge durable tool cards into each message object by message_id —
    // repeated same-tool calls persist distinctly and sort by call_index.
    let mut by_message: HashMap<&str, Vec<&OxpilotToolCallRow>> = HashMap::new();
    for call in &tool_calls {
        by_message
            .entry(call.message_id.as_str())
            .or_default()
            .push(call);
    }
    for cards in by_message.values_mut() {
        cards.sort_by_key(|c| c.call_index);
    }
    for message in &mut messages {
        let cards = message
            .get("id")
            .and_then(|id| id.as_str())
            .and_then(|id| by_message.get(id))
            .cloned()
            .unwrap_or_default();
        message["tool_calls"] = json!(cards
            .iter()
            .map(|c| json!({
                "call_id": c.call_id,
                "call_index": c.call_index,
                "tool": c.tool,
                "status": c.status,
                "args_summary": c.args_summary,
                "output_summary": c.output_summary,
            }))
            .collect::<Vec<_>>());
    }
    json!({
        "type": "snapshot",
        "messages": messages,
        "pending_queue": pending_queue.iter().map(queued_message_json).collect::<Vec<_>>(),
        "active_turn": active_turn.map(|t| json!({
            "turn_id": t.turn_id,
            "started_seq": t.started_seq,
            "message_id": t.message_id,
            "last_delta_seq": t.last_delta_seq,
        })),
    })
}

/// The `active_turn` view inside a snapshot frame.
pub struct ActiveTurnView {
    pub turn_id: String,
    pub started_seq: i64,
    pub message_id: String,
    /// Max *emitted* delta seq — a liveness watermark, not a persistence
    /// watermark (persisted content trails emitted frames). Clients apply
    /// every replayed frame and dedupe by `(message_id, seq)`.
    pub last_delta_seq: i64,
}

/// Helper — view an [`ActiveTurn`] for `snapshot.active_turn`.
pub async fn active_turn_view(turn: &ActiveTurn) -> ActiveTurnView {
    let deltas = turn.emitted_deltas.lock().await;
    let last_delta_seq = deltas
        .iter()
        .rfind(|f| f.get("type").and_then(|t| t.as_str()) == Some("append"))
        .and_then(|f| f.get("seq").and_then(|s| s.as_i64()))
        .unwrap_or(0);
    ActiveTurnView {
        turn_id: turn.turn_id.clone(),
        started_seq: turn.started_seq.load(std::sync::atomic::Ordering::SeqCst),
        message_id: turn.message_id.clone(),
        last_delta_seq,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(session: &str, turn_id: &str, msg_id: &str) -> ActiveTurn {
        let (fanout, _) = broadcast::channel(TURN_FANOUT_CAPACITY);
        ActiveTurn {
            session_id: session.to_string(),
            turn_id: turn_id.to_string(),
            message_id: msg_id.to_string(),
            started_seq: std::sync::atomic::AtomicI64::new(1),
            pending_client_msg_id: None,
            pending_content: String::new(),
            fanout,
            emitted_deltas: Mutex::new(Vec::new()),
            cancel: CancellationToken::new(),
            admission_lock: Arc::new(Mutex::new(())),
            terminal_sent: std::sync::atomic::AtomicBool::new(false),
            generation: 0,
        }
    }

    #[tokio::test]
    async fn claim_insert_if_absent_rejects_second() {
        let turns = ActiveTurns::default();
        let t1 = turns
            .claim("s1", || turn("s1", "t1", "m1"))
            .await
            .expect("first claim wins");
        assert!(turns.claim("s1", || turn("s1", "t2", "m2")).await.is_none());
        assert!(turns.claim("s2", || turn("s2", "t3", "m3")).await.is_some());
        assert_eq!(turns.get("s1").await.unwrap().turn_id, "t1");
        turns.release("s1", &t1.turn_id).await;
        assert!(turns.get("s1").await.is_none());
    }

    #[tokio::test]
    async fn release_guards_against_newer_turn() {
        let turns = ActiveTurns::default();
        let t1 = turns.claim("s", || turn("s", "old", "m1")).await.unwrap();
        turns.release("s", &t1.turn_id).await;
        let _t2 = turns.claim("s", || turn("s", "new", "m2")).await.unwrap();
        // Releasing with the stale turn_id must not evict the live claim.
        turns.release("s", "old").await;
        assert_eq!(turns.get("s").await.unwrap().turn_id, "new");
    }

    #[tokio::test]
    async fn buffered_deltas_replay_to_late_reader() {
        let turns = ActiveTurns::default();
        let t = turns.claim("s", || turn("s", "t", "m")).await.unwrap();
        t.emitted_deltas.lock().await.push(
            json!({"type":"append","message_id":"m","seq":1,"role":"assistant","delta":"hi"}),
        );
        let view = active_turn_view(&t).await;
        assert_eq!(view.last_delta_seq, 1);
        assert_eq!(view.message_id, "m");
    }

    #[test]
    fn enablement_view_reads_stamped_reason() {
        let turns = Arc::new(ActiveTurns::default());
        // Live provider slot.
        let live = OxpilotRuntimeSlot {
            provider: Some(Arc::new(StubProvider { fail: false })),
            model: Some("stub-1".to_string()),
            disabled_reason: None,
            turns: turns.clone(),
            built_from: Some(ProviderIdentity {
                provider_id: "stub".to_string(),
                model: Some("stub-1".to_string()),
                base_url: None,
                key_source: KeySource::None,
            }),
            budget_caps: BudgetCaps::default(),
            tools: None,
        };
        let st = resolve_enablement_slot(&live);
        assert!(st.enabled);
        assert_eq!(st.provider.unwrap().0, "stub");

        // flag_off retains the Arc but stays gated.
        let off = OxpilotRuntimeSlot {
            disabled_reason: Some("flag_off".to_string()),
            ..live.clone()
        };
        let st = resolve_enablement_slot(&off);
        assert!(!st.enabled);
        assert!(!st.flag_on);
        assert_eq!(st.reason.as_deref(), Some("flag_off"));

        // Reason-bearing disabled slot.
        let dead = disabled_slot(turns, "misconfigured", BudgetCaps::default(), None);
        let st = resolve_enablement_slot(&dead);
        assert!(!st.enabled);
        assert_eq!(st.reason.as_deref(), Some("misconfigured"));
        assert!(st.provider.is_none());
    }

    // ---------- U2 seam tests ----------

    /// A trait-object probe — verbs are never called; the tests only need
    /// an `Arc<dyn SessionService>` to install and identify.
    struct ProbeService;

    #[async_trait]
    impl SessionService for ProbeService {
        async fn create_session(
            &self,
            _u: &str,
            _i: CreateSession,
        ) -> Result<OxpilotSessionRow, AppError> {
            unimplemented!()
        }
        async fn get_session(&self, _u: &str, _s: &str) -> Result<OxpilotSessionRow, AppError> {
            unimplemented!()
        }
        async fn list_sessions(&self, _u: &str) -> Result<Vec<OxpilotSessionRow>, AppError> {
            unimplemented!()
        }
        async fn delete_session(&self, _u: &str, _s: &str) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn append_message(
            &self,
            _u: &str,
            _i: AppendMessage,
        ) -> Result<OxpilotMessageRow, AppError> {
            unimplemented!()
        }
        async fn transcript(&self, _u: &str, _s: &str) -> Result<Vec<OxpilotMessageRow>, AppError> {
            unimplemented!()
        }
        async fn list_messages(
            &self,
            _u: &str,
            _s: &str,
        ) -> Result<Vec<OxpilotMessageRow>, AppError> {
            unimplemented!()
        }
        async fn find_message_by_client_msg_id(
            &self,
            _u: &str,
            _s: &str,
            _c: &str,
        ) -> Result<Option<OxpilotMessageRow>, AppError> {
            unimplemented!()
        }
        async fn set_message_content(&self, _u: &str, _m: &str, _c: &str) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn set_message_status(&self, _u: &str, _m: &str, _s: &str) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn enqueue(&self, _u: &str, _i: Enqueue) -> Result<QueuedReceipt, AppError> {
            unimplemented!()
        }
        async fn peek_queued_receipt(
            &self,
            _u: &str,
            _s: &str,
            _c: &str,
        ) -> Result<Option<QueuedReceipt>, AppError> {
            unimplemented!()
        }
        async fn drain_queue(
            &self,
            _u: &str,
            _s: &str,
        ) -> Result<Vec<OxpilotMessageRow>, AppError> {
            unimplemented!()
        }
        async fn list_queue(&self, _u: &str, _s: &str) -> Result<Vec<OxpilotQueuedRow>, AppError> {
            unimplemented!()
        }
        async fn delete_queued(&self, _u: &str, _s: &str, _q: &str) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn append_tool_call(
            &self,
            _u: &str,
            _i: ToolCallWrite,
        ) -> Result<OxpilotToolCallRow, AppError> {
            unimplemented!()
        }
        async fn list_tool_calls(
            &self,
            _u: &str,
            _s: &str,
        ) -> Result<Vec<OxpilotToolCallRow>, AppError> {
            unimplemented!()
        }
        async fn set_tool_call_status(
            &self,
            _u: &str,
            _i: ToolCallStatusUpdate,
        ) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn save_proposal(
            &self,
            _u: &str,
            _i: ProposalWrite,
        ) -> Result<OxpilotProposalRow, AppError> {
            unimplemented!()
        }
        async fn proposal(&self, _u: &str, _p: &str) -> Result<OxpilotProposalRow, AppError> {
            unimplemented!()
        }
        async fn mark_applied(
            &self,
            _u: &str,
            _p: &str,
            _r: &str,
        ) -> Result<OxpilotProposalRow, AppError> {
            unimplemented!()
        }
        async fn record_usage(&self, _u: &str, _d: UsageDelta) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn check_budget(&self, _u: &str, _c: BudgetCaps) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn admit_budget(&self, _u: &str, _c: BudgetCaps) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn check_token_budget(&self, _u: &str, _c: BudgetCaps) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn usage_view(&self, _u: &str, _c: BudgetCaps) -> Result<UsageView, AppError> {
            unimplemented!()
        }
    }

    fn bare_slot() -> OxpilotRuntimeSlot {
        disabled_slot(
            Arc::new(ActiveTurns::default()),
            "flag_off",
            BudgetCaps::default(),
            None,
        )
    }

    /// The seam starts unset (`oxpilot.internal` — the settings-unwrap
    /// idiom's honest failure), installs once, and is set-once: a second
    /// `install_sessions` can never swap the port under in-flight
    /// callers.
    #[test]
    fn sessions_seam_unset_then_installed_once() {
        let runtime = OxpilotRuntime::from_slot(bare_slot());
        let err = match runtime.sessions() {
            Err(e) => e,
            Ok(_) => panic!("sessions() must fail before install"),
        };
        assert_eq!(err.code, "oxpilot.internal");

        let first: Arc<dyn SessionService> = Arc::new(ProbeService);
        runtime.install_sessions(first.clone());
        let got = runtime.sessions().expect("installed");
        assert!(Arc::ptr_eq(&got, &first));

        // Set-once — a late install is a no-op, the original stays.
        let second: Arc<dyn SessionService> = Arc::new(ProbeService);
        runtime.install_sessions(second);
        let got = runtime.sessions().expect("still installed");
        assert!(Arc::ptr_eq(&got, &first));
    }

    /// Boot parses `OXIDEAN_OXPILOT_{USER,INSTANCE}_BUDGET` into
    /// `slot.budget_caps`; `disabled_slot`/`from_slot` carry whatever
    /// they are handed. (The resolve/carry/build paths live in
    /// api-oxpilot's settings suite — same stamp asserted there.)
    #[test]
    fn budget_caps_boot_and_disabled_paths() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("OXIDEAN_OXPILOT_USER_BUDGET", "11");
        std::env::set_var("OXIDEAN_OXPILOT_INSTANCE_BUDGET", "22");
        let runtime = OxpilotRuntime::new();
        let caps = runtime.slot.blocking_read().budget_caps;
        assert_eq!(
            caps,
            BudgetCaps {
                user: Some(11),
                instance: Some(22)
            }
        );
        std::env::remove_var("OXIDEAN_OXPILOT_USER_BUDGET");
        std::env::remove_var("OXIDEAN_OXPILOT_INSTANCE_BUDGET");

        // A disabled slot keeps the resolved posture it was handed.
        let dead = disabled_slot(
            Arc::new(ActiveTurns::default()),
            "no_provider",
            BudgetCaps {
                user: Some(7),
                instance: None,
            },
            None,
        );
        assert_eq!(dead.budget_caps.user, Some(7));
        assert_eq!(dead.budget_caps.instance, None);
    }

    /// C2 U2 frame — queued rows serialize in queue order with the
    /// `queued_message_json` shape; tool cards merge into their owning
    /// message by `message_id`, sorted by `call_index`.
    #[test]
    fn snapshot_frame_carries_queue_and_tool_cards() {
        let queued = |id: &str, queue_seq: i64, cmid: Option<&str>| OxpilotQueuedRow {
            id: id.to_string(),
            session_id: "s1".to_string(),
            queue_seq,
            content: "queued".to_string(),
            client_msg_id: cmid.map(str::to_string),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let call = |id: &str, msg: &str, call_index: i64| OxpilotToolCallRow {
            id: id.to_string(),
            session_id: "s1".to_string(),
            message_id: msg.to_string(),
            call_id: format!("call-{id}"),
            seq: 2,
            call_index,
            tool: "fs_read".to_string(),
            args_summary: "{}".to_string(),
            output_summary: String::new(),
            status: "done".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let messages = vec![
            json!({"id": "m1", "seq": 1, "role": "user"}),
            json!({"id": "m2", "seq": 2, "role": "assistant"}),
        ];
        // Out-of-order call_index input — the frame sorts per message.
        let tool_calls = vec![
            call("tc2", "m2", 1),
            call("tc1", "m2", 0),
            call("tc3", "m9", 0), // orphan message_id — merged nowhere
        ];
        let frame = snapshot_frame(
            messages,
            vec![queued("q1", 1, Some("cm")), queued("q2", 4, None)],
            tool_calls,
            None,
        );

        assert_eq!(frame["type"], "snapshot");
        // Queue order preserved, pinned fields present.
        let q = frame["pending_queue"].as_array().unwrap();
        assert_eq!(q.len(), 2);
        assert_eq!(q[0]["id"], "q1");
        assert_eq!(q[0]["queue_seq"], 1);
        assert_eq!(q[0]["client_msg_id"], "cm");
        assert_eq!(q[1]["id"], "q2");
        assert!(q[1]["client_msg_id"].is_null());

        // Tool cards merged + sorted by call_index; message without
        // cards gets an empty array (stable wire shape).
        let msgs = frame["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["tool_calls"].as_array().unwrap().len(), 0);
        let cards = msgs[1]["tool_calls"].as_array().unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0]["call_id"], "call-tc1");
        assert_eq!(cards[0]["call_index"], 0);
        assert_eq!(cards[0]["tool"], "fs_read");
        assert_eq!(cards[1]["call_id"], "call-tc2");
        assert!(frame["active_turn"].is_null());
    }
}
