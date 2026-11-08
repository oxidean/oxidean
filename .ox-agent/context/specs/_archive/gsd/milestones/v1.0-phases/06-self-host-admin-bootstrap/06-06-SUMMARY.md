---
phase: 06-self-host-admin-bootstrap
plan: "06"
subsystem: auth
tags: [setup-wizard, credentials, switch, allow_signup, AUTH-06, AUTH-07, D-08, D-16, D-17, D-22]

requires:
  - phase: 06-self-host-admin-bootstrap
    provides: SSR root gate + bootstrap/confirm RPCs from 06-01…06-05
provides:
  - "Base UI Switch wrapper (shared by 06-08 admin)"
  - "/setup wizard with Allow open signup Switch (D-08)"
  - "/setup/credentials forced change UI (D-16/D-17)"
  - "Login must_change → credentials + safeReturnTo (D-22)"
affects:
  - 06-08 admin allow_signup Switch reuse
  - 06-07/06-09 closed-signup chrome

actuals:
  tokens: 9307
  tasks: 2
  commits: 3

plan_head_before: aad278566d3f4208ccd5a7a8bc367c244863f587

tech-stack:
  added: []
  patterns:
    - "Hand-authored @octanejs/base-ui Switch (data-slot=switch; ≥44px hit)"
    - "/setup layout Outlet + setup.index wizard + nested setup.credentials"

key-files:
  created:
    - apps/web/src/components/ui/switch.tsrx
    - apps/web/src/routes/setup.index.tsrx
    - apps/web/src/routes/setup.credentials.tsrx
    - apps/web/src/routes/login.tsrx
    - .planning/phases/06-self-host-admin-bootstrap/06-06-tdd-red-evidence.json
  modified:
    - apps/web/src/routes/setup.tsrx
    - apps/web/src/routes/setup.credentials.integration.test.ts
    - apps/web/src/routeTree.gen.ts
    - packages/api-client/src/index.ts
    - crates/oxidean-api/src/bin/rpc_gen.rs

key-decisions:
  - "Land Switch/setup/credentials/login on .tsrx (Octane rename in flight)"
  - "Split /setup into layout Outlet so /setup/credentials nests without blank child"
  - "confirmAdminCredentials + BootstrapSetupRequest.allow_signup added to api-client (Rule 2)"

patterns-established:
  - "Auth Switch: Label + Switch in min-h-11 row; helper Label under"
  - "Forced-change: client reject system-administrator before RPC; Keep password default on"

requirements-completed: [AUTH-06, AUTH-07]

coverage:
  - id: D1
    description: "/setup wizard Allow open signup Switch + Create system admin CTA"
    requirement: AUTH-07
    verification:
      - kind: integration
        ref: "apps/web/src/routes/setup.integration.test.ts"
        status: pass
    human_judgment: false
  - id: D2
    description: "/setup/credentials Keep current password + reject default username"
    requirement: AUTH-06
    verification:
      - kind: integration
        ref: "apps/web/src/routes/setup.credentials.integration.test.ts"
        status: pass
    human_judgment: false
  - id: D3
    description: "Web production build includes setup + credentials routes"
    requirement: AUTH-07
    verification:
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
    human_judgment: false

duration: 8min
completed: 2026-09-11
status: complete
---

# Phase 06 Plan 06: Switch + `/setup` wizard + `/setup/credentials` Summary

**Hand-authored Switch primitive, UI-SPEC `/setup` wizard with allow_signup, and ENV forced-credential change at `/setup/credentials` with login returnTo (D-22).**

## Performance

- **Duration:** 8 min
- **Started:** 2026-09-11T21:07:19Z
- **Completed:** 2026-09-11T21:14:55Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments

- Switch UI wrapper from `@octanejs/base-ui/switch` (primary checked track, ≥44px hit)
- `/setup` AuthShell wizard: field order, Allow open signup Switch default off, SSR `!needs_setup` → `/`
- `/setup/credentials` Confirm admin account + Keep current password; client blocks `system-administrator`
- Login success / already-signed-in with `must_change_credentials` → credentials + safe `returnTo`

## Task Commits

1. **Task 1: Switch UI + /setup wizard** - `8e97808` (feat)
2. **Task 2 RED: credentials UI-SPEC assertions** - `e43f153` (test)
3. **Task 2 GREEN: credentials UI + login must_change** - `4ba7398` (feat)

**Plan metadata:** `1263313` (docs: complete plan)

## Files Created/Modified

- `apps/web/src/components/ui/switch.tsrx` — Base UI Switch wrapper
- `apps/web/src/routes/setup.tsrx` — `/setup` layout Outlet (+ re-exports SetupPage)
- `apps/web/src/routes/setup.index.tsrx` — wizard UI + SSR beforeLoad
- `apps/web/src/routes/setup.credentials.tsrx` — forced credential change UI
- `apps/web/src/routes/login.tsrx` — must_change post-login redirect (`.tsx` → `.tsrx`)
- `packages/api-client` / `rpc_gen.rs` — `allow_signup` on bootstrap + `confirmAdminCredentials`

## Decisions Made

- Prefer `.tsrx` for new/edited route and UI modules (rename already in flight)
- Nest credentials under `/setup` via layout Outlet rather than `setup_.credentials` escape
- Ship missing client RPC/DTO fields with the UI (wizard/credentials cannot call without them)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] BootstrapSetupRequest.allow_signup + confirmAdminCredentials absent from TS client**
- **Found during:** Tasks 1–2
- **Issue:** Rust DTOs/RPCs existed; api-client/`rpc_gen` lacked fields the wizard/credentials UI must send
- **Fix:** Added `allow_signup?` on BootstrapSetupRequest and ConfirmAdminCredentialsRequest + client method
- **Files modified:** `packages/api-client/src/index.ts`, `crates/oxidean-api/src/bin/rpc_gen.rs`
- **Commit:** `8e97808`, `4ba7398`

**2. [Rule 3 - Blocking] Nested `/setup/credentials` blank without parent Outlet**
- **Found during:** Task 2 GREEN
- **Issue:** `setup.credentials.tsrx` nests under `/setup`; leaf wizard component had no `<Outlet />`
- **Fix:** Split layout (`setup.tsrx`) + wizard (`setup.index.tsrx`); re-export SetupPage for Wave 0 test
- **Files modified:** `apps/web/src/routes/setup.tsrx`, `apps/web/src/routes/setup.index.tsrx`
- **Commit:** `4ba7398`

**3. [Rule 3 - Blocking] login route is `.tsrx` while plan listed `.tsx`**
- **Found during:** Task 2
- **Issue:** In-flight Octane rename deleted tracked `login.tsx`
- **Fix:** Landed must_change redirect on `login.tsrx` and staged `.tsx` deletion (same pattern as 06-05)
- **Files modified:** `apps/web/src/routes/login.tsrx`
- **Commit:** `4ba7398`

## TDD Gate Compliance

| Task | RED | GREEN | Notes |
|------|-----|-------|-------|
| T2 credentials | `e43f153` | `4ba7398` | Assertion RED: missing "Confirm admin account". Vitest non-TAP → `tdd-red-evidence` INVALID_RED (`zero_tests_discovered`); evidence JSON retained for audit |

## Known Stubs

None.

## Threat Flags

None beyond plan threat model (safeReturnTo on confirm/login; no new npm packages).

## Self-Check: PASSED

- FOUND: `apps/web/src/components/ui/switch.tsrx`
- FOUND: `apps/web/src/routes/setup.tsrx`
- FOUND: `apps/web/src/routes/setup.index.tsrx`
- FOUND: `apps/web/src/routes/setup.credentials.tsrx`
- FOUND: `apps/web/src/routes/login.tsrx`
- FOUND commits: `8e97808`, `e43f153`, `4ba7398`
