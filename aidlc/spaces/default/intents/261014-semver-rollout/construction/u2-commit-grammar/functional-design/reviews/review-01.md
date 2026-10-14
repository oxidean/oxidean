**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** NOT-READY

Scope: `construction/u2-commit-grammar/functional-design/` — entities.md, rules.md, functional-spec.md, traceability.json.

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| F-01 | Major | entities.md `ConventionalCommit.type` + rules.md BR2.3 / spec WF-1 step 1 | Contract contradiction: `type` is declared `required: true` with an enum constrained to allowed_types — but WF-1 step 1 short-circuits exempt commits (`Merge …`, `Revert "…"`, `chore(release):`) before the grammar check, so an exempt commit *cannot* produce a valid `type`. As written the output schema is unsatisfiable for exactly the inputs the exemption exists for; U3's detector would read a field that can't be populated. | Make `type` optional (`required: iff not exempt`) in the entity and state the exempt-commit field values explicitly. | New |
| F-02 | Minor | entities.md `ConventionalCommit` + spec WF-1 | The exempt short-circuit never defines the other fields: `scope_status`, `resolved`, `breaking`, `warnings` on an exempt result are unspecified — implementers could return anything. | State: exempt commits emit `scope_status=absent`, `resolved=[]`, `breaking=false`, `warnings=[]` — the exemption swallows everything else. | New |
| F-03 | Minor | questions Q3 vs rules BR2.2 | Q3 scopes BREAKING footers to "the body footer block"; BR2.2 says "anywhere in the body". Footer-block-only matches the strict conventional spec; whole-body is lenient. The two sources disagree — pick one in the BR. | Fix BR2.2 wording to the decided scope (recommend whole-body: a `BREAKING CHANGE:` line mid-body still signals intent; false positives are rare and the cost is one bump). | New |

### Closing note

Grammar coverage is complete and the single-parser-shared-everywhere constraint is correctly modelled. F-01 is a real unsatisfiable-schema defect for exempt inputs; F-02/F-03 are precision gaps. Fix and re-review.
