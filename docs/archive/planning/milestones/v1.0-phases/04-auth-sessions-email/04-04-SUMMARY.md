---
phase: 04-auth-sessions-email
plan: "04"
subsystem: auth
tags: [rpc, sessions, cookies, argon2, email, signup, login, rpc-gen]

requires:
  - phase: 04-auth-sessions-email
    provides: "Auth schema/CRUD (01), EmailSender (02), Argon2id + SessionService (03)"
provides:
  - "auth.signup|login|logout|logout_all|me|provider_config RPC over RpcCtx"
  - "HttpOnly oxidean_session Set-Cookie on signup/login; clear on logout"
  - "Welcome email on local signup only (D-20)"
  - "Optional OXIDEAN_ADMIN_* empty-table admin seed (T-04-13)"
  - "Generated api-client auth namespace with credentials: include"
  - "auth_signup + auth_session integration tests (AUTH-01/02/03)"
affects:
  - 04-05-oidc-workos
  - 04-06-profile-admin
  - 04-07-auth-ui

tech-stack:
  added: []
  patterns:
    - "RpcCtx carries session + CookieChange; rpc_http attaches Set-Cookie"
    - "auth.unauthenticated → HTTP 401; other auth errors → 400"
    - "AppState::new + router_with_state for injectable EmailSender in tests"
    - "Rust-native sessions only — no Better Auth (D-10)"

key-files:
  created:
    - crates/oxidean-api/src/auth/local.rs
    - crates/oxidean-api/tests/auth_signup.rs
    - crates/oxidean-api/tests/auth_session.rs
  modified:
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-api/src/auth/mod.rs
    - crates/oxidean-api/src/lib.rs
    - crates/oxidean-api/src/main.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts
    - .env.example
    - .planning/phases/04-auth-sessions-email/04-USER-SETUP.md

key-decisions:
  - "auth.unauthenticated maps to HTTP 401; other auth domain errors stay BAD_REQUEST (200 only on ok)"
  - "provider_config falls back to local when DB settings unavailable"
  - "Welcome email failures are logged, not failing signup"
  - "Admin seed username admin (or admin1); Phase 6 owns wizard"

patterns-established:
  - "Session-aware dispatch via RpcCtx + CookieChange Set|Clear"
  - "Integration tests use router_with_state + RecordingSender/LogSink"
  - "rpc-gen emits auth.* + TanStack Query helpers"

requirements-completed: [AUTH-01, AUTH-02, AUTH-03]

duration: 4min
completed: 2026-09-09
---

# Phase 4 Plan 04: Local Auth RPC & Sessions Summary

**Rust-native local auth RPC with HttpOnly `oxidean_session` cookies, welcome email on signup, optional admin seed, and green AUTH-01/02/03 integration tests**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-09T23:31:43Z
- **Completed:** 2026-09-09T23:36:10Z
- **Tasks:** 2
- **Files modified:** 12

## Accomplishments

- Local `auth.signup` / `auth.login` / `auth.logout` / `auth.logout_all` / `auth.me` / `auth.provider_config` over session-aware `RpcCtx`
- Cookie round-trip: Set-Cookie on signup/login; clear on logout; `auth.me` persists across requests
- Welcome email (`Welcome to Oxidean`) on local signup only; mode≠local → `auth.provider_mismatch`
- Optional `OXIDEAN_ADMIN_*` seed when users empty; rpc-gen auth client; AUTH-01/02/03 tests green

## Task Commits

Each task was committed atomically:

1. **Task 1: Local provider + session-aware RPC dispatch** - `ada59cd` (feat)
2. **Task 2: Admin seed, rpc-gen, integration tests AUTH-01/02/03** - `8906e42` (feat)

**Plan metadata:** `5725aa4` (docs: complete local auth RPC plan)

## Files Created/Modified

- `crates/oxidean-api/src/auth/local.rs` — signup/login/logout/me/provider_config + welcome email
- `crates/oxidean-api/src/rpc.rs` — `RpcCtx`, `CookieChange`, auth procedure dispatch
- `crates/oxidean-api/src/app.rs` — expanded `AppState`, cookie parse/Set-Cookie, 401 for unauthenticated
- `crates/oxidean-api/src/main.rs` — `OXIDEAN_ADMIN_*` empty-table admin seed
- `crates/oxidean-api/src/bin/rpc_gen.rs` / `packages/api-client/src/index.ts` — auth namespace + helpers
- `crates/oxidean-api/tests/auth_signup.rs` — AUTH-01 cookie, taken, reserved, welcome
- `crates/oxidean-api/tests/auth_session.rs` — AUTH-02/03 me/logout/logout_all
- `.env.example` / `04-USER-SETUP.md` — admin + mail + SSO placeholders

## Decisions Made

- HTTP status: `auth.unauthenticated` → 401; other auth errors → 400 (existing style); success → 200
- Signup does not fail if welcome email send errors (log only)
- Honored D-10: sessions owned in Rust/`sessions` table only — no Better Auth path

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

**External services / optional bootstrap require manual configuration.** See [04-USER-SETUP.md](./04-USER-SETUP.md) for:
- `OXIDEAN_ADMIN_EMAIL` / `OXIDEAN_ADMIN_PASSWORD`
- SMTP / Resend / mail From
- Verification commands

## Next Phase Readiness

- Local cookie auth ready for OIDC/WorkOS (04-05) to mint the same Oxidean session after callback
- Profile/admin RPCs (04-06) and auth UI (04-07) can call generated `auth.*` client

## Self-Check: PASSED

- `crates/oxidean-api/src/auth/local.rs` — FOUND
- `crates/oxidean-api/tests/auth_signup.rs` — FOUND
- `crates/oxidean-api/tests/auth_session.rs` — FOUND
- Commits `ada59cd`, `8906e42` — FOUND
- `cargo test -p oxidean-api --test auth_signup --test auth_session` — 7 passed

---
*Phase: 04-auth-sessions-email*
*Completed: 2026-09-09*
