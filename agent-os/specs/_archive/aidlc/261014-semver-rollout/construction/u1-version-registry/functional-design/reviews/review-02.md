**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 2
**Verdict:** READY

Scope: `construction/u1-version-registry/functional-design/` — entities.md, rules.md, functional-spec.md, traceability.json. Re-review of iteration-1 findings.

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| F-01 | Major | entities.md `baseline_commit`; rules.md BR1.6; functional-spec.md WF-3 | RESOLVED. Entity type is now `sha40 \| "pending"`; the two-commit flow is explicit (merge ships `"pending"`, a follow-up `chore(release):` commit on main records the merge sha); `check` warns-but-exits-0 on `pending` and U3's FR3.5 scan fallback while pending is named (repo root). The self-referential-commit defect is gone; main stays green in the recording gap. | — | Resolved |
| F-02 | Minor | rules.md BR1.7 | RESOLVED. BR1.7 now enforces glob uniqueness alongside scopes/tag prefixes with exit-1 semantics; entity summary table updated. | — | Resolved |
| F-03 | Minor | functional-spec.md WF-2 | RESOLVED. WF-2 now names `check` (local + ci-commit job) as the sole completeness gate and states `load_registry` callers rely on it. | — | Resolved |

### Closing note

All three iteration-1 findings verified fixed on the reviewed bytes. One implementation note carried forward for U3 (not a blocker here): BR1.7's glob-overlap check is statically conservative — identical/contained literal patterns can be flagged at `check` time, but a changed file matching two entries' globs must also fail closed at detection time in U3's ChangeDetector. traceability.json covers the unit's assigned FRs (FR2, FR2.1–FR2.4) with valid BR targets and N/A'd technical rules. Recommend unit completion.
