## Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 2

### Scope reviewed

`crates/oxidean-api-oxpilot/src/tools/` (mod.rs, dispatch.rs, procs.rs,
propose.rs, read.rs, registry.rs, validate.rs, comment.rs) — each
iteration-1 finding re-verified in the CURRENT code, plus a fresh scan
for regressions and missed defects against the design contracts at
`construction/oxpilot-tools/{functional-design,nfr-design}/` and the
shipped surfaces the unit consumes (`oxidean-api-core/src/oxpilot.rs`
traits/constants, `repo_types.rs`/`pull_types.rs`/`issue_types.rs`/
`action_types.rs` request/response shapes, `oxidean-db` proposal verbs,
`oxidean-api-repo` `commitPolicy`/`refs` implementations, the crate's
`services.rs::db_err` convention, and `runtime.rs::invoke_call`'s
message handling). Constraint honoured: no cargo builds/tests — this is
source-level verification; signatures were read at their definitions.

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | Major | tools/mod.rs:184,194-203,215,222 | String::truncate at a byte bound panics mid-codepoint on non-ASCII payloads | Char-boundary truncation at all three sites | Resolved |
| R-02 | Minor | tools/mod.rs:208-229 | bound_output truncated content with no in-band truncated marker — silently-corrupt JSON reached the model | Carry an explicit truncation marker in the bounded payload | Resolved |
| R-03 | Minor | tools/propose.rs:46-73 | diff_header_paths pushed /dev/null sides verbatim — create/delete diffs could never satisfy BR4.3 | Map dev/null sides to the other side's path | Resolved |
| R-04 | Minor | tools/propose.rs:150-155 | files:[] plus headerless diff_body passed coherence and committed a row that can never apply | Reject empty ops | Resolved |
| R-05 | Nit | tools/dispatch.rs:90-93 | no_target message named the user proc oxpilot.target_list the model cannot call | Name targets_list plus ask the user to pair | Resolved |
| R-06 | Nit | tools/registry.rs:147-159,315 | pull_list state enum lacked merged though pull.list accepts it | Dedicated pull_list_schema with merged | Resolved |
| R-07 | Nit | tools/read.rs:25-36 | owner/name injected before args copy — a stray arg key could overwrite the binding | Inject the session binding last | Resolved |
| R-08 | Nit | tools/procs.rs:299-304 | pr number unwrap_or_default minted owner/name#0 on a missing field | Hard oxpilot.internal on missing/non-positive number | Resolved |
| R-09 | Nit | tools/validate.rs:57-71,111,141 | No recursion depth cap in the schema walker | MAX_DEPTH 16 threaded through check_at | Resolved |
| R-10 | Nit | tools/validate.rs:163-173 | u64 above i64::MAX passed schema then failed the backing proc's serde as internal | Reject as invalid_args | Resolved |

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| N-01 | Minor | tools/comment.rs:36 and tools/procs.rs:78 | Raw db error String forwarded verbatim into the wire error message — comment.rs puts it in ToolError.message which invoke_call feeds to the model after only a secrets-redact pass (sqlx text can expose dialect/table detail), and procs.rs does the same onto the user surface; both deviate from the internal-code-generic-message contract (security-design denial map, BR3.4/NFR4.5) and the crate's own services.rs db_err convention (generic message plus tracing::error) | Map to a generic message and log the raw detail via tracing — e.g. "internal error" at the tool arm and a services-style generic at the proc arm | Open |
| N-02 | Nit | tools/registry.rs:285 with tools/comment.rs:51; tools/registry.rs:221 with tools/propose.rs:147 | Schema maxLength counts chars while the matching handler bounds count bytes (body 4096, diff_body 262144) — a multibyte-heavy payload passes schema then fails the handler bound, so the schema over-advertises the pin for non-ASCII input; it fails closed so the effective KiB pin holds | None required — optionally note the byte unit or tighten the schema; informational inconsistency, not a correctness defect | Open |

### Verification notes

- **R-01**: `truncate_charsafe` (mod.rs:194-203) early-returns when
  `s.len() <= max`, then walks `cut` down from `max` (always `< s.len()`)
  to the first char boundary — terminates at 0, which is always a
  boundary. Applied at `map_proc_error`'s message (184), `bound_output`'s
  text (215), and `output_summary` (222). No other `.truncate(` remains
  outside the guarded call. The `is_char_boundary` loop is correct:
  `cut` starts within bounds and only decrements.
- **R-02**: `TRUNC_MARKER` "\n[output truncated]" (mod.rs:209) appended
  after the char-safe cut; `TOOL_OUTPUT_MAX - marker.len()` cannot
  underflow (32768 vs 20); final content stays ≤ TOOL_OUTPUT_MAX; pin
  test at mod.rs:318-323. Truncated outputs report the literal summary
  "output truncated" — BR9.4-compliant.
- **R-03**: `dev_null` closure (propose.rs:63) matches both `dev/null`
  and `/dev/null`; a-side dev/null maps to (b,b), b-side to (a,a) —
  create headers now contribute the new path and delete headers the old
  path. Degenerate `a/dev/null b/dev/null` collapses to a literal
  "dev/null" coherence entry — satisfiable only by an op on the path
  `dev/null`, which is a legal repo-relative path anyway; harmless.
  Headers without a ` b/` separator are skipped, and any such diff fails
  coherence closed when ops are present.
- **R-04**: `ops.is_empty()` rejects at propose.rs:150-155, before the
  max-files check and before coherence — `files:[]` can never reach the
  insert; a headerless diff_body still fails set coherence whenever ops
  are non-empty.
- **R-05**: `no_target` (dispatch.rs:90-93) names `targets_list` and
  tells the model to ask the user to pair one — matches the taxonomy's
  pair-a-target guidance; `ambiguous_target` (97-104) also names
  `targets_list`.
- **R-06**: `pull_list_schema` (registry.rs:149-159) carries
  `["open","closed","merged","all"]`, matching PullListRequest.state
  (pull_types.rs:157); `issue_list_schema` (139) stays
  `["open","closed","all"]`, matching IssueListRequest
  (issue_types.rs:341) — `merged` did not leak. `pull_list_schema` is
  referenced only by the `pull_list` entry (315); `number_schema` shared
  by issue_get/pull_get is unchanged.
- **R-07**: `build_input` copies args first then inserts `owner`/`name`
  last (read.rs:27-36) with the load-bearing comment; ref/git_ref/branch
  defaults and the `wait_ms` strip run after, none of which touch
  identity keys.
- **R-08**: `pr["number"].as_i64().filter(|n| *n > 0)` → hard
  `oxpilot.internal` on missing/zero/negative (procs.rs:299-304); PullPublic
  `number: i64` is the source field.
- **R-09**: `MAX_DEPTH = 16` (validate.rs:60), enforced on `check_at`
  entry (69-71), `depth + 1` threaded at both recursion sites (111
  object properties, 141 array items) — catalog depth ≤3 so nothing
  legitimate is near the cap.
- **R-10**: the integer arm rejects `as_u64() > i64::MAX` as
  invalid_args (validate.rs:170-172) before numeric bounds; negatives
  (`as_u64` → None) and floats fall through correctly.

### Regression + missed-defect scan

- **Panic paths**: no `unwrap`/`expect`/`panic!`/`unreachable!` outside
  `#[cfg(test)]` in the unit. `connected[0]` is guarded by the `len == 1`
  match arm (dispatch.rs:95). `bound_job_log` slices `content[start..]`
  at a char boundary located via `char_indices` (read.rs:86-92).
- **diff_apply ordering** (procs.rs:134-327): gates run
  `require_enabled` → `require_verified` → `svc.proposal` (user-scope) →
  `status != pending` check → proposal's OWN session re-resolve →
  `repo.file.commitPolicy` unprotected-name gate → `repo.refs`
  branch-exists gate → `branchCreate` → ops (single batched upload at
  the first create/update position, per-op delete/rename in declared
  order) → `pull.create` → `mark_applied` → audit. Both prechecks land
  before any effect; `direct_commit_allowed` and the
  `refs/heads/<branch>` comparisons match
  `RepoFileCommitPolicyResponse.direct_commit_allowed`
  (repo_types.rs:1411) and the full-refname shape `repo.refs` returns
  (`for-each-ref %(refname)`, cli.rs:2115; `RepoRefEntry.name`).
- **Wire-name alignment**: `repo.tree`/`repo.blob`/`repo.commits`
  deserialize `ref` via `#[serde(rename = "ref")]` on `ref_name`
  (repo_types.rs:861,891,935) — the injected `ref` is correct and
  load-bearing (field is non-Option). `listRuns` takes
  page/per_page/status/branch/workflow — all schema-offered names map.
  `getJobLog` takes run_id/job_id/offset plus `wait_ms`, which
  `build_input` strips (read.rs:70). `listWorkflows` takes `git_ref`
  (action_types.rs:232). `pull.comments.create` is invoked unanchored
  `{owner,name,number,body}` (comment.rs:62-67), and `pull.create`
  returns `PullPublic.number: i64`.
- **Denial decode**: `denied_or_mapped` matches `data.denied`
  structurally — `consent_required` requires a usable `grant_id` else
  internal, the hard three carry the reason in the message, malformed or
  unknown markers fail closed to `internal` — matches the C7 `denied()`
  producer (oxpilot.rs:571-579) and the NFR4.3 decode table.
- **Catalog**: 23 entries in the pinned order; proc bindings are
  literals; the banned-proc test asserts control/mutation names absent;
  exec/fs names equal `CONNECTOR_CAPABILITIES`; Write class is exactly
  {propose_diff, pr_comment, exec_shell, fs_read, fs_write} per BR2.4;
  every schema is `additionalProperties: false` via `obj()`/`empty()`.
- **Redaction surface**: proc messages redact before bounding
  (map_proc_error), output_summary redacts (bound_output), the comment
  body redacts pre-dispatch (comment.rs:54), audit detail carries
  ids/counts with title redacted (propose.rs:266-278), and the
  synthesized RpcCtx uses an `oxpilot:`-namespaced session id with
  `ClientMeta::default()`.
- **Observations evaluated and dismissed as non-defects**:
  `bound_output`'s truncated summary is the literal "output truncated"
  rather than a redacted preview (compliant); propose_diff/proposal_get
  re-resolve `sessions()` rather than reuse the ladder's `_svc`
  (harmless OnceLock read); `created_at` rides `proposal_get`'s output
  beyond the pinned field set (additive); `emit_card`/`audit` are
  best-effort post-commit per NFR4.8/4.9; `sessions()`/`gateway()`
  port-miss sites pass the static "not installed" message as internal
  (matches the crate's unset-seam convention, no leak).

### Verdict rationale

All ten iteration-1 findings verified Resolved in current code with no
regressions introduced. The only new findings are a Minor internal-error
message leak on the db-failure edge path (N-01) and a Nit unit
inconsistency between schema char bounds and handler byte bounds (N-02)
— both non-blocking.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |
