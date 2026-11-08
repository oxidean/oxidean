---
phase: 04-auth-sessions-email
plan: "03"
subsystem: auth
tags: [argon2, sessions, cookies, sha256, httponly]

requires:
  - phase: 04-auth-sessions-email
    provides: "users/sessions CRUD via oxidean-db Database helpers"
provides:
  - "Argon2id hash_password / verify_password PHC helpers"
  - "SessionService create/resolve/revoke/revoke_all with opaque oxidean_session cookie"
  - "24h idle slide + 30d remember-me absolute TTL; Secure flag via OXIDEAN_ENV"
affects:
  - 04-04-local-auth-rpc
  - 04-05-oidc-workos
  - 04-07-auth-ui

tech-stack:
  added: [argon2@0.6, password-hash@0.6, sha2, rand, uuid, chrono, cookie]
  patterns:
    - "Store SHA-256(token) only; cookie holds raw CSPRNG token"
    - "secure_cookies false for development/dev only"
    - "Idle sessions refresh expires_at on resolve; remember-me keeps absolute expiry"

key-files:
  created:
    - crates/oxidean-api/src/auth/mod.rs
    - crates/oxidean-api/src/auth/password.rs
    - crates/oxidean-api/src/auth/session.rs
  modified:
    - crates/oxidean-api/Cargo.toml
    - crates/oxidean-api/src/lib.rs
    - Cargo.lock

key-decisions:
  - "argon2 0.6 API: SaltString::generate + hash_password_with_salt (not 0.5 OsRng path)"
  - "Raw session token hex-encoded 32 bytes; SHA-256 hex in DB"
  - "SessionService holds env_name for Secure cookie flag; methods take &Database"

patterns-established:
  - "auth/ module under oxidean-api with password + session unit tests"
  - "Cookie flags: HttpOnly Path=/ SameSite=Lax; no Domain attribute"

requirements-completed: [AUTH-02]

duration: 3min
completed: 2026-09-09
---

# Phase 4 Plan 03: Argon2id + SessionService Summary

**Argon2id PHC password helpers and opaque `oxidean_session` cookies with SHA-256 token hashes, 24h idle / 30d remember-me TTLs**

## Performance

- **Duration:** 3 min
- **Started:** 2026-09-09T23:25:58Z
- **Completed:** 2026-09-09T23:29:22Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- `hash_password` / `verify_password` with Argon2id PHC strings; `hash_password_str` enforces min length 8
- `SessionService` mint/resolve/revoke over `oxidean-db`; HttpOnly SameSite=Lax cookies; Secure when not development
- Idle sessions slide expiry on resolve; remember-me keeps absolute 30d expiry while updating last_seen

## Task Commits

Each task was committed atomically:

1. **Task 1: Argon2id password helpers** - `0d08797` (feat)
2. **Task 2: SessionService + cookie builders** - `4896609` (feat)

**Plan metadata:** (docs commit after this summary)

## Files Created/Modified

- `crates/oxidean-api/src/auth/password.rs` — Argon2id hash/verify + min-length wrapper
- `crates/oxidean-api/src/auth/session.rs` — SessionService, cookie builders, secure_cookies
- `crates/oxidean-api/src/auth/mod.rs` — auth module exports
- `crates/oxidean-api/src/lib.rs` — `pub mod auth`
- `crates/oxidean-api/Cargo.toml` — crypto/cookie deps
- `Cargo.lock` — lockfile update

## Decisions Made

- Used argon2 0.6 `SaltString::generate` + `hash_password_with_salt` (RESEARCH snippet targeted 0.5-style API)
- Hex-encoded 32-byte tokens (not base64url); SHA-256 digest hex for `token_hash`
- `SessionService::new(env_name)` carries Secure-flag policy; DB passed per call

## Deviations from Plan

None - plan executed exactly as written (argon2 0.6 call shape adapted to current crate API without behavior change).

---

**Total deviations:** 0 auto-fixed
**Impact on plan:** N/A

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Password + session primitives ready for `auth.login` / `auth.logout` / `auth.me` RPC wiring in 04-04
- AUTH-02 end-to-end (cookie round-trip via HTTP) still needs plan 04-04

## Self-Check: PASSED

- Created files present: password.rs, session.rs, auth/mod.rs, 04-03-SUMMARY.md
- Commits present: `0d08797`, `4896609`
- `cargo test -p oxidean-api --lib auth::` — 9 passed

---
*Phase: 04-auth-sessions-email*
*Completed: 2026-09-09*
