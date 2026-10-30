---
phase: 05-cloud-verify-reset
plan: "07"
subsystem: auth
tags: [auth, reset-password, otp, privileged-cta, anti-enumeration]

requires:
  - phase: 05-cloud-verify-reset
    provides: InputOtp + auth.requestPasswordReset / auth.resetPassword client methods
provides:
  - /reset-password request + redeem UI with anti-enumeration and SSO mode gate
  - Local-only Forgot password? link on /login
  - Dashboard disabled New repository CTA with verify vs later-phase hints (D-12)
affects:
  - AUTH-12 password reset UX
  - AUTH-04 privileged CTA visibility pattern
  - AUTH-05 open signup confirmation
  - Phase 7 repo.create CTA enablement

actuals:
  tokens: 4783
  tasks: 2
  commits: 2

plan_head_before: 9bb1b94f6e7fcdfe876a12778d7af6e63a6acc5b

tech-stack:
  added: []
  patterns:
    - "Reset request always shows identical anti-enumeration success (never not-found)"
    - "Privileged CTA disabled for everyone in Phase 5; hint differs by email_verified"
    - "Referrer-Policy no-referrer on /reset-password (parity with /verify)"

key-files:
  created:
    - apps/web/src/routes/reset-password.tsrx
  modified:
    - apps/web/src/routes/login.tsrx
    - apps/web/src/components/signed-in-home.tsrx
    - apps/web/src/routes/dashboard.tsrx
    - apps/web/src/routeTree.gen.ts

key-decisions:
  - "After anti-enumeration success, primary Enter reset code advances to redeem (code path without magic link)"
  - "Request form offers Already have a code? Enter it here for redeem without waiting for email send"
  - "styles.css unchanged — AuthShell already provides oct-auth-enter for /reset-password"

patterns-established:
  - "Reset redeem: token query and/or OTP + password/confirm; OTP onComplete auto-submits when passwords valid"
  - "SSO/OIDC reset page: IdP body + Continue CTA, no email form"
  - "Dashboard New repository: disabled primary + wrapping Label hint under button"

requirements-completed: [AUTH-12, AUTH-04, AUTH-05]

coverage:
  - id: D1
    description: "/reset-password request + redeem with anti-enumeration success, SSO mode body, InputOtp, exact UI-SPEC copy"
    requirement: AUTH-12
    verification:
      - kind: other
        ref: "bun run build (apps/web) — reset-password chunk emitted"
        status: pass
    human_judgment: true
    rationale: "Anti-enumeration copy, OTP/password redeem, and SSO mode need visual UAT"
  - id: D2
    description: "Login Forgot password? link to /reset-password in local mode only"
    requirement: AUTH-12
    verification:
      - kind: other
        ref: "grep Forgot password apps/web/src/routes/login.tsrx"
        status: pass
    human_judgment: true
    rationale: "Local-only visibility vs WorkOS/OIDC panels needs human-check"
  - id: D3
    description: "Dashboard New repository disabled with verify vs later-phase hints (D-12); signup remains invite-free"
    requirement: AUTH-04
    verification:
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
      - kind: integration
        ref: "cargo test -p oxidean-api --test auth_verify_gate --test auth_verify_reset --test auth_signup"
        status: pass
    human_judgment: true
    rationale: "Hint copy and disabled styling light/dark need human-check; AUTH-05 is absence of invite UI"

duration: 4min
completed: 2026-09-10
status: complete
---

# Phase 5 Plan 07: Reset UI + Forgot Link + Privileged CTA Summary

**/reset-password request/redeem with anti-enumeration + SSO gate, local Forgot password? entry, and disabled New repository CTA**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-10T22:49:36Z
- **Completed:** 2026-09-10T22:53:52Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Shipped `/reset-password` (AuthShell + request anti-enumeration + redeem OTP/password + SSO IdP panel + Referrer-Policy)
- Added local-only **Forgot password?** on `/login` before Remember me
- Signed-in home **New repository** always disabled with verify vs later-phase hints (D-12); no create RPC (`/dashboard` soft-redirects to `/`)

## Task Commits

Each task was committed atomically:

1. **Task 1: /reset-password + login forgot link** — `50824be` (feat)
2. **Task 2: Dashboard disabled New repository CTA** — `e1453a2` (feat)

## Files Created/Modified

- `apps/web/src/routes/reset-password.tsrx` — request + redeem + SSO mode per UI-SPEC
- `apps/web/src/routes/login.tsrx` — Forgot password? → `/reset-password` (local only)
- `apps/web/src/components/signed-in-home.tsrx` — disabled New repository + hints
- `apps/web/src/routes/dashboard.tsrx` — legacy `/dashboard` → `/` redirect
- `apps/web/src/routeTree.gen.ts` — `/reset-password` registration

## Decisions Made

- After anti-enumeration success, **Enter reset code** advances to redeem so users can use OTP without the magic link
- Request form includes **Already have a code?** for redeem without a prior send in this session
- No `styles.css` change — `AuthShell` already applies `oct-auth-enter`

## Deviations from Plan

None - plan executed exactly as written.

Note: first parallel `auth_verify_gate` + `auth_verify_reset` run hit a pre-existing `OXIDEAN_PUBLIC_ORIGIN` env race (`request_verify_sends_magic_and_otp_email`). Re-ran suites sequentially — all green. Not caused by this plan’s UI changes; left as-is (scope boundary).

## Authentication Gates

None

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 5 plans complete. Ready for end-of-phase UAT / `$gsd-verify-work` (AUTH-04, AUTH-05, AUTH-12). Phase 7 can enable real `repo.create` behind the existing CTA pattern.

---
*Phase: 05-cloud-verify-reset*
*Completed: 2026-09-10*

## Self-Check: PASSED
