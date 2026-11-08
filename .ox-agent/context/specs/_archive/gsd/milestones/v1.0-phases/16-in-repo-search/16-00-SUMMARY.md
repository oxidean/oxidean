---
phase: 16-in-repo-search
plan: "00"
subsystem: testing
tags: [nextest, vitest, wave0, repo.search, GIT-18]
requires:
  - phase: 12-pull-requests
    provides: PR domain assumed for later pulls type plans
provides:
  - Discoverable ignored nextest stubs for repo_search_* cluster
  - Vitest it.todo stub for /$owner/$repo/search route
affects: [16-01, 16-02, 16-03]
actuals:
  tokens: 834
  tasks: 2
  commits: 2
tech-stack:
  added: []
  patterns: [Wave 0 #[ignore] nextest stubs, Vitest it.todo route stubs]
key-files:
  created:
    - crates/oxidean-api/tests/repo_search.rs
    - apps/web/src/routes/$owner.$repo.search.integration.test.ts
  modified: []
key-decisions:
  - "Used #[ignore] stubs (not failing asserts) so CI stays green while filters remain discoverable via --run-ignored all"
requirements-completed: [GIT-18]
coverage:
  - id: D1
    description: "nextest lists all six repo_search_* test names"
    requirement: GIT-18
    verification:
      - kind: unit
        ref: "cargo nextest list -p oxidean-api -E 'test(repo_search)' --run-ignored all"
        status: pass
    human_judgment: false
  - id: D2
    description: "Vitest search integration stub file with type-tab coverage intent"
    requirement: GIT-18
    verification:
      - kind: other
        ref: "apps/web/src/routes/$owner.$repo.search.integration.test.ts"
        status: pass
    human_judgment: false
duration: 20min
completed: 2026-09-16
status: complete
plan_head_before: b01a62ba624170562bae32fbae53121cadd96351
commits: 2
---

# Phase 16 Plan 00: Wave 0 Search Stubs Summary

**Discoverable RED nextest + Vitest stubs for GIT-18 `repo.search` (code/commits/issues/pulls/ACL/limits) with no production handlers.**

## Performance

- **Duration:** 20min
- **Started:** 2026-09-16T13:26:40Z
- **Completed:** 2026-09-16T13:46:00Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Six `repo_search_*` ignored integration stubs listable via nextest filters
- Vitest stub documents Code/Commits/Issues/Pull requests tab + query coverage
- No product search behavior shipped (Wave 0 only)

## Task Commits

1. **Task 1: RED API stubs for repo.search cluster** - `db2cce8` (test)
2. **Task 2: RED Vitest stub for search route** - `132104e` (test)

## Files Created/Modified

- `crates/oxidean-api/tests/repo_search.rs` — Wave 0 ignored stubs for all search filters
- `apps/web/src/routes/$owner.$repo.search.integration.test.ts` — `it.todo` UI coverage intents

## Decisions Made

- Prefer `#[ignore]` over hard-failing tests so default CI does not fail while Wave 0 names stay discoverable with `--run-ignored all`

## Deviations from Plan

None - plan executed exactly as written (minor: dropped unused `support` lock after clippy/sync-lock lint on `let _ =`).

## Self-Check: PASSED

- FOUND: crates/oxidean-api/tests/repo_search.rs
- FOUND: apps/web/src/routes/$owner.$repo.search.integration.test.ts
- FOUND: db2cce8, 132104e
