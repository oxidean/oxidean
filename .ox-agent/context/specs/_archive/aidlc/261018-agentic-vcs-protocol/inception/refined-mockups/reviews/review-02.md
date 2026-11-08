## Review

**Stage:** refined-mockups
**Reviewer:** aidlc-product-lead-agent
**Verdict:** READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location |
|----|----------|---------|----------|
| R-15 | minor | R-12's fix didn't fully propagate: the coverage note still says S3 "names the two deliberate absences," while S3's own paragraph now enumerates six (`vcs_repo_enable`, `vcs_ref_update`, `vcs_changeset_get`, `vcs_git_import`, `vcs_git_export`, checkout). The stale "two" contradicts the new full list in the same file. | mockups.md:235 vs :203 |
| R-16 | minor | C3's absence parenthetical still enumerates only 3 of the 6 MCP absences (`vcs_repo_enable`, `vcs_ref_update`, checkout) — it omits `vcs_changeset_get`, `vcs_git_import`, `vcs_git_export`, which mockups.md:203 now names as "the full absence list." The two documents disagree on the absence set post-R-12. | interaction-spec.md:124 vs mockups.md:203 |
| R-17 | minor | S1's States block gained `loading` but still doesn't mirror C1's declared set: the transient `success`/"Saved" confirmation is absent, and `disabled-input`'s second trigger ("caller lacks admin class", `canAdmin` prop) isn't rendered — only the archived case appears. Same drift class as R-11, one row narrower. | mockups.md:28-36 vs interaction-spec.md:15-22,31 |
| R-18 | minor | The toggle's read path is unspecified for never-enabled repos: AC1.2.3 has `vcs.*` calls on a never-enabled repo fail `vcs.disabled`, yet S1's loading→default flow assumes `vcs.status` resolves to a flag. Never-enabled is every repo's starting state, so the docs need "vcs.disabled-on-read → unchecked default" (or a note that `vcs.status` is exempt) — otherwise a literal implementation lands in `error` on a normal repo's settings page. | mockups.md:29-35, interaction-spec.md:17-21 vs stories.md:25 |

### Summary

All four claimed fixes are resolved in the bytes: R-09 by "16 command names" matching the 16-name subcommand list (interaction-spec.md:64,83 — verified against AC8.4.1), R-10 by the explicit DISTINCT declaration with per-command procedure mapping (mockups.md:115-116, memory.md:23), R-11 by the added `loading` state (mockups.md:29-30), and R-12 by the six-item absence list that is accurate against FR13.1's 12 tools vs FR10.1's 17 procedures (mockups.md:203). Codebase anchors re-verified: `RpcRequest {procedure,input}` (oxidean-core lib.rs:196-199), `Out::Rpc` + `error_exit_code`'s literal list (rpc.rs:52-66 — the "must add `vcs.unauthenticated`/`vcs.forbidden`" caveat is accurate), `ox: <code>:` stderr format (main.rs:76), serde `rename="ref"` (repo_types.rs), `fg_repo_access_check` (mcp.rs:1035), `VALID_PUSH_TYPES`/`insert_repo_activity` (repo/activity.rs:20,404). The residuals are four minors: two stale/partial R-12 propagations, a narrowing S1↔C1 state drift, and one unspecified never-enabled read mapping — none blocking, all one-clause fixes.
