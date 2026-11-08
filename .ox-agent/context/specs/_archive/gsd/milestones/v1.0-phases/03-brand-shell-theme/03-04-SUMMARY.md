---
phase: 03-brand-shell-theme
plan: "04"
subsystem: ui
tags: [tanstack-router, semantic-tokens, css-animation, status-page]

requires:
  - phase: 03-brand-shell-theme
    plan: "01"
    provides: Full ShadCN semantic token layer (--primary/--secondary/--muted/--card/--border/--destructive) replacing legacy --color-accent-cool/--color-accent-warm
provides:
  - Branded /status page with a Display-type status hero (loading/healthy/unhealthy/unreachable) driven exclusively by live system.health
  - Status · Oxidean document title via route head
  - Soft opacity resolve transition on the status panel, reduced-motion safe
affects: [03-05-pwa-icons]

tech-stack:
  added: []
  patterns:
    - "Route-scoped inline <style> block for a single-use keyframe animation, guarded by prefers-reduced-motion, instead of a global CSS addition"

key-files:
  created: []
  modified:
    - apps/web/src/routes/status.tsx

key-decisions:
  - "Kept the Phase 1 single-shot useEffect + cancelled-guard fetch pattern unchanged (T-03-14 / D-26 live-health-only); only route metadata and presentation changed"
  - "Used Display type (40px/600) for all three resolved hero states per UI-SPEC's explicit tie-break ('default Display for status hero'), not the alternate Heading-24 option"

requirements-completed: [BRAND-02, BRAND-03]

duration: 20min
completed: 2026-09-09
---

# Phase 3 Plan 04: Branded Status Hero States Summary

**`/status` rewritten onto semantic tokens with a 40px Display-type hero for all three resolved states (primary for healthy, destructive for unhealthy/unreachable), a `Status · Oxidean` document title, and a reduced-motion-safe fade-in on resolution — still backed by exactly one live `system.health` call per mount.**

## Performance

- **Duration:** 20 min
- **Tasks:** 2 completed
- **Files modified:** 1

## Accomplishments
- Route now declares `head: () => ({ meta: [{ title: "Status · Oxidean" }] })`, satisfying D-21 on a non-landing route
- All four hero states read as unmistakable per UI-SPEC: loading is muted Body text with no accent fill; healthy uses `text-primary` on a 40px Display line; unhealthy and unreachable use `text-destructive` on the same Display treatment
- Every legacy `var(--color-*)` arbitrary-value class removed from the file in favor of `bg-card`, `border-border`, `text-muted-foreground`, `text-primary`, `text-destructive`
- Added the UI-SPEC's single allowed status motion — a 200ms opacity fade-in (`oct-status-resolve` + `@keyframes octStatusIn`) on the resolved panel content only, fully disabled under `prefers-reduced-motion: reduce`, with the loading state left unanimated

## Task Commits

Each task was committed atomically:

1. **Task 1: Branded status hero states + document title** - `6cfb888` (feat)
2. **Task 2: Soft resolve transition for the status hero** - `9a463c4` (feat)

**Plan metadata:** pending (this commit)

## Files Created/Modified
- `apps/web/src/routes/status.tsx` - Branded hero states on semantic tokens, `Status · Oxidean` title, soft resolve fade-in

## Decisions Made
- Preserved the exact `Phase` union, single-shot `useEffect` fetch, and `cancelled` guard from the pre-existing implementation untouched — only route metadata (`head`) and JSX/class output changed, keeping T-03-14 (no polling) and D-26 (live health only) intact by construction
- Chose Display (40px/600) over Heading (24px/600) for the status hero per the UI-SPEC's explicit default, applied consistently across all three resolved states

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None. The repository had two other wave-2 plans (03-02 chrome, 03-03 landing) committing to the same branch concurrently; per the parallel-wave note this plan touched only `apps/web/src/routes/status.tsx` and did not read or modify their uncommitted/in-flight files.

## User Setup Required
None - no external service configuration required.

## Known Stubs
None. All four states are wired to the live `system.health` response; no hardcoded/mock data paths.

## Next Phase Readiness
- Status half of D-15 is closed: `grep -cE 'accent-cool|accent-warm|--color-(text|surface|bg|muted|border|destructive)' apps/web/src/routes/status.tsx` outputs `0`
- `bun run --filter @oxidean/web build` exits `0` after both tasks
- No blockers for the remaining phase-level gate, which also depends on 03-02 and 03-03 clearing the same legacy-token grep in their own files

---
*Phase: 03-brand-shell-theme*
*Completed: 2026-09-09*

## Self-Check: PASSED

Verified `apps/web/src/routes/status.tsx` contains `title: "Status · Oxidean"`, `text-primary`, `text-destructive` (count 2), `text-[40px]` (count 3), `oct-status-resolve`, `@keyframes` (count 1), and `prefers-reduced-motion: reduce`; zero occurrences of legacy `--color-*` tokens, `text-secondary`/`bg-secondary`/`border-secondary`, `text-[20px]`/`font-medium`/`font-bold`, `setInterval`/`setTimeout`/`refetchInterval`, `dangerouslySetInnerHTML`, and animation-loop utility classes. Commits `6cfb888` and `9a463c4` verified present in `git log --oneline`. `bun run --filter @oxidean/web build` exits 0.
