---
phase: 22-compose-ci-deploy
plan: "01"
subsystem: infra
tags: [github-actions, docker-compose, smoke, postgres, sqlite, mysql, ci]

requires:
  - phase: 02-multi-db-storage
    provides: make smoke / smoke-sqlite / smoke-mysql + compose-smoke.sh dialect asserts
  - phase: 11.1-quality-hardening
    provides: smoke-protocol CI fail-closed pattern + artifact upload
provides:
  - PR CI compose-smoke matrix (postgres/sqlite/mysql) via existing Make smoke targets
  - scripts/ci-compose-smoke.sh fail-closed CI wrapper
  - docs/TESTING.md CI table row for compose-smoke
affects: [22-02-cloud-deploy, PLAT-03, PLAT-09]

actuals:
  tokens: 2036
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Thin CI wrapper exports CI/SMOKE_REQUIRE_STACK then delegates to make smoke*"
    - "compose-smoke matrix fail-fast:false; keep smoke-protocol separate"

key-files:
  created:
    - scripts/ci-compose-smoke.sh
  modified:
    - .github/workflows/ci.yml
    - Makefile
    - docs/TESTING.md

key-decisions:
  - "Single compose-smoke job with strategy.matrix dialects rather than three sibling jobs"
  - "Reuse make smoke* unchanged; wrapper only sets fail-closed env and dispatches dialect"

patterns-established:
  - "ci-compose-smoke.sh mirrors ci-smoke-protocol.sh entrypoint style"
  - "make smoke-compose-ci DIALECT=… for local parity with GHA"

requirements-completed: [PLAT-03, PLAT-09]

coverage:
  - id: D1
    description: "PR CI compose-smoke job builds/brings up Postgres Compose and asserts health/db_probe"
    requirement: PLAT-03
    verification:
      - kind: other
        ref: "rg compose-smoke .github/workflows/ci.yml; python yaml matrix assert"
        status: pass
    human_judgment: false
  - id: D2
    description: "PR CI compose-smoke matrix includes SQLite and MySQL bring-up legs"
    requirement: PLAT-09
    verification:
      - kind: other
        ref: "ci.yml matrix dialect: [postgres, sqlite, mysql]"
        status: pass
    human_judgment: false
  - id: D3
    description: "TESTING.md documents compose-smoke alongside compose / smoke-protocol / db-matrix"
    requirement: PLAT-03
    verification:
      - kind: other
        ref: "docs/TESTING.md CI jobs table"
        status: pass
    human_judgment: false

plan_head_before: 7f796912a48f83675da2be0f2e58ee1c0122e8c6
duration: 2min
completed: 2026-09-16
status: complete
---

# Phase 22 Plan 01: Compose CI Dialect Matrix Summary

**PR CI now runs Compose bring-up smokes for Postgres, SQLite, and MySQL via existing `make smoke*` targets, without inventing a parallel stack.**

## Performance

- **Duration:** 2 min
- **Started:** 2026-09-16T18:18:32Z
- **Completed:** 2026-09-16T18:20:31Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- Added `compose-smoke` GitHub Actions matrix (`postgres` / `sqlite` / `mysql`) with `fail-fast: false`, free-disk step, and artifact upload on failure
- Introduced `scripts/ci-compose-smoke.sh` + `make smoke-compose-ci` that fail-closed and delegate to existing Make smoke recipes
- Documented the job and local targets in `docs/TESTING.md`; left config-only `compose` and `smoke-protocol` independent

## Task Commits

1. **Task 1: End-to-end CI Compose bring-up — Postgres path only** - `d129008` (feat)
2. **Task 2: SQLite Compose bring-up matrix leg** - `48c91e1` (feat)
3. **Task 3: MySQL Compose bring-up in PR CI + TESTING docs** - `f690a96` (feat)

## Files Created/Modified

- `scripts/ci-compose-smoke.sh` - Fail-closed CI entrypoint dispatching by dialect
- `.github/workflows/ci.yml` - `compose-smoke` matrix job
- `Makefile` - `smoke-compose-ci` target + help
- `docs/TESTING.md` - CI table + compose dialect smoke section

## Decisions Made

- Used one matrix job (not three siblings) with `fail-fast: false` so one dialect failure does not cancel others
- SQLite on GHA relies on native Linux `./var` from `sqlite-host-dir.sh` (no WSL docker.exe path)

## Deviations from Plan

None - plan executed exactly as written.

## Known Stubs

None.

## Self-Check: PASSED

- FOUND: scripts/ci-compose-smoke.sh
- FOUND: .github/workflows/ci.yml compose-smoke matrix
- FOUND: commits d129008, 48c91e1, f690a96
