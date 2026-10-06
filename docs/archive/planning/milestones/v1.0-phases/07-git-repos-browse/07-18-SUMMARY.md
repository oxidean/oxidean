---
phase: 07-git-repos-browse
plan: "18"
subsystem: ui
tags: [branches, tags, dialog, alert-dialog, soft-protect, shadcn]

requires:
  - phase: 07-git-repos-browse
    provides: "Owner-only repo.branchCreate/Rename/Delete + soft-protect (07-07)"
  - phase: 07-git-repos-browse
    provides: "RepoChrome link-row + archive HTTP (07-15 / 07-08)"
provides:
  - "Branches page with owner CRUD + default soft-protect UI"
  - "Tags page with list + ZIP/tar.gz download (no create-tag CTA)"
  - "Official shadcn Base UI Dialog + AlertDialog wrappers"
affects:
  - 07-verify
  - phase-08

actuals:
  tokens: 13266
  tasks: 1
  commits: 1

plan_head_before: fc542c5246aa3a3155ffd2e6f04265f12b480894

tech-stack:
  added:
    - "@octanejs/base-ui/dialog"
    - "@octanejs/base-ui/alert-dialog"
  patterns:
    - "Owner gate via auth.me id === repo.owner_id; mutate actions hidden for non-owners"
    - "Default soft-protect: disabled Rename/Delete + title copy (D-28)"
    - "Delete branch AlertDialog requires typing exact branch name"

key-files:
  created:
    - apps/web/src/routes/$owner.$repo.branches.tsrx
    - apps/web/src/routes/$owner.$repo.tags.tsrx
    - apps/web/src/components/ui/dialog.tsrx
    - apps/web/src/components/ui/alert-dialog.tsrx
  modified:
    - apps/web/src/lib/repo-browse.ts
    - apps/web/src/routeTree.gen.ts

key-decisions:
  - "Hand-authored Dialog/AlertDialog from official Base UI + base-nova patterns (no third-party registry)"
  - "Branch/tag lists show tip short SHA; per-ref updated timestamps omitted until API exposes them"
  - "Tags remain list+download only — no create-tag primary CTA (UI-SPEC lock)"

patterns-established:
  - "Destructive branch delete: AlertDialog + type-to-confirm + destructive Delete branch CTA"
  - "Create/rename: controlled Dialog with Input + Cancel/primary footer"

requirements-completed: [GIT-06, GIT-05]

coverage:
  - id: D1
    description: "Owner can create/rename/delete non-default branches from Branches UI; non-owner read-only"
    requirement: GIT-06
    verification:
      - kind: other
        ref: "bun --cwd apps/web run build"
        status: pass
    human_judgment: true
    rationale: "Owner vs non-owner mutate UX needs visual confirmation beyond build"
  - id: D2
    description: "Default branch rename/delete controls disabled with soft-protect title copy"
    requirement: GIT-06
    verification:
      - kind: other
        ref: "rg -n 'Default branch can.t be renamed' apps/web/src/routes/$owner.$repo.branches.tsrx"
        status: pass
    human_judgment: false
  - id: D3
    description: "Tags list with archive download actions and no create-tag CTA"
    requirement: GIT-05
    verification:
      - kind: other
        ref: "bun --cwd apps/web run build"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 18: Branches / Tags UI Summary

**Owner branch CRUD Octane UI with default soft-protect + type-to-confirm delete; Tags list with archive downloads only (no create-tag CTA)**

## Performance

- **Duration:** 5 min
- **Started:** 2026-09-12T18:38:42Z
- **Completed:** 2026-09-12T18:43:32Z
- **Tasks:** 1
- **Files modified:** 6

## Accomplishments

- Branches page lists name + tip SHA; owner New branch / rename / delete; default soft-protect disables mutate with UI-SPEC title
- Delete branch AlertDialog requires typing the branch name; destructive **Delete branch** CTA
- Tags page lists tags with Download ZIP / tar.gz; no create-tag primary CTA
- Official `@octanejs/base-ui` Dialog + AlertDialog wrappers (base-nova styling)

## Task Commits

Each task was committed atomically:

1. **Task 1: Branches / Tags UI + delete confirm dialog** - `c714f4d` (feat)

**Plan metadata:** `4aac4dc` (docs: complete plan)

## Files Created/Modified

- `apps/web/src/routes/$owner.$repo.branches.tsrx` — Branches management UI + dialogs
- `apps/web/src/routes/$owner.$repo.tags.tsrx` — Tags list + archive downloads
- `apps/web/src/components/ui/dialog.tsrx` — shadcn Dialog wrapper
- `apps/web/src/components/ui/alert-dialog.tsrx` — shadcn AlertDialog wrapper
- `apps/web/src/lib/repo-browse.ts` — branch/tag/archive helpers
- `apps/web/src/routeTree.gen.ts` — register `/branches` and `/tags`

## Decisions Made

- Hand-authored Dialog/AlertDialog against `@octanejs/base-ui` (official Base UI path) instead of `shadcn add`, matching existing Octane `.tsrx` UI wrappers
- Tip short SHA shown as the tip column; per-ref “updated” time deferred (API `repo.refs` has no authordate yet)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- GIT-06 UI + tags half of GIT-05 shipped; remaining Phase 7 plans (09–11) can proceed independently
- Visual UAT for owner vs non-owner mutate chrome still useful at verify-work

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*

## Self-Check: PASSED
