---
phase: 04-auth-sessions-email
plan: "07"
subsystem: ui
tags: [auth-ui, shadcn, login, signup, dashboard, dropdown-menu, chrome]

requires:
  - phase: 04-auth-sessions-email
    provides: "auth.* RPC + api-client (04-04/04-06)"
  - phase: 03-brand-shell-theme
    provides: "SiteHeader chrome, Button/Input, design tokens"
provides:
  - "/login /signup /dashboard routes with mode-exclusive provider UI"
  - "Header Sign in/Sign up + signed-in account menu with this-device logout"
  - "Landing Get started → /signup"
  - "shadcn Label/Checkbox/Textarea/Dropdown Menu primitives"
affects:
  - 04-08-profile-admin-ui
  - AUTH-01
  - AUTH-02
  - AUTH-03

tech-stack:
  added: []
  patterns:
    - "Auth pages fetch auth.providerConfig and branch local|workos|oidc exclusively (D-16)"
    - "safeReturnTo rejects //, schemes, and homepage → /dashboard (D-15, T-04-23)"
    - "Error banners render plain text only (T-04-24); UI-SPEC copy verbatim"

key-files:
  created:
    - apps/web/src/routes/login.tsx
    - apps/web/src/routes/signup.tsx
    - apps/web/src/routes/dashboard.tsx
    - apps/web/src/components/ui/label.tsx
    - apps/web/src/components/ui/checkbox.tsx
    - apps/web/src/components/ui/textarea.tsx
    - apps/web/src/components/ui/dropdown-menu.tsx
    - apps/web/src/components/auth-shell.tsx
    - apps/web/src/lib/return-to.ts
    - apps/web/src/lib/api-client.ts
  modified:
    - apps/web/src/components/chrome.tsx
    - apps/web/src/routes/index.tsx
    - apps/web/src/routeTree.gen.ts
    - apps/web/vite.config.ts

key-decisions:
  - "Hand-authored Base UI shadcn wrappers (same as Phase 3) rather than CLI scaffold"
  - "Post-auth redirects via window.location.assign(safeReturnTo) to support untyped returnTo paths"
  - "Vite proxies /api/auth for WorkOS/OIDC start URLs in local dev"

patterns-established:
  - "AuthShell: max-w-md column, mark 48, Heading + muted Body, enter motion with reduced-motion off"
  - "Account menu: Dropdown Menu trigger ≥44px with aria-label Account menu; Log out = auth.logout this device"

requirements-completed: [AUTH-01, AUTH-02, AUTH-03]

duration: 4min
completed: 2026-09-10
---

# Phase 4 Plan 07: Auth UI Shell Summary

**Mode-exclusive `/login` `/signup` plus thin `/dashboard`, shadcn form/menu primitives, and chrome/landing CTAs wired to `auth.*` with this-device logout**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-10T00:02:14Z
- **Completed:** 2026-09-10T00:06:37Z
- **Tasks:** 2
- **Files modified:** 14

## Accomplishments

- Added Label, Checkbox, Textarea, Dropdown Menu (Base UI / base-nova) matching h-11 / ≥44px targets
- `/login` and `/signup` render local vs WorkOS vs OIDC exclusively from `auth.providerConfig` with UI-SPEC copy
- `/dashboard` greeting shell with Profile/Status/(Auth settings) links and incomplete-profile banner
- Header Sign in/Sign up enabled; signed-in account menu with Profile, Dashboard, Auth settings (admin), Log out
- Landing **Get started** → `/signup` (hero + closing); no Coming soon / forgot / verify UI

## Task Commits

Each task was committed atomically:

1. **Task 1: shadcn form primitives + auth pages** - `9027157` (feat)
2. **Task 2: Chrome account menu + landing Get started** - `efa4343` (feat)

**Plan metadata:** `a28b718` (docs: complete plan)

## Files Created/Modified

- `apps/web/src/routes/login.tsx` — Sign in page (mode-exclusive)
- `apps/web/src/routes/signup.tsx` — Create account page (mode-exclusive)
- `apps/web/src/routes/dashboard.tsx` — Thin signed-in home
- `apps/web/src/components/auth-shell.tsx` — Shared auth column + error banner
- `apps/web/src/components/ui/label.tsx` — Form labels
- `apps/web/src/components/ui/checkbox.tsx` — Remember me control
- `apps/web/src/components/ui/textarea.tsx` — Bio-ready textarea (Phase 4-08)
- `apps/web/src/components/ui/dropdown-menu.tsx` — Account menu primitive
- `apps/web/src/lib/return-to.ts` — D-15 / T-04-23 safe redirect helper
- `apps/web/src/lib/api-client.ts` — Shared credentials-include client
- `apps/web/src/components/chrome.tsx` — Auth CTAs + account menu
- `apps/web/src/routes/index.tsx` — Get started → /signup
- `apps/web/src/routeTree.gen.ts` — Generated login/signup/dashboard routes
- `apps/web/vite.config.ts` — Proxy `/api/auth` for SSO start

## Decisions Made

- Hand-authored shadcn wrappers against `@octanejs/base-ui` (Phase 3 pattern) so contracts stay stable
- Used `window.location.assign` for post-auth/`returnTo` navigation so arbitrary safe paths work before profile/admin routes exist
- Added Vite `/api/auth` proxy so WorkOS/OIDC CTAs reach the API in local dev

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Vite proxy for `/api/auth`**
- **Found during:** Task 1 (SSO CTAs)
- **Issue:** Dev server only proxied `/api/rpc`; WorkOS/OIDC start URLs would 404 in Vite
- **Fix:** Added `/api/auth` proxy to `apps/web/vite.config.ts`
- **Files modified:** `apps/web/vite.config.ts`
- **Verification:** Config present; `bun run build` succeeds
- **Committed in:** `9027157` (Task 1)

---

**Total deviations:** 1 auto-fixed (1 missing critical)
**Impact on plan:** Required for SSO CTAs in local dev; no scope creep.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Ready for `04-08` profile + admin auth UI (routes linked from dashboard/menu already)
- AUTH-01/02/03 UI paths exist for local mode; backend already complete

## Self-Check: PASSED

- Created routes and UI primitives exist on disk
- Commits `9027157` and `efa4343` present in `git log`
- `bun run build` in `apps/web` succeeded after both tasks
- Acceptance greps for UI-SPEC copy and no forgot/better-auth/verify strings passed

---
*Phase: 04-auth-sessions-email*
*Completed: 2026-09-10*
