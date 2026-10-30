**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** NOT-READY

Scope: `construction/u1-version-registry/functional-design/` — entities.md, rules.md, functional-spec.md, traceability.json.

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| F-01 | Major | entities.md `baseline_commit`; rules.md BR1.6; functional-spec.md WF-3 | `baseline_commit` is specced as a required 40-hex sha "written once at merge" — but a commit cannot contain its own sha. As written, the field can never legally be populated on the merge commit: either the merged tree ships a placeholder (violating BR1.6's required constraint the moment it lands, red CI on main) or the spec silently assumes an impossible write. The transition-window semantics are undefined. | Define the two-commit flow explicitly: merge ships `baseline_commit` in a well-defined pending state (e.g. literal `"pending"` or absent-with-schema-allowance); a follow-up `chore(release):` commit on main records the merge sha. `check` must accept the pending state (warn or pass) so main stays green in the gap, and U3's scan lower-bound fallback while pending must be named (scan from repo root per FR3.5 fallback). Update entity constraint, BR1.6, WF-3. | New |
| F-02 | Minor | entities.md `RegistryEntry.globs` vs rules.md | Entities declare `globs` "unique across entries" but no BR carries the violation behaviour — an overlapping glob would double-attribute a changed file and the spec never says `check` fails on it. | Extend BR1.7 (or add a rule) to cover glob uniqueness enforcement with exit-1 semantics. | New |
| F-03 | Minor | functional-spec.md WF-2 | `load_registry` skips workspace enumeration "to stay cheap" — reasonable, but the spec doesn't state the consequence: consumers may hold a registry that `check` would reject for incompleteness, and nothing names `check`-in-CI as the compensating completeness gate. | One sentence in WF-2: completeness is enforced only by `check` (local + ci-commit job); `load_registry` correctness relies on that gate. | New |

### Closing note

The unit is structurally sound — entity carriers, strict schema, dual-format check output, and FR2.x coverage all trace correctly. F-01 is a real feasibility defect (a commit cannot embed its own sha) that must be specified before downstream units (U3's scan bound, U4's baseline tags) build on it; F-02/F-03 are cheap spec completions. Fix and re-review.
