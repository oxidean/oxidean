---
phase: 14-git-lfs
plan: "03"
subsystem: api
tags: [git-lfs, auth, acl, pat, enable]

requires:
  - phase: 14-git-lfs
    provides: Batch + basic transfer tracer (14-02)
provides:
  - LFS PAT Basic auth/ACL mirroring Smart HTTP (D-LFS-09/11)
  - Admin-only repo.lfs.setEnabled session RPC (D-LFS-10)
affects: [14-04, 14-08, 14-09]

actuals:
  tokens: 11178
  tasks: 2
  commits: 1

tech-stack:
  added: []
  patterns:
    - "lfs/auth.rs authorize_lfs for upload/download gates"
    - "Soft-deny Admin toggle via resolve_repo_for_admin → HTTP 404"

key-files:
  created:
    - crates/oxidean-api/src/lfs/auth.rs
  modified:
    - crates/oxidean-api/src/routes/git_lfs.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/tests/lfs_batch.rs
    - crates/oxidean-core/src/repo_types.rs

key-decisions:
  - "Combined Task 1+2 in one commit (auth + enable tightly coupled in routes/tests)"
  - "Non-Admin setEnabled soft-denies as repo.not_found (parity with visibility)"

patterns-established:
  - "Cookie ignored for LFS; LFS-Authenticate on 401; scope fail → 403"

requirements-completed: [GIT-12]

coverage:
  - id: D1
    description: "Cookie ignored; private unauth 401; FG read 403 on upload; unverified upload denied"
    requirement: GIT-12
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(lfs_batch)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Admin repo.lfs.setEnabled; disabled repo rejects batch"
    requirement: GIT-12
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(lfs_enable)'"
        status: pass
    human_judgment: false

duration: 35min
completed: 2026-09-14
status: complete
plan_head_before: d22f6851978f07f786b15834fef13f1aae6ba774
commits: 1
---

# Phase 14 Plan 03: LFS auth/ACL + enable Summary

**PAT Basic LFS auth mirrors Smart HTTP; Admin-only per-repo enable rejects disabled transfers.**

## Performance

- **Duration:** ~35 min
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- `lfs/auth.rs` with Cookie ignore, classic/FG scopes, Capability Read/Write, verified upload
- `repo.lfs.setEnabled` / `getEnabled` session RPCs
- Green auth-matrix + enable nextest cases

## Task Commits

1. **Task 1+2: PAT/ACL matrix + Admin enable RPC** - `ec72b71` (feat)

## Decisions Made
Combined tasks into one commit; soft-deny non-Admin toggle via existing admin resolver.

## Deviations from Plan

None - plan executed as written (combined commit for intertwined files).

## Self-Check: PASSED
- FOUND: crates/oxidean-api/src/lfs/auth.rs
- FOUND: ec72b71
