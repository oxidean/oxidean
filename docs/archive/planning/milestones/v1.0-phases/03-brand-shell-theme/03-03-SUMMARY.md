---
phase: 03-brand-shell-theme
plan: "03"
subsystem: ui
tags: [landing, marketing, motion, intersection-observer, tailwindcss]

requires:
  - phase: 03-01
    provides: OxideanMark component, CVA Button/buttonVariants, full semantic token layer
provides:
  - Four-band editorial marketing landing (Hero → Dual-mode → Collaboration pillars → Closing CTA) replacing the Phase 1 single-screen draft
  - Exactly three intentional motions (hero stagger, one-shot scroll reveal, CTA hover) all neutralized under prefers-reduced-motion
  - Landing-half closure of D-15 (no legacy accent-cool/accent-warm token references in routes/index.tsx)
affects: [03-05-pwa-icons, future-auth-phase-landing-ctas]

tech-stack:
  added: []
  patterns:
    - "Page-scoped keyframes/motion CSS via a <style> element inside the route component (styles.css remains owned by 03-01)"
    - "Single IntersectionObserver (threshold 0.15) driving one-shot .oct-reveal → .oct-reveal-in scroll reveals, unobserving each target after it fires and disconnecting on effect cleanup"

key-files:
  created: []
  modified:
    - apps/web/src/routes/index.tsx

key-decisions:
  - "Wrapped OxideanMark in a plain <div className=\"oct-rise\"> for the hero stagger instead of extending OxideanMark's props, since OxideanMark (owned by 03-01) only accepts size/className and adding a style prop would touch a file outside this plan's scope"
  - "Ordered reveal-section class lists as \"py-24 oct-reveal\" (oct-reveal last) so the class is directly adjacent to the closing quote, satisfying the plan's literal grep-based acceptance check for oct-reveal usage"

requirements-completed: [BRAND-01, BRAND-02, BRAND-03]

duration: 20min
completed: 2026-09-09
---

# Phase 3 Plan 03: Four-Band Editorial Landing + Motions Summary

**Four-band editorial marketing landing (Hero → Dual-mode → Pillars → Closing CTA) with locked UI-SPEC copy, the shared 96px OxideanMark, brand-atmosphere gradients from semantic tokens, and exactly three reduced-motion-safe motions.**

## Performance

- **Duration:** 20 min
- **Tasks:** 2 completed
- **Files modified:** 1

## Accomplishments
- Replaced the Phase 1 single-screen landing draft with the full four-`<section>` marketing page (Hero, `id="explore"` Dual-mode split row, Collaboration pillars, Closing CTA), all copy verbatim from the UI-SPEC Copywriting Contract
- Migrated the route off every legacy `--color-accent-cool` / `--color-accent-warm` / `--color-text` / `--color-surface` / `--color-bg` reference onto semantic tokens (`--primary`, `--secondary`, `--background`, `--card`, `text-foreground`, `text-muted-foreground`) — landing half of D-15 closed
- Removed the duplicate "Oxidean" `<p>` above the hero headline; the 96×96 `OxideanMark` is now the only brand identifier in the hero (D-04)
- Implemented exactly three motions — hero entrance stagger (mark → headline → support/CTA, 0/80/160ms `oct-rise`), a one-shot `IntersectionObserver`-driven scroll reveal on the dual-mode and pillars bands, and the pre-existing `Button`/`buttonVariants` hover transition (no new hover motion added) — with a single `@media (prefers-reduced-motion: reduce)` block that neutralizes all three

## Task Commits

Each task was committed atomically:

1. **Task 1 (03-03-T1): Four editorial bands with locked copy** - `1a2841e` (feat)
2. **Task 2 (03-03-T2): Exactly three motions with reduced-motion neutralization** - `99674e7` (feat)

**Plan metadata:** pending (this commit)

## Files Created/Modified
- `apps/web/src/routes/index.tsx` - Rewritten as four editorial `<section>` bands (Hero, Dual-mode, Pillars, Closing CTA) with locked copy, semantic-token gradients, disabled `Get started` + `Explore` anchor CTAs, and the three-motion system (hero stagger, scroll reveal, CTA hover) neutralized under reduced motion

## Decisions Made
- Wrapped `OxideanMark` in a `<div className="oct-rise" style={{ animationDelay: "0ms" }}>` rather than adding a `style` prop to the component itself, keeping this plan's changes scoped to `routes/index.tsx` only (03-01/03-02 own `oxidean-mark.tsx`)
- Kept the CTA hover motion exactly as delivered by 03-01's `Button`/`buttonVariants` `transition-[background-color,border-color,color,filter] duration-150` base class — did not add a second hover animation, per the plan's explicit instruction

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- Initial paragraph text for the Dual-mode band bodies was line-wrapped across two JSX lines, which caused the acceptance-criteria `grep -qF` verbatim-string checks to fail (grep matches within a single line). Fixed by putting each Copywriting Contract sentence on one line in the JSX. Not a deviation — this was a formatting mistake in first-draft implementation, corrected before commit, not unplanned work.

## User Setup Required
None - no external service configuration required.

## Known Stubs
- `<Button disabled title="Coming soon">Get started</Button>` appears in both the Hero and Closing CTA bands with no `onClick` handler. This is the plan's explicit, locked behavior (D-24): auth/signup lands in a later phase, and the disabled state with a "Coming soon" tooltip is the intended placeholder, not an oversight.

## Next Phase Readiness
- `apps/web/src/routes/index.tsx` is fully migrated to semantic tokens, verified with `bun run --filter @oxidean/web build` (exits 0), and matches every UI-SPEC Copywriting Contract string verbatim for the landing route
- No blockers for sibling plans 03-02 (chrome/navigation) or 03-04 (status) — this plan touched only `routes/index.tsx` as scoped
- Landing-half of D-15 (accent-cool/accent-warm removal) is closed; the phase-level gate (`grep -rn 'accent-cool|accent-warm' apps/web/src` returning zero) still depends on 03-02/03-04 completing their own migrations

---
*Phase: 03-brand-shell-theme*
*Completed: 2026-09-09*

## Self-Check: PASSED

- `apps/web/src/routes/index.tsx` verified present on disk and contains `id="explore"`, `OxideanMark size={96}`, `buttonVariants({ variant: "secondary" })`, and all locked Copywriting Contract strings
- Both task commits (`1a2841e`, `99674e7`) verified present in `git log --oneline`
- `bun run --filter @oxidean/web build` exits 0 (both client and SSR builds)
- All plan `<acceptance_criteria>` and plan-level `<verification>` checks re-run and passing: 4 sections, `id="explore"`/`href="#explore"` present, zero legacy accent-token references, zero `>Oxidean<` text in the hero segment, exactly 1 `@keyframes` block, exactly 1 reduced-motion media query, `border-l-2 border-primary`/`border-l-2 border-secondary` each appear exactly once, zero card-grid patterns, zero out-of-scope typography sizes/weights
