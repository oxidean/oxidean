---
phase: 15-releases-transfer
plan: "01"
subsystem: api
tags: [releases, rpc, migrations, draft-acl, git-tags]
requires:
  - phase: 15-00
    provides: "Wave 0 release_rpc + dialect_releases stubs"
provides:
  - "release.create/list/get/update/delete RPC"
  - "0014_releases_redirects schema (releases, release_assets, repository_redirects)"
  - "generated release.* api-client"
affects: [15-02, 15-03, 15-04, 15-06]
actuals:
  tokens: 19622
  tasks: 2
  commits: 1
plan_head_before: 3b8e28f43f9cfa4a9914e2f2a99245cead415fc1
tech-stack:
  added: []
  patterns:
    - "Tag must exist via GitBackend list_refs before release.create"
    - "Draft visibility gated by meets(Write)"
key-files:
  created:
    - crates/oxidean-core/src/release_types.rs
    - crates/oxidean-db/migrations/sqlite/0014_releases_redirects.sql
    - crates/oxidean-db/src/releases.rs
    - crates/oxidean-db/src/redirects.rs
    - crates/oxidean-api/src/release/mod.rs
  modified:
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/app.rs
    - packages/api-client/src/index.ts
    - crates/oxidean-api/tests/release_rpc.rs
    - crates/oxidean-db/tests/dialect_releases.rs
key-decisions:
  - "Default ENV stubs: OXIDEAN_RELEASE_ASSETS_DIR=var/release-assets, max 512MiB, redirect retention 90d"
  - "release.tag_missing + repo.not_found map to HTTP 400/404 via existing rpc_status"
patterns-established:
  - "Release DTOs in oxidean-core; persistence in oxidean-db; handlers in api/release"
requirements-completed: [GIT-14]
coverage:
  - id: D1
    description: "Write+ create release for existing tag with notes/draft/prerelease"
    requirement: GIT-14
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(release_create)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Missing tag returns release.tag_missing; drafts hidden from Read/anon"
    requirement: GIT-14
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(release_tag) | test(release_draft)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Author/Write+ update; Admin delete"
    requirement: GIT-14
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(release_update) | test(release_delete)'"
        status: pass
    human_judgment: false
duration: 10min
completed: 2026-09-14
status: complete
---

# Phase 15 Plan 01: Release RPC Tracer Summary

**Tag-bound release notes RPC (create/list/get/update/delete) with draft ACL, shared releases/redirects migration, and generated client (GIT-14).**

## Performance

- **Duration:** ~20 min
- **Tasks:** 2/2 (combined atomic commit)
- **Commits:** 1

## Accomplishments

- Tri-dialect `0014_releases_redirects` with `releases`, `release_assets`, `repository_redirects`
- `release.*` RPC: tag must exist; Write+ create/update; Admin delete; drafts Write+-only on list/get
- AppState ENV knobs for release-assets dir, max bytes, redirect retention (for later plans)
- Green `dialect_releases` + six release nextest cases; asset tests remain ignored for 15-02

## Task Commits

| Task | Commit | Notes |
|------|--------|-------|
| 1 Tracer create/list + schema | 4e1b5d1 | migration + create/list/tag_missing |
| 2 Draft/update/delete | 4e1b5d1 | same commit (single wave implementation) |

## Deviations from Plan

**1. [Rule 3 - Blocking] Combined both plan tasks into one commit**
- **Found during:** Task 2
- **Issue:** Parallel LFS agent kept switching the shared worktree branch; isolated to `agent-15-releases` worktree
- **Fix:** Implemented both tasks then single commit; will merge to `feat/execute-15-releases`

**2. [Rule 1 - Bug] rpc_json asserted HTTP 200 for all RPC errors**
- **Found during:** Task 2 verify
- **Issue:** `release.tag_missing` maps to HTTP 400; `repo.not_found` to 404
- **Fix:** Parse JSON body regardless of status in release_rpc helpers

## Auth Gates

None.

## Known Stubs

| Stub | File | Reason |
|------|------|--------|
| release_asset_* #[ignore] | release_rpc.rs | Green in 15-02 |
| Asset HTTP routes | — | 15-02 |
| Octane Releases UI | — | 15-06 |

## Self-Check: PASSED

- FOUND: crates/oxidean-api/src/release/mod.rs
- FOUND: crates/oxidean-db/migrations/sqlite/0014_releases_redirects.sql
- FOUND: commit 4e1b5d1
- TESTS: dialect_releases 2/2; release create/tag/draft/update/delete/write 6/6
