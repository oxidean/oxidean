**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** READY

Scope: `construction/u7-contributor-guidance/functional-design/` — entities.md, rules.md, functional-spec.md, traceability.json.

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| F-01 | Minor | spec WF-1; rules BR7.3 | `versioning scopes` is introduced as the doc-authority command but no unit owns it — it's a new subcommand on U1's tool surface beyond the C1/C2 contract list (`check`, `commit-check`, `detect`, `plan`, `bump`, `changelog`, `tag`). Tiny surface, but undocumented in the contracts. | At code-gen: implement as a trivial list subcommand on the shared CLI (reads registry, prints id+aliases table); no contract change needed — it's a read-only projection of C1 data. | Carried |
| F-02 | Minor | rules BR7.2 | "cursor/agent rules that describe commits are updated" — which rules? The repo has `.cursor/rules/*.mdc`; the ones touching commit practice (e.g. conventional-commit or PR guidance if present) should be named in code-gen rather than discovered ad hoc. | At code-gen: enumerate `.cursor/rules/` and `.agents/` for commit-related guidance and update each hit; AGENTS.md + new skill file are already named. | Carried |

### Closing note

Clean docs unit — the file set, audiences, no-duplication rule, and dogfooding rule are all correctly specified; FR7.x coverage is complete. Both findings are code-gen-time implementation notes (name the subcommand's owner, enumerate the rule files), not design defects. Recommend unit completion.
