---
phase: 20-packages-registry
plan: "01"
subsystem: testing
tags: [packages, vitest, smoke, wave0, ui]

requires:
  - phase: 20-packages-registry
    provides: CONTEXT D-PKG decisions
provides:
  - Wave 0 Vitest stubs for owner/repo/admin/tokens packages UI
  - smoke-packages.sh + make smoke-packages
affects: [20-10, 20-11, 20-12]

actuals:
  tokens: 1818
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns: [it.fails Wave 0 Vitest stubs, skip-ok Compose smoke]

key-files:
  created:
    - apps/web/src/routes/$owner.packages.integration.test.ts
    - apps/web/src/routes/$owner.$repo.packages.integration.test.ts
    - apps/web/src/routes/admin/packages.integration.test.ts
    - apps/web/src/routes/settings/tokens.packages.integration.test.ts
    - scripts/smoke-packages.sh
  modified:
    - Makefile

key-decisions:
  - "Used it.fails for intentional RED UI stubs so Vitest reports expected fail"

patterns-established:
  - "make smoke-packages mirrors smoke-git-https skip-ok Docker pattern"

requirements-completed: [PKG-05, PKG-01, PKG-02, PKG-03]

coverage:
  - id: D1
    description: Wave 0 packages UI Vitest stubs
    requirement: PKG-05
    verification:
      - kind: automated_ui
        ref: bun run test -- src/routes/$owner.packages.integration.test.ts
        status: pass
    human_judgment: false
  - id: D2
    description: smoke-packages Make target
    verification:
      - kind: other
        ref: bash -n scripts/smoke-packages.sh
        status: pass
    human_judgment: false

plan_head_before: 051bb7a98dc6430bfe0f70b24e9d6108fd47a37f
duration: 4min
completed: 2026-09-14
status: complete
---

# Phase 20 Plan 01: Wave 0 Web + Smoke Summary

**Nyquist Wave 0 Vitest stubs for packages UI surfaces plus skip-ok Traefik smoke for /v2|/npm|/generic.**

## Performance

- **Duration:** ~4 min
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- Owner, repo-linked, Admin quota, and tokens package-scope UI stubs (`it.fails`)
- `scripts/smoke-packages.sh` + `make smoke-packages` (Docker-missing → exit 0)

## Task Commits

1. **Task 1: Packages UI Wave 0 Vitest stubs** - `a6692ab` (test)
2. **Task 2: smoke-packages script + Make target** - `8c00913` (chore)

## Files Created/Modified
- `apps/web/src/routes/$owner.packages.integration.test.ts`
- `apps/web/src/routes/$owner.$repo.packages.integration.test.ts`
- `apps/web/src/routes/admin/packages.integration.test.ts`
- `apps/web/src/routes/settings/tokens.packages.integration.test.ts`
- `scripts/smoke-packages.sh`
- `Makefile`

## Decisions Made
- `it.fails` for intentional RED stubs matching Vitest expected-fail semantics

## Deviations from Plan
None - plan executed as written (verify used `bun run test` / `bunx vitest` instead of `bun --cwd apps/web exec vitest` which is not a valid bun script).

## Self-Check: PASSED
- FOUND: all four Vitest stub files
- FOUND: scripts/smoke-packages.sh
- FOUND: a6692ab, 8c00913
