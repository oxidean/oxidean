# Business Rules — agent-runtime (U3)

## Rule set

```yaml
rules:
  # ---- BR1 — gates & turn admission (chat_post order) ----
  - id: BR1.1
    kind: constraint
    text: >
      `POST /api/oxpilot/sessions/{id}/chat` admission order is pinned:
      session auth → enablement gate → owned session (foreign/absent
      ⇒ 404 `not_found`) → body validate (422 `invalid_body` — content
      non-empty ≤ 64 KiB, client_msg_id ≤ 128 B) → union dedupe (BR2.2)
      → `check_budget` (429 `budget_exceeded`, BR6.2 — `svc_err` gains
      a dedicated `budget_exceeded` → 429 arm; the catch-all would
      silently 500 it) → one-live-turn claim (409 `turn_active`) →
      post-claim dedupe RECHECK (release-on-hit — the shipped window
      cover; a recheck exit NEVER drains) → residual drain (BR4.2) →
      user-row insert → turn task spawn → attach.
    rationale: >
      Ownership before dedupe/budget keeps foreign ids indistinguishable;
      dedupe before budget keeps resends free (a resend consumes no new
      spend — an over-budget resend still attaches); budget before claim
      refuses spend before occupying the session.
    violation_behaviour: typed non-2xx JSON per the C2 vocabulary — never SSE.
    applies_to: chat_post.

  - id: BR1.2
    kind: constraint
    text: >
      One live turn per session, enforced by `ActiveTurns.claim` — the
      claim winner is the only writer of frames, the only drainer, and
      the only `record_usage` writer for that turn.
    rationale: one-active-turn is the load-bearing invariant for
      exactly-once drain, terminal semantics, and seq ordering.
    violation_behaviour: losing POST ⇒ 409 `turn_active` (client calls
      `oxpilot.message_queue`).

  - id: BR1.3
    kind: policy
    text: >
      `provider_for_turn()` pins `{provider Arc, model, executor}` under
      one lock read at turn start (widened to `turn_parts()`); a mid-turn
      settings rebuild swaps the SLOT, never this turn's pins (ADR-010).
      `provider = None` ⇒ the turn fails terminally with
      `oxpilot.disabled`-equivalent code — the enablement gate normally
      refuses earlier, this arm is the post-gate rebuild race cover.
    rationale: provider identity must be stable across one reply's
      multi-round tool protocol.
    applies_to: run_turn.

  # ---- BR2 — transcript & message integrity inside the loop ----
  - id: BR2.1
    kind: constraint
    text: >
      One turn writes exactly ONE assistant row (caller-minted
      `message_id` on `ActiveTurn`, status `streaming` at insert) whose
      content accumulates across all tool rounds; `seq` mints inside the
      service insert. Tool-call rows hang off it by `message_id` with
      per-message `call_index` ordinals. No second assistant row per
      turn — the C2 snapshot joins cards by message_id and U8 renders
      one bubble per row.
    violation_behaviour: a second assistant insert per turn is a defect;
      tests assert row counts.

  - id: BR2.2
    kind: constraint
    text: >
      Union dedupe on `client_msg_id` runs transcript-first then queued
      (U2 BR2.2 — shipped); U3's change is only WHERE in the order it
      runs (BR1.1). Transcript hit ⇒ attach (live, matching) or
      snapshot; queued-only hit ⇒ 409 `turn_active`; miss ⇒ proceed.
    applies_to: chat_post (U2-owned rule, restated for ordering context).

  - id: BR2.3
    kind: calculation
    text: >
      Prompt assembly reads `svc.transcript` (complete+error rows only,
      BR2.3 of U2 — `streaming` self-row excluded by the status filter)
      and windows it: walk newest→oldest accumulating until
      `PROMPT_TRANSCRIPT_MAX_BYTES` (128 KiB), then reverse to temporal
      order. Each assistant row appends bounded `[tool_call <name> →
      <status>]` summary lines for its persisted tool-call rows (one
      line per call, ≤ 200 bytes each).
    rationale: bounded prompt under provider context limits while the
      model keeps visibility of what it already did this session.
    applies_to: prompt assembly.

  - id: BR2.4
    kind: constraint
    text: >
      Durable-first ordering — a card row persists (status `running`)
      BEFORE its `tool` frame emits; the assistant row's terminal
      status write precedes the terminal frame; drained rows land in
      the transcript BEFORE the terminal frame that ends the turn. A
      crash mid-ordering leaves rows, never phantom frames.
    rationale: snapshot-remount reads rows — frames that outrun their
      rows would vanish on remount.

  # ---- BR3 — tool orchestration (C6 seam) ----
  - id: BR3.1
    kind: constraint
    text: >
      `ChatRequest.tools` = `executor.catalog()` (the slot-pinned
      executor). Tool calls arrive as `ChatStreamItem::ToolCall{id,
      name, args_json}`; each is invoked via `executor.invoke(ToolCall{
      call_id:id, name, args_json}, ToolCtx{session_id, user_id,
      repo_id, proposal_id?})` — ADR-008's identity envelope. The
      executor is the ONLY authz seam (C6) — the runtime never
      pre-filters calls it doesn't understand.
    applies_to: turn loop.

  - id: BR3.2
    kind: constraint
    text: >
      Tool results re-enter the provider natively (C4 amendment): the
      round's assistant message is buffered with `tool_calls_json`
      (serialized call array), each result follows as
      `ChatMsg{role:"tool", tool_call_id, tool_name, content:output}`.
      The next round's request = windowed transcript + the round
      buffer. Three pins make it real on every adapter: (a) the turn
      loop MINTS a fallback `call_id` (`"call_{message_id}_{call_index}"`)
      when the provider's `id` is empty — Ollama may omit ids and the
      durable card keys on `call_id` (unique per session), so an empty
      id would collide on the second call and wedge durable-first;
      (b) `ChatMsg` carries `tool_name` too — Ollama's tool-result
      message is name-keyed, not id-keyed; (c) Anthropic coalescing —
      consecutive `role:"tool"` msgs for one round merge into ONE
      `role:"user"` message whose content array leads with
      `tool_result` blocks in call order, emitted only once every
      emitted call has a result (its `tool_use`↔`tool_result` pairing
      is a hard wire rule; one-user-message-per-result is documented-
      bad). All three adapters map these fields to native shapes.
    violation_behaviour: dropping a result block desyncs providers that
      require tool_call/result pairing — the loop must append a result
      for EVERY emitted call (an executor error is still a result:
      status `error`, content = error text).

  - id: BR3.3
    kind: constraint
    text: >
      Tool rounds bound at `MAX_TOOL_ROUNDS = 8` per turn; a round that
      completes `MAX_TOOL_ROUNDS` with calls still pending answers
      terminal `{result:"error", code:"tool_rounds_exceeded"}` after
      persisting the last card `error`. Per-call execution wraps in a
      `TOOL_CALL_TIMEOUT = 60 s` `tokio::time::timeout` racing
      `turn.cancel`; timeout/abort ⇒ `ToolError{code:"timeout"|
      "aborted"}` → card `error` → the model sees the failure (the
      model may recover within remaining rounds).
    rationale: bounds provider spend and turn wall time; keeps the
      model informed rather than silently killing calls.

  - id: BR3.4
    kind: policy
    text: >
      `message_queue`'s enqueue is ATOMIC with liveness: under
      `turns().admission_lock(session_id)` the proc re-verifies a live
      turn then commits the insert — serializing against the dying
      turn's release→recheck section (BR4.1) so no row lands in a
      drainless window. The first live-turn check earlier in the proc
      order stays advisory (error-shape); this locked one is
      authoritative. Post-commit, `message_queue` emits
      `card{type:"card", card_kind:"queued", ref_id}` into the live
      turn's `emitted_deltas` + `fanout` — ONLY when the receipt
      carries `queued_id: Some(_)` (a
      transcript-dedupe hit returns `{null,null}`; a null-ref card is
      malformed C2). This is the one sanctioned non-owner push (the
      proc already proved a live turn via `session_active_turn_absent`).
      Late attachers replay it from the buffer; C2's ref resolves via
      `oxpilot.queue_list`, and a client reconciles staleness the same
      way — a buffered card for a since-drained row is dropped client-
      side when `queue_list` no longer lists it.
    applies_to: message_queue (proc) + ActiveTurn buffer.

  # ---- BR4 — queue drain & chaining ----
  - id: BR4.1
    kind: constraint
    text: >
      End-of-turn drain runs INSIDE the turn task: assistant terminal
      status write → `svc.drain_queue` (atomic — the U2 drain rule) →
      terminal frame emit → `release` → **post-release recheck**: one
      final `svc.drain_queue` on the released session — an enqueue that
      committed in the drain→release window passed the live-turn check
      while no drainer remained; the recheck is the airtight seam.
      Non-empty batch (from either drain) ∧ `generation <
      MAX_CHAIN_DEPTH` ⇒ immediately `claim` + `spawn_turn_task` seeded
      by it → recurse; claim-loss to an interleaved POST ⇒ exit
      quietly (the winner's turn consumes the delivered rows in its
      window). Empty ∨ depth-cap ⇒ exit. **The airtight pin**: the
      release→recheck section runs under a per-session admission lock
      (`ActiveTurns.admission_lock(session_id)` — an
      `tokio::sync::Mutex<()>` per session, stored alongside the
      entry and dropped with it; held across the drain's DB I/O —
      acceptable, it's per-session) and `message_queue`'s
      live-turn re-verify + `enqueue` commit run under the SAME lock
      (BR3.4) — guard-scoped so a panic inside either section drops
      it and can't wedge the session; an enqueue can no longer
      commit inside the recheck's
      blind window: either the lock holder's recheck sees the row
      (chains) or the enqueue sees no live turn and refuses
      (`session_active_turn_absent` → the client's POST path + its
      residual drain). A recheck-drain `Err` under the lock ⇒
      warn-log `{session_id}` and exit (no chain).
    rationale: delivered follow-ups get replies without new POSTs; the
      claim-per-turn shape preserves fairness; the recheck closes the
      commit-window race that would strand a just-queued row.

  - id: BR4.2
    kind: constraint
    text: >
      Residual drain (the U2 WF5.3 rule) runs in `chat_post`
      post-claim/post-recheck/pre-append — orphaned queued rows mint
      `seq` before the triggering send so transcript order stays
      temporal. The claim winner at that instant is the only possible
      drainer (BR1.2). Drain failure semantics are pinned both places:
      residual-drain `Err` ⇒ `release` + 500 `oxpilot.internal` (the
      send fails honestly, no row mints out of order); end-of-turn
      `Err` ⇒ warn-log `{session_id}`, treat as empty batch, still emit
      terminal + release (a drain failure must never eat the turn's
      own terminal).
    applies_to: chat_post.

  - id: BR4.3
    kind: constraint
    text: >
      `MAX_CHAIN_DEPTH = 16` bounds consecutive auto-chained turns per
      initial send (runaway queue protection); reaching it ends the
      chain — remaining queued rows drain at the NEXT turn start
      (residual drain) rather than dropping.
    rationale: bounds provider spend per user action; nothing is ever
      lost — the queue is the durable floor.

  # ---- BR5 — abort & failure semantics ----
  - id: BR5.1
    kind: constraint
    text: >
      `OxpilotTurnAbortHook` (C8, registered first at boot — before
      U6's grant hook): `on_session_delete` looks up the live turn,
      pushes `terminal{result:"aborted"}` into buffer+fanout, calls
      `cancel.cancel()`, returns `Ok(())`. Never waits on the task;
      idempotent (absent turn ⇒ `Ok`). The turn task observes the
      token in its `select!` arm, sets the assistant row `error`
      (status vocabulary is streaming|complete|error — `aborted` lives
      only on the frame), and unwinds; post-cascade row writes land
      `not_found` and are logged, never propagated.
    applies_to: SessionDeleteHook registration + turn loop select.

  - id: BR5.2
    kind: constraint
    text: >
      A `stream()` open failure or `ChatStreamItem::Error` produces
      terminal `{result:"error", code}` — `code` is the provider's
      stable code verbatim (C4 pin); persisted status `error`; the
      transcript row stays (AC1.4.2). Internal failures (persist/
      assembly) answer `code:"internal"`. Abort answers
      `{result:"aborted"}` with no code. Runtime-minted codes:
      `tool_rounds_exceeded`, `turn_timeout` (turn wall cap
      `TURN_MAX_DURATION = 10 min`), `no_model` (provider pinned but
      no resolved model — keyless Ollama without a configured model),
      `card_persist` (durable card write failed — fed back as a
      ToolError result, not terminal). `oxpilot.disabled` reaches the
      frame path only on the WF4-side kill (mid-turn flag flip) —
      it too is runtime-minted.
    rationale: AC1.4.1 — every failure is a provable terminal frame.

  - id: BR5.3
    kind: constraint
    text: >
      The turn task ALWAYS releases its claim — including panic paths.
      The pinned mechanism: `spawn_turn_task` wraps `run_turn` in an
      INNER `tokio::spawn` and awaits its `JoinHandle` — `Err(JoinError)`
      covers panic and cancellation alike, so the wrapper's release arm
      runs on every exit shape (Ok, Err, panic). The turn body never
      releases; the wrapper owns it unconditionally. One leaked claim
      pins the session at 409 forever.
    rationale: the map entry IS the session's liveness; `JoinError` is
      Rust's unwind boundary — no Drop-ordering subtlety.

  - id: BR5.4
    kind: constraint
    text: >
      Terminal emission is FIRST-WINS across its three writers (the
      abort hook, `run_turn`'s exit paths, the wrapper's Err arm):
      `ActiveTurn` gains `terminal_sent: AtomicBool` and every terminal
      frame goes through `emit_terminal(&turn, frame)` which
      compare-and-sets under the `emitted_deltas` lock — the loser
      no-ops. On collision the abort hook's `aborted` frame wins when it
      lands first; a `done`/`error` already emitted is not overwritten
      by a later `aborted` (the delete's cascade proceeds regardless).
      C2's "frames after terminal are never emitted" holds in the
      replay buffer.
    rationale: the buffer replays verbatim — a second terminal in it
      double-ends late attachers and violates C2.
    applies_to: ActiveTurn + abort hook + turn task + wrapper.

  # ---- BR6 — usage recording ----
  - id: BR6.1
    kind: calculation
    text: >
      `record_usage(user_id, UsageDelta{requests:1, tokens:t})` fires
      once per TURN iff the provider stream yielded ≥1 item across all
      rounds (Delta / ToolCall / Done / Error — partial consumption
      counts: `turn_timeout` and `tool_rounds_exceeded` after streamed
      items are counted; a `stream()` open failure or abort before the
      first item consumed nothing → uncounted). `t` sums every round's
      `Done.usage` (None contributes +0 — the request still counts).
    applies_to: turn terminal path.
    rationale: BR6.2's budget reads these counters — undercounting
      spends unbudgeted provider capacity.

  - id: BR6.2
    kind: policy
    text: >
      `check_budget(user_id, slot.budget_caps)` at the BR1.1 position —
      absent caps ⇒ unbounded (U2's absent-caps rule); over ⇒ 429 before claim.
      C1's `budget_exceeded` on POST; the proc arm is U2's.
    applies_to: chat_post.

  # ---- BR7 — observability & misc pins ----
  - id: BR7.1
    kind: constraint
    text: >
      Pinned log lines: `info` turn start {session_id, turn_id,
      provider_id, model}; `debug` tool call {session_id, tool,
      call_index, status}; `debug` drain chain {session_id,
      drained_count, generation}; `warn` abort {session_id};
      `debug` budget refusal {scope}; terminal {session_id, result,
      code?} at `debug`.
    rationale: NFR6 — reuse existing tracing, no new stack.

  - id: BR7.2
    kind: constraint
    text: >
      `redact()` on every frame's user-derived field (delta content,
      tool args_summary, tool output before it reaches the provider
      buffer AND the card's output_summary) — the persisted row is the
      safety floor, the wire matches it (NFR1.8 shipped pin carried
      forward).
    applies_to: turn loop + tool execution boundary.
```

## Summary

| Range | Kind | Covers |
|---|---|---|
| BR1.1–1.3 | constraint/policy | pinned admission order, one-live-turn, pinned provider snapshot |
| BR2.1–2.4 | constraint/calculation | one assistant row per turn, dedupe position, prompt window, durable-first ordering |
| BR3.1–3.4 | constraint/policy | C6 executor seam, native tool round-trip (C4 amendment), round/time bounds, queued card |
| BR4.1–4.3 | constraint | end-of-turn atomic drain, residual drain, chain-depth bound |
| BR5.1–5.3 | constraint | C8 abort hook, terminal-error vocabulary, guaranteed release |
| BR6.1–6.2 | calculation/policy | per-turn usage accounting, budget gate |
| BR7.1–7.2 | constraint | pinned observability, redact-on-wire |
