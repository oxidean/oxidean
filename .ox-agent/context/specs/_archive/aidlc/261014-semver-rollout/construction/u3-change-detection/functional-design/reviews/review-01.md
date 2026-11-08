**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** NOT-READY

Scope: `construction/u3-change-detection/functional-design/` — entities.md, rules.md, functional-spec.md, traceability.json.

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| F-01 | Minor | entities.md `BumpPlanEntry.commits`; spec WF-2 | `commits` content for propagated-only entries is unspecified — does a propagated-only entry list an empty array, or the origin entries' commits? C3 just says `{sha,...}[]`. U4's changelog needs to know whether propagated entries carry their own commit list or rely on `propagatedFrom` for the `### Dependencies` note. | Define: `commits` lists only the entry's own directly-attributed commits (empty for propagated-only); `propagatedFrom` names the origins whose changelog sections carry those commits. | New |
| F-02 | Minor | rules.md BR3.4 | The propagation sentence in BR3.4's logic ("propagated signal contributes patch iff the entry had no direct commits or direct < patch") is both redundant with BR3.5 and malformed — `direct < patch` compares a boolean to a bump level. | Delete the clause; BR3.4 computes direct_bump only, BR3.5 owns the combination rule. | New |
| F-03 | Minor | rules.md BR3.2 | "mapped scope → resolved ids (glob hits unioned in)" — the union behaviour is right but the ordering statement "scope-then-glob" is ambiguous: for a *mapped* scope commit, do its changed-file globs also attribute to OTHER entries, or only to the resolved ones? If `feat(api):` touches `crates/oxidean-core/x.rs`, does core get a hit? | Decide and state: either globs union in freely for all commits (path evidence is real regardless of the declared scope — recommended: a commit touching another component's files IS a change to it) or glob fallback applies only to unmapped/absent scopes. Pick union-for-all (safer for detection) or scope-wins (cleaner intent) — the spec must choose. | New |

### Closing note

Core mechanics are sound — transitive closure, max(direct, patch), unconditional lockfile regen, channel suffixes, baseline/pending bounds all correctly specified and traced. The three findings are precision gaps in the same neighbourhood: what counts as a hit when scope and paths disagree, and what a propagated entry's commit list means. Fix and re-review.
