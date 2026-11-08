---
phase: 07-git-repos-browse
plan: "16"
subsystem: testing
tags: [wave0, nyquist, vitest, git-01, new-route, signed-in-home]

requires:
  - phase: 06-self-host-admin-bootstrap
    provides: Phase 06 vitest integration stub patterns + SignedInHome Phase 5 CTA stub
provides:
  - "Wave 0 RED /new verify-wall integration stub (D-11)"
  - "Wave 0 RED SignedInHome verified CTA → /new stub (D-01)"
affects:
  - 07-13 /new create tracer greening
  - 07-04 SignedInHome list + CTA wiring

actuals:
  tokens: 1262
  tasks: 1
  commits: 1

plan_head_before: 6f50ddbfcbdd917e5507013223c6616d1af8600a

tech-stack:
  added: []
  patterns:
    - "Wave 0 web stubs use @vite-ignore dynamic import so suites load while route modules are absent"
    - "SignedInHome Wave 0 expects verified Link to /new; unverified stays disabled button + hint"

key-files:
  created:
    - apps/web/src/routes/new.integration.test.ts
  modified:
    - apps/web/src/components/signed-in-home.integration.test.ts

key-decisions:
  - "Wave 0 is RED-only — no new.tsrx or SignedInHome production CTA wiring (07-13 / 07-04)"
  - "new.integration.test uses vite-ignore dynamic import so vitest discovers runnable tests instead of transform-failing the suite"

patterns-established:
  - "Nyquist Wave 0 web: failing vitest paths for /new wall + home CTA before UI waves"
  - "Unverified /new asserts UI-SPEC copy: Verify your email / body / Verify email → /verify"

requirements-completed: [GIT-01]

coverage:
  - id: D1
    description: "Wave 0 /new unverified verify-wall stub discoverable and RED until new.tsrx"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "bunx vitest run src/routes/new.integration.test.ts#unverified session shows Verify your email wall"
        status: pass
    human_judgment: false
  - id: D2
    description: "Wave 0 SignedInHome verified New repository → /new stub RED; unverified disabled+hint still green"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "bunx vitest run src/components/signed-in-home.integration.test.ts#verified: enabled New repository navigates to /new"
        status: pass
    human_judgment: false

duration: 2min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 16: Web Wave 0 /new + Home CTA Stubs Summary

**Vitest Wave 0 RED stubs for unverified `/new` verify wall and SignedInHome New repository → `/new` (D-01, D-11)**

## Performance

- **Duration:** 2 min
- **Started:** 2026-09-12T16:48:45Z
- **Completed:** 2026-09-12T16:50:35Z
- **Tasks:** 1
- **Files modified:** 2

## Accomplishments

- Added `new.integration.test.ts` asserting UI-SPEC unverified wall copy (`Verify your email`, body, `Verify email` → `/verify`) without create form
- Updated `signed-in-home.integration.test.ts` so verified users expect an enabled **New repository** link to `/new`; unverified disabled CTA + hint retained
- Vitest discovers both files; run exits non-zero (Wave 0 RED) until 07-13 / CTA wiring

## Task Commits

Each task was committed atomically:

1. **Task 1: Web Wave 0 stubs for /new and home CTA** - `57950ef` (test)

_Note: Wave 0 is RED-only by design — GREEN belongs to 07-13 (and related home CTA plans)._

## Files Created/Modified

- `apps/web/src/routes/new.integration.test.ts` — D-11 verify-wall Wave 0 stub (dynamic import until `new.tsrx` exists)
- `apps/web/src/components/signed-in-home.integration.test.ts` — D-01/D-11 CTA enablement stub (verified → `/new`)

## Decisions Made

- Kept Wave 0 RED-only; did not add production `/new` or enable the home CTA
- Used `@vite-ignore` + runtime import id so the `/new` suite loads and fails on assertion while the route module is missing (avoids Vite transform suite-load failure)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Vite static analysis broke Wave 0 suite load**
- **Found during:** Task 1 (verify)
- **Issue:** Static `import("./new")` made Vite fail transform when `new.tsrx` is absent → 0 tests in suite
- **Fix:** Dynamic module id + `/* @vite-ignore */` so the test runs and fails with a clear “route must exist” assertion
- **Files modified:** `apps/web/src/routes/new.integration.test.ts`
- **Verification:** vitest reports 2 failed / 1 passed; both stub files discovered
- **Committed in:** `57950ef` (Task 1)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary for runnable RED stubs discoverable by vitest; no scope creep.

## Issues Encountered

- Plan verify command `bun --cwd apps/web exec vitest run …` fails (`Script not found "exec"`); used `bunx vitest run` from `apps/web` instead (same vitest filter)

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Web Wave 0 stubs ready for 07-13 to green `/new` wall + CTA navigation
- 07-04 may further expand SignedInHome list surfaces against the same integration file
- Ready for remaining Phase 07 plans

## Self-Check: PASSED

- FOUND: `apps/web/src/routes/new.integration.test.ts`
- FOUND: `apps/web/src/components/signed-in-home.integration.test.ts`
- FOUND: commit `57950ef`

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*
