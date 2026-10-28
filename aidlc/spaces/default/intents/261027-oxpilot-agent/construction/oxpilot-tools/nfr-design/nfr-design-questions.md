# NFR Design Questions — oxpilot-tools

Unit: `oxpilot-tools` (U5, kind `service`) — `OxpilotTools`: the closed
model-callable tool catalog (repo/issue/pull reads, six read-only
Actions tools, `propose_diff`/`proposal_get`, ArtifactScope-bound
`pr_comment`, `exec_shell`/`fs_*`/`targets_list`/`action_status`
dispatching into `ConnectorGateway`) plus the user-only
`oxpilot.diff_list`/`diff_get`/`diff_apply` procs.

## Sources

- [nfr-req] construction/oxpilot-tools/nfr-requirements/*.md —
  NFR1.x–NFR6.x/NFR-Sx/TS-x rows the designs must satisfy (READY
  iter-2).
- [design] functional-design/{entities,rules,functional-spec}.md —
  BR1–BR9 entity/state ground truth (READY iter-2).
- [contract] contract-summary.md — C1 `oxpilot.diff_*` procs, C6
  `ToolExecutor`/`ToolCtx`/`ToolOutput`/`ToolError`, C7
  `dispatch_action`/`action_status` + `Denied{reason, grant_id?}`.
- [code] crates/oxidean-api-core/src/{oxpilot,app,audit,rpc}.rs,
  crates/oxidean-api-oxpilot/src/{runtime,session,services,
  connector/gateway}.rs, crates/oxidean-core/src/redact.rs — the
  shipped seams U5 plugs into.

## Q1. Module layout inside `oxidean-api-oxpilot`?

- A. `src/tools/` dir mirroring the `connector/` precedent:
  `mod.rs` (`OxpilotTools` — AppState handle + the `ToolExecutor`
  impl: `catalog()`, the `invoke` dispatch ladder, and the shared
  helpers `resolve`/`emit_card`/`audit`/`bounded`/`dispatch_proc`),
  `registry.rs` (the `&'static [CatalogEntry]` table + per-tool
  `schema` fns — one row per tool, NAMED handler fns, no closures),
  `validate.rs` (the closed-vocabulary JSON-Schema walker),
  `read.rs` (the 15 proc-backed read handlers — BR3.2's tenth
  literal `repo.get` is allowlisted-but-undispatched in v1: no
  `repo_get` catalog entry exists, the session row already
  carries the binding, and a per-invoke precheck dispatch would
  break NFR3.2's exactly-one-`rpc_dispatch` pin; the BR3.2
  "(session-binding precheck)" annotation reconciled upstream),
  `propose.rs` (`propose_diff`/`proposal_get` + the diff↔ops
  coherence check), `dispatch.rs` (the exec/fs trio +
  `targets_list`/`action_status` on the gateway),
  `comment.rs` (`pr_comment` + ArtifactScope), `procs.rs` (the
  user-only `oxpilot.diff_*` trio, registered on PROCEDURES +
  dispatch.rs alongside the shipped oxpilot arms). Every U5 file
  sits in one dir; the crate's PROCEDURES/dispatch pin tests cover
  the proc registration the same way they cover U6's arms.
- B. Single `src/tools.rs` — ~1.5k+ lines across five distinct
  concerns (registry, validator, four handler families, user procs);
  the registry/handler seams the pin tests need would hide in one
  file, and the crate's own `connector/` split is the established
  shape for a multi-surface unit.
- C. New `oxidean-api-tools` crate — `emit_frame` is `pub(crate)`
  and `invoke(call, ctx)` hands the tool no turn handle, so the
  card emit path only compiles same-crate; a new crate forces a
  visibility widening for zero separation gain (TS-1, already
  settled at FD).
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q2. Registry row shape + handler form — what does a `CatalogEntry` hold?

- A. `CatalogEntry{name: &'static str, description: &'static str,
  schema: fn() -> Value, class: ToolClass, kind: EntryKind,
  handler: HandlerFn}` inside `static CATALOG: &[CatalogEntry]` —
  a `&'static` table can't hold `String`, so `schema` is a plain fn
  returning the `parameters_json` `Value` (`serde_json::json!`
  isn't const-constructible); `class: Read|Write` stays internal
  (D-01, never on the wire `ToolSpec`); `kind: EntryKind
  {Proc(&'static str) | Connector | Proposal | GatewayRead}` is the
  FIXED dispatch binding — the literal allowlisted proc name for
  reads/`pr_comment`, `spec.name` verbatim as the C7 kind for the
  exec/fs trio, the svc/gateway path for the rest — so the closure
  pin test audits name→kind→handler in ONE table without executing
  anything (NFR1.4's "absence, not blocklist"); `handler` is a
  NAMED async fn per tool (`fn(&OxpilotTools, &InvokeCtx, Value)
  -> BoxFuture<Result<ToolOutput, ToolError>>`) — fn pointers, not
  boxed closures, so every handler is greppable by name.
  DELIBERATE DEVIATION from the READY upstream's pinned
  `fn(&OxpilotTools, ToolCtx, Value) -> Result<ToolOutput,
  ToolError>` (entities.md:54): the `BoxFuture` wrapper is
  required for fn pointers to async fns, and
  `InvokeCtx{ctx, session, entry}` is an NFR-design-introduced
  type carrying the already-resolved session + entry.
- B. `schema_json: &'static str` parsed lazily per `catalog()`/
  `invoke` — a malformed schema is a runtime panic instead of
  compile-checked code; the fn() form keeps schemas typed and the
  pin test exercises the same builder `catalog()` does.
- C. `Vec<CatalogEntry>` built once in a `LazyLock`/constructor —
  the entries ARE compile-time data; a runtime-built registry adds
  an init seam and invites "just push another entry" code paths
  the closed posture (BR1.1/NFR1.1) exists to forbid.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q3. Validator implementation — how does the BR1.4 schema gate run?

- A. `validate.rs`: one recursive-descent
  `validate(schema: &Value, value: &Value, path, depth)
  -> Result<(), FieldErr>` — keyword dispatch over EXACTLY the
  closed set the registry emits (`type`, `required`, `enum`,
  `properties`, `items`, `minItems`, `maxItems`, `minLength`,
  `maxLength`, `minimum`, `maximum`, `pattern`,
  `additionalProperties: false`); recursion depth-capped (authored
  schemas nest ≤3 — cap at 16); `pattern` resolves through a
  closed lookup of hand-authored matcher fns (the only authored
  patterns are path-shape rules — no `..`, no leading `/`) since
  no regex engine ships in this crate's dep tree (TS-9) and an
  unrecognized pattern is a registry defect, never a lenient pass;
  every violation ⇒ `invalid_args` naming the dotted field path
  (`files[2].new_path`); op-conditional requireds (rename⇒
  `new_path`, create/update⇒`content_b64`) stay handler-side —
  schema can't express them (TS-5).
- B. `jsonschema` crate — a full draft-2020-12 engine (new dep) to
  validate a ~12-keyword vocabulary U5 emits itself; rejected at
  FD-Q8-B/TS-5/TS-9.
- C. Per-handler manual arg checks — scatters the
  `additionalProperties:false` identity rejection (NFR1.6's whole
  mechanism) across 23 sites; the gate must run BEFORE the handler
  sees input (BR1.4), so a shared walker is the only consistent
  placement.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q4. Internal dispatch ctx synthesis — where is the `RpcCtx` minted?

- A. One shared helper on `OxpilotTools`:
  `dispatch_proc(&self, ctx: &ToolCtx, procedure, input)
  -> Result<Value, ToolError>` builds
  `build_rpc_ctx_with_session(&self.state, Some(ResolvedSession
  {session_id: "oxpilot:"+session.id, user_id: ctx.user_id,
  remember_me: false, expires_at: now+1h}),
  ClientMeta::default())` and invokes `state.rpc_dispatch` on it —
  PER CALL, dropped with the call (NFR-S4); every read tool,
  `pr_comment`, and the `diff_apply` proc chain ride the same
  seam; the `oxpilot:`-namespaced id can't collide with a Uuid
  session row (NFR1.10) and no IP/UA is fabricated.
- B. Constructor-synthesized ctx cached on `OxpilotTools` — a
  cached ctx leaks identity across sessions AND pins a stale
  session (NFR-S4; rejected at FD-Q11-B).
- C. Synthesize inside each handler — the identical mechanism
  scattered across ~18 call sites; one helper is the auditable
  seam and the only place the synthetic-id convention lives.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q5. Card emit path detail — how do `card{diff|consent}` reach the frame sink?

- A. `emit_card(&self, session_id, card_kind, ref_id)` helper in
  `mod.rs`: `state.oxpilot.turns().await.get(session_id).await` →
  `Some(turn)` ⇒ `crate::runtime::emit_frame(&turn,
  json!({"type":"card","card_kind":kind,"ref_id":ref_id}))` —
  `pub(crate)` compiles same-crate (TS-1); `None` (no live turn)
  or a post-`terminal_sent` frame drops silently — the
  `message_queue` `card{queued}` precedent (session.rs:331–341);
  the durable tool-call row is the guaranteed record and a dropped
  card is cosmetic (NFR4.8). One helper carries both
  `card{diff}` and `card{consent}`. Emission is PINNED to
  synchronous execution inside the awaited `invoke` — never a
  detached/spawned emit: `turns().get(session_id)` can only ever
  resolve the INVOKING turn because `release` post-dates the
  turn task and `drain_and_chain` claims a successor only after
  release under the session admission lock
  (runtime.rs:1062–1149); a detached emit could land on a
  successor turn's replay buffer — misattribution strictly
  worse than a silent drop.
- B. Widen `emit_frame` to `pub` and place the tools in api-core —
  a visibility widening on a seam the design keeps internal;
  rejected at TS-1.
- C. Emit through a new `SessionService` verb — the frame sink is
  the per-process `ActiveTurn` (`emit_frame` takes `&ActiveTurn`),
  not the db-backed service; widening the C5 seam for an
  in-memory emit is the wrong layer.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q6. Audit actor resolution — where does `actor` come from?

- A. Per-event `state.db.find_user_by_id(&ctx.user_id)` inside the
  audit helper → `Some((user_id, username))` on hit; miss or error
  ⇒ `None` — the event still emits (NFR5.4's explicit fallback —
  never skip the row). The lookup runs ONLY on write-class
  post-commit paths (≤1 indexed read per audited call) with NO
  cache: the executor is stateless (NFR-S1), and a username map
  would need a lock plus goes stale on rename.
- B. Executor-held `RwLock<HashMap<user_id, username>>` — the only
  lock U5 would own, stale-on-rename, and against the
  stateless-executor pin; saves one indexed read per write-class
  call.
- C. Always `actor: None` — drops the `admin.users.getActivity`
  actor join `record_with`'s doc asks for (audit.rs) and violates
  NFR5.4's "resolved via find_user_by_id" pin.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q7. `diff_apply` proc layering + the BR4.6 session pin's enforcement site?

- A. `tools/procs.rs` rides the shipped layering: `diff_apply`
  runs `require_enabled` → `require_verified` →
  `svc.proposal(user.id, proposal_id)` (user-scoped —
  same-user-other-session deliberately reachable, BR5.2) →
  `status != "pending"` ⇒ `diff_not_pending` → the ordered
  `ctx.rpc_dispatch` chain on the CALLER's own ctx
  (`repo.branchCreate` → `ops_json` ops → `pull.create`, each
  failure ⇒ `apply_failed:<step>`) → `svc.mark_applied` →
  `oxpilot.diff_applied` audit. The chain stays PROC-side because
  the C5 `SessionService` seam holds no `rpc_dispatch` handle —
  layering it there is impossible without widening the port.
  The BR4.6 same-session pin (`row.session_id ==
  ctx.session_id` post-fetch ⇒ `not_found`) lives in the
  `proposal_get` TOOL handler only — `svc.proposal`/
  `owned_proposal` stays user-scoped so the user proc's
  documented reach isn't broken.
- B. New `SessionService::apply_proposal` verb owning the whole
  chain — the oxpilot port has no dispatch handle (it isn't
  `AppState`); widening C5 to carry one crosses the seam
  contract for a sequence the proc layer already owns.
- C. Push the session pin into `svc.proposal`/`owned_proposal`
  (add a `session_id` param) — breaks `diff_apply`'s deliberate
  same-user-other-session relaxation (BR5.2); the pin is a
  MODEL-facing rule, not a data-ownership rule.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Consolidated Summary Confirmation

- Layout: `src/tools/{mod,registry,validate,read,propose,
  dispatch,comment,procs}.rs` inside `oxidean-api-oxpilot`;
  the `connector/` precedent, zero new crates (Q1).
- Registry: `&'static [CatalogEntry]` of
  `{name, description, schema fn, class, kind, handler fn}` —
  `EntryKind` makes the proc/kind allowlist auditable data;
  named handler fns, never closures (Q2).
- Validator: one recursive keyword walker over the closed
  vocabulary; `pattern` via a hand-authored matcher table — no
  regex dep; depth-capped; field-path errors (Q3).
- Dispatch ctx: per-call `build_rpc_ctx_with_session` inside one
  shared `dispatch_proc` helper; `oxpilot:`-namespaced id;
  `ClientMeta::default()` (Q4).
- Cards: one `emit_card` helper over `turns().get → emit_frame`;
  silent drop on absent/terminalled turn; durable row is the
  record (Q5).
- Audit actor: per-event `find_user_by_id`, `None` fallback, no
  cache — stateless executor (Q6).
- Apply: proc-side ordered `ctx.rpc_dispatch` chain (service has
  no dispatch handle); the BR4.6 session pin lives in the
  `proposal_get` handler only so `diff_apply`'s user-side
  relaxation survives (Q7).

[Answer]: Looks correct
