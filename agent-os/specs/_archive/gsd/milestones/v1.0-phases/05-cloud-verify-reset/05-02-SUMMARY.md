---
phase: 05-cloud-verify-reset
plan: "02"
subsystem: auth
tags: [auth, email-verify, otp, require_verified, privileged_ping]

requires:
  - phase: 05-cloud-verify-reset
    provides: email_tokens CRUD, set_email_verified_at, UserPublic.email_verified field
provides:
  - require_verified gate with auth.email_unverified → HTTP 403
  - auth.verify OTP consume (signed-in, D-21) + issue_verify for tests
  - auth.dev.privileged_ping env-gated to {development,dev,test,compose}
  - auth.me / user_to_public email_verified wiring (D-13)
affects:
  - 05-03 verify/resend expansion
  - 05-04 password reset
  - Phase 7 repo.create privileged consumer

actuals:
  tokens: 5508
  tasks: 2
  commits: 2

plan_head_before: dfd53eb72c89fd036997963aa02c82fbdc1c4a96

tech-stack:
  added: []
  patterns:
    - "require_verified mirrors require_admin; returns UserRow"
    - "auth.dev.privileged_ping registered only on env allowlist (D-10)"
    - "SHA-256 hex at rest for magic + 8-digit OTP (D-20)"

key-files:
  created:
    - crates/oxidean-api/src/auth/gate.rs
    - crates/oxidean-api/src/auth/verify_reset.rs
    - crates/oxidean-api/tests/auth_verify_gate.rs
    - crates/oxidean-api/tests/auth_verify_reset.rs
  modified:
    - crates/oxidean-api/src/auth/local.rs
    - crates/oxidean-api/src/auth/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/app.rs

key-decisions:
  - "privileged_ping allowlist is {development,dev,test,compose} per Open Q2 RESOLVED"
  - "issue_verify is a library helper for tests; full resend/email RPC deferred to 05-03"

patterns-established:
  - "Gate helpers live in auth/gate.rs; privileged handlers call require_verified"
  - "rpc_status maps auth.email_unverified → 403 alongside admin.forbidden"

requirements-completed: [AUTH-04]

coverage:
  - id: D1
    description: "Unverified privileged_ping denied with auth.email_unverified and HTTP 403; OTP verify then ping ok"
    requirement: AUTH-04
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_gate.rs#unverified_privileged_ping_forbidden_then_ok_after_otp"
        status: pass
    human_judgment: false
  - id: D2
    description: "issue OTP, consume sets email_verified_at, auth.me.email_verified true"
    requirement: AUTH-04
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_reset.rs#issue_otp_consume_sets_email_verified_on_me"
        status: pass
    human_judgment: false
  - id: D3
    description: "privileged_ping absent outside allowlisted envs (production → rpc.unknown_procedure)"
    requirement: AUTH-04
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_gate.rs#privileged_ping_unknown_outside_env_allowlist"
        status: pass
    human_judgment: false

duration: 3min
completed: 2026-09-10
status: complete
---

# Phase 5 Plan 02: AUTH-04 Verify Gate Tracer Summary

**End-to-end verify OTP → `email_verified` on `auth.me` → `require_verified` / env-gated `auth.dev.privileged_ping`**

## Performance

- **Duration:** 3 min
- **Started:** 2026-09-10T22:03:57Z
- **Completed:** 2026-09-10T22:06:52Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- Shipped `require_verified` with stable `auth.email_unverified` + HTTP 403 mapping
- Wired `user_to_public.email_verified` from `email_verified_at` (D-13)
- Tracer OTP issue/consume (8-digit, 30m TTL, signed-in user match) and green gate/reset tests
- Confirmed `auth.dev.privileged_ping` is unknown outside `{development,dev,test,compose}`

## Task Commits

Each task was committed atomically:

1. **Task 1: E2E verify OTP → email_verified → privileged_ping** - `9e00337` (feat)
2. **Task 2: Confirm env-gated ping unknown outside allowlist** - `8de6cb3` (test)

**Plan metadata:** `5b056d7` (docs: complete plan)

## Files Created/Modified

- `crates/oxidean-api/src/auth/gate.rs` — `require_verified` helper
- `crates/oxidean-api/src/auth/verify_reset.rs` — issue/consume verify OTP + privileged_ping
- `crates/oxidean-api/src/auth/local.rs` — `email_verified` on `user_to_public`
- `crates/oxidean-api/src/auth/mod.rs` — module exports
- `crates/oxidean-api/src/rpc.rs` — `auth.verify` + env-gated `auth.dev.privileged_ping`
- `crates/oxidean-api/src/app.rs` — `auth.email_unverified` → 403
- `crates/oxidean-api/tests/auth_verify_gate.rs` — gate + production unknown coverage
- `crates/oxidean-api/tests/auth_verify_reset.rs` — OTP → me.email_verified

## Decisions Made

- Env allowlist for privileged_ping includes `compose` (plan Open Q2 RESOLVED over older RESEARCH wording)
- `issue_verify` stays a test/helper API; resend/rate-limit/email templates land in 05-03

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Tracer path proven; ready for 05-03 (resend/magic/rate-limit) and 05-04 (password reset)
- Future `repo.create` should call `require_verified` (Phase 7)

---
*Phase: 05-cloud-verify-reset*
*Completed: 2026-09-10*

## Self-Check: PASSED
