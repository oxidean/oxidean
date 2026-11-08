---
status: complete
phase: 01-monorepo-scaffold
source:
  - 01-01-SUMMARY.md
  - 01-02-SUMMARY.md
  - 01-03-SUMMARY.md
  - 01-04-SUMMARY.md
  - 01-05-SUMMARY.md
started: 2026-09-09T12:40:16.020Z
updated: 2026-09-09T12:51:00.665Z
---

## Current Test

[testing complete]


## Tests

### 1. Cold Start Smoke Test
expected: Compose smoke brings stack up from scratch; Traefik /, /healthz, and RPC system.health succeed; smoke tears down cleanly.
result: issue
reported: "I prefer if we use /health instead of /healthz"
severity: major

### 2. Landing first viewport
expected: Open http://localhost:3000/ (make dev) or http://localhost/ (Compose). See Oxidean brand mark + name as hero signal, headline "Where repositories nest — cloud or yours", CTAs Get started and Explore Oxidean, and placeholder search — not a dashboard of stats/cards.
result: pass

### 3. Theme persistence
expected: Theme control (System / Light / Dark) changes appearance; choosing Dark (or Light), refresh the page, preference remains and UI stays in that mode.
result: pass

### 4. Status page live health
expected: Footer Status link goes to /status titled "System status". With API up, page shows "All systems operational" (not stuck on Checking…). With API down, unreachable copy appears.
result: pass
verified_by: agent (Playwright)

### 5. API reachable from the UI path
expected: Browser or curl through the same origin the UI uses (Vite proxy or Traefik) can hit /healthz and POST /api/rpc system.health with header Oxidean-RPC-Version: 1 and get ok JSON.
result: pass
verified_by: agent (curl via Vite proxy)

### 6. RPC client stays in sync
expected: `make rpc-sync-check` completes successfully (regenerates client and finds no drift vs packages/api-client).
result: pass
verified_by: agent (`make rpc-sync-check`)

## Summary

total: 6
passed: 5
issues: 1
pending: 0
skipped: 0
blocked: 0


## Gaps

- truth: "HTTP probe path is /healthz for Compose/Traefik healthchecks"
  status: resolved
  reason: "User reported: I prefer if we use /health instead of /healthz"
  severity: major
  test: 1
  root_cause: "Phase 1 locked Kubernetes-style /healthz; product preference is /health for the public probe path."
  artifacts:
    - crates/oxidean-api/src/app.rs
    - docker-compose.yml
    - apps/web/vite.config.ts
    - scripts/compose-smoke.sh
  missing:
    - "Rename GET /healthz → GET /health everywhere (API, Traefik, Compose healthcheck, Vite proxy, smoke, docs)"
  debug_session: "agent-self-verify"
  resolution: "Renamed GET /healthz → GET /health across API, Compose, Traefik, Vite proxy, smoke, Dockerfile, docs"
