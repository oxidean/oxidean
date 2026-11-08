## Review

**Stage:** functional-design
**Unit:** u1-vcs-wire-foundation
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| R-15 | minor | BR1.5 scopes "optional fields serialize absent-not-null" to the CAS pair only; the artifact set pins two more explicit-null classes (contract `\|null` response fields, widened RepoActivityItem fields). | rules.md BR1.5 | Unresolved (carried) |
| R-16 | minor | `reverse` entry explains BR1.6 as un-AC'd while coverage rows name BR1.6 as an OK target — self-inconsistent either way. | traceability.json | Unresolved (carried) |
| R-17 | minor | `ref_conflict.actual` misparsed — it is the actual stored value compared against, not the caller's `expected_old`. | entities.md ErrorData | Unresolved (carried) |
| R-18 | minor | `conflict.conflicts` payload row omits the landed `truncated: bool` field. | entities.md ErrorData | Unresolved (carried) |
| R-19 | minor | AC2.1.2/AC2.6.2 cited inside entities.md yet absent from upstream_ids — enumeration-scope asymmetry. | traceability.json | Unresolved (carried) |
| R-20 | note | NFR8 caveat: `AppError.message` is free String — message hygiene remains u4/u7 code-review discipline. | entities.md VcsErrorCode | Acknowledged |
| R-21 | note | `system.manifest.procedures` advertises all 17 `vcs.*` while dispatch returns `vcs.disabled` — procedures = recognized names; capability discovery is `vcs.status` (u7). | rpc.rs; LC5/SD2 | Acknowledged |
| R-22 | minor | Q3 answer asserts `kind`/`mode` "plain strings on the wire DTO" — entities.md pins "enum string" + allowed_values and code landed closed serde enums; stale-answer wording. | functional-design-questions.md Q3 | Unresolved |

### Summary

Re-verification pass on unchanged bytes: READY holds. All claims re-verified —
17 DTO pairs + `VcsErrorCode` (11 codes) + `Nullable<T>` CAS trichotomy +
`RepoActivityItem` widening match landed code; proc_classes 10/6/1 + 2 aliases
with `SessionOnly` fail-close; PROCEDURES + 17 `vcs.disabled` stub arms keep
parity tests green; generated client covers all 17 names; `versioning.toml` and
both `WIRE_VERSION` pin sites verified; web null guards present. AC coverage
complete for U1's 8 assigned stories. Six minors + two notes, all
wording/derived-view nits — none affect implementation correctness.
