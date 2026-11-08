# Code Summary — agent-runtime (U3)

The turn loop landed: provider streaming with multi-round tool
execution, one-live-turn admission with an atomic claim, exactly-once
terminal emission, durable-first persistence, queue drain-and-chain,
cancellation via a first-wins CAS, budget refusal at admission, and the
C8 delete-hook that aborts a live turn before session cascade. U1/U2's
HTTP surface and contract vocabulary are preserved end-to-end.

## Runtime seam (api-core)

- `crates/oxidean-api-core/src/oxpilot.rs`:
  - C6 surface (l.57–126): `ToolSpec`, `ToolCall`, `ToolCtx`
    (`session_id`/`user_id`/`repo_id`/`proposal_id` — executors never
    trust caller-supplied scope, ADR-008), `ToolOutput`, `ToolError`,
    `trait ToolExecutor` (sync `catalog()` + async `invoke()`),
    `NullTools` (empty catalog; invoke ⇒ `no_tools`). Provided by U3,
    consumed by U5.
  - `ChatMsg` (l.35) gained `tool_call_id`/`tool_name`/`tool_calls_json`
    — the C4 amendment carrying tool results and assistant tool-call
    blocks through the provider-agnostic transcript shape.
  - `ActiveTurn` (l.534) gained `admission_lock` (per-session,
    Arc-shared into `ActiveTurns`), `terminal_sent` (first-wins CAS for
    terminal emission), `generation` (chain depth counter).
  - `ActiveTurns::claim` (l.586) is atomic insert-if-absent returning
    `Option<Arc<ActiveTurn>>`; `admission_lock(session_id)` (l.610)
    returns the `(turn, lock)` pair for stale-guard re-verify (R-13c).
  - `OxpilotRuntimeSlot` gained `tools: Option<Arc<dyn ToolExecutor>>`;
    `turn_parts()` (l.855 `TurnParts`) captures provider + model +
    executor under ONE slot-lock read; `None` tools ⇒ `NullTools`.
    `disabled_slot`/`new`/`from_slot`/rebuild constructors widened;
    `settings.rs` carries the executor across slot transitions.

## Turn engine (`src/runtime.rs`, new)

- `emit_frame` (l.109) appends to `emitted_deltas` then fans out;
  `emit_terminal` (l.117) takes the same buffer lock, CASes
  `terminal_sent` false→true inside it, pushes the terminal frame —
  exactly-once against three writers (turn exit, cancel proc/hook,
  wrapper panic arm).
- `assemble_prompt` (l.198) builds the provider request from persisted
  transcript rows: system prompt with repo binding (FR2.3), tool-result
  `ChatMsg`s and assistant `tool_calls_json` reconstructed from durable
  card rows, newest-first walk under `PROMPT_TRANSCRIPT_MAX_BYTES`
  (128 KiB) then reversed — ≤200 B/call summary lines bound card cost.
- `run_round` (l.307) iterates the provider stream: `Delta` → frame +
  accumulate; `ToolCall` → effective call_id mint (`effective_call_id`
  l.293) → card persisted `running` durable-first → frame →
  `executor.invoke` under `TOOL_CALL_TIMEOUT` (60 s) racing
  `cancel.cancelled()` → card `done`/`error` → redacted tool `ChatMsg`
  (≤ `TOOL_OUTPUT_MAX` 32 KiB) fed back next round; `Done` → terminal
  done; `Error(c)` → terminal `{error,c}`. Both `run_round` and
  `invoke_call` pre-check `cancel.is_cancelled()` so an already-aborted
  turn never emits or invokes past the cancel (found by the abort test
  — a Ready item could otherwise beat the select's poll order).
- `run_turn` (l.752) owns the loop: `turn_parts()` pins provider/model/
  tools for the whole turn (ADR-010); `model:None` ⇒ `no_model`,
  provider-absent ⇒ `oxpilot.disabled`, `MAX_TOOL_ROUNDS` (8) ⇒
  `tool_rounds_exceeded`, wall `TURN_MAX_DURATION` (10 min) ⇒
  `turn_timeout`; pending calls at any exit become durable error cards
  + error frames (`fail_pending_calls`). `finish_turn` (l.663) persists
  the assistant row (error rows on failure paths), records dual-axis
  usage at terminal (FR6.7), then emits terminal.
- `spawn_turn_task` (l.938) is the release-owning wrapper: outer task
  awaits the inner `JoinHandle`, `JoinError` ⇒ warn +
  `emit_terminal{internal}` (CAS no-ops if already sent), then
  `drain_and_chain` (l.1003) under `admission_lock` — `release` →
  `svc.drain_queue` → non-empty ∧ `generation < 16` ⇒ `claim(gen+1)` +
  respawn; claim-loss ⇒ quiet exit. `run_turn` never releases — a
  disconnect or panic cannot orphan the map (BR5.3).
- `OxpilotTurnAbortHook` (l.1090): `on_session_delete` emits
  terminal`{aborted}` + cancels the token; idempotent, never waits on
  the task. Registered on the session service at shell construction in
  `main.rs` (l.125) so the C8 hook chain (hook → proposal status →
  cascade) runs in order.

## HTTP/RPC surface

- `chat_post` (routes.rs) runs BR1.1's pinned order: session auth →
  `require_enabled` → owned session → body validate → union dedupe →
  `check_budget` (429 `budget_exceeded` via new `svc_err` arm) → atomic
  `claim` → post-claim dedupe recheck (release-on-hit) → residual
  queue drain (BR4.2) → user-row insert → detached `spawn_turn_task`.
  Every post-claim exit releases.
- `message_queue` (session.rs l.254): `check_budget` pre-hold; under
  the per-session `admission_lock` the proc re-verifies
  `(turn_id, liveness)` from the pair then commits `enqueue`;
  absent/stale ⇒ `session_active_turn_absent`; queued-card emission
  unchanged behind the queued_id guard.
- `oxpilot.turn_cancel` (session.rs l.366, dispatch.rs l.35, lib.rs
  PROCEDURES l.38): SessionOnly + `require_enabled` + ownership-first
  (anti-enumeration — foreign/absent ids resolve `not_found` before any
  turn lookup); live ⇒ `emit_terminal{aborted}` + `cancel()` ⇒
  `{aborted:true}`; CAS-loss ⇒ `{aborted:false}`; no live turn ⇒
  `no_active_turn`.
- `chat_stream` attach order verified unchanged: `fanout.subscribe()`
  before `emitted_deltas` clone on both the GET attach-poll path and
  the POST-side `attach_stream` (R-06); snapshot via `svc.list_messages`;
  advertised-turn re-snapshot covers terminal-between-snapshot-and-attach.

## Provider wire mapping (C4 amendment)

- `openai.rs`: tool results send `{role:"tool", tool_call_id, name}`;
  assistant `tool_calls_json` → OpenAI `tool_calls` (function name +
  raw `arguments`).
- `anthropic.rs`: tool results become `user` messages with
  `tool_result` blocks (consecutive results merge into one user turn);
  assistant calls become `tool_use` blocks.
- `ollama.rs`: tool results carry tool `name`; assistant
  `tool_calls_json` → Ollama `tool_calls` with parsed argument objects.
- All three still raw `reqwest` — no provider SDK dependency, no
  api-core → api-oxpilot edge (the C6 trait lives in api-core and U3's
  engine consumes it through `Arc<dyn ToolExecutor>`).
- The protocol end-marker is the ONLY success signal in all three
  decoders (`[DONE]` / `message_stop` or terminal `error` / `done:true`);
  a bare EOF closes the stream on `ChatStreamItem::Error{provider_error}`
  rather than fabricating `Done` — a truncated wire is never durably
  labelled complete (review R-04).

## Generated surface

- `rpc_gen.rs` gained `OxpilotTurnCancelResponse` + the
  `client.oxpilot.turnCancel` helper; `make rpc-gen` output committed —
  `packages/api-client/src/index.ts` regenerated (byte-identical on
  rerun; the `oxpilot.*` client block is new in this unit line).

## Tests

- `cargo nextest run -p oxidean-api-oxpilot` — 113 green, incl. the new
  `runtime::tests` module (~30 tests): `emit_terminal` first-wins +
  concurrent exactly-once + post-terminal `emit_frame` drop,
  `effective_call_id` mint, `truncate_marker` bounds, `assemble_prompt`
  windowing + tool summary lines, `run_round`
  delta/call/done + silent-end + deadline + cancel, `invoke_call`
  ok/timeout/abort, `execute_calls` persist+feedback and error-card
  paths, `run_turn` done/no_model/missing_provider/open_failure/
  rounds_exceeded/null_tools, `drain_and_chain`
  empty/nonempty/depth-cap/stale/claim-loss/recheck-Err-with-batch,
  `spawn_turn_task` panic/done, `abort_hook` idempotence — plus 3
  truncated-wire wiremock tests (one per adapter: bare EOF ⇒
  `provider_error`, never `Done`).
- `cargo nextest run -p oxidean-api --test oxpilot` — 24 green, incl.
  `turn_cancel_aborts_live_turn`, `turn_cancel_absent_and_foreign`,
  `session_delete_aborts_live_turn_via_hook`,
  `chat_post_budget_exceeded_is_429`,
  `get_stream_late_attach_replays_buffer_to_terminal`, and the
  PROCEDURES-set pin.
- `dialect_oxpilot` — 16/16 (fresh scratch DB, `-j 1` per the pin).

## Review-driven corrections (iteration 1)

- `emit_frame` now checks `terminal_sent` inside the `emitted_deltas`
  lock — post-terminal frames (late error cards, queued pushes) drop
  instead of landing in the replay buffer/on live readers (C2) [R-01].
- `chat_post` residual `drain_queue` `Err` ⇒ release + 500
  `oxpilot.internal` per BR4.2's pinned arm — never mints the user row's
  `seq` ahead of undrained orphans [R-02].
- `drain_and_chain`'s locked recheck `Err` (or missing svc) warns and
  exits WITHOUT consulting the in-turn batch — no successor chained on
  stale rows per BR4.1 [R-03].
- Bare-EOF ⇒ `provider_error` in all three adapters (see Provider wire
  mapping) [R-04].
- The `provider.stream(req)` open await now races `cancel` + the wall
  deadline with a deterministic pre-check — a stalled open can't hold
  the claim and a dead turn never burns a fresh provider request (WF2)
  [R-05].

## Deviations from plan

- `claim` returns `Option<Arc<ActiveTurn>>` rather than `Result<_, ()>`
  — clippy `result_unit_err` under `-D warnings`; `None` IS the
  occupied signal, identical semantics to the pinned insert-if-absent.
- Two pre-existing `field_reassign_with_default` warnings in
  `dialect_oxpilot.rs` test fixtures were fixed (struct-init syntax) —
  required for the workspace `-D warnings` gate, no behavior change.
- `routes.rs` carries a scoped `#![allow(clippy::result_large_err)]` —
  the axum early-return idiom (`Err = Response`) is deliberate and
  failure-path-only; the comment records why.
