## Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 2

**Scope:** adversarial re-review of the eight U5 `oxpilot-tools` NFR-design
artifacts (`security-design.md` primary, plus `reliability-design.md`,
`performance-design.md`, `scalability-design.md`,
`observability-design.md`, `logical-components.md`,
`nfr-design-questions.md`, `traceability.json`) — every open R-01..R-10
finding re-checked against the current bytes AND the shipped source, plus
a fresh defect hunt on the fix diffs. Source-verified:
`oxidean-core/src/repo_types.rs` (`RepoBranchCreateRequest` :1226-1233 —
`start` Option defaulting to repo default branch, matching
lifecycle.rs:1347-1352; `RepoFileCommitOptions` flattened + protection
fallback :1258-1288; `message` non-Option on delete :1335 / rename :1355
/ upload :1376; `RepoFileUploadEntry.content_base64` :1363-1368;
`RepoFileCommitResponse{branch, created_branch, pr_number}` :1419-1429;
`RepoFileCommitPolicy{Request,Response{direct_commit_allowed,
requires_pr}}` :1397-1415), `oxidean-core/src/pull_types.rs`
(`CreatePullRequest{owner,name,title,body,base_ref,head_ref}` :129-144),
`oxidean-api-core/src/oxpilot.rs` (`ProposalWrite` lacks `ops_json`
:337-342; `SessionService` port :393-522 — no `proposals` verb;
`denied()` :568-576; `ActiveTurns::{claim,get,release}` :941-991;
`OxpilotRuntimeSlot.tools` pub :1046; `OxpilotRuntime::new`/`from_slot`
:1163-1187; `install_sessions`/`install_gateway` :1192/:1207;
`require_enabled` :1324), `oxidean-db/src/oxpilot.rs`
(`OxpilotProposalRow` :776-788; `map_proposal!` :874;
`PROPOSAL_SELECT_{PG,MYSQL,SQLITE}` :967-980; `NewOxpilotProposal`
:1784-1793; `insert_proposal` :1796-1850; `mark_proposal_applied`
unconditional UPDATE :1898-1929), `oxidean-api-oxpilot/src/services.rs`
(`save_proposal` :625-647; `mark_applied` `status != "pending"` recheck
:665-666), `oxidean-api-oxpilot/src/runtime.rs` (`emit_frame` pub(crate)
+ terminal CAS :113-122; `truncate_marker` :141-150; ToolCtx mint
:456-461; error→message path :579-588; `drain_and_chain` release→claim
under admission lock :1062-1149), `oxidean-api-oxpilot/src/session.rs`
(`card{queued}` precedent :326-341; `require_enabled`→`require_verified`
:349-350), `oxidean-api-oxpilot/src/{lib,dispatch}.rs` (PROCEDURES↔arm
parity test lib.rs:57-87; `connector::procs` precedent), `settings.rs`
(`prior.tools.clone()` :463/:471/:498), `oxidean-api/src/main.rs`
(`install_gateway`/`install_sessions` post-`AppState::new` :143-144),
`app.rs` (`OxpilotRuntime::new` field-init :257;
`build_rpc_ctx_with_session` :537-576), `auth/gate.rs` (`require_verified`
free fn :23), `audit.rs` (`record_with` :25-56),
`oxidean-api-repo/src/lifecycle.rs` (`branch_create` :1326-1362 — no
existence pre-check, `map_git_err` :1362),
`oxidean-api-core/src/repo/{mod.rs:78-87, files.rs}` (`map_git_err`
catch-all `repo.git_failed`; `file_git_err`'s `repo.branch_exists` arm
:59-61/:416-419; `direct_push_allowed` :342-354; protected fallback
:398-404), `oxidean-api-repo/src/dispatch.rs` (`repo.file.commitPolicy`
arm :68), `oxidean-git/src/cli.rs` (`git branch` exists-failure ⇒
`GitError::Process` :2609-2615/:322-332). Upstream contracts re-verified:
rules.md BR3.2 :131-145 (10-literal allowlist + "(session-binding
precheck)" still present), entities.md :54 (pinned handler sig) +
:93-97 (ops_json enumeration of record), tech-stack-decisions.md TS-7
:20 (`find_oxpilot_proposals_by_session` additive verb),
performance-requirements.md NFR3.5 :19.

**Verified-correct mechanisms (no finding):** the pinned per-step wire
payloads now match the wire structs verbatim — `branchCreate{owner,name,
branch:"oxpilot/proposal-<id8>",start:row.branch}` satisfies
:1226-1233's required fields + the default-branch trap; every
`repo.file.*` op's `{owner,name,...,message:<synthesized>,branch:...}`
satisfies the non-Option `message` + flattened `RepoFileCommitOptions`;
upload `files[].content_base64` matches `RepoFileUploadEntry`;
`pull.create{...,base_ref:row.branch,head_ref:...}` matches
`CreatePullRequest` (:129-144). The pinned verbatim error set
{`not_found`,`forbidden`,`invalid_args`} + all-else→`internal` is
internally consistent across `map_err` (security :279-287) and
`map_denied` (reliability :52-64) and preserves the totality claim —
`rpc.bad_input`/`auth.*`/`repo.*`/`oxpilot.internal` correctly fold to
`internal`. The "same string" pin for `ACTION_ARGS_MAX` is unambiguous
in both places. `mark_proposal_applied`'s unconditional UPDATE
(:1898-1929) is now accurately described as non-atomic with the real
guard named. `repo.get` undispatched is consistent — BR3.2's allowlist
is closed over ENTRIES, not coverage; the pin test asserts membership.
`find_oxpilot_proposals_by_session` is correctly framed as an ADDITIVE
verb (absent from crates/ — pinned upstream at TS-7 :20 + NFR3.5 :19).
`ActiveTurns::get` :966-968 + the release-then-claim ordering
:1074/:1122 under the admission lock make the synchronous-emission pin
load-bearing and true. `repo.file.commitPolicy` exists
(dispatch.rs:68) — the unprotected-precheck is implementable.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Major | performance-design.md:36; security-design.md:158-161,175-180; logical-components.md:57-58 | The `diff_apply` per-step wire payloads are under-specified to the point the documented chain cannot complete: `repo.file.delete`/`rename`/`upload` all REQUIRE `message` (non-Option, no serde default — repo_types.rs:1335/1355/1376), the apply branch rides the flattened `RepoFileCommitOptions.branch` wire field (:1271-1288), `repo.branchCreate.start` omitted defaults to the repo DEFAULT branch not the session's bound branch (:1230-1232), and upload entries require `content_base64` not the ops field name `content_b64` (:1363-1368). None of `message`/`start`/`branch`/`content_base64` appears in the design — the first file op produces `rpc.bad_input`. Additionally `RepoFileCommitOptions`'s protected-branch fallback (:1267-1269: protected `branch` + absent `new_branch` ⇒ auto `web-edit/*` branch + auto-opened PR) is unaddressed — if `oxpilot/proposal-*` ever matches a repo protection rule the apply commits silently retarget and `pull.create` opens an incorrect PR. | Pin the per-step wire payloads: `branchCreate{owner,name,branch:"oxpilot/proposal-<id8>",start:row.branch}`; every `repo.file.*` op `{owner,name,...,message:<synthesized>,branch:"oxpilot/proposal-<id8>"}` with upload entries renaming `content_b64`→`content_base64`; `pull.create{owner,name,title,body,base_ref:row.branch,head_ref:"oxpilot/proposal-<id8>"}`; and state whether the protection-fallback edge is accepted or excluded (e.g. verify the branchCreate'd name is unprotected or fail `apply_failed`). | Resolved (reviewer) |
| R-02 | Major | logical-components.md:27-31; traceability.json TS-7 (:91) vs entities.md:93-97 | The `ops_json` additive chain is under-enumerated: the wiring section names only the ×3-dialect migration + `find_oxpilot_proposals_by_session`. The full chain upstream enumerates is absent here — `ProposalWrite.ops_json` (core/oxpilot.rs:337-342 lacks it), `NewOxpilotProposal.ops_json` (db/oxpilot.rs:1784-1793), `OxpilotProposalRow.ops_json` (:776-788), the `insert_oxpilot_proposal` bind list (:1796-1850), the `PROPOSAL_SELECT_{PG,MYSQL,SQLITE}` column lists (:967-980) + `map_proposal!`, and the `save_proposal` signature/impl (services.rs:625-647). Implemented to this doc's own enumeration, the column exists but `propose_diff` cannot write it and `proposal`/`diff_get` cannot return it — a silently incomplete apply path, and TS-7's `OK` papers over it. | Enumerate the full additive chain (three types + insert column list + three SELECT constants + row mapper + `save_proposal` signature) in the module map/wiring section, or cite entities.md:93-97 explicitly as the enumeration of record; expand the TS-7 note to cover the type/mapper chain not just the column. | Resolved (reviewer) |
| R-03 | Major | logical-components.md:56,91 | The reused-seams table and wiring diagram name `svc.proposals` — no such verb exists on the C5 `SessionService` port (oxpilot.rs:393-522 exposes `get_session`/`save_proposal`/`proposal`/`mark_applied` only; a `proposals`/`list_proposals` verb is nowhere in the codebase). The same row attributes `require_enabled`/`require_verified` to `SessionService` — they are free fns: `oxidean_api_core::oxpilot::require_enabled` (:1324) and `auth::gate::require_verified` (gate.rs:23). As written `diff_list` has no implementable path: calling a nonexistent verb fails to compile and widening C5 is the alternative Q7-B explicitly rejects. | Pin `diff_list`'s backing in both places: `svc.get_session` (ownership) → `state.db.find_oxpilot_proposals_by_session(session_id)` — the additive db verb already pinned for exactly this (upstream functional-spec names it for `diff_list`/ArtifactScope) — or explicitly declare a new C5 verb with its port change; correct the seams row to attribute `require_enabled`/`require_verified` to their actual homes. | Resolved (reviewer) |
| R-04 | Minor | reliability-design.md:110-114; traceability.json NFR4.12 (:64) | "Double-materialization is impossible: `svc.mark_applied` re-enforces ... `status != 'pending'`" attributes the guarantee to a non-atomic check-then-write — `mark_proposal_applied` issues an unconditional `UPDATE ... SET status='applied' ... WHERE id=?` with no `AND status='pending'` predicate on any dialect (db/oxpilot.rs:1898-1929). The actual serialization is the deterministic `oxpilot/proposal-<id8>` branch name colliding at `branchCreate` (the loser's chain dies at `apply_failed:branch_exists` before reaching `mark_applied`); the service recheck only covers sequential retries. The conclusion holds but the mechanism cited does not provide it, and the db write would be racy if the chain order ever changed. | Correct the mechanism text to name the deterministic branch-name + fails-closed `branchCreate` as the concurrency guard (the service recheck as the sequential-retry guard only); optionally tighten `mark_proposal_applied` to `AND status='pending'` + `rows_affected` ⇒ `diff_not_pending` so the claim is literally true. | Resolved (reviewer) |
| R-05 | Minor | logical-components.md:16; nfr-design-questions.md:34 vs rules.md BR3.2 (:134-137) | "16 proc-backed read handlers" counts one more than the 15 handler names listed (5 repo + 2 issue + 2 pull + 6 actions). The missing 16th maps to `repo.get`, BR3.2's "(session-binding precheck)" — an allowlisted proc with no catalog handler (no `repo_get` tool exists in the 23) and no consistent dispatch site: a per-invoke precheck dispatch would break NFR3.2's exactly-one-`rpc_dispatch` pin (performance-design.md:30). The design carries the 10-literal allowlist (:53 "(10+6+1 procs)") without resolving where `repo.get`'s annotated role lands. | Fix the count to 15 and state explicitly whether `repo.get` is allowlisted-but-undispatched in v1 (the session row already carries the binding — reconcile the BR3.2 parenthetical upstream), or name the handler/site that dispatches it and reconcile with the single-dispatch pin. | Resolved (reviewer) |
| R-06 | Minor | security-design.md:229-232; reliability-design.md:37-40 vs nfr-requirements NFR4.5 | The proc-error mapping is ambiguous between two readings: "any other code → verbatim `ToolError{code}`" versus "internal/unknown ⇒ `internal`". Under the first, `auth.email_unverified`/`rpc.bad_input`/`repo.archived`/`db.not_configured`/`oxpilot.internal` become ToolError codes outside the pinned 11-arm taxonomy — contradicting reliability-design's "every failure lands on exactly one typed arm" totality claim and making a U5-side input defect (`rpc.bad_input` from a malformed synthesized request) a named arm instead of `internal`. The verbatim set is never enumerated. | Pin the verbatim set explicitly — recommended {`not_found`, `forbidden`, `invalid_args`} verbatim, ALL other codes ⇒ `internal` + generic message — and reconcile the taxonomy's totality claim with it. | Resolved (reviewer) |
| R-07 | Minor | performance-design.md:76; security-design.md:183-184 | The serialized-args bound `serde_json::to_string(&args).len() <= ACTION_ARGS_MAX` does not pin which string is measured and dispatched: the re-serialized validated `Value` vs the raw `call.args_json`. If the raw string is passed to `dispatch_action` while the re-serialized form is bounded, a whitespace-padded raw >2 KiB slips the check and hits the gateway's `head_bounded` truncation mid-JSON — the intended `invalid_args` reject degrades into a truncated/unparseable stored-args connector failure. | Pin "bound and send the SAME string": re-serialize the validated args, measure that string, and pass exactly that string to `dispatch_action`'s `args` parameter. | Resolved (reviewer) |
| R-08 | Minor | observability-design.md:82-87; nfr-design-questions.md:146-155 | `emit_card`'s correctness rests on an unstated assumption: `turns().get(ctx.session_id)` resolves the CURRENT map entry, which is the invoking turn only because emits run synchronously inside the awaited `invoke` — `release` post-dates the turn task and `drain_and_chain` claims the successor only after release under the admission lock (runtime.rs:1062-1149). A future detached/spawned emit would land on a SUCCESSOR turn's replay buffer — card misattribution, a strictly worse outcome than the documented silent drop. The helper's contract never pins the synchronous-emission requirement. | Add one line pinning card emits to synchronous execution inside `invoke` (no detached emission), so `get()` can only ever resolve the invoking turn. | Resolved (reviewer) |
| R-09 | Minor | logical-components.md:25-26 | "installed onto `OxpilotRuntimeSlot.tools` where the slot is constructed (runtime.rs)" mislocates the install on two axes: the slot is constructed in api-core `OxpilotRuntime::new`/`from_slot` (oxpilot.rs:1163-1187) and the settings.rs rebuild paths — not `crate::runtime` — and `OxpilotTools::new(state.clone())` needs a finished `Arc<AppState>` that cannot exist inside `AppState::new` (which itself calls `OxpilotRuntime::new` at :93's field init). No `install_tools` setter exists. The workable site is post-`AppState::new` `state.oxpilot.slot.write().await.tools = Some(...)` at shell assembly beside `install_sessions`/`install_gateway`; the rebuild-time carry is already shipped (settings.rs:463/471/498/550/562/670/684). | Pin the actual install site (shell assembly, post-`AppState::new`, via the `pub` field or a new `install_tools` mirroring `install_gateway`) and drop the `(runtime.rs)` cite. | Resolved (reviewer) |
| R-10 | Nit | security-design.md:30-33; nfr-design-questions.md:73-75 vs entities.md:54 | `CatalogEntry.handler` deviates from the READY upstream's pinned type `fn(&OxpilotTools, ToolCtx, Value) -> Result<ToolOutput, ToolError>` (entities.md:54) to `fn(&OxpilotTools, &InvokeCtx, Value) -> BoxFuture<Result<ToolOutput, ToolError>>` — substituting an NFR-design-only `InvokeCtx{ctx, session, entry}` and adding the BoxFuture wrapper (necessary for fn pointers to async fns). A reasonable refinement, but it is a deviation from a pinned contract type that no artifact flags. | Flag the handler-signature change as a deliberate deviation in the design, or carry an upstream amendment to entities.md:54; acknowledge `InvokeCtx` as an NFR-design-introduced type. | Resolved (reviewer) |
| R-11 | Minor | security-design.md:176-178,217-224; logical-components.md:76-79; performance-design.md:36 | The new "apply-name precheck" pins the REQUIREMENT (`oxpilot/proposal-<id8>` verified UNPROTECTED else `apply_failed`) but never the mechanism — and the choice has an unsafe fork: the exact predicate the commit path re-evaluates per op is `direct_push_allowed`/`crate::protection::{effective_for_branch,evaluate_push}` on the caller's capability (files.rs:342-354,398-404), or equivalently a `repo.file.commitPolicy` dispatch (`RepoFileCommitPolicyResponse{direct_commit_allowed,requires_pr}`, repo_types.rs:1407-1415, dispatch.rs:68). A hand-rolled `list_branch_protection_rules` + pattern match instead would drift from `evaluate_push` semantics (capability bypass/`enforce_admins`) and can yield a false "unprotected" — re-admitting the very silent retarget R-01 excluded. Additionally the "commits can never silently retarget" claim is overbroad: the commit path re-evaluates protection PER OP, so a rule matching the name created between precheck and a later op still falls back on that op — while `RepoFileCommitResponse{branch,created_branch,pr_number}` (:1419-1429) echoes the actual commit target and would let the apply detect the retarget (`resp.branch != "oxpilot/proposal-<id8>"` ⇒ `apply_failed`), but no per-op echo check is pinned. | Name the precheck predicate (recommended: `repo.file.commitPolicy` dispatch on the caller ctx, or `direct_push_allowed`-equivalent — never a hand-rolled rule match), and either pin the per-op `resp.branch`/`pr_number` echo check or explicitly bound the mid-apply rule-creation residual; if implemented as a dispatch, account for it in the ~53 count. | New |
| R-12 | Minor | reliability-design.md:113-122; traceability.json NFR4.12 (:64); security-design.md:205-206 | The pinned surfaced label `apply_failed:branch_exists` is unproducible under the shipped code path. `repo.branchCreate` does no existence pre-check (lifecycle.rs:1326-1362); `git branch` on a taken name exits nonzero ⇒ `GitError::Process` (cli.rs:2609-2615/:322-332) ⇒ `map_git_err`'s catch-all `repo.git_failed` (repo/mod.rs:78-87) — `repo.branch_exists` is produced ONLY inside the file-commit path (`file_git_err`'s Conflict arm files.rs:59-61 + the rev_parse early check :416-419), never by `branchCreate`. The taxonomy's own convention `apply_failed:<step>` "naming the proc" (:28,:113) would surface `apply_failed:branchCreate`, and `branch_exists` names no chain step (chain = branchCreate → file ops → pull.create). Minting the pinned label literally requires an unpinned existence pre-check (`repo.refs`/`rev_parse` on the caller ctx) or stderr-message sniffing — which the design's structural-decode posture forbids. The concurrency guard itself holds (the collision fails closed either way) — only the promised error label is wrong. | Relabel to the convention-true code (`apply_failed:branchCreate` / whatever step label is chosen) or pin the existence pre-check that legitimately produces `apply_failed:branch_exists` (e.g. a pre-dispatch `repo.refs`/rev_parse probe on the caller ctx — which also covers the retry-leftover case at :117). | New |
| R-13 | Nit | security-design.md:326 vs :229-234; performance-design.md:76 | The redaction call-sites table still says "U5 hands the raw string to `dispatch_action`" — stale wording contradicting the R-07 pin in the same file ("passes exactly that [re-serialized validated] string … never the raw `call.args_json`") and performance-design's identical pin. Reads as two different strings crossing the gateway boundary. | Reword to "the measured re-serialized args string" (or "the full args string — the re-serialized one pinned above") so the row can't be read as handing `call.args_json` raw. | New |
| R-14 | Nit | security-design.md:61-63 | "The security mechanism for the ~30 control/mutation Actions procs the registry exposes is ABSENCE from the table" — as written, "exposes" asserts the registry DOES surface those procs, inverting the pinned mechanism (they are ABSENT, not exposed). The BR6.3/NFR1.4 cite rescues the intent, but the sentence literally contradicts itself in a security-critical claim. | One-word fix: "the ~30 control/mutation Actions procs the registry OMITS" (or "…keeps out"). | New |

> R-01 Reviewer note: FIXED: the per-step wire payloads are now pinned verbatim — `branchCreate{owner,name,branch:"oxpilot/proposal-<id8>",start:row.branch}` with `start` called out as REQUIRED (repo_types.rs:1230-1232 default trap named); every `repo.file.*` op carries `{owner,name,...,message:<synthesized>,branch:"oxpilot/proposal-<id8>"}` (non-Option `message` :1335/:1355/:1376 + flattened `RepoFileCommitOptions.branch` :1273-1274); upload `files[]` entries rename `content_b64`→`content_base64` (:1367); `pull.create{owner,name,title,body,base_ref:row.branch,head_ref:"oxpilot/proposal-<id8>"}` matches `CreatePullRequest` :129-144; and the protected-branch fallback is explicitly EXCLUDED via an apply-start unprotected check ⇒ `apply_failed` (:176-178,:217-224). The check's mechanism is unpinned — see R-11.; Required action: —

> R-02 Reviewer note: FIXED: the wiring section now enumerates the full additive chain — `ProposalWrite.ops_json` (core/oxpilot.rs:337-342 confirmed lacking it), `NewOxpilotProposal.ops_json` (db:1784-1793), `OxpilotProposalRow.ops_json` (:776-788), the `insert_proposal` column/bind list (:1796-1850), `PROPOSAL_SELECT_{PG,MYSQL,SQLITE}` (:967-980), `map_proposal!` (:874), `save_proposal` signature/impl (services.rs:625-647) — and cites entities.md:93-97 as the enumeration of record; TS-7's note now covers the type/mapper chain, not just the column.; Required action: —

> R-03 Reviewer note: FIXED: `diff_list` now rides `svc.get_session` (ownership) → `state.db.find_oxpilot_proposals_by_session` in both the wiring diagram (:72-73) and the reused-seams row (:123); the row explicitly notes no `proposals`/`list_proposals` verb exists on C5 (verified :393-522) and attributes `require_enabled`/`require_verified` to their real homes — `oxidean_api_core::oxpilot::require_enabled` (:1324) and `auth::gate::require_verified` (gate.rs:23).; Required action: —

> R-04 Reviewer note: FIXED: the concurrency guard is now correctly attributed to the deterministic `oxpilot/proposal-<id8>` branch name colliding at `branchCreate` (chain dies before `mark_applied`); `svc.mark_applied`'s recheck (services.rs:665-666) is scoped to SEQUENTIAL retries and explicitly called out as a non-atomic check-then-write (`mark_proposal_applied`'s unconditional `UPDATE ... WHERE id=?` confirmed :1898-1929, no `AND status='pending'` on any dialect); the optional `AND status='pending'` + `rows_affected`⇒`diff_not_pending` tightening is recorded. Residual on the surfaced label: R-12.; Required action: —

> R-05 Reviewer note: FIXED: the count is now "15 handlers" matching the 15 names listed; `repo.get` is explicitly declared allowlisted-but-undispatched in v1 (no `repo_get` catalog entry; the session row already carries the binding; a per-invoke precheck dispatch would break NFR3.2's exactly-one-`rpc_dispatch` pin); the BR3.2 "(session-binding precheck)" annotation's upstream reconciliation is acknowledged.; Required action: —

> R-06 Reviewer note: FIXED: the verbatim set is now pinned to exactly {`not_found`, `forbidden`, `invalid_args`}; ALL other codes — `internal`/unknown, `rpc.bad_input`, `auth.*`, `db.*`, `repo.*`, `oxpilot.*` — map to `internal` + generic message; a U5-side input defect (`rpc.bad_input` from a malformed synthesized request) is explicitly `internal`, and the taxonomy's totality claim is reconciled with it.; Required action: —

> R-07 Reviewer note: FIXED: both places now pin "bound and send the SAME string" — re-serialize the validated args, measure THAT string, and pass exactly that string to `dispatch_action`'s `args` param, never the raw `call.args_json` (the whitespace-pad → `head_bounded` mid-JSON truncation path at gateway.rs:590 is named). Residual wording: one stale "raw string" elsewhere — R-13.; Required action: —

> R-08 Reviewer note: FIXED: card emits are now PINNED to synchronous execution inside the awaited `invoke` — no detached/spawned emission — with the mechanism stated (`release` post-dates the turn task; `drain_and_chain` claims the successor only after release under the admission lock, runtime.rs:1062-1149 — verified :1074 release precedes :1122 claim under `_g`) and the detached-emit misattribution consequence named.; Required action: —

> R-09 Reviewer note: FIXED: the install site is now pinned correctly — `Arc::new(OxpilotTools::new(state.clone()))` onto the `pub` `OxpilotRuntimeSlot.tools` field (:1046) at shell assembly POST-`AppState::new` beside `install_sessions`/`install_gateway` (main.rs:143-144 confirmed), either a direct `state.oxpilot.slot.write().await.tools = Some(...)` or a new `install_tools` mirroring `install_gateway`; the why is stated (slot built inside `OxpilotRuntime::new`/`from_slot` :1163-1187, which `AppState::new` calls at field-init :257 — no finished `Arc<AppState>` exists then); the `(runtime.rs)` miscite is dropped and the shipped settings-rebuild carry (`prior.tools.clone()` :463/:471/:498) is cited.; Required action: —

> R-10 Reviewer note: FIXED: the handler signature is now flagged verbatim as a DELIBERATE DEVIATION from the pinned `fn(&OxpilotTools, ToolCtx, Value) -> Result<ToolOutput, ToolError>` (entities.md:54), with `InvokeCtx{ctx, session, entry}` acknowledged as an NFR-design-introduced type and the `BoxFuture` necessity stated — in both the registry listing and Q2-A.; Required action: —

### Summary

All ten iteration-1 findings are resolved — every file:line claim
re-verified against shipped source and the new pins (wire payloads,
verbatim error set, ops_json chain, `diff_list` backing, install site,
sync-emission, handler-sig deviation flag) are accurate. The fixes
introduced no regressions; the four new findings are narrow
under-specifications in the freshly-added `diff_apply` precheck/error
surface — the unpinned unprotected-check mechanism (with a drift-prone
wrong implementation available), the unproducible
`apply_failed:branch_exists` label, and two wording stales — none of
which blocks implementation. READY with Minor cleanups.
