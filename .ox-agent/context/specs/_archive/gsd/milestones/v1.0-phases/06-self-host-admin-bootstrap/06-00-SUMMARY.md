---
phase: 06-self-host-admin-bootstrap
plan: "00"
subsystem: testing
tags: [wave0, nyquist, auth-06, auth-07, nextest, vitest, bootstrap]

requires:
  - phase: 05-cloud-verify-reset
    provides: auth signup/session integration harness + Role-aware user APIs
provides:
  - "Wave 0 RED API stubs (auth_bootstrap, auth_forced_credentials, seeded_admin extensions)"
  - "Wave 0 RED web stubs (setup, setup.credentials, dashboard, index SSR tree, chrome)"
  - "Wave 0 dialect_auth stub for 0006_bootstrap_flags columns"
affects:
  - 06-01 schema/DTO
  - 06-02 ENV seed + confirm credentials
  - 06-03 wizard + RPC allowlist
  - 06-04 closed signup
  - 06-05 root SSR gate
  - 06-06 setup UI
  - 06-08 chrome omit
  - 06-09 dashboard notFound

actuals:
  tokens: 8177
  tasks: 2
  commits: 2

plan_head_before: 96ffea11a75579746af48e36f2c264de823f6c07

tech-stack:
  added: []
  patterns:
    - "Wave 0 intentional RED stubs with compile-safe placeholders until columns/RPCs land"
    - "tests/support ENV mutex without coupling to production bootstrap lock"
    - "Vitest integration stubs avoid static-import of not-yet-created route modules"

key-files:
  created:
    - crates/oxidean-api/tests/auth_bootstrap.rs
    - crates/oxidean-api/tests/auth_forced_credentials.rs
    - crates/oxidean-api/tests/support/mod.rs
    - apps/web/src/routes/setup.integration.test.ts
    - apps/web/src/routes/setup.credentials.integration.test.ts
    - apps/web/src/routes/dashboard.integration.test.ts
    - apps/web/src/routes/index.integration.test.ts
    - apps/web/src/components/chrome.integration.test.ts
  modified:
    - crates/oxidean-api/tests/auth_signup.rs
    - crates/oxidean-db/tests/dialect_auth.rs

key-decisions:
  - "Wave 0 is RED-only — no GREEN/REFACTOR; later 06-xx plans turn stubs green"
  - "support::lock_admin_env owns the ENV mutex (does not import untracked bootstrap.rs)"
  - "setup.credentials stub documents UI-SPEC without static-importing the missing route module (Vite transform)"
  - "Web stubs target .tsrx in-flight rename; SUMMARY notes .tsrx as the landed extension for route modules under test"

patterns-established:
  - "Nyquist Wave 0: failing nextest/vitest paths exist before implementation waves"
  - "Compile-safe Option<bool>/placeholder asserts for columns not yet on DTOs"

requirements-completed: []

coverage:
  - id: D1
    description: "API Wave 0 stubs for partial ENV, seed username, allowlist, allow_signup, confirm credentials"
    requirement: AUTH-06
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(bootstrap) | test(seeded_admin) | test(forced_credentials) | test(confirm_admin)'"
        status: fail
    human_judgment: false
  - id: D2
    description: "Web + dialect Wave 0 stubs for setup Switch, credentials, dashboard notFound, index SSR tree, chrome omit, 0006 columns"
    requirement: AUTH-07
    verification:
      - kind: integration
        ref: "bunx vitest run --project integration src/routes/setup.integration.test.ts src/routes/setup.credentials.integration.test.ts src/routes/dashboard.integration.test.ts src/routes/index.integration.test.ts src/components/chrome.integration.test.ts"
        status: fail
      - kind: integration
        ref: "cargo nextest run -p oxidean-db -E 'test(0006_bootstrap)'"
        status: fail
    human_judgment: false

duration: 8min
completed: 2026-09-11
status: complete
---

# Phase 06 Plan 00: Wave 0 Nyquist Stubs Summary

**Failing AUTH-06/07 nextest + vitest stubs for bootstrap, ENV seed, confirm credentials, setup UI, and 0006 columns — RED until later Phase 06 plans**

## Performance

- **Duration:** 8 min
- **Started:** 2026-09-11T20:14:04Z
- **Completed:** 2026-09-11T20:22:12Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments

- API Wave 0 stubs cover partial ENV (D-13), strict RPC allowlist (D-11), `setup_unavailable`, closed/`allow_signup` persistence, `system-administrator` seed expectations, and `auth.confirm_admin_credentials`
- Web Wave 0 stubs cover `/setup` Switch+CTA, `/setup/credentials` UI-SPEC copy, `/dashboard` notFound, index SSR tree priority (D-18/D-20), and chrome omit Sign up
- `dialect_auth` asserts `0006_bootstrap_flags` migration presence + `allow_signup` / `must_change_credentials`

## Task Commits

1. **Task 1: API Wave 0 stubs** - `7812c22` (test)
2. **Task 2: Web + dialect Wave 0 stubs** - `d0213cf` (test)

_Note: Wave 0 TDD is RED-only by design — GREEN/REFACTOR belong to later plans._

## Files Created/Modified

- `crates/oxidean-api/tests/auth_bootstrap.rs` — bootstrap/allowlist/allow_signup RED cases
- `crates/oxidean-api/tests/auth_forced_credentials.rs` — confirm credentials RED cases
- `crates/oxidean-api/tests/auth_signup.rs` — seeded_admin username / must_change / OXIDEAN_ALLOW_SIGNUP expectations
- `crates/oxidean-api/tests/support/mod.rs` — ENV mutex + unlock_signup helpers
- `crates/oxidean-db/tests/dialect_auth.rs` — 0006 column Wave 0 stub
- `apps/web/src/routes/setup.integration.test.ts` — Allow open signup + Create system admin
- `apps/web/src/routes/setup.credentials.integration.test.ts` — Keep current password UI-SPEC stub
- `apps/web/src/routes/dashboard.integration.test.ts` — notFound contract
- `apps/web/src/routes/index.integration.test.ts` — needs_setup / SignedInHome / marketing tree
- `apps/web/src/components/chrome.integration.test.ts` — omit Sign up when allow_signup false/unknown

## Decisions Made

- Kept Wave 0 RED-only (no production implementation in 06-00)
- ENV lock lives in `tests/support` so stubs do not depend on untracked `bootstrap.rs`
- `setup.credentials` stub avoids static import of a missing route (Vite would fail transform)
- Route modules under WIP rename use `.tsrx`; stubs import `./setup`, `./dashboard`, `./index`, `./chrome`

## TDD Gate Compliance

- **RED:** Present — `test(06-00):` commits `7812c22`, `d0213cf`; nextest/vitest filters exit nonzero on planned assertions
- **GREEN:** N/A — Wave 0 scaffold plan; implementation waves turn stubs green
- **REFACTOR:** N/A

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Avoid Vite static resolve of missing `setup.credentials`**
- **Found during:** Task 2 verify
- **Issue:** `await import("./setup.credentials")` failed the whole file at transform time (0 tests) when the route module does not exist
- **Fix:** Document UI-SPEC via intentional `expect(false).toBe(true)` without importing the missing module
- **Files modified:** `apps/web/src/routes/setup.credentials.integration.test.ts`
- **Verification:** vitest discovers the stub and fails on the assertion
- **Committed in:** `d0213cf`

**2. [Rule 2 - Missing Critical] ENV mutex in support without bootstrap import**
- **Found during:** Task 1
- **Issue:** WIP `support` called `bootstrap::lock_admin_env_for_tests`, coupling Wave 0 commits to untracked production bootstrap
- **Fix:** Own the mutex inside `tests/support/mod.rs`
- **Files modified:** `crates/oxidean-api/tests/support/mod.rs`
- **Verification:** nextest compiles and runs Wave 0 filter
- **Committed in:** `7812c22`

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 missing critical)
**Impact on plan:** Necessary for runnable RED stubs without pulling production WIP into the Wave 0 commit set.

## Issues Encountered

- Working tree has extensive untracked Phase 06 WIP (`.tsrx`, `bootstrap.rs`); only plan test files were staged
- `bun --cwd apps/web run test:unit` excludes `*.integration.test.ts` — verify used `bunx vitest run --project integration …`

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Wave 0 paths exist for VALIDATION checklist tick-as-file-present
- 06-01+ should turn dialect/API/web stubs green without renaming test paths
- AUTH-06/AUTH-07 remain open in REQUIREMENTS until implementation plans complete

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/tests/auth_bootstrap.rs`
- FOUND: `crates/oxidean-api/tests/auth_forced_credentials.rs`
- FOUND: `apps/web/src/routes/setup.integration.test.ts`
- FOUND: `apps/web/src/routes/setup.credentials.integration.test.ts`
- FOUND: `apps/web/src/routes/dashboard.integration.test.ts`
- FOUND: `apps/web/src/routes/index.integration.test.ts`
- FOUND: `apps/web/src/components/chrome.integration.test.ts`
- FOUND: commit `7812c22`
- FOUND: commit `d0213cf`

---
*Phase: 06-self-host-admin-bootstrap*
*Completed: 2026-09-11*
