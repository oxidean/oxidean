---
phase: 06-self-host-admin-bootstrap
plan: "01"
subsystem: database
tags: [auth-06, auth-07, allow_signup, must_change_credentials, migrations, dto, tdd]

requires:
  - phase: 06-self-host-admin-bootstrap
    provides: Wave 0 RED dialect_auth stub for 0006_bootstrap_flags
provides:
  - "Dialect-parity 0006_bootstrap_flags (allow_signup + must_change_credentials)"
  - "AuthSettingsRow.allow_signup + update signature"
  - "UserRow.must_change_credentials + update_user_email / set|clear helpers"
  - "Core DTOs + reserved system-administrator; dialect_auth GREEN"
affects:
  - 06-02 ENV seed + confirm credentials
  - 06-03 wizard + RPC allowlist
  - 06-04 closed signup
  - 06-08 chrome omit

actuals:
  tokens: 8505
  tasks: 2
  commits: 3

plan_head_before: e8d2cb73e6f7ed3f152157ca8f49faa0f4912a0f

tech-stack:
  added: []
  patterns:
    - "Boolean columns: INTEGER 0/1 SQLite, BOOLEAN elsewhere; map via bool|i64|i8 like sessions.remember_me"
    - "TDD RED via serde JSON field absence (Null) + reserved-username assertion before DTO implementation"

key-files:
  created:
    - crates/oxidean-db/migrations/sqlite/0006_bootstrap_flags.sql
    - crates/oxidean-db/migrations/postgres/0006_bootstrap_flags.sql
    - crates/oxidean-db/migrations/mysql/0006_bootstrap_flags.sql
  modified:
    - crates/oxidean-db/src/auth_settings.rs
    - crates/oxidean-db/src/users.rs
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-db/tests/dialect_auth.rs
    - crates/oxidean-core/src/auth_types.rs
    - crates/oxidean-api/src/auth/admin.rs
    - crates/oxidean-api/src/auth/local.rs

key-decisions:
  - "BootstrapSetupRequest.allow_signup and UpdateAuthSettingsRequest.allow_signup use serde default false (fail closed)"
  - "provider_config reads allow_signup from settings and fails closed on DB error"
  - "No separate instance_flags table — columns on existing singleton + users"

patterns-established:
  - "Dialect-safe bool mapping mirrored from SessionRow.remember_me"
  - "Database facades for email update and must_change_credentials flag"

requirements-completed: [AUTH-06, AUTH-07]

coverage:
  - id: D1
    description: "0006 migrations + AuthSettingsRow/UserRow helpers for allow_signup and must_change_credentials"
    requirement: AUTH-06
    verification:
      - kind: unit
        ref: "cargo test -p oxidean-db --lib migration_parity"
        status: pass
      - kind: integration
        ref: "cargo test -p oxidean-db --test dialect_auth migrate_0006_bootstrap_flags_columns"
        status: pass
    human_judgment: false
  - id: D2
    description: "Core DTOs expose allow_signup / must_change_credentials; system-administrator reserved"
    requirement: AUTH-07
    verification:
      - kind: unit
        ref: "cargo test -p oxidean-core --lib"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-09-11
status: complete
---

# Phase 06 Plan 01: Bootstrap Flags Schema + DTOs Summary

**Dialect-parity `0006_bootstrap_flags` plus DB helpers and core DTOs for `allow_signup` / `must_change_credentials`, with `system-administrator` reserved**

## Performance

- **Duration:** 6 min
- **Started:** 2026-09-11T20:24:40Z
- **Completed:** 2026-09-11T20:30:29Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments

- Added sqlite/postgres/mysql `0006_bootstrap_flags` with defaults false/0 (T-06-01 fail closed)
- Extended `AuthSettingsRow` / `UserRow` CRUD and `Database` facades (`update_user_email`, set/clear `must_change_credentials`)
- Greened Wave 0 `dialect_auth` round-trip; shipped DTO fields + reserved `system-administrator`

## Task Commits

1. **Task 1: 0006_bootstrap_flags + DB row helpers** - `c540de7` (feat)
2. **Task 2 RED: failing DTO/reserved tests** - `ca39ed4` (test)
3. **Task 2 GREEN: DTO fields + dialect coverage** - `eedd5c0` (feat)

_Note: TDD task produced RED then GREEN commits; no REFACTOR needed._

## Files Created/Modified

- `crates/oxidean-db/migrations/*/0006_bootstrap_flags.sql` — dialect-parity columns
- `crates/oxidean-db/src/auth_settings.rs` — `allow_signup` on row/select/update
- `crates/oxidean-db/src/users.rs` — `must_change_credentials` + email/flag helpers
- `crates/oxidean-db/src/lib.rs` — Database facades
- `crates/oxidean-db/tests/dialect_auth.rs` — migrate + round-trip GREEN
- `crates/oxidean-core/src/auth_types.rs` — DTO fields + reserved username
- `crates/oxidean-api/src/auth/admin.rs` / `local.rs` — map new fields (Rule 3 compile fix)

## Decisions Made

- `#[serde(default)]` on request `allow_signup` fields so older clients omit safely (default false)
- `auth.provider_config` exposes `allow_signup` for chrome; DB error → false
- Kept BootstrapStatus lean (`needs_setup` only) per RESEARCH A2

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated API mappers for new row/DTO fields**
- **Found during:** Task 1 / Task 2
- **Issue:** `update_auth_settings` signature and `AuthSettingsRow`/`UserPublic`/`ProviderConfigPublic` field adds broke `oxidean-api` constructors
- **Fix:** Wired `admin.rs` / `local.rs` to preserve then persist `allow_signup` and map `must_change_credentials`
- **Files modified:** `crates/oxidean-api/src/auth/admin.rs`, `crates/oxidean-api/src/auth/local.rs`
- **Commit:** `c540de7`, `eedd5c0`

## TDD Gate Compliance

- RED: `ca39ed4` — `system_administrator_is_reserved` + DTO JSON field tests failed intentionally; evidence `.planning/tmp/06-01-tdd-red-evidence.json` → `RED_EVIDENCE_OK`
- GREEN: `eedd5c0` — core lib + dialect_auth pass
- REFACTOR: skipped (no cleanup needed)

## Known Stubs

None in this plan's deliverables. Wave 0 API/web stubs remain RED for later 06-xx plans (expected).

## Threat Flags

None beyond plan register (T-06-01/T-06-02 mitigated by DEFAULT false + fail-closed provider_config read).

## Self-Check: PASSED

- FOUND: `crates/oxidean-db/migrations/sqlite/0006_bootstrap_flags.sql`
- FOUND: `crates/oxidean-db/migrations/postgres/0006_bootstrap_flags.sql`
- FOUND: `crates/oxidean-db/migrations/mysql/0006_bootstrap_flags.sql`
- FOUND: commits `c540de7`, `ca39ed4`, `eedd5c0`
