# Rules — session-core (U2)

## Rule set

```yaml
rules:

  # ---- BR1 — caller scoping & gates ----

  - id: BR1.1
    statement: >
      Every `SessionService` method takes the caller's `user_id`; row
      lookups filter through `oxpilot_sessions.user_id`. A session that
      is missing or belongs to another user resolves `not_found` — never
      a distinguishing error — per U1's shipped `find_owned_session`
      precedent. DELIBERATE REFINEMENT of C5's "forbidden at this layer
      too" wording: the shipped store already resolves cross-user to
      `not_found` to avoid the 403/404 existence oracle; contract text
      reconciliation requested (denied ⇒ not_found).
    category: authorization
    applies_to: all reads/writes (session, message, tool_call, queue, proposal)
    trigger: any service call
    logic: IF row.user_id != caller.user_id OR row absent THEN not_found.
    violation_behaviour: `not_found` error; no row data disclosed.
    source: FR1.2, FR2.4, AC1.5.2, C5 rules (refined).

  - id: BR1.2
    statement: >
      `user_id` on writes always comes from the authenticated caller
      context, never from request payload fields — no client may set or
      override row ownership.
    category: authorization
    applies_to: insert paths (session/message/tool_call/queue/proposal/usage)
    trigger: any insert
    logic: IF write row carries user_id THEN bind caller.user_id unconditionally.
    violation_behaviour: n/a — the field isn't in the request schema.
    source: FR1.2, C5 rules.

  - id: BR1.3
    statement: >
      The new procs (`oxpilot.session_get`, `oxpilot.session_delete`,
      `oxpilot.message_queue`, `oxpilot.queue_list`, `oxpilot.usage_get`)
      are SessionOnly-gated per the U1 pin, and — like every proc except
      `oxpilot.status` — check the enablement gate FIRST: disabled ⇒
      `oxpilot.disabled` before any other logic (C1 note, shipped at
      session.rs:58).
    category: authorization
    applies_to: the five new procs
    trigger: dispatch
    logic: IF credential not session-class ⇒ deny; ELSE IF slot disabled ⇒ oxpilot.disabled.
    violation_behaviour: same vocabulary as existing gate.
    source: AC5.5.1, C1, U1 SessionOnly pin + enablement gate.

  # ---- BR2 — transcript & message integrity ----

  - id: BR2.1
    statement: >
      `seq` is monotonic per session on `oxpilot_messages`; minted inside
      the insert so a failed insert never burns a value. Queued rows and
      tool_call rows do NOT mint on this axis — queue positions are
      `queue_seq`, tool calls copy the owning message's seq positionally.
    category: constraint
    applies_to: oxpilot_messages inserts
    trigger: append_message, drain-delivery append
    logic: seq = MAX(seq)+1 within the session's insert, UNIQUE(session_id, seq) enforced.
    violation_behaviour: conflict surfaces as a storage error; never a silent gap.
    source: FR2.4, NFR3 transcript integrity.

  - id: BR2.2
    statement: >
      `client_msg_id` dedupe is UNION-deduped across BOTH
      `oxpilot_messages` and `oxpilot_queued_messages`: a send whose
      (session_id, client_msg_id) exists in either table returns the
      existing artifact — a message persisted pre-turn then resent
      re-attaches via `resend_matches`; a queued row resent on the POST
      while a turn is live answers 409 `turn_active` (the resend is
      already queued — the SSE route has no queued-position return
      shape); a queued row resent on `message_queue` returns its
      existing `{queued_id, position}`. The live-turn table split never
      produces a duplicate.
    category: constraint
    applies_to: append_message (chat POST), message_queue (enqueue), drain-append
    trigger: insert with client_msg_id
    logic: IF (session_id, cmid) in messages THEN attach/return it; ELIF in queued THEN POST ⇒ 409 turn_active / message_queue ⇒ return existing; ELSE insert.
    violation_behaviour: return the persisted artifact, no duplicate.
    source: AC1.6.1 (exactly-once per message), U1 claim path, review R-09/R-14.

  - id: BR2.3
    statement: >
      Transcript reads return rows ordered by `seq` ascending with
      `status` in {complete, error} — `streaming` rows are the in-flight
      row the SSE GET drives, not history; queued rows are NOT transcript
      entries (no seq until drained). `error` rows persist so a reopened
      transcript renders the interrupted message (U1 sweep marks orphans
      'error'; C2 terminal error|aborted map to it).
    category: validation
    applies_to: transcript, queue_list
    trigger: U3 prompt assembly, SSE snapshot
    logic: transcript = messages WHERE session AND status IN (complete,error) ORDER BY seq; pending = queued ORDER BY queue_seq.
    violation_behaviour: n/a — read-shape rule.
    source: FR2.4, AC1.3.3, AC1.8.2, review R-04.

  - id: BR2.4
    statement: >
      `last_activity_at` on the session is touched on every successful
      message, tool-call, enqueue, and proposal write so the index list
      ordering (AC1.1.1 newest-first) reflects real activity.
    category: calculation
    applies_to: all child-row writes
    trigger: successful child insert/update
    logic: UPDATE oxpilot_sessions SET last_activity_at=now WHERE id=session_id.
    violation_behaviour: n/a — inside the same write path.
    source: AC1.1.1, FR1.2.

  # ---- BR3 — redaction ----

  - id: BR3.1
    statement: >
      Every body-bearing write is secrets-redacted at the service
      boundary before the store call: message content, queued content,
      proposal title AND diff_body, tool_call args_summary/output_summary,
      and session goal/title. `oxidean_core::redact()` is the single
      detector — the same function U1's SSE path uses.
    category: policy
    applies_to: all persisted text fields
    trigger: every write
    logic: stored = redact(input); reads return stored text untouched.
    violation_behaviour: n/a — redaction is unconditional.
    source: AC5.3.3, NFR1, C5 rules ("nothing leaves unredacted"), review R-08.

  # ---- BR4 — queue (US1.8, contract-pinned semantics) ----

  - id: BR4.1
    statement: >
      Queuing is CLIENT-DRIVEN through `oxpilot.message_queue`, never
      auto-enqueue: the chat POST keeps answering 409 `turn_active` when
      a turn is live (C2 pin, shipped at routes.rs:316-320), and the
      client then calls `message_queue{session_id, content}` which
      requires a live turn (none ⇒ `session_active_turn_absent`), runs
      `check_budget` (over ⇒ `budget_exceeded`), union-dedupes on
      `client_msg_id` (BR2.2), and inserts the queued row returning
      `{queued_id, position}` (position = queue_seq order index).
      `queue_seq` mints inside the insert under
      UNIQUE(session_id, queue_seq) — a concurrent enqueue collision
      retries the mint, same mechanics as message `seq` (BR2.1).
    category: validation
    applies_to: oxpilot.message_queue proc + queued table
    trigger: client follow-up after 409 turn_active
    logic: gate → owned session → live turn required → budget → dedupe → insert.
    violation_behaviour: `not_found` foreign session; `session_active_turn_absent` when no turn; `budget_exceeded` at cap.
    source: AC1.8.1, C1 `message_queue` pin, C2 409 rule, review R-01.

  - id: BR4.2
    statement: >
      `drain_queue` is a SINGLE-TRANSACTION drain-into-transcript: it
      locks/claims the session's queued rows in `queue_seq` order
      (DELETE…RETURNING or SELECT…FOR UPDATE equivalent per dialect),
      inserts each as an `oxpilot_messages` role=user row minting `seq`
      and CARRYING `client_msg_id`, and commits both halves atomically —
      a crash anywhere mid-drain loses nothing (queued rows remain) and
      delivers nothing twice. Exactly one drainer exists per session at
      a time by construction: U3's one-active-turn-per-session invariant
      makes the turn-end drain single-writer.
    category: constraint
    applies_to: oxpilot_queued_messages → oxpilot_messages
    trigger: turn end AND turn start (U3)
    logic: tx{ DELETE queued WHERE session RETURNING * ORDER BY queue_seq; INSERT messages minting seq, carrying cmid } — atomic.
    violation_behaviour: empty queue ⇒ empty vec, no error; tx failure ⇒ nothing consumed, nothing appended. Residual rule: rows orphaned by a crash between drain points deliver at the NEXT turn start (drain runs before the new turn's own message consumes) in queue_seq order — never out-of-order relative to each other, never silently dropped.
    source: AC1.8.1, C5 api, review R-07/R-09/R-22.

  - id: BR4.3
    statement: >
      `queue_list` (`oxpilot.queue_list` proc + service list) is
      read-only: returns `{queued:[QueuedMessage]}` ordered by
      `queue_seq` for the UI pending chip and the C2 `card{queued}`
      ref-resolution — it never consumes.
    category: validation
    applies_to: oxpilot.queue_list proc
    trigger: proc call / card resolution / UI poll
    logic: SELECT WHERE session ORDER BY queue_seq — no delete.
    violation_behaviour: `not_found` foreign session; `oxpilot.disabled` gate (BR1.3).
    source: AC1.8.2, C1 `queue_list` pin, C2 queued-card rule, review R-02.

  # ---- BR5 — delete cascade (US1.5, C8) ----

  - id: BR5.1
    statement: >
      `delete_session` verifies caller ownership (BR1.1), then runs
      registered `SessionDeleteHook`s in registration order BEFORE the
      DB delete. U2 ships zero registrars — the seam exists, U3
      (abort-turns) and U6 (revoke-grants) inject at shell construction.
    category: constraint
    applies_to: oxpilot.session_delete proc + SessionService.delete_session
    trigger: proc call
    logic: gate → owned? → for h in hooks { h.on_session_delete(id, user)? } → DELETE session.
    violation_behaviour: missing/foreign session ⇒ `not_found`; gate-off ⇒ `oxpilot.disabled`.
    source: US1.5, C8 hook contract, C1 session_* family.

  - id: BR5.2
    statement: >
      A hook failure aborts the delete and surfaces the error — the
      session and all its rows remain. Hooks are documented idempotent;
      re-entry is a no-op for each registrar.
    category: constraint
    applies_to: delete_session hook loop
    trigger: hook returns Err
    logic: propagate error; do NOT issue the DELETE.
    violation_behaviour: caller sees the hook's error; nothing deleted.
    source: C8 rules, ADR-010.

  - id: BR5.3
    statement: >
      The DB delete is a single `DELETE FROM oxpilot_sessions` relying on
      the declared `ON DELETE CASCADE` chain — messages, tool_calls,
      queued, proposals all go with it. `oxpilot_usage_counters` survive
      (audit-grade rollup, keyed by user/day, no FK).
    category: constraint
    applies_to: delete path
    trigger: after hooks succeed
    logic: one statement; FK cascade does the rest.
    violation_behaviour: n/a.
    source: AC1.5.2, C8.

  # ---- BR6 — usage counters & budget refusal (US5.2) ----

  - id: BR6.1
    statement: >
      `record_usage` upsert-increments (scope, scope_key, window) where
      window is the UTC ISO day (`YYYY-MM-DD`, UTC-midnight rollover —
      never local/server time). `requests` always +1 per completed turn
      exchange; `tokens` += the reported count when the provider supplies
      `Done{usage}`. Both the user row (scope_key=user_id) and the
      instance row (scope_key='__instance__') update on every call.
    category: calculation
    applies_to: oxpilot_usage_counters
    trigger: completed turn exchange (U3 hook point; U2 ships the counter)
    logic: INSERT ... ON CONFLICT(scope,scope_key,window) DO UPDATE SET requests=requests+1, tokens=tokens+?.
    violation_behaviour: n/a.
    source: AC5.2.1, FR6.7, review R-06.

  - id: BR6.2
    statement: >
      `check_budget(user_id)` reads today's user row and the instance
      row, compares BOTH axes — `requests` against requests_cap and
      `tokens` against tokens_cap — to the resolved U4 budget value V
      (one configured `user_budget`/`instance_budget` sets both caps:
      requests_cap=V, tokens_cap=V in `oxpilot.usage_get` terms), and
      returns `BudgetExceeded{scope}` when either axis is at/over.
      A missing counter row reads as zero.
    category: policy
    applies_to: turn-start gate (U3 consumes) + message_queue (BR4.1)
    trigger: turn start / enqueue
    logic: IF user.requests >= V OR user.tokens >= V OR instance.* >= V THEN BudgetExceeded.
    violation_behaviour: in-session refusal via the error vocabulary (US5.2.2) — 429/typed `budget_exceeded`, never a silent drop.
    source: AC5.2.2, FR6.7, C1 `usage_get`/`message_queue` pins.

  - id: BR6.3
    statement: >
      An ABSENT configured budget (env unset, override NULL) resolves
      unbounded — `check_budget` returns Ok and `usage_get` reports
      `*_cap: null`. This IS the generous default of AC5.2.1: configured
      means bounded, unconfigured means unlimited by deliberate choice.
    category: policy
    applies_to: check_budget, usage_get
    trigger: resolved budget absent
    logic: IF budget is None THEN allow / cap=null.
    violation_behaviour: n/a.
    source: AC5.2.1, review R-06.

  # ---- BR7 — service boundary ----

  - id: BR7.1
    statement: >
      `SessionService` is reached through a TRAIT PORT — `trait
      SessionService` declared in `oxidean-api-core` beside `LlmProvider`,
      with the concrete `OxpilotSessionService` implemented in
      `oxidean-api-oxpilot` over `oxidean-db` stores. The runtime slot /
      RpcCtx holds `Arc<dyn SessionService>` exactly as it holds
      `Arc<dyn LlmProvider>` — api-core never names the concrete type,
      no crate cycle. `trait SessionDeleteHook` (C8) also lives in
      api-core beside SessionService so U3/U6 register hooks against a
      type api-core already knows.
    category: constraint
    applies_to: crate layering
    trigger: module layout
    logic: trait in api-core, impl in api-oxpilot — same direction as the provider seam.
    violation_behaviour: n/a — compile-time layering rule.
    source: C5, components.md, LlmProvider precedent, review R-03.

  - id: BR7.2
    statement: >
      Transcript-for-prompt reads return the full ordered set including
      `error` rows so U3's prompt assembly sees what the user saw;
      windowing/truncation for prompt budget is a U3 concern. There is
      NO `oxpilot.transcript` proc — remount flows through the C2 SSE
      snapshot (Message + ToolCall rows + pending_queue).
    category: policy
    applies_to: service transcript read
    trigger: U3 prompt assembly
    logic: return all non-streaming rows; caller windows.
    violation_behaviour: n/a.
    source: FR2.4, C5 api, C2 snapshot frame, review R-13.

  - id: BR7.3
    statement: >
      `append_tool_call` writes keyed by (message_id, call_id) with a
      positional `seq` copy PLUS a per-message `call_index` (0-based
      emission order inside the message) — the C2 snapshot merge orders
      a message's tool cards by `call_index`, deterministic even under
      same-millisecond created_at. A `set_tool_call_status` path
      transitions running → done|error. Both caller-scope through the
      owning session.
    category: constraint
    applies_to: oxpilot_tool_calls
    trigger: U3 emission / U5 tool completion
    logic: insert at message.seq + next call_index for the message; status updates by (session_id, call_id); intra-message order = call_index.
    violation_behaviour: unknown call_id ⇒ not_found at service level.
    source: C6 card contract, C2 tool frame, review R-05/R-12/R-19.
```

## Summary

| Rule | Category | Binds |
|------|----------|-------|
| BR1.1–1.3 | authorization | user-scope + `not_found` anti-enumeration (C5 refined), SessionOnly + enablement gate |
| BR2.1–2.4 | constraint/calculation | seq minting, union dedupe, transcript shape {complete,error}, activity touch |
| BR3.1 | policy | redact-on-write, single detector |
| BR4.1–4.3 | validation/constraint | contract-pinned `message_queue`/`queue_list`, atomic drain-into-transcript |
| BR5.1–5.3 | constraint | C8 hook-then-cascade, abort-on-fail, FK cascade |
| BR6.1–6.3 | calculation/policy | UTC-window rollup, dual-axis BudgetExceeded, absent⇒unbounded |
| BR7.1–7.3 | constraint/policy | trait-port layering (LlmProvider precedent), transcript read shape, tool-call identity |
