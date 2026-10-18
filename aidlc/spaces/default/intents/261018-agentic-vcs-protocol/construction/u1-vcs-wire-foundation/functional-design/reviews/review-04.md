## Review

**Stage:** functional-design
**Unit:** u1-vcs-wire-foundation
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings
| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| R-15 | minor | BR1.5 scopes "optional fields serialize absent-not-null" to the CAS pair only, but the artifact set pins two more explicit-null classes (contract-pinned `\|null` response fields; the widened RepoActivityItem fields) — reword to name all three classes | rules.md BR1.5 | Unresolved |
| R-16 | minor | `reverse` entry explains BR1.6 as un-AC'd while coverage rows now name BR1.6 as an OK target — self-inconsistent either way | traceability.json | Unresolved |
| R-17 | minor | `ref_conflict.actual` description misparses — it is the actual stored value compared against, not the caller's `expected_old` | entities.md ErrorData | Unresolved |
| R-18 | minor | `conflict.conflicts` payload row omits the landed `truncated: bool` field (VCS_CONFLICT_DATA_MAX_ENTRIES, SR3) | entities.md ErrorData | Unresolved |
| R-19 | minor | AC2.1.2/AC2.6.2 are cited inside entities.md yet absent from upstream_ids while non-assigned-story ACs are enumerated — enumeration-scope asymmetry | traceability.json | Unresolved |
| R-20 | note | NFR8 caveat (cross-stage): `AppError.message` is free String — message hygiene remains u4/u7 code-review discipline | entities.md VcsErrorCode | Acknowledged |
| R-21 | note | Manifest client-contract: `system.manifest.procedures` advertises all 17 `vcs.*` while dispatch arms return `vcs.disabled` — procedures = recognized names, capability discovery is `vcs.status` (u7) | rpc.rs; nfr-design LC5/SD2 | Acknowledged |

### Summary

Re-pass binds evidence to current bytes: 17 DTO pairs + VcsErrorCode (11 codes) + Nullable<T> trichotomy + ref-free RepoActivityItem widening all match landed code; classification 10/6/1 + 2 aliases with SessionOnly fail-close; zero-dep oxidean-vcs skeleton registered; PROCEDURES/stub arms/coverage tests/generated-client parity verified. READY — 5 unresolved minors + 2 acknowledged notes, all wording/derived-view nits.
