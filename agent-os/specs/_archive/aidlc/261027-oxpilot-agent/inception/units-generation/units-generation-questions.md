# Units Generation Questions — `/oxpilot`

Upstream inputs: `inception/domain-design/components.md` (8 components),
`inception/domain-design/decisions.md` (ADR-001..010),
`inception/requirements-analysis/requirements.md`,
`inception/user-stories/stories.md`. Walking Skeleton is ON — the first
DAG unit must be a running end-to-end slice.

## Sources

- [desc] Intent: `/oxpilot` agentic chat/builder/debugger with opt-in computer use.
- [scope] Workflow-selected scope: `feature`.

## Q1. Unit boundary strategy?

- A. Component-aligned (1 unit ≈ 1 domain-design component), merged only where a component is too thin alone — matches the affirmed "units align 1:1 with component boundaries" practice.
- B. Feature-aligned (chat unit, builder unit, debugger unit, connector unit).
- C. Layer-aligned (db unit, api unit, ui unit).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — component-aligned units. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. Unit granularity and count?

- A. Eight units: `skeleton-slice` (crate+route+page+stub provider+SSE round trip), `session-core` (SessionService), `agent-runtime`, `providers` (registry+adapters+admin settings), `tools` (read/proposal/PR surface), `connector-gateway`, `connector-client` (`ox pilot`), `oxpilot-ui` (full surfaces beyond the skeleton page).
- B. Five units — fold providers into runtime, gateway+client into one connector unit.
- C. Ten-plus — split UI per surface, split db migrations out.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — eight units. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Skeleton slice contents (u1)?

- A. `/oxpilot` page → `oxpilot.session_create/list` → stub provider → streamed reply over the SSE route — proves route registration, domain crate, SessionOnly auth, SSE transport, and page mount end-to-end; everything behind `oxpilot.disabled` gating.
- B. Crate + db only (no UI) — thinner but not an integrated slice.
- C. Include a real provider call in the skeleton — needs secrets/config before the slice exists.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — the chat round-trip skeleton (stub provider, real transport). Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Connector client unit boundary?

- A. `connector-client` is its own unit living inside the `ox` crate (subcommand family + client loop) — a unit may span its component's codebase even though `ox` is a separate binary; the gateway it talks to is a different unit.
- B. Ship connector client code inside `connector-gateway` — one connector unit.
- C. New standalone crate for the client.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `ox pilot` client is its own unit, separate from the server-side gateway. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Boundaries: component-aligned units (Q1).
- Count: 8 — skeleton-slice, session-core, agent-runtime, providers, tools, connector-gateway, connector-client, oxpilot-ui (Q2).
- u1 = the walking-skeleton chat round-trip (Q3).
- Connector client = own unit inside `ox` (Q4).

Does this all look correct before I generate the unit artifacts and DAG?

- Looks correct
- Request changes

[Answer]: Looks correct
