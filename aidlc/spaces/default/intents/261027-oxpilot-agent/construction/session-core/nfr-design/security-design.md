# Security Design — session-core (U2)

Consumes: `nfr-requirements/security-requirements.md` (NFR1.1–1.10,
NFR2.1), `functional-design/rules.md` (BR1.x, BR3.1, BR5.x),
`nfr-design-questions.md` (Q1, Q2, Q5).

## Defense-in-depth layout

| boundary | control | covers |
|----------|---------|--------|
| RPC transport | five new procs SessionOnly-by-omission + `require_enabled` gate at each handler top (same as `session_create`/`session_list`, session.rs:58/:139) — PAT `auth.pat_scope`, anon `auth.unauthenticated`, flag-off `oxpilot.disabled`; closed PROCEDURES-set test pins the registration | NFR1.3, NFR10.8 |
| Service | every method takes `caller_user_id`; store fns filter `WHERE user_id=?`; missing-or-foreign ⇒ `not_found` uniformly | NFR1.1, NFR1.2, NFR1.4 |
| Write path | `redact_in_place` at the top of each body-bearing method BEFORE the store call — store layer only ever sees redacted bytes | NFR1.5, NFR1.6 |
| Delete | hooks vec (ordered) → single cascade DELETE; hook error aborts before DELETE runs; audit row written by the PROC layer after a successful service return | NFR1.8, NFR1.9, NFR5.1 |

## Service trait + DTO set (Q1, NFR10.4)

```rust
// crates/oxidean-api-core/src/oxpilot.rs — beside `LlmProvider` (:111)
// Error type is the shipped shared `AppError` (oxidean_core):
// { code, message, data } — C1 vocabulary on `code`; BudgetExceeded
// carries {"scope": "user"|"instance"} in `data`.

// Return types are the SHIPPED `oxidean_db::oxpilot` row structs —
// `OxpilotSessionRow`, `OxpilotMessageRow`, and the new U2
// `OxpilotToolCallRow`/`OxpilotQueuedRow`/`OxpilotProposalRow`/
// `OxpilotUsageCounterRow` (added by the 0087 store code). api-core
// already depends on oxidean-db (Cargo.toml:13), so no parallel DTO
// layer is invented for reads. (R-08 residual: `Session`/`Message`/
// `ToolCall` do not exist — `ChatMsg` is a provider-request type.)
//
// INPUT DTOs — declared in this module beside the trait (codegen
// names them; shapes mirror the entities + C1 payloads):
//   CreateSession {repo_owner, repo_name, repo_id, branch, goal?, title?}
//     — the PROC resolves repo_owner/name via resolve_repo_for_read
//     (RpcCtx-bound) and fills repo_id + default-branch fallback BEFORE
//     the service call; the service never re-resolves (R-23)
//   AppendMessage {id, session_id, role, content, client_msg_id?, status}
//     — `id` is caller-minted (the turn passes its `turn.message_id`
//     for the assistant row; procs mint a fresh id for user rows) and
//     `status` is `complete` | `streaming` | `error` — mirrors the
//     shipped `NewOxpilotMessage` minus `seq` (the service mints seq
//     via next_message_seq inside the call) (R-02)
//   Enqueue {session_id, content, client_msg_id?}
//   QueuedReceipt {queued_id: Option<String>, position: Option<i64>} —
//     enqueue's return type: the C1-pinned wire shape the row struct
//     can't carry (position is the ordinal, not queue_seq — see "Queue
//     position" below); both fields null = transcript-hit dedupe (R-01)
//   ToolCallWrite {session_id, message_id, seq, call_index, call_id,
//                  tool, args_summary, status}
//     — `seq` is the positional copy of the owning message's seq,
//     supplied by the caller (the turn holds the OxpilotMessageRow it
//     created); `call_index` is the caller-minted per-message ordinal
//     (0-based, incremented per call in the turn loop) (R-03)
//   ToolCallStatusUpdate {session_id, call_id, status, output_summary?}
//   ProposalWrite {session_id, title, base, diff_body}
//   UsageDelta {requests, tokens}
//   BudgetCaps {user: Option<i64>, instance: Option<i64>}
//   UsageView {period, tokens_used, tokens_cap, requests_used, requests_cap}
//     — usage_view fills ALL five fields from the caps arg it was
//     handed; the proc serializes the view verbatim as the C1 payload
//     (single writer — no second merge step) (R-09)

#[async_trait]
pub trait SessionService: Send + Sync {
    async fn create_session(&self, user_id: &str, in_: CreateSession)
        -> Result<OxpilotSessionRow, AppError>;
    async fn get_session(&self, user_id: &str, id: &str)
        -> Result<OxpilotSessionRow, AppError>;
    async fn list_sessions(&self, user_id: &str)
        -> Result<Vec<OxpilotSessionRow>, AppError>;
    async fn delete_session(&self, user_id: &str, id: &str) -> Result<(), AppError>;
    async fn append_message(&self, user_id: &str, in_: AppendMessage)
        -> Result<OxpilotMessageRow, AppError>;
    async fn find_message_by_client_msg_id(&self, user_id: &str, session_id: &str, cmid: &str)
        -> Result<Option<OxpilotMessageRow>, AppError>;  // ANY status incl. streaming (R-04)
    async fn set_message_content(&self, user_id: &str, session_id: &str,
        message_id: &str, content: &str) -> Result<(), AppError>;
        // streaming accumulation — verifies message_id's row belongs to
        // session_id (find_message_by_id) then writes REDACTED content (R-02)
    async fn set_message_status(&self, user_id: &str, session_id: &str,
        message_id: &str, status: &str) -> Result<(), AppError>;
        // streaming → complete|error transitions; same row-membership check (R-02)
    async fn append_tool_call(&self, user_id: &str, in_: ToolCallWrite)
        -> Result<(), AppError>;
    async fn set_tool_call_status(&self, user_id: &str, in_: ToolCallStatusUpdate)
        -> Result<(), AppError>;                        // BR7.3 running → done|error
    async fn list_tool_calls(&self, user_id: &str, session_id: &str)
        -> Result<Vec<OxpilotToolCallRow>, AppError>;   // C2 snapshot merge source
    async fn transcript(&self, user_id: &str, session_id: &str)
        -> Result<Vec<OxpilotMessageRow>, AppError>;    // BR7.2 full non-streaming set —
        // `require_owned_session` + `list_messages` (REUSED unfiltered — it is
        // shared with the snapshot path that needs `streaming` rows), filtered
        // to `status IN ('complete','error')` in the service (R-01 of this
        // pass: the SQL-shape pin elsewhere describes the returned SET)
    async fn enqueue(&self, user_id: &str, in_: Enqueue)
        -> Result<QueuedReceipt, AppError>;             // {queued_id, position} — C1 wire shape (R-01)
    async fn drain_queue(&self, user_id: &str, session_id: &str)
        -> Result<Vec<OxpilotQueuedRow>, AppError>;
    async fn list_queue(&self, user_id: &str, session_id: &str)
        -> Result<Vec<OxpilotQueuedRow>, AppError>;
    async fn record_usage(&self, user_id: &str, d: UsageDelta) -> Result<(), AppError>;
    async fn check_budget(&self, user_id: &str, caps: BudgetCaps)
        -> Result<(), AppError>;   // Err code=budget_exceeded, data={scope} — amended C5
    async fn usage_view(&self, user_id: &str, caps: BudgetCaps)
        -> Result<UsageView, AppError>;   // usage_get backing read — NEVER refuses (R-16)
    async fn save_proposal(&self, user_id: &str, in_: ProposalWrite)
        -> Result<OxpilotProposalRow, AppError>;
    async fn proposal(&self, user_id: &str, id: &str)
        -> Result<OxpilotProposalRow, AppError>;
    async fn mark_applied(&self, user_id: &str, id: &str, pr_ref: &str)
        -> Result<(), AppError>;                        // WF8.3 carries the PR ref — amended C5
}

#[async_trait]
pub trait SessionDeleteHook: Send + Sync {
    async fn on_session_delete(&self, session_id: &str, user_id: &str)
        -> Result<(), AppError>;
}
```


## Budget caps reach the service as input (R-01/R-15 fix)

Budgets live on `EffectiveProviderConfig` (`user_budget`/
`instance_budget`, api-oxpilot settings.rs:52/104) — the slot never
stored them. U2 adds `budget_caps: BudgetCaps` to
`OxpilotRuntimeSlot`. EVERY constructor path populates it — the pin
is per-site, not "populated at rebuild" (an ambiguous phrase that
lets `carried_slot` silently keep stale caps, R-15):

- `OxpilotRuntime::new` (boot, api-core) parses
  `OXIDEAN_OXPILOT_USER_BUDGET`/`OXIDEAN_OXPILOT_INSTANCE_BUDGET`
  into the boot slot's caps.
- `SlotPlan::Slot` literals inside `resolve_slot` set
  `budget_caps: BudgetCaps::from(cfg)`.
- `carried_slot(prior, cfg)` — signature GAINS `&EffectiveProviderConfig`;
  caps come from cfg, NOT `prior` (a budget-only write must take effect).
- `built_slot(…, cfg, …)` sets them from cfg (it already takes cfg).
- `disabled_slot(turns, reason, caps: BudgetCaps)` — the caps arg is
  typed `BudgetCaps` (an api-core type): api-oxpilot callers pass
  `BudgetCaps::from(cfg)`, api-core's standalone callers
  (`OxpilotRuntime::new` boot path :624-class, `api/tests/oxpilot.rs:47`)
  pass `BudgetCaps::default()` = unbounded. The
  `impl From<&EffectiveProviderConfig> for BudgetCaps` lives in
  api-oxpilot (it owns the source type — orphan-rule legal;
  api-core cannot name `EffectiveProviderConfig`, R-07).
- `from_slot` test seam takes the field on the slot it's handed.

`check_budget` takes the slot-read caps as a param — the stateless
service stays pure over `Database` + hooks and never needs a
back-reference into its own containing slot.

## Service residence — `OxpilotRuntime`, not the slot (R-15)

`sessions` CANNOT be a non-Option slot field: `OxpilotRuntime::new`
builds the boot slot inside api-core which cannot name
`OxpilotSessionService` (api-oxpilot type — the forbidden reverse
edge). The service instead lives beside the slot:

```rust
// api-core::oxpilot::OxpilotRuntime gains:
sessions: std::sync::OnceLock<Arc<dyn SessionService>>,   // private field
// installed once by the shell post-construction:
//   oxpilot.install_sessions(Arc::new(OxpilotSessionService::new(db, vec![])))
// install_sessions is `self.sessions.get_or_init(|| svc)` semantics —
// set-once; a second call is a no-op returning the installed Arc
// (tests install per-runtime; from_slot runtimes start empty).
// main.rs calls it right after AppState::new — the same place U4's
// unconditional rebuild runs.
//
// Named accessor (R-04) — `ctx.oxpilot` is `Option<SharedOxpilot>`
// (rpc.rs:135), so procs unwrap per the settings.rs:591 idiom
// (`ctx.oxpilot.clone().ok_or_else(|| AppError::new("oxpilot.internal",
// …))?`) then call `.sessions()?`; the `require_enabled` gate has
// already mapped None → oxpilot.disabled, so the None arm is
// unreachable post-gate:
//   pub fn sessions(&self) -> Result<Arc<dyn SessionService>, AppError> {
//       self.sessions.get().cloned()
//           .ok_or_else(|| AppError::new("oxpilot.internal",
//               "session service not installed"))
//   }
// Unset ⇒ `oxpilot.internal` (boot invariant violated), never silently
// unscoped.
```

## Ownership binding (NFR1.1–1.4)

```rust
// concrete impl skeleton — services.rs in api-oxpilot
impl SessionService for OxpilotSessionService {
    async fn get_session(&self, user_id: &str, id: &str) -> Result<OxpilotSessionRow, AppError> {
        db::oxpilot::find_owned_session(&self.db, id, user_id).await
            .map_err(store_err)?                    // String store err → AppError internal
            .ok_or_else(|| AppError::new("not_found", "session not found"))
    }
    // every write: redact_in_place(&mut in_) FIRST (Q5), then the store
    // call binds user_id from the caller arg — input has no ownership
    // field (NFR1.1/BR1.2)
}
```

The store layer takes `user_id` on ownership-bearing lookups
(`find_owned_session`); the session-scoped reused fns
(`find_message_by_client_msg_id`, `list_messages`, `next_message_seq`,
`insert_message`, `list_tool_calls_for_session`, `drain_queue_tx`, …)
are keyed by `session_id` only — so EVERY session-scoped service method
runs a `require_owned_session(user_id, session_id)` pre-check
(`find_owned_session` → `not_found`) BEFORE the session-keyed store
call (R-18). One extra indexed read per op — the performance budget
rows carry it explicitly (NFR3.x).

The two id-keyed proposal verbs take no `session_id`, so the pre-check
can't run first: `proposal(user_id, id)` and `mark_applied(user_id, id,
pr_ref)` FETCH the proposal row via `find_proposal(id)` then run
`require_owned_session(user_id, row.session_id)` against the row's FK
— missing-or-foreign still resolves `not_found` uniformly (the join
alternative was rejected: one indexed read + the same ownership fn
keeps the mechanism single-sourced). `mark_applied` runs the check
before `mark_proposal_applied` writes.

## Redaction boundary (Q5)

```rust
fn redact_in_place(&self, w: &mut impl Redactable) {
    // redact() each body-bearing field per BR3.1's exact set:
    // message/queued content, tool_call args_summary/output_summary,
    // proposal title + diff_body, session goal/title.
    // NOT redacted: ids (client_msg_id, proposal base ref/sha,
    // call_id) — secret-shaped ids are caller handles; corrupting
    // them breaks dedupe/diff-apply (NFR1.5's identifier carve-out).
}
```

`Redactable` is a private per-payload trait (or per-field calls — a
mechanical detail for codegen), not an invented framework.

## Delete seam (Q2)

```rust
// OxpilotSessionService fields (constructed once at shell build)
db: oxidean_db::Database,
hooks: Vec<Arc<dyn SessionDeleteHook>>,   // U2 ships vec![] + cfg(test) ProbeHook

// delete_session
let _session = db::oxpilot::find_owned_session(&self.db, id, user_id)
    .await.map_err(store_err)?
    .ok_or_else(|| AppError::new("not_found", "session not found"))?;
for (i, h) in self.hooks.iter().enumerate() {
    h.on_session_delete(id, user_id).await.map_err(|e| {
        // MERGE, not overwrite — with_data replaces `data` wholesale
        // (oxidean-core lib.rs:136), so insert hook_index into the
        // hook's own object when it has one, else wrap it (R-08):
        let data = match e.data {
            Some(serde_json::Value::Object(mut m)) => {
                m.insert("hook_index".into(), i.into()); m.into()
            }
            other => json!({"hook_index": i, "hook_error": other}),
        };
        e.with_data(data)
    })?;                                            // Err aborts (R-20)
}
db::oxpilot::delete_session_cascade(&self.db, id, user_id).await   // NEW store fn (R-06)
    .map_err(store_err)
```

`delete_session_cascade` is a NEW `oxidean-db` helper — one
`DELETE FROM oxpilot_sessions WHERE id=? AND user_id=?` relying on
`ON DELETE CASCADE`. Audit is NOT in the service (the service has no
`RpcCtx`): the `oxpilot.session_delete` proc calls
`audit::record(&ctx, …, "oxpilot.session_delete", …)` after the
service returns Ok — proc-layer audit matches `audit::record`'s
signature and `record_with` swallows errors by design, so a failed
audit never half-deletes (NFR5.1, R-09). A hook-aborted delete audits
`detail={"result":"hook_failed","hook_index":i}` (the JSON detail
column — `insert_audit_event` has no result column; the index rides
`AppError.data` out of the service so the proc can log it,
NFR5.3/R-20).

Hook registration: shell `main.rs` constructs
`OxpilotSessionService::new(db, vec![])` in v1 — U3 inserts
`AbortTurnsHook` at index 0, U6 appends `RevokeGrantsHook` (order =
construction order; no priority field). The `#[cfg(test)]`
`ProbeHook` records call order and returns scripted errors to prove
abort-before-cascade (NFR10.10).

## Proc-side pinning

| proc | auth | gate | errors surfaced |
|------|------|------|-----------------|
| `oxpilot.session_get` | SessionOnly | gated | forbidden, not_found, oxpilot.disabled, oxpilot.internal |
| `oxpilot.session_delete` | SessionOnly | gated | forbidden, not_found, oxpilot.disabled, oxpilot.internal, hook error |
| `oxpilot.message_queue` | SessionOnly | gated | forbidden, not_found, oxpilot.disabled, budget_exceeded, session_active_turn_absent (no live turn — BR4.1/C1-pinned), oxpilot.internal |
| `oxpilot.queue_list` | SessionOnly | gated | forbidden, not_found, oxpilot.disabled, oxpilot.internal |
| `oxpilot.usage_get` | SessionOnly | gated | forbidden, oxpilot.disabled, oxpilot.internal |

(Error columns match the functional-spec proc table plus the
`oxpilot.internal` escape hatch on every store-touching proc — fd
lists it only where it itemized it; `oxpilot.internal`, not bare
`internal`, is the session.rs:23 convention for store failures;
`forbidden` rides every SessionOnly proc's auth gate.)

`usage_get`'s proc calls `usage_view(user_id, slot.budget_caps)` and
serializes the returned `UsageView` VERBATIM as the C1 `{period,
tokens_used, tokens_cap, requests_used, requests_cap}` payload — the
service is the single writer of the cap fields (it was handed the
caps); no second merge step in the proc (R-09). Null caps = unbounded.

## Queue position derivation (R-01/R-26)

`enqueue` returns `QueuedReceipt {queued_id, position}` — `position`
is the ordinal, NOT `queue_seq` (drains leave gaps): `position =
COUNT(*) WHERE session_id=? AND queue_seq < mine) + 1`, computed
inside the enqueue path immediately post-insert. Resend arms return
the SAME shape:

- queued-hit (`find_queued_by_cmid` match): the existing row's
  `{queued_id, position}` with position recomputed by the same count —
  the deduped row may have moved forward after a drain.
- transcript-hit (`find_message_by_client_msg_id` match): the pinned
  literal `{"queued_id": null, "position": null}` — the artifact is
  already persisted; nulls signal "nothing queued, client already has
  the row" (BR2.2 "IF in messages THEN attach/return it").
