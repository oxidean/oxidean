# Intent Backlog — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `intent-capture/intent-statement.md`,
`feasibility/feasibility-assessment.md`, `feasibility/constraint-register.md`;
proto-Units for delivery-planning to shape into the Unit DAG (prior
intents' learning: align units 1:1 with domain-design component boundaries —
this list is the capability ordering, not the final unit map).

## Ordering rationale

Risk-first inside a dependency-valid order (Q5): the foundation chain must
exist before anything else; the two highest risks — edge streaming (RAID R-3)
and the connector security model (RAID R-1) — are retired early; UI polish
and coverage complete each slice.

## Proto-Units (MoSCoW + dependency order)

| # | Proto-unit | MoSCoW | Depends on | Value / risk retired |
|---|---|---|---|---|
| P-1 | Foundation: feature flag + provider config (`OXIDEAN_*` env, `.env.example`/docs) + session/transcript schema in `oxidean-db` + `oxpilot.*` RPC skeleton | Must | — | TC-5/TC-6; unblocks everything |
| P-2 | `/oxpilot` page + route-coverage row + agent chat endpoint (SSE through `/api/`) + `useChat`/`createChatHook` integration | Must | P-1 | R-3 (edge streaming spike) — first real round-trip |
| P-3 | Forge context tools: repo/file read, issue/PR references, repo metadata — server tools behind the forge-tool layer | Must | P-2 | Chat becomes repo-aware |
| P-4 | Builder surface: propose changes → branch + patch/PR via existing VCS/RPC paths; diff preview card | Must | P-3 | Core named capability |
| P-5 | Debugger surface: Actions run/build-log reads (verify D-4 coverage at requirements-analysis) + failure-analysis tool | Must | P-3 | Core named capability |
| P-6 | Computer-use connector: consent + PAT-class credential issuance, outbound dial-in channel, per-action authorization + audit, user-side connector spec | Must | P-2 | R-1 — biggest security surface |
| P-7 | Session UX: list/resume sessions, queued follow-ups, interrupts/approvals UI | Should | P-2 | Usability depth |
| P-8 | Hardening: transcript redaction/retention hooks (R-4), rate limits, audit trail surfacing, operator docs | Must | P-1..P-6 | RC-2 compliance posture |
| P-9 | E2E evidence: browser tests for high-risk UI, stack e2e for chat round-trip + connector handshake, manifest rows | Must | all | Coverage gates |

## Notes for delivery-planning

- P-1/P-2 should fuse into the first construction unit so the streaming spike
  rides a real page (feasibility sequencing rule).
- P-6's connector protocol is the only genuinely new wire surface — expect it
  to split into protocol spec + server endpoint + connector reference impl in
  domain-design.
- P-9 is not a terminal "test" unit: coverage evidence lands per-unit per the
  testing posture; P-9 is the residual e2e sweep.
- Story-map convention (learned): a multi-unit story gets ONE primary unit;
  P-2 is the primary owner of the chat story, P-4/P-5 secondary cells.

## Assumptions & Open Questions

- [assumption] Proto-unit boundaries are provisional — delivery-planning may
  merge P-1+P-2 (foundation+page spike) and split P-6 (connector protocol vs
  connector client); the ordering, not the count, is the committed part.
