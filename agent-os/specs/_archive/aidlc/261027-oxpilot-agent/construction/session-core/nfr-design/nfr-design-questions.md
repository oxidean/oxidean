# NFR Design Questions — session-core

Unit: `session-core` (U2). NFR requirements approved at iteration 2
(`nfr-requirements/`); this stage pins the concrete mechanisms.

## Q1. `SessionService` trait shape?

- A. `oxidean_api_core::oxpilot::SessionService` — async trait with
  exactly C5's verbs: `create_session`, `get_session`, `list_sessions`,
  `delete_session`, `append_message`, `append_tool_call`,
  `transcript`, `enqueue`, `drain_queue`, `list_queue`, `record_usage`,
  `check_budget`, `save_proposal`, `proposal`, `mark_applied`. Every
  method's first arg is `caller_user_id: &str` (scoping can't be
  omitted); returns use the shipped shared `AppError`
  (`code`/`message`/`data`) so `not_found`/`budget_exceeded` map to C1
  codes at the proc layer.
- B. Split trait per concern (MessageStore + QueueStore + BudgetStore)
  — more surface, no consumer asks for it; C5 is one service.
- C. Concrete type only — breaks the trait-port layering (BR7.1) that
  lets U3/U6 depend on api-core without importing api-oxpilot.
- X. Other (please specify)

[Answer]: A — one trait in `api_core::oxpilot` covering C5's verbs plus the reads/writes the seams need (`find_message_by_client_msg_id` for dedupe, `set_message_content`/`set_message_status` for the assistant streaming-row lifecycle, `list_tool_calls` for the snapshot merge, `set_tool_call_status` per BR7.3, `mark_applied(id, pr_ref)` carrying the PR link); every method takes `caller_user_id`; the error type is the shipped shared `AppError` (`code`/`message`/`data` — C1 vocabulary on `code`, `budget_exceeded` carries `{scope}` in `data`), and the trait's DTO set (`CreateSession`, `AppendMessage` with caller-minted `id`+`status`, `Enqueue`, `QueuedReceipt` — `enqueue`'s `{queued_id, position}` return, `ToolCallWrite` with caller-supplied `seq`+`call_index`, `ToolCallStatusUpdate`, `ProposalWrite`, `UsageDelta`, `BudgetCaps`, `UsageView`) is declared in the same `oxpilot` module. `check_budget` takes `caps: BudgetCaps` — budgets live on `EffectiveProviderConfig`, which the slot never stored, so the slot gains a `budget_caps` field populated at rebuild and callers pass the slot-read value in (review R-01). Conductor-decided (2026-10-27); ratify at PR review.

## Q2. `SessionDeleteHook` registry mechanics?

- A. `Vec<Arc<dyn SessionDeleteHook>>` built at shell construction and
  held as a field of the service (the runtime carries the service Arc
  via `OnceLock`); `delete_session`
  iterates the vec in order before the DB delete (BR5.1/BR5.2). U3
  inserts its abort hook at index 0, U6 appends revoke at index 1 —
  order is construction-order, no priority field. U2 ships
  `hooks: vec![]` plus a `#[cfg(test)]` probe hook for the abort
  regression (NFR10.10).
- B. Registry map keyed by unit name — adds lookup complexity for a
  two-entry ordered list.
- C. Priority field per hook — over-spec for two registrars.
- X. Other (please specify)

[Answer]: A — ordered `Vec<Arc<dyn SessionDeleteHook>>` as a field of `OxpilotSessionService` (constructed once at shell build; the RUNTIME carries `OnceLock<Arc<dyn SessionService>>` via `install_sessions`, reached by procs through the `sessions()` accessor — unset ⇒ `oxpilot.internal`), construction-order = run-order; U2 ships empty + a cfg(test) probe. Conductor-decided (2026-10-27); ratify at PR review. Refined at iter-1 (R-10): the vec lives ON the service, not beside it in the slot; refined again (R-15): the service Arc lives on the runtime, not the slot.

## Q3. Atomic `drain_queue` per dialect?

- A. One `oxidean-db` fn `drain_queue_tx` that runs a single
  transaction: Postgres `DELETE … WHERE session_id=$1 RETURNING *`
  (single statement, atomic — no ORDER BY on DELETE; the service sorts
  the RETURNING rows by `queue_seq` in memory before appending); MySQL
  `SELECT … FOR UPDATE ORDER BY queue_seq` + `DELETE` + inserts inside
  `sqlx::Transaction`; SQLite `DELETE … RETURNING` inside
  `sqlx::Transaction` (sqlite RETURNING is supported ≥3.35 — repo ships
  a modern sqlx sqlite). The drained rows append to `oxpilot_messages`
  minting `seq` and carrying `client_msg_id` INSIDE the same
  transaction via a store helper taking `&mut Transaction`.
- B. Two-phase: claim-by-update then delete — leaves a `claimed`
  status column to GC; unnecessary when single-tx is expressible on
  all three dialects.
- C. Application-side: read then delete in separate statements — the
  crash window NFR4.2 exists to close.
- X. Other (please specify)

[Answer]: A — one store-level tx per dialect (pg/mysql/sqlite DELETE…RETURNING or FOR UPDATE variant); drain→append lands in the same tx via a `&mut Transaction` helper. Conductor-decided (2026-10-27); ratify at PR review.

## Q4. Union-dedupe lookup order?

- A. `enqueue`/chat-POST dedupe runs `find_message_by_client_msg_id` FIRST
  (transcript wins — matches `resend_matches` re-attach semantics),
  then `find_queued_by_cmid`; insert only when both miss. A UNIQUE
  violation on either insert triggers the bounded retry/re-read
  (NFR4.4, NFR4.12) rather than a 500.
- B. Queue-first — would re-attach a queued row that already drained
  to messages between the queue read and the resend, inverting
  BR2.2's arm order.
- X. Other (please specify)

[Answer]: A — transcript read first, then queue; unique-key collision → bounded retry/re-read, idempotent return. Conductor-decided (2026-10-27); ratify at PR review.

## Q5. Redaction application?

- A. `OxpilotSessionService` redacts at the top of every body-bearing
  method (private `fn redact_in_place(&mut WritePayload)`) before
  calling the store — one list of fields matching BR3.1 exactly:
  message/queued `content`, tool_call `args_summary`/`output_summary`,
  proposal `title`/`diff_body`, session `goal`/`title`. The service
  never sees the store's raw insert for these fields — the redacted
  value is what crosses the boundary.
- B. Redact inside `oxidean-db` — wrong crate: redact() lives in
  oxidean-core and the redaction boundary is the service (BR3.1),
  not the dialect layer.
- X. Other (please specify)

[Answer]: A — service-top redaction of exactly BR3.1's field set before every store call. Conductor-decided (2026-10-27); ratify at PR review.

## Q6. `check_budget` mechanics?

- A. One read of both counter rows (`scope='user', scope_key=user_id`
  and `scope='instance', scope_key='__instance__'`, `window`=today UTC)
  via a single store fn `read_counters` returning the pair; compare
  both axes against `BudgetCaps` passed by the caller (the slot gains a
  `budget_caps` field — `resolved` is not a slot field)
  (provider-agnostic — budget lives on the slot, not the adapter);
  `BudgetExceeded{scope}` on the first breach (user scope reported
  before instance on double-breach — deterministic).
- B. Two separate reads — doubles the round trip for no benefit.
- C. Push the comparison into SQL — dialect-specific predicate logic
  crosses BR7.1's layering.
- X. Other (please specify)

[Answer]: A — one `read_counters` call returning the user+instance pair; the caller passes `caps: BudgetCaps` read from the slot's new `budget_caps` field (populated at rebuild from `EffectiveProviderConfig.user_budget`/`instance_budget`, default `(None,None)` = unbounded); deterministic breach order (user before instance). Conductor-decided (2026-10-27); ratify at PR review. Refined at iter-1 (R-01): the slot has no `resolved` field — budgets live on the U4 EffectiveProviderConfig that was never stored; `budget_caps` is the U2 slot-field addition.

## Q7. Proc wiring for the +5?

- A. PROCEDURES gains `oxpilot.session_get`, `oxpilot.session_delete`,
  `oxpilot.message_queue`, `oxpilot.queue_list`, `oxpilot.usage_get` —
  all SessionOnly, all enablement-gated via `require_enabled(ctx)` at
  each handler top (identical to `session_create`/`session_list`,
  session.rs:58/:139). Handlers are thin: bind
  caller→`ctx.user_id`, parse input, unwrap `ctx.oxpilot`
  (`Option<SharedOxpilot>`) then call `.sessions()?.*` (the runtime's
  named accessor over the `OnceLock`-installed service),
  map typed errors to C1 codes.
- B. Per-handler gate calls — duplicates what resolve_slot already
  centralizes in U1.
- X. Other (please specify)

[Answer]: A — five procs on PROCEDURES/dispatch, SessionOnly, same enablement-gate path as U1 procs; thin handler + typed-error map; handlers reach the service via `ctx.oxpilot.sessions()?` (the named accessor — unset ⇒ `oxpilot.internal`). Conductor-decided (2026-10-27); ratify at PR review.

## Q8. Migration shape for `0087_oxpilot_session_core`?

- A. Four tables × 3 dialects (postgres/mysql/sqlite files):
  `oxpilot_tool_calls` {id pk, session_id→cascade, message_id,
  call_id, seq (positional copy of the owning message's seq),
  call_index, tool, args_summary, output_summary, status,
  created_at; UNIQUE(session_id, call_id) per BR7.3},
  `oxpilot_queued_messages` {id pk, session_id→cascade, client_msg_id
  UNIQUE-per-session, content, queue_seq, created_at;
  UNIQUE(session_id,queue_seq), INDEX(session_id)},
  `oxpilot_proposals` {id pk, session_id→cascade, title, base,
  diff_body, status pending|applied, pr_ref nullable, created_at},
  `oxpilot_usage_counters` {id pk, scope, scope_key, window,
  requests, tokens, updated_at; UNIQUE(scope,scope_key,window)} —
  types per dialect (TEXT/VARCHAR(191)/TEXT keys, BIGINT counts,
  dialect timestamp types matching U1's 0085); files land under
  `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/` (the in-crate
  migrations tree, not the repo root).
- B. JSON blob table — defeats dialect indexing/uniqueness.
- X. Other (please specify)

[Answer]: A — four concrete tables ×3 dialects with the uniques/indexes listed; types mirror 0085 conventions. Conductor-decided (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- One `SessionService` trait in api-core covering C5 (Q1).
- Ordered hook vec as a field of the service, construction-order, U2
  ships empty + test probe (Q2, iter-1 refined).
- Per-dialect single-tx drain via DELETE…RETURNING / FOR UPDATE (Q3).
- Dedupe order: transcript first, then queue; collision ⇒ idempotent
  return (Q4).
- Service-top redaction of BR3.1's exact field set (Q5).
- `check_budget`: one `read_counters` pair read, compare to the
  caller-passed `BudgetCaps` (slot `budget_caps` field), user-scope
  breach reported first (Q6, iter-1/2 refined: caps are a param — the
  service never reads its own containing slot).
- Five new SessionOnly gated procs on the existing dispatch/gate path
  (Q7).
- `0087_oxpilot_session_core` four tables ×3 dialects (Q8).

Does this all look correct before I write the NFR-design artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
