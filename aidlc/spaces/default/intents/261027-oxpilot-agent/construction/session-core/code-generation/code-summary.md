# Code Summary — session-core (U2)

Persistence + service layer for Oxpilot conversations: the
`0087_oxpilot_session_core` migration on all three dialects, the
dialect-dispatched store functions behind `Database`, the
`SessionService`/`SessionDeleteHook` trait seam in api-core, the
concrete `OxpilotSessionService` in api-oxpilot, five new
SessionOnly RPC procedures, route writes re-pointed through the
service, durable tool-call cards on the SSE path, dual-scope usage
counters with budget refusal, and the regenerated typed client.

## Data layer

- **`crates/oxidean-db`** — `0087_oxpilot_session_core.sql` x3
  dialects: `oxpilot_queued_messages` (`queue_seq` +
  `(session_id, client_msg_id)` dedupe key), `oxpilot_tool_calls`
  (`(message_id, call_id)` identity, `call_index` ordering),
  `oxpilot_proposals`, and `oxpilot_usage_counters`
  (`(scope, scope_key, window)` with the `__instance__` sentinel).
  Session-owned children FK-cascade; usage counters intentionally
  carry no session/user FK and survive both deletions.
- `src/oxpilot.rs` — row structs + store fns per dialect:
  session cascade delete, `next_message_seq` allocation,
  enqueue/list-queued/`drain_queue_tx` (atomic delete-and-append in
  delivery order, `RETURNING` sorted by `queue_seq`), tool-call
  insert/status, proposal insert/owned-read/`pending→applied`
  transition, usage upsert + dual-scope read.
- `src/lib.rs` — `Database` facade methods over the same set.
- `tests/dialect_oxpilot.rs` — 0087 assertions: table shape +
  unique keys, cascade-but-keep-usage, dual-scope upsert, and
  exactly-once ordered drain; `tests/dialect_actions.rs` reseat
  ledger updated for version 87; `docs/TESTING.md` db-dialects row
  names the 0087 proofs.

## API-core seam (`src/oxpilot.rs`)

- `trait SessionService` (21 verbs, `user_id` first arg,
  `Result<_, AppError>`, db row returns) + `trait SessionDeleteHook`
  (`on_session_delete(session_id, user_id)` — hooks run in
  registration order BEFORE the cascade; the first `Err` aborts).
- DTO set: `CreateSession`, `AppendMessage{id,status}`, `Enqueue`,
  `QueuedReceipt{Option,Option}`, `ToolCallWrite{seq,call_index}`,
  `ToolCallStatusUpdate`, `ProposalWrite`, `UsageDelta`,
  `BudgetCaps`, `UsageView`.
- `OxpilotRuntime.sessions: OnceLock<Arc<dyn SessionService>>` +
  `install_sessions` (set-once) + `sessions()` accessor (unset ⇒
  `oxpilot.internal`) — installed unconditionally in
  `oxidean-api/src/main.rs` after app-state construction and immune
  to settings rebuilds.
- `OxpilotRuntimeSlot.budget_caps` stamped on EVERY constructor path
  (boot env parse `OXIDEAN_OXPILOT_{USER,INSTANCE}_BUDGET`,
  disabled/carried/built slot helpers — `disabled_slot` and
  `carried_slot` signatures extended).
- `snapshot_frame(messages, pending_queue, tool_calls, active_turn)`
  merges durable tool cards into each message by `message_id`
  ordered by `call_index` and serializes `pending_queue` through the
  shared `queued_message_json` shape (same item shape as
  `oxpilot.queue_list`).

## Service (`oxidean-api-oxpilot/src/services.rs`)

- `OxpilotSessionService` over `oxidean_db::Database` +
  `Vec<Arc<dyn SessionDeleteHook>>` (`register_delete_hook`).
- Ownership: every read/write resolves `find_owned_session` →
  `not_found`, so foreign ids are indistinguishable from absent
  ones; all stored content runs `redact()` before persistence.
- Messages: `next_message_seq` allocation, transcript filter,
  `find_message_by_client_msg_id` dedupe, content/status updates.
- Queue: transcript-first/queue-second dedupe returning the
  `{null,null}` receipt, ordinal positions among live rows,
  `drain_queue` delegating to the atomic store tx
  (`debug{session_id, drained_count}` logged); every successful
  child write (`enqueue`, `set_message_*`, `append_tool_call`,
  `set_tool_call_status`, `save_proposal`, `mark_applied`) touches
  `last_activity_at` (BR2.4).
- Tool calls + proposals: durable card writes (message-membership
  verified — `not_found` when `message_id` belongs to a different
  session), existence-keyed status transitions (a SELECT gates
  `not_found` — MySQL `rows_affected` counts changed rows), owned
  proposal reads, `pending→applied` guard. `delete_session_cascade`
  carries the `AND user_id=?` belt-and-braces predicate.
- Usage: `record_usage` upserts user + instance scopes for the UTC
  day window; `check_budget` refuses at/over cap on EITHER axis
  (user scope reported first, `budget_exceeded` + `data.scope`);
  `usage_view` returns the C1 five-field payload and never refuses
  under breach.

## Procedures, routes, shell

- `dispatch.rs`/`session.rs` — `oxpilot.session_get`,
  `oxpilot.session_delete`, `oxpilot.message_queue`,
  `oxpilot.queue_list`, `oxpilot.usage_get` added to the closed
  `PROCEDURES` set + dispatch arms; all SessionOnly (PAT ⇒
  `auth.pat_scope`, anonymous ⇒ `auth.unauthenticated`), all run
  `require_enabled` before session/store work.
- `message_queue` orders BR4.1-pinned — owned session FIRST
  (`not_found` for foreign/absent alike), then live-turn
  (`session_active_turn_absent`), then `check_budget`, then the
  dedupe+insert inside `enqueue`.
- `session_delete` audits every resolved outcome —
  `result: ok | hook_failed (hook_index) | not_found` —
  identifiers and result metadata only. Hook failures propagate the
  hook's own `code`/`message` with `hook_index` MERGED into `data`
  (never rewritten); the proc reads `data.hook_index` presence as
  the hook_failed signal.
- `routes.rs` — chat/turn writes route through the service
  (`find_message_by_client_msg_id`, `append_message`,
  `set_message_content`, `set_message_status`,
  `append_tool_call`); `chat_post` runs BOTH halves of the union
  dedupe — a transcript hit attaches/answers the snapshot, a
  queued-only hit answers `409 turn_active` (a duplicate
  `(session_id, client_msg_id)` transcript row would poison every
  later `drain_queue_tx` commit on the unique index). All snapshot
  inputs ride the port: `list_messages` (unfiltered — the
  `streaming` assistant row seeds remounts), `list_queue`,
  `list_tool_calls`; `owned_session` delegates to
  `svc.get_session`. `ChatStreamItem::ToolCall` persists a redacted
  durable card and emits the `tool_call` frame with a per-message
  `call_index`.
- `main.rs` — `install_sessions(OxpilotSessionService::new(db))`
  after app-state construction; the `OnceLock` field is private —
  `install_sessions`/`sessions()` are the only access paths.

## Tests

- api-core colocated: 7 new `oxpilot::tests` (seam set-once,
  unset-error, budget-cap stamping, snapshot merge).
- api-oxpilot: 78 green — service tests cover CRUD/foreign
  `not_found`, redaction, seq ordering, dedupe arms, ordinal
  positions, concurrent insert, exactly-once drain, tool ordering,
  proposal guard, usage/budget ordering, hook order + abort.
- api: `tests/oxpilot.rs` +8 — auth matrix (anon/PAT/flag-off) for
  all five procs, receipt shapes incl `{null,null}`, queue ordering,
  usage payload + recorded values, budget_exceeded scope, delete
  audit rows + `hook_failed` detail, PROCEDURES pin.

## Verification

- `cargo nextest run -p oxidean-api-oxpilot` — 78/78
- `cargo nextest run -p oxidean-api --test oxpilot` — 18/18
- `cargo nextest run -p oxidean-api-core -E 'test(oxpilot)'` — 7/7
- `DATABASE_URL=sqlite:var/… OXIDEAN_DB_DIALECT=sqlite cargo nextest
  run -p oxidean-db --test dialect_oxpilot --test-threads=1` — 16/16
- `cargo nextest run --workspace` — 2123/2123 (3 skipped)
- `cargo nextest run -p oxidean-vcs --features server` — 216/216
- `bun run test` — 3 tasks green (web 708)
- `make rpc-gen` — regenerated; deterministic on re-run

Postgres/MySQL dialect legs and `rpc-sync-check`'s committed-diff
gate are CI surfaces — no local containers, and the generated client
diff is intentionally left uncommitted per instruction.
