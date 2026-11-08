---
phase: 08-git-https-pats
plan: "07"
subsystem: infra
tags: [traefik, smart-http, compose, smoke, git-02, pathregexp]

requires:
  - phase: 08-git-https-pats
    provides: 08-06 Smart HTTP ACL/auth on Axum /{owner}/{repo}.git
provides:
  - "Traefik PathRegexp ^/[^/]+/[^/]+\\.git → API priority 110 (D-18/D-22)"
  - "make smoke-git-https / scripts/smoke-git-https.sh for Compose ls-remote (+ optional PAT push)"
affects:
  - 08-09 CloneBox how-to
  - 08-13 docs (CONFIGURATION Traefik note)
  - operator make up + smoke-git-https

actuals:
  tokens: 1868
  tasks: 1
  commits: 5

plan_head_before: a74989927c96653cb96a7936361d337ba0a8f8f5

tech-stack:
  added: []
  patterns:
    - "Dedicated Traefik router api-git (prio 110) → same service api; leave api@100 and web@1"
    - "Compose git smoke: assert non-HTML on info/refs before git ls-remote; skip cleanly without Docker"

key-files:
  created:
    - scripts/smoke-git-https.sh
    - .planning/phases/08-git-https-pats/08-07-SUMMARY.md
  modified:
    - docker-compose.yml
    - Makefile

key-decisions:
  - "Separate api-git router with service=api rather than widening the PathPrefix rule (keeps /api|/uploads|/health at prio 100)"
  - "Token examples use oxidean_pat_ / oxidean_fg_ (D-18 lock / critical deviation — not ona_*)"
  - "Smoke skips exit 0 when Docker absent; fails on HTML/SPA; ls-remote needs public SMOKE_GIT_OWNER/REPO"

patterns-established:
  - "Traefik .git PathRegexp priority 110 before SPA catch-all"
  - "make smoke-git-https against already-up stack (not full compose bring-up)"

requirements-completed: [GIT-02]

coverage:
  - id: D1
    description: "Traefik routes PathRegexp ^/[^/]+/[^/]+\\.git to API at priority 110"
    requirement: GIT-02
    verification:
      - kind: other
        ref: "rg -n 'PathRegexp|priority=110' docker-compose.yml"
        status: pass
    human_judgment: false
  - id: D2
    description: "smoke-git-https asserts non-HTML .git routing and git ls-remote through public origin"
    requirement: GIT-02
    verification:
      - kind: e2e
        ref: "make smoke-git-https / scripts/smoke-git-https.sh"
        status: unknown
    human_judgment: true
    rationale: "Docker engine unavailable on executor host; script present and wired — operator/CI must run against make up"

duration: 6min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 07: Traefik .git + smoke-git-https Summary

**Traefik PathRegexp routes `/{owner}/{repo}.git` to the API at priority 110; `make smoke-git-https` checks non-HTML Smart HTTP and optional PAT push**

## Performance

- **Duration:** 6 min
- **Started:** 2026-09-13T18:58:21Z
- **Completed:** 2026-09-13T19:04:30Z
- **Tasks:** 1
- **Files modified:** 3

## Accomplishments
- API service label `api-git`: `Host(localhost) && PathRegexp(^/[^/]+/[^/]+\.git)` at priority 110 → service `api` (D-18 / D-22)
- Existing API router priority 100 and web catch-all priority 1 unchanged; `OXIDEAN_PUBLIC_ORIGIN` semantics untouched (D-19)
- `scripts/smoke-git-https.sh` + `make smoke-git-https`: health wait, fail on text/html, `git ls-remote`, optional `SMOKE_PAT` push (`oxidean_pat_…`)

## Task Commits

1. **Task 1: Traefik .git PathRegexp + smoke-git-https** - `a315997` (feat)

**Plan metadata:** `b7aac8d`, `3c8dc2b`, `f944cd5` (docs: complete plan)

## Files Created/Modified
- `docker-compose.yml` — `api-git` Traefik router (PathRegexp + priority 110)
- `scripts/smoke-git-https.sh` — Compose Smart HTTP smoke
- `Makefile` — `smoke-git-https` target + help line

## Decisions Made
- Separate `api-git` router pointing at existing `api` service instead of merging PathRegexp into the PathPrefix rule — clearer priority story and matches RESEARCH.
- Smoke documents prerequisite public repo via `SMOKE_GIT_OWNER` / `SMOKE_GIT_REPO`; does not invent DB seed helpers. Docker-missing hosts skip exit 0 (ASSUME from plan).

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- Docker engine unreachable on executor host — automated verify limited to `rg` / file presence; live `git ls-remote` through Traefik left for operator (`make up` then `make smoke-git-https`). Recorded in coverage D2 as `human_judgment: true`.

## User Setup Required
None - no external service configuration required. Operator needs Docker for live smoke.

## Next Phase Readiness
- Traefik pitfall closed for Compose; CloneBox / docs plans can cite PathRegexp + smoke target.
- Remaining Phase 08 plans: 08-09 … 08-13 (08-08 already summarized).

---
*Phase: 08-git-https-pats*
*Completed: 2026-09-13*

## Self-Check: PASSED

- FOUND: docker-compose.yml (PathRegexp + priority=110)
- FOUND: scripts/smoke-git-https.sh
- FOUND: 08-07-SUMMARY.md
- FOUND: commit a315997
