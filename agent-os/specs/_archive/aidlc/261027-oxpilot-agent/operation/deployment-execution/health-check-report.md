# Health Check Report — oxpilot

## Existing probes (unchanged)

- `api` — `/health` healthcheck (railway.ts), covers api ↔ postgres connectivity.
- `web` — `/health` healthcheck, covers static serving + API proxy readiness.

## Oxpilot-specific health signals

- **Flag state:** `GET /api/oxpilot` procs + SSE deny with a stable flag-off code when disabled — observable without the feature enabled.
- **Schema:** api boot runs migrations; boot failure surfaces as api healthcheck failure (fail-closed, correct).
- **In-flight turns:** single-replica in-memory caveat — post-deploy, previously in-flight turns are dropped; transcript history persists (durable-first writes). Operators see this as new sessions, not errors.

## Validation

All health surfaces above are the same probes exercised by `e2e-stack` in CI (which boots api+web+db with dev-auth). No new probe endpoint was added — none was needed; oxpilot is inert unless flagged on.
