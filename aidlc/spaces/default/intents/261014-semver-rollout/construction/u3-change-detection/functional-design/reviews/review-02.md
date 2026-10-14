**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 2
**Verdict:** READY

Scope: `construction/u3-change-detection/functional-design/` — entities.md, rules.md, functional-spec.md, traceability.json. Re-review of iteration-1 findings.

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| F-01 | Minor | entities.md `BumpPlanEntry.commits` | RESOLVED. `commits` now constrained to the entry's own directly-attributed commits, empty for propagated-only; `propagatedFrom` carries the origins. U4's `### Dependencies` source is unambiguous. | — | Resolved |
| F-02 | Minor | rules.md BR3.4 | RESOLVED. The malformed propagation clause is gone; BR3.4 computes direct_bump only, BR3.5 owns combination. | — | Resolved |
| F-03 | Minor | rules.md BR3.2 | RESOLVED. Attribution is union-always (`resolved_scope_ids ∪ glob_matched_ids`) with signals applying uniformly — path evidence counts regardless of declared scope, the conservative choice for a detection system. | — | Resolved |

### Closing note

All three findings verified fixed. The engine's semantics are now unambiguous end-to-end: bounded scan → union attribution (with multi-match warnings as the runtime backstop to U1's static glob-overlap check) → transitive closure → 0.x-aware bump table → channel-aware toVersion → manifest writes with unconditional lockfile regen. traceability.json covers the unit's six FRs. Recommend unit completion.
