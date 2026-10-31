# Functional Design Questions — session-core

Unit: `session-core` (U2, kind `service`) — owns `SessionService`: full
session/message/tool_call/queued/proposeddiff/usage_counter persistence,
per-user scoping enforcement, delete cascade with the `on_session_delete`
registration seam (C8), secrets-redaction on write, budget counters +
over-budget refusal, and transcript history reads for prompt assembly.
depends_on: U1 (crate + `oxpilot_sessions`/`oxpilot_messages` tables exist).

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U2.
- [contract] `inception/contract-design/contract-summary.md` — C5
  (`SessionService` API surface + scoping/redaction rules), C8
  (`on_session_delete` hook ordering + failure semantics).
- [req] FR1.2 (user-scoped list), FR1.7 (delete cascade), FR2.4
  (server-side persistence), FR5.6 (revocation integration via hook),
  FR6.6 (persisted entity set), FR6.7 (budget counters), FR7.x
  (tool_call + proposal rows the tools write through); NFR1 (secrets
  redaction), NFR3.x (session/message integrity), US5.3 AC5.3.3
  (stored content redacted on write).
- [stories] US1.5 (AC1.5.2 cascade incl. per-session grants), US1.8
  (AC1.8.1 queue, AC1.8.2 pending visibility), US5.2 (AC5.2.1/AC5.2.2
  budget counters + refusal; config half is U4's).
- [shipped] `crates/oxidean-db/src/oxpilot.rs` (U1 store: insert/find/
  list session, insert/find/list message, seq minting, sweep),
  `crates/oxidean-db/migrations/*/0085_oxpilot.sql` (sessions +
  messages tables), `crates/oxidean-core/src/redact.rs` (U1 `redact()`).

## Q1. Entity set — which persisted entities does U2 own?

- A. Four new entities layered on the two U1 tables: `OxpilotToolCall`
  (per-session tool invocation record: tool name, args summary, status,
  output summary, seq-positioned), `OxpilotQueuedMessage` (pending
  follow-up queue: session, seq, content, queued_at), `OxpilotProposal`
  (proposed diff: session, title, unified-diff body, status
  pending|applied, target PR link on materialize), `OxpilotUsageCounter`
  (per-user + per-instance token/message rollup keyed by scope+window).
  Plus the U1-owned `OxpilotSession`/`OxpilotMessage` gain the fields
  U2 needs (none — U1's schema already carries `status`,
  `last_activity_at`, `client_msg_id`, `seq`, `role`, `content`;
  cascade on session/user delete is declared at the FK level).
- B. One row per table is wrong — merge tool calls into messages:
  loses the tool-call semantics (status transitions, summarized args)
  the SSE `tool` frame and AC1.3.6 cards need.
- C. Single JSON blob per session — kills per-row indexing, queue
  drain ordering, and cascade integrity.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — 4 new entities (`OxpilotToolCall`, `OxpilotQueuedMessage`,
`OxpilotProposal`, `OxpilotUsageCounter`) on the 2 existing U1 tables
(`OxpilotSession`, `OxpilotMessage`); no schema change needed on the U1
pair. Conductor-decided under standing instruction (2026-10-27); ratify
at PR review.

## Q2. Where does `SessionService` live?

- A. A `trait SessionService` port declared in `oxidean-api-core` beside
  `LlmProvider`, with the concrete `OxpilotSessionService` in
  `oxidean-api-oxpilot::services` delegating to `oxidean-db` store
  functions. The slot/RpcCtx holds `Arc<dyn SessionService>` — a trait
  object, never the concrete type — so api-core stays oxpilot-agnostic
  and no crate cycle forms. The `SessionDeleteHook` registry is
  injected at shell construction.
- B. Concrete `SessionService` type on the api-core slot — creates an
  api-core→api-oxpilot dependency while api-oxpilot already depends on
  api-core: a crate cycle (review R-03).
- C. In `oxidean-db` — conflates storage mechanics with workflow
  semantics (hook ordering, redaction policy, budget refusal).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — trait port in api-core (`Arc<dyn SessionService>` on the slot, LlmProvider precedent), concrete `OxpilotSessionService` in api-oxpilot; delete-hook registry injected at shell construction. Corrected at review-1 (R-03). Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Delete-cascade ordering and hook failure semantics?

- A. Per C8 exactly: `SessionService.delete_session` calls registered
  `on_session_delete` hooks in registration order **before** the DB
  delete; any hook error aborts the delete and surfaces to the caller;
  hooks are documented idempotent. U2 ships zero registrars — U3
  (abort-turns) and U6 (revoke-grants) inject later; the seam exists
  and is regression-tested via a probe hook. The DB delete itself is a
  single `DELETE` on `oxpilot_sessions` relying on the declared
  `ON DELETE CASCADE` chain for all child rows.
- B. Delete first, then hooks — leaves live turns pointing at a deleted
  session; C8 explicitly places hooks BEFORE cascade.
- C. Best-effort hooks — a failed revoke leaves orphaned grants; C8 says
  hook failure aborts the delete.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — hooks run in registration order before the single
`DELETE`, failure aborts, U2 ships the seam with zero registrars
(probe-hook regression test). Conductor-decided under standing
instruction (2026-10-27); ratify at PR review.

## Q4. Queue semantics (US1.8)?

- A. Dedicated `oxpilot_queued_messages` table with per-session
  monotonic `queue_seq`; the chat POST keeps its contract-pinned
  409 `turn_active` answer and the CLIENT drives the explicit
  `oxpilot.message_queue` proc to enqueue (C1/C2 pin). `drain_queue`
  is a single-transaction claim+delete+append-into-transcript
  (DELETE…RETURNING + message INSERTs minting seq, carrying
  `client_msg_id`) — exactly-once with no committed-delete loss
  window. `oxpilot.queue_list` is the read-only pending list for the
  UI chip and the C2 queued-card ref (AC1.8.2). Enqueue is
  caller-scoped, budget-checked, union-deduped by `client_msg_id`
  across both tables, and redacted on write.
- B. Auto-enqueue inside the chat POST — contract-forbidden (C2 pins
  409 turn_active; auto-enqueue and silent accept are both barred);
  also creates a resend/attach ambiguity.
- C. Queue-as-flags on `oxpilot_messages` with `status='pending'` —
  conflates transcript order with delivery order and breaks
  UNIQUE(session_id,seq) (queued rows have no transcript position
  until drained).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — dedicated queued table + contract-pinned `message_queue`/`queue_list` procs, atomic drain-into-transcript carrying `client_msg_id`. Corrected at review-1 (R-01/R-02/R-07/R-09). Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. Usage counters and over-budget refusal (US5.2)?

- A. `OxpilotUsageCounter` rows keyed by `(scope, scope_key, window)`
  where scope ∈ {user, instance}, scope_key = user_id or the literal
  `__instance__`, window = **UTC** ISO day (`YYYY-MM-DD`, UTC-midnight
  rollover). Two counter axes per row: `requests` (always +1 per
  completed turn exchange) and `tokens` (+= when the provider reports
  `Done{usage}`). One configured budget value V — `user_budget` /
  `instance_budget` — caps BOTH axes (`requests_cap=V`,
  `tokens_cap=V` in `oxpilot.usage_get` terms); `check_budget` refuses
  `BudgetExceeded` when either axis is at/over, via the typed
  `budget_exceeded`/429 vocabulary (US5.2.2). Absent budget ⇒
  unbounded + `cap: null` — the generous default of AC5.2.1.
- B. Count on read (aggregate over messages each call) — needs a
  per-message tokens column and scales badly; the counter table is
  O(2 rows) per window.
- C. Provider-reported usage only — `Done{usage}` isn't guaranteed
  (Ollama reports none); the refusal must work for stub+ollama too.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — UTC-day counters keyed (scope, key, window); requests + tokens axes both capped by the single configured V; `check_budget` → `BudgetExceeded`; absent ⇒ unbounded. Corrected at review-1 (R-06). Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q6. Redaction-on-write boundary?

- A. Every body-bearing write (`append_message`, `enqueue`,
  `save_proposal`, `append_tool_call` summary fields) runs the text
  through `oxidean_core::redact()` before the store call — the same
  detector U1 uses in the SSE path (AC5.3.3, NFR1). Read paths return
  stored (redacted) text untouched. The service never sees an
  unredacted-then-redacted pair; redaction is at the service boundary,
  not the store layer, so the rule travels with the service.
- B. Redact at the proc layer only — C5 pins the service as the
  enforcement point ("nothing leaves this service unredacted"); proc-
  level-only leaves a bypass via direct service callers (tools, U3).
- C. Redact on read — stored plaintext violates NFR1 at-rest intent and
  AC5.3.3's "on write" wording.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `redact()` at the service boundary on every body-bearing
write (`append_message`, `enqueue`, `save_proposal`, tool-call summary
fields). Conductor-decided under standing instruction (2026-10-27);
ratify at PR review.

## Q7. Per-user scoping enforcement (anti-enumeration)?

- A. Every service method takes `user_id`; row lookups join/filter
  through `sessions.user_id` and resolve cross-user or missing rows to
  `not_found` (never `forbidden` — same vocabulary U1's
  `find_owned_session` already ships). This is a deliberate refinement
  of C5's literal "forbidden" wording: the shipped store resolves
  cross-user to `not_found` to kill the 403/404 existence oracle —
  contract-text reconciliation (denied ⇒ not_found) is requested.
- B. Literal `forbidden` per C5 — reintroduces the existence oracle the
  shipped store deliberately avoided.
- C. Rely on the proc layer only — direct service callers (U3 turn
  loop, U5 tools) would bypass scoping; C5 pins enforcement at the
  service layer.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — every method caller-scoped by `user_id`; cross-user and missing both resolve `not_found` (shipped `find_owned_session` precedent; deliberate C5 refinement, reconciliation requested). Corrected at review-1 (R-11). Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q8. New `oxpilot.*` procs for the service?

- A. The U1 proc set (`status`, `session_create`, `session_list`) +
  SSE routes stay; U2 adds the C1-pinned five: `oxpilot.session_get`,
  `oxpilot.session_delete` (cascade entry point for US1.5),
  `oxpilot.message_queue` (client-driven enqueue after the POST's
  409 turn_active), `oxpilot.queue_list` (pending list for the UI chip
  + C2 queued-card ref), and `oxpilot.usage_get` (per-user budget
  view). All SessionOnly-gated AND enablement-gated (`oxpilot.disabled`
  first). No `oxpilot.transcript` — remount flows through the C2 SSE
  snapshot, which already carries Message + ToolCall rows +
  pending_queue.
- B. Every service method gets a proc — internal calls (drain,
  record_usage, transcript) have no user-facing RPC contract; exposing
  them widens attack surface.
- C. No new procs — AC1.5.x, AC1.8.x and the usage view need server
  endpoints.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — +5 contract-pinned procs (`session_get`, `session_delete`, `message_queue`, `queue_list`, `usage_get`); no transcript proc — remount is the C2 snapshot's job. Corrected at review-1 (R-01/R-02/R-10/R-13). Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q9. Migration layout and dialect coverage?

- A. One migration `0087_oxpilot_session_core.sql` per dialect
  (sqlite/postgres/mysql) creating the four new tables with their
  indexes, plus the `dialect_oxpilot` test extending to the new DDL
  — same tri-dialect + `-E` filter pattern U1/U4 use.
- B. Per-table migrations — four files per dialect for one logical
  unit is fragmentation without benefit; U1's single-file pattern
  already binds a unit's tables.
- C. Extend 0085 — migrations are append-only once committed; editing
  a released migration breaks checksums.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `0087_oxpilot_session_core.sql` ×3 dialects, four tables
+ indexes in one file, `dialect_oxpilot` test extended. Conductor-
decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Entities: 4 new (`OxpilotToolCall` keyed (message_id, call_id),
  `OxpilotQueuedMessage`, `OxpilotProposal`, `OxpilotUsageCounter`
  requests+tokens axes) on the 2 U1 tables (Q1).
- Service: `trait SessionService` port in api-core; concrete
  `OxpilotSessionService` in api-oxpilot; hook registry injected at
  shell build — no crate cycle (Q2).
- Delete: C8 hooks run in registration order before the single cascade
  `DELETE`; failure aborts; U2 ships zero registrars + probe test (Q3).
- Queue: contract-pinned — POST keeps 409 `turn_active`; client calls
  `oxpilot.message_queue`; `drain_queue` is an atomic
  claim+delete+append carrying `client_msg_id` (Q4).
- Budgets: UTC-day (scope, key) counters; configured V caps requests
  AND tokens; `check_budget` → `BudgetExceeded`; absent ⇒ unbounded
  (Q5).
- Redaction: `redact()` at the service boundary on every body-bearing
  write incl. proposal title + tool summaries (Q6).
- Scoping: `user_id` on every call; cross-user/missing ⇒ `not_found`
  (deliberate C5 refinement, reconciliation requested) (Q7).
- API: +5 contract-pinned procs (`session_get`, `session_delete`,
  `message_queue`, `queue_list`, `usage_get`), SessionOnly +
  enablement-gated; no `transcript` proc — remount is the SSE
  snapshot's job (Q8).
- Migrations: `0087_oxpilot_session_core.sql` ×3 dialects + dialect test
  (Q9).

Does this all look correct before I write the functional-design
artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
