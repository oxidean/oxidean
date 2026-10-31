**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** NOT-READY

Scope: `construction/u6-version-surfaces/functional-design/` — entities.md, rules.md, functional-spec.md, traceability.json.

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| F-01 | Minor | rules.md BR6.3; entities ManifestSurface.min_cli_version | `min_cli_version` is labelled a "distinct ordinal floor" but its enforcement is unspecified — who compares against it (the generated client, the server?) and is the outcome advisory (wire-style warn) or a hard refusal? Domain design calls it a floor check; the spec never says where it runs. | State it: the generated client compares its own version against `min_cli_version` and REFUSES on `own < min` (hard floor — the existing manifest.rs:94–107 semantics, unchanged); wire compat stays advisory. | New |
| F-02 | Minor | rules.md BR6.2; spec WF-2 | Unstated consequence: legacy `"1"`-speaking CLIs map to wire 0.1.x — so the first feat/breaking wire bump to 0.2.x will hard-reject every pre-versioning CLI. That's intended SemVer behaviour, but it should be a stated consequence (and lands in docs/WIRE-CHANGELOG.md via U4) rather than discovered later. | One sentence in WF-2/BR6.2 noting the consequence is deliberate: the legacy alias is a 0.1.x contract, not a permanent bypass. | New |

### Closing note

Transition semantics are otherwise correctly specified end-to-end — dual-accept gate, co-emitted manifest fields, latest.json-only `/cli/latest`, rpc-gen'd MCP constant, dead-config removal. The two gaps are precision issues, not design defects. Fix and re-review.
