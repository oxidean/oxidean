---
phase: 07-git-repos-browse
plan: "05"
subsystem: api
tags: [git, ACL, ls_tree, cat_blob, repo.raw, anti-enumeration, D-25]
requires:
  - phase: 07-git-repos-browse
    provides: "CliGitBackend init_bare/seed_commit + repo.create (07-12); Wave 0 private_404 stub (07-00)"
provides:
  - "GitBackend ls_tree / cat_blob / list_refs"
  - "Owner-only private ACL with identical repo.not_found"
  - "RPC repo.get/tree/blob/refs + GET raw blob under /api"
affects:
  - 07-06-history-blame
  - 07-15-code-ui
plan_head_before: d9b47bee31ec7bf18949313ba0912f01e98a53ac
actuals:
  tokens: 13238
  tasks: 1
  commits: 2
commits: 2
tech-stack:
  added: []
  patterns:
    - "resolve_repo_for_read: missing OR private≠owner → identical not_found"
    - "Raw blobs via HTTP GET under /api (not RPC JSON); soft 1 MiB cap headers"
tech-stack-added: []
key-files:
  created:
    - crates/oxidean-api/src/repo/acl.rs
    - crates/oxidean-api/src/routes/repo_raw.rs
  modified:
    - crates/oxidean-git/src/backend.rs
    - crates/oxidean-git/src/cli.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-core/src/repo_types.rs
    - packages/api-client/src/index.ts
    - crates/oxidean-api/tests/repo_private_404.rs
key-decisions:
  - "Private ACL stub is owner-only until Phase 10 (D-23); no repo.forbidden code"
  - "Blob soft limit 1 MiB for RPC preview and raw X-Oxidean-Blob-Truncated header (D-20)"
  - "Empty bare repo tree returns { empty: true, entries: [] } without 500"
patterns-established:
  - "Browse read path: ACL first, then bare_repo_path, then GitBackend"
  - "repo.not_found maps to HTTP 404 on RPC and raw routes"
requirements-completed: [GIT-05, GIT-08]
coverage:
  - id: D1
    description: "Private non-owner and missing repo share identical repo.not_found"
    requirement: GIT-05
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_private_404.rs#repo_private_404_identical_not_found_for_missing_and_private"
        status: pass
    human_judgment: false
  - id: D2
    description: "Anonymous can read public repo metadata via repo.get"
    requirement: GIT-05
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_private_404.rs#repo_private_404_public_anonymous_get_succeeds"
        status: pass
    human_judgment: false
  - id: D3
    description: "Empty repo tree returns structured empty without 500"
    requirement: GIT-05
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_private_404.rs#repo_private_404_empty_tree_structured"
        status: pass
    human_judgment: false
  - id: D4
    description: "ls_tree returns blob/tree/gitlink modes; cat_blob returns bytes"
    requirement: GIT-05
    verification:
      - kind: unit
        ref: "crates/oxidean-git/src/cli.rs#ls_tree_returns_dirs_files_and_gitlink_modes"
        status: pass
    human_judgment: false
  - id: D5
    description: "GET raw blob route mounted under /api with ACL"
    requirement: GIT-08
    verification:
      - kind: other
        ref: "rg repo_raw crates/oxidean-api/src/app.rs"
        status: pass
    human_judgment: false
duration: 7min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 05: ACL-safe tree/blob/raw APIs Summary

**Owner-only private ACL with identical `repo.not_found`, plus `ls_tree`/`cat_blob` RPC and raw HTTP under `/api`**

## Performance

- **Duration:** 7 min
- **Started:** 2026-09-12T17:51:39Z
- **Completed:** 2026-09-12T17:59:04Z
- **Tasks:** 1
- **Files modified:** 15

## Accomplishments

- Unified anti-enumeration ACL (`resolve_repo_for_read`) for all browse reads
- `GitBackend` read APIs: `ls_tree`, `cat_blob`, `list_refs` via CLI argv
- RPC `repo.get` / `repo.tree` / `repo.blob` / `repo.refs` + `GET .../raw/{ref}/{*path}`
- Soft 1 MiB blob cap for UI (truncated flag / raw headers)

## Task Commits

Each task was committed atomically:

1. **Task 1 RED: failing ACL browse tests** — `dc0c2ad` (test)
2. **Task 1 GREEN: ACL-safe tree/blob/raw APIs** — `038ac5f` (feat)

**TDD:**
- RED: `dc0c2ad` — `test(07-05): add failing ACL browse tests for repo.get/tree (RED)`
- GREEN: `038ac5f` — `feat(07-05): ACL-safe tree/blob/raw APIs for git browse`
- REFACTOR: skipped (no cleanup needed)

## Files Created/Modified

- `crates/oxidean-api/src/repo/acl.rs` — owner-only private + identical not_found
- `crates/oxidean-api/src/routes/repo_raw.rs` — raw blob HTTP with path/ref validation
- `crates/oxidean-git/src/backend.rs` / `cli.rs` — ls_tree, cat_blob, list_refs
- `crates/oxidean-api/src/repo/mod.rs` — get/tree/blob/refs handlers
- `crates/oxidean-core/src/repo_types.rs` — browse DTOs
- `packages/api-client/src/index.ts` — rpc-gen client methods

## Decisions Made

- Private = owner-only stub until Phase 10; never emit distinct forbidden messaging
- Soft blob limit fixed at 1 MiB for both RPC preview and raw responses
- Empty repos return structured empty tree (no 500) so Code UI can show first-push guide

## Deviations from Plan

### Auto-fixed Issues

None that changed production scope.

### Scoped exclusions

**1. [Out of scope] Full `oxidean-git --lib` includes Wave 0 archive stub**
- **Found during:** Task 1 verify
- **Issue:** `git_archive_formats_zip_and_tar_gz` still `assert!(false)` (GIT-07 / later plan)
- **Fix:** Verified with `-E 'not test(git_archive)'`; logged deferred-items + WINDOWS.md
- **Files modified:** `.planning/phases/07-git-repos-browse/deferred-items.md`
- **Committed in:** `038ac5f`

---

**Total deviations:** 1 scoped exclusion (pre-existing Wave 0 stub)
**Impact on plan:** GIT-05 browse APIs complete; archive remains deferred

## Issues Encountered

None beyond the known Wave 0 archive stub.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- History/blame (07-06) and Code UI (07-15) can consume ACL + tree/blob/raw
- Do not implement Octane browse routes here (still 07-15)

## TDD Gate Compliance

| Gate | Commit | Evidence |
|------|--------|----------|
| RED | `dc0c2ad` | `07-05-tdd-red-evidence.json` — RED_EVIDENCE_OK (`repo.not_found` vs `rpc.unknown_procedure`) |
| GREEN | `038ac5f` | `repo_private_404` + `ls_tree` unit tests pass |
| REFACTOR | n/a | skipped |

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/src/repo/acl.rs`
- FOUND: `crates/oxidean-api/src/routes/repo_raw.rs`
- FOUND: commits `dc0c2ad`, `038ac5f`

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*
