# Functional Design Questions — oxpilot-tools

Unit: `oxpilot-tools` (U5, kind `service`, deploy: embedded in API) —
owns `OxpilotTools`: the model-callable tool catalog — repository read
tools (file/tree/branch/log/issues/PRs), read-only Actions run/job/log
tools, ProposedDiff creation + patch view, explicit-confirm branch + PR
materialization with the PR-body provenance line, agent-created-artifact
comments only (FR7.5), per-tool authorization, write-tool audit, and the
computer-use `exec_shell`/`fs_read`/`fs_write` tool definitions,
validation, and dispatch into `ConnectorGateway`'s action queue.
depends_on: U3 (C6 `ToolExecutor` seam + the turn loop that consumes
`catalog()`/`invoke`), U6 (C7 `dispatch_action`/`action_status` + grant
checks). U8 renders the diff/consent cards this unit's calls mint.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U5 — "the tool
  catalog the model can call — repo read tools (file/tree/branch/log/
  issues/PRs — names verified against the dispatch registry in
  contract-design), actions run/job/log reads (read-only), ProposedDiff
  creation + patch view, branch+PR materialization on explicit user
  confirm (provenance line in PR body), agent-created artifact comments
  only (FR7.5). Per-tool authorization + write-tool audit emission …
  the tools never reach a target directly; they enqueue `Action` rows
  the paired connector claims."
- [story-map] `unit-of-work-story-map.md` — US2.1 → U5 (U3, U8);
  US2.2 → U5; US2.3 → U8 (U5); US3.1 → U5 (U3); US3.2 → U5 (U3);
  US4.3 → U7 (U5, U6); US5.3 → U6 (U5); US5.5 → U1 (U2, U5, U6).
- [contract] `inception/contract-design/contract-summary.md` §C1 —
  `oxpilot.diff_*` family (`diff_list`, `diff_get`, `diff_apply` =
  confirm → branch+PR; payload `ProposedDiff{id, session_id, base,
  files[{path, patch}], status}`; errors `[forbidden, not_found,
  diff_not_pending]`); §C6 — `ToolExecutor{catalog, invoke}` +
  `ToolCtx{session_id, user_id, repo_id, proposal_id?}`, "write-class
  tools emit audit rows; read-class do not" and "exec_shell/fs_* are
  write-class and route through C7, never direct"; §C7 —
  `dispatch_action(user_id, session_id, target_id, kind, args) ->
  Result<Action, Denied{reason, grant_id?}>` + `action_status` ->
  `ActionView` (shipped signatures prepend `client: &ClientMeta` —
  server-side callers pass `&ClientMeta::default()`, BR7.1); C1 open
  question pinned to this unit: "Exact repo-read
  tool names vs. existing `vcs.*`/`actions.*` procs — resolved during U5
  functional-design against the live dispatch registry".
- [req] `inception/requirements-analysis/requirements.md` — FR2.3
  (repo context through typed RPC tools), FR3.1–FR3.3 (propose / confirm-
  materialize / never autonomous), FR4.1–FR4.3 (read-only CI surface),
  FR5.3 (exec/fs-only caps), FR5.5 (secrets-redacted audit detail),
  FR7.1–FR7.5 (tool surface partition).
- [stories] US2.1 (AC2.1.1–AC2.1.2), US2.2 (AC2.2.1–AC2.2.4), US2.3
  (AC2.3.1), US3.1 (AC3.1.1–AC3.1.3), US3.2 (AC3.2.1), US4.3
  (AC4.3.1–AC4.3.5), US5.3 (AC5.3.1–AC5.3.2), US5.5 (AC5.5.2, AC5.5.4).
- [domain] `inception/domain-design/components.md` §OxpilotTools —
  "Read tools route through rpc_dispatch (never direct DB/git); write
  tools are proposal-scoped (diffs, agent-created issue/PR comments
  only — FR7.5); exec/fs tools forward to ConnectorGateway and fail when
  no live consented target exists. Every call produces a ToolCall row
  (inputs summarized, secrets redacted)." The single authz seam,
  including PR materialization.
- [u3-fd] `construction/agent-runtime/functional-design/` — WF2 turn
  loop consumes `catalog()` at turn start, mints effective `call_id`,
  persists cards durable-first, races `invoke` under 60 s against
  cancel + turn wall, bounds at MAX_TOOL_ROUNDS=8.
- [u6-fd] `construction/connector-gateway/functional-design/` — WF2:
  "U5's tool output = 'consent requested (grant_id)' → U8 renders the
  consent card from `grant_get`"; `Denied` rides `AppError.data.denied`
  — U5 distinguishes consent refusals by the marker, never by type or
  message sniffing.
- [shipped] `crates/oxidean-api-core/src/oxpilot.rs` — `ToolSpec`
  (l.58: `{name, description, parameters_json}` — note: no read|write
  class field, the C6 sketch lists it but the shipped struct is
  three-field), `ToolCall`, `ToolCtx` (l.85: `proposal_id:
  Option<String>`), `ToolOutput`, `ToolError`, `NullTools`,
  `denied()`/`denied_reason` (l.550–576),
  `CONNECTOR_CAPABILITIES` (l.547), `ActionView` (l.652),
  `ConnectorGateway::dispatch_action` (l.736), `OxpilotRuntimeSlot.
  tools` (l.1046) carried through every rebuild path;
  `crates/oxidean-api-oxpilot/src/runtime.rs` — `invoke_call` mints
  `ToolCtx` with `proposal_id: None` (l.460); bounds
  `ARGS_SUMMARY_MAX`/`OUTPUT_SUMMARY_MAX` = 2 KiB, `TOOL_OUTPUT_MAX` =
  32 KiB, `TOOL_CALL_TIMEOUT` = 60 s, `MAX_TOOL_ROUNDS` = 8;
  `emit_frame` `pub(crate)` (l.113) — buffer + fanout, same seam
  `message_queue` uses for `card{queued}` (session.rs:331);
  `crates/oxidean-api-oxpilot/src/connector/gateway.rs` — dispatch
  check order (l.553–693): session → target lifecycle+liveness → caps
  → live `action_session` grant → pending-grant dedupe/mint →
  `denied(consent_required, grant_id)`;
  `crates/oxidean-api-core/src/app.rs` — `build_rpc_ctx_with_session`
  (l.537) + `AppState.rpc_dispatch`;
  `crates/oxidean-api-core/src/audit.rs` — `record`/`record_with`
  (post-commit, failure-swallowing).
- [procs] Verified `PROCEDURES` consts — `repo.tree`, `repo.blob`,
  `repo.refs`, `repo.commits`, `repo.commit`, `repo.get`,
  `repo.branchCreate`, `repo.file.{create,update,delete,rename,upload}`
  (oxidean-api-repo); `issue.list`, `issue.get`,
  `issue.comments.{list,create}` (oxidean-api-issues); `pull.list`,
  `pull.get`, `pull.create`, `pull.comments.{list,create}`
  (oxidean-api-pulls); `repo.actions.{listRuns, getRun, getJob,
  getJobLog, listWorkflows, jobAnnotations, listArtifacts,
  getArtifact}` (oxidean-api-actions). `oxpilot.diff_*` is NOT yet
  registered (oxidean-api-oxpilot PROCEDURES stops at grant_respond) —
  U5 adds it.
- [db] `crates/oxidean-db` — `oxpilot_proposals` (0087:
  `{id, session_id, title, base, diff_body, status, pr_ref,
  created_at}`, `status ∈ pending|applied`, files derive by splitting
  `diff --git` headers); verbs `insert_oxpilot_proposal`,
  `find_oxpilot_proposal` (by id), `mark_oxpilot_proposal_applied` —
  no session-scoped list verb exists today (gap, Q4/Q10).

### Q1. Catalog naming — what do the model-facing tool names look like?

C1's open question pins this to U5. The provider adapters serialize
`ToolSpec.name` verbatim into each provider's tool schema — OpenAI and
Anthropic tool names must match `^[a-zA-Z0-9_-]{1,64}$` (no dots), so
dotted proc names like `repo.tree` cannot be the tool ids.

- A. Flat `domain_verb` underscore names mirroring the backing proc's
  tail — `repo_tree`, `repo_file_read`, `repo_log`, `repo_commit_get`,
  `repo_refs`, `issue_list`, `issue_get`, `pull_list`, `pull_get`,
  `actions_list_runs`, `actions_get_run`, `actions_get_job`,
  `actions_get_job_log`, `actions_list_workflows`,
  `actions_job_annotations`, `propose_diff`, `proposal_get`,
  `targets_list`, `exec_shell`, `fs_read`, `fs_write`,
  `action_status`, `pr_comment` — provider-legal on every adapter,
  greppable back to the backing proc, and `exec_shell`/`fs_read`/
  `fs_write` intentionally identical to the C7 kind vocabulary so the
  dispatch passes name → kind verbatim.
- B. Dotted `repo.tree`-style names matching the proc strings: fails on
  OpenAI/Anthropic adapters outright (dot is outside the legal name
  charset); would need a per-adapter rename shim for zero benefit.
- C. Semantic names unrelated to proc names (`read_file`, `search_ci`):
  hides the backing proc from reviewers and makes the audit/grep story
  worse for no provider benefit.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

### Q2. Tool classes + per-tool authorization — how is the catalog split?

C6 pins "write-class tools emit audit rows; read-class do not" and the
shipped `ToolSpec` has no class field — so the class lives in the
executor's internal registry, not on the wire.

- A. Static registry of catalog entries `{spec, class: read|write,
  handler}` compiled into `OxpilotTools`: `catalog()` returns the
  `ToolSpec` half only; `invoke` looks the call up in the same table.
  Write-class = `propose_diff`, `pr_comment`, `exec_shell`, `fs_read`,
  `fs_write` (plus the user-facing `diff_apply` proc, which audits at
  the proc layer). Every invoke re-resolves `get_session(user_id,
  session_id)` for repo binding + liveness, then the tool's own
  authorization rule (read: session-repo-scoped `rpc_dispatch` read;
  write: class-specific gate — proposal write, pr_ref-binding, or C7
  dispatch).
- B. One flat catalog where every tool audits: noisy — read calls would
  flood `audit_events` with non-events the C6 rule explicitly exempts.
- C. Per-call dynamic class assignment: the class is a property of the
  tool definition, not the args; dynamic assignment invites a model to
  talk a write tool into read treatment.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

### Q3. ProposedDiff payload — what does `propose_diff` accept and store?

The stored row is `{title, base, diff_body}` and materialization needs
per-file ops the diff text alone can't express (deletes, binary-safe
content). There is no server-side `git apply` seam, and `repo.compare`/
`GitBackend::diff` only diff committed refs — so the tool must receive
both review text and apply instructions.

- A. Args `{title, diff_body}` only — the model supplies the
  canonical unified diff; `diff_apply` re-derives the ops by parsing
  the stored `diff_body` at apply time (hunk `+` lines reconstruct
  post-image text for create/update; `deleted file`/`rename` headers
  carry delete/rename). Zero schema churn — but text-hunk parsing is
  lossy for binary files and exact-byte fidelity, and a malformed
  hunk fails at APPLY time, after the user already reviewed it.
- B. Args `{title, files: [{path, op, content_b64?, new_path?}],
  diff_body}` — `base` is NOT a model arg in v1: the row's `base` is
  always the session's bound branch, resolved server-side (BR4.4),
  so a model can never point a proposal at an arbitrary ref. The tool
  persists BOTH the diff text (into
  `diff_body` for review) and the ops (into the row's args — requires
  an additive `ops_json` column on `oxpilot_proposals`, else the apply
  path can't recover contents). This is the honest two-payload shape:
  the apply path uses `ops_json` verbatim; the diff text is review-
  only. Coherence check as in A. Cost: one additive column in all
  three dialects + C1's `ProposedDiff` stays unchanged (ops are
  internal).
- C. Model supplies `diff_body` only; apply parses the unified diff and
  reconstructs file contents by `repo.blob`-reading each base file and
  applying hunks server-side: needs a real patch-application
  implementation (context matching, fuzz, binary hunks) — a much
  bigger correctness surface than passing post-image bytes.
- D. Model supplies ops only; the server computes the canonical diff by
  fetching base blobs and diffing in-process: needs a new diff
  dependency (none is in the workspace) or new `GitBackend` plumbing
  verb — real fidelity gain but a crate-level seam U5 doesn't own.
- X. Other (please specify)

[Answer]: B

### Q4. Materialization path — how does branch+PR get created?

AC2.2.1 pins "explicit user action creates branch + PR via existing
RPC" and C1 gives `oxpilot.diff_apply` to the user — so the catalog
carries NO model-facing apply verb at all.

- A. `oxpilot.diff_apply{proposal_id}` is a USER proc (SessionOnly,
  require_enabled + require_verified + owned session): fetch proposal
  (pending only, else `diff_not_pending`) → `repo.branchCreate{owner,
  name, branch: oxpilot/proposal-<id8>, start: base}` → file ops via
  `repo.file.upload` (create/update batch) + `repo.file.delete` /
  `repo.file.rename` per remaining op → `pull.create{base_ref,
  head_ref, title, body: summary + provenance line}` →
  `mark_applied(user_id, proposal_id, pr_ref)` → return
  `{proposal_id, pr_ref}`.
  All through `ctx.rpc_dispatch` on the user's own ctx so repo ACL,
  protection hooks, and archived checks apply verbatim. No model-
  callable name maps to this path — AC2.2.3 falls out structurally.
- B. A model-facing `apply_proposal` tool that goes through the C7
  consent machinery: mixes two consent models; the diff card's Apply
  button IS the consent mechanism C1 ships — a second gate duplicates
  UX without adding assurance.
- C. Let the model call `repo.branchCreate`/`pull.create` as catalog
  tools directly: puts write-capable repo procs on the model surface —
  the model could then push branches without the diff card's explicit
  confirm, violating AC2.2.3.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

### Q5. Actions read set — which `repo.actions.*` procs are exposed?

AC3.1.3 pins "no rerun/cancel/approve control" — the catalog must be a
closed read list, and several shipped procs are write/control surface
that must never appear.

- A. Exactly six tools mapped to read-only procs:
  `actions_list_runs`→`repo.actions.listRuns`,
  `actions_get_run`→`repo.actions.getRun` (run + jobs),
  `actions_get_job`→`repo.actions.getJob`,
  `actions_get_job_log`→`repo.actions.getJobLog`,
  `actions_list_workflows`→`repo.actions.listWorkflows`,
  `actions_job_annotations`→`repo.actions.jobAnnotations`. The control
  families (`cancelRun`, `rerunRun`, `approveRun`, `approveJob`,
  `dispatchWorkflow`, `dispatchRepository`, `delete*`, `set*`,
  `secrets.*`, `variables.*`, `environments.*`) never enter the
  registry — the registry is the allowlist, not a filter on the full
  proc set.
- B. Expose the artifact reads too (`listArtifacts`, `getArtifact`):
  useful for CI debugging but artifact payloads can be large/binary —
  deferrable; the AC only pins runs/jobs/logs.
- C. Mirror every `repo.actions.*` read-shaped proc indiscriminately:
  mixes management reads (`listRunners`, `getDefaultPermissions`) the
  agent doesn't need and widens the surface for no story.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

### Q6. Computer-use dispatch — arg validation, target resolution, denial mapping?

`dispatch_action` takes `target_id` but the session row has no target
column — the model can't fabricate it and the session can't pin it.

- A. Tool args: `exec_shell{command, target_id?}`,
  `fs_read{path, target_id?}`, `fs_write{path, content_b64,
  target_id?}`; a `targets_list` read tool + the catalog descriptions
  let the model discover ids. `target_id` absent ⇒ resolve the user's
  targets via `gateway.list_targets`: exactly one `connected` target ⇒
  use it; zero ⇒ `ToolError{no_target}`; multiple ⇒ `ToolError
  {ambiguous_target}` telling the model to pass `target_id`. JSON
  Schema validation before dispatch; serialized args bounded ≤
  ACTION_ARGS_MAX (2 KiB) + redacted. Denial mapping: `data.denied.
  reason == consent_required` ⇒ emit `card{card_kind:"consent",
  ref_id: grant_id}` via `runtime::emit_frame` on the live turn (the
  `message_queue` `card{queued}` precedent) + `ToolOutput` "consent
  requested — grant <id> pending" (U6-WF2's pinned shape); the other
  three reasons ⇒ `ToolError{action_denied}` carrying the reason
  (AC4.3.4's visible error). `action_status{action_id}` is a read
  tool for later polling.
- B. Require `target_id` always: honest but brittle — the model invents
  or omits ids when it can't enumerate them; the single-target
  fallback covers the common case and `targets_list` makes ambiguity
  resolvable.
- C. Add a `target_id` column to `oxpilot_sessions` and bind at
  session create: schema churn for all three dialects to express what
  `list_targets` already answers; the create dialog's optional pick
  (FR1.3) predates any shipped binding.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

### Q7. PR provenance — what exactly does the generated PR body carry?

AC2.2.4 requires the body to disclose `/oxpilot` provenance + a session
reference — deliberately carved out from the repo's no-AI-attribution
rule (that rule governs development tooling metadata, not this shipped
product feature).

- A. Fixed line appended to the agent-authored body:
  `\n\n---\n_Produced by an /oxpilot agent session: {session_id}
  (proposal {proposal_id})._` — names the product surface and carries
  both refs the operator needs to trace the PR back to its review
  context; no model/vendor names, no `Co-authored-by`/`Generated with`
  trailers (those stay forbidden even here — provenance is a fact
  about the session, not an author claim).
- B. A `X-Oxpilot-Session:` git trailer + a mention in the title:
  trailers on the merge/squash commit body are brittle under rebase
  and the AC pins the disclosure on the PR BODY specifically.
- C. Rely on `pr_ref` bookkeeping only (link exists server-side, PR
  says nothing): fails AC2.2.4 — Riley must see provenance as a normal
  PR.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

### Q8. `ToolCtx.proposal_id` — how does the field get bound?

The shipped runtime mints `ToolCtx{proposal_id: None}` on every call
(runtime.rs:460) — the C6 optional field is currently dead.

- A. v1 ignores it: proposal-binding tools (`propose_diff` superseding
  a prior proposal, `proposal_get`) take `proposal_id` as an explicit
  arg validated against `svc.proposal(user_id, proposal_id)` — the
  ownership check lands in the tool, not the ctx. Note the scope
  precisely: `svc.proposal`/`owned_proposal` is USER-scoped — it
  proves the row's session belongs to the user, not that it is THIS
  session (services.rs:140–153) — so the tool additionally pins
  `row.session_id == ctx.session_id` post-fetch ⇒ `not_found` for
  every session-scoped read (BR4.6). Document the
  contract-vs-code gap; filling the ctx field is a U3 runtime change
  (which proposal? the session can hold many pending rows) deferred
  until a consumer actually needs ambient binding.
- B. U5 patches `invoke_call` to fill `proposal_id` from the session's
  latest pending proposal: crosses the U3/U5 unit boundary for a
  speculative use; ambient binding also lets a model silently mutate a
  proposal the user is reviewing — explicit args are the safer v1.
- C. Remove `proposal_id` from `ToolCtx`: breaking a shipped C6 field
  for cosmetics — the Option shape already costs nothing.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

### Q9. Artifact comments — which proc and what binding?

FR7.5/AC5.5.2 scope agent comments to "artifacts the agent itself
created with user approval" — in v1 that set is exactly the PRs this
session's applied proposals materialized.

- A. One write-class tool `pr_comment{number, body}`: resolve the
  session's applied proposals, require `number` to match a recorded
  `pr_ref` (else `ToolError{forbidden}` — no cross-artifact comment is
  possible because the model can't name an artifact it didn't cause),
  then `pull.comments.create{owner, name, number, body}` — already
  shipped in oxidean-api-pulls, and called WITHOUT `path`/`side`/`line`
  so the comment lands as a general PR conversation comment (the anchor
  fields are optional; omitting them is the conversation path, not a
  separate proc). The forge shares the issue NUMBER COUNTER
  (`pull.create` calls `allocate_next_issue_number`) but NOT the
  tables — PRs live in `pull_requests`/`pull_comments`, and
  `issue.comments.create` resolves through the issues table so it
  would answer `issue_not_found` on every real PR number. Line-
  anchored review comments (same proc, anchors supplied) stay out of
  v1.
- B. `issue.comments.create` for the conversation write: dead on
  arrival — it resolves its target via `load_issue_in_repo` →
  `find_issue_by_repo_number` on the ISSUES table, so every call on a
  real PR number returns `issue_not_found`. PRs share the issue
  number counter, not the issue tables.
- C. Let the model comment on any issue/PR in the bound repo: violates
  FR7.5 verbatim — "only on artifacts the agent itself created".
- D. Not decided.
- X. Other (please specify)

[Answer]: A

### Q10. `oxpilot.diff_*` wire shapes — payloads, pr_ref format, list source?

C1 pins `diff_list`, `diff_get`, `diff_apply` + payload `ProposedDiff
{id, session_id, base, files[{path, patch}], status}` but not the
inputs/returns or the `pr_ref` format; and `oxpilot_proposals` has no
session-scoped list verb today.

- A. `diff_list{session_id} -> {diffs: [ProposedDiff]}` — needs a new
  additive DB verb `find_oxpilot_proposals_by_session` (service-layer
  pass-through like `list_tool_calls`; flagged as a C5 additive seam
  U5 must add under contract ownership rules — C5 is U2's, additive
  is permitted with a deviation note); `diff_get{proposal_id} ->
  ProposedDiff` (+ `pr_ref` when applied — additive field) where
  `files[]` derives by splitting `diff_body` on `diff --git` headers
  (the table's own convention, 0087); `diff_apply{proposal_id} ->
  {proposal_id, pr_ref}`; `pr_ref` format `owner/name#number` — the
  same `owner/name` the session binds plus the pull number, parseable
  and URL-derivable. All errors ride C1's `[forbidden, not_found,
  diff_not_pending]` + `rpc.bad_input`.
- B. `pr_ref` as a full URL `{base}/owner/name/pulls/number`: ties a
  stored field to deployment URL shape; the `owner/name#number` form
  is derivable everywhere without env knowledge.
- C. `diff_list` derives from `list_tool_calls` scanning for
  `propose_diff` calls: the card row's args_summary is a bounded
  excerpt, not a queryable relation — wrong shape, loses status and
  pr_ref.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

### Q11. Internal dispatch context — how do read tools build their `RpcCtx`?

`invoke` receives `ToolCtx`, not `RpcCtx` — read tools still have to
reach `repo.*`/`issue.*`/`pull.*`/`repo.actions.*` through the proc
layer so repo ACL, feature flags, and archived checks apply (the
components.md pin: "never direct DB/git").

- A. Synthesize a session-equivalent `RpcCtx` per call via
  `build_rpc_ctx_with_session(state, Some(ResolvedSession{session_id:
  "oxpilot:<sid>", user_id: ctx.user_id, remember_me: false,
  expires_at: now+1h}), ClientMeta::default())` then
  `(ctx.rpc_dispatch)(&mut ctx, RpcRequest{procedure, input})` — the
  shipped seam built for exactly this shape; `require_verified`-class
  proc guards still execute; no fabricated IP/UA. The synthesized id
  is namespaced (`oxpilot:`) so it can never collide with a real
  session row.
- B. Hold a long-lived `RpcCtx` on `OxpilotTools`: wrong lifetime —
  the ctx is per-user/per-session, the executor is global; caching
  would leak identity across sessions.
- C. Call the DB/service verbs directly: violates the pinned "route
  through rpc_dispatch" rule and silently skips proc-level
  authorization the UI path relies on.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

### Q12. Write-tool audit — event names and detail bounds?

C6 says write-class tools emit audit rows; the existing `oxpilot.*`
event family uses `<noun>_<verb>` names (`oxpilot.action_dispatched`,
`oxpilot.connect_grant_allowed`).

- A. Three events owned by U5 — `oxpilot.proposal_created{session_id,
  proposal_id, title, file_count}` on `propose_diff` commit;
  `oxpilot.diff_applied{session_id, proposal_id, branch, pr_ref}` on
  `diff_apply` success; `oxpilot.artifact_comment{session_id, pr_ref,
  comment_id}` on `pr_comment` commit — all post-commit,
  best-effort via `audit::record_with(&state.db,
  &ClientMeta::default(), actor, event, target, detail)` (actor
  `Some((user_id, username))` via `find_user_by_id`, `None` fallback),
  detail bounded to ids/counts/refs
  (never diff bodies, file contents, commands, or args). exec/fs get
  NO U5 row — `oxpilot.action_dispatched` already covers them (U6).
- B. One generic `oxpilot.tool_write{tool, ...}` event: loses the
  per-effect naming convention the operator greps for and duplicates
  U6's granularity choice for no benefit.
- C. Audit every tool call including reads: contradicts the C6 class
  rule verbatim; the durable card row already records reads.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

### Q13. Test/registration strategy — what proves the surface stays closed?

NFR9 wants registration coverage; AC5.5.1 pins a closed-set test for
`oxpilot.*` procs in U1 — the tool catalog needs the same shape of
proof.

- A. Pinned tests at three seams: (1) a registry test asserting the
  catalog is exactly the 23 pinned names and that no entry maps to a
  control/write proc (the Actions mutation vocabulary asserted absent,
  not filtered); (2) per-tool handler tests with a stubbed
  `rpc_dispatch`/`ConnectorGateway` asserting binding (foreign session
  ⇒ `not_found`), schema rejection, denied-marker mapping, and
  read-class/no-audit vs write-class/audit; (3) `diff_apply` negative
  tests (non-pending ⇒ `diff_not_pending`, mid-sequence failure ⇒
  `apply_failed:<step>` + still-pending row). Uses the existing
  in-memory service/test harness patterns — no new harness.
- B. Rely on U3's runtime tests only: they cover the loop, not the
  registry's closure or per-tool binding rules.
- C. Snapshot-test `catalog()` output only: proves names, not the
  authz/denial behavior the ACs pin.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Consolidated Summary Confirmation

Decisions recap:

- Catalog: flat `domain_verb` provider-legal tool names incl. the C7
  kinds verbatim (Q1).
- Classes: static `{spec, class, handler}` registry; write-class =
  propose_diff, pr_comment, exec/fs; read-class never audits (Q2).
- ProposedDiff: model supplies canonical diff + file ops; ops persist
  via an additive `ops_json` column (Q3).
- Materialization: user-only `oxpilot.diff_apply` → branchCreate +
  file.* + pull.create via the caller's own rpc_dispatch; no model-
  facing apply verb (Q4).
- Actions: six read tools; control/write procs never registered (Q5).
- Computer-use: schema-validated args ≤ 2 KiB, target_id arg +
  single-connected fallback, `targets_list`/`action_status` reads,
  consent_required → card + output, other denials → ToolError (Q6).
- Provenance: fixed `/oxpilot` session+proposal line in the PR body; no
  AI-attribution trailers (Q7).
- `ToolCtx.proposal_id`: dead in v1 — tools bind via explicit args;
  gap documented (Q8).
- Comments: `pr_comment` bound to this session's applied `pr_ref`s via
  `pull.comments.create` unanchored (Q9).
- `diff_*` procs: list/get/apply shapes, `owner/name#number` pr_ref,
  additive session-list DB verb (Q10).
- Internal dispatch: per-call synthesized `RpcCtx` via
  `build_rpc_ctx_with_session` + `ClientMeta::default()` — proc
  authorization rides verbatim (Q11).
- Audit: three U5 events (`oxpilot.proposal_created`,
  `oxpilot.diff_applied`, `oxpilot.artifact_comment`), post-commit,
  bounded redacted detail; exec/fs covered by U6's dispatch row (Q12).
- Tests: registry-closure + per-handler + apply negative-path seams
  on the existing in-memory harness (Q13).

Does this all look correct before the functional-design artifacts are
written?

- Looks correct
- Request changes

[Answer]: Looks correct
