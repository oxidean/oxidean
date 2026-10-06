---
phase: 14-git-lfs
plan: "08"
subsystem: api
tags: [git-lfs, rpc, api-client, codegen]

requires:
  - phase: 14-git-lfs
    provides: Enable + quota RPCs (14-03/14-04)
provides:
  - repo.lfs.getStatus/getUsage/listObjects/download session RPCs
  - admin.lfs.getUsage instance breakdown
  - Generated @oxidean/api-client LFS methods
affects: [14-09, 14-10, 14-11]

actuals:
  tokens: 11852
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Browser Download via session RPC base64 soft-cap (16 MiB), never cookie on .git/info/lfs"
    - "Usage breakdown queries grouped by repo and owner"

key-files:
  created: []
  modified:
    - crates/oxidean-core/src/repo_types.rs
    - crates/oxidean-db/src/lfs.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/auth/admin.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts
    - crates/oxidean-api/tests/lfs_batch.rs

key-decisions:
  - "repo.lfs.download returns soft-capped base64; oversized → lfs.too_large_for_rpc"
  - "Kept getEnabled alongside richer getStatus for backward compat"

requirements-completed: [GIT-12, GIT-13]

coverage:
  - id: D1
    description: "Session LFS RPCs for status/usage/list/download + admin usage"
    requirement: GIT-12
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(lfs_session_rpc) | test(lfs_admin_get_usage)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Generated api-client exposes repo.lfs.* and admin.lfs.*"
    requirement: GIT-13
    verification:
      - kind: other
        ref: "make rpc-gen && make rpc-sync-check"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-09-14
status: complete
plan_head_before: 863162b1d43390426f856ca1793d81b05e9861e3
commits: 2
---

# Phase 14 Plan 08: LFS session RPC + api-client Summary

**Typed `repo.lfs.*` / `admin.lfs.*` session RPCs ship with usage breakdowns, browser download, and regenerated `@oxidean/api-client`.**

## Task Commits

1. **Task 1: Complete repo.lfs + admin.lfs RPC DTOs** - `5f269d5` (feat)
2. **Task 2: make rpc-gen + sync-check** - `832016c` (chore)

## Deviations from Plan

None - plan executed exactly as written.

## Self-Check: PASSED
