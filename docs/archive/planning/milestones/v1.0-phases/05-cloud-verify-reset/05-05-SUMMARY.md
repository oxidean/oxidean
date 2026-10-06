---
phase: 05-cloud-verify-reset
plan: "05"
subsystem: auth
tags: [auth, sso, idp-trust, email-verification, workos, oidc]

requires:
  - phase: 05-cloud-verify-reset
    provides: require_verified gate + auth.dev.privileged_ping + email_verified on auth.me
provides:
  - ExternalIdentity.email_verified IdP-trust marking on WorkOS/OIDC link/create (D-03, D-15)
  - clear_email_verification helper for future email-change (D-05)
affects:
  - AUTH-04 SSO edge verification
  - future email-edit path (D-05)

actuals:
  tokens: 3466
  tasks: 2
  commits: 3

plan_head_before: fef921584b0164e714ff75f134aa78eb79fb63d4

tech-stack:
  added: []
  patterns:
    - "ExternalIdentity.email_verified → apply_idp_email_verified after link/create"
    - "OIDC trusts email_verified claim only when Some(true)"
    - "clear_email_verification is pub(crate) only — no public RPC (T-05-14)"

key-files:
  created: []
  modified:
    - crates/oxidean-api/src/auth/external.rs
    - crates/oxidean-api/src/auth/workos.rs
    - crates/oxidean-api/src/auth/oidc.rs
    - crates/oxidean-api/src/auth/verify_reset.rs
    - crates/oxidean-api/tests/auth_verify_gate.rs

key-decisions:
  - "Re-apply IdP-trust on existing identity link path so returning SSO users with newly verified IdP email get marked"
  - "OIDC map uses claims.email_verified() == Some(true); false/absent leaves local verify flows"

patterns-established:
  - "map_workos_email_verified / map_oidc_email_verified pure helpers at adapter boundary"
  - "D-05 clear helper lives beside verify_reset issue/consume for future email-change callers"

requirements-completed: [AUTH-04]

coverage:
  - id: D1
    description: "WorkOS/OIDC IdP email_verified asserted → users.email_verified_at set; privileged_ping ok without local OTP"
    requirement: AUTH-04
    verification:
      - kind: unit
        ref: "crates/oxidean-api/src/auth/external.rs#link_or_create_marks_verified_when_idp_asserts"
        status: pass
      - kind: unit
        ref: "crates/oxidean-api/src/auth/workos.rs#maps_workos_email_verified_into_identity_flag"
        status: pass
      - kind: unit
        ref: "crates/oxidean-api/src/auth/oidc.rs#maps_oidc_email_verified_claim_some_true_only"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/auth_verify_gate.rs#idp_trust_verified_sso_user_privileged_ping_ok"
        status: pass
    human_judgment: false
  - id: D2
    description: "IdP false/absent leaves email_verified_at null so local verify flows still apply"
    requirement: AUTH-04
    verification:
      - kind: unit
        ref: "crates/oxidean-api/src/auth/external.rs#link_or_create_leaves_unverified_when_idp_does_not_assert"
        status: pass
    human_judgment: false
  - id: D3
    description: "clear_email_verification clears flag and require_verified fails afterward (D-05 prep)"
    requirement: AUTH-04
    verification:
      - kind: unit
        ref: "crates/oxidean-api/src/auth/verify_reset.rs#clear_email_verification_clears_verified_flag"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-09-10
status: complete
---

# Phase 5 Plan 05: IdP-Trust Verified Marking Summary

**SSO IdP-trust marks `email_verified_at` when WorkOS/OIDC asserts verified email, plus internal `clear_email_verification` for D-05 email-change**

## Performance

- **Duration:** 5 min
- **Started:** 2026-09-10T22:28:26Z
- **Completed:** 2026-09-10T22:33:46Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Extended `ExternalIdentity` with `email_verified`; WorkOS/OIDC map IdP claims; `link_or_create_user` sets `email_verified_at` when trusted
- Integration proof: IdP-trust SSO user → `auth.me.email_verified` true and `privileged_ping` without local OTP
- Shipped `clear_email_verification` + required unit test that `require_verified` fails after clear

## TDD Gate Compliance

- **RED:** `test(05-05)` `040d37f` — `link_or_create_marks_verified_when_idp_asserts` failed on assertion (`email_verified_at` None); evidence `RED_EVIDENCE_OK` in `.planning/phases/05-cloud-verify-reset/.evidence/05-05-t1-red.json`
- **GREEN:** `feat(05-05)` `f72dc49` — mapping helpers + `apply_idp_email_verified`; all IdP-trust tests pass
- **REFACTOR:** skipped (no cleanup needed)

## Task Commits

Each task was committed atomically:

1. **Task 1: IdP-trust mark verified on SSO success** - `040d37f` (test) → `f72dc49` (feat)
2. **Task 2: clear_email_verification helper for D-05** - `9047375` (feat)

**Plan metadata:** `2d49a5d` (docs: complete plan)

_Note: TDD tasks may have multiple commits (test → feat → refactor)_

## Files Created/Modified

- `crates/oxidean-api/src/auth/external.rs` — `email_verified` field + IdP-trust apply on link/create
- `crates/oxidean-api/src/auth/workos.rs` — `map_workos_email_verified` + finish mapping
- `crates/oxidean-api/src/auth/oidc.rs` — `map_oidc_email_verified` (Some(true) only) + finish mapping
- `crates/oxidean-api/src/auth/verify_reset.rs` — `clear_email_verification` + unit test
- `crates/oxidean-api/tests/auth_verify_gate.rs` — IdP-trust privileged_ping integration

## Decisions Made

- Apply IdP-trust on both new-user and existing-identity return paths so a later IdP assertion still marks verified
- Keep clear helper `pub(crate)` with rustdoc for D-05; no email-edit RPC this phase (T-05-14)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Ready for 05-06 / 05-07 UI against IdP-trust + clear helper
- AUTH-04 SSO edge complete; email-change callers can invoke `clear_email_verification` later

---
*Phase: 05-cloud-verify-reset*
*Completed: 2026-09-10*

## Self-Check: PASSED
