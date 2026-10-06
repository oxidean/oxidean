---
phase: 06-self-host-admin-bootstrap
plan: "08"
subsystem: auth
tags: [chrome, allow_signup, closed-signup, admin-auth, switch, D-06, D-08, AUTH-06, AUTH-07]

requires:
  - phase: 06-self-host-admin-bootstrap
    provides: Switch primitive (06-06) + signup SSR notFound (06-09) + provider_config/update_settings allow_signup
provides:
  - "Logged-out chrome/landing/login omit Sign up when allow_signup false/unknown"
  - "Header omits Sign in/Sign up while needs_setup"
  - "admin/auth Allow open signup Switch wired to update_settings"
affects:
  - 06-07 docs
  - phase UAT closed-signup UX

actuals:
  tokens: 9889
  tasks: 2
  commits: 4

plan_head_before: 105dbf61c3c1894ad5a92fc4c5e221b6bb081b8a

tech-stack:
  added: []
  patterns:
    - "Fail-closed allow_signup: only ===true shows Sign up / Get started / Create an account"
    - "Chrome AccountActions: parallel me + bootstrapStatus + providerConfig"

key-files:
  created:
    - apps/web/src/components/chrome.tsrx
    - apps/web/src/routes/admin/auth.tsrx
    - apps/web/src/routes/admin/auth.integration.test.ts
    - .planning/phases/06-self-host-admin-bootstrap/06-08-tdd-red-evidence-t1.json
    - .planning/phases/06-self-host-admin-bootstrap/06-08-tdd-red-evidence-t2.json
  modified:
    - apps/web/src/components/chrome.integration.test.ts
    - apps/web/src/routes/index.tsrx
    - apps/web/src/routes/login.tsrx

key-decisions:
  - "Land chrome/admin edits on .tsrx (Octane rename in flight) — plan .tsx paths reconciled"
  - "needs_setup omits account CTAs via bootstrapStatus; Sign up fail-closed until allow_signup===true"

patterns-established:
  - "Public chrome gates signup CTAs on providerConfig.allow_signup === true"
  - "Admin allow_signup Switch reuses wizard Label/helper + update_settings field"

requirements-completed: [AUTH-06, AUTH-07]

coverage:
  - id: D1
    description: "Header omits Sign up when allow_signup false/unknown; shows when true; omits Sign in/up while needs_setup"
    requirement: AUTH-06
    verification:
      - kind: integration
        ref: "apps/web/src/components/chrome.integration.test.ts"
        status: pass
    human_judgment: false
  - id: D2
    description: "Landing Get started→/signup and login Create an account omitted when closed"
    requirement: AUTH-06
    verification:
      - kind: other
        ref: "bun run build (apps/web) + source gate on index/login"
        status: pass
    human_judgment: false
  - id: D3
    description: "admin/auth Allow open signup Switch + helper persists via update_settings"
    requirement: AUTH-07
    verification:
      - kind: integration
        ref: "apps/web/src/routes/admin/auth.integration.test.ts#shows Allow open signup Switch with wizard helper copy"
        status: pass
    human_judgment: false

duration: 13min
completed: 2026-09-11
status: complete
---

# Phase 06 Plan 08: Closed-signup chrome omit + admin Switch Summary

**Fail-closed logged-out UX omits Sign up/Get started/Create an account; admin auth Switch toggles `allow_signup`.**

## Performance

- **Duration:** 13 min
- **Started:** 2026-09-11T21:22:27Z
- **Completed:** 2026-09-11T21:34:55Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- Chrome header/mobile gates Sign up on `allow_signup === true` and omits Sign in/Sign up while `needs_setup`
- Landing Get started→/signup and login Create an account omitted when closed; restored when open
- `/admin/auth` exposes Allow open signup Switch (wizard copy) persisted via `update_settings`

## Task Commits

Each task was committed atomically (TDD RED → GREEN):

1. **Task 1 RED:** `42fc483` — test(06-08): add failing chrome omit Sign up tests (D-06)
2. **Task 1 GREEN:** `4bf4c8c` — feat(06-08): omit closed-signup chrome, landing, and login CTAs
3. **Task 2 RED:** `d2c3c53` — test(06-08): add failing admin allow_signup Switch test (D-08)
4. **Task 2 GREEN:** `1091f6f` — feat(06-08): add admin Allow open signup Switch (D-08)

## Files Created/Modified

- `apps/web/src/components/chrome.tsrx` — AccountActions + mobile nav fail-closed signup / needs_setup gates
- `apps/web/src/components/chrome.integration.test.ts` — D-06 omit/open/needs_setup cases
- `apps/web/src/routes/index.tsrx` — AnonLanding Get started gated on allow_signup
- `apps/web/src/routes/login.tsrx` — Create an account cross-link gated on allow_signup
- `apps/web/src/routes/admin/auth.tsrx` — Allow open signup Switch + update_settings wiring
- `apps/web/src/routes/admin/auth.integration.test.ts` — D-08 Switch copy assertion
- `06-08-tdd-red-evidence-t1.json` / `t2.json` — RED gate records

## Decisions Made

- Prefer `.tsrx` over restoring deleted `.tsx` (Octane rename in flight; matches 06-06/06-09)
- Fail closed: Sign up only when `allow_signup === true`; unknown/missing → omit
- `needs_setup` uses `bootstrapStatus` so account CTAs stay hidden during empty-instance setup

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] Export AdminAuthPage for integration seam**
- **Found during:** Task 2 RED
- **Issue:** Plan listed only `admin/auth.tsx` with no test file; TDD required an observable seam
- **Fix:** Added `auth.integration.test.ts` and exported `AdminAuthPage` (same pattern as SetupPage)
- **Files modified:** `apps/web/src/routes/admin/auth.tsrx`, `apps/web/src/routes/admin/auth.integration.test.ts`
- **Commit:** `d2c3c53`

## TDD Gate Compliance

| Task | RED | GREEN | REFACTOR | Notes |
|------|-----|-------|----------|-------|
| T1 chrome omit | ✓ `42fc483` | ✓ `4bf4c8c` | — | `06-08-tdd-red-evidence-t1.json` RED_EVIDENCE_OK |
| T2 admin Switch | ✓ `d2c3c53` | ✓ `1091f6f` | — | `06-08-tdd-red-evidence-t2.json` RED_EVIDENCE_OK |

## Known Stubs

None.

## Threat Flags

None beyond plan threat model (T-06-13 mitigated by UI omit + existing API gate).

## Next

- Remaining incomplete plan: `06-07` (docs wave)
- Phase UAT: closed-signup chrome + admin Switch visual check

## Self-Check: PASSED

- All key files present (chrome/index/login/admin + tests + SUMMARY + RED evidence)
- Commits verified: 42fc483, 4bf4c8c, d2c3c53, 1091f6f
