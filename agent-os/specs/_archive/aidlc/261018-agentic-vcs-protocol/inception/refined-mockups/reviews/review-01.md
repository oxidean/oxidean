## Review

**Stage:** refined-mockups
**Reviewer:** aidlc-product-lead-agent
**Verdict:** READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location |
|----|----------|---------|----------|
| R-09 | minor | "14-entry set" miscounts its own list: `enable status commit log diff show\|cat blame history merge-base merge rebase checkout ref import export` is 15 entries with `show\|cat` merged (16 names) — matching AC8.4.1 (stories.md:283), which also has 15. 14 is wrong under either counting convention; prior "15 vs 16" ambiguity is now a "14" error. | interaction-spec.md:64 (vs :83) |
| R-10 | minor | New "`show`/`cat` are aliases of one" claim contradicts the mockups: `ox vcs cat … <path> --at <cs>` → `vcs.file_get` and `ox vcs show … <cs>` → `vcs.changeset_get` are distinct commands on distinct procedures/arg shapes (mockups.md:113-114; coverage note :232 repeats "file_get (`cat`), changeset_get (`show`)"). If they're truly aliases, `vcs.file_get` loses its only `ox vcs` verb; if distinct (as rendered), the set is 16 and the alias claim is false. Pick one and record the resolution of AC8.4.1's ambiguous "`show`/`cat`" for contract-design. | interaction-spec.md:64 vs mockups.md:113-114,232 |
| R-11 | minor | R-06's `loading` state was added to C1's spec (interaction-spec.md:17) but S1's own state list was not synced — it still enumerates only disabled/enabled/saving/error/read-only, so the two documents now disagree on the same control's state set by one. | mockups.md:28-34 vs interaction-spec.md:15-22 |
| R-12 | minor | "Names the two deliberate absences" reads as exhaustive, but the 12-tool MCP set omits five FR10.1 procedures: `vcs_repo_enable` (noted at interaction-spec.md:124), `vcs_changeset_get`, `vcs_git_import`, `vcs_git_export`, plus `vcs_ref_update` (named) and checkout (never an RPC). `vcs_changeset_get` is the conspicuous gap — agents resolve changed-file manifests through it for want-based fetch (AC3.3.2; `vcs_tree` only partially covers it). One clause enumerating the full absence set prevents misreading "the two". | mockups.md:201,233 |

### Summary

All eight prior findings are resolved in the bytes, with the codebase anchors re-verified (`RpcRequest`, `Out::Rpc`/`error_exit_code`, `ox: <code>:` stderr format, serde `rename="ref"`, MCP grant fail-open). The revision introduced only minor defects: the "14-entry" count is wrong under both readings, and the new `show`/`cat` alias claim contradicts the rendered distinct-command usage in S2.3 — the one edit worth making before contract-design freezes names. Coverage, state matrices, and the accessibility bar remain met; READY with the four minor cleanups above.
