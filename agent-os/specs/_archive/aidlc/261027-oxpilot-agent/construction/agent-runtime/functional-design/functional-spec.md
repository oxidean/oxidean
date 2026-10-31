# Functional Spec — agent-runtime (U3)

## Sources

- `functional-design-questions.md` — 11 conductor-decided answers
  (Q1–Q11) + confirmed consolidated summary.
- `inception/contract-design/contract-summary.md` — C2 (SSE frames +
  error vocabulary), C4 (provider trait; U3 amendment adds
  `tool_call_id`/`tool_calls_json` to `ChatMsg`), C5 (service port),
  C6 (U3-owned `ToolExecutor`/`ToolCtx`/`ToolCall`/`ToolOutput`/
  `ToolError`), C8 (hook #1).
- `inception/requirements-analysis/requirements.md` — FR2.x, FR6.1/6.3/6.7,
  FR7.x; NFR3, NFR4, NFR6.
- `inception/domain-design/decisions.md` — ADR-008, ADR-010;
  `components.md` — AgentRuntime behaviour/responsibilities block.
- `inception/units-generation/unit-of-work.md` — §U3 scope statement
  (turn loop, prompt assembly, provider streaming, drain, abort
  coordination, one-active-turn).
- `construction/session-core/functional-design/functional-spec.md` —
  U2's pins this unit consumes (BR4.1 order, WF5 drain contract,
  BR2.3 transcript shape).

## Scope

U3 turns the shipped skeleton turn plumbing (claim, dedupe, attach,
snapshot, lagged resync, ping — all live and tested) into the real
agent loop. It does NOT change the HTTP/RPC surface except inside
existing pinned envelopes (no new routes, no new procs). Everything
below is behavior inside `chat_post`'s post-gate path, the spawned
turn task, `message_queue`'s tail, and boot-time registration.

## Workflows

### WF1 — Turn admission (`chat_post`, BR1.1 order)

1. `session_user` → `require_gate` → `owned_session` (404 on
   foreign/absent) → body validate (422).
2. Union dedupe (BR2.2): transcript hit + live matching turn ⇒ attach;
   transcript hit + dead turn ⇒ snapshot stream; queued-only hit ⇒
   409 `turn_active`. Resends never pay the budget check.
3. `check_budget(user_id, slot.budget_caps)` ⇒ 429 `budget_exceeded`
   (BR6.2, debug log `{scope}`) — `svc_err` gains a dedicated
   `budget_exceeded` → 429 arm so the refusal can't fall through the
   500 catch-all (R-07).
4. `ActiveTurns.claim` — loser ⇒ resend-match → attach/snapshot else
   409 `turn_active` (shipped behavior kept).
5. **Post-claim dedupe recheck** (shipped, now pinned in order): a hit
   releases the claim and answers attach/snapshot — a recheck exit
   NEVER runs the residual drain (it would deliver orphans for a
   request that only re-reads).
6. **Residual drain** (BR4.2): `svc.drain_queue` — orphaned queue rows
   mint transcript `seq` BEFORE the new user row; `Err` ⇒ release +
   500 `oxpilot.internal` (fail honest, never mint out of order).
7. `append_message` (user row) → `started_seq` store →
   `spawn_turn_task` → `attach_stream`.

### WF2 — The turn loop (`run_turn` interior — replaces shipped stub)

1. Pin `turn_parts()` (provider, model, executor) — one lock read,
   detach-to-complete (BR1.3). `None` provider ⇒ terminal
   `{result:"error", code:"oxpilot.disabled"}`.
2. Insert the assistant row (`streaming`, caller-minted `message_id`)
   — BR2.1.
3. Assemble prompt (BR2.3): system preamble + repo binding
   (`owner/name@branch`) + goal → `svc.transcript` windowed 128 KiB
   newest-first → per-assistant-row tool-call summary lines →
   `executor.catalog()` → `ChatRequest`.
4. Round loop `for round in 0..MAX_TOOL_ROUNDS(8)`:
   a. `provider.stream(req)` — open failure ⇒ terminal error with the
      provider's code (BR5.2), record nothing.
   b. `select!` per item: `turn.cancel.cancelled()` ⇒ abort path;
      `stream.next()`:
      - `Delta` → append to content; `append` frame (redacted delta,
        per-message `seq`++); `set_message_content` persists cumulative.
      - `ToolCall{id,name,args_json}` → mint `call_id := id` or
        `call_{message_id}_{call_index}` when the provider's `id` is
        empty (Ollama may omit it; the durable card keys
        UNIQUE(session_id, call_id) — an empty id collides on call 2)
        → persist card `running` (durable-first, BR2.4); persist
        FAILURE ⇒ emit `tool{status:"error"}` anyway + feed
        `ToolError{card_persist}` back (the turn continues honestly —
        no phantom running card, no lost call result) → `tool` frame
        carries `call_id` + `call_index` (additive C2 fields — live
        readers disambiguate parallel same-name calls) →
        `executor.invoke` under `timeout(60s)` racing `cancel` →
        card `done|error` + `tool` frame → push assistant round msg
        (`tool_calls_json`, `content` := the PER-ROUND delta slice —
        text streamed since this round opened, not the cumulative row
        — earlier rounds' text already sits in the buffer) +
        `ChatMsg{role:"tool", tool_call_id, tool_name, content:
        redacted output ≤ TOOL_OUTPUT_MAX}` per call onto the round
        buffer (BR3.2).
      - `Done{usage}` → accumulate usage; break to step c.
      - `Error{code,msg}` → terminal `{error, code}` path (BR5.2);
        usage counts if any content/calls happened (BR6.1).
   c. Round ended with no pending calls ⇒ proceed to terminal-done
      path. Round ended with calls ⇒ continue loop (round+1).
   d. Breaching `MAX_TOOL_ROUNDS` ⇒ terminal
      `{error, "tool_rounds_exceeded"}`.
5. `select!` also races `TURN_MAX_DURATION` (10 min) ⇒
   `{error, "turn_timeout"}` and `cancel.cancel()`.

### WF3 — Turn end (terminal + drain + chain)

1. Assistant row → `set_message_status` `complete` (done) or `error`
   (error/aborted/timeout) — durable-first (BR2.4).
2. `record_usage` if BR6.1 says counted.
3. `svc.drain_queue` — end-of-turn drain (BR4.1); `Err` ⇒ warn-log
   `{session_id}`, treat as empty batch (a drain failure must not eat
   the terminal — BR4.2's pin). Non-empty batch ⇒ debug log
   `{session_id, drained_count, generation}`.
4. Emit terminal frame via `emit_terminal` (first-wins, BR5.4) into
   buffer + fanout (`done`/`error`+code/`aborted` per BR5.2).
5. `release(session_id, turn_id)` — guaranteed by the wrapper (BR5.3:
   inner `tokio::spawn` + `JoinHandle` await — `JoinError` covers
   panic), not by the turn body.
6. **Post-release recheck** (BR4.1): `release` + one `drain_queue` on
   the released session run under `turns().admission_lock(session_id)`
   — the same per-session lock `message_queue`'s atomic enqueue section
   holds (WF5), so no row can commit inside the blind window. Recheck
   `Err` ⇒ warn-log `{session_id}`, treat as empty batch. The lock is
   per-session so the section costs nothing to unrelated sessions.
7. Drained non-empty ∧ `generation < MAX_CHAIN_DEPTH(16)` ⇒ claim next
   turn → `spawn_turn_task` seeded: `started_seq :=` the LAST drained
   row's `seq` (satisfies the assistant-follows-user assert — the
   assistant mints `seq+1` — and makes a resend of the last row's cmid
   ATTACH via `resend_matches`'s `started_seq == prior.seq` arm while
   earlier-row resends answer snapshot, already delivered);
   `pending_client_msg_id := None` and `pending_content := ""` — the
   field is a non-nullable `String`; the empty literal can never equal
   a non-empty resend body so `resend_matches` can't false-positive
   (drained rows are durable transcript — resends resolve on the
   persisted `prior.seq` arm). NO new user-row insert — the batch is
   the input. Interleaved POST winning the claim gap ⇒ the chain exits
   quietly; the winner's residual drain covers any still-queued rows
   and its prompt window consumes the delivered batch either way —
   coherent `seq` order preserved.

### WF4 — Abort path (C8 hook #1 + in-loop observation)

1. `OxpilotTurnAbortHook::on_session_delete(session_id)` →
   `turns().get` → Some(turn): push `terminal{result:"aborted"}` into
   buffer+fanout (stream readers close on the pinned contract) →
   `turn.cancel.cancel()` → `Ok(())`. None ⇒ `Ok(())` (idempotent).
2. In-loop: `cancel.cancelled()` wins any `select!` arm ⇒ stop round
   (a mid-`invoke` call gets `ToolError{aborted}` → card `error`) ⇒
   assistant row `error` (best-effort — post-cascade writes land
   `not_found`, logged, swallowed) ⇒ wrapper releases.
3. Registration (boot wiring pin): `register_delete_hook` exists only
   on the concrete `OxpilotSessionService`, so `main.rs` constructs the
   concrete service → `register_delete_hook(OxpilotTurnAbortHook::
   new(state.oxpilot.clone()))` → `install_sessions(svc)` — hooks attach
   BEFORE the port is installed so no request can observe the
   unhooked state, and U6's hook registers on the same construction
   line in U6's own change (order = C8).

### WF5 — `message_queue` atomic enqueue + queued card (BR3.4)

The proc's tail gains an atomicity pin the review caught: the earlier
live-turn check stays advisory (error-shape), but the COMMIT runs under
`turns().admission_lock(session_id)` — the per-session `Arc<Mutex<()>>`
the map hands out — inside which the proc RE-verifies a live turn then
`enqueue`s. The dying turn's `release`→recheck-drain section holds the
same lock, so a row can never commit inside the recheck's blind window:
either the locked recheck sees the committed row (chains) or the
enqueue sees no live turn and refuses `session_active_turn_absent`.

After the locked commit, the proc:
1. Guard on the receipt: `queued_id == None` (transcript-dedupe hit —
   `{null,null}`) ⇒ emit nothing; a null-ref card is malformed C2.
2. `runtime.turns().get(session_id)` — under the live-turn guarantee
   from the locked section this is Some at commit; between commit and
   push the turn may still terminal — a dropped card is fine, the row
   exists and `queue_list` resolves.
3. Push `{"type":"card","card_kind":"queued","ref_id":<queued_id>}`
   into `emitted_deltas` + `fanout`. Staleness resolves client-side:
   a replayed card whose ref no longer appears in `queue_list` (row
   since drained) is dropped by the client.

### WF6 — Stream-resume (unchanged plumbing, new content classes)

`chat_stream` snapshot/attach/lagged/ping behavior is shipped and
kept. U3 additions that flow THROUGH it without shape change:
`tool` frames, `card{queued}` frames, `terminal{aborted}`, and chained
turns. Chained-turn pickup is exactly the shipped reconnect path: an
attached reader ENDS on the first terminal (it does not migrate); the
client reconnects → fresh snapshot advertises the successor's
`active_turn` (a reader polling inside the release→reclaim gap sees
`active_turn:null` — transient, next poll/attach picks it up) →
attach-poll subscribes to the new fanout.

## Contract deltas this unit carries

- **C4 amendment** (flagged, additive): `ChatMsg` gains
  `tool_call_id: Option<String>` + `tool_name: Option<String>` +
  `tool_calls_json: Option<String>`; `role` gains `"tool"`. Adapter
  serialization updates in the same change: openai → assistant
  `tool_calls` array + `role:"tool"` + `tool_call_id`; anthropic →
  assistant `tool_use` blocks + ONE following `role:"user"` message
  whose content leads with `tool_result` blocks in call order
  (consecutive `role:"tool"` msgs coalesce; pairing every `tool_use`
  is a hard wire rule); ollama → `role:"tool"` + `name` (name-keyed —
  `tool_name` carries it). The turn loop mints `call_id =
  call_{message_id}_{call_index}` when the provider's id is empty.
- **C6 provided**: `ToolExecutor` + `ToolCtx` + `ToolCall` +
  `ToolOutput` + `ToolError` defined in `oxidean-api-core::oxpilot`;
  slot gains `tools: Option<Arc<dyn ToolExecutor>>`; `provider_for_turn`
  widens to `turn_parts()` returning the triple; `NullTools` ships as
  the default (empty catalog — a model emitting calls anyway gets a
  failed card + error result fed back).
- **C2 additive use**: `tool` frames carry `call_id` + `call_index`
  (unknown-field tolerance makes this additive-safe — C2 ownership
  rules).
- **C5 consumed**: `transcript`, `drain_queue`, `append_message`,
  `set_message_content`, `set_message_status`, `append_tool_call`,
  `set_tool_call_status`, `record_usage`, `check_budget`,
  `find_message_by_client_msg_id`, `list_queue` (queued-arm dedupe),
  `list_messages`/`list_tool_calls` (snapshot, already wired).
- **C8 implemented**: `OxpilotTurnAbortHook` (first registrar).

## Procedure/route deltas

| Surface | Change | Errors added to the path |
|---|---|---|
| `POST .../chat` | budget gate (429), residual drain, real turn spawn | `budget_exceeded` (C1-pinned already) |
| `GET .../chat/stream` | none — plumbing carries new frames | — |
| `oxpilot.message_queue` | queued-card emit after insert | none |
| `oxpilot.turn_cancel` | NEW proc (C1-pinned, previously orphaned): SessionOnly + gate + owned session → live turn ⇒ `emit_terminal{aborted}` + `cancel.cancel()` ⇒ C1's pinned `{aborted: true}`; absent turn ⇒ C1's `no_active_turn` (NOT `session_active_turn_absent` — that code is `message_queue`'s). TWO pinned deviations from C1's generic verbs: (a) PAT refusal surfaces `auth.pat_scope` (the upstream SessionOnly gate emits it before dispatch — the proc body never sees the credential class; C1's `forbidden` is the generic verb, the shipped arm is `pat_scope`); (b) `{aborted: bool}` semantics — `true` when this call won the terminal CAS and observed the cancel take; a CAS-loss (turn terminalled between live-check and emit) answers `{aborted: false}` rather than `no_active_turn` — the caller's intent (turn is dead) is met either way, and C1's bool exists exactly to carry that distinction | `not_found`, `oxpilot.disabled`, `no_active_turn` |
| boot (`main.rs`) | abort-hook registration | — |

## Negative paths

| Condition | Answer |
|---|---|
| Provider `stream()` open fails | terminal `{error, <provider code>}`; no usage recorded |
| Mid-stream `Error` | terminal `{error, code}`; row `error`; usage counted |
| Abort mid-turn | terminal `{aborted}`; row `error`; usage counted if stream opened |
| Turn exceeds 10 min | `turn_timeout` terminal; cancel fired |
| Tool round 8 reached with calls pending | `tool_rounds_exceeded` terminal |
| Over budget at send | 429 pre-claim |
| Foreign session | 404 `not_found` before any other check |
| No executor installed | empty catalog; emitted calls get `error` results fed back |
| `model = None` on pinned slot | terminal `{error, "no_model"}` — slot has provider but no resolved model (keyless ollama w/o configured model) |
| Card persist (`append_tool_call`) fails | `tool{status:"error"}` frame + `ToolError{card_persist}` fed back; turn continues; warn-log |
| Residual drain fails (chat_post) | release + 500 `oxpilot.internal` |
| End-of-turn drain fails | warn-log `{session_id}`, empty batch, terminal still emitted, release |
| Panic inside turn | wrapper still releases the claim (inner-spawn + JoinError) |

## Observability (BR7.1 pins)

| Event | Level | Fields |
|---|---|---|
| turn start | info | session_id, turn_id, provider_id, model |
| tool call | debug | session_id, tool, call_index, status |
| drain chain | debug | session_id, drained_count, generation |
| abort | warn | session_id |
| budget refusal | debug | scope |
| terminal | debug | session_id, result, code? |
