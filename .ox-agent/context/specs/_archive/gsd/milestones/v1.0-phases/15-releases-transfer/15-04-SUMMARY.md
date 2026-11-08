---
phase: 15-releases-transfer
plan: "04"
subsystem: api
tags: [transfer, rename, redirects, git]
requires:
  - phase: 15-releases-transfer
    provides: "repo.rename + redirects from 15-03"
provides:
  - "repo.transfer with type-confirm, disk move, owner rewrite"
  - "user→user former owner admin collaborator"
  - "rpc-gen client repo.transfer"
affects: [15-05]
actuals:
  tokens: 5005
  tasks: 2
  commits: 1
plan_head_before: ba9deb9438ab4fed34bc5c6ff9e341a378c264c6
tech-stack:
  added: []
  patterns: ["Admin transfer reuses rename disk+redirect path; confirm_name exact match"]
key-files:
  created: []
  modified:
    - crates/oxidean-api/src/repo/rename_transfer.rs
    - crates/oxidean-db/src/repositories.rs
    - crates/oxidean-api/tests/repo_rename_transfer.rs
    - packages/api-client/src/index.ts
key-decisions:
  - "Immediate transfer after type-confirm (no accept-email)"
  - "user→user: former personal owner added as admin collaborator"
requirements-completed: [GIT-17]
coverage:
  - id: D1
    description: "Admin transfer to user/org with confirm_name, disk move, redirect"
    requirement: GIT-17
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(repo_transfer)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Cascade keeps issues/LFS on repo_id; no OID copy"
    requirement: GIT-17
    verification:
      - kind: integration
        ref: "repo_transfer_cascade_issues_lfs_by_repo_id"
        status: pass
    human_judgment: false
duration: 25min
completed: 2026-09-14
status: complete
---

# Phase 15 Plan 04: Repository Transfer Summary

**Admin `repo.transfer` moves bare git storage and rewrites polymorphic owner after exact type-confirm; issues/LFS stay on `repo_id` (GIT-17).**

## Task Commits

| Task | Commit |
|------|--------|
| 1–2 transfer + cascade | `e6d21c4` |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Restored `lookup_repo_row_or_redirect` import**
- **Found during:** Task 1 compile
- **Issue:** Import dropped while `resolve_repo_or_redirect` still called it
- **Fix:** Re-import from `super::acl`
- **Commit:** `e6d21c4`

**2. [Rule 3 - Blocking] Combined T1–T2 into one commit**
- Shared `rename_transfer.rs` + tests; cascade lives in same transfer path

## Self-Check: PASSED

- FOUND: `repo.transfer` RPC, `update_repository_owner`, nextest green (3 tests), rpc-sync-check ok
