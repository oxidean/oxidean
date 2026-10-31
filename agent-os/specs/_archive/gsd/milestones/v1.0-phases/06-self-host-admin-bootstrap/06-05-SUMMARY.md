---
phase: 06-self-host-admin-bootstrap
plan: "05"
subsystem: auth
tags: [ssr, cookie-forward, createServerFn, app-access-gate, signed-in-home, D-09, D-18]

requires:
  - phase: 06-self-host-admin-bootstrap
    provides: allow_signup + must_change_credentials DTOs/RPC from 06-01…06-04
provides:
  - "Cookie-forward createServerFn SSR auth helpers (bootstrap/me/provider_config)"
  - "pure resolveAppAccessRedirect path matrix (needs_setup + must_change)"
  - "shared root beforeLoad app-access gate before paint"
  - "SSR / session vs marketing tree after gate (D-18/D-20)"
affects:
  - 06-06 /setup/credentials UI (root already redirects here)
  - 06-09 dashboard notFound + signup closed chrome

actuals:
  tokens: 5956
  tasks: 3
  commits: 4

plan_head_before: 303caf765825a8110417245e18b3a82eda262fa5

tech-stack:
  added: []
  patterns:
    - "createServerFn + getRequestHeader Cookie forward to createClient fetch override"
    - "Shared root beforeLoad resolves access redirects; index loader only picks home tree"

key-files:
  created:
    - apps/web/src/lib/ssr-auth.ts
    - apps/web/src/lib/ssr-auth.gate.test.ts
    - apps/web/src/lib/bootstrap.ts
    - apps/web/src/routes/__root.tsrx
    - apps/web/src/routes/index.tsrx
  modified:
    - packages/api-client/src/index.ts
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - apps/web/src/routes/index.integration.test.ts
    - apps/web/vitest.config.ts

key-decisions:
  - "Land route edits on .tsrx (in-flight Octane rename) instead of restoring deleted .tsx"
  - "UserPublic.must_change_credentials added to api-client + rpc_gen for SSR gate"
  - "Client redirectIfNeedsSetup demoted to PE after shared root SSR gate landed"

patterns-established:
  - "SSR auth: createServerFn → Cookie header → createClient fetch override → Oxidean RPC"
  - "App locks: resolveAppAccessRedirect pure helper; root beforeLoad throws redirect({ href })"

requirements-completed: [AUTH-06, AUTH-07]

coverage:
  - id: D1
    description: "Cookie-forward SSR helpers + resolveAppAccessRedirect compile"
    requirement: AUTH-06
    verification:
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Shared root path matrix for needs_setup + must_change carve-outs"
    requirement: AUTH-06
    verification:
      - kind: unit
        ref: "apps/web/src/lib/ssr-auth.gate.test.ts"
        status: pass
    human_judgment: false
  - id: D3
    description: "/ SSR selects SignedInHome vs marketing after access gate (D-18/D-20)"
    requirement: AUTH-07
    verification:
      - kind: integration
        ref: "apps/web/src/routes/index.integration.test.ts"
        status: pass
    human_judgment: false

duration: 8min
completed: 2026-09-11
status: complete
---

# Phase 06 Plan 05: SSR Cookie-forward + shared root access gate + `/` tree Summary

**Cookie-forward `createServerFn` SSR helpers, shared root `beforeLoad` for needs_setup/must_change, and `/` loader that paints SignedInHome vs marketing without client flicker.**

## Performance

- **Duration:** 8 min
- **Started:** 2026-09-11T20:56:18Z
- **Completed:** 2026-09-11T21:04:11Z
- **Tasks:** 3
- **Files modified:** 9

## Accomplishments

- SSR auth helpers forward Cookie to Oxidean RPC (no Start `useSession`)
- Shared root gate redirects all app UI except `/status`, `/setup`, `/setup/credentials`
- `/` first HTML matches session vs marketing via loader + `selectHomeTree`

## Task Commits

1. **Task 1: ssr-auth Cookie-forward server fns** - `f4b92eb` (feat)
2. **Task 2: Shared root SSR app-access gate** - `0d35e07` (test) + `438130e` (feat)
3. **Task 3: `/` SSR session vs marketing tree** - `983bc81` (feat)

## Files Created/Modified

- `apps/web/src/lib/ssr-auth.ts` — Cookie-forward server fns + `resolveAppAccessRedirect`
- `apps/web/src/lib/ssr-auth.gate.test.ts` — path-matrix unit coverage
- `apps/web/src/routes/__root.tsrx` — shared SSR `beforeLoad` access gate
- `apps/web/src/routes/index.tsrx` — SSR home tree loader + `selectHomeTree`
- `apps/web/src/lib/bootstrap.ts` — PE-only `redirectIfNeedsSetup` (demoted)
- `packages/api-client/src/index.ts` / `rpc_gen.rs` — `must_change_credentials` on `UserPublic`
- `apps/web/vitest.config.ts` — discover `*.gate.test.ts` under unit project

## Decisions Made

- Prefer `.tsrx` route modules (rename already in flight) over restoring tracked `.tsx`
- Skip `auth.me` during `needs_setup` (priority + avoid setup allowlist blocks)
- Keep `oxidean_signed_in` presence hint as PE skeleton only (D-21)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] UserPublic.must_change_credentials missing from TS client**
- **Found during:** Task 1
- **Issue:** Rust DTO had the flag; api-client/`rpc_gen` UserPublic did not — SSR must_change gate could not type-check
- **Fix:** Added field to `packages/api-client` and `rpc_gen.rs`
- **Files modified:** `packages/api-client/src/index.ts`, `crates/oxidean-api/src/bin/rpc_gen.rs`
- **Commit:** `f4b92eb`

**2. [Rule 3 - Blocking] `*.gate.test.ts` not discovered by vitest projects**
- **Found during:** Task 2
- **Issue:** Unit project only included `*.unit.test.ts`
- **Fix:** Extended unit include with `*.gate.test.ts`
- **Files modified:** `apps/web/vitest.config.ts`
- **Commit:** `0d35e07`

**3. [Rule 3 - Blocking] Route sources are `.tsrx` not plan-tracked `.tsx`**
- **Found during:** Tasks 2–3
- **Issue:** Working tree rename deleted `.tsx`; live routes are `.tsrx`
- **Fix:** Landed `__root`/`index` on `.tsrx` and staged `.tsx` deletions for those two files only
- **Files modified:** `apps/web/src/routes/__root.tsrx`, `apps/web/src/routes/index.tsrx`
- **Commit:** `438130e`, `983bc81`

## TDD Gate Compliance

| Task | RED | GREEN | Notes |
|------|-----|-------|-------|
| T2 path matrix | N/A (helper shipped in T1) | `0d35e07` + `438130e` | Pure helper locked GREEN; root wiring is the feat |
| T3 home tree | Wave 0 stubs already failing | `983bc81` | Vitest TAP nested output not classifiable by `tdd-red-evidence`; assertion RED observed on Wave 0 stubs |

## Known Stubs

None — `/setup/credentials` route UI deferred to 06-06 by plan (root may redirect there).

## Threat Flags

None beyond plan threat model (Cookie forward + access gate mitigations applied).

## Self-Check: PASSED

- FOUND: `apps/web/src/lib/ssr-auth.ts`
- FOUND: `apps/web/src/lib/ssr-auth.gate.test.ts`
- FOUND: `apps/web/src/routes/__root.tsrx`
- FOUND: `apps/web/src/routes/index.tsrx`
- FOUND commits: `f4b92eb`, `0d35e07`, `438130e`, `983bc81`
