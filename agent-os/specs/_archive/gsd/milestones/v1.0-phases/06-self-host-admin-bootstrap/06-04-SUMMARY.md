---
phase: 06-self-host-admin-bootstrap
plan: "04"
subsystem: auth
tags: [auth-05, allow_signup, provider_config, signup_closed, admin-auth, rpc-gen]

requires:
  - phase: 06-self-host-admin-bootstrap
    provides: allow_signup column/DTOs + signup gate + wizard persistence from 06-01…06-03
provides:
  - "auth.signup contract tests for closed/open allow_signup + needs_setup"
  - "auth.provider_config.allow_signup public chrome contract"
  - "admin.auth get/update allow_signup round-trip coverage"
  - "rpc_gen + api-client DTOs include allow_signup"
affects:
  - 06-05 SSR /signup 404 chrome
  - 06-08 admin auth UI Switch
  - 06-07 docs OXIDEAN_ALLOW_SIGNUP

actuals:
  tokens: 3820
  tasks: 2
  commits: 6

plan_head_before: 532728bf4af33ef41700949befc5eb37e55c747a

tech-stack:
  added: []
  patterns:
    - "Public allow_signup via auth.provider_config; closed registration via auth.signup_closed"
    - "Admin allow_signup threaded only through require_admin settings path"

key-files:
  created: []
  modified:
    - crates/oxidean-api/tests/auth_signup.rs
    - crates/oxidean-api/tests/admin_auth_settings.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - crates/oxidean-api/src/auth/admin.rs
    - packages/api-client/src/index.ts

key-decisions:
  - "Signup/provider_config enforcement already landed in 06-03 — 06-04 locks RPC contracts + rpc_gen DTOs"
  - "provider_config tested post-bootstrap (needs_setup allowlist blocks it on empty instance)"

patterns-established:
  - "auth.signup_closed for post-bootstrap closed registration; auth.setup_required while empty"
  - "make rpc-gen regenerates packages/api-client allow_signup fields from rpc_gen stubs"

requirements-completed: [AUTH-06, AUTH-07]

coverage:
  - id: D1
    description: "auth.signup rejects when allow_signup false; succeeds when true; needs_setup still blocks"
    requirement: AUTH-06
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(signup_rejects_when_allow_signup_false) | test(signup_succeeds_when_allow_signup_true) | test(signup_setup_required_before_bootstrap)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "auth.provider_config exposes allow_signup for public chrome"
    requirement: AUTH-06
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(provider_config_includes_allow_signup)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "admin.auth get/update_settings round-trip allow_signup for sys-admin; non-admin forbidden"
    requirement: AUTH-07
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'binary(admin_auth_settings)'"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-09-11
status: complete
---

# Phase 06 Plan 04: allow_signup RPC/admin/provider_config Summary

**Server-side closed signup enforced with public provider_config + admin settings round-trip and typed client DTOs.**

## Performance

- **Duration:** 5 min
- **Started:** 2026-09-11T20:49:58Z
- **Completed:** 2026-09-11T20:54:32Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Locked `auth.signup` closed/open + `needs_setup` contracts in `auth_signup` nextest
- Confirmed `auth.provider_config.allow_signup` for chrome (fail closed by default)
- Sys-admin `admin.auth` get/update round-trips `allow_signup`; regenerated rpc_gen / api-client DTOs

## Task Commits

1. **Task 1: provider_config.allow_signup + signup reject when closed** - `51977aa` (test)
2. **Task 2: admin.auth settings allow_signup round-trip** - `96c4640` (test), `f2e0a46` (feat)

**Plan metadata:** `107bd59` (docs: complete plan)

_Note: Signup gate + provider_config + admin persistence already implemented in 06-03 / 06-01; this plan added contract tests and TS DTO surface._

## Files Created/Modified

- `crates/oxidean-api/tests/auth_signup.rs` — closed/open signup, provider_config, setup_required cases
- `crates/oxidean-api/tests/admin_auth_settings.rs` — allow_signup get/update round-trip
- `crates/oxidean-api/src/bin/rpc_gen.rs` — allow_signup on ProviderConfigPublic / AuthSettings* DTOs
- `packages/api-client/src/index.ts` — regenerated from rpc_gen
- `crates/oxidean-api/src/auth/admin.rs` — unit assert allow_signup serialization

## Decisions Made

- Enforcement code from 06-03 was sufficient; 06-04 focuses on RPC contract tests + client types
- `provider_config` assertions run after creating a sys-admin so needs_setup allowlist does not mask the flag

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] Plan verify filter `test(admin_auth)` skips admin_auth_settings binary**
- **Found during:** Task 2 verification
- **Issue:** nextest `test(admin_auth)` matches test *names*, not the `admin_auth_settings` binary; would miss round-trip coverage
- **Fix:** Verified with `binary(admin_auth_settings) | test(signup)` (and explicit `admin_get_and_update` / `non_admin_get_settings`)
- **Files modified:** none (verify command only)
- **Commit:** n/a

### TDD note

Implementation for signup gate / provider_config / admin persistence pre-existed (06-03). Intentional RED probes (temporarily disable gate / hardcode provider_config / ignore update allow_signup) confirmed new assertions fail for the planned behaviors, then restored production code for GREEN. `workflow.tdd_mode` is false; no formal `tdd-red-evidence` gate required.

## Threat Flags

None — no new trust boundaries beyond plan threat model (T-06-09/10 already mitigated).

## Self-Check: PASSED

- FOUND: contract tests, local.rs/admin.rs gates, rpc_gen + api-client allow_signup
- FOUND: commits 51977aa, 96c4640, f2e0a46
