# Deployment Strategy — oxpilot

## Strategy

Rolling redeploy (Railway service image retag) — the project's established pattern; no canary/blue-green infra exists or is added by this intent.

## Safety posture

- **Flag-gated rollout:** the entire feature is inert unless `OXIDEAN_OXPILOT_ENABLED=true`; SSE routes deny when disabled (tested: `sse_routes_deny_when_disabled`). Blast radius = one env var.
- **Single-replica caveat:** active-turn state is in-memory (documented NFR); a mid-turn deploy drops in-flight turns — acceptable for flag-off default + staged opt-in; transcripts persist durable-first so users lose no history.
- **DB migrations:** forward-only, idempotent, dialect-appropriate (`0087`–`0090` verified in `db-dialects` CI); `ON DELETE SET NULL` on optional FKs.
- **Budget safety:** unbounded defaults; operators opt into ceilings.

## Environment promotion matrix

| Env | Channel | Flag | Gate |
|---|---|---|---|
| preview/PR | `:main` | off (operator may opt in) | PR required checks |
| staging | `:staging` | opt-in | plan/apply approval |
| production | `:production` | opt-in | plan/apply approval |

## Verification hooks

Post-deploy smoke: `/health` on api + web; flag-off deny on `/api/oxpilot` SSE; when enabled — session create → stub-provider turn → SSE snapshot/append contract (same assertions as `e2e-stack`).
