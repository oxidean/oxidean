---
phase: 16-in-repo-search
plan: "02"
subsystem: api
tags: [repo.search, commits, issues, pulls, qualifiers, GIT-18]
requires:
  - phase: 16-01
    provides: repo.search code path + GrepHit
  - phase: 12-pull-requests
    provides: pull_requests table + pull.create
provides:
  - Qualifier parser (is:/author:/path:)
  - GitBackend::log_search
  - type=commits/issues/pulls dispatch
affects: [16-03]
actuals:
  tokens: 18000
  tasks: 3
  commits: 2
tech-stack:
  added: []
  patterns: [search_query strip-unknown, pulls::search_by_repo]
key-files:
  created:
    - crates/oxidean-api/src/repo/search_query.rs
  modified:
    - crates/oxidean-api/src/repo/search.rs
    - crates/oxidean-git/src/cli.rs
    - crates/oxidean-db/src/pulls.rs
    - crates/oxidean-api/tests/repo_search.rs
key-decisions:
  - "Unknown qualifiers stripped (RESEARCH)"
  - "Issues use IssueListFilters; pulls use dedicated pull_requests search"
requirements-completed: [GIT-18]
coverage:
  - id: D1
    description: "Commit search by message and author:"
    requirement: GIT-18
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(repo_search_commits)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Issues search with is:open and no PR mixing"
    requirement: GIT-18
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(repo_search_issues)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Pulls search isolated from issues"
    requirement: GIT-18
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(repo_search_pulls)'"
        status: pass
    human_judgment: false
duration: 35min
completed: 2026-09-16
status: complete
plan_head_before: ce4a8b6060935269efa229d7c2ca45cd7099f3b1
commits: 2
---

# Phase 16 Plan 02: Commits/Issues/Pulls Search Summary

**Expanded `repo.search` with qualifier parsing, `log_search`, and separate issues/pulls DB backends — all four types green.**

## Performance

- **Duration:** 35min
- **Tasks:** 3
- **Files modified:** 8

## Accomplishments

- Modest qualifier parser (`is:`, `author:`, `path:`; unknown stripped)
- Commit search via `git log --grep` / `--author`
- Issues + pulls search with D-SRCH-11 isolation

## Task Commits

1. **Task 1: Qualifier parser + commit log_search** - `d340ddc` (feat)
2. **Task 2+3: Issues and pulls search types** - `408ac8b` (feat)

## Decisions Made

- Reuse `IssueListFilters` for issues; dedicated `pulls::search_by_repo` for PRs

## Deviations from Plan

None material — Tasks 2 and 3 shared one commit because `search.rs` dispatch landed with Task 1 and DB helper completed pulls.

## Self-Check: PASSED

- FOUND: search_query.rs, log_search, search_by_repo
- FOUND: d340ddc, 408ac8b
