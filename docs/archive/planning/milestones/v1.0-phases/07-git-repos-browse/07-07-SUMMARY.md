---
phase: 07-git-repos-browse
plan: "07"
subsystem: api
tags: [git, branches, soft-protect, rpc, acl]

requires:
  - phase: 07-git-repos-browse
    provides: "CliGitBackend + list_refs / ACL resolve_repo_for_read (07-05/06)"
  - phase: 07-git-repos-browse
    provides: "Seeded bare repos via repo.create templates"
provides:
  - "GitBackend branch_create / branch_rename / branch_delete via CLI argv"
  - "Owner-only repo.branchCreate/Rename/Delete with default soft-protect"
  - "Green repo_branch_soft_protect integration tests (GIT-06)"
affects:
  - 07-18-branches-tags-ui

actuals:
  tokens: 5928
  tasks: 1
  commits: 2

plan_head_before: 3d9700f0098685c64db049c5a1cffc1c53d3c4b3

tech-stack:
  added: []
  patterns:
    - "Owner mutate via resolve_repo_for_read + owner_id check; non-owner → repo.not_found"
    - "Soft-protect: rename/delete of repositories.default_branch → repo.default_branch_protected"
    - "Branch names validated with same treeish argv guards (T-07-17)"

key-files:
  created:
    - .planning/phases/07-git-repos-browse/.tdd/07-07-red-evidence.json
  modified:
    - crates/oxidean-git/src/backend.rs
    - crates/oxidean-git/src/cli.rs
    - crates/oxidean-core/src/repo_types.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - crates/oxidean-api/tests/repo_branch_soft_protect.rs
    - packages/api-client/src/index.ts

key-decisions:
  - "RPC names repo.branchCreate/Rename/Delete (camelCase multi-word); error repo.default_branch_protected"
  - "Non-owner mutate returns repo.not_found (anti-enumeration) even on public repos"
  - "git branch -D for forge delete; UI deferred to 07-18"

patterns-established:
  - "Branch mutate: require_verified + owner_id match before CliGitBackend"
  - "list_refs already exposed as repo.refs — no new list RPC"

requirements-completed: [GIT-06, GIT-05]

coverage:
  - id: D1
    description: "Owner create/rename/delete non-default branches via RPC"
    requirement: GIT-06
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_branch_soft_protect.rs#repo_branch_soft_protect_allows_non_default_crud"
        status: pass
    human_judgment: false
  - id: D2
    description: "Default branch rename/delete rejected with soft-protect error"
    requirement: GIT-06
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_branch_soft_protect.rs#repo_branch_soft_protect_blocks_default_rename_and_delete"
        status: pass
    human_judgment: false
  - id: D3
    description: "Non-owner branch mutate blocked without leaking (repo.not_found)"
    requirement: GIT-06
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_branch_soft_protect.rs#repo_branch_soft_protect_non_owner_mutate_not_found"
        status: pass
    human_judgment: false

duration: 4min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 07: Branch CRUD + soft-protect Summary

**Owner-only branch create/rename/delete RPC with default-branch soft-protect (`repo.default_branch_protected`); Branches/Tags UI deferred to 07-18**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-12T18:24:29Z
- **Completed:** 2026-09-12T18:28:36Z
- **Tasks:** 1
- **Files modified:** 9

## Accomplishments

- `GitBackend` + `CliGitBackend` `branch_create` / `branch_rename` / `branch_delete` via argv-only `git branch`
- `repo.branchCreate` / `repo.branchRename` / `repo.branchDelete` enforce verified owner (D-27) and soft-protect default (D-28)
- Integration suite `repo_branch_soft_protect_*` green; api-client regenerated via rpc-gen

## Task Commits

Each task was committed atomically:

1. **Task 1 RED: failing soft-protect + CRUD RPC tests** - `e1e29ad` (test)
2. **Task 1 GREEN: branch mutate API + CliGitBackend** - `d40adac` (feat)

_TDD: RED evidence at `.planning/phases/07-git-repos-browse/.tdd/07-07-red-evidence.json` (`RED_EVIDENCE_OK`)_

## Files Created/Modified

- `crates/oxidean-git/src/backend.rs` — trait methods for branch CRUD
- `crates/oxidean-git/src/cli.rs` — `git branch` / `-m` / `-D` implementations
- `crates/oxidean-core/src/repo_types.rs` — branch request/response DTOs
- `crates/oxidean-api/src/repo/mod.rs` — owner gate + soft-protect handlers
- `crates/oxidean-api/src/rpc.rs` — procedure dispatch
- `crates/oxidean-api/src/bin/rpc_gen.rs` + `packages/api-client/src/index.ts` — client types
- `crates/oxidean-api/tests/repo_branch_soft_protect.rs` — GIT-06 / D-27 / D-28 coverage

## Decisions Made

- Soft-protect error code `repo.default_branch_protected` with stable copy for UI (07-18)
- Non-owner mutate uses `repo.not_found` (same anti-enumeration shape as private reads)
- No Branches/Tags Octane routes here (07-18 owns UI)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Branch mutate API ready for 07-18 Branches/Tags UI
- `repo.refs` already lists branches/tags for UI consumers

## Self-Check: PASSED

- FOUND: crates/oxidean-api/tests/repo_branch_soft_protect.rs
- FOUND: crates/oxidean-git/src/cli.rs branch_* methods
- FOUND: e1e29ad (RED), d40adac (GREEN)
- FOUND: nextest `test(repo_branch)` 3 passed

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*
