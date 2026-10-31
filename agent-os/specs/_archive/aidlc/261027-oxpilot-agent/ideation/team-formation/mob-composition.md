# Mob Composition — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `scope-definition/intent-backlog.md`,
`scope-definition/scope-document.md`, `feasibility/feasibility-assessment.md`.

## Composition model

Persona assignment is **stage-driven**, not roster-driven: the stage graph
declares the lead (and any supports) per stage, and the conductor adopts each
persona inline. Proto-units inherit that assignment.

| Proto-unit | Lead persona | Support lenses |
|---|---|---|
| P-1 Foundation (flag/config/session schema/RPC skeleton) | developer | architect |
| P-2 Page + chat endpoint + SSE spike | developer | architect, design |
| P-3 Forge context tools | developer | architect |
| P-4 Builder surface | developer | architect, quality |
| P-5 Debugger surface | developer | architect, quality |
| P-6 Computer-use connector | architect | devsecops, developer |
| P-7 Session UX depth | developer | design |
| P-8 Hardening (redaction/limits/audit/docs) | devsecops | developer, product |
| P-9 E2E evidence sweep | quality | developer |

## RACI

| Activity | Responsible | Accountable | Consulted | Informed |
|---|---|---|---|---|
| Artifacts/code per stage | Conductor (persona) | Maintainer (PR) | Support personas | — |
| Gate approvals | Conductor under standing delegation | Maintainer (PR ratification) | — | CodeRabbit |
| CI/coverage gates | Conductor | Maintainer | — | — |
| Production deploy | Out of scope for this intent | Maintainer | — | — |

## Capacity allocation agreement

Serial, single-lane: each stage/unit completes its loop (work → sensors →
learnings → gate) before the next starts. No parallel human work on the
intent branch. Escalation: any discovery that contradicts a ratified upstream
decision surfaces at the current gate with the correction noted, not by
silently rewriting upstream artifacts (corrections-register pattern).

## Onboarding checklist

For the personas as they engage: stage directive + consumes[] artifacts +
`aidlc-state.md` + relevant in-repo skills (`octane`,
`rust-async-patterns`, `rust-best-practices`, `tdd`, `frontend-design`,
`semver-conventions`, `ox`, `use-railway`).

## Assumptions & Open Questions

None.
