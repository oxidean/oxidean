---
phase: 04-auth-sessions-email
plan: "08"
subsystem: ui
tags: [profile, admin-auth, avatar-preview, settings, uat]

requires:
  - phase: 04-auth-sessions-email
    provides: "user.* / admin.auth.* API + api-client (04-06); login/signup/dashboard chrome (04-07)"
  - phase: 03-brand-shell-theme
    provides: "Design tokens, Input/Button, squircle mark"
provides:
  - "/settings/profile with display name, username, bio, avatar upload, logout this device + logout all"
  - "/admin/auth with provider/email selects and Configured via ENV badges (no secret inputs)"
  - "Phase 4 success-criteria UAT approved"
affects:
  - Phase 5 cloud verify & reset
  - AUTH-08
  - AUTH-09
  - AUTH-10
  - AUTH-11

tech-stack:
  added: []
  patterns:
    - "Avatar upload via multipart fetch('/api/user/avatar') credentials include; preview uses oxidean-squircle"
    - "Admin auth UI shows ENV configured badges only — never password-style secret fields (D-09, T-04-26)"
    - "Non-admin /admin/auth shows forbidden Body copy; API remains the security boundary (T-04-25)"

key-files:
  created:
    - apps/web/src/routes/settings/profile.tsx
    - apps/web/src/routes/admin/auth.tsx
    - apps/web/src/components/avatar-preview.tsx
  modified:
    - apps/web/src/routeTree.gen.ts
    - apps/web/vite.config.ts

key-decisions:
  - "Vite proxies /api/user and /uploads so avatar POST/preview work in local Vite dev"
  - "Human UAT checkpoint approved with no defects — no post-UAT code changes"

patterns-established:
  - "Profile settings: max-w-xl column, AvatarPreview 64×64, logout-all confirm with UI-SPEC copy"
  - "Admin auth settings: max-w-2xl, mode-exclusive non-secret fields, Save auth settings → admin.auth.updateSettings"

requirements-completed: [AUTH-08, AUTH-09, AUTH-10, AUTH-11]

coverage:
  - id: D1
    description: "Signed-in user can edit profile fields and upload avatar on /settings/profile"
    requirement: AUTH-08
    verification:
      - kind: other
        ref: "cd apps/web && bun run build"
        status: pass
    human_judgment: false
  - id: D2
    description: "Admins manage provider mode and email delivery on /admin/auth with ENV badges only"
    requirement: AUTH-09
    verification:
      - kind: other
        ref: "cd apps/web && bun run build"
        status: pass
    human_judgment: false
  - id: D3
    description: "Phase 4 ROADMAP success criteria verified in browser (signup/login/session/profile/mail sink/theme)"
    verification: []
    human_judgment: true
    rationale: "End-to-end UAT of four Phase 4 success criteria requires a human in a running stack"

duration: 5min
completed: 2026-09-10
status: complete
---

# Phase 4 Plan 08: Profile + Admin Auth UI Summary

**`/settings/profile` (fields, avatar, logout-all) and `/admin/auth` (provider + email with ENV badges); Phase 4 UAT checkpoint approved with no defects**

## Performance

- **Duration:** ~5 min implementation (Tasks 1–2) + human UAT (Task 3)
- **Started:** 2026-09-10T00:10:00Z (approx Task 1)
- **Completed:** 2026-09-10T15:00:00Z (UAT approved; docs wrap-up)
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments

- Profile settings page with AvatarPreview, multipart avatar upload, Save profile, Log out / Log out all devices (UI-SPEC confirm copy)
- Admin auth settings page with Local/WorkOS/OIDC and Log sink/SMTP/Resend selects; **Configured via ENV** badges; non-admin forbidden messaging
- Human verified Phase 4 success criteria — **approved**, no defects listed
- Service worker still bypasses `/api/` (unchanged; verified)

## Task Commits

Each task was committed atomically:

1. **Task 1: Profile page + avatar preview** - `8d21829` (feat)
2. **Task 2: Admin auth settings page** - `bb9a5d8` (feat)
3. **Task 3: Human verify Phase 4 success criteria** - no code commit (checkpoint: **approved**)

**Plan metadata:** `46628e1` (docs: complete plan)

## Files Created/Modified

- `apps/web/src/routes/settings/profile.tsx` — Profile · Oxidean settings (AUTH-08, D-13, D-18)
- `apps/web/src/components/avatar-preview.tsx` — 64×64 squircle avatar preview
- `apps/web/src/routes/admin/auth.tsx` — Auth settings · Oxidean (D-09, T-04-25/26)
- `apps/web/src/routeTree.gen.ts` — Generated profile + admin/auth routes
- `apps/web/vite.config.ts` — Dev proxies for `/api/user` and `/uploads`

## Decisions Made

- Added Vite `/api/user` and `/uploads` proxies so avatar upload and preview work under the Vite dev server
- UAT returned **approved** with an empty defect list — no application code changes after the checkpoint

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Vite proxies for avatar upload/serve**
- **Found during:** Task 1 (profile avatar)
- **Issue:** Dev server lacked proxies for `/api/user` and `/uploads`; avatar POST/preview would miss the API
- **Fix:** Extended `apps/web/vite.config.ts` proxies
- **Files modified:** `apps/web/vite.config.ts`
- **Verification:** Proxies present; Task 1 build verify
- **Committed in:** `8d21829` (Task 1)

---

**Total deviations:** 1 auto-fixed (1 missing critical)
**Impact on plan:** Required for local avatar UX; no scope creep.

## Issues Encountered

None

## User Setup Required

None for default local + log-sink path. SMTP/Resend/WorkOS/OIDC remain optional ENV (see `04-USER-SETUP.md`).

## Next Phase Readiness

- Phase 4 plans 01–08 complete; AUTH-01/02/03/08/09/10/11 UI + API surfaces landed
- Ready for Phase 5 (cloud verify & reset) and Phase 6 (self-host admin bootstrap)

## Self-Check: PASSED

- `apps/web/src/routes/settings/profile.tsx` FOUND
- `apps/web/src/routes/admin/auth.tsx` FOUND
- `apps/web/src/components/avatar-preview.tsx` FOUND
- Commits `8d21829` and `bb9a5d8` present in `git log`
- Human resume signal: **approved** (no defects)
- `sw.js` still bypasses paths starting with `/api/`

---
*Phase: 04-auth-sessions-email*
*Completed: 2026-09-10*
