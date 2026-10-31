# Logical Components — session-core (U2)

Consumes: `functional-design/{entities,rules,functional-spec}.md`,
`nfr-requirements/*`, `nfr-design-questions.md` (Q1–Q8),
`codekb/oxidean-app/technology-stack.md`.

## Component catalogue

| component | crate | kind | owns |
|-----------|-------|------|------|
| `trait SessionService` | `oxidean-api-core::oxpilot` | trait port | C5 verbs + the seam reads/writes (`find_message_by_client_msg_id`, `set_message_content`, `set_message_status`, `list_tool_calls`, `set_tool_call_status`, `usage_view`); every method takes `user_id`; `Result<_, AppError>`; read returns are `oxidean_db::oxpilot` row structs (api-core already deps oxidean-db, Cargo.toml:13); `enqueue` returns the `QueuedReceipt` DTO — namespaced `oxpilot::SessionService`, DISTINCT from `auth::session::SessionService` (NFR10.4) |
| `trait SessionDeleteHook` | `oxidean-api-core::oxpilot` | trait port | C8 `on_session_delete(session_id, user_id) -> Result<(), AppError>` |
| trait input DTOs | `oxidean-api-core::oxpilot` | types | `CreateSession`, `AppendMessage` (caller-minted `id`+`status` — assistant rows insert as `streaming` with `turn.message_id`), `Enqueue`, `QueuedReceipt` (`{queued_id, position}` — enqueue's return, position is the ordinal not queue_seq), `ToolCallWrite` (caller-supplied `seq` copied from the owning message + caller-minted `call_index`), `ToolCallStatusUpdate`, `ProposalWrite`, `UsageDelta`, `BudgetCaps`, `UsageView` — declared beside the trait; other return types are oxidean-db row structs, not DTOs |
| `OxpilotSessionService` | `oxidean-api-oxpilot::services` | concrete service | every store call, redaction, dedupe, budget compare; fields `db: oxidean_db::Database` + `hooks: Vec<Arc<dyn SessionDeleteHook>>` (hooks live ON the service — R-10); installed on `OxpilotRuntime` via `OnceLock` post-construction (`install_sessions`) — a non-Option slot field would force api-core's boot constructor to name this api-oxpilot type (forbidden edge, R-15) |
| store helpers | `oxidean-db::oxpilot` (extend) | persistence | NEW fns: `delete_session_cascade`, `next_queue_seq`, `enqueue_queued` (bounded queue_seq retry), `find_queued_by_cmid`, `list_queued_for_session` (queue_list + snapshot pending_queue source), `drain_queue_tx`, `insert_tool_call`, `list_tool_calls_for_session`, `set_tool_call_status`, `insert_proposal`, `find_proposal`, `mark_proposal_applied`, `upsert_usage`, `read_counters`; REUSED untouched: `find_owned_session`, `insert_session`, `list_sessions_for_user`, `touch_session_activity`, `find_message_by_client_msg_id` (no status filter — returns `streaming` rows, which attach requires), `find_message_by_id` (row-membership check for `set_message_content`/`set_message_status`/`append_tool_call` — the store updates are id-keyed, so the service verifies `row.session_id == session_id` before writing), `next_message_seq`, `insert_message`, `list_messages`, `set_message_content`, `set_message_status`, `sweep_streaming_messages` |
| proc handlers | `oxidean-api-oxpilot::dispatch` | rpc boundary | 5 new procs on PROCEDURES + dispatch; thin input-parse → service call → `AppError` code map; `session_delete` handler also writes the audit row (`audit::record` needs `RpcCtx` — proc layer owns it, not the service) |
| `snapshot_frame` extension | `oxidean-api-core::oxpilot` | shared frame builder | signature gains `pending_queue` + `tool_calls` params — today it hardcodes `pending_queue: []` (api-core:499) and never merges tool cards (R-05) |
| `budget_caps` slot field | `oxidean-api-core::oxpilot` | slot field | `OxpilotRuntimeSlot.budget_caps: BudgetCaps` — populated at rebuild from `EffectiveProviderConfig.user_budget`/`instance_budget` (R-01) |
| migration 0087 | `crates/oxidean-db/migrations/{postgres,mysql,sqlite}` | ddl | four tables (NFR10.1) — the in-crate migrations tree, not repo root |

## Dependency shape

```
apps/web ──RPC──▶ api-oxpilot::dispatch ──▶ OxpilotSessionService
                                               │ redact() (oxidean-core)
                                               ▼
                                         oxidean-db::oxpilot ──▶ 0087 tables
api-core::oxpilot::{SessionService, SessionDeleteHook, BudgetCaps, DTOs}
        ◀── impl'd/consumed by api-oxpilot
RpcCtx.oxpilot: OxpilotRuntime { slot, rebuild_lock, sessions } —
  `sessions: OnceLock<Arc<dyn SessionService>>` lives ON THE RUNTIME
  (not the slot) — api-core's boot constructor can't name the concrete
  impl; the shell installs it once post-construction (R-15).
  Slot gains `budget_caps: BudgetCaps` — populated per-constructor
  from cfg/env (per-site pin in security-design; carried_slot gains a
  cfg param so a budget-only write never keeps stale caps).
```

One-way edges only: `api-oxpilot → api-core → oxidean-core`,
`api-oxpilot → oxidean-db`. No api-core → api-oxpilot edge (the U4
R-01-class bug — traits live downstream of their impls' dependents).

## Ownership notes

- `OxpilotSessionService` holds `db` + `hooks`; it lives on the
  RUNTIME (`OnceLock`), not the slot — rebuilds never touch it; the
  hooks vec is built at shell construction (Q2) and travels inside
  the service Arc.
- The service NEVER owns `ActiveTurns` — turn claim/release is U3's;
  `message_queue`'s live-turn check reads turn state through the
  slot accessor the dispatch handler already reaches.
- `resend_matches` stays a PURE comparator in `routes.rs` (it compares
  turn state + a prior row's seq — no DB inside it, R-04). What
  changes is its INPUT: the route's dedupe read
  `find_message_by_client_msg_id` moves onto the service so the
  union-dedupe rule lives in one place; that read carries NO status
  filter — a `streaming` row MUST be findable or re-attach can never
  happen (attach keys on `prior.seq`).
- `SessionDeleteHook` impls are owned by U3 (`AbortTurnsHook`) and U6
  (`RevokeGrantsHook`) — NOT this unit. U2's `#[cfg(test)]`
  `ProbeHook` is a test-module struct (NFR10.10).

## U1 seam deltas (explicit)

| seam | U1 state | U2 change |
|------|----------|-----------|
| `OxpilotRuntimeSlot` | `{provider, model, disabled_reason, turns, built_from}` | + `budget_caps: BudgetCaps` populated per-constructor (boot parses budget envs; resolve_slot/built_slot/carried_slot pass `BudgetCaps::from(cfg)` — the `From<&EffectiveProviderConfig>` impl lives in api-oxpilot; `disabled_slot` gains a `caps: BudgetCaps` param — api-core can't name `EffectiveProviderConfig`, standalone callers pass `BudgetCaps::default()`; carried_slot's signature gains `&EffectiveProviderConfig`) — R-15 |
| `OxpilotRuntime` | `{slot, rebuild_lock}` | + `sessions: OnceLock<Arc<dyn SessionService>>` + `install_sessions` setter + `sessions()` accessor (`Result<Arc<…>, AppError>` — unset ⇒ `oxpilot.internal`) — the shell installs `OxpilotSessionService` post-construction (api-core can't name it); procs reach the service via `ctx.oxpilot.sessions()?` |
| dedupe read | `routes.rs` calls `db::oxpilot::find_message_by_client_msg_id` | routes via `SessionService::find_message_by_client_msg_id` — same store fn, now behind the service boundary so redaction/scoping rules apply uniformly; `resend_matches` stays pure |
| `snapshot_frame` | `snapshot_frame(messages, active_turn)` — `pending_queue` hardcoded `[]`, no tool_calls | `snapshot_frame(messages, active_turn, pending_queue, tool_calls)` — the ROUTE gathers the two reads through the service (`list_queue`, `list_tool_calls`); wire shape: `pending_queue` = queued row JSON ordered by queue_seq; each message object gains `tool_calls: [{call_id, call_index, tool, status, args_summary, output_summary}]` ordered by call_index, attached by `message_id` (R-05/R-21, BR7.3, WF4); every `snapshot_frame` call site in routes.rs updates |
| chat POST | inserts message + redacts inline; turn task calls `set_message_content`/`set_message_status`/`insert_tool_call` directly | ALL session writes route through the service: `append_message` (caller-minted `id` — turn passes `turn.message_id` — + `status`), `set_message_content`, `set_message_status`, `append_tool_call` (caller-supplied `seq`/`call_index`) — redaction + ownership + row-membership checks move INTO the service; the route/turn keeps claim/attach/dedupe orchestration only |
| PROCEDURES | 3 procs | +5 (Q7) |
| `dialect_oxpilot` test | covers 0085 tables | extends to 0087 tables (NFR10.9) |
| `docs/TESTING.md` `-E` row | exists for the dialect test | updated pattern covers 0087 (NFR10.2) |
