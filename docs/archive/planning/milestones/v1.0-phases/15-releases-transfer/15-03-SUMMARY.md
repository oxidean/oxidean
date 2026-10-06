---
phase: 15-releases-transfer
plan: "03"
subsystem: api
tags: [rename, redirects, smart-http, ssh, orphan-reconcile]
requires:
  - phase: 15-releases-transfer
    provides: "0014_releases_redirects migration + redirects DB helpers from 15-00/15-01"
provides:
  - "repo.rename with bare-disk move and repository_redirects"
  - "lookup_repo_row_or_redirect for web/Smart HTTP/SSH"
  - "Expired redirect purge via orphan reconcile"
  - "OXIDEAN_REPO_REDIRECT_RETENTION_DAYS (default 90)"
affects: [15-04, 15-05]
actuals:
  tokens: 10818
  tasks: 3
  commits: 1
plan_head_before: ea56ad760f708ce194229a3c7d9b4d48c73ede72
tech-stack:
  added: []
  patterns:
    - "FS-then-DB bare rename with compensate (mirror username migrate)"
    - "Live owner/name supersedes redirect; resolve via repo_id only"
key-files:
  created:
    - crates/oxidean-api/src/repo/rename_transfer.rs
  modified:
    - crates/oxidean-api/src/repo/acl.rs
    - crates/oxidean-api/src/routes/git_smart_http.rs
    - crates/oxidean-api/src/ssh/pack.rs
    - crates/oxidean-api/src/jobs/reconcile.rs
    - crates/oxidean-db/src/repositories.rs
    - packages/api-client/src/index.ts
    - docs/CONFIGURATION.md
key-decisions:
  - "Single commit for T1–T3 — rename, resolve, and purge share lookup + disk paths"
  - "Default redirect retention 90 days via OXIDEAN_REPO_REDIRECT_RETENTION_DAYS"
  - "Smart HTTP rewrites CGI path_info to current owner/name after redirect"
requirements-completed: [GIT-16]
coverage:
  - id: D1
    description: "Admin rename moves bare dir + DB name and inserts retention redirect"
    requirement: GIT-16
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(repo_rename)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Old path resolves via redirect; create supersedes; expired purge"
    requirement: GIT-16
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(redirect)'"
        status: pass
    human_judgment: false
duration: 11min
completed: 2026-09-14
status: complete
---

# Phase 15 Plan 03: Rename & Redirects Summary

**Admin `repo.rename` moves the bare git dir and DB name, inserts finite-retention redirects honored by web/Smart HTTP/SSH, and orphan reconcile purges expired redirect rows (GIT-16).**

## Performance

- **Duration:** 11 min
- **Tasks:** 3/3
- **Commits:** 1
- **Files modified:** 16

## Accomplishments

- Implemented `repo.rename` behind `resolve_repo_for_admin` with FS-then-DB move and compensate
- Shared `lookup_repo_row_or_redirect` for `repo.get`, browse ACL, Smart HTTP CGI rewrite, and SSH pack
- Create at old slug/name deletes matching redirect; reconcile purges `expires_at < now`
- Documented `OXIDEAN_REPO_REDIRECT_RETENTION_DAYS` (default 90)

## Task Commits

| Task | Commit | Notes |
|------|--------|-------|
| 1 rename + disk + redirect insert | `cb520e0` | Combined with T2/T3 (shared wiring) |
| 2 resolve web/Smart HTTP/SSH + supersede | `cb520e0` | Same commit |
| 3 purge job + ENV docs | `cb520e0` | Same commit |

## Deviations from Plan

### Auto-fixed Issues

None.

### Process

**1. [Rule 3 - Blocking] Combined T1–T3 into one commit**
- **Found during:** Task commits
- **Issue:** Redirect resolve and purge are inseparable from rename insert paths in tests/wiring
- **Fix:** Single `feat(15-03)` commit covering all three task outcomes; SUMMARY maps all tasks to `cb520e0`
- **Files modified:** (see key-files)
- **Commit:** `cb520e0`

## Auth Gates

None.

## Known Stubs

| Stub | File | Reason |
|------|------|--------|
| Transfer tests still `#[ignore]` | repo_rename_transfer.rs | Green in 15-04 |

## Self-Check: PASSED

- FOUND: crates/oxidean-api/src/repo/rename_transfer.rs
- FOUND: cb520e0
- FOUND: OXIDEAN_REPO_REDIRECT_RETENTION in docs/CONFIGURATION.md and .env.example
