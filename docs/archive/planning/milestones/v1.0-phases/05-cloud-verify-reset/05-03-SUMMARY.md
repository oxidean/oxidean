---
phase: 05-cloud-verify-reset
plan: "03"
subsystem: auth
tags: [auth, email-verify, magic-link, otp, rate-limit, signup, admin-seed]

requires:
  - phase: 05-cloud-verify-reset
    provides: email_tokens CRUD, require_verified gate, tracer auth.verify OTP consume
provides:
  - auth.request_verify / auth.resend_verify with magic+OTP email (D-14, D-22)
  - Soft rate limits auth.rate_limited ≥60s and ≤5/hour with issue_count (D-19)
  - Session-scoped redeem with 10-attempt invalidate (D-20, D-21)
  - Signup auto-send verify + welcome; admin seed auto-verified (D-23, D-04)
  - RESERVED_USERNAMES includes verify and reset-password (D-16)
  - AUTH-05 open signup retained (D-08)
affects:
  - 05-04 password reset channel reuse
  - 05-05 verify/reset UI pages and banner

actuals:
  tokens: 11976
  tasks: 3
  commits: 3

plan_head_before: 67e8dd5f4ceb5c8fe34075c84c4367a768aa11b3

tech-stack:
  added: []
  patterns:
    - "issue_count column + created_at for DB soft rate limits (no Redis)"
    - "Session-scoped token row lookup for redeem attempt attribution"
    - "OXIDEAN_PUBLIC_ORIGIN-only magic URLs (T-05-07)"
    - "maybe_seed_admin extracted to auth::seed for testability"

key-files:
  created:
    - crates/oxidean-api/src/auth/seed.rs
    - crates/oxidean-db/migrations/sqlite/0004_email_token_issue_count.sql
    - crates/oxidean-db/migrations/postgres/0004_email_token_issue_count.sql
    - crates/oxidean-db/migrations/mysql/0004_email_token_issue_count.sql
  modified:
    - crates/oxidean-api/src/auth/verify_reset.rs
    - crates/oxidean-api/src/auth/local.rs
    - crates/oxidean-api/src/auth/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/main.rs
    - crates/oxidean-db/src/email_tokens.rs
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-core/src/auth_types.rs
    - crates/oxidean-api/tests/auth_verify_reset.rs
    - crates/oxidean-api/tests/auth_signup.rs

key-decisions:
  - "Added issue_count via migration 0004 rather than amending 0003 (checksum safety)"
  - "Redeem looks up by session user_id+purpose so failed OTP attempts attribute correctly"
  - "Extracted maybe_seed_admin to auth::seed for unit/integration coverage of D-04"

patterns-established:
  - "issue_and_send_verify shared by request/resend/signup; mail failures log-only"
  - "Rate-limit tests backdate created_at via Database::set_email_token_created_at"

requirements-completed: [AUTH-04, AUTH-05]

coverage:
  - id: D1
    description: "request_verify sends one email with magic URL + 8-digit OTP"
    requirement: AUTH-04
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#request_verify_sends_magic_and_otp_email"
        status: pass
    human_judgment: false
  - id: D2
    description: "Resend replaces prior issuance; 60s and hourly rate limits return auth.rate_limited"
    requirement: AUTH-04
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#resend_replaces_prior_and_rate_limits_within_60s"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#sixth_issue_within_hour_rate_limited"
        status: pass
    human_judgment: false
  - id: D3
    description: "Magic token and OTP consume; wrong session user rejected; 10 failed OTPs invalidate"
    requirement: AUTH-04
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#magic_token_consume_sets_verified"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#verify_wrong_session_user_rejected"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#ten_failed_otp_attempts_invalidate"
        status: pass
    human_judgment: false
  - id: D4
    description: "Signup auto-sends verify+welcome; email_verified stays false; open signup without invite"
    requirement: AUTH-05
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_signup.rs#signup_sets_cookie_and_sends_welcome"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_signup.rs#signup_open_without_invite_fields_auth05"
        status: pass
    human_judgment: false
  - id: D5
    description: "Env-seeded admin is auto-verified (D-04)"
    requirement: AUTH-04
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_signup.rs#seeded_admin_is_auto_verified"
        status: pass
    human_judgment: false
  - id: D6
    description: "verify and reset-password are reserved usernames"
    requirement: AUTH-04
    verification:
      - kind: unit
        ref: "crates/oxidean-core/src/auth_types.rs#validate_username_rejects_verify_and_reset_password"
        status: pass
    human_judgment: false

duration: 9min
completed: 2026-09-10
status: complete
---

# Phase 5 Plan 03: Full Verify Channel Summary

**Verify issue/resend with magic+OTP email, DB soft rate limits, signup auto-send, admin auto-verify, and reserved route usernames — AUTH-04/AUTH-05 locked**

## Performance

- **Duration:** 9 min
- **Started:** 2026-09-10T22:09:16Z
- **Completed:** 2026-09-10T22:18:47Z
- **Tasks:** 3
- **Files modified:** 15

## Accomplishments

- Shipped `auth.request_verify` / `auth.resend_verify` and expanded `auth.verify` for magic token + OTP
- Soft rate limits (`auth.rate_limited`) via `issue_count` + `created_at`; 10-attempt redeem invalidate
- Local signup auto-sends verify (plus welcome); env admin seed sets `email_verified_at`; AUTH-05 open signup confirmed

## Task Commits

Each task was committed atomically:

1. **Task 1: Issue, resend, rate limits, magic-link consume** - `b1615a0` (feat)
2. **Task 2: Signup auto-send verify + admin seed verified** - `4b0acb3` (feat)
3. **Task 3: Reserve verify/reset-password usernames** - `344de19` (feat)

**Plan metadata:** `806a457` (docs: complete plan)

## Files Created/Modified

- `crates/oxidean-api/src/auth/verify_reset.rs` — issue/send/rate-limit/consume
- `crates/oxidean-api/src/auth/seed.rs` — `maybe_seed_admin` with D-04 verify
- `crates/oxidean-api/src/auth/local.rs` — signup auto-issue verify
- `crates/oxidean-api/src/rpc.rs` — request/resend procedures
- `crates/oxidean-db/migrations/postgres/0004_email_token_issue_count.sql` — `issue_count` column
- `crates/oxidean-db/migrations/mysql/0004_email_token_issue_count.sql` — `issue_count` column
- `crates/oxidean-db/migrations/sqlite/0004_email_token_issue_count.sql` — `issue_count` column
- `crates/oxidean-core/src/auth_types.rs` — reserved `verify`, `reset-password`
- Tests: `auth_verify_reset.rs`, `auth_signup.rs`

## Decisions Made

- Migration **0004** for `issue_count` (do not amend shipped 0003 checksums)
- Session-scoped redeem for attempt capping; magic URLs from `OXIDEAN_PUBLIC_ORIGIN` only

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Added `issue_count` column for hourly soft limit**
- **Found during:** Task 1
- **Issue:** UNIQUE `(user_id, purpose)` replace-on-resend cannot count ≤5/hour from `created_at` alone
- **Fix:** Migration `0004_email_token_issue_count` + upsert `issue_count`
- **Files modified:** email_tokens migrations + CRUD
- **Commit:** `b1615a0`

**2. [Rule 2 - Missing critical] Session-scoped redeem for attempt attribution**
- **Found during:** Task 1
- **Issue:** `find_by_otp_hash` on wrong code returns None with no row to increment
- **Fix:** Load token by session `user_id`+purpose; compare hashes; increment/delete at 10
- **Commit:** `b1615a0`

**Total deviations:** 2 auto-fixed (Rule 2). **Impact:** Required for D-19/D-20 threat mitigations.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Ready for 05-04 (password reset reusing issue/rate-limit/consume patterns)
- Ready for 05-05 UI (`/verify`, banner resend)

---
*Phase: 05-cloud-verify-reset*
*Completed: 2026-09-10*

## Self-Check: PASSED
