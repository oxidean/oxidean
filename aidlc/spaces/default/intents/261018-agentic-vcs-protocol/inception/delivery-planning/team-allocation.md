# Team Allocation — Agentic VCS Protocol

A **mob** is the small role set that builds a Bolt; a **Bolt** is one build pass ending in something that runs. Team Formation (1.5) was SKIP under the `classic` scope, so there is no multi-team Program Board: every Bolt is executed by `aidlc-developer-agent` (AI), with the stage-declared support agent consulting inside its stage, not as a separate build team.

| Bolt | Units | Executing mob | Rationale |
|------|-------|---------------|-----------|
| B1 wire-spine | U1, U2 | aidlc-developer-agent | Foundational crate + persistence; no specialist hand needed beyond the in-stage architect consult. |
| B2 changeset-core | U3, U4 | aidlc-developer-agent | Storage + tx discipline; GC race testing is ordinary test-after work. |
| B3 history-algebra | U5, U6 | aidlc-developer-agent | Merge/interop — the riskiest pair; keeps one owner so merge and import share the same changeset invariants. |
| B4 api-surface | U7 | aidlc-developer-agent | Host wiring + routes; the seam points were contract-frozen, so this is assembly. |
| B5 agent-surfaces | U8, U9, U10 | aidlc-developer-agent | Client surfaces; U10 touches `apps/web` Octane — the stage-declared design support reviews the toggle, no separate team. |

## Notes

- Serial unit-major execution means allocation is sequencing, not contention — no two Bolts run concurrently.
- Specialist consults happen inside each unit's applicable stages (architect on design-heavy units, quality on test plans) rather than as parallel build teams.
