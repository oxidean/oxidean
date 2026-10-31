# RAID Log — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `intent-capture/intent-statement.md`,
`market-research/competitive-analysis.md`, `market-research/market-trends.md`,
`market-research/build-vs-buy.md`; Q7 in `feasibility-questions.md`.

## Risks

| ID | Risk | Likelihood | Impact | Retirement | Owner stage |
|---|---|---|---|---|---|
| R-1 | Computer-use connector is novel attack surface — consent spoofing, credential theft, unintended action execution on user systems | Medium | High | Dedicated security design (consent + authz + action audit + PAT-class credentials + per-action scoping); PAT-only transport auth per D-12 precedent | nfr-requirements → nfr-design → domain-design |
| R-2 | `ai-octane` API churn (first published 2026-08-21) breaks the build mid-delivery | Medium | Medium | Exact version pinning via bun.lock; isolate provider/tool calls behind a forge-tool layer; OpenHands Agent Server fallback recorded | construction units; re-check at code-generation |
| R-3 | Long-lived streaming channels fail through the existing edge/proxy/auth stack (timeouts, idle reaping, auth expiry) | Medium | High | Early spike: one real SSE round-trip through `/api/` proxy in the first construction unit before bulk UI work | first construction unit; infrastructure-design |
| R-4 | Secrets/private code leak into transcripts or provider egress | Medium | High | Transcript redaction + retention policy; provider egress disclosure; operator docs; no secret in examples rule | nfr-requirements; code-generation (redaction hooks) |
| R-5 | Forgejo/Gitea ships a native agent, compressing the differentiation window | Low | Medium | Ship the differentiated core (user-connected computerUse + self-host parity) regardless; positioning can shift to openness | scope-definition; marketing decision outside this intent |
| R-6 | Feature-flag complexity leaks: partially-enabled states (flag on, no provider configured) produce broken UX | Medium | Medium | Disabled/unconfigured states are designed states (empty-state UI + operator docs), not error paths | functional-design |

## Assumptions (carried from upstream + this stage)

| ID | Assumption | Validation path |
|---|---|---|
| A-1 | LLM provider is operator-configurable; no provider named in request (intent-capture) | requirements-analysis picks the provider matrix |
| A-2 | Sessions/transcripts persist per-user server-side (intent-capture) | domain-design session model |
| A-3 | Computer-use targets are user-connected only, never admin-enabled (intent-capture) | consent model in nfr-design |
| A-4 | Builder/debugger can propose file changes + inspect CI output; exact action surface deferred (intent-capture) | requirements-analysis |
| A-5 | `ai-octane` feasible for full delivery window (feasibility) | construction spike; OpenHands fallback |
| A-6 | `/api/` proxy supports SSE without edge changes (feasibility) | first construction unit spike |
| A-7 | No hard regulatory driver (feasibility, compliance lens) | nfr-requirements formal scan |

## Issues

| ID | Issue | Status |
|---|---|---|
| I-1 | None open at stage exit | — |

## Dependencies

| ID | Dependency | Direction | Needed by |
|---|---|---|---|
| D-1 | `@tanstack/ai-octane` + `@tanstack/ai` + provider adapter packages (npm) | external | code-generation |
| D-2 | `octane >=0.12.0` (we have `^0.15.0`) | internal | code-generation |
| D-3 | Existing RPC dispatch, session auth, `/api/` proxy, `oxidean-db` migrations, route/browser coverage manifests | internal | all construction units |
| D-4 | Actions CI/build-log read APIs for the debugger surface | internal | requirements-analysis must verify coverage exists |
| D-5 | User-side connector transport through existing edge (outbound dial-in) | internal | domain-design; first construction unit |

## Assumptions & Open Questions

- [assumption] Risk likelihood/impact ratings are the conductor's qualitative
  judgment at ideation depth, not measured probabilities — they order work,
  they are not statistics.
