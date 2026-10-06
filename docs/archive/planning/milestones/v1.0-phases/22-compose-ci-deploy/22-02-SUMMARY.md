---
phase: 22-compose-ci-deploy
plan: "02"
subsystem: infra
tags: [railway, caddy, docker, deploy, postgres, iac]

requires:
  - phase: 22-01
    provides: Compose CI smoke pattern; no cloud secrets in PR CI
  - phase: 01-monorepo-scaffold
    provides: api/web Dockerfiles + Compose Traefik path priorities
provides:
  - .railway/railway.ts TypeScript IaC (DOCKERFILE api/web/gateway + managed Postgres)
  - deploy/cloud Caddy file gateway mirroring Compose routes
  - Operator DEPLOYMENT.md + make cloud-plan (plan-only)
affects: [PLAT-02, 22-03-validation]

actuals:
  tokens: 9222
  tasks: 3
  commits: 2

tech-stack:
  added: ["railway@3.11.0 (nested .railway/)", "caddy:2.11.4-alpine"]
  patterns:
    - "Isolated .railway npm env outside Bun workspaces"
    - "preserve() for cloud secrets; human-only railway config apply"
    - "Single forge-data volume at /var for Compose-aligned paths"

key-files:
  created:
    - .railway/railway.ts
    - .railway/package.json
    - .railway/README.md
    - deploy/cloud/Caddyfile
    - deploy/cloud/Dockerfile
    - deploy/cloud/README.md
  modified:
    - docs/DEPLOYMENT.md
    - .env.example
    - Makefile

key-decisions:
  - "Caddy 2.11.4-alpine official image for cloud gateway (approved)"
  - "One forge-data volume mounted at /var covering repos/lfs/packages/release-assets/uploads/ssh"
  - "Omit invalid api --migrate preDeploy; document temporary AUTO_MIGRATE for first boot"

patterns-established:
  - "make cloud-plan never applies; apply is operator-only"
  - "Gateway uses RAILWAY_PRIVATE_DOMAIN for api/web upstreams"

requirements-completed: [PLAT-02]

coverage:
  - id: D1
    description: "Railway IaC references existing api/web Dockerfiles + managed Postgres + Caddy gateway"
    requirement: PLAT-02
    verification:
      - kind: other
        ref: "test -f .railway/railway.ts && deploy/cloud/Caddyfile; tsx import"
        status: pass
    human_judgment: false
  - id: D2
    description: "DEPLOYMENT.md documents Railway-class operator steps without VERIFY stubs for happy path"
    requirement: PLAT-02
    verification:
      - kind: other
        ref: "rg Oxidean Cloud docs/DEPLOYMENT.md"
        status: pass
    human_judgment: false
  - id: D3
    description: "Live railway config apply remains human-only"
    requirement: PLAT-02
    verification: []
    human_judgment: true
    rationale: "D-CLOUD-07 — production apply requires operator token and explicit approval"

plan_head_before: 8e52c8643400853212413fb54ea75b0f90bc4355
duration: 12min
completed: 2026-09-16
status: complete
---

# Phase 22 Plan 02: Railway Cloud Deploy Path Summary

**Oxidean Cloud IaC deploys the same api/web Dockerfiles behind a Caddy file gateway with managed Postgres — no Docker-socket Traefik, no secrets in git.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-09-16T18:22:35Z
- **Completed:** 2026-09-16T18:30:00Z
- **Tasks:** 3 (checkpoint + tracer + docs)
- **Files modified:** 11

## Accomplishments

- Nested `.railway/` with `railway@3.11.0` IaC: postgres + api + web + gateway (DOCKERFILE builders)
- `deploy/cloud` Caddy gateway mirrors Compose git/packages/api/SPA priorities
- Operator docs + `make cloud-plan` / `cloud-docs`; forge-data volume at `/var`

## Task Commits

1. **Task 1: Package legitimacy checkpoint** - approved (npm `railway`, Docker Hub `caddy:2.11.4-alpine`) — no commit
2. **Task 2: End-to-end cloud path — gateway + IaC** - `eaa457a` (feat)
3. **Task 3: Volumes, cloud env docs, Make helpers, SSH TCP note** - `1e1e2e7` (feat)

## Files Created/Modified

- `.railway/railway.ts` — IaC topology
- `.railway/package.json` / `package-lock.json` — isolated SDK install
- `deploy/cloud/{Caddyfile,Dockerfile,README.md}` — edge gateway
- `docs/DEPLOYMENT.md` — concrete Railway operator path
- `.env.example` — commented cloud section
- `Makefile` — `cloud-plan` / `cloud-docs`

## Decisions Made

- Pin `caddy:2.11.4-alpine` (approved Official Image)
- Single `forge-data` volume at `/var` instead of six separate mounts (Railway-friendly; same Compose paths)
- No `oxidean-api --migrate` preDeploy (binary lacks migrate CLI) — document temporary `OXIDEAN_AUTO_MIGRATE=true` for first boot

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Invalid migrate preDeploy removed**
- **Found during:** Task 2
- **Issue:** API image has no `--migrate` flag / migrate binary
- **Fix:** Document first-boot AUTO_MIGRATE procedure instead of broken preDeploy
- **Files modified:** `.railway/railway.ts`, `docs/DEPLOYMENT.md`
- **Commit:** `eaa457a` / `1e1e2e7`

**2. [Rule 2 - Ops] Consolidated volumes to one `/var` mount**
- **Found during:** Task 3
- **Issue:** Multi-volume per service is awkward on Railway-class hosts
- **Fix:** `forge-data` → `/var` with Compose-aligned subdirs
- **Files modified:** `.railway/railway.ts`
- **Commit:** `1e1e2e7`

## Auth Gates

Package legitimacy checkpoint completed with human APPROVED for `railway` npm + `caddy` Official Image.

## Known Stubs

None that block PLAT-02 (live apply intentionally human-verified).

## Self-Check: PASSED

- FOUND: .railway/railway.ts, deploy/cloud/Caddyfile, deploy/cloud/Dockerfile
- FOUND: commits eaa457a, 1e1e2e7
- FOUND: no railway.json / railway.toml
