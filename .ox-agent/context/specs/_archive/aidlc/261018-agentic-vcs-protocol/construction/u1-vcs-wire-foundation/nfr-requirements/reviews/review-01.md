## Review

**Verdict:** READY
**Reviewer:** aidlc-architecture-reviewer-agent
**Date:** 2026-10-18T06:30:00Z
**Iteration:** 1

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Minor | functional-spec.md WF2.4 vs tech-stack-decisions.md | WF2.4 still says u1 registers "`depends_on` edges from `oxidean-api` and `ox`"; tech-stack row (correct per version-check's DAG-only validation + release-plan dependents closure) defers them to u7/u8. | Rephrase WF2.4: component registration in u1; edges land in u7/u8 with the real Cargo deps. | Unresolved |
| R-02 | Minor | nfr-requirements-questions.md Q1 | Q1 answer still describes `detail:"truncated"` on the last conflict entry; SR3 now uses dedicated `data.truncated:true`. | Amend Q1 answer to name the flag. | Unresolved |
| R-03 | Minor | SR4/SR5 | Request-side DTO bounds unstated: `limit` ≤500, `changes[]` ≤10_000 (`vcs.too_large`), string bounds on owner/name/ref/path/cursor are contract-pinned but unenforced at the type layer. | One bullet: bounds handler-enforced in u7 (clamp/reject + `validate_repo_name`-equivalent); DTO fields unvalidated by design. | Unresolved |
| R-04 | Note | SR4 | `absent → rpc.bad_input` imprecise for FG-token callers — `repo_target` parses owner/name pre-deserialization so absent target fails `auth.pat_scope` first. | Clarifying clause. | Unresolved |
| R-05 | Note | SR2 row 2 | Omits `vcs.not_found`/`vcs.disabled`/`vcs.quota_exceeded` from C2's denial set; also flag to u7 that contract omits job tokens on object routes though the shared resolver admits `oxidean_job_` for free. | Footnote for symmetry. | Unresolved |
| R-06 | Note | tech-stack-decisions.md | "13 existing uses" of `#[serde(rename="ref")]` exact for repo_types.rs; 15 across crates/. | Scope the count. | Unresolved |
| R-07 | Note | tech-stack-decisions.md | `serde_with` appears transitively in Cargo.lock but not as a direct dep — `Nullable<T>` conclusion stands. | None | Resolved |
| R-08 | Note | SR2 | `vcs.*` codes serialize as HTTP 400 on the RPC envelope (`rpc_status` whitelist doesn't know them) — consistent with platform behavior. | None | Resolved |

### Summary

READY. The artifact of record is accurate; every security claim verified against the worktree. Verified: SR1 fail-closed classification end-to-end (`_ => SessionOnly` → `auth.pat_scope`); SR2 codes all real (`rpc.version_mismatch` pre-auth, `rpc.unknown_procedure` post-authz for cookies); SR3's 500 cap matches C1 `limits.pagination`; `version-check` genuinely requires a real workspace member with literal `[package] version`, so U1 minting the crate skeleton is necessary; `depends_on` deferral is correct (DAG-only validation, release-plan closure would over-bump api/ox). Traceability N/A calls all honest.
