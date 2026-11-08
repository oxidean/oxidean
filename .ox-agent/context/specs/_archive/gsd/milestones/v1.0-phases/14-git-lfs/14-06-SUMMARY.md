---
phase: 14-git-lfs
plan: "06"
subsystem: api
tags: [git-lfs, gc, factory-reset]

requires:
  - phase: 14-git-lfs
    provides: Dedup/store (14-05)
provides:
  - Periodic LFS OID GC with lockfile
  - Factory reset wipes LFS_DIR
affects: [14-12]

actuals:
  tokens: 8000
  tasks: 2
  commits: 1

tech-stack:
  added: []
  patterns:
    - "LFS_DIR/.gc.lock serializes GC"
    - "wipe_lfs_dir_contents mirrors wipe_repos_dir_contents"

key-files:
  created:
    - crates/oxidean-api/src/jobs/lfs_gc.rs
  modified:
    - crates/oxidean-api/src/jobs/schedule.rs
    - crates/oxidean-api/src/auth/admin.rs
    - crates/oxidean-api/tests/factory_reset_scope.rs

key-decisions:
  - "Grace 0 uses far-future cutoff for tests; default grace 7d"

requirements-completed: [GIT-13]

coverage:
  - id: D1
    description: "GC deletes unreferenced OID; factory reset wipes LFS children"
    requirement: GIT-13
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(lfs_gc) | test(factory_reset_database_and_repositories_wipes_lfs)'"
        status: pass
    human_judgment: false

duration: 30min
completed: 2026-09-14
status: complete
plan_head_before: ec2dc28
commits: 1
---

# Phase 14 Plan 06: LFS GC + factory wipe Summary

**Periodic GC removes only unreferenced OIDs; factory reset with repositories also clears the LFS volume.**

## Task Commits

1. **Task 1+2: GC + factory wipe** - `622adff` (feat)

## Deviations from Plan

None.

## Self-Check: PASSED
