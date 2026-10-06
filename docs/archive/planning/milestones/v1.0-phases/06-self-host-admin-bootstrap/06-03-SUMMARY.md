---
phase: 06-self-host-admin-bootstrap
plan: "03"
subsystem: auth
tags: [auth-07, bootstrap-wizard, allow_signup, rpc-allowlist, needs_setup, setup_required]

requires:
  - phase: 06-self-host-admin-bootstrap
    provides: ENV seed + confirm_admin_credentials + allow_signup DTOs/columns
provides:
  - "Wizard bootstrap_setup persists allow_signup (D-08) and issues session"
  - "auth.signup_closed when allow_signup false after bootstrap"
  - "Strict RPC allowlist while needs_setup (D-11)"
affects:
  - 06-04 allow_signup UI /signup 404
  - 06-05 SSR needs_setup gate
  - 06-08 /setup wizard UI Switch

actuals:
  tokens: 2634
  tasks: 2
  commits: 8

plan_head_before: 289d097f8053daff08978d4741b08afe5ee8bd0c

tech-stack:
  added: []
  patterns:
    - "Early needs_setup allowlist in rpc::dispatch before procedure match"
    - "Wizard allow_signup write mirrors ENV seed update_auth_settings preserve-other-fields"

key-files:
  created: []
  modified:
    - crates/oxidean-api/src/auth/bootstrap.rs
    - crates/oxidean-api/src/auth/local.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/tests/auth_bootstrap.rs
    - crates/oxidean-api/tests/support/mod.rs

key-decisions:
  - "allow_signup false persistence proven by pre-opening settings then wizard close"
  - "SSO reject_if_setup_required already present — no auth_callbacks change required"

patterns-established:
  - "auth.setup_required for empty-instance RPC lock; auth.signup_closed for post-bootstrap closed registration"
  - "unlock_signup helper creates sys-admin AND sets allow_signup=true for AUTH-01 tests"

requirements-completed: [AUTH-07]

coverage:
  - id: D1
    description: "Wizard bootstrap_setup creates sys-admin with allow_signup persistence and idempotent second reject"
    requirement: AUTH-07
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(bootstrap_allow_signup) | test(bootstrap_second)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Strict RPC allowlist while needs_setup (bootstrap_* + system.health only)"
    requirement: AUTH-07
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(allowlist)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "SSO starts reject via reject_if_setup_required while needs_setup"
    requirement: AUTH-07
    verification:
      - kind: other
        ref: "rg reject_if_setup_required crates/oxidean-api/src/routes/auth_callbacks.rs"
        status: pass
    human_judgment: false

duration: 3min
completed: 2026-09-11
status: complete
---

# Phase 06 Plan 03: AUTH-07 Wizard + RPC Allowlist Summary

**Empty-instance wizard persists `allow_signup`, closes signup when false, and locks RPC to bootstrap_* + health while `needs_setup`.**

## Performance

- **Duration:** 3 min
- **Started:** 2026-09-11T20:43:57Z
- **Completed:** 2026-09-11T20:47:00Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- `bootstrap_setup` writes `allow_signup` to `instance_auth_settings` (D-08); wizard admins stay `must_change_credentials=false` (D-17)
- `auth.signup` returns `auth.signup_closed` when registration is closed
- `rpc::dispatch` early allowlist: only `auth.bootstrap_status`, `auth.bootstrap_setup`, `system.health` while `needs_setup` (D-11)
- SSO `reject_if_setup_required` already on WorkOS/OIDC starts + mint path (verified, unchanged)

## Task Commits

1. **Task 1 RED: wizard allow_signup assertions** — `06215a6` (test)
2. **Task 1 GREEN: persist allow_signup + signup_closed** — `897dc23` (feat)
3. **Task 2 RED: expand allowlist coverage** — `06c9ac8` (test)
4. **Task 2 GREEN: RPC needs_setup allowlist** — `9c502ac` (feat)

**Plan metadata:** `0529bb0` (docs: complete plan)

## Decisions Made

- Pre-open `allow_signup` in the false-persistence test so default-false cannot mask a missing write
- Left `auth_callbacks.rs` unchanged — `reject_if_setup_required` already satisfies D-09/D-10

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] unlock_signup left allow_signup false**
- **Found during:** Task 1 GREEN
- **Issue:** Enforcing `auth.signup_closed` broke AUTH-01 signup tests that only seed a sys-admin via `unlock_signup`
- **Fix:** Helper now also sets `allow_signup=true` after ensuring a sys-admin exists
- **Files modified:** `crates/oxidean-api/tests/support/mod.rs`
- **Commit:** `897dc23`

**2. [Rule 3 - Blocking] Task 1 plan verify includes allowlist RED**
- **Found during:** Task 1
- **Issue:** Plan `<verify>` `test(bootstrap)` includes D-11 allowlist owned by Task 2
- **Fix:** Task 1 verified with `test(bootstrap) & !test(allowlist)`; full filter green after Task 2
- **Files modified:** none (verify scoping only)
- **Commit:** n/a

## TDD Gate Compliance

- RED → GREEN commits present for both tasks (`test(06-03)` then `feat(06-03)`)
- `workflow.tdd_mode` was false; cargo nextest output is not TAP — RED evidence checker (node TAP schema) not used; failures were intentional assertion misses on target tests

## Auth Gates

None.

## Known Stubs

None — Wave 0 allow_signup / allowlist stubs greened.

## Threat Flags

None beyond plan threat model (T-06-06 allowlist, T-06-07 race re-check retained, T-06-08 SSO reject retained).

## Self-Check: PENDING
