---
phase: 16-in-repo-search
plan: "01"
subsystem: api
tags: [repo.search, git-grep, acl, octane, GIT-18]
requires:
  - phase: 16-00
    provides: Wave 0 repo_search_* stubs
provides:
  - GitBackend::grep with binary skip and match caps
  - repo.search RPC type=code behind Read ACL
  - Thin Octane /search tracer page
affects: [16-02, 16-03]
actuals:
  tokens: 12000
  tasks: 3
  commits: 2
tech-stack:
  added: []
  patterns: [git grep via GitBackend, tagged RepoSearchHit, soft truncated flag]
key-files:
  created:
    - crates/oxidean-api/src/repo/search.rs
    - apps/web/src/routes/$owner.$repo.search.tsrx
  modified:
    - crates/oxidean-git/src/backend.rs
    - crates/oxidean-git/src/cli.rs
    - crates/oxidean-core/src/repo_types.rs
    - crates/oxidean-api/src/rpc.rs
    - packages/api-client/src/index.ts
    - apps/web/src/lib/repo-chrome-active.ts
key-decisions:
  - "Tree-ish must precede pathspec (--); bare-repo git grep works with -C"
  - "Non-code types return empty hits until 16-02 for tracer stability"
requirements-completed: [GIT-18]
coverage:
  - id: D1
    description: "Read user finds seeded code via repo.search type=code"
    requirement: GIT-18
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(repo_search_code)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Private unauthorized search soft-404s as repo.not_found"
    requirement: GIT-18
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(repo_search_acl)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Thin /search page shows code hits"
    requirement: GIT-18
    verification:
      - kind: automated_ui
        ref: "bun run test src/routes/$owner.$repo.search.integration.test.ts"
        status: pass
    human_judgment: false
duration: 75min
completed: 2026-09-16
status: complete
plan_head_before: 9ede4ba6b76c9b834d9d5285b97667c8529124d8
commits: 2
---

# Phase 16 Plan 01: Code Search Tracer Summary

**End-to-end `GitBackend::grep` → `repo.search` type=code (Read ACL) → thin Octane `/search` page with truncated/binary safeguards.**

## Performance

- **Duration:** 75min
- **Started:** 2026-09-16T13:47:39Z
- **Completed:** 2026-09-16T15:00:00Z
- **Tasks:** 3
- **Files modified:** 14

## Accomplishments

- `GitBackend::grep` using `git grep -n -I` with soft max-matches + truncated
- `repo.search` registered + rpc-gen; green code/ACL/limits tests
- Tracer search UI + `RepoChromeActive` `"search"` mapping

## Task Commits

1. **Task 1+3: End-to-end code search + binary/cap** - `7ae1538` (feat)
2. **Task 2: Thin Octane /search tracer** - `b1ed3bb` (feat)

## Files Created/Modified

- `crates/oxidean-api/src/repo/search.rs` — RPC handler
- `crates/oxidean-git/src/cli.rs` — grep impl + unit tests
- `apps/web/src/routes/$owner.$repo.search.tsrx` — tracer page
- `packages/api-client/src/index.ts` — generated client

## Decisions Made

- Non-code types accepted with empty hits until 16-02
- Task 3 (binary skip + cap) landed in the same commit as Task 1

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] git grep `--` before treeish**
- **Found during:** Task 1
- **Issue:** Bare-repo grep failed with “must be run in a work tree”
- **Fix:** Pass treeish before optional `-- pathspec`
- **Files modified:** `crates/oxidean-git/src/cli.rs`
- **Commit:** `7ae1538`

**2. [Rule 3 - Blocking] Soft not_found is HTTP 404**
- **Found during:** Task 1 ACL test
- **Issue:** Test helper asserted HTTP 200 on RPC errors
- **Fix:** Parse body regardless of status
- **Commit:** `7ae1538`

## Self-Check: PASSED

- FOUND: search.rs, search.tsrx, GrepHit, repo.search in rpc
- FOUND: 7ae1538, b1ed3bb
