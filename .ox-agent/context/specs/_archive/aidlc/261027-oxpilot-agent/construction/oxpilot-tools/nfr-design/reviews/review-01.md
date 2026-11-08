## Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY
**Iteration:** 1

**Scope:** adversarial review of the eight U5 `oxpilot-tools` NFR-design
artifacts (`security-design.md` primary, plus `reliability-design.md`,
`performance-design.md`, `scalability-design.md`,
`observability-design.md`, `logical-components.md`,
`nfr-design-questions.md`, `traceability.json`) against the READY
upstream contracts in `construction/oxpilot-tools/functional-design/`
(BR1–BR9, `entities.md`, `contract-summary.md`) and
`construction/oxpilot-tools/nfr-requirements/` (NFR1.x–NFR6.x, NFR-Sx,
TS-1–TS-9), with every shipped-code claim verified against source:
`crates/oxidean-api-core/src/oxpilot.rs` (ToolExecutor/ToolCtx/ToolSpec
:58-122, `ACTION_ARGS_MAX` :541, `denied` :568-576, `TargetView`/
`ActionView` :613-664, `ConnectorGateway` :736-770, `ActiveTurns`
:939-991, `OxpilotRuntimeSlot.tools` :1046, `OxpilotRuntime` :1102-1231,
`SessionService` :393-522, `require_enabled` :1324),
`crates/oxidean-api-core/src/app.rs` (`AppState` :23-99,
`build_rpc_ctx_with_session` :537-576), `auth/gate.rs` :23-46,
`auth/session.rs` (`ResolvedSession` :37-42), `audit.rs` :25-56,
`rpc.rs` (`RpcDispatch`), `oxidean-api-oxpilot/src/runtime.rs`
(`emit_frame` :113-121, `invoke_call`, `drain_and_chain` :1062-1149),
`services.rs` (`owned` :107-117, `owned_proposal` :140-153,
`save_proposal` :625-647, `proposal` :650-655, `mark_applied` :658-681),
`settings.rs` (tools carry :463/471/498/550/562/670/684),
`connector/gateway.rs` :553-694, `oxidean-db/src/oxpilot.rs`
(`OxpilotProposalRow` :776-788, `PROPOSAL_SELECT_*` :967-980,
`NewOxpilotProposal` :1784-1793, `insert_proposal` :1796-1850,
`mark_proposal_applied` :1898-1929, `OxpilotActionRow` :2206),
`oxidean-db/src/lib.rs` (`find_user_by_id` :3131,
`mark_oxpilot_proposal_applied` :7206-7211),
`oxidean-core/src/repo_types.rs` (:1226-1379),
`oxidean-core/src/pull_types.rs` (:129-144, :295-310),
`oxidean-api-pulls/src/comments.rs` :195-264.

**Verified-correct mechanisms (no finding):** the dispatch ladder order
satisfies BR1.3/BR1.4/BR2.5; `denied()`'s structural shape
`data.denied.{reason,grant_id?}` matches the decoder verbatim;
`ACTION_ARGS_MAX` is exactly `2 * 1024`; `turns()` returns the shared
`Arc<ActiveTurns>` (not a snapshot) and synchronous emits inside
`invoke` can only resolve the invoking turn (release+successor-claim
post-date it under the admission lock); `emit_frame` is `pub(crate)`
and same-crate placement compiles; `require_verified` resolves the real
`session.user_id` DB-side and works on the synthesized `oxpilot:`-id
ctx; every `repo.*`/`pull.*` proc name and the `pull.comments.create`
unanchored path exist with the claimed wire shapes;
`audit::record_with` warn-and-swallow and `find_user_by_id` are as
claimed; `svc.proposal` is user-scoped so the post-fetch session pin is
correctly layered; `OxpilotRuntimeSlot.tools` is `pub` and every
rebuild path already carries `prior.tools.clone()`.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Major | performance-design.md:36; security-design.md:158-161,175-180; logical-components.md:57-58 | The `diff_apply` per-step wire payloads are under-specified to the point the documented chain cannot complete: `repo.file.delete`/`rename`/`upload` all REQUIRE `message` (non-Option, no serde default — repo_types.rs:1335/1355/1376), the apply branch rides the flattened `RepoFileCommitOptions.branch` wire field (:1271-1288), `repo.branchCreate.start` omitted defaults to the repo DEFAULT branch not the session's bound branch (:1230-1232), and upload entries require `content_base64` not the ops field name `content_b64` (:1363-1368). None of `message`/`start`/`branch`/`content_base64` appears in the design — the first file op produces `rpc.bad_input`. Additionally `RepoFileCommitOptions`'s protected-branch fallback (:1267-1269: protected `branch` + absent `new_branch` ⇒ auto `web-edit/*` branch + auto-opened PR) is unaddressed — if `oxpilot/proposal-*` ever matches a repo protection rule the apply commits silently retarget and `pull.create` opens an incorrect PR. | Pin the per-step wire payloads: `branchCreate{owner,name,branch:"oxpilot/proposal-<id8>",start:row.branch}`; every `repo.file.*` op `{owner,name,...,message:<synthesized>,branch:"oxpilot/proposal-<id8>"}` with upload entries renaming `content_b64`→`content_base64`; `pull.create{owner,name,title,body,base_ref:row.branch,head_ref:"oxpilot/proposal-<id8>"}`; and state whether the protection-fallback edge is accepted or excluded (e.g. verify the branchCreate'd name is unprotected or fail `apply_failed`). | New |
| R-02 | Major | logical-components.md:27-31; traceability.json TS-7 (:91) vs entities.md:93-97 | The `ops_json` additive chain is under-enumerated: the wiring section names only the ×3-dialect migration + `find_oxpilot_proposals_by_session`. The full chain upstream enumerates is absent here — `ProposalWrite.ops_json` (core/oxpilot.rs:337-342 lacks it), `NewOxpilotProposal.ops_json` (db/oxpilot.rs:1784-1793), `OxpilotProposalRow.ops_json` (:776-788), the `insert_oxpilot_proposal` bind list (:1796-1850), the `PROPOSAL_SELECT_{PG,MYSQL,SQLITE}` column lists (:967-980) + `map_proposal!`, and the `save_proposal` signature/impl (services.rs:625-647). Implemented to this doc's own enumeration, the column exists but `propose_diff` cannot write it and `proposal`/`diff_get` cannot return it — a silently incomplete apply path, and TS-7's `OK` papers over it. | Enumerate the full additive chain (three types + insert column list + three SELECT constants + row mapper + `save_proposal` signature) in the module map/wiring section, or cite entities.md:93-97 explicitly as the enumeration of record; expand the TS-7 note to cover the type/mapper chain not just the column. | New |
| R-03 | Major | logical-components.md:56,91 | The reused-seams table and wiring diagram name `svc.proposals` — no such verb exists on the C5 `SessionService` port (oxpilot.rs:393-522 exposes `get_session`/`save_proposal`/`proposal`/`mark_applied` only; a `proposals`/`list_proposals` verb is nowhere in the codebase). The same row attributes `require_enabled`/`require_verified` to `SessionService` — they are free fns: `oxidean_api_core::oxpilot::require_enabled` (:1324) and `auth::gate::require_verified` (gate.rs:23). As written `diff_list` has no implementable path: calling a nonexistent verb fails to compile and widening C5 is the alternative Q7-B explicitly rejects. | Pin `diff_list`'s backing in both places: `svc.get_session` (ownership) → `state.db.find_oxpilot_proposals_by_session(session_id)` — the additive db verb already pinned for exactly this (upstream functional-spec names it for `diff_list`/ArtifactScope) — or explicitly declare a new C5 verb with its port change; correct the seams row to attribute `require_enabled`/`require_verified` to their actual homes. | New |
| R-04 | Minor | reliability-design.md:110-114; traceability.json NFR4.12 (:64) | "Double-materialization is impossible: `svc.mark_applied` re-enforces ... `status != 'pending'`" attributes the guarantee to a non-atomic check-then-write — `mark_proposal_applied` issues an unconditional `UPDATE ... SET status='applied' ... WHERE id=?` with no `AND status='pending'` predicate on any dialect (db/oxpilot.rs:1898-1929). The actual serialization is the deterministic `oxpilot/proposal-<id8>` branch name colliding at `branchCreate` (the loser's chain dies at `apply_failed:branch_exists` before reaching `mark_applied`); the service recheck only covers sequential retries. The conclusion holds but the mechanism cited does not provide it, and the db write would be racy if the chain order ever changed. | Correct the mechanism text to name the deterministic branch-name + fails-closed `branchCreate` as the concurrency guard (the service recheck as the sequential-retry guard only); optionally tighten `mark_proposal_applied` to `AND status='pending'` + `rows_affected` ⇒ `diff_not_pending` so the claim is literally true. | New |
| R-05 | Minor | logical-components.md:16; nfr-design-questions.md:34 vs rules.md BR3.2 (:134-137) | "16 proc-backed read handlers" counts one more than the 15 handler names listed (5 repo + 2 issue + 2 pull + 6 actions). The missing 16th maps to `repo.get`, BR3.2's "(session-binding precheck)" — an allowlisted proc with no catalog handler (no `repo_get` tool exists in the 23) and no consistent dispatch site: a per-invoke precheck dispatch would break NFR3.2's exactly-one-`rpc_dispatch` pin (performance-design.md:30). The design carries the 10-literal allowlist (:53 "(10+6+1 procs)") without resolving where `repo.get`'s annotated role lands. | Fix the count to 15 and state explicitly whether `repo.get` is allowlisted-but-undispatched in v1 (the session row already carries the binding — reconcile the BR3.2 parenthetical upstream), or name the handler/site that dispatches it and reconcile with the single-dispatch pin. | New |
| R-06 | Minor | security-design.md:229-232; reliability-design.md:37-40 vs nfr-requirements NFR4.5 | The proc-error mapping is ambiguous between two readings: "any other code → verbatim `ToolError{code}`" versus "internal/unknown ⇒ `internal`". Under the first, `auth.email_unverified`/`rpc.bad_input`/`repo.archived`/`db.not_configured`/`oxpilot.internal` become ToolError codes outside the pinned 11-arm taxonomy — contradicting reliability-design's "every failure lands on exactly one typed arm" totality claim and making a U5-side input defect (`rpc.bad_input` from a malformed synthesized request) a named arm instead of `internal`. The verbatim set is never enumerated. | Pin the verbatim set explicitly — recommended {`not_found`, `forbidden`, `invalid_args`} verbatim, ALL other codes ⇒ `internal` + generic message — and reconcile the taxonomy's totality claim with it. | New |
| R-07 | Minor | performance-design.md:76; security-design.md:183-184 | The serialized-args bound `serde_json::to_string(&args).len() <= ACTION_ARGS_MAX` does not pin which string is measured and dispatched: the re-serialized validated `Value` vs the raw `call.args_json`. If the raw string is passed to `dispatch_action` while the re-serialized form is bounded, a whitespace-padded raw >2 KiB slips the check and hits the gateway's `head_bounded` truncation mid-JSON — the intended `invalid_args` reject degrades into a truncated/unparseable stored-args connector failure. | Pin "bound and send the SAME string": re-serialize the validated args, measure that string, and pass exactly that string to `dispatch_action`'s `args` parameter. | New |
| R-08 | Minor | observability-design.md:82-87; nfr-design-questions.md:146-155 | `emit_card`'s correctness rests on an unstated assumption: `turns().get(ctx.session_id)` resolves the CURRENT map entry, which is the invoking turn only because emits run synchronously inside the awaited `invoke` — `release` post-dates the turn task and `drain_and_chain` claims the successor only after release under the admission lock (runtime.rs:1062-1149). A future detached/spawned emit would land on a SUCCESSOR turn's replay buffer — card misattribution, a strictly worse outcome than the documented silent drop. The helper's contract never pins the synchronous-emission requirement. | Add one line pinning card emits to synchronous execution inside `invoke` (no detached emission), so `get()` can only ever resolve the invoking turn. | New |
| R-09 | Minor | logical-components.md:25-26 | "installed onto `OxpilotRuntimeSlot.tools` where the slot is constructed (runtime.rs)" mislocates the install on two axes: the slot is constructed in api-core `OxpilotRuntime::new`/`from_slot` (oxpilot.rs:1163-1187) and the settings.rs rebuild paths — not `crate::runtime` — and `OxpilotTools::new(state.clone())` needs a finished `Arc<AppState>` that cannot exist inside `AppState::new` (which itself calls `OxpilotRuntime::new` at :93's field init). No `install_tools` setter exists. The workable site is post-`AppState::new` `state.oxpilot.slot.write().await.tools = Some(...)` at shell assembly beside `install_sessions`/`install_gateway`; the rebuild-time carry is already shipped (settings.rs:463/471/498/550/562/670/684). | Pin the actual install site (shell assembly, post-`AppState::new`, via the `pub` field or a new `install_tools` mirroring `install_gateway`) and drop the `(runtime.rs)` cite. | New |
| R-10 | Nit | security-design.md:30-33; nfr-design-questions.md:73-75 vs entities.md:54 | `CatalogEntry.handler` deviates from the READY upstream's pinned type `fn(&OxpilotTools, ToolCtx, Value) -> Result<ToolOutput, ToolError>` (entities.md:54) to `fn(&OxpilotTools, &InvokeCtx, Value) -> BoxFuture<Result<ToolOutput, ToolError>>` — substituting an NFR-design-only `InvokeCtx{ctx, session, entry}` and adding the BoxFuture wrapper (necessary for fn pointers to async fns). A reasonable refinement, but it is a deviation from a pinned contract type that no artifact flags. | Flag the handler-signature change as a deliberate deviation in the design, or carry an upstream amendment to entities.md:54; acknowledge `InvokeCtx` as an NFR-design-introduced type. | New |
