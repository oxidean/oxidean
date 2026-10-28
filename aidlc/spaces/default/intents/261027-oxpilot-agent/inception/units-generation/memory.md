# Units Generation — stage diary

## What ran

- Questions (4) resolved under standing instruction; summary confirmed.
- Eight component-aligned units defined: skeleton-slice (U1, walking
  skeleton), session-core, agent-runtime, providers, oxpilot-tools,
  connector-gateway, connector-client, oxpilot-ui.
- DAG + story map + traceability written; sensors passed.
- Advisory architecture review: READY, 1 major + 6 minor, all applied.

## Review findings applied

- R-01 (major): exec/fs computer-use tool surface had no owner → added
  to U5 (`oxpilot-tools` dispatches into U6's action queue); U5→U6 edge
  added, matching `OxpilotTools→ConnectorGateway` in components.md.
- R-02: YAML edges now mirror every prose-claimed direct dep (U3←U1,
  U8←U1+U2).
- R-03: story count corrected to 24.
- R-04: US5.5 target aligned to `u1-skeleton-slice`.
- R-05: US5.5/US4.3 contributing units made explicit.
- R-06: `LlmProvider` trait + slot pinned to U1; U4 owns adapters.
- R-07: U2 delete cascade marked phased — own rows + `on_session_delete`
  hook seam; U3/U6 register handlers when they land.

## Learnings candidates surfaced

- Traceability targets must be declared unit names or `u{n}-<name>`
  directories exactly — invented directory spellings fail the sensor.
