# Delivery Planning Questions — `/oxpilot`

This stage picks the economic path through the Units DAG — a **Bolt** is
one build pass over a piece of the work, ending in something that runs.
Inputs: all inception artifacts; affirmed practices resolved from
`memory/{project,team,org}.md`.

## Sources

- [dag] `inception/units-generation/unit-of-work-dependency.md` — 8 units,
  acyclic, skeleton-on.
- [contract] `inception/contract-design/contract-summary.md` — C1..C8.
- [practices] org `## Walking Skeleton` — first DAG Unit runs the full
  integrated slice + checkpoint before later units.

## Q1. Build first — risk, value, or the thin end-to-end slice?

- A. Skeleton-first (mandated by scope `skeleton: on` + org practice): Bolt 1 = u1-skeleton-slice, verified end-to-end (`/oxpilot` page → RPC → stub provider → SSE stream) and checkpoint-approved before any later unit. Then foundation-value order for the rest.
- B. Risk-first — connector gateway first. Rejected: U6 has no running substrate without U1/U2.
- C. Value-first — UI first. Rejected: U8 is the DAG fan-in.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — skeleton first, then dependency-respecting value order. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. Formal scoring model (WSJF)?

- A. No numeric WSJF — 8 units with a hard DAG and one obvious skeleton; ordering is topological + a one-line value note per Bolt in the rationale. A scoring table would be ceremony, not signal.
- B. Full WSJF score per unit.
- C. Lightweight CD3 ranking.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — topological order + qualitative rationale. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Bolt size?

- A. One Bolt = one Unit — units are already Bolt-sized (S–L); bundling would only blur per-unit verification.
- B. Bundle related units.
- C. Thin cross-unit slices.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — one Bolt per unit, 8 Bolts. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Parallel Bolts?

- A. Serial (construction_execution: serial, unit-major iteration) — single conductor; the DAG's parallel-eligible sets ({U2,U4}, {U3,U6}, {U5,U7}) are noted in the rationale but not exploited by one build lane.
- B. Swarm the independent sets.
- C. Parallel only for U7.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — serial unit-major. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. External dependencies?

- A. None blocking: TanStack `ai-octane` reference patterns are vendored knowledge (the registry is TanStack AI, not Octane-server); OpenAI/Anthropic/Ollama APIs are consumed via outbound HTTP with wiremock fixtures — no live keys needed for CI; no external-team hand-off. The map artifact records them as informational only.
- B. Real provider accounts gate the build.
- C. TanStack package publish needed first.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — no blocking external dependencies. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q6. Biggest worry to tackle early?

- A. The SSE turn loop + provider seam (U3) and the connector grant/cancel protocol (U6) — both land as early Bolts (3rd and 5th of 8) rather than at the tail; the U1 skeleton already de-risks the transport.
- B. UI polish risk — late.
- C. Provider API drift — adapter tests cover.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — transport + consent protocol land early post-skeleton. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Construction settings

- Iteration: **unit-major** (default; each unit's design stages + code
  generation run serially, verified at its checkpoint).
- Execution: **serial** (no swarm).
- Checkpoints: **enabled** (default).
- Walking skeleton: **on** — u1-skeleton-slice is the first resolved DAG
  unit and the integrated slice; expected demo = a real `/oxpilot` chat
  round-trip against the stub provider over the live SSE route; the
  verification command below demonstrates it.
- Verification command: **`make test`** (cargo nextest workspace +
  `bun run test`) — the project's real check, reused at every unit
  checkpoint; for the skeleton it must include the new oxpilot
  proc/SSE tests so the slice is proven, not just compiled.

## Consolidated Summary Confirmation

- Bolt order: B1 skeleton-slice → B2 session-core → B3 providers →
  B4 agent-runtime → B5 connector-gateway → B6 oxpilot-tools →
  B7 connector-client → B8 oxpilot-ui. (Topological check: U6 before U5
  because U5 depends on U6's action queue.)
- One Bolt per unit; serial unit-major; no blocking externals (Q3–Q5).
- Verification command `make test` at every checkpoint (Q-settings).

Does this all look correct before I write the four planning artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
