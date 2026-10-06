---
phase: 05-cloud-verify-reset
plan: "04"
subsystem: auth
tags: [auth, password-reset, anti-enumeration, magic-link, otp, session-revoke]

requires:
  - phase: 05-cloud-verify-reset
    provides: verify issue/consume/rate-limit channel (purpose verify)
provides:
  - auth.request_password_reset anti-enumeration (D-28) with local-only mail (D-26)
  - auth.reset_password logged-out redeem → hash + revoke_all + sign-in (D-27)
  - Reset magic+OTP email subject/body reusing verify TTL/rate-limit policy (D-24, D-25)
affects:
  - 05-07 reset-password UI pages
  - AUTH-12 verification

actuals:
  tokens: 7178
  tasks: 2
  commits: 4

plan_head_before: 409aee5dbdca78c97a3051d5fa787751067b7ffb

tech-stack:
  added: []
  patterns:
    - "Anti-enumeration request always ok; rate_limit swallowed into ok (no existence leak)"
    - "Logged-out redeem via token_hash/otp_hash lookup (contrast session-scoped verify)"
    - "Post-reset: revoke_all then CookieChange::Set (not Clear)"

key-files:
  created: []
  modified:
    - crates/oxidean-api/src/auth/verify_reset.rs
    - crates/oxidean-api/src/auth/local.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/tests/auth_verify_reset.rs
    - crates/oxidean-db/src/users.rs
    - crates/oxidean-db/src/lib.rs

key-decisions:
  - "Swallow auth.rate_limited into identical ok on request_password_reset so rate-limit side channel cannot enumerate accounts (D-28 over surfacing verify-style errors)"
  - "SSO-only redeem uses auth.sso_only with UI-SPEC copy; never revealed on request"
  - "Database::set_password_hash added for Argon2id update on redeem"

patterns-established:
  - "PURPOSE_RESET shares issue_token_inner / next_issue_count with verify"
  - "Reset email subject: Reset your Oxidean password; link /reset-password?token="

requirements-completed: [AUTH-12]

coverage:
  - id: D1
    description: "request_password_reset identical ok for unknown/local/SSO; mail only for local-password"
    requirement: AUTH-12
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#request_password_reset_anti_enumeration_identical_success"
        status: pass
    human_judgment: false
  - id: D2
    description: "Soft rate limit on reset does not send extra mail and still returns ok"
    requirement: AUTH-12
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#request_password_reset_rate_limit_swallows_into_ok"
        status: pass
    human_judgment: false
  - id: D3
    description: "reset_password sets hash, revokes prior sessions, signs in with Set-Cookie"
    requirement: AUTH-12
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#reset_password_token_revokes_others_and_signs_in"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#reset_password_otp_consume"
        status: pass
    human_judgment: false
  - id: D4
    description: "SSO-only redeem returns auth.sso_only; invalid/weak rejected"
    requirement: AUTH-12
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#reset_password_sso_only_rejected"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#reset_password_invalid_token_rejected"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#reset_password_weak_password_rejected"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-09-10
status: complete
---

# Phase 5 Plan 04: Password Reset RPCs Summary

**AUTH-12 password reset with anti-enumeration request, magic+OTP email for local accounts only, and logged-out redeem that revokes other sessions and signs in**

## Performance

- **Duration:** 6 min
- **Started:** 2026-09-10T22:20:25Z
- **Completed:** 2026-09-10T22:26:41Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- Shipped `auth.request_password_reset` with identical success for unknown/local/SSO and mail only when `password_hash` is set
- Shipped `auth.reset_password` (token or OTP + password ≥8) → Argon2id update, `revoke_all`, fresh session cookie
- Integration coverage for anti-enumeration, rate-limit swallow, multi-session revoke, SSO-only, invalid/weak paths

## Task Commits

Each task was committed atomically:

1. **Task 1: Request password reset (anti-enumeration)** - `01a74d1` (test) → `abb7d88` (feat)
2. **Task 2: Redeem reset → password + revoke others + sign-in** - `892a376` (test) → `dae88f8` (feat)

**Plan metadata:** `b672f7d` (docs: complete plan)

_Note: TDD tasks may have multiple commits (test → feat → refactor)_

## Files Created/Modified

- `crates/oxidean-api/src/auth/verify_reset.rs` — reset issue/send/redeem
- `crates/oxidean-api/src/auth/local.rs` — `normalize_email` pub(crate) for shared use
- `crates/oxidean-api/src/rpc.rs` — `auth.request_password_reset` / `auth.reset_password`
- `crates/oxidean-db/src/users.rs` + `lib.rs` — `set_password_hash`
- `crates/oxidean-api/tests/auth_verify_reset.rs` — AUTH-12 integration tests

## Decisions Made

- Rate-limited reset requests return the same `ok` payload (no `auth.rate_limited` leak) while skipping send
- Error class `auth.sso_only` carries UI-SPEC SSO copy for redeem-only failure

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Swallow rate_limit into ok on reset request**
- **Found during:** Task 1
- **Issue:** Surfacing `auth.rate_limited` only for known local emails would enumerate accounts (conflicts with D-28 / T-05-09)
- **Fix:** On rate limit, log and return identical success without sending
- **Files modified:** `verify_reset.rs`, tests
- **Verification:** `request_password_reset_rate_limit_swallows_into_ok`
- **Committed in:** `abb7d88`

**2. [Rule 2 - Missing critical] Added `Database::set_password_hash`**
- **Found during:** Task 2
- **Issue:** No DB helper to update Argon2id hash on redeem
- **Fix:** Dialect-aware `set_password_hash` on users + Database facade
- **Files modified:** `oxidean-db` users/lib
- **Verification:** redeem + login-with-new-password tests
- **Committed in:** `dae88f8`

---

**Total deviations:** 2 auto-fixed (Rule 2)
**Impact on plan:** Required for anti-enumeration correctness and password update; no scope creep.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Ready for 05-05 / 05-07 UI (`/reset-password` request + redeem) against these RPCs
- AUTH-12 RPC surface complete; log-sink counts as provider

---
*Phase: 05-cloud-verify-reset*
*Completed: 2026-09-10*

## Self-Check: PASSED
