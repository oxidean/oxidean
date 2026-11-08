**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 2
**Verdict:** READY

Scope: `construction/u2-commit-grammar/functional-design/` — entities.md, rules.md, functional-spec.md, traceability.json. Re-review of iteration-1 findings.

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| F-01 | Major | entities.md `ConventionalCommit.type` | RESOLVED. `type` is now `required: iff not exempt` — exempt results carry no type, and a new entity-level constraint fixes all exempt field values (`scope_status=absent`, `resolved=[]`, `breaking=false`, `warnings=[]`). The schema is now satisfiable for exempt inputs. | — | Resolved |
| F-02 | Minor | entities.md + spec WF-1 step 1 | RESOLVED. Exempt-commit output fields are now explicit in both the entity constraint and the workflow step. | — | Resolved |
| F-03 | Minor | rules.md BR2.2 | RESOLVED. BR2.2 now says whole-body scan (lenient), removing the Q3/BR wording divergence. | — | Resolved |

### Closing note

All three findings verified fixed on the reviewed bytes. The contract now round-trips cleanly: exempt inputs produce a well-defined partial result, raw signals flow to U3 unpolluted by bump decisions, and the shared-parser constraint (BR2.5) keeps hook/CI/PR-title behaviour identical per FR1.3. traceability.json covers FR1/FR1.3 with the range-scan rule correctly attributed to U5's enforcement surface. Recommend unit completion.
