---
phase: 07-git-repos-browse
plan: "10"
subsystem: ops
tags: [factory-reset, orphan-reconcile, git-gc, repos-dir, admin]

requires:
  - phase: 07-git-repos-browse
    provides: soft-delete repos, GitBackend CLI, admin factory reset, repos_dir
provides:
  - Factory reset scope radios (database_only vs database_and_repositories)
  - In-process orphan reconcile + soft-delete retention purge
  - Scheduled and manual git gc (admin.repos.gc)
  - CONFIGURATION docs for OXIDEAN_REPOS_DIR, git ≥2.5, UID/GID, cleanup knobs
affects: [ship, ops, admin-ui]

actuals:
  tokens: 17054
  tasks: 3
  commits: 3

plan_head_before: 061ad05d7c79d8611d58d32702694e6031bd0841

tech-stack:
  added: []
  patterns:
    - "In-process tokio interval jobs for orphan reconcile + gc (A3)"
    - "Path-safe deletes via canonicalize + strip_prefix under repos_dir (T-07-26)"
    - "FactoryResetScope serde snake_case with database_only default (D-34)"

key-files:
  created:
    - apps/web/src/components/ui/radio-group.tsrx
    - crates/oxidean-api/src/jobs/mod.rs
    - crates/oxidean-api/src/jobs/reconcile.rs
    - crates/oxidean-api/src/jobs/schedule.rs
    - crates/oxidean-api/tests/factory_reset_scope.rs
    - crates/oxidean-api/tests/orphan_reconcile_gc.rs
  modified:
    - crates/oxidean-core/src/auth_types.rs
    - crates/oxidean-core/src/repo_types.rs
    - crates/oxidean-api/src/auth/admin.rs
    - crates/oxidean-api/src/main.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-git/src/backend.rs
    - crates/oxidean-git/src/cli.rs
    - crates/oxidean-db/src/repositories.rs
    - apps/web/src/routes/admin/auth.tsrx
    - docs/CONFIGURATION.md
    - packages/api-client/src/index.ts

key-decisions:
  - "Cleanup frequency/retention via ENV (not new DB columns) to avoid schema migration in Phase 7"
  - "orphan_reconcile_with_retention for tests; keep soft-deleted dirs until retention elapses"
  - "git gc --auto for scheduled/manual GC; modest default 7d interval"

patterns-established:
  - "Background jobs spawn from main after AppState build"
  - "Admin Dialog + RadioGroup for destructive scoped confirms"

requirements-completed: [GIT-08, GIT-09]

coverage:
  - id: D1
    description: Factory reset modal offers Database only vs Database and repositories; default Database only; RESET phrase required
    requirement: GIT-08
    verification:
      - kind: integration
        ref: crates/oxidean-api/tests/factory_reset_scope.rs#factory_reset_database_only_keeps_repo_files
        status: pass
      - kind: integration
        ref: crates/oxidean-api/tests/factory_reset_scope.rs#factory_reset_database_and_repositories_wipes_disk
        status: pass
    human_judgment: false
  - id: D2
    description: Periodic orphan reconcile removes orphan bare dirs and purges soft-deleted past retention
    requirement: GIT-08
    verification:
      - kind: integration
        ref: crates/oxidean-api/tests/orphan_reconcile_gc.rs#orphan_reconcile_purges_soft_deleted_past_retention
        status: pass
      - kind: unit
        ref: crates/oxidean-api/src/jobs/reconcile.rs#orphan_reconcile_removes_disk_without_db_row
        status: pass
    human_judgment: false
  - id: D3
    description: Scheduled git gc plus sys-admin manual GC trigger
    requirement: GIT-09
    verification:
      - kind: integration
        ref: crates/oxidean-api/tests/orphan_reconcile_gc.rs#admin_repos_gc_runs_for_one_and_all
        status: pass
      - kind: unit
        ref: crates/oxidean-git/src/cli.rs#gc_runs_on_bare_repo
        status: pass
    human_judgment: false
  - id: D4
    description: CONFIGURATION documents OXIDEAN_REPOS_DIR, git ≥2.5, Compose ownership, cleanup knobs
    requirement: GIT-08
    verification:
      - kind: other
        ref: rg -n 'OXIDEAN_REPOS_DIR|git.*2\.5|var/repos|UID|GID|orphan|gc' docs/CONFIGURATION.md
        status: pass
    human_judgment: false

duration: 10min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 10: Ops Lifecycle Summary

**Factory-reset scope radios, orphan reconcile, scheduled/manual git gc, and operator docs for volume-backed repos (D-34–D-38).**

## Performance

- **Duration:** 10 min
- **Started:** 2026-09-12T18:51:28Z
- **Completed:** 2026-09-12T19:01:30Z
- **Tasks:** 3
- **Files modified:** 20

## Accomplishments

- Admin factory reset Dialog with Database only (default) vs Database and repositories scopes; server-side wipe confined under `repos_dir`
- In-process orphan reconcile (orphans + soft-delete retention, default 14d) and scheduled `git gc --auto`, plus `admin.repos.gc`
- CONFIGURATION documents `OXIDEAN_REPOS_DIR`, git ≥2.5 boot gate, Compose UID/GID ownership, and cleanup interval knobs

## Task Commits

Each task was committed atomically:

1. **Task 1: Factory reset scope API + admin modal** - `a4be3d4` (feat)
2. **Task 2: Orphan reconcile + scheduled/manual gc** - `e6d7c22` (feat)
3. **Task 3: CONFIGURATION operator docs** - `caf38a0` (docs)

## Files Created/Modified

- `apps/web/src/components/ui/radio-group.tsrx` - Base UI radio group wrapper
- `apps/web/src/routes/admin/auth.tsrx` - Reset this instance Dialog + scopes
- `crates/oxidean-api/src/jobs/*` - Orphan reconcile + interval schedulers
- `crates/oxidean-git/src/backend.rs` / `cli.rs` - `GitBackend::gc`
- `docs/CONFIGURATION.md` - Repos/git/cleanup operator runbook

## Decisions Made

- Cleanup frequency and soft-delete retention via ENV (`OXIDEAN_*_INTERVAL_SECS`, `OXIDEAN_SOFT_DELETE_RETENTION_DAYS`) rather than new instance_settings columns — avoids a Phase 7 schema migration while still documenting operator knobs.
- Soft-deleted bare dirs are kept until retention elapses; orphan scan only removes dirs with no matching DB row (active or soft-deleted).
- Manual GC: both `owner`+`name` for one repo, or omit both for all active repos.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] Path-escape refusal for wipe/orphan deletes**
- **Found during:** Task 1 / Task 2
- **Issue:** Threat model T-07-26 requires canonicalize-under-repos_dir for destructive FS ops.
- **Fix:** Shared strip_prefix guards in factory wipe and `delete_under_repos_dir`.
- **Files modified:** `crates/oxidean-api/src/auth/admin.rs`, `crates/oxidean-api/src/jobs/reconcile.rs`
- **Commit:** `a4be3d4`, `e6d7c22`

**2. [Rule 2 - Missing critical functionality] Explicit retention API for tests**
- **Found during:** Task 2
- **Issue:** Backdating `deleted_at` via raw sqlx in integration tests needed unlinked sqlx dep.
- **Fix:** Added `orphan_reconcile_with_retention` so retention=0 proves purge without env races or sqlx in tests.
- **Files modified:** `crates/oxidean-api/src/jobs/reconcile.rs`, `tests/orphan_reconcile_gc.rs`
- **Commit:** `e6d7c22`

## Threat Flags

None — factory reset and FS purge surfaces were in the plan threat model (T-07-25/26); `admin.repos.gc` is sys-admin gated like other admin RPCs.

## Self-Check: PASSED

- FOUND: `apps/web/src/components/ui/radio-group.tsrx`
- FOUND: `crates/oxidean-api/src/jobs/reconcile.rs`
- FOUND: `docs/CONFIGURATION.md` (OXIDEAN_REPOS_DIR + git 2.5)
- FOUND: commits `a4be3d4`, `e6d7c22`, `caf38a0`
