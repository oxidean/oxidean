---
phase: 19-actions-runners
plan: "01"
subsystem: testing
tags: [actions, vitest, smoke, wave0]

requires:
  - phase: 20-packages-registry
    provides: it.fails Wave 0 Vitest + skip-ok smoke pattern
provides:
  - Wave 0 Vitest stubs for Actions list/detail UI
  - smoke-actions.sh + make smoke-actions
affects: [19-08, 19-09, 19-11]

actuals:
  tokens: 24
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns: [it.fails Wave 0 Vitest stubs, skip-ok Compose smoke]

key-files:
  created:
    - apps/web/src/routes/$owner.$repo.actions.integration.test.ts
    - apps/web/src/routes/$owner.$repo.actions.$run.integration.test.ts
    - scripts/smoke-actions.sh
  modified:
    - Makefile

key-decisions:
  - "Used it.fails for intentional RED UI stubs until 19-09"

patterns-established:
  - "Actions UI integration tests live beside $owner.$repo.actions*.tsrx routes"

requirements-completed: [ACT-03, ACT-04, ACT-05]

coverage:
  - id: D1
    description: Wave 0 Actions UI Vitest stubs
    requirement: ACT-03
    verification:
      - kind: unit
        ref: apps/web/src/routes/$owner.$repo.actions.integration.test.ts
        status: pass
    human_judgment: false
  - id: D2
    description: smoke-actions skip-ok Make target
    requirement: ACT-04
    verification:
      - kind: other
        ref: make smoke-actions / scripts/smoke-actions.sh
        status: pass
    human_judgment: false

plan_head_before: 0b0d1cc
duration: 2min
completed: 2026-09-16
status: complete
---

# Phase 19 Plan 01: Wave 0 Web + Smoke Summary

**Nyquist Wave 0 Vitest stubs for Actions list/detail UI plus skip-ok Compose smoke for ACT-04/05.**

## Performance

- **Duration:** ~2 min
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Added Actions list and run-detail Vitest stubs (`it.fails`) for D-ACT-12/13/18
- Added `scripts/smoke-actions.sh` + `make smoke-actions` (skip without Docker/stack)

## Task Commits

1. **Task 1: Actions Vitest Wave 0 stubs** - `c50461e` (test)
2. **Task 2: smoke-actions stub + Make target** - `80c0fae` (chore)

## Files Created/Modified
- `apps/web/src/routes/$owner.$repo.actions.integration.test.ts` — list UI stubs
- `apps/web/src/routes/$owner.$repo.actions.$run.integration.test.ts` — detail/logs stubs
- `scripts/smoke-actions.sh` — skip-ok smoke scaffold
- `Makefile` — `smoke-actions` target + help

## Decisions Made
- `it.fails` for intentional RED stubs matching Phase 20 Wave 0 pattern

## Deviations from Plan

None - plan executed exactly as written.

## Self-Check: PASSED

- FOUND: apps/web/src/routes/$owner.$repo.actions.integration.test.ts
- FOUND: apps/web/src/routes/$owner.$repo.actions.$run.integration.test.ts
- FOUND: scripts/smoke-actions.sh
- FOUND: c50461e
- FOUND: 80c0fae
