//! The U3 agent-runtime — the pinned turn machinery (BR2–BR6, WF2–WF5):
//! `turn_parts()` pin, bounded prompt assembly, the provider round loop
//! with tool-call execution, first-wins `emit_terminal`, wrapper-owned
//! release, `drain_and_chain` under the per-session admission lock, and
//! the C8 `OxpilotTurnAbortHook`.
//!
//! Two lock disciplines do the heavy lifting: `emitted_deltas`'s mutex
//! makes the terminal CAS atomic (BR5.4), and each `ActiveTurn`'s
//! `admission_lock` closes the release→recheck blind window against a
//! concurrent `message_queue` commit (BR3.4/R-20).

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use oxidean_api_core::app::AppState;
use oxidean_api_core::oxpilot::{
    ActiveTurn, AppendMessage, ChatMsg, ChatRequest, ChatStreamItem, SessionDeleteHook,
    SessionService, SharedOxpilot, ToolCall, ToolCallStatusUpdate, ToolCallWrite, ToolCtx,
    ToolError, ToolExecutor, ToolOutput, Usage, UsageDelta, TURN_FANOUT_CAPACITY,
};
use oxidean_core::{redact, AppError};
use oxidean_db::{OxpilotMessageRow, OxpilotSessionRow, OxpilotToolCallRow};
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

/// Prompt transcript window (BR2.3) — newest-first `take_while` bound.
const PROMPT_TRANSCRIPT_MAX_BYTES: usize = 128 * 1024;
/// `[tool_call <name> → <status>]` summary-line bound (BR2.3).
const TOOL_SUMMARY_LINE_MAX: usize = 200;
/// Card `args_summary` bound — post-redact truncation (NFR1.7).
const ARGS_SUMMARY_MAX: usize = 2 * 1024;
/// Card `output_summary` + `tool` frame summary bound — the tighter of
/// the two output bounds (the fed-back error text rides it too).
const OUTPUT_SUMMARY_MAX: usize = 2 * 1024;
/// Tool output fed back to the provider (BR3.2) — the model sees the
/// real (capped) output.
const TOOL_OUTPUT_MAX: usize = 32 * 1024;
/// Tool-executing rounds bound (BR3.3).
const MAX_TOOL_ROUNDS: usize = 8;
/// Per-call wall bound — `tokio::time::timeout` racing cancel (BR3.3).
const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// Whole-turn wall bound — raced on every stream await (NFR3.7/3.8).
const TURN_MAX_DURATION: Duration = Duration::from_secs(10 * 60);
/// Turn-chain depth bound — `generation` counting (BR4.3).
const MAX_CHAIN_DEPTH: u32 = 16;

// ---------- frames ----------

/// The `terminal` frame JSON — `code` rides only on `result:"error"`.
fn terminal_json(turn: &ActiveTurn, terminal: &Terminal) -> Value {
    match terminal {
        Terminal::Done => json!({
            "type": "terminal",
            "message_id": turn.message_id,
            "result": "done",
        }),
        Terminal::Aborted => json!({
            "type": "terminal",
            "message_id": turn.message_id,
            "result": "aborted",
        }),
        Terminal::Error(code) => json!({
            "type": "terminal",
            "message_id": turn.message_id,
            "result": "error",
            "code": code,
        }),
    }
}

/// The C2 `tool` frame — `call_id`/`call_index` are the additive fields
/// live readers use to disambiguate parallel same-name calls.
fn tool_frame(
    turn: &ActiveTurn,
    tool: &str,
    call_id: &str,
    call_index: i64,
    status: &str,
    args_summary: Option<&str>,
    output_summary: Option<&str>,
) -> Value {
    let mut f = json!({
        "type": "tool_call",
        "message_id": turn.message_id,
        "tool": tool,
        "call_id": call_id,
        "call_index": call_index,
        "status": status,
    });
    if let Some(s) = args_summary {
        f["args_summary"] = json!(s);
    }
    if let Some(s) = output_summary {
        f["output_summary"] = json!(s);
    }
    f
}

/// Non-terminal frame emit — buffer push (the replay surface) + fanout
/// send (live readers). A full ring only drops lagged subscribers; the
/// buffer is the durable side for exactly-once replay (C2). The
/// `terminal_sent` check sits INSIDE the buffer lock so the check and
/// the push are atomic against `emit_terminal`: after a terminal wins
/// its CAS, no non-terminal frame can land in the replay buffer or on
/// live readers (C2 — frames after terminal are never emitted).
pub(crate) async fn emit_frame(turn: &ActiveTurn, frame: Value) {
    let mut buf = turn.emitted_deltas.lock().await;
    if turn.terminal_sent.load(Ordering::SeqCst) {
        return;
    }
    buf.push(frame.clone());
    // The send rides the same lock emit_terminal holds — a checked
    // frame can never land on the live ring after the terminal.
    let _ = turn.fanout.send(frame);
}

/// BR5.4 first-wins terminal — the CAS sits INSIDE the buffer lock so
/// the flag set + frame push are atomic against a concurrent claimant;
/// losers return early. `true` ⇒ this caller emitted the one terminal.
pub(crate) async fn emit_terminal(turn: &ActiveTurn, frame: Value) -> bool {
    let mut buf = turn.emitted_deltas.lock().await;
    if turn.terminal_sent.swap(true, Ordering::SeqCst) {
        return false;
    }
    buf.push(frame.clone());
    let _ = turn.fanout.send(frame);
    true
}

// ---------- prompt assembly (BR2.3) ----------

/// Byte-bounded truncation at a char boundary with an ellipsis marker —
/// the result never exceeds `max` bytes.
fn truncate_marker(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max.saturating_sub('…'.len_utf8());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// One `[tool_call <name> → <status>]` summary line per persisted card
/// (BR2.3) — bounded ≤200 B so the window's byte math stays honest.
fn tool_summary_line(call: &OxpilotToolCallRow) -> String {
    truncate_marker(
        &format!("\n[tool_call {} → {}]", call.tool, call.status),
        TOOL_SUMMARY_LINE_MAX,
    )
}

/// Transcript row → provider `ChatMsg` — assistant rows append their
/// per-card summary lines; `role`/`content` carry through verbatim
/// (rows are already redacted at the U2 write boundary).
fn into_chat_msg(row: &OxpilotMessageRow, cards: &[&OxpilotToolCallRow]) -> ChatMsg {
    let mut content = row.content.clone();
    if row.role == "assistant" {
        for card in cards {
            content.push_str(&tool_summary_line(card));
        }
    }
    ChatMsg {
        role: row.role.clone(),
        content,
        tool_call_id: None,
        tool_name: None,
        tool_calls_json: None,
    }
}

/// `take_while` byte accounting — the POST-conversion size, including
/// the ≤200 B/call summary lines (BR2.3).
fn rendered_len(msg: &ChatMsg) -> usize {
    msg.role.len()
        + msg.content.len()
        + msg.tool_call_id.as_deref().map(str::len).unwrap_or(0)
        + msg.tool_name.as_deref().map(str::len).unwrap_or(0)
        + msg.tool_calls_json.as_deref().map(str::len).unwrap_or(0)
}

/// The system preamble — repo binding + goal (BR2.3); rides
/// `ChatRequest.system`, not the message list.
fn system_prompt(session: &OxpilotSessionRow) -> String {
    let mut s = format!(
        "You are Oxpilot, an agent assistant embedded in the Oxidean forge. \
         Repository: {}/{} (branch {}).",
        session.repo_owner, session.repo_name, session.branch
    );
    if let Some(goal) = session.goal.as_deref().filter(|g| !g.is_empty()) {
        s.push_str("\n\nGoal: ");
        s.push_str(goal);
    }
    s
}

/// The bounded transcript window (BR2.3): `transcript` rows newest-first
/// under `PROMPT_TRANSCRIPT_MAX_BYTES`, reversed to temporal order.
/// Pure — unit-testable without a provider.
async fn assemble_prompt(
    svc: &Arc<dyn SessionService>,
    session: &OxpilotSessionRow,
) -> Result<Vec<ChatMsg>, AppError> {
    let rows = svc.transcript(&session.user_id, &session.id).await?;
    let calls = svc.list_tool_calls(&session.user_id, &session.id).await?;
    let mut by_msg: HashMap<&str, Vec<&OxpilotToolCallRow>> = HashMap::new();
    for c in &calls {
        by_msg.entry(c.message_id.as_str()).or_default().push(c);
    }
    for v in by_msg.values_mut() {
        v.sort_by_key(|c| c.call_index);
    }
    let mut budget = PROMPT_TRANSCRIPT_MAX_BYTES;
    let mut window: Vec<ChatMsg> = Vec::new();
    for row in rows.iter().rev() {
        let cards = by_msg
            .get(row.id.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let msg = into_chat_msg(row, cards);
        // Empty rows (failed/aborted turns with no deltas and no cards)
        // carry nothing and break providers that reject empty content.
        if msg.content.trim().is_empty() {
            continue;
        }
        let n = rendered_len(&msg);
        if n > budget {
            break;
        }
        budget -= n;
        window.push(msg);
    }
    window.reverse();
    Ok(window)
}

// ---------- the turn loop ----------

/// How the turn ended — maps onto the terminal frame's fields.
#[derive(Debug)]
enum Terminal {
    Done,
    /// `code` rides the terminal frame — provider-verbatim codes and the
    /// internal pins (`no_model`, `oxpilot.disabled`, `turn_timeout`,
    /// `tool_rounds_exceeded`).
    Error(String),
    Aborted,
}

/// Per-turn streaming state carried across rounds.
struct TurnAccum {
    /// Cumulative assistant row content — `set_message_content` persists
    /// this; the per-round msg slices carve from it.
    content: String,
    /// `append` frame `seq` — per-message ordinal.
    delta_seq: i64,
    /// Tool-card ordinal — per-message (one assistant row per turn).
    call_index: i64,
    /// BR6.1 — `true` once the provider yields ≥1 item across all rounds.
    any_items: bool,
    /// Summed round usage (a `Done{usage:None}` contributes +0).
    tokens: i64,
    /// The wall deadline already fired — classifies later `cancelled()`
    /// observations as `turn_timeout`, not `aborted` (deterministic).
    wall_expired: bool,
}

impl TurnAccum {
    fn new() -> Self {
        TurnAccum {
            content: String::new(),
            delta_seq: 0,
            call_index: 0,
            any_items: false,
            tokens: 0,
            wall_expired: false,
        }
    }
}

/// What one provider stream yielded.
struct RoundResult {
    /// Calls the model emitted (raw provider ids — empty strings allowed;
    /// minting happens at card time).
    calls: Vec<(String, String, String)>,
    /// This round's delta slice — the assistant round message's content.
    text: String,
    /// `Done{usage}` payload when the wire reported one.
    usage: Option<Usage>,
    /// Any stream item observed (BR6.1's usage-count flag).
    any_items: bool,
    /// Set when the round ended on cancel/timeout/provider `Error`.
    terminal: Option<Terminal>,
}

/// The effective call id — the provider's `id`, or the deterministic
/// mint `call_{message_id}_{call_index}` when it's empty (Ollama omits;
/// the durable card keys `UNIQUE(session_id, call_id)` — an empty id
/// collides on the second call).
fn effective_call_id(message_id: &str, provider_id: &str, call_index: i64) -> String {
    if provider_id.trim().is_empty() {
        format!("call_{message_id}_{call_index}")
    } else {
        provider_id.to_string()
    }
}

/// Consume one provider stream — Delta → append frame + cumulative
/// persist; ToolCall → collect for the round; Done → usage + end;
/// Error → terminal. `cancel`/`deadline` race every `next()` — the
/// deadline arm cancels the token so in-flight work wakes, and marks
/// `wall_expired` so later `cancelled()` reads still classify
/// `turn_timeout`.
async fn run_round(
    stream: &mut oxidean_api_core::oxpilot::ChatStream,
    svc: &Arc<dyn SessionService>,
    session: &OxpilotSessionRow,
    turn: &Arc<ActiveTurn>,
    accum: &mut TurnAccum,
    deadline: tokio::time::Instant,
) -> Result<RoundResult, AppError> {
    let mut out = RoundResult {
        calls: Vec::new(),
        text: String::new(),
        usage: None,
        any_items: false,
        terminal: None,
    };
    loop {
        // Already-cancelled wins deterministically — else a Ready stream
        // item could beat the cancel arm's poll order and emit a frame
        // after the abort landed (mid-flight stays a fair race).
        if turn.cancel.is_cancelled() {
            out.terminal = Some(if accum.wall_expired {
                Terminal::Error("turn_timeout".to_string())
            } else {
                Terminal::Aborted
            });
            break;
        }
        tokio::select! {
            _ = turn.cancel.cancelled() => {
                out.terminal = Some(if accum.wall_expired {
                    Terminal::Error("turn_timeout".to_string())
                } else {
                    Terminal::Aborted
                });
                break;
            }
            _ = tokio::time::sleep_until(deadline) => {
                accum.wall_expired = true;
                turn.cancel.cancel();
                out.terminal = Some(Terminal::Error("turn_timeout".to_string()));
                break;
            }
            item = stream.next() => {
                match item {
                    Some(ChatStreamItem::Delta(d)) => {
                        out.any_items = true;
                        accum.content.push_str(&d);
                        out.text.push_str(&d);
                        accum.delta_seq += 1;
                        // Redact per-delta at emission (NFR1.8); the
                        // service re-redacts the cumulative write —
                        // the row stays the safety floor.
                        emit_frame(turn, json!({
                            "type": "append",
                            "message_id": turn.message_id,
                            "seq": accum.delta_seq,
                            "role": "assistant",
                            "delta": redact(&d),
                        }))
                        .await;
                        svc.set_message_content(
                            &session.user_id,
                            &turn.message_id,
                            &accum.content,
                        )
                        .await?;
                    }
                    Some(ChatStreamItem::ToolCall { id, name, args_json }) => {
                        out.any_items = true;
                        out.calls.push((id, name, args_json));
                    }
                    Some(ChatStreamItem::Done { usage }) => {
                        out.any_items = true;
                        out.usage = usage;
                        break;
                    }
                    Some(ChatStreamItem::Error { code, msg }) => {
                        out.any_items = true;
                        tracing::warn!(code = %code, error = %msg, "provider stream error");
                        out.terminal = Some(Terminal::Error(code));
                        break;
                    }
                    // Silent end — the wire's `Done` marker is the ONLY
                    // success signal (C4); a dropped/truncated stream is a
                    // provider error, never an implicit completion.
                    None => {
                        out.terminal = Some(Terminal::Error("provider_error".to_string()));
                        break;
                    }
                }
            }
        }
    }
    Ok(out)
}

/// One executed call — the durable card + fed-back result pair.
struct ExecutedCall {
    call_id: String,
    name: String,
    args_json: String,
    /// The `ChatMsg{role:"tool"}` content — redacted + bounded.
    result_content: String,
}

/// The plumbing a round's tool execution shares — bundled so
/// `execute_calls`/`fail_pending_calls` stay under the arity lint.
struct ExecCtx<'a> {
    svc: &'a Arc<dyn SessionService>,
    session: &'a OxpilotSessionRow,
    turn: &'a Arc<ActiveTurn>,
    executor: &'a dyn ToolExecutor,
    assistant_seq: i64,
    deadline: tokio::time::Instant,
}

/// Invoke one tool call under the per-call timeout racing cancel + the
/// turn wall (BR3.3). `call_timeout` is `TOOL_CALL_TIMEOUT` in the turn
/// loop — a param so unit tests can shrink it without sleeping 60s.
/// Error results still feed back — the model sees every emitted call
/// answered.
async fn invoke_call(
    executor: &dyn ToolExecutor,
    session: &OxpilotSessionRow,
    turn: &Arc<ActiveTurn>,
    call: ToolCall,
    accum: &mut TurnAccum,
    deadline: tokio::time::Instant,
    call_timeout: Duration,
) -> Result<ToolOutput, ToolError> {
    // Already-cancelled must win deterministically — without this check a
    // fast executor can complete on the first select! poll and swallow the
    // abort (cancel-vs-completion is only a fair race mid-flight).
    if turn.cancel.is_cancelled() {
        return Err(ToolError {
            code: "aborted".to_string(),
            message: "turn aborted".to_string(),
        });
    }
    let ctx = ToolCtx {
        session_id: session.id.clone(),
        user_id: session.user_id.clone(),
        repo_id: session.repo_id.clone(),
        proposal_id: None,
    };
    let invoke = executor.invoke(call, ctx);
    tokio::pin!(invoke);
    tokio::select! {
        r = tokio::time::timeout(call_timeout, &mut invoke) => match r {
            Ok(res) => res,
            Err(_) => Err(ToolError {
                code: "timeout".to_string(),
                message: "tool call timed out".to_string(),
            }),
        },
        _ = turn.cancel.cancelled() => Err(ToolError {
            code: "aborted".to_string(),
            message: "turn aborted".to_string(),
        }),
        _ = tokio::time::sleep_until(deadline) => {
            accum.wall_expired = true;
            turn.cancel.cancel();
            Err(ToolError {
                code: "timeout".to_string(),
                message: "turn deadline reached".to_string(),
            })
        }
    }
}

/// Execute one round's pending calls — durable-first per call (card
/// `running` persists BEFORE its frame; `done|error` before the next
/// emit). Card-persist failure ⇒ `tool{status:"error"}` frame anyway +
/// a `card_persist` result fed back — no phantom running card, no lost
/// call result (BR2.4).
async fn execute_calls(
    ctx: &ExecCtx<'_>,
    calls: Vec<(String, String, String)>,
    accum: &mut TurnAccum,
) -> Vec<ExecutedCall> {
    let mut executed = Vec::with_capacity(calls.len());
    for (raw_id, name, args_json) in calls {
        let call_id = effective_call_id(&ctx.turn.message_id, &raw_id, accum.call_index);
        let args_summary = truncate_marker(&redact(&args_json), ARGS_SUMMARY_MAX);
        if let Err(e) = ctx
            .svc
            .append_tool_call(
                &ctx.session.user_id,
                ToolCallWrite {
                    session_id: ctx.session.id.clone(),
                    message_id: ctx.turn.message_id.clone(),
                    seq: ctx.assistant_seq,
                    call_index: accum.call_index,
                    call_id: call_id.clone(),
                    tool: name.clone(),
                    args_summary: args_summary.clone(),
                    status: "running".to_string(),
                },
            )
            .await
        {
            // The call result must still feed back honestly — the model
            // learns the card write failed (card_persist), the wire sees
            // an error frame, and no phantom `running` card remains.
            tracing::warn!(error = %e.message, "oxpilot tool-call persist failed");
            emit_frame(
                ctx.turn,
                tool_frame(
                    ctx.turn,
                    &name,
                    &call_id,
                    accum.call_index,
                    "error",
                    Some(&args_summary),
                    Some("card_persist"),
                ),
            )
            .await;
            executed.push(ExecutedCall {
                call_id,
                name,
                args_json,
                result_content: "tool call could not be recorded (card_persist)".to_string(),
            });
            accum.call_index += 1;
            continue;
        }
        emit_frame(
            ctx.turn,
            tool_frame(
                ctx.turn,
                &name,
                &call_id,
                accum.call_index,
                "running",
                Some(&args_summary),
                None,
            ),
        )
        .await;
        tracing::debug!(
            session_id = %ctx.session.id,
            tool = %name,
            call_index = accum.call_index,
            status = "running",
            "oxpilot tool call"
        );

        let res = invoke_call(
            ctx.executor,
            ctx.session,
            ctx.turn,
            ToolCall {
                call_id: call_id.clone(),
                name: name.clone(),
                args_json: args_json.clone(),
            },
            accum,
            ctx.deadline,
            TOOL_CALL_TIMEOUT,
        )
        .await;
        let (status, summary, result_content) = match res {
            Ok(out) => (
                "done",
                truncate_marker(&redact(&out.output_summary), OUTPUT_SUMMARY_MAX),
                truncate_marker(&redact(&out.content), TOOL_OUTPUT_MAX),
            ),
            Err(e) => {
                let bounded = truncate_marker(&redact(&e.message), OUTPUT_SUMMARY_MAX);
                ("error", bounded.clone(), bounded)
            }
        };
        // The card transition is best-effort post-cascade — a dead row
        // can't block the frame (WF4.2's logged-not-found discipline).
        if let Err(e) = ctx
            .svc
            .set_tool_call_status(
                &ctx.session.user_id,
                ToolCallStatusUpdate {
                    session_id: ctx.session.id.clone(),
                    call_id: call_id.clone(),
                    status: status.to_string(),
                    output_summary: Some(summary.clone()),
                },
            )
            .await
        {
            tracing::warn!(error = %e.message, "oxpilot tool-call status write failed");
        }
        emit_frame(
            ctx.turn,
            tool_frame(
                ctx.turn,
                &name,
                &call_id,
                accum.call_index,
                status,
                None,
                Some(&summary),
            ),
        )
        .await;
        tracing::debug!(
            session_id = %ctx.session.id,
            tool = %name,
            call_index = accum.call_index,
            status = %status,
            "oxpilot tool call"
        );
        executed.push(ExecutedCall {
            call_id,
            name,
            args_json,
            result_content,
        });
        accum.call_index += 1;
    }
    executed
}

/// Persist the calls a cap-breached or terminal-died round left
/// pending — each gets a durable `error` card (never `running` — they
/// will not invoke) and a matching `tool{status:"error"}` frame, then
/// the turn lands its terminal (BR3.3).
async fn fail_pending_calls(
    ctx: &ExecCtx<'_>,
    calls: Vec<(String, String, String)>,
    accum: &mut TurnAccum,
    summary: &str,
) {
    for (raw_id, name, args_json) in calls {
        let call_id = effective_call_id(&ctx.turn.message_id, &raw_id, accum.call_index);
        let args_summary = truncate_marker(&redact(&args_json), ARGS_SUMMARY_MAX);
        if let Err(e) = ctx
            .svc
            .append_tool_call(
                &ctx.session.user_id,
                ToolCallWrite {
                    session_id: ctx.session.id.clone(),
                    message_id: ctx.turn.message_id.clone(),
                    seq: ctx.assistant_seq,
                    call_index: accum.call_index,
                    call_id: call_id.clone(),
                    tool: name.clone(),
                    args_summary: args_summary.clone(),
                    status: "error".to_string(),
                },
            )
            .await
        {
            tracing::warn!(error = %e.message, "oxpilot tool-call persist failed");
        }
        emit_frame(
            ctx.turn,
            tool_frame(
                ctx.turn,
                &name,
                &call_id,
                accum.call_index,
                "error",
                Some(&args_summary),
                Some(summary),
            ),
        )
        .await;
        accum.call_index += 1;
    }
}

/// Terminal sequence (WF3): terminal row status (durable-first, BR2.4)
/// → usage accounting (BR6.1) → the in-turn end-of-turn drain → the
/// first-wins terminal frame. Every fallible write is best-effort —
/// post-cascade `not_found`s are logged and swallowed (WF4.2), and a
/// drain failure warns + treats as an empty batch (it must never eat
/// the terminal).
///
/// The drain is gated on chain admissibility (BR6.2): queued rows move
/// to transcript ONLY while a successor could actually run — a disabled
/// slot or a post-usage `budget_exceeded` leaves them QUEUED for the
/// next admission, never consumed into the transcript unanswered.
async fn finish_turn(
    state: &AppState,
    svc: &Arc<dyn SessionService>,
    session: &OxpilotSessionRow,
    turn: &Arc<ActiveTurn>,
    row_exists: bool,
    terminal: Terminal,
    accum: &TurnAccum,
) -> TurnOutcome {
    if row_exists {
        let status = if matches!(terminal, Terminal::Done) {
            "complete"
        } else {
            "error"
        };
        if let Err(e) = svc
            .set_message_status(&session.user_id, &turn.message_id, status)
            .await
        {
            tracing::warn!(
                session_id = %session.id,
                error = %e.message,
                "oxpilot terminal status write failed"
            );
        }
    }
    if accum.any_items {
        if let Err(e) = svc
            .record_usage(
                &session.user_id,
                UsageDelta {
                    requests: 1,
                    tokens: Some(accum.tokens),
                },
            )
            .await
        {
            tracing::warn!(
                session_id = %session.id,
                error = %e.message,
                "oxpilot usage record failed"
            );
        }
    }
    // The drain gate — evaluated AFTER usage accounting so the recheck
    // sees this turn's own spend (BR6.2's at/over-the-cap refusal).
    let chain_open = {
        let slot = state.oxpilot.slot.read().await;
        if slot.disabled_reason.is_some() {
            tracing::debug!(
                session_id = %session.id,
                "oxpilot end-of-turn drain skipped — slot disabled"
            );
            false
        } else {
            let caps = slot.budget_caps;
            drop(slot);
            match svc.check_budget(&session.user_id, caps).await {
                Ok(()) => true,
                Err(e) => {
                    // A breach keeps rows queued; any other check
                    // failure fails CLOSED — the budget gate never
                    // opens on a blind read.
                    if e.code == "budget_exceeded" {
                        tracing::debug!(
                            session_id = %session.id,
                            "oxpilot end-of-turn drain skipped — budget exhausted"
                        );
                    } else {
                        tracing::warn!(
                            session_id = %session.id,
                            error = %e.message,
                            "oxpilot end-of-turn drain skipped — budget check failed"
                        );
                    }
                    false
                }
            }
        }
    };
    let drained = if !chain_open {
        Vec::new()
    } else {
        match svc.drain_queue(&session.user_id, &session.id).await {
            Ok(rows) => {
                if !rows.is_empty() {
                    tracing::debug!(
                        session_id = %session.id,
                        drained_count = rows.len(),
                        generation = turn.generation,
                        "oxpilot drain chain"
                    );
                }
                rows
            }
            Err(e) => {
                tracing::warn!(
                    session_id = %session.id,
                    error = %e.message,
                    "oxpilot end-of-turn drain failed"
                );
                Vec::new()
            }
        }
    };
    let (result, code) = match &terminal {
        Terminal::Done => ("done", None),
        Terminal::Aborted => ("aborted", None),
        Terminal::Error(c) => ("error", Some(c.as_str())),
    };
    tracing::debug!(
        session_id = %session.id,
        result = %result,
        code = code.unwrap_or("-"),
        "oxpilot terminal"
    );
    emit_terminal(turn, terminal_json(turn, &terminal)).await;
    TurnOutcome { drained }
}

/// What `run_turn` hands its wrapper — the in-turn drain batch (BR4.1's
/// pre-terminal drain); the wrapper's locked recheck adds to it for
/// the chain decision.
struct TurnOutcome {
    drained: Vec<OxpilotMessageRow>,
}

/// Mint the refusal turn's durable assistant row — `streaming`, then
/// `finish_turn` lands the terminal `error` status on it (the same
/// insert-then-flip order the live path takes, BR2.4). Returns whether
/// the row persisted so the status write can skip cleanly on
/// post-cascade `not_found`.
async fn mint_refusal_row(
    svc: &Arc<dyn SessionService>,
    session: &OxpilotSessionRow,
    turn: &Arc<ActiveTurn>,
) -> bool {
    svc.append_message(
        &session.user_id,
        AppendMessage {
            id: turn.message_id.clone(),
            session_id: session.id.clone(),
            role: "assistant".to_string(),
            content: String::new(),
            client_msg_id: None,
            status: "streaming".to_string(),
        },
    )
    .await
    .is_ok()
}

/// WF2 — pin the `turn_parts()` triple, mint the assistant row, run the
/// bounded round loop, and land the terminal sequence. NEVER releases
/// the claim — the wrapper owns release+recheck (BR5.3).
async fn run_turn(
    state: &AppState,
    session: &OxpilotSessionRow,
    turn: &Arc<ActiveTurn>,
) -> Result<TurnOutcome, AppError> {
    let svc = state.oxpilot.sessions()?;
    let mut accum = TurnAccum::new();

    // WF2.1 — pin (provider, model, executor) under ONE slot read; a
    // mid-turn settings swap can't tear the set (BR1.3/NFR4.5).
    let parts = state.oxpilot.turn_parts().await;
    let Some(provider) = parts.provider else {
        // Post-gate provider-absent — the disabled-class terminal
        // (a settings write raced the admission gate). The refusal row
        // mints durable-first so a drained follow-up batch never sits
        // transcript-visible with no outcome.
        let row_exists = mint_refusal_row(&svc, session, turn).await;
        return Ok(finish_turn(
            state,
            &svc,
            session,
            turn,
            row_exists,
            Terminal::Error("oxpilot.disabled".to_string()),
            &accum,
        )
        .await);
    };
    let Some(model) = parts.model else {
        // Provider with no resolved model — a typed refusal before any
        // stream opens (NFR4.9), never a fabricated request. Same
        // durable-refusal row as the provider-absent arm.
        let row_exists = mint_refusal_row(&svc, session, turn).await;
        return Ok(finish_turn(
            state,
            &svc,
            session,
            turn,
            row_exists,
            Terminal::Error("no_model".to_string()),
            &accum,
        )
        .await);
    };
    let executor = parts.executor;
    tracing::info!(
        session_id = %session.id,
        turn_id = %turn.turn_id,
        provider_id = %provider.id(),
        model = %model,
        "oxpilot turn start"
    );

    // WF2.2 — the assistant row (`streaming`, caller-minted id, BR2.1).
    let assistant_row = svc
        .append_message(
            &session.user_id,
            AppendMessage {
                id: turn.message_id.clone(),
                session_id: session.id.clone(),
                role: "assistant".to_string(),
                content: String::new(),
                client_msg_id: None,
                status: "streaming".to_string(),
            },
        )
        .await?;
    debug_assert_eq!(
        assistant_row.seq,
        turn.started_seq.load(Ordering::SeqCst) + 1,
        "assistant row must follow the user row's seq"
    );

    // WF2.3 — bounded prompt: system preamble + repo binding + goal ride
    // `ChatRequest.system`; the transcript window + catalog fill `req`.
    let mut messages = assemble_prompt(&svc, session).await?;
    let system = system_prompt(session);
    let tools = executor.catalog();
    let deadline = tokio::time::Instant::now() + TURN_MAX_DURATION;

    // WF2.4 — the round loop: each iteration opens one stream and
    // consumes it; a call-bearing round executes its calls and feeds the
    // results back via the round buffer. At most MAX_TOOL_ROUNDS rounds
    // execute tools; the (bound+1)-th call-bearing round leaves its
    // calls pending — error cards + terminal (BR3.3).
    let terminal: Terminal;
    let mut round = 0usize;
    loop {
        let req = ChatRequest {
            model: model.clone(),
            system: system.clone(),
            messages: messages.clone(),
            tools: tools.clone(),
        };
        // The open await is raced like every other turn await point
        // (WF2): a stalled open can't hold the claim past an abort or
        // the wall, and an already-cancelled/dead turn never burns a
        // fresh provider request.
        if turn.cancel.is_cancelled() {
            terminal = if accum.wall_expired {
                Terminal::Error("turn_timeout".to_string())
            } else {
                Terminal::Aborted
            };
            break;
        }
        let mut stream = tokio::select! {
            _ = turn.cancel.cancelled() => {
                terminal = if accum.wall_expired {
                    Terminal::Error("turn_timeout".to_string())
                } else {
                    Terminal::Aborted
                };
                break;
            }
            _ = tokio::time::sleep_until(deadline) => {
                accum.wall_expired = true;
                turn.cancel.cancel();
                terminal = Terminal::Error("turn_timeout".to_string());
                break;
            }
            opened = provider.stream(req) => match opened {
                Ok(s) => s,
                Err(e) => {
                    // Open failure — the provider's stable code rides
                    // the terminal; nothing streamed so nothing is
                    // counted.
                    tracing::warn!(
                        code = %e.code,
                        error = %e.message,
                        "provider stream failed to open"
                    );
                    terminal = Terminal::Error(e.code);
                    break;
                }
            }
        };
        let res = run_round(&mut stream, &svc, session, turn, &mut accum, deadline).await?;
        if let Some(u) = res.usage {
            accum.tokens += (u.input_tokens + u.output_tokens) as i64;
        }
        accum.any_items |= res.any_items;
        let exec_ctx = ExecCtx {
            svc: &svc,
            session,
            turn,
            executor: executor.as_ref(),
            assistant_seq: assistant_row.seq,
            deadline,
        };
        if let Some(t) = res.terminal {
            // Calls the provider emitted before dying/cancelling were
            // never invoked — each still earns a durable error card
            // (same discipline as the rounds cap; the card set is the
            // honest record of what the model asked for). The card's
            // summary is the terminal's own reason.
            if !res.calls.is_empty() {
                let reason = match &t {
                    Terminal::Error(c) => c.clone(),
                    Terminal::Aborted => "aborted".to_string(),
                    Terminal::Done => "provider_error".to_string(),
                };
                fail_pending_calls(&exec_ctx, res.calls, &mut accum, &reason).await;
            }
            terminal = t;
            break;
        }
        if res.calls.is_empty() {
            terminal = Terminal::Done;
            break;
        }
        if round == MAX_TOOL_ROUNDS {
            fail_pending_calls(&exec_ctx, res.calls, &mut accum, "tool_rounds_exceeded").await;
            terminal = Terminal::Error("tool_rounds_exceeded".to_string());
            break;
        }
        let executed = execute_calls(&exec_ctx, res.calls, &mut accum).await;
        if !executed.is_empty() {
            // The assistant round message carries the PER-ROUND delta
            // slice + the serialized call array; each result follows as
            // a `role:"tool"` msg in call order (BR3.2).
            let calls_json = serde_json::to_string(
                &executed
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.call_id,
                            "name": c.name,
                            "args_json": c.args_json,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_else(|_| "[]".to_string());
            messages.push(ChatMsg {
                role: "assistant".to_string(),
                content: res.text,
                tool_call_id: None,
                tool_name: None,
                tool_calls_json: Some(calls_json),
            });
            for c in executed {
                messages.push(ChatMsg {
                    role: "tool".to_string(),
                    content: c.result_content,
                    tool_call_id: Some(c.call_id),
                    tool_name: Some(c.name),
                    tool_calls_json: None,
                });
            }
        }
        round += 1;
    }
    Ok(finish_turn(state, &svc, session, turn, true, terminal, &accum).await)
}

// ---------- wrapper + chain ----------

/// The detached turn task (BR4.6): inner `tokio::spawn` + `JoinHandle`
/// await — a panicking turn can never escape release — then
/// `drain_and_chain` under the admission lock. `run_turn` NEVER calls
/// `release` — the wrapper owns it on every exit shape (BR5.3).
pub(crate) fn spawn_turn_task(state: AppState, session: OxpilotSessionRow, turn: Arc<ActiveTurn>) {
    let span = tracing::info_span!(
        "oxpilot.turn",
        session_id = %session.id,
        turn_id = %turn.turn_id,
        message_id = %turn.message_id,
        repo_id = %session.repo_id,
    );
    let internal = json!({
        "type": "terminal",
        "message_id": turn.message_id,
        "result": "error",
        "code": "internal",
    });
    tokio::spawn(
        async move {
            let inner = {
                let state = state.clone();
                let session = session.clone();
                let turn = turn.clone();
                tokio::spawn(async move { run_turn(&state, &session, &turn).await })
            };
            let outcome = match inner.await {
                Ok(Ok(outcome)) => Some(outcome),
                Ok(Err(e)) => {
                    tracing::error!(error = %e.message, code = %e.code, "oxpilot turn task failed");
                    // Best-effort error row — post-cascade misses are
                    // logged, swallowed (WF4.2).
                    if let Ok(svc) = state.oxpilot.sessions() {
                        let _ = svc
                            .set_message_status(&session.user_id, &turn.message_id, "error")
                            .await;
                    }
                    emit_terminal(&turn, internal.clone()).await;
                    None
                }
                Err(je) => {
                    // Panic/cancellation of the inner task — the CAS
                    // makes the internal frame a no-op if a terminal
                    // already landed (NFR4.2).
                    tracing::warn!(
                        session_id = %session.id,
                        error = %je,
                        "turn task join error"
                    );
                    if let Ok(svc) = state.oxpilot.sessions() {
                        let _ = svc
                            .set_message_status(&session.user_id, &turn.message_id, "error")
                            .await;
                    }
                    emit_terminal(&turn, internal).await;
                    None
                }
            };
            drain_and_chain(&state, &session, &turn, outcome).await;
        }
        .instrument(span),
    );
}

/// Post-turn release + recheck drain + optional chain — the WHOLE
/// `release` → `drain_queue` → `claim` section runs under the session's
/// admission lock, so a `message_queue` commit can never land inside the
/// release→recheck blind window (BR4.1 + R-20's airtight seam). The
/// in-turn drain's batch merges into the chain decision (WF3.7).
async fn drain_and_chain(
    state: &AppState,
    session: &OxpilotSessionRow,
    turn: &Arc<ActiveTurn>,
    outcome: Option<TurnOutcome>,
) {
    let turns = state.oxpilot.turns().await;
    let Some((_entry, lock)) = turns.admission_lock(&session.id).await else {
        // Entry already gone — nothing left to release or drain.
        return;
    };
    let _g = lock.lock().await;
    turns.release(&session.id, &turn.turn_id).await;
    let mut batch = outcome.map(|o| o.drained).unwrap_or_default();
    let svc = match state.oxpilot.sessions() {
        Ok(s) => s,
        Err(e) => {
            // The recheck can't run — no chain on a blind batch. Rows
            // stay durable for the next POST's residual drain.
            tracing::warn!(
                session_id = %session.id,
                error = %e.message,
                "oxpilot post-release drain skipped — sessions unavailable"
            );
            return;
        }
    };
    // Chain admissibility — the SAME gate the in-turn drain took, now
    // re-evaluated under the admission lock against the post-usage
    // counters and the live slot (BR6.2). A blocked chain leaves every
    // queued row intact for the next admission — the successor claim
    // can never outrun the daily cap or a settings-write disable.
    {
        let slot = state.oxpilot.slot.read().await;
        if slot.disabled_reason.is_some() {
            tracing::debug!(
                session_id = %session.id,
                "oxpilot chain skipped — slot disabled"
            );
            return;
        }
        let caps = slot.budget_caps;
        drop(slot);
        if let Err(e) = svc.check_budget(&session.user_id, caps).await {
            if e.code == "budget_exceeded" {
                tracing::debug!(
                    session_id = %session.id,
                    "oxpilot chain skipped — budget exhausted"
                );
            } else {
                tracing::warn!(
                    session_id = %session.id,
                    error = %e.message,
                    "oxpilot chain skipped — budget check failed"
                );
            }
            return;
        }
    }
    match svc.drain_queue(&session.user_id, &session.id).await {
        Ok(rows) => {
            if !rows.is_empty() {
                tracing::debug!(
                    session_id = %session.id,
                    drained_count = rows.len(),
                    generation = turn.generation,
                    "oxpilot drain chain"
                );
            }
            batch.extend(rows);
        }
        Err(e) => {
            // BR4.1: a recheck-drain `Err` under the lock warns and
            // exits WITHOUT consulting the in-turn batch — chaining a
            // successor on a stale outcome would spawn a doomed turn.
            tracing::warn!(
                session_id = %session.id,
                error = %e.message,
                "oxpilot post-release drain failed"
            );
            return;
        }
    }
    if batch.is_empty() || turn.generation >= MAX_CHAIN_DEPTH {
        return;
    }
    // The LAST drained row's seq seeds `started_seq` — the assistant row
    // mints `seq + 1`, and a resend of that row's cmid attaches via
    // `resend_matches`'s `started_seq == prior.seq` arm. Pending fields
    // reset — drained rows are durable transcript (WF3.7).
    let seed_seq = batch.last().map(|r| r.seq).unwrap_or(0);
    let session_id = session.id.clone();
    let next = turns
        .claim(&session.id, || {
            let (fanout, _) = broadcast::channel(TURN_FANOUT_CAPACITY);
            ActiveTurn {
                session_id: session_id.clone(),
                turn_id: Uuid::new_v4().to_string(),
                message_id: Uuid::new_v4().to_string(),
                started_seq: std::sync::atomic::AtomicI64::new(seed_seq),
                pending_client_msg_id: None,
                pending_content: String::new(),
                fanout,
                emitted_deltas: tokio::sync::Mutex::new(Vec::new()),
                cancel: CancellationToken::new(),
                admission_lock: Arc::new(tokio::sync::Mutex::new(())),
                terminal_sent: std::sync::atomic::AtomicBool::new(false),
                generation: turn.generation + 1,
            }
        })
        .await;
    match next {
        Some(next) => {
            // `_g` is still held — the new turn's OWN lock is a different
            // Mutex; spawning here can't deadlock its drain_and_chain.
            spawn_turn_task(state.clone(), session.clone(), next);
        }
        None => {
            // Claim-loss — a POST won the gap; its residual drain +
            // prompt window cover the delivered rows. Quiet exit.
        }
    }
}

// ---------- C8 delete hook ----------

/// C8 hook #1 — abort a live turn BEFORE the session cascade delete.
/// Idempotent: no live turn ⇒ `Ok(())`; the doomed-session recheck
/// exits quietly via the claim-loss arm.
pub struct OxpilotTurnAbortHook {
    runtime: SharedOxpilot,
}

impl OxpilotTurnAbortHook {
    pub fn new(runtime: SharedOxpilot) -> Self {
        OxpilotTurnAbortHook { runtime }
    }
}

#[async_trait]
impl SessionDeleteHook for OxpilotTurnAbortHook {
    async fn on_session_delete(&self, session_id: &str, _user_id: &str) -> Result<(), AppError> {
        if let Some(turn) = self.runtime.turns().await.get(session_id).await {
            // CAS — only the first abort warns (BR7.1); a terminal that
            // already landed leaves no abort to report.
            if emit_terminal(
                &turn,
                json!({
                    "type": "terminal",
                    "message_id": turn.message_id,
                    "result": "aborted",
                }),
            )
            .await
            {
                tracing::warn!(session_id = %session_id, "oxpilot turn aborted");
            }
            turn.cancel.cancel();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! U3 runtime unit tests — a REAL SQLite `Database` under the real
    //! `OxpilotSessionService` (the behavior under test IS the store
    //! composition); providers are scripted fakes, never network.
    //! Wiremock-only adapters live in `tests/oxpilot_providers.rs`.

    use super::*;
    use oxidean_api_core::email::{EmailSender, LogSink};
    use oxidean_api_core::oxpilot::{
        ActiveTurns, AppendMessage as _AppendMessage, BudgetCaps, ChatStream, CreateSession,
        LlmProvider, OxpilotRuntime, OxpilotRuntimeSlot, ProviderError, ToolSpec, Usage as _Usage,
    };
    use std::collections::VecDeque;
    use std::sync::Mutex as StdMutex;

    use crate::services::OxpilotSessionService;
    use oxidean_db::Database;

    // ---------- fixtures ----------

    async fn test_db() -> Database {
        let db = Database::connect("sqlite::memory:").await.expect("connect");
        db.migrate().await.expect("migrate");
        db
    }

    async fn seed(db: &Database, tag: &str) -> (String, String) {
        let uid = format!("u-rt-{tag}");
        db.create_user(
            &uid,
            &format!("rt-{tag}@example.com"),
            &format!("rt{tag}"),
            None,
            "RT Seed",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("seed user");
        let repo = format!("repo-rt-{tag}");
        db.insert_repository(&repo, &uid, "user", &repo, "private", "", "main")
            .await
            .expect("seed repo");
        (uid, repo)
    }

    fn svc(db: &Database) -> Arc<dyn SessionService> {
        Arc::new(OxpilotSessionService::new(db.clone()))
    }

    async fn session_for(
        svc: &Arc<dyn SessionService>,
        uid: &str,
        repo: &str,
        id: &str,
    ) -> OxpilotSessionRow {
        svc.create_session(
            uid,
            CreateSession {
                id: id.to_string(),
                repo_owner: "oxpilot".to_string(),
                repo_name: repo.to_string(),
                repo_id: repo.to_string(),
                branch: "main".to_string(),
                goal: Some("ship it".to_string()),
                title: "t".to_string(),
                computer_use_target_id: None,
            },
        )
        .await
        .expect("create session")
    }

    fn turn(session_id: &str, generation: u32) -> ActiveTurn {
        let (fanout, _) = broadcast::channel(TURN_FANOUT_CAPACITY);
        ActiveTurn {
            session_id: session_id.to_string(),
            turn_id: Uuid::new_v4().to_string(),
            message_id: Uuid::new_v4().to_string(),
            started_seq: std::sync::atomic::AtomicI64::new(0),
            pending_client_msg_id: None,
            pending_content: String::new(),
            fanout,
            emitted_deltas: tokio::sync::Mutex::new(Vec::new()),
            cancel: CancellationToken::new(),
            admission_lock: Arc::new(tokio::sync::Mutex::new(())),
            terminal_sent: std::sync::atomic::AtomicBool::new(false),
            generation,
        }
    }

    /// A turn claimed into a fresh `ActiveTurns` map (returns the Arc the
    /// map holds — identity guards need the same turn_id).
    async fn claimed(session_id: &str, generation: u32) -> (ActiveTurns, Arc<ActiveTurn>) {
        let turns = ActiveTurns::default();
        let t = turns
            .claim(session_id, || turn(session_id, generation))
            .await
            .expect("claim");
        (turns, t)
    }

    /// A full app state: real service installed on a runtime carrying the
    /// given provider + tools.
    async fn state_with(
        db: &Database,
        provider: Option<Arc<dyn LlmProvider>>,
        model: Option<&str>,
        tools: Option<Arc<dyn ToolExecutor>>,
    ) -> AppState {
        let svc = svc(db);
        let runtime = OxpilotRuntime::from_slot(OxpilotRuntimeSlot {
            provider,
            model: model.map(str::to_string),
            disabled_reason: None,
            turns: Arc::new(ActiveTurns::default()),
            built_from: None,
            budget_caps: BudgetCaps::default(),
            tools,
        });
        runtime.install_sessions(svc);
        AppState::new(
            db.clone(),
            Arc::new(LogSink) as Arc<dyn EmailSender>,
            "development",
        )
        .with_oxpilot(runtime)
    }

    /// Queued-reply provider — each `stream()` pops the next canned
    /// items-or-error; captures requests for assertions.
    struct ScriptProvider {
        scripts: StdMutex<VecDeque<Result<Vec<ChatStreamItem>, ProviderError>>>,
        requests: StdMutex<Vec<ChatRequest>>,
    }

    impl ScriptProvider {
        fn new(scripts: Vec<Result<Vec<ChatStreamItem>, ProviderError>>) -> Self {
            ScriptProvider {
                scripts: StdMutex::new(scripts.into()),
                requests: StdMutex::new(Vec::new()),
            }
        }
        fn requests(&self) -> Vec<ChatRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl LlmProvider for ScriptProvider {
        fn id(&self) -> &'static str {
            "script"
        }

        async fn stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
            self.requests.lock().unwrap().push(req);
            let next = self
                .scripts
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(vec![ChatStreamItem::Done { usage: None }]));
            next.map(|items| Box::pin(futures_util::stream::iter(items)) as ChatStream)
        }
    }

    /// Provider whose stream() panics — proves the wrapper's JoinError arm.
    struct PanicProvider;

    #[async_trait]
    impl LlmProvider for PanicProvider {
        fn id(&self) -> &'static str {
            "panic"
        }
        async fn stream(&self, _req: ChatRequest) -> Result<ChatStream, ProviderError> {
            panic!("scripted provider panic");
        }
    }

    /// Executor that echoes args back as the tool output.
    struct EchoTools;

    #[async_trait]
    impl ToolExecutor for EchoTools {
        fn catalog(&self) -> Vec<ToolSpec> {
            vec![ToolSpec {
                name: "echo".to_string(),
                description: "echo args".to_string(),
                parameters_json: json!({"type": "object"}),
            }]
        }
        async fn invoke(&self, call: ToolCall, ctx: ToolCtx) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput {
                content: format!("echo:{}:{}", ctx.session_id, call.args_json),
                output_summary: format!("echoed {}", call.name),
            })
        }
    }

    /// Executor that fails every call with a fixed code.
    struct FailTools(&'static str);

    #[async_trait]
    impl ToolExecutor for FailTools {
        fn catalog(&self) -> Vec<ToolSpec> {
            vec![ToolSpec {
                name: "boom".to_string(),
                description: "fails".to_string(),
                parameters_json: json!({"type": "object"}),
            }]
        }
        async fn invoke(&self, _call: ToolCall, _ctx: ToolCtx) -> Result<ToolOutput, ToolError> {
            Err(ToolError {
                code: self.0.to_string(),
                message: format!("failure {}", self.0),
            })
        }
    }

    /// Executor that never answers — timeout/cancel probe.
    struct HangTools;

    #[async_trait]
    impl ToolExecutor for HangTools {
        fn catalog(&self) -> Vec<ToolSpec> {
            Vec::new()
        }
        async fn invoke(&self, _call: ToolCall, _ctx: ToolCtx) -> Result<ToolOutput, ToolError> {
            std::future::pending().await
        }
    }

    async fn appended(
        svc: &Arc<dyn SessionService>,
        uid: &str,
        session_id: &str,
        role: &str,
        content: &str,
    ) -> OxpilotMessageRow {
        svc.append_message(
            uid,
            _AppendMessage {
                id: Uuid::new_v4().to_string(),
                session_id: session_id.to_string(),
                role: role.to_string(),
                content: content.to_string(),
                client_msg_id: None,
                status: "complete".to_string(),
            },
        )
        .await
        .expect("append")
    }

    async fn buffered(turn: &Arc<ActiveTurn>) -> Vec<Value> {
        turn.emitted_deltas.lock().await.clone()
    }

    /// Async poll — detached-task completion waits, ~3s bound.
    async fn eventually_async<Fut, P>(mut p: P) -> bool
    where
        P: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        for _ in 0..300 {
            if p().await {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    // ---------- emit_terminal CAS (BR5.4) ----------

    #[tokio::test]
    async fn emit_terminal_first_wins() {
        let (_turns, t) = claimed("s1", 0).await;
        let mut rx = t.fanout.subscribe();
        let first = emit_terminal(&t, json!({"type": "terminal", "result": "done"})).await;
        let second = emit_terminal(&t, json!({"type": "terminal", "result": "aborted"})).await;
        assert!(first);
        assert!(!second);
        let buf = buffered(&t).await;
        assert_eq!(buf.len(), 1);
        assert_eq!(buf[0]["result"], "done");
        // The live receiver saw exactly the winner.
        assert_eq!(rx.recv().await.unwrap()["result"], "done");
    }

    #[tokio::test]
    async fn emit_terminal_concurrent_exactly_one_wins() {
        let (_turns, t) = claimed("s1", 0).await;
        let mut handles = Vec::new();
        for i in 0..8 {
            let t = t.clone();
            handles.push(tokio::spawn(async move {
                emit_terminal(&t, json!({"type": "terminal", "result": format!("r{i}")})).await
            }));
        }
        let mut wins = 0;
        for h in handles {
            if h.await.unwrap() {
                wins += 1;
            }
        }
        assert_eq!(wins, 1);
        assert_eq!(buffered(&t).await.len(), 1);
        assert!(t.terminal_sent.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn emit_frame_drops_after_terminal() {
        // C2: frames after terminal are never emitted — the abort path
        // emits terminal then cancels; late error-card frames from
        // invoke_call/fail_pending_calls must not land in the replay
        // buffer or on live readers.
        let (_turns, t) = claimed("s1", 0).await;
        let mut rx = t.fanout.subscribe();
        assert!(emit_terminal(&t, json!({"type": "terminal", "result": "aborted"})).await);
        emit_frame(
            &t,
            json!({"type": "tool", "status": "error", "code": "aborted"}),
        )
        .await;
        emit_frame(&t, json!({"type": "append", "delta": "late"})).await;
        let buf = buffered(&t).await;
        assert_eq!(buf.len(), 1);
        assert_eq!(buf[0]["result"], "aborted");
        // The live reader saw only the terminal.
        assert_eq!(rx.recv().await.unwrap()["result"], "aborted");
        assert!(rx.try_recv().is_err());
    }

    // ---------- id minting / bounds ----------

    #[test]
    fn effective_call_id_mints_when_empty() {
        assert_eq!(effective_call_id("m1", "", 0), "call_m1_0");
        assert_eq!(effective_call_id("m1", "  ", 3), "call_m1_3");
        assert_eq!(effective_call_id("m1", "tc_9", 0), "tc_9");
    }

    #[test]
    fn truncate_marker_bounds_and_char_boundary() {
        assert_eq!(truncate_marker("short", 10), "short");
        let s = "x".repeat(100);
        let out = truncate_marker(&s, 10);
        assert!(out.len() <= 10);
        assert!(out.ends_with('…'));
        // Multibyte boundary safety — truncating mid-char must not panic.
        let mb = "é".repeat(50); // 2 bytes each
        let out = truncate_marker(&mb, 11);
        assert!(out.len() <= 11);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn tool_summary_line_bounded() {
        let card = OxpilotToolCallRow {
            id: "c".into(),
            session_id: "s".into(),
            message_id: "m".into(),
            seq: 1,
            call_index: 0,
            call_id: "x".into(),
            tool: "t".repeat(500),
            args_summary: "a".into(),
            status: "done".into(),
            output_summary: String::new(),
            created_at: "now".into(),
        };
        let line = tool_summary_line(&card);
        assert!(line.len() <= TOOL_SUMMARY_LINE_MAX);
        assert!(line.starts_with("\n[tool_call "));
    }

    // ---------- prompt assembly (BR2.3) ----------

    #[tokio::test]
    async fn assemble_prompt_newest_first_bounded_temporal_order() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "ap1").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-ap1").await;
        // 6 × 40 KiB msgs = 240 KiB > 128 KiB window — oldest dropped.
        for i in 0..6 {
            appended(
                &svc,
                &uid,
                &session.id,
                "user",
                &format!("{i}:{}", "x".repeat(39_990)), // ~40 KiB each
            )
            .await;
        }
        let window = assemble_prompt(&svc, &session).await.unwrap();
        assert!(window.len() < 6);
        let total: usize = window.iter().map(rendered_len).sum();
        assert!(total <= PROMPT_TRANSCRIPT_MAX_BYTES);
        // Temporal order — newest kept, oldest first in output.
        let last = window.last().unwrap();
        assert!(last.content.starts_with("5:"));
        let first = window.first().unwrap();
        // index of first kept message — 6 - window.len()
        let expect = 6 - window.len();
        assert!(first.content.starts_with(&format!("{expect}:")));
    }

    #[tokio::test]
    async fn assemble_prompt_tool_summary_lines() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "ap2").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-ap2").await;
        let row = appended(&svc, &uid, &session.id, "assistant", "answer").await;
        svc.append_tool_call(
            &uid,
            ToolCallWrite {
                session_id: session.id.clone(),
                message_id: row.id.clone(),
                seq: row.seq,
                call_index: 0,
                call_id: "c0".to_string(),
                tool: "lookup".to_string(),
                args_summary: "{}".to_string(),
                status: "done".to_string(),
            },
        )
        .await
        .expect("card");
        let window = assemble_prompt(&svc, &session).await.unwrap();
        assert_eq!(window.len(), 1);
        assert!(window[0].content.contains("[tool_call lookup → done]"));
    }

    // ---------- run_round ----------

    #[tokio::test]
    async fn run_round_collects_deltas_calls_and_done() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "rr1").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-rr1").await;
        let (_turns, t) = claimed(&session.id, 0).await;
        // The assistant row must exist for set_message_content.
        let user_row = appended(&svc, &uid, &session.id, "user", "hi").await;
        t.started_seq.store(user_row.seq, Ordering::SeqCst);
        svc.append_message(
            &uid,
            _AppendMessage {
                id: t.message_id.clone(),
                session_id: session.id.clone(),
                role: "assistant".to_string(),
                content: String::new(),
                client_msg_id: None,
                status: "streaming".to_string(),
            },
        )
        .await
        .unwrap();
        let items = vec![
            ChatStreamItem::Delta("he".into()),
            ChatStreamItem::Delta("llo".into()),
            ChatStreamItem::ToolCall {
                id: "tc1".into(),
                name: "lookup".into(),
                args_json: "{}".into(),
            },
            ChatStreamItem::Done {
                usage: Some(_Usage {
                    input_tokens: 3,
                    output_tokens: 5,
                }),
            },
        ];
        let mut stream: ChatStream = Box::pin(futures_util::stream::iter(items));
        let mut accum = TurnAccum::new();
        let res = run_round(
            &mut stream,
            &svc,
            &session,
            &t,
            &mut accum,
            tokio::time::Instant::now() + Duration::from_secs(60),
        )
        .await
        .unwrap();
        assert_eq!(res.text, "hello");
        assert_eq!(res.calls.len(), 1);
        assert!(res.terminal.is_none());
        assert!(res.any_items);
        assert_eq!(accum.delta_seq, 2);
        assert_eq!(accum.content, "hello");
        // Frames: two append frames emitted.
        let buf = buffered(&t).await;
        assert_eq!(buf.len(), 2);
        assert_eq!(buf[0]["type"], "append");
        assert_eq!(buf[1]["seq"], 2);
        // The row accumulated the content.
        let msgs = svc.list_messages(&uid, &session.id).await.unwrap();
        let arow = msgs.iter().find(|m| m.id == t.message_id).unwrap();
        assert_eq!(arow.content, "hello");
    }

    #[tokio::test]
    async fn run_round_silent_end_is_provider_error() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "rr2").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-rr2").await;
        let (_turns, t) = claimed(&session.id, 0).await;
        let mut stream: ChatStream = Box::pin(futures_util::stream::empty());
        let mut accum = TurnAccum::new();
        let res = run_round(
            &mut stream,
            &svc,
            &session,
            &t,
            &mut accum,
            tokio::time::Instant::now() + Duration::from_secs(60),
        )
        .await
        .unwrap();
        match res.terminal {
            Some(Terminal::Error(c)) => assert_eq!(c, "provider_error"),
            _ => panic!("silent end must terminal-error"),
        }
        assert!(!res.any_items);
    }

    #[tokio::test]
    async fn run_round_deadline_yields_turn_timeout() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "rr3").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-rr3").await;
        let (_turns, t) = claimed(&session.id, 0).await;
        // A stream that never yields — the wall deadline must fire.
        let mut stream: ChatStream = Box::pin(futures_util::stream::pending());
        let mut accum = TurnAccum::new();
        let res = run_round(
            &mut stream,
            &svc,
            &session,
            &t,
            &mut accum,
            tokio::time::Instant::now() + Duration::from_millis(10),
        )
        .await
        .unwrap();
        match res.terminal {
            Some(Terminal::Error(c)) => assert_eq!(c, "turn_timeout"),
            _ => panic!("deadline must terminal-error turn_timeout"),
        }
        assert!(accum.wall_expired);
        assert!(t.cancel.is_cancelled());
    }

    #[tokio::test]
    async fn run_round_cancel_is_aborted() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "rr4").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-rr4").await;
        let (_turns, t) = claimed(&session.id, 0).await;
        t.cancel.cancel(); // pre-cancelled
        let mut stream: ChatStream = Box::pin(futures_util::stream::pending());
        let mut accum = TurnAccum::new();
        let res = run_round(
            &mut stream,
            &svc,
            &session,
            &t,
            &mut accum,
            tokio::time::Instant::now() + Duration::from_secs(60),
        )
        .await
        .unwrap();
        assert!(matches!(res.terminal, Some(Terminal::Aborted)));
        assert!(!accum.wall_expired);
    }

    // ---------- invoke_call races ----------

    #[tokio::test]
    async fn invoke_call_ok_timeout_and_abort() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "ic").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-ic").await;
        let (_turns, t) = claimed(&session.id, 0).await;
        let far = tokio::time::Instant::now() + Duration::from_secs(60);
        let mut accum = TurnAccum::new();
        let call = ToolCall {
            call_id: "c0".into(),
            name: "echo".into(),
            args_json: "{}".into(),
        };
        // OK
        let out = invoke_call(
            &EchoTools,
            &session,
            &t,
            call.clone(),
            &mut accum,
            far,
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        assert!(out.content.starts_with("echo:"));
        // Timeout — HangTools never answers; the small call_timeout wins.
        let err = invoke_call(
            &HangTools,
            &session,
            &t,
            call.clone(),
            &mut accum,
            far,
            Duration::from_millis(20),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "timeout");
        // Abort — a pre-cancelled token wins immediately.
        t.cancel.cancel();
        let err = invoke_call(
            &EchoTools,
            &session,
            &t,
            call,
            &mut accum,
            far,
            Duration::from_secs(60),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "aborted");
    }

    // ---------- execute_calls ----------

    #[tokio::test]
    async fn execute_calls_persists_cards_and_feeds_results() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "ec").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-ec").await;
        let user_row = appended(&svc, &uid, &session.id, "user", "hi").await;
        let (_turns, t) = claimed(&session.id, 0).await;
        t.started_seq.store(user_row.seq, Ordering::SeqCst);
        // The assistant row's id IS the turn's message_id — the card's
        // message_id FK binds them.
        let arow = svc
            .append_message(
                &uid,
                _AppendMessage {
                    id: t.message_id.clone(),
                    session_id: session.id.clone(),
                    role: "assistant".to_string(),
                    content: String::new(),
                    client_msg_id: None,
                    status: "streaming".to_string(),
                },
            )
            .await
            .unwrap();
        let mut accum = TurnAccum::new();
        let calls = vec![
            ("".to_string(), "echo".to_string(), "{\"q\":1}".to_string()),
            ("tc2".to_string(), "boom".to_string(), "{}".to_string()),
        ];
        // Two executors can't mix in one call batch through this seam —
        // EchoTools runs both (the `boom` name is catalog-agnostic here);
        // FailTools coverage is the next test.
        let ctx = ExecCtx {
            svc: &svc,
            session: &session,
            turn: &t,
            executor: &EchoTools,
            assistant_seq: arow.seq,
            deadline: tokio::time::Instant::now() + Duration::from_secs(60),
        };
        let out = execute_calls(&ctx, calls, &mut accum).await;
        assert_eq!(out.len(), 2);
        // Empty provider id ⇒ deterministic mint.
        assert_eq!(out[0].call_id, format!("call_{}_0", t.message_id));
        assert_eq!(out[1].call_id, "tc2");
        // Durable cards — both done via EchoTools.
        let cards = svc.list_tool_calls(&uid, &session.id).await.unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].status, "done");
        assert_eq!(cards[0].call_index, 0);
        assert_eq!(cards[1].call_index, 1);
        // Frames: running + done per call = 4, in order.
        let buf = buffered(&t).await;
        assert_eq!(buf.len(), 4);
        assert_eq!(buf[0]["status"], "running");
        assert_eq!(buf[0]["call_index"], 0);
        assert_eq!(buf[1]["status"], "done");
        assert_eq!(buf[2]["status"], "running");
        assert_eq!(buf[3]["status"], "done");
        // The fed-back content is the full (bounded) echo.
        assert!(out[0].result_content.contains("echo:"));
    }

    #[tokio::test]
    async fn execute_calls_error_card_and_feedback() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "ec2").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-ec2").await;
        let (_turns, t) = claimed(&session.id, 0).await;
        let arow = svc
            .append_message(
                &uid,
                _AppendMessage {
                    id: t.message_id.clone(),
                    session_id: session.id.clone(),
                    role: "assistant".to_string(),
                    content: String::new(),
                    client_msg_id: None,
                    status: "streaming".to_string(),
                },
            )
            .await
            .unwrap();
        let mut accum = TurnAccum::new();
        let calls = vec![("x".to_string(), "boom".to_string(), "{}".to_string())];
        let ctx = ExecCtx {
            svc: &svc,
            session: &session,
            turn: &t,
            executor: &FailTools("no_tools"),
            assistant_seq: arow.seq,
            deadline: tokio::time::Instant::now() + Duration::from_secs(60),
        };
        let out = execute_calls(&ctx, calls, &mut accum).await;
        let cards = svc.list_tool_calls(&uid, &session.id).await.unwrap();
        assert_eq!(cards[0].status, "error");
        assert_eq!(cards[0].output_summary, "failure no_tools");
        let buf = buffered(&t).await;
        assert_eq!(buf[1]["status"], "error");
        // The error text feeds back (bounded to OUTPUT_SUMMARY_MAX).
        assert_eq!(out[0].result_content, "failure no_tools");
    }

    // ---------- run_turn end-to-end (scripted provider) ----------

    async fn start_turn(
        state: &AppState,
        session: &OxpilotSessionRow,
        generation: u32,
    ) -> (Arc<ActiveTurn>, Arc<dyn SessionService>) {
        let svc = state.oxpilot.sessions().unwrap();
        let t = state
            .oxpilot
            .turns()
            .await
            .claim(&session.id, || turn(&session.id, generation))
            .await
            .expect("claim");
        (t, svc)
    }

    #[tokio::test]
    async fn run_turn_done_persists_terminal_and_usage() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "t1").await;
        let session = session_for(&svc(&db), &uid, &repo, "s-t1").await;
        let provider = Arc::new(ScriptProvider::new(vec![Ok(vec![
            ChatStreamItem::Delta("hi".into()),
            ChatStreamItem::Done {
                usage: Some(_Usage {
                    input_tokens: 10,
                    output_tokens: 2,
                }),
            },
        ])]));
        let state = state_with(&db, Some(provider.clone()), Some("m-1"), None).await;
        let (t, svc) = start_turn(&state, &session, 0).await;
        let outcome = run_turn(&state, &session, &t).await.unwrap();
        assert!(outcome.drained.is_empty());
        let buf = buffered(&t).await;
        let last = buf.last().unwrap();
        assert_eq!(last["type"], "terminal");
        assert_eq!(last["result"], "done");
        // Row completed + usage recorded (any_items ⇒ requests:1, tokens:12).
        let msgs = svc.list_messages(&uid, &session.id).await.unwrap();
        let arow = msgs.iter().find(|m| m.id == t.message_id).unwrap();
        assert_eq!(arow.status, "complete");
        assert_eq!(arow.content, "hi");
        let view = svc.usage_view(&uid, BudgetCaps::default()).await.unwrap();
        assert_eq!(view.requests_used, 1);
        assert_eq!(view.tokens_used, 12);
    }

    #[tokio::test]
    async fn run_turn_tool_round_feeds_results_back() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "t2").await;
        let session = session_for(&svc(&db), &uid, &repo, "s-t2").await;
        let provider = Arc::new(ScriptProvider::new(vec![
            Ok(vec![
                ChatStreamItem::Delta("let me ".into()),
                ChatStreamItem::ToolCall {
                    id: "tc_1".into(),
                    name: "echo".into(),
                    args_json: "{\"a\":1}".into(),
                },
                ChatStreamItem::Done { usage: None },
            ]),
            Ok(vec![
                ChatStreamItem::Delta("final".into()),
                ChatStreamItem::Done { usage: None },
            ]),
        ]));
        let state = state_with(
            &db,
            Some(provider.clone()),
            Some("m-1"),
            Some(Arc::new(EchoTools)),
        )
        .await;
        let (t, svc) = start_turn(&state, &session, 0).await;
        let outcome = run_turn(&state, &session, &t).await.unwrap();
        assert!(outcome.drained.is_empty());
        let buf = buffered(&t).await;
        assert_eq!(buf.last().unwrap()["result"], "done");
        // The second request carries: transcript msgs + assistant round
        // msg (tool_calls_json echo) + the role:tool result msg.
        let reqs = provider.requests();
        assert_eq!(reqs.len(), 2);
        let m2 = &reqs[1].messages;
        let asst = m2
            .iter()
            .find(|m| m.role == "assistant" && m.tool_calls_json.is_some());
        let asst = asst.expect("assistant round msg with calls");
        assert!(asst.tool_calls_json.as_deref().unwrap().contains("tc_1"));
        assert_eq!(asst.content, "let me ");
        let tool_msg = m2.iter().find(|m| m.role == "tool").expect("tool msg");
        assert_eq!(tool_msg.tool_call_id.as_deref(), Some("tc_1"));
        assert_eq!(tool_msg.tool_name.as_deref(), Some("echo"));
        assert!(tool_msg.content.starts_with("echo:"));
        // Catalog rode the request.
        assert_eq!(reqs[0].tools.len(), 1);
        assert_eq!(reqs[0].tools[0].name, "echo");
        // Card persisted done.
        let cards = svc.list_tool_calls(&uid, &session.id).await.unwrap();
        assert_eq!(cards[0].status, "done");
    }

    #[tokio::test]
    async fn run_turn_no_model_terminal() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "t3").await;
        let session = session_for(&svc(&db), &uid, &repo, "s-t3").await;
        let provider = Arc::new(ScriptProvider::new(vec![]));
        let state = state_with(&db, Some(provider.clone()), None, None).await;
        let (t, svc) = start_turn(&state, &session, 0).await;
        run_turn(&state, &session, &t).await.unwrap();
        let buf = buffered(&t).await;
        assert_eq!(buf.last().unwrap()["code"], "no_model");
        // Provider never contacted.
        assert!(provider.requests().is_empty());
        // The refusal mints a durable error row — a drained follow-up
        // batch keeps a retryable outcome instead of a silent skip.
        let msgs = svc.list_messages(&uid, &session.id).await.unwrap();
        let arow = msgs.iter().find(|m| m.id == t.message_id).unwrap();
        assert_eq!(arow.status, "error");
    }

    #[tokio::test]
    async fn run_turn_missing_provider_terminal_disabled() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "t4").await;
        let session = session_for(&svc(&db), &uid, &repo, "s-t4").await;
        let state = state_with(&db, None, Some("m-1"), None).await;
        let (t, svc) = start_turn(&state, &session, 0).await;
        run_turn(&state, &session, &t).await.unwrap();
        let buf = buffered(&t).await;
        assert_eq!(buf.last().unwrap()["code"], "oxpilot.disabled");
        // Same durable-refusal row as the no_model arm.
        let msgs = svc.list_messages(&uid, &session.id).await.unwrap();
        let arow = msgs.iter().find(|m| m.id == t.message_id).unwrap();
        assert_eq!(arow.status, "error");
    }

    #[tokio::test]
    async fn run_turn_open_failure_uses_provider_code() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "t5").await;
        let session = session_for(&svc(&db), &uid, &repo, "s-t5").await;
        let provider = Arc::new(ScriptProvider::new(vec![Err(ProviderError {
            code: "rate_limited".to_string(),
            message: "slow down".to_string(),
        })]));
        let state = state_with(&db, Some(provider), Some("m-1"), None).await;
        let (t, svc) = start_turn(&state, &session, 0).await;
        run_turn(&state, &session, &t).await.unwrap();
        let buf = buffered(&t).await;
        assert_eq!(buf.last().unwrap()["result"], "error");
        assert_eq!(buf.last().unwrap()["code"], "rate_limited");
        let msgs = svc.list_messages(&uid, &session.id).await.unwrap();
        let arow = msgs.iter().find(|m| m.id == t.message_id).unwrap();
        assert_eq!(arow.status, "error");
        // The open failure streamed nothing — no usage counted.
        let view = svc.usage_view(&uid, BudgetCaps::default()).await.unwrap();
        assert_eq!(view.requests_used, 0);
    }

    #[tokio::test]
    async fn run_turn_tool_rounds_exceeded() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "t6").await;
        let session = session_for(&svc(&db), &uid, &repo, "s-t6").await;
        // Nine call-bearing rounds — rounds 0..8 execute, the ninth's
        // pending calls get error cards and the turn terminal-errors.
        let scripts = (0..=MAX_TOOL_ROUNDS)
            .map(|i| {
                Ok(vec![
                    ChatStreamItem::ToolCall {
                        id: format!("tc{i}"),
                        name: "echo".into(),
                        args_json: "{}".into(),
                    },
                    ChatStreamItem::Done { usage: None },
                ])
            })
            .collect::<Vec<_>>();
        let provider = Arc::new(ScriptProvider::new(scripts));
        let state = state_with(
            &db,
            Some(provider.clone()),
            Some("m-1"),
            Some(Arc::new(EchoTools)),
        )
        .await;
        let (t, svc) = start_turn(&state, &session, 0).await;
        run_turn(&state, &session, &t).await.unwrap();
        let buf = buffered(&t).await;
        let last = buf.last().unwrap();
        assert_eq!(last["result"], "error");
        assert_eq!(last["code"], "tool_rounds_exceeded");
        let cards = svc.list_tool_calls(&uid, &session.id).await.unwrap();
        let done = cards.iter().filter(|c| c.status == "done").count();
        let err = cards.iter().filter(|c| c.status == "error").count();
        assert_eq!(done, MAX_TOOL_ROUNDS);
        assert_eq!(err, 1);
        assert_eq!(provider.requests().len(), MAX_TOOL_ROUNDS + 1);
    }

    #[tokio::test]
    async fn run_turn_null_tools_feeds_no_tools_back() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "t7").await;
        let session = session_for(&svc(&db), &uid, &repo, "s-t7").await;
        let provider = Arc::new(ScriptProvider::new(vec![
            Ok(vec![
                ChatStreamItem::ToolCall {
                    id: "tc".into(),
                    name: "anything".into(),
                    args_json: "{}".into(),
                },
                ChatStreamItem::Done { usage: None },
            ]),
            Ok(vec![
                ChatStreamItem::Delta("ok".into()),
                ChatStreamItem::Done { usage: None },
            ]),
        ]));
        // tools: None ⇒ turn_parts maps to NullTools (empty catalog).
        let state = state_with(&db, Some(provider.clone()), Some("m-1"), None).await;
        let (t, svc) = start_turn(&state, &session, 0).await;
        run_turn(&state, &session, &t).await.unwrap();
        let reqs = provider.requests();
        assert!(reqs[0].tools.is_empty());
        let tool_msg = reqs[1]
            .messages
            .iter()
            .find(|m| m.role == "tool")
            .expect("result msg");
        assert!(tool_msg.content.contains("no tool executor installed"));
        let cards = svc.list_tool_calls(&uid, &session.id).await.unwrap();
        assert_eq!(cards[0].status, "error");
    }

    // ---------- drain_and_chain (BR4.1/4.3) ----------

    async fn queued(svc: &Arc<dyn SessionService>, uid: &str, session_id: &str, content: &str) {
        svc.enqueue(
            uid,
            oxidean_api_core::oxpilot::Enqueue {
                session_id: session_id.to_string(),
                content: content.to_string(),
                client_msg_id: None,
            },
        )
        .await
        .expect("enqueue");
    }

    #[tokio::test]
    async fn drain_and_chain_empty_releases() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "dc1").await;
        let session = session_for(&svc(&db), &uid, &repo, "s-dc1").await;
        let provider = Arc::new(ScriptProvider::new(vec![]));
        let state = state_with(&db, Some(provider), Some("m-1"), None).await;
        let (t, _svc) = start_turn(&state, &session, 0).await;
        drain_and_chain(&state, &session, &t, None).await;
        assert!(state.oxpilot.turns().await.get(&session.id).await.is_none());
    }

    #[tokio::test]
    async fn drain_and_chain_claims_successor_with_rows() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "dc2").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-dc2").await;
        queued(&svc, &uid, &session.id, "follow up one").await;
        queued(&svc, &uid, &session.id, "follow up two").await;
        let provider = Arc::new(ScriptProvider::new(vec![Ok(vec![
            ChatStreamItem::Delta("next".into()),
            ChatStreamItem::Done { usage: None },
        ])]));
        let state = state_with(&db, Some(provider), Some("m-1"), None).await;
        let (t, _s) = start_turn(&state, &session, 0).await;
        drain_and_chain(&state, &session, &t, None).await;
        // Successor claimed — generation+1, pending fields reset.
        let next = state.oxpilot.turns().await.get(&session.id).await;
        let next = next.expect("successor claimed");
        assert_eq!(next.generation, 1);
        assert!(next.pending_client_msg_id.is_none());
        assert!(next.pending_content.is_empty());
        // Drained rows became durable transcript rows (queue emptied).
        assert!(svc.list_queue(&uid, &session.id).await.unwrap().is_empty());
        let msgs = svc.list_messages(&uid, &session.id).await.unwrap();
        let drained: Vec<_> = msgs.iter().filter(|m| m.role == "user").collect();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].content, "follow up one");
        assert_eq!(drained[1].content, "follow up two");
        // started_seq seeds from the LAST drained row — the successor's
        // assistant row mints seq+1.
        assert_eq!(next.started_seq.load(Ordering::SeqCst), drained[1].seq);
        // The detached successor completes on the scripted provider —
        // the map empties when its own wrapper releases.
        let sid = session.id.clone();
        let st = state.clone();
        let settled = eventually_async(move || {
            let st = st.clone();
            let sid = sid.clone();
            async move { st.oxpilot.turns().await.get(&sid).await.is_none() }
        })
        .await;
        assert!(settled, "successor turn ran to completion");
        let msgs = svc.list_messages(&uid, &session.id).await.unwrap();
        assert_eq!(msgs.iter().filter(|m| m.role == "assistant").count(), 1);
    }

    #[tokio::test]
    async fn drain_and_chain_depth_cap_stops() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "dc3").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-dc3").await;
        queued(&svc, &uid, &session.id, "left behind").await;
        let provider = Arc::new(ScriptProvider::new(vec![]));
        let state = state_with(&db, Some(provider), Some("m-1"), None).await;
        let (t, _s) = start_turn(&state, &session, MAX_CHAIN_DEPTH).await;
        drain_and_chain(&state, &session, &t, None).await;
        // Released, drained rows delivered as transcript, NO successor.
        assert!(state.oxpilot.turns().await.get(&session.id).await.is_none());
        let msgs = svc.list_messages(&uid, &session.id).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "left behind");
        assert!(svc.list_queue(&uid, &session.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn drain_and_chain_stale_entry_exits_quietly() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "dc4").await;
        let session = session_for(&svc(&db), &uid, &repo, "s-dc4").await;
        let provider = Arc::new(ScriptProvider::new(vec![]));
        let state = state_with(&db, Some(provider), Some("m-1"), None).await;
        let (t, _s) = start_turn(&state, &session, 0).await;
        // Release first — the wrapper's entry is already gone.
        state
            .oxpilot
            .turns()
            .await
            .release(&session.id, &t.turn_id)
            .await;
        drain_and_chain(&state, &session, &t, None).await;
        assert!(state.oxpilot.turns().await.get(&session.id).await.is_none());
    }

    #[tokio::test]
    async fn drain_and_chain_drain_error_exits_quietly() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "dc6").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-dc6").await;
        let provider = Arc::new(ScriptProvider::new(vec![]));
        let state = state_with(&db, Some(provider), Some("m-1"), None).await;
        let (t, _s) = start_turn(&state, &session, 0).await;
        // Delete the session — the post-release drain_queue hits
        // `owned()` ⇒ not_found ⇒ warn + exit, no successor.
        svc.delete_session(&uid, &session.id).await.expect("delete");
        drain_and_chain(&state, &session, &t, None).await;
        assert!(state.oxpilot.turns().await.get(&session.id).await.is_none());
    }

    #[tokio::test]
    async fn drain_and_chain_recheck_error_with_batch_never_chains() {
        // BR4.1: the locked recheck's `Err` exits WITHOUT consulting the
        // in-turn batch — a non-empty `outcome.drained` must not chain a
        // successor on a session that just went unreachable.
        let db = test_db().await;
        let (uid, repo) = seed(&db, "dc7").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-dc7").await;
        // Real drained rows for the in-turn batch — a queued row drained
        // by THIS winner before the session disappears.
        queued(&svc, &uid, &session.id, "already delivered").await;
        let drained = svc
            .drain_queue(&uid, &session.id)
            .await
            .expect("in-turn drain");
        assert!(!drained.is_empty());
        let provider = Arc::new(ScriptProvider::new(vec![Ok(vec![
            ChatStreamItem::Delta("ghost".into()),
            ChatStreamItem::Done { usage: None },
        ])]));
        let state = state_with(&db, Some(provider), Some("m-1"), None).await;
        let (t, _s) = start_turn(&state, &session, 0).await;
        svc.delete_session(&uid, &session.id).await.expect("delete");
        drain_and_chain(&state, &session, &t, Some(TurnOutcome { drained })).await;
        // No successor was claimed despite the non-empty batch.
        assert!(state.oxpilot.turns().await.get(&session.id).await.is_none());
    }

    #[tokio::test]
    async fn drain_and_chain_claim_loss_keeps_winner() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "dc5").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-dc5").await;
        queued(&svc, &uid, &session.id, "straggler").await;
        let provider = Arc::new(ScriptProvider::new(vec![]));
        let state = state_with(&db, Some(provider), Some("m-1"), None).await;
        // turn A releases, turn B already claims — A's drain_and_chain
        // must not disturb B's entry.
        let turns = state.oxpilot.turns().await;
        let a = turns
            .claim(&session.id, || turn(&session.id, 0))
            .await
            .unwrap();
        turns.release(&session.id, &a.turn_id).await;
        let b = turns
            .claim(&session.id, || turn(&session.id, 0))
            .await
            .unwrap();
        drain_and_chain(&state, &session, &a, None).await;
        let live = turns.get(&session.id).await.expect("B survives");
        assert_eq!(live.turn_id, b.turn_id);
        // The queue still drained to transcript under the lock.
        let msgs = svc.list_messages(&uid, &session.id).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "straggler");
    }

    // ---------- wrapper ----------

    #[tokio::test]
    async fn spawn_turn_task_panic_emits_internal_terminal_and_releases() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "w1").await;
        let session = session_for(&svc(&db), &uid, &repo, "s-w1").await;
        let user_row = appended(&svc(&db), &uid, &session.id, "user", "hi").await;
        let state = state_with(&db, Some(Arc::new(PanicProvider)), Some("m-1"), None).await;
        let (t, svc) = start_turn(&state, &session, 0).await;
        t.started_seq.store(user_row.seq, Ordering::SeqCst);
        spawn_turn_task(state.clone(), session.clone(), t.clone());
        // Poll until the wrapper lands the internal terminal + releases.
        let sid = session.id.clone();
        let state2 = state.clone();
        let ok = eventually_async(move || {
            let st = state2.clone();
            let sid = sid.clone();
            async move { st.oxpilot.turns().await.get(&sid).await.is_none() }
        })
        .await;
        assert!(ok, "turn released after panic");
        let buf = buffered(&t).await;
        assert_eq!(buf.last().unwrap()["result"], "error");
        assert_eq!(buf.last().unwrap()["code"], "internal");
        let msgs = svc.list_messages(&uid, &session.id).await.unwrap();
        let arow = msgs.iter().find(|m| m.id == t.message_id);
        assert_eq!(arow.map(|m| m.status.as_str()), Some("error"));
    }

    #[tokio::test]
    async fn spawn_turn_task_done_releases_and_chains() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "w2").await;
        let svc = svc(&db);
        let session = session_for(&svc, &uid, &repo, "s-w2").await;
        let user_row = appended(&svc, &uid, &session.id, "user", "hi").await;
        queued(&svc, &uid, &session.id, "queued during").await;
        let provider = Arc::new(ScriptProvider::new(vec![
            Ok(vec![
                ChatStreamItem::Delta("one".into()),
                ChatStreamItem::Done { usage: None },
            ]),
            Ok(vec![
                ChatStreamItem::Delta("two".into()),
                ChatStreamItem::Done { usage: None },
            ]),
        ]));
        let state = state_with(&db, Some(provider), Some("m-1"), None).await;
        let (t, _s) = start_turn(&state, &session, 0).await;
        t.started_seq.store(user_row.seq, Ordering::SeqCst);
        spawn_turn_task(state.clone(), session.clone(), t.clone());
        // Wait for the full chain to settle — map empties when the
        // successor (generation 1) also completes.
        let sid = session.id.clone();
        let st = state.clone();
        let settled = eventually_async(move || {
            let st = st.clone();
            let sid = sid.clone();
            async move { st.oxpilot.turns().await.get(&sid).await.is_none() }
        })
        .await;
        assert!(settled, "chain ran to completion");
        // Both assistant rows persisted.
        let msgs = svc.list_messages(&uid, &session.id).await.unwrap();
        let assistants = msgs.iter().filter(|m| m.role == "assistant").count();
        assert_eq!(assistants, 2);
        // The queued follow-up became a transcript row before the
        // successor's assistant row.
        let users: Vec<_> = msgs.iter().filter(|m| m.role == "user").collect();
        assert_eq!(users.len(), 2);
        assert_eq!(users[1].content, "queued during");
    }

    // ---------- C8 abort hook ----------

    #[tokio::test]
    async fn abort_hook_aborts_live_turn_idempotently() {
        let db = test_db().await;
        let (uid, repo) = seed(&db, "h1").await;
        let session = session_for(&svc(&db), &uid, &repo, "s-h1").await;
        let state = state_with(&db, None, Some("m-1"), None).await;
        let (t, _s) = start_turn(&state, &session, 0).await;
        let hook = OxpilotTurnAbortHook::new(state.oxpilot.clone());
        hook.on_session_delete(&session.id, &uid).await.unwrap();
        assert!(t.cancel.is_cancelled());
        let buf = buffered(&t).await;
        assert_eq!(buf.last().unwrap()["result"], "aborted");
        // Second call — CAS loses, still Ok.
        hook.on_session_delete(&session.id, &uid).await.unwrap();
        // No live turn at all — quiet Ok.
        hook.on_session_delete("nope", &uid).await.unwrap();
    }
}
