---
phase: 08-git-https-pats
plan: "12"
subsystem: ui
tags: [pat, how-to, clone-box, quick-setup, git-02, d-13, octane]

requires:
  - phase: 08-git-https-pats
    provides: 08-01 Wave 0 clone-box.pat.integration.test stubs (GIT-02)
  - phase: 08-git-https-pats
    provides: 08-09 /settings/tokens list route for CTA target
provides:
  - "Shared PatHowTo panel (D-13 username aliases + PAT-as-password)"
  - "CloneBox + QuickSetup HTTPS credential how-to wire-up"
affects:
  - 08-13 docs / phase close
  - Phase 9 SSH clone UI (placeholder untouched)

actuals:
  tokens: 1165
  tasks: 1
  commits: 1

plan_head_before: b5ba0bcc80d7affb3b348aa020ec1df7fb315613

tech-stack:
  added: []
  patterns:
    - "Shared PatHowTo embed below HTTPS URL; plain <a> CTA to /settings/tokens (no RouterProvider in CloneBox tests)"
    - "SSH remains muted placeholder until Phase 9 (D-02)"

key-files:
  created:
    - apps/web/src/components/repo/pat-how-to.tsrx
  modified:
    - apps/web/src/components/repo/clone-box.tsrx
    - apps/web/src/components/repo/quick-setup.tsrx
    - apps/web/src/components/repo/clone-box.pat.integration.test.ts

key-decisions:
  - "Plain <a href=/settings/tokens> instead of TanStack Link so CloneBox unit tests need no RouterProvider; tokens route redirects signed-out users"
  - "Always-visible how-to (not expandable) matching UI-SPEC ordered content + git clone example with overflow-x-auto"

patterns-established:
  - "PatHowTo shared between CloneBox (compact) and QuickSetup (full)"

requirements-completed: [GIT-02]

coverage:
  - id: D1
    description: "Authenticate with a personal access token how-to with username aliases git/token/oauth2"
    requirement: GIT-02
    verification:
      - kind: integration
        ref: "apps/web/src/components/repo/clone-box.pat.integration.test.ts#shows Authenticate with a personal access token how-to with username aliases"
        status: pass
    human_judgment: false
  - id: D2
    description: "Create a personal access token CTA points to /settings/tokens"
    requirement: GIT-02
    verification:
      - kind: integration
        ref: "apps/web/src/components/repo/clone-box.pat.integration.test.ts#Create a personal access token CTA points to /settings/tokens"
        status: pass
    human_judgment: false
  - id: D3
    description: "QuickSetup embeds same PatHowTo after HTTPS block; SSH placeholder unchanged"
    requirement: GIT-02
    verification:
      - kind: other
        ref: "bun --cwd apps/web run build"
        status: pass
    human_judgment: false

duration: 2min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 12: Shared PAT how-to panel Summary

**CloneBox and QuickSetup teach username+PAT over HTTPS via shared PatHowTo (D-13 / GIT-02)**

## Performance

- **Duration:** 2 min
- **Started:** 2026-09-13T19:44:23Z
- **Completed:** 2026-09-13T19:46:36Z
- **Tasks:** 1
- **Files modified:** 4

## Accomplishments

- Shared `PatHowTo` with UI-SPEC copy: username aliases (`git` / `token` / `oauth2`), password=PAT not account password, create CTA, `git clone` example
- Embedded in CloneBox dropdown below HTTPS URL and in QuickSetup after HTTPS block
- Left SSH muted placeholder untouched (D-02 / Phase 9)

## Task Commits

1. **Task 1: PatHowTo shared component + CloneBox/QuickSetup wire-up** - `f21f9e3` (feat)

## Files Created/Modified

- `apps/web/src/components/repo/pat-how-to.tsrx` — shared D-13 how-to panel
- `apps/web/src/components/repo/clone-box.tsrx` — embed compact PatHowTo
- `apps/web/src/components/repo/quick-setup.tsrx` — embed PatHowTo after HTTPS
- `apps/web/src/components/repo/clone-box.pat.integration.test.ts` — comment refresh (now green)

## Decisions Made

- Used plain `<a href="/settings/tokens">` for the CTA so CloneBox integration tests (no RouterProvider) stay green; settings tokens route already redirects unauthenticated users to login with returnTo
- Always-visible panel (not accordion) with monospace `overflow-x-auto` / `break-all` clone example for long URLs

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] TanStack Link required RouterProvider in CloneBox tests**
- **Found during:** Task 1 (PatHowTo wire-up)
- **Issue:** `@octanejs/tanstack-router` `Link` threw `useRouter must be used inside a <RouterProvider>` when CloneBox rendered in vitest
- **Fix:** Switch CTA to plain `<a href="/settings/tokens">` (same pattern as `settings-nav.tsrx`)
- **Files modified:** `apps/web/src/components/repo/pat-how-to.tsrx`
- **Commit:** `f21f9e3`

## Verification

- `bunx vitest run src/components/repo/clone-box.pat.integration.test.ts` — 2 passed
- `bun run build` (apps/web) — OK
- Acceptance: `pat-how-to.tsrx` exists; rg finds Authenticate / oauth2 / Create CTA copy

## Known Stubs

None — SSH placeholder is intentional Phase 9 deferral (D-02), not a stub blocking GIT-02.

## Self-Check: PASSED

- FOUND: `apps/web/src/components/repo/pat-how-to.tsrx`
- FOUND: `f21f9e3` in git log
