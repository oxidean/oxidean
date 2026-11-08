---
phase: 05-cloud-verify-reset
plan: "01"
subsystem: database
tags: [auth, email-tokens, sqlx, multi-dialect, UserPublic]

requires:
  - phase: 04-auth-sessions-email
    provides: users/sessions schema, DbPool match CRUD, email_verified_at column
provides:
  - Dialect-parity auth_email_tokens (0003) with hash-only magic+OTP columns
  - email_tokens CRUD + set/clear email_verified_at Database facades
  - UserPublic.email_verified field (D-13 shape for 05-02 tracer)
affects:
  - 05-02 require_verified tracer
  - 05-03 verify/reset expansion

actuals:
  tokens: 6116
  tasks: 2
  commits: 3

plan_head_before: 1a64a5c80ab8ababa4b2b09e725d856ca1495aa9

tech-stack:
  added: []
  patterns:
    - "DbPool match CRUD for auth_email_tokens mirroring sessions"
    - "UNIQUE(user_id, purpose) upsert replace-on-resend"
    - "Hash-only token_hash/otp_hash CHAR/TEXT 64 (T-05-03)"

key-files:
  created:
    - crates/oxidean-db/migrations/postgres/0003_email_tokens.sql
    - crates/oxidean-db/migrations/mysql/0003_email_tokens.sql
    - crates/oxidean-db/migrations/sqlite/0003_email_tokens.sql
    - crates/oxidean-db/src/email_tokens.rs
  modified:
    - crates/oxidean-db/src/users.rs
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-core/src/auth_types.rs
    - crates/oxidean-db/tests/dialect_auth.rs

key-decisions:
  - "Upsert re-fetches by (user_id, purpose) after ON CONFLICT for stable row return"
  - "UserPublic.email_verified field added without wiring user_to_public (deferred to 05-02)"

patterns-established:
  - "Email token CRUD stays in oxidean-db; API never embeds raw SQL"
  - "Dialect_auth DATABASE_URL gate covers token + verified helpers per dialect leg"

requirements-completed: [AUTH-04]

coverage:
  - id: D1
    description: "Dialect-parity 0003_email_tokens migrations with purpose, hashes, UNIQUE(user_id, purpose)"
    requirement: AUTH-04
    verification:
      - kind: unit
        ref: "crates/oxidean-db/src/migrate.rs#migration_parity"
        status: pass
    human_judgment: false
  - id: D2
    description: "email_tokens CRUD + set/clear email_verified_at proven on dialect_auth"
    requirement: AUTH-04
    verification:
      - kind: integration
        ref: "crates/oxidean-db/tests/dialect_auth.rs#migrate_email_token_and_verified_helpers"
        status: pass
    human_judgment: false
  - id: D3
    description: "UserPublic.email_verified boolean field present for D-13"
    requirement: AUTH-04
    verification:
      - kind: other
        ref: "grep email_verified crates/oxidean-core/src/auth_types.rs"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-09-10
status: complete
---

# Phase 5 Plan 01: Email Token Persistence Summary

**Dialect-parity `auth_email_tokens` (hash-at-rest magic+OTP), DB CRUD/verified helpers, and `UserPublic.email_verified` DTO field for the 05-02 tracer**

## Performance

- **Duration:** 6 min
- **Started:** 2026-09-10T21:56:10Z
- **Completed:** 2026-09-10T22:02:18Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- Shipped postgres/mysql/sqlite `0003_email_tokens.sql` with purpose, token_hash, otp_hash, expires_at, attempt_count, UNIQUE(user_id, purpose), token_hash index
- Added `email_tokens` module (upsert/find/increment/delete) and `set_email_verified_at` / `clear_email_verified_at` with Database facades
- Extended `UserPublic` with `email_verified: bool` (D-13); `user_to_public` wiring left for 05-02
- dialect_auth proves token upsert/find and verified flag round-trip when DATABASE_URL is set

## Task Commits

Each task was committed atomically:

1. **Task 1: 0003_email_tokens + CRUD + UserPublic.email_verified** - `b785a09` (feat)
2. **Task 2 RED: dialect_auth email token coverage** - `ec08dfd` (test)
3. **Task 2 GREEN: upsert re-fetch by user/purpose** - `6957f08` (feat)

_Note: TDD Task 2 produced RED + GREEN commits; no REFACTOR needed._

## TDD Gate Compliance

| Gate | Commit | Status |
|------|--------|--------|
| RED | `ec08dfd` test(5-01) | Pass — `check tdd-red-evidence` → RED_EVIDENCE_OK (`migrate_email_token_and_verified_helpers` assertion on `email_verified_at`) |
| GREEN | `6957f08` feat(5-01) | Pass — dialect_auth + migration_parity green |
| REFACTOR | — | Skipped (no cleanup needed) |

## Files Created/Modified

- `crates/oxidean-db/migrations/postgres/0003_email_tokens.sql` — dialect-parity token table
- `crates/oxidean-db/migrations/mysql/0003_email_tokens.sql` — dialect-parity token table
- `crates/oxidean-db/migrations/sqlite/0003_email_tokens.sql` — dialect-parity token table
- `crates/oxidean-db/src/email_tokens.rs` — hash-at-rest token CRUD
- `crates/oxidean-db/src/users.rs` — set/clear email_verified_at
- `crates/oxidean-db/src/lib.rs` — Database facades
- `crates/oxidean-core/src/auth_types.rs` — UserPublic.email_verified
- `crates/oxidean-db/tests/dialect_auth.rs` — token + verified helper integration

## Decisions Made

- Upsert returns the row via `find_by_user_purpose` after ON CONFLICT so replace-on-resend always resolves the UNIQUE key row
- Left `user_to_public` unwired per plan — 05-02 owns RPC contract mapping; API may not compile UserPublic literals until then

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Persistence foundation ready for 05-02 (`require_verified` + verify OTP tracer)
- Note: `UserPublic` gained a field; 05-02 must update `user_to_public` (and rpc-gen client) before API builds cleanly

---
*Phase: 05-cloud-verify-reset*
*Completed: 2026-09-10*

## Self-Check: PASSED
