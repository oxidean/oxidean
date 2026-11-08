---
phase: 14-git-lfs
plan: "04"
subsystem: api
tags: [git-lfs, quota, admin]

requires:
  - phase: 14-git-lfs
    provides: Auth/ACL + enable (14-03)
provides:
  - Max object + per-repo/user quota enforcement
  - admin.lfs.getSettings / updateSettings
affects: [14-05, 14-08, 14-10]

actuals:
  tokens: 10304
  tasks: 2
  commits: 1

tech-stack:
  added: []
  patterns:
    - "instance_lfs_settings overrides env defaults"
    - "Logical bytes via SUM(links); physical via SUM(lfs_objects)"

key-files:
  created:
    - crates/oxidean-api/src/lfs/quota.rs
    - crates/oxidean-db/migrations/sqlite/0013_lfs_quotas.sql
  modified:
    - crates/oxidean-api/src/routes/git_lfs.rs
    - crates/oxidean-api/src/auth/admin.rs
    - crates/oxidean-api/tests/lfs_batch.rs

key-decisions:
  - "Defaults 2GiB/10GiB/50GiB; 0/-1 unlimited"
  - "Batch returns per-object 422/507 errors; PUT returns HTTP status"

requirements-completed: [GIT-12, GIT-13]

coverage:
  - id: D1
    description: "Over-size and over-quota upload rejected"
    requirement: GIT-12
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(lfs_quota)'"
        status: pass
    human_judgment: false

duration: 40min
completed: 2026-09-14
status: complete
plan_head_before: 8b898b1e265d4396282d453c33ade43645696f82
commits: 1
---

# Phase 14 Plan 04: LFS quotas Summary

**Configurable max object and logical quotas reject oversized/over-quota LFS uploads; Admin can override env defaults.**

## Accomplishments
- Migration `0013_lfs_quotas` + usage queries
- Batch/PUT enforcement via `lfs/quota.rs`
- `admin.lfs.getSettings` / `updateSettings`

## Task Commits

1. **Task 1+2: Quotas + Admin overrides** - `6c32a13` (feat)

## Deviations from Plan

None.

## Self-Check: PASSED
- FOUND: quota.rs, 0013 migrations, 6c32a13
