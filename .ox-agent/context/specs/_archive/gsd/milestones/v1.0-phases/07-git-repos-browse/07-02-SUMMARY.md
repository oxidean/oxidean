---
phase: 07-git-repos-browse
plan: "02"
subsystem: database
tags: [sqlx, migrations, repositories, validate_repo_name, soft-delete]
requires:
  - phase: 07-git-repos-browse
    provides: "D-14 owner_repo_path + D-33 fail_boot_git locked (07-01); Wave 0 dialect_repositories stub (07-00)"
provides:
  - "Tri-dialect 0007_repositories with soft-delete uniqueness"
  - "validate_repo_name + reserved flat routes including new"
  - "insert_repository / find_repository_by_owner_name DB API"
affects:
  - 07-12-create-tracer
  - 07-03-create-ux
  - 07-04-home-defaults
actuals:
  tokens: 7583
  tasks: 2
  commits: 6
plan_head_before: 19e8e769a44d18fd2a8bf1129b378b29b11dd5da
tech-stack:
  added: []
  patterns:
    - "Partial unique index (PG/SQLite WHERE deleted_at IS NULL); MySQL generated active_name UNIQUE"
    - "validate_repo_name separate from validate_username (allows _ and .)"
key-files:
  created:
    - crates/oxidean-db/migrations/postgres/0007_repositories.sql
    - crates/oxidean-db/migrations/mysql/0007_repositories.sql
    - crates/oxidean-db/migrations/sqlite/0007_repositories.sql
    - crates/oxidean-db/src/repositories.rs
    - crates/oxidean-core/src/repo_types.rs
    - .planning/phases/07-git-repos-browse/.evidence/07-02-t1-red.json
  modified:
    - crates/oxidean-core/src/auth_types.rs
    - crates/oxidean-core/src/lib.rs
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-db/src/users.rs
    - crates/oxidean-db/src/auth_settings.rs
    - crates/oxidean-db/tests/dialect_repositories.rs
key-decisions:
  - "Checkpoint Task 0: proceed_locked — honor owner_repo_path (D-14) + fail_boot_git (D-33)"
  - "MySQL soft-delete uniqueness via generated active_name column (NULLs exempt from UNIQUE)"
  - "Extend RESERVED_USERNAMES with new plus browse path segments (commits/branches/tags/…)"
patterns-established:
  - "RepositoryRow + dialect match helpers mirrored from users.rs"
  - "User.default_branch / instance default_visibility columns co-shipped with 0007"
requirements-completed: [GIT-01, GIT-08]
coverage:
  - id: D1
    description: "Tri-dialect 0007_repositories with soft-delete + uniqueness among non-deleted"
    requirement: GIT-01
    verification:
      - kind: unit
        ref: "crates/oxidean-db/tests/dialect_repositories.rs#migrate_0007_repositories_schema_presence"
        status: pass
      - kind: unit
        ref: "crates/oxidean-db --lib migration_parity"
        status: pass
    human_judgment: false
  - id: D2
    description: "validate_repo_name allows hyphen/underscore/period; reserved includes new"
    requirement: GIT-01
    verification:
      - kind: unit
        ref: "crates/oxidean-core/src/repo_types.rs#validate_repo_name_accepts_my_app"
        status: pass
      - kind: unit
        ref: "crates/oxidean-core/src/auth_types.rs#new_is_reserved_for_flat_routes"
        status: pass
    human_judgment: false
  - id: D3
    description: "insert_repository + get by owner+name for non-deleted rows; default_branch/default_visibility columns"
    requirement: GIT-08
    verification:
      - kind: unit
        ref: "crates/oxidean-db/tests/dialect_repositories.rs#migrate_0007_repositories_schema_presence"
        status: pass
    human_judgment: false
duration: 6min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 02: Repositories Schema & Validators Summary

**Tri-dialect `0007_repositories` with soft-delete uniqueness, `validate_repo_name`, reserved `/new`, and DB insert/get helpers ready for the 07-12 create tracer.**

## Performance

- **Duration:** 6 min
- **Started:** 2026-09-12T16:58:07Z
- **Completed:** 2026-09-12T17:03:57Z
- **Tasks:** 2 (T0 checkpoint + T1 TDD)
- **Files modified:** 12

## Accomplishments

- Confirmed **proceed_locked** for D-14 (`owner_repo_path`) and D-33 (`fail_boot_git`) from 07-01
- Landed tri-dialect `0007_repositories` + user `default_branch` / instance `default_visibility`
- Implemented `validate_repo_name` (D-06) and extended `RESERVED_USERNAMES` with `new` and browse segments
- Wired `RepositoryRow` + `insert_repository` / `find_repository_by_owner_name` on `Database`

## Task Commits

1. **Task 0: Confirm proceed with D-14/D-33** — checkpoint decision — selected `proceed_locked` (no code commit)
2. **Task 1 RED: failing validators + schema tests** — `ed91eb0` (test)
3. **Task 1 GREEN: migrations + validators + DB API** — `2920eea` (feat)

**Plan metadata:** `b1017f9` (docs: complete plan)

## TDD Gate Compliance

| Gate | Commit | Evidence |
|------|--------|----------|
| RED | `ed91eb0` | `.evidence/07-02-t1-red.json` → `RED_EVIDENCE_OK` (`validate_repo_name_accepts_my_app`) |
| GREEN | `2920eea` | `cargo test -p oxidean-core --lib` + `migration_parity` + `dialect_repositories` all pass |
| REFACTOR | — | skipped (not needed) |

## Files Created/Modified

- `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0007_repositories.sql` — repositories + settings columns
- `crates/oxidean-db/src/repositories.rs` — dialect CRUD helpers
- `crates/oxidean-core/src/repo_types.rs` — DTOs + `validate_repo_name`
- `crates/oxidean-core/src/auth_types.rs` — reserved `new` + browse path segments
- `crates/oxidean-db/src/users.rs` / `auth_settings.rs` — read new default columns
- `crates/oxidean-db/tests/dialect_repositories.rs` — green insert/get coverage

## Decisions Made

- **proceed_locked** — continue with locked D-14/D-33 from 07-01
- MySQL uniqueness via generated `active_name` (partial indexes unsupported)
- Repo name validator is separate from username (allows `_` and `.`)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Schema + validators ready for **07-12** `repo.create` tracer (RPC + GitBackend still deferred)
- Default branch/visibility columns ready for **07-04** settings UI

## Auth Gates

None

## Known Stubs

None — `CreateRepoRequest` / `RepoPublic` DTOs are intentional pre-RPC types (wired in 07-12); not empty UI stubs.

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*

## Self-Check: PASSED

- Created files present
- RED `ed91eb0` and GREEN `2920eea` commits present
