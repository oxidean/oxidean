# References — Oxpilot Agent

## Code surfaces

- `crates/oxidean-api-oxpilot` — settings, session core, connector
  gateway/client, agent-runtime hooks
- `apps/web/src/pages/oxpilot.astro`, `pages/oxpilot/`,
  `components/oxpilot/`, `lib/oxpilot-*` — UI + streaming
- `apps/web/src/pages/admin/oxpilot.astro` — admin surface
- `crates/oxidean-db/migrations/*/0085–0093_oxpilot*` — persistence

## Archive + provenance

- AI-DLC intent:
  [`../_archive/aidlc/261027-oxpilot-agent/`](../_archive/aidlc/261027-oxpilot-agent/)
  — units: session-core, agent-runtime, providers, oxpilot-tools,
  connector-gateway, connector-client, oxpilot-ui
- Shipped: PR #203
