---
phase: 15-releases-transfer
plan: "00"
subsystem: testing
tags: [wave0, nextest, vitest, releases, rename, transfer]
requires:
  - phase: 11-issues
    provides: "ACL soft-not-found and integration test harness patterns"
provides:
  - "Discoverable ignored nextest stubs for release_*, rename/transfer/redirect"
  - "dialect_releases migration parity stub"
  - "Vitest stubs for Releases tab and settings danger zone"
affects: [15-01, 15-02, 15-03, 15-04, 15-05, 15-06]
actuals:
  tokens: 4068
  tasks: 2
  commits: 1
plan_head_before: 27404c39cc993eae813af77b0641c420925cbc42
tech-stack:
  added: []
  patterns: ["Wave 0 #[ignore] stubs discoverable via nextest list --run-ignored all"]
key-files:
  created:
    - crates/oxidean-api/tests/release_rpc.rs
    - crates/oxidean-api/tests/repo_rename_transfer.rs
    - crates/oxidean-db/tests/dialect_releases.rs
    - apps/web/src/routes/$owner.$repo.releases.integration.test.ts
    - apps/web/src/routes/$owner.$repo.settings.rename-transfer.integration.test.ts
  modified: []
key-decisions:
  - "Wave 0 stubs use #[ignore] so CI stays green until 15-01..15-05 turn them green"
patterns-established:
  - "Phase 15 test filters: release, rename, transfer, redirect, dialect_releases"
requirements-completed: [GIT-14, GIT-15, GIT-16, GIT-17]
coverage:
  - id: D1
    description: "Wave 0 discoverable stubs for release/rename/transfer/redirect/dialect_releases and web surfaces"
    requirement: GIT-14
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(release)' --run-ignored all"
        status: pass
    human_judgment: false
duration: 2min
completed: 2026-09-14
status: complete
---

# Phase 15 Plan 00: Wave 0 Stubs Summary

**Ignored nextest + Vitest stubs for releases, rename/transfer redirects, dialect migrations, and web Releases/settings surfaces (GIT-14..17 discoverability).**

## Performance

- **Duration:** 2 min
- **Tasks:** 2/2
- **Commits:** 1

## Accomplishments

- Scaffolded `release_rpc.rs` covering create/tag_missing/draft/update/delete/asset ACL stubs
- Scaffolded `repo_rename_transfer.rs` covering rename, redirect, supersede, purge, transfer, cascade
- Scaffolded `dialect_releases.rs` for next-free `00NN_releases_redirects` parity
- Scaffolded Vitest stubs for Releases tab routes and settings Danger zone rename/transfer

## Task Commits

| Task | Commit | Files |
|------|--------|-------|
| 1 Rust Wave 0 stubs | 4b21443 | release_rpc.rs, repo_rename_transfer.rs, dialect_releases.rs |
| 2 Web Wave 0 stubs | 4b21443 | releases + rename-transfer integration tests |

## Deviations from Plan

### Auto-fixed Issues

None - plan executed as written (single commit for both stub tasks).

**Note:** Concurrent Phase 14/20 agents leave unrelated untracked files in the shared worktree; only Phase 15 Wave 0 stubs were staged.

## Auth Gates

None.

## Known Stubs

| Stub | File | Reason |
|------|------|--------|
| All release_* tests #[ignore] | release_rpc.rs | Green in 15-01/15-02 |
| All rename/transfer/redirect tests #[ignore] | repo_rename_transfer.rs | Green in 15-03/15-04 |
| dialect_releases #[ignore] | dialect_releases.rs | Green in 15-01 |
| Releases Vitest asserts missing UI | releases.integration.test.ts | Green in 15-06 |
| Settings rename/transfer Vitest | rename-transfer.integration.test.ts | Green in 15-05 |

## Self-Check: PASSED

- FOUND: crates/oxidean-api/tests/release_rpc.rs
- FOUND: crates/oxidean-api/tests/repo_rename_transfer.rs
- FOUND: crates/oxidean-db/tests/dialect_releases.rs
- FOUND: apps/web/src/routes/$owner.$repo.releases.integration.test.ts
- FOUND: apps/web/src/routes/$owner.$repo.settings.rename-transfer.integration.test.ts
- FOUND: commit 4b21443
