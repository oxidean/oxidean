# Security Design — agent-runtime (U3)

Consumes: `nfr-requirements/security-requirements.md` (NFR1.1–1.11,
NFR2.x), `functional-design/{rules,entities}.md`,
`nfr-design-questions.md` (Q4, Q5).

## Defense-in-depth layout

| boundary | control | covers |
|----------|---------|--------|
| proc | `turn_cancel` SessionOnly-by-omission + `require_enabled` top of handler; PAT `auth.pat_scope`, anon `auth.unauthenticated`, flag-off `oxpilot.disabled`; foreign session ⇒ `not_found` — same arm vocabulary as U2's proc set (ERRATUM to fd proc-table deviation (a): the upstream SessionOnly gate emits `auth.pat_scope` before dispatch — the proc body never sees the credential class, so `not_found` for PATs is unimplementable; only `no_active_turn`/`{aborted:false}` are this proc's to pin) | NFR1.1 |
| session ownership | `owned_session`/proc-layer `svc.get_session(&user.id, &req.session_id)` before any turn state touched — the proc never reads `ActiveTurns` for foreign sessions | NFR1.1, NFR1.2 |
| turn context | `ToolCtx.user_id`/`session_id`/`repo_id` minted from the CLAIM's captured owner context — chained turns carry it forward (no request in flight on chain); provider args can't rebind them | NFR1.2, NFR1.4 |
| tool authz | every provider-emitted call routes through `executor.invoke(ToolCall{call_id, name, args_json}, ctx)` — the runtime never executes, never pre-filters; `NullTools` answers `no_tools` until U5 lands | NFR1.3, C6 |
| provider creds | runtime holds `Arc<dyn LlmProvider>` from `turn_parts()` only — no key accessor exists on the trait; decrypt lives in U4 | NFR1.5 |
| content | `redact()` on tool-result text before it becomes `ChatMsg{role:"tool"}` (BR7.2's on-wire pass) — U2's boundary never saw tool output | NFR1.6 |
| frames | SSE payloads carry ids/deltas/cards/statuses only; card summaries capped `ARGS_SUMMARY_MAX`/`OUTPUT_SUMMARY_MAX` = 2 KiB — no raw `args_json` or env in frames | NFR1.7, NFR3.5 |
| prompt | `assemble_prompt` reads `transcript(owner_id, session_id)` (C5 arg order) — service refuses foreign reads; window is already-redacted rows | NFR1.8 |

## `turn_parts()` pin (Q4)

```rust
// api-core oxpilot.rs — widened provider_for_turn:
pub struct TurnParts {
    pub provider: Option<Arc<dyn LlmProvider>>,
    pub model: Option<String>,
    pub executor: Arc<dyn ToolExecutor>,
}
// one slot-lock read; a mid-turn settings write can't swap the
// provider OR the executor — the triple is captured atomically.
```

`model: None` ⇒ `emit_terminal {result:"error", code:"no_model"}`
BEFORE any stream opens — keyless Ollama with no configured model is a
typed refusal, not a fabricated request (NFR4.9).

## Admission lock (Q1, NFR1.11)

`admission_lock(session_id) -> Option<(Arc<ActiveTurn>, Arc<Mutex<()>>)>` —
the PAIR returns so a stale guard can re-verify under the lock:

- proc side (`message_queue`): `check_budget` runs BEFORE the hold
  (shipped order: live-check → budget → enqueue); inside the hold the
  proc re-verifies `turn.turn_id == entry.turn_id` AND liveness — a
  guard cloned from a since-released entry fails the id check and
  falls to `session_active_turn_absent` (the thin residual of R-20's
  TOCTOU — a guard held across a full generation — closes).
  `admission_lock` returning `None` ⇒ `session_active_turn_absent`.
- task side (`drain_and_chain`): `let g = lock().await;
  release(session_id, turn_id); recheck drain; chain-or-exit` —
  release+recheck is one hold.

The lock NEVER covers provider streaming, tool execution, or prompt
assembly — only the two short DB sections whose interleave was the
TOCTOU (review R-20). `tokio::sync::Mutex` (async hold), guard RAII
(panic-safe).

## Secret-path enumeration

| path | barrier |
|------|---------|
| provider key | `Arc<dyn LlmProvider>` — opaque; decrypt in U4 build only |
| tool args → card | `args_summary` truncated ≤2 KiB post-redact |
| tool result → provider | `redact()` then `≤ TOOL_OUTPUT_MAX (32 KiB)` truncated-with-marker into `ChatMsg{role:"tool"}.content` — the model sees the real (capped) output per BR3.2 |
| tool result → card/frame | `output_summary`/`tool` frame arg `≤ OUTPUT_SUMMARY_MAX (2 KiB)` — the durable+wire summary bound is the tighter one |
| prompt | transcript rows already redacted at U2 write boundary |
| frames | ids/deltas/cards only — never prompt, args_json, env |
| pending_content | redacted copy held in-memory for resend match; transient, never logged (NFR2.2) |
