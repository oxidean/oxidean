---
phase: 03-brand-shell-theme
plan: "02"
subsystem: ui
tags: [theme, fouc, base-ui-select, chrome, header, footer]

requires:
  - phase: 03-brand-shell-theme
    plan: "01"
    provides: OxideanMark, Button/buttonVariants, Input, Select* primitives, full semantic token layer
provides:
  - Pre-paint theme boot script (THEME_BOOT_SCRIPT) rendered before HeadContent in __root.tsx, eliminating FOUC
  - ThemeSelect component — Base UI Select bound to theme preference with Monitor/Sun/Moon lucide icons
  - Branded SiteHeader/SiteFooter chrome built entirely from ShadCN/Base UI primitives on semantic tokens
affects: [03-05-pwa-icons]

tech-stack:
  added: []
  patterns:
    - "Module-level static script string (THEME_BOOT_SCRIPT) with zero interpolation, injected via dangerouslySetInnerHTML as the first child of <Head>, before <HeadContent />"
    - "Theme control state lives in ThemeSelect (not SiteHeader) — SiteHeader is now a stateless composition"

key-files:
  created:
    - apps/web/src/components/theme-select.tsx
  modified:
    - apps/web/src/lib/theme.ts
    - apps/web/src/routes/__root.tsx
    - apps/web/src/components/chrome.tsx

key-decisions:
  - "Kept THEME_STORAGE_KEY inlined as a literal string inside THEME_BOOT_SCRIPT (rather than referencing the exported constant) since the boot script runs before any module evaluates; documented in a code comment to keep the two in sync"
  - "Moved theme read/apply/media-query-listener logic out of SiteHeader into ThemeSelect so SiteHeader has no local state, matching the plan's contract that chrome.tsx becomes a stateless composition"

requirements-completed: [BRAND-01, BRAND-02, BRAND-03, BRAND-04, BRAND-05]

duration: 15min
completed: 2026-09-09
---

# Phase 3 Plan 02: Branded Chrome, Theme Select, FOUC Boot Summary

**Pre-paint theme boot script plus a Base UI Select theme control and a fully ShadCN-primitive header/footer, closing out the chrome half of the legacy accent-token migration.**

## Performance

- **Duration:** 15 min
- **Tasks:** 2 completed
- **Files modified:** 4 (3 modified, 1 created)

## Accomplishments

- Added `THEME_BOOT_SCRIPT` (static, zero-interpolation) and `THEME_OPTIONS` to `lib/theme.ts`; rendered the script as the first child of `<Head>` in `__root.tsx`, ahead of `<HeadContent />`, so `html.dark` is toggled before any stylesheet paints (D-12, BRAND-04/05, no FOUC)
- Built `ThemeSelect` — a Base UI `Select` bound to `readThemePreference`/`applyTheme`, with `Monitor`/`Sun`/`Moon` lucide icons on menu items, a `Check` selected-indicator, and `aria-label="Theme"` on the trigger (D-08, D-11)
- Rewrote `chrome.tsx` end to end: `SiteHeader` now composes `OxideanMark` (32px) + responsive wordmark (`hidden … sm:inline`, Heading type) → disabled `Input` search → `ThemeSelect` → a `role="group"` auth `Button` pair (ghost Sign in, secondary Sign up), all on semantic tokens (`border-border`, `bg-card`, `text-foreground`, `text-muted-foreground`) with zero remaining `--color-accent-*` / legacy `var(--color-*)` references; `SiteFooter` stays text-only (`© Oxidean` + Status link, no mark)

## Task Commits

Each task was committed atomically:

1. **Task 1: FOUC boot script + theme Select component** - `8ccc119` (feat)
2. **Task 2: Branded header + text-only footer on semantic tokens** - `348f025` (feat)

**Plan metadata:** pending (this commit)

## Files Created/Modified

- `apps/web/src/lib/theme.ts` - Added `THEME_OPTIONS`, `THEME_BOOT_SCRIPT` (static literal, `try/catch`-wrapped, allowlists `light|dark|system`)
- `apps/web/src/routes/__root.tsx` - Boot script rendered via `dangerouslySetInnerHTML` before `HeadContent`; `title: "Oxidean"` head meta unchanged
- `apps/web/src/components/theme-select.tsx` - New: `ThemeSelect` component, mounts to sync trigger label post-hydration, listens for OS `prefers-color-scheme` changes while preference is `"system"`
- `apps/web/src/components/chrome.tsx` - `SiteHeader`/`SiteFooter` rewritten onto ShadCN primitives (`OxideanMark`, `Input`, `Button`, `ThemeSelect`) and semantic Tailwind tokens; theme state moved out to `ThemeSelect`

## Decisions Made

- `THEME_STORAGE_KEY`'s value (`"oxidean-theme"`) is duplicated as a literal inside `THEME_BOOT_SCRIPT` rather than interpolated, because the script executes in `<head>` before any JS module (including `theme.ts`) has run. A code comment flags the two must stay in sync.
- Theme preference state and the `prefers-color-scheme` media-query listener moved from `SiteHeader` into `ThemeSelect`, so `chrome.tsx` has no local state and is purely compositional, matching the plan's task-1/task-2 split.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None. Sibling plans 03-03 (`apps/web/src/routes/index.tsx`) and 03-04 (`apps/web/src/routes/status.tsx`) completed their own accent-token migrations before this plan's final build verification ran, so `bun run --filter @oxidean/web build` was green with zero legacy `--color-accent-*` references anywhere in `apps/web/src` at completion — no transient build failure was observed or needed to be documented.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None. `ThemeSelect` is fully wired to `readThemePreference`/`applyTheme`; the disabled search `Input` and disabled auth `Button`s are intentional placeholders per D-18/D-19/T-03-07 (accepted threat, not a stub — auth ships in Phase 4).

## Threat Flags

None. `THEME_BOOT_SCRIPT` matches the plan's T-03-04/T-03-05/T-03-06 mitigations exactly (zero interpolation, `try/catch`-wrapped, `classList.toggle` only, value allowlisted to `light|dark|system`) — verified via the acceptance-criteria greps in both tasks. No new network endpoints, auth paths, or schema changes were introduced.

## Next Phase Readiness

- `apps/web/src/lib/theme.ts`, `apps/web/src/components/theme-select.tsx`, `apps/web/src/components/chrome.tsx`, and `apps/web/src/routes/__root.tsx` are all in place and build-verified (`bun run --filter @oxidean/web build` exits 0) for 03-05 (PWA/icons) to build on
- Chrome half of D-15 (legacy accent token removal) is closed alongside 03-03/03-04's route-level migrations — `grep -rcE 'accent-cool|accent-warm' apps/web/src` returns 0 project-wide
- No blockers for the remaining phase-3 plan (03-05)

---
*Phase: 03-brand-shell-theme*
*Completed: 2026-09-09*

## Self-Check: PASSED

All created/modified files verified present on disk (`apps/web/src/lib/theme.ts`, `apps/web/src/components/theme-select.tsx`, `apps/web/src/components/chrome.tsx`, `apps/web/src/routes/__root.tsx`); both task commits (`8ccc119`, `348f025`) verified present in `git log --oneline`. Plan-level verification (`bun run --filter @oxidean/web build` exit 0, zero legacy accent tokens in owned files, boot script precedes `HeadContent`, zero string interpolation in `THEME_BOOT_SCRIPT`, zero native `<select>/<option>/<input>/<button>` in `chrome.tsx`) all passed.
