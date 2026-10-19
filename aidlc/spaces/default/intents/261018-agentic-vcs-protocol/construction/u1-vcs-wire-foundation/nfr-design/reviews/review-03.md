## Review

**Stage:** nfr-design
**Unit:** u1-vcs-wire-foundation
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings
| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| R-09 (carry) | minor | SD1 still cites "proc_classes.rs is 639 total" — the file is now 709 lines; absolute count remains drifted. | security-design.md §SD1 | Unresolved |
| R-13 (carry) | minor | LC1's "no imports beyond `serde`/`schemars`/serde_json" remains literally false — `vcs_types.rs:17-18` imports `crate::protection_types::ProtectionBlockReasons` and `crate::AppError` (intra-crate, leaf-module intent holds; wording needs amending). | logical-components.md §LC1 | Unresolved |
| R-14 (carry) | minor | SD2's credential pin still drops SR2's open job-token question (security-requirements.md SR2:18 asks u7 to confirm `oxidean_job_` access on object routes). | security-design.md §SD2 vs nfr-requirements/security-requirements.md:18 | Unresolved |
| R-15 (carry) | minor | SD4's "DTO fields are unvalidated serde structs by design" is still literally false — `ChangesetId`/`ObjectId` validate format at `Deserialize` (vcs_types.rs:220-225, 277-282); sentence should scope to *bounds*. | security-design.md §SD4 | Unresolved |
| N-01 | minor | SD1 says "17 procedure arms + 2 object-route alias arms"; the landed block uses grouped alternation arms — 3 arms cover the 17 + 2 alias arms (proc_classes.rs:362-383). Intent and enforcement hold; wording-precision only. | security-design.md §SD1 vs proc_classes.rs:362-383 | Unresolved |
| N-02 | note | LC3's "`row_to_item` wraps `Some(...)`" + pending u2 follow-up no longer describe current bytes: u2 has landed (Option fields, migration 0053), `row_to_item` assigns directly. Pinned follow-up discharged as designed; a "(consumed by u2)" note would keep the artifact current. | logical-components.md §LC3 vs activity.rs:354-382 | Unresolved |
| N-03 | note | Activity constants live in `oxidean-core` (`VCS_ACTIVITY_*`/`VCS_ACTIVITY_KINDS`, vcs_types.rs:57-81); `VALID_PUSH_TYPES` untouched pending u7 as pinned. | logical-components.md §LC3 | Unresolved |
| N-04 | note | LC5's wire 0.1→0.2 bump pin is release-mechanics; `WIRE_VERSION` remains `0.1.0` consistently (lib.rs:69, rpc_gen.rs:13, index.ts:13). | logical-components.md §LC5 | Unresolved |

### Summary
READY — re-bind clean: all SD/LC pins hold on current bytes (fail-closed classification, 11-code closed enum with 500-cap truncation, `Nullable` trichotomy, zero-dep `oxidean-vcs`, 17 `vcs.disabled` dispatch arms + manifest parity, drift guards, traceability SR1–SR5 covered). 4 carried minors + 4 new non-blocking wording/precision items.
