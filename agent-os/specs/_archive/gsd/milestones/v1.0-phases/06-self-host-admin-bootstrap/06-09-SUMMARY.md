---
phase: 06-self-host-admin-bootstrap
plan: "09"
subsystem: auth
tags: [notFound, ssr, allow_signup, dashboard, signup, D-19, D-06, AUTH-06, AUTH-07]

requires:
  - phase: 06-self-host-admin-bootstrap
    provides: shared root SSR app-access gate + fetchProviderConfig from 06-05
provides:
  - "/dashboard hard 404 via beforeLoad throw notFound() (D-19)"
  - "/signup SSR notFound when allow_signup false (D-06)"
  - "dashboard.integration + signup.integration beforeLoad green"
affects:
  - 06-08 closed-signup chrome (omit Sign up links)
  - phase UAT for dashboard/signup 404 paths

actuals:
  tokens: 3890
  tasks: 2
  commits: 4

plan_head_before: e89c1783851a3e5faef93bf8d457c99877331da1

tech-stack:
  added: []
  patterns:
    - "Route hard-404: beforeLoad throw notFound() from @octanejs/tanstack-router"
    - "Closed signup SSR gate: fetchProviderConfig.allow_signup === false → notFound"

key-files:
  created:
    - apps/web/src/routes/dashboard.tsrx
    - apps/web/src/routes/signup.tsrx
    - apps/web/src/routes/signup.integration.test.ts
    - .planning/phases/06-self-host-admin-bootstrap/06-09-tdd-red-evidence-t1.json
    - .planning/phases/06-self-host-admin-bootstrap/06-09-tdd-red-evidence-t2.json
  modified:
    - apps/web/src/routes/dashboard.integration.test.ts

key-decisions:
  - "Land dashboard/signup on .tsrx (Octane rename in flight) — plan .tsx paths reconciled"
  - "throw notFound() (not soft redirect / AuthShell) for /dashboard and closed /signup"
  - "Signup API-unreachable: do not 404; API allow_signup remains authority"

patterns-established:
  - "Hard route 404: beforeLoad throws notFound(); component never soft-redirects"
  - "Closed-signup SSR: fetchProviderConfig → allow_signup false → notFound"

requirements-completed: [AUTH-06, AUTH-07]

coverage:
  - id: D1
    description: "/dashboard direct hit → notFound hard 404 (D-19)"
    requirement: AUTH-07
    verification:
      - kind: integration
        ref: "apps/web/src/routes/dashboard.integration.test.ts#resolves to notFound instead of soft-redirect"
        status: pass
    human_judgment: false
  - id: D2
    description: "/signup allow_signup false → SSR notFound (D-06)"
    requirement: AUTH-06
    verification:
      - kind: integration
        ref: "apps/web/src/routes/signup.integration.test.ts#beforeLoad calls notFound when allow_signup is false"
        status: pass
    human_judgment: false
  - id: D3
    description: "Web production build includes dashboard + signup route modules"
    requirement: AUTH-07
    verification:
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
    human_judgment: false

duration: 7min
completed: 2026-09-11
status: complete
---

# Phase 06 Plan 09: Dashboard notFound + Closed Signup SSR Gate Summary

**/dashboard hard-404s via `notFound()` and closed `/signup` SSR-404s when `allow_signup` is false — no soft redirects.**

## Performance

- **Duration:** 7 min
- **Started:** 2026-09-11T21:14:40Z
- **Completed:** 2026-09-11T21:20:49Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- Replaced `/dashboard` soft `window.location.replace("/")` with `beforeLoad` `throw notFound()` (D-19)
- Added `/signup` SSR `beforeLoad` using `fetchProviderConfig`; `allow_signup === false` → `notFound()` (D-06)
- Greened Wave 0 dashboard integration test and new signup closed-signup beforeLoad tests

## Task Commits

Each task was committed atomically (TDD RED → GREEN):

1. **Task 1 RED:** strengthen dashboard notFound assertions — `925153f` (test)
2. **Task 1 GREEN:** hard-404 `/dashboard` via notFound beforeLoad — `def34fa` (feat)
3. **Task 2 RED:** failing signup closed-signup beforeLoad assertions — `85457e7` (test)
4. **Task 2 GREEN:** SSR-404 closed `/signup` via allow_signup beforeLoad — `325ea3b` (feat)

**Plan metadata:** `7a7b1a4` (docs: complete plan)

## TDD Gate Compliance

| Task | RED | GREEN | REFACTOR | Status |
|------|-----|-------|----------|--------|
| 06-09-T1 dashboard | ✓ `925153f` + RED_EVIDENCE_OK | ✓ `def34fa` | — | Pass |
| 06-09-T2 signup | ✓ `85457e7` + RED_EVIDENCE_OK | ✓ `325ea3b` | — | Pass |

## Files Created/Modified

- `apps/web/src/routes/dashboard.tsrx` — hard 404 beforeLoad; no soft redirect
- `apps/web/src/routes/dashboard.integration.test.ts` — asserts beforeLoad throws `isNotFound`
- `apps/web/src/routes/signup.tsrx` — SSR allow_signup gate + existing SignupPage UI
- `apps/web/src/routes/signup.integration.test.ts` — closed/open beforeLoad + AUTH-05 invite absence
- `06-09-tdd-red-evidence-t1.json` / `t2.json` — RED gate evidence

## Decisions Made

- Reconciled plan `.tsx` paths to tracked `.tsrx` (same as 06-05/06-06)
- Used `throw notFound()` to match existing `throw redirect()` style
- On provider_config fetch failure, signup beforeLoad does not 404 (API remains authority)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Landed routes on `.tsrx` instead of restoring deleted `.tsx`**
- **Found during:** Task 1 / Task 2
- **Issue:** Plan listed `dashboard.tsx` / `signup.tsx`; workspace uses Octane `.tsrx` rename (`.tsx` deleted untracked)
- **Fix:** Edited/created `.tsrx` modules; tests import `./dashboard` / `./signup` as before
- **Files modified:** `apps/web/src/routes/dashboard.tsrx`, `apps/web/src/routes/signup.tsrx`
- **Verification:** vitest + `bun run build` green
- **Committed in:** `def34fa`, `325ea3b`

---

**Total deviations:** 1 auto-fixed (blocking path reconcile)
**Impact on plan:** Required for correct source tree; no scope creep.

## Issues Encountered

None beyond `.tsrx` reconcile (documented above).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- D-19 and D-06 route-level outcomes complete on top of 06-05 root gate
- 06-08 still owns logged-out chrome omit of Sign up links when closed
- Ready for remaining Phase 06 plans / UAT of 404 paths

## Self-Check: PASSED

- FOUND: `apps/web/src/routes/dashboard.tsrx`
- FOUND: `apps/web/src/routes/dashboard.integration.test.ts`
- FOUND: `apps/web/src/routes/signup.tsrx`
- FOUND: `apps/web/src/routes/signup.integration.test.ts`
- FOUND: commits `925153f`, `def34fa`, `85457e7`, `325ea3b`

---
*Phase: 06-self-host-admin-bootstrap*
*Completed: 2026-09-11*
