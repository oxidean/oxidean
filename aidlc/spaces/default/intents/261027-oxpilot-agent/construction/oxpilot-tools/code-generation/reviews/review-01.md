## Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY
**Iteration:** 1

### Scope reviewed

`crates/oxidean-api-oxpilot/src/tools/` (mod.rs, dispatch.rs, procs.rs,
propose.rs, read.rs, registry.rs, validate.rs, comment.rs) plus crate
wiring (`lib.rs` PROCEDURES, `dispatch.rs` match arms, `main.rs`
`install_tools`) checked against functional-spec/entities/rules, the
NFR designs (security, reliability, performance, scalability,
observability, logical-components), and the U3/U6 contracts in
`oxidean-api-core/src/oxpilot.rs` and `app.rs`. `cargo check -p
oxidean-api-oxpilot` is clean; `cargo test -p oxidean-api-oxpilot
tools::` passes 29/29. The shipped code is deeply conformant on the
structural pins — the findings below are the residual defects.

### Prior findings

(none — first review iteration)

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | Major | tools/mod.rs:185,200,207 | String::truncate at a fixed byte index panics mid-codepoint — non-ASCII payloads/messages over the bound panic inside invoke, killing the turn and leaving the tool-call card stuck running | Char-boundary truncation (floor_char_boundary/char_indices) at all three sites | Open |
| R-02 | Minor | tools/mod.rs:196-215 | bound_output truncates content with no in-band truncated marker — the model receives silently-corrupt JSON | Carry an explicit truncation marker in the bounded payload | Open |
| R-03 | Minor | tools/propose.rs:46-62 | diff_header_paths pushes /dev/null sides verbatim into the coherence set — valid create/delete diffs can never satisfy BR4.3 | Skip dev/null sides when building the coherence set, mirroring derive_files | Open |
| R-04 | Minor | tools/propose.rs:181-242 | files:[] plus headerless diff_body passes coherence (empty==empty) and commits a pending row that can never apply | Reject an empty files array | Open |
| R-05 | Nit | tools/dispatch.rs:90-93 | no_target guidance names oxpilot.target_list — a user proc the model cannot invoke | Reword to user-directed guidance naming targets_list | Open |
| R-06 | Nit | tools/registry.rs:139,301 | pull_list reuses issue_list_schema — state enum omits merged which PullListRequest supports | Extend the pull_list state enum with merged | Open |
| R-07 | Nit | tools/read.rs:27-33 | build_input inserts session {owner,name} then lets arg keys overwrite — wrong defensive ordering | Copy arg keys first, then insert owner/name | Open |
| R-08 | Nit | tools/procs.rs:299-302 | pr[number].as_i64().unwrap_or_default() silently mints corrupt owner/name#0 pr_ref on response drift | Require pr.number with a hard error instead of defaulting | Open |
| R-09 | Nit | tools/validate.rs:59 | No recursion depth cap — the pinned design wants MAX_DEPTH 16 | Thread a depth counter through check() capped at 16 | Open |
| R-10 | Nit | tools/validate.rs:151-155 | Integers above i64::MAX pass schema then fail downstream as rpc.bad_input — internal instead of invalid_args | Reject u64 > i64::MAX at schema validation | Open |

### Verified-conformant surface (no findings)

- Catalog: exactly the 23 pinned provider-legal names in fixed order, closed static registry (registry.rs:293-317); write-class set is exactly the pinned five; banned-proc absence is pin-tested (registry.rs:439-464).
- Invocation ladder: unknown name, then schema, then session, then handler — order matches BR1.3/BR1.4/BR2.1 (mod.rs:223-260).
- `map_proc_error`: the verbatim passthrough set is exactly the three pinned codes; everything else collapses to `internal` plus generic message (mod.rs:180-191).
- `dispatch_proc`: synthesized `oxpilot:<session_id>` `ResolvedSession`, `build_rpc_ctx_with_session`, `ClientMeta::default()` — proc-level `require_verified` still executes (mod.rs:96-126; app.rs:546-585; gate.rs:23-46).
- Denial decode is structural on `data.denied` only: `consent_required` plus `grant_id` emits the consent card and returns the success-shaped do-not-re-issue output; `not_connected`, `offline`, `capability` map `action_denied`; missing, malformed, or unknown markers fail closed `internal`; no message-sniffing (dispatch.rs:151-190).
- `diff_apply`: the pinned fail-stop sequence — user-scoped `svc.proposal`, pending check, `repo.file.commitPolicy` requiring `direct_commit_allowed` else `apply_failed:protected_branch`, `repo.refs` existence precheck else `apply_failed:branch_exists`, `repo.branchCreate` with `start: row.base`, one batched `repo.file.upload` for create/update (fields renamed to `content_base64`), per-op `repo.file.delete`/`repo.file.rename` each carrying `message` plus `branch`, `pull.create` with `base_ref: row.base`, `head_ref` the apply branch, and the exact provenance line, then `mark_applied` then `oxpilot.diff_applied` audit — no rollback (procs.rs:134-323).
- Audit targets: `oxpilot_proposal` for propose and apply, `pull_request` for the artifact comment (propose.rs:255, procs.rs:311, comment.rs:77).
- Serialized-args bound: the reserialized validated string is measured and dispatched — never raw `call.args_json` — and `target_id` is stripped before serialization (dispatch.rs:112-131).
- `pr_comment`: ArtifactScope rebuilt per call from this session's `applied` `pr_ref`s, `owner/name#number` join key, unanchored `pull.comments.create`, body redacted pre-dispatch (comment.rs:31-84).
- `targets_list` auto-resolution filters `lifecycle` (never `status`) — `== "connected"` (dispatch.rs:84-87); `ActionView` passes through verbatim (dispatch.rs:55-57); `wait_ms` never reaches the log proc (read.rs:68).
- Exec/fs dispatch goes through `ConnectorGateway::dispatch_action` only — no direct target access (dispatch.rs:124-133).
- `propose_diff`: `base` is server-derived `session.branch`, never an arg; `ops_json` persisted alongside `diff_body`; the diff card emits after commit (propose.rs:229-247); `proposal_get` pins `row.session_id == ctx.session_id` post-fetch (propose.rs:292-294).
- Wiring: the three user procs are SessionOnly, registered on PROCEDURES and the dispatch match (lib.rs:52-57, dispatch.rs:55-57); `install_tools` rides the runtime slot beside `install_gateway` (main.rs:143-152); the `ops_json` chain is complete across `ProposalWrite`, `NewOxpilotProposal`, `OxpilotProposalRow`, all three `PROPOSAL_SELECT` lists, and the three-dialect `0089_oxpilot_proposals_ops` migrations.
- Output identity flow: `ToolCtx` is the sole identity authority; `{owner, name}` inject from the resolved session row; `ref`/`branch` defaults to the session branch; `git_ref` remap on `listWorkflows`; `repo.refs` names are fully-qualified `refs/heads/...`, so the `want` match at procs.rs:180-184 is correct.
- Panic/timeout posture otherwise holds: no `unwrap` on model-controlled paths outside the audited sites; the 60s race and 8-round budget are U3-side and honored.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |
