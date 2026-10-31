---
phase: 14-git-lfs
plan: "00"
subsystem: testing
tags: [git-lfs, nextest, wave0, smoke]

requires:
  - phase: 11-issues
    provides: factory_reset_scope and dialect test patterns
provides:
  - Discoverable Wave 0 nextest stubs for lfs_batch/lfs_store/lfs_enable/lfs_quota/lfs_dedup/lfs_verify/lfs_gc
  - dialect_lfs stub and smoke-git-lfs Makefile target
affects: [14-02, 14-03, 14-04, 14-05, 14-06, 14-08]

actuals:
  tokens: 2500
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns: [Wave 0 discoverable empty/placeholder nextest stubs]

key-files:
  created:
    - crates/oxidean-api/tests/lfs_batch.rs
    - crates/oxidean-api/tests/lfs_store.rs
    - crates/oxidean-db/tests/dialect_lfs.rs
    - scripts/smoke-git-lfs.sh
  modified:
    - crates/oxidean-api/tests/factory_reset_scope.rs
    - Makefile

key-decisions:
  - "Wave 0 stubs are empty passing tests (not #[ignore]) so default nextest list filters match plan verify"
  - "dialect_lfs early-returns until migration files exist (14-03/14-02)"

patterns-established:
  - "LFS nextest filter names: lfs_batch, lfs_store, lfs_enable, lfs_quota, lfs_dedup, lfs_verify, lfs_gc"

requirements-completed: []

coverage:
  - id: D1
    description: "API Wave 0 LFS stubs discoverable under named nextest filters"
    requirement: GIT-12
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(lfs)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Dialect + factory-reset + smoke scaffolds on disk"
    requirement: GIT-13
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-db -E 'test(dialect_lfs)'; rg smoke-git-lfs Makefile"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-09-14
status: complete
plan_head_before: 27404c39cc993eae813af77b0641c420925cbc42
commits: 2
---

# Phase 14 Plan 00: Wave 0 Rust/smoke stubs Summary

**Discoverable nextest + smoke scaffolds for Git LFS before implementation greens them.**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-09-14T16:54:00Z
- **Completed:** 2026-09-14T17:00:00Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- Named API stubs for batch/auth/enable/quota/dedup and store/verify/gc
- dialect_lfs + factory_reset LFS_DIR wipe stub + `make smoke-git-lfs`

## Task Commits

1. **Task 1: LFS batch/store Wave 0 API stubs** - `26788d4` (test)
2. **Task 2: Dialect + factory reset + smoke scaffolds** - `1d17605` (test)

## Files Created/Modified
- `crates/oxidean-api/tests/lfs_batch.rs` — batch/auth/enable/quota/dedup stubs
- `crates/oxidean-api/tests/lfs_store.rs` — shard/verify/gc stubs
- `crates/oxidean-db/tests/dialect_lfs.rs` — migration parity stub
- `scripts/smoke-git-lfs.sh` — Traefik LFS routing smoke (docker-skip)
- `Makefile` — `smoke-git-lfs` target
- `factory_reset_scope.rs` — D-LFS-04 wipe stub

## Decisions Made
Empty passing stubs instead of `#[ignore]` so plan verify `nextest list` filters succeed without `--run-ignored`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] nextest list hides #[ignore] by default**
- **Found during:** Task 1 verify
- **Issue:** Plan verify `cargo nextest list -E 'test(lfs)'` finds nothing for ignored tests
- **Fix:** Use empty passing placeholder bodies instead of `#[ignore]` / `assert!(false)`
- **Files modified:** lfs_batch.rs, lfs_store.rs, dialect_lfs.rs, factory_reset_scope.rs
- **Commit:** 26788d4 / 1d17605

## Self-Check: PASSED
- FOUND: crates/oxidean-api/tests/lfs_batch.rs
- FOUND: crates/oxidean-api/tests/lfs_store.rs
- FOUND: crates/oxidean-db/tests/dialect_lfs.rs
- FOUND: scripts/smoke-git-lfs.sh
- FOUND: 26788d4, 1d17605
