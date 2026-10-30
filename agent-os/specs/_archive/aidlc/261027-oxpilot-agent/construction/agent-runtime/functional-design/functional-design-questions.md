# Functional Design Questions — agent-runtime

Unit: `agent-runtime` (U3, kind `service`) — owns `AgentRuntime`: the real
turn loop (prompt assembly, multi-round tool orchestration, provider
streaming → SSE frames), per-turn provider pinning, turn-start budget
gate, usage recording, queue drain + turn chaining, abort-on-delete
coordination (C8 hook #1), and the `ToolExecutor`/`ToolCtx` seam it
provides to U5 (C6). depends_on: U1 (crate + skeleton turn plumbing) +
U2 (`SessionService` port); consumes U4's provider slot through
`RpcCtx.oxpilot`.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U3.
- [contract] `inception/contract-design/contract-summary.md` — C2 (SSE
  frame contract + error vocabulary), C4 (`LlmProvider` + `ChatRequest`/
  `ChatStreamItem`), C5 (`SessionService` verbs incl. amendments), C6
  (`ToolExecutor`/`ToolCtx`/`ToolCard` — U3 provides to U5), C8 (delete
  hook ordering: U3 abort first).
- [req] FR2.1–FR2.4 (workspace + streamed multi-turn + persisted state),
  FR6.1 (ai-octane runtime layer), FR6.3 (SSE route contract), FR6.7
  (budget gate at turn start), FR7.x (tool catalog declared + executed),
  NFR3 (first-token budget, reconnect without duplication), NFR4
  (provider failures surface, never silent stalls).
- [stories] US1.3 (AC1.3.1 SSE stream, AC1.3.3 server transcript,
  AC1.3.5 repo-aware via read tools, AC1.3.6 tool cards), US1.4
  (AC1.4.1 terminal error frame, AC1.4.2 preserved transcript),
  US1.6 (AC1.6.1 snapshot-resume), US1.8 contribution (drain delivers
  queued follow-ups).
- [decisions] `inception/domain-design/decisions.md` ADR-008 (identity-
  bound ToolCtx), ADR-010 (provider pinned per turn; delete aborts
  before cascade).
- [shipped] `crates/oxidean-api-oxpilot/src/routes.rs` (U1/U4 turn
  plumbing: claim/dedupe/attach/snapshot/lagged-resync/ping/terminal;
  tool calls persist cards but no executor is wired; `Done{usage}`
  dropped), `crates/oxidean-api-core/src/oxpilot.rs` (`ActiveTurns`
  claim/release/cancel, `provider_for_turn`, `SessionService` port,
  `Usage`/`UsageDelta`, `StubProvider`), `crates/oxidean-api-oxpilot/
  src/session.rs` (`message_queue` pinned order; no queued-card emit),
  `crates/oxidean-api-oxpilot/src/providers/*.rs` (adapters serialize
  `ChatMsg{role,content}` verbatim + `tools` → provider-native schemas).

## Q1. Scope split — what does U3 add over the shipped turn plumbing?

- A. Keep the shipped skeleton plumbing untouched where correct
  (claim/dedupe/snapshot/attach/ping/lagged-resync are all live), and
  U3 replaces the interior of `run_turn` with the real loop —
  transcript-window prompt assembly, `tools` catalog from the executor,
  tool-call rounds fed back to the provider, `Done{usage}` →
  `record_usage`, drain-and-chain at turn end — plus the three edges
  around it: `chat_post` gains the pinned turn-start `check_budget`
  (C1's 429 arm, currently absent) and the residual drain, `message_queue`
  gains the queued-card frame emission (C2's card contract), and boot
  registers U3's abort hook before U6's (C8 order).
- B. Rebuild `chat_post`/`chat_stream` wholesale in a new module —
  discards the reviewed, test-covered plumbing and re-earns every
  dedupe/resume guarantee.
- C. Leave `run_turn` as the dumb pipe and hang tools off a separate
  path — splits one turn's frames across two emitters; violates the
  one-writer-per-turn invariant the buffer/fanout replay relies on.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — surgical interior replacement + three named edges; the
shipped plumbing stays. Conductor-decided under standing instruction
(2026-10-27); ratify at PR review.

## Q2. Tool-result round-trip — how do tool results re-enter the provider?

`ChatMsg{role, content}` (C4) cannot express provider-native tool
round-trips: OpenAI needs `role:"tool"` + `tool_call_id` plus the
assistant message's `tool_calls` array; Anthropic needs `tool_result`/
`tool_use` content blocks; Ollama needs `role:"tool"` + `name`.

- A. Extend `ChatMsg` with `tool_call_id: Option<String>` and
  `tool_calls_json: Option<String>` (the assistant message's serialized
  call array for replay); adapters map to each provider's native shape
  (openai/anthropic/ollama each get a mapping arm). Flagged as a C4
  amendment per the contract deviation rule — additive fields on a
  U1-owned type, needed by the U3 loop that C4 already contemplates
  (`ToolCall` items exist precisely so results can come back).
- B. Text-injection: tool results re-enter as `role:"user"` lines like
  `[tool read_file]: <output>` — zero schema change but silently drops
  the native tool protocol: providers that emitted real `tool_calls`
  receive no matching result block (OpenAI 400s when a `tool_calls`
  assistant turn lacks its results if we replay it; if we don't replay
  it the model never learns its call was honoured — both degrade
  agentic quality), and it diverges from what ai-octane-shaped
  providers expect.
- C. New `ToolResultMsg` sibling type outside `ChatMsg` — splits the
  ordered message list into two shapes; every adapter then merges two
  streams positionally.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `ChatMsg` gains `tool_call_id` + `tool_name` +
`tool_calls_json` (Option<String>); adapters map natively (incl.
Anthropic's single-user-message tool_result coalescing + Ollama's
name-keyed results + a runtime-minted fallback `call_id` when the
provider omits one). Logged as a C4 contract amendment (additive,
consumer-proposed per ownership rules). Conductor-decided under
standing instruction (2026-10-27); ratify at PR review.

## Q3. Turn structure — how do provider rounds compose into one turn?

- A. One turn = one `ActiveTurn` = one assistant transcript row
  (`message_id`). Each provider round streams `Delta`s (appended to
  that row's content, emitted as `append` frames with continuing
  `seq`), then either `Done` (turn ends) or `ToolCall`s (cards
  persisted `running`, invoked via the executor, `done|error` +
  `output_summary` persisted, `tool` frames emitted, results appended
  to the in-loop message buffer, next round opens). Bounded by
  `MAX_TOOL_ROUNDS` per turn; exceeding answers terminal
  `error`/`tool_rounds_exceeded`.
- B. One assistant row PER ROUND — fragments one logical reply into
  many transcript rows; the C2 snapshot merge and tool-card join
  (by `message_id`) keep working, but remount renders N bubbles for
  one turn.
- C. Unbounded rounds — a misbehaving model loops tools forever;
  no refusal exists.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — one assistant row per turn; rounds continue inside it;
`MAX_TOOL_ROUNDS = 8` with terminal `tool_rounds_exceeded` on breach.
Conductor-decided under standing instruction (2026-10-27); ratify at
PR review.

## Q4. Drain & chain — how do queued messages become turns?

- A. At turn end (after the assistant row's terminal status, BEFORE the
  terminal frame): `drain_queue` delivers all pending rows into the
  transcript atomically; then release → re-claim → the delivered batch
  seeds the NEXT turn's prompt (the runner loops while the drain keeps
  delivering). One turn per drain batch — a user batching three
  follow-ups gets one reply addressing all three; a POST landing in
  the release→reclaim gap wins the claim and lands after the drained
  rows in `seq` order (coherent, lossless).
- B. One turn per queued ROW — serial N provider calls for a burst;
  per-message replies are tidy but a 5-message batch is 5 sequential
  model waits, and each later turn's window already contains the
  earlier rows anyway.
- C. Drain delivers but no auto-turn — the queued messages sit in the
  transcript until the user sends again; violates AC1.8.1's "delivered
  after the current reply ends".
- D. Not decided.
- X. Other (please specify)

[Answer]: A — end-of-turn drain inside the task (one drainer per the
one-live-turn invariant) + a post-release `drain_queue` recheck that
closes the drain→release commit-window race, then batch-seeded
chained turns. Conductor-decided under standing instruction
(2026-10-27); ratify at PR review.

## Q5. Residual drain — where does the turn-start drain of WF5.3 live?

- A. In `chat_post` after winning the claim, before `append_message` of
  the new user row — orphaned queue rows (crashed prior turn) mint
  `seq` BEFORE the triggering send, so transcript order stays
  temporal (they were authored first).
- B. Inside `run_turn` before prompt assembly — the user row is already
  inserted by then, so orphans mint `seq` AFTER it even though they
  were authored first; the assembled prompt also shows them
  positioned after the message they predate.
- C. Drop the residual drain — relies on "crash sweeps clean"; but a
  crash leaves `queued` rows forever (they're not swept — the sweep
  only flips `streaming` message rows), silently stranding follow-ups.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — post-claim/pre-append in `chat_post`; temporal order
preserved, one-drainer invariant holds (the claim winner is the only
possible drainer at that instant). Conductor-decided under standing
instruction (2026-10-27); ratify at PR review.

## Q6. `chat_post` order — where does the turn-start budget gate land?

- A. `session_user → require_gate → owned_session → body validate →
  dedupe (transcript+queue arms; resend attach/snapshot short-circuits
  FREE of the budget check — a resend consumes nothing new) →
  `check_budget` → claim → residual drain → user-row insert → spawn`.
  Over-budget on a genuinely new send ⇒ 429 `budget_exceeded` (C1).
- B. Budget BEFORE dedupe — an over-budget user re-sending gets 429
  instead of their attach/snapshot; strands them retrying a send that
  already landed.
- C. Budget inside `run_turn` after spawn — the POST already answered;
  refusal would have to ride a terminal frame mid-stream, mixing a
  pre-send gate into the stream path and leaving the user row
  persisted for a send that "failed".
- D. Not decided.
- X. Other (please specify)

[Answer]: A — budget after dedupe, before claim; resends stay free.
Conductor-decided under standing instruction (2026-10-27); ratify at
PR review.

## Q7. Usage recording — what counts as an exchange?

- A. `record_usage` fires when a provider stream was OPENED and reached
  `Done{usage}` or `Error` mid-stream (partial consumption counts —
  tokens were spent): `requests + 1` per TURN (multi-round turns sum
  `usage` across rounds into `tokens` but count once — "exchange" =
  user send → terminal). A `stream()` open failure consumed nothing →
  not counted. A turn aborted before its first provider response →
  not counted; aborted mid-stream → counted.
- B. Count every provider ROUND — N-round tool turns inflate requests
  N×; the budget then conflates agentic depth with message count.
- C. Count only clean `Done`s — a user spamming failing requests burns
  provider spend unbudgeted.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — per-turn request count, per-round token accumulation,
counted on done or mid-stream error. Conductor-decided under standing
instruction (2026-10-27); ratify at PR review.

## Q8. Abort-on-delete — the C8 hook's mechanics?

- A. `OxpilotTurnAbortHook` (api-oxpilot) registered FIRST at boot
  (before U6's grant-revoke hook, per C8 ordering): on
  `on_session_delete(session_id)` → `turns().get` → if live, push
  `terminal{result:"aborted"}` into the turn's buffer + fanout (readers
  end their stream) then `turn.cancel.cancel()` and return `Ok(())` —
  no wait-for-exit. The turn task observes the token mid-loop, stops,
  its post-cascade `set_message_status` lands `error` or `not_found`
  (row already cascaded — benign); the map entry frees when the task
  unwinds. Idempotent: second call finds no live turn ⇒ `Ok`.
- B. Hook blocks until the turn task exits — delete waits on provider
  latency; a hung stream hangs the delete.
- C. Hook hard-drops the map entry (`release` from the hook) — the task
  keeps streaming into a dead fanout; readers see transport close, not
  the pinned `aborted` terminal.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — signal (terminal frame + cancel token), never wait;
release stays the turn task's own exit. Conductor-decided under
standing instruction (2026-10-27); ratify at PR review.

## Q9. Queued card — how does a `message_queue` insert surface on the stream?

- A. `message_queue` (proc) fetches the live turn via
  `runtime.turns().get(session_id)` (guaranteed present — BR4.1 already
  required it) and pushes `card{type:"card", card_kind:"queued",
  ref_id:queued_id}` into `emitted_deltas` + `fanout`, so attached
  readers AND late attachers (buffer replay) see the pending indicator;
  the C2 ref resolves via `oxpilot.queue_list`.
- B. No card — the next snapshot shows the row; but mid-stream readers
  get no pending feedback, missing AC1.8.2's "visible in the transcript
  as pending" until reconnect.
- C. A separate `queued` frame type — off-contract; C2 pins `card`
  with `card_kind: queued`.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — the proc emits the C2-pinned queued card into the live
turn's buffer+fanout at insert time. Conductor-decided under standing
instruction (2026-10-27); ratify at PR review.

## Q10. `ToolExecutor` seam (C6) — where does it live and how is it reached?

- A. `oxidean_api_core::oxpilot` beside `LlmProvider`/`SessionService`
  (all three are the oxpilot port surface the crates share). The slot
  gains `tools: Option<Arc<dyn ToolExecutor>>` — built at boot (None ⇒
  turn declares an empty catalog; a model that still emits a call gets
  a failed card + error result fed back); `provider_for_turn` widens to
  `turn_parts()` returning `(provider, model, tools)` so the pinned
  snapshot is taken once under one lock. U5 installs the real registry;
  U3 ships `NullTools` (empty catalog) as the standing default and the
  turn-loop code path that consumes it.
- B. api-oxpilot — the executor trait would sit in the crate that
  consumes it, but U5's impl lives in the same crate so either works;
  however C4/C5 already established api-core as the port crate (the
  slot itself is api-core; a slot field typed on an api-oxpilot trait
  would invert the dependency).
- C. On `RpcCtx` outside the slot — survives rebuilds but bypasses the
  snapshot-at-turn-start pinning ADR-010 requires of per-turn state.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — trait in api-core, slot field pinned per turn via a
widened `turn_parts()` snapshot, `NullTools` default until U5 installs.
Conductor-decided under standing instruction (2026-10-27); ratify at
PR review.

## Q11. Prompt assembly — what goes into `ChatRequest`?

- A. `system`: fixed preamble (Oxpilot identity, forge context, tool-use
  guidance incl. "propose diffs, never apply" + consent wording) + the
  session's `repo_owner/repo_name/branch` binding + goal. `messages`:
  `svc.transcript` (complete+error — BR2.3) windowed newest-first to a
  byte budget, then reversed; each assistant row's persisted tool-call
  rows append as bounded `[tool_call name → status]` summary lines so
  the model sees what it already did; the queued-batch rows drained at
  turn start are simply the window's tail. `tools`: the executor's
  `catalog()`. In-loop round messages (assistant `tool_calls_json` +
  `role:"tool"` results) extend the buffer between rounds.
- B. Full unwindowed transcript — unbounded prompt growth blows
  provider context windows and token budgets.
- C. Repo file-tree embedding in the system prompt — proactive context
  without the model asking; huge prompt tax when read tools exist for
  exactly this (FR7.1) — on-demand reads keep the base prompt small.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — bounded transcript window + tool-call summaries + binding
in system; catalog via executor; no proactive tree embed (tools pull
context on demand). Conductor-decided under standing instruction
(2026-10-27); ratify at PR review.

## Assumptions & Open Questions

- [assumption] `ChatMsg`'s extension is additive-safe for C4 consumers:
  the three U4 adapters + the stub are the only constructors/serializers
  and all are updated in the same change.
- [assumption] `MAX_TOOL_ROUNDS = 8`, per-call timeout `60 s`, turn wall
  cap `10 min`, transcript window budget `128 KiB` — concrete values
  pinned here, tunable in nfr-design without redesign.
- [Q1] Whether tool-call RESULTS replay into later turns' prompt window
  beyond the bounded summary lines (full `output_summary` replay raises
  token-cost questions) — resolved here as summaries-only; nfr-design
  may revisit under NFR3's token budget.

## Consolidated Summary Confirmation

The shape I intend to write into the functional-design artifacts for
`agent-runtime`:

- Scope: surgical replacement of `run_turn`'s interior (prompt
  assembly, tool rounds, usage, drain-chain) on the shipped plumbing;
  +3 edges — `chat_post` budget gate + residual drain, queued-card
  emission in `message_queue`, boot-registered abort hook (Q1).
- Wire: `ChatMsg` gains `tool_call_id`/`tool_calls_json` for native
  tool round-trips — a flagged C4 amendment (Q2).
- Turn: one assistant row per turn, tool rounds inside it, bounded at
  `MAX_TOOL_ROUNDS = 8` (Q3).
- Queue: end-of-turn atomic drain inside the task, batch-seeded chained
  turns, one drainer per the one-live-turn invariant (Q4, Q5).
- Gates: `chat_post` runs dedupe → `check_budget` → claim — resends
  attach free, new sends pay the 429 arm (Q6).
- Usage: `record_usage` per turn on done-or-mid-stream-error; tokens
  summed across rounds (Q7).
- Abort: C8 hook #1 signals terminal{aborted} + cancel, never waits
  (Q8).
- Cards: `message_queue` emits the C2 `card{queued,ref_id}` into the
  live turn's buffer+fanout (Q9).
- Seam: `ToolExecutor`/`ToolCtx`/`ToolCall`/`ToolOutput`/`ToolError` in
  api-core beside C4/C5; slot field pinned via `turn_parts()`;
  `NullTools` default until U5 installs (Q10).
- Prompt: system preamble + repo binding + bounded transcript window
  (tool-call summaries replayed) + executor catalog (Q11).

Does this all look correct before I write the functional-design
artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
