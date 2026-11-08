---
phase: 10-orgs-permissions
plan: "01"
subsystem: testing
tags: [wave0, nyquist, vitest, orgs, permissions, org-01, org-03, d-org-06, ui]

requires:
  - phase: 08-git-https-pats
    provides: Wave 0 Vitest runtime-variable @vite-ignore stub pattern (tokens.integration.test)
provides:
  - "Wave 0 RED /orgs/new Vitest stubs (ORG-01 / D-ORG-01 / D-ORG-06)"
  - "Wave 0 RED /new owner picker Vitest stubs (D-ORG-06)"
  - "Wave 0 RED collaborators panel Vitest stubs (ORG-03 / D-ORG-02c / D-ORG-04)"
  - "Wave 0 RED org members/invites/member_base Vitest stubs (ORG-01 / D-ORG-02b / D-ORG-03)"
affects:
  - 10-10 org members UI greens
  - 10-11 owner picker + collaborators greens
  - 10-13 /orgs/new tracer greens

actuals:
  tokens: 3031
  tasks: 1
  commits: 7

plan_head_before: 06b5bc207b840cd8c366a45374e9a63db322af05

tech-stack:
  added: []
  patterns:
    - "Wave 0 Vitest RED stubs with runtime-variable import(/* @vite-ignore */) while .tsrx absent"
    - "Owner-picker stubs assert against live /new (placeholder copy + missing Owner Select) for immediate RED"

key-files:
  created:
    - apps/web/src/routes/orgs.new.integration.test.ts
    - apps/web/src/routes/new.owner-picker.integration.test.ts
    - apps/web/src/routes/$owner.$repo.settings.collaborators.integration.test.ts
    - apps/web/src/routes/$owner.settings.members.integration.test.ts
  modified: []

key-decisions:
  - "Wave 0 is RED-only — no production Octane org routes"
  - "Collaborators stubs import future collaborators-panel module (absent → clear Wave 0 error)"
  - "T-10-03 encoded: lookup autocomplete must never show email"

patterns-established:
  - "Phase 10 web Nyquist Wave 0 mirrors Phase 08 tokens stub discoverability"

requirements-completed: []  # Wave 0 scaffolds only; greens land in 10-10/10-11/10-13

coverage:
  - id: D1
    description: "Vitest Wave 0 stubs for /orgs/new verified create + reserved/taken slug errors"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "bun --cwd apps/web run test --project integration -- src/routes/orgs.new.integration.test.ts"
        status: pass
    human_judgment: false
  - id: D2
    description: "Vitest Wave 0 stubs for /new owner picker (self + Owner/Admin orgs, owner slug on create)"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "bun --cwd apps/web run test --project integration -- src/routes/new.owner-picker.integration.test.ts"
        status: pass
    human_judgment: false
  - id: D3
    description: "Vitest Wave 0 stubs for Collaborators panel gated by can_admin + permission ladder"
    requirement: ORG-03
    verification:
      - kind: integration
        ref: "bun --cwd apps/web run test --project integration -- src/routes/$owner.$repo.settings.collaborators.integration.test.ts"
        status: pass
    human_judgment: false
  - id: D4
    description: "Vitest Wave 0 stubs for org members username add + email invite + member_base"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "bun --cwd apps/web run test --project integration -- src/routes/$owner.settings.members.integration.test.ts"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-09-13
status: complete
---

# Phase 10 Plan 01: Web Wave 0 Orgs UI Stubs Summary

**Nyquist/Wave 0 Vitest RED stubs for /orgs/new, /new owner picker, Collaborators, and org members surfaces (ORG-01, ORG-03, D-ORG-06).**

## Performance

- **Duration:** 5 min
- **Started:** 2026-09-13T23:36:00Z
- **Completed:** 2026-09-13T23:40:41Z
- **Tasks:** 1
- **Files modified:** 4

## Accomplishments

- `/orgs/new` stubs encode verified create form, verify wall, reserved slug copy, and taken slug dual-namespace errors (D-ORG-01 / D-ORG-06)
- `/new` owner-picker stubs assert Owner Select + removal of Organizations come in a later phase + owner slug on create (D-ORG-06)
- Collaborators stubs target future `collaborators-panel` with can_admin gate, read|write|admin ladder, and T-10-03 no-email lookup
- Members stubs cover username add, invites Revoke/Keep dialog, and member_base None|Read|Write (D-ORG-02b / D-ORG-03)

## Task Commits

Each task was committed atomically:

1. **Task 1: Web Wave 0 stubs for orgs UI surfaces** - `b414571` (test)

**Plan metadata:** `d789f73` (docs: complete plan)

_Note: Wave 0 is RED-only by design — GREEN belongs to 10-10 / 10-11 / 10-13._

## Files Created/Modified

- `apps/web/src/routes/orgs.new.integration.test.ts` - /orgs/new Nyquist RED stubs
- `apps/web/src/routes/new.owner-picker.integration.test.ts` - /new owner picker Nyquist RED stubs
- `apps/web/src/routes/$owner.$repo.settings.collaborators.integration.test.ts` - Collaborators panel Nyquist RED stubs
- `apps/web/src/routes/$owner.settings.members.integration.test.ts` - members/invites/member_base Nyquist RED stubs

## Decisions Made

- Kept Wave 0 RED-only; no production `.tsrx` org routes
- Used runtime-variable `import(/* @vite-ignore */)` for absent modules; owner-picker asserts against live `/new` for immediate RED
- Encoded T-10-03 (no email in lookup) in collaborators + members stub messages

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Relative Write paths landed in main checkout**
- **Found during:** Task 1 (web stubs)
- **Issue:** Agent workspace root is the main `oxidean` checkout; relative `Write` wrote stubs there instead of `oxidean-wt-10`
- **Fix:** Copied files into worktree, deleted untracked copies from main, rewrote subsequent edits via absolute worktree paths
- **Files modified:** four integration test files (worktree only)
- **Verification:** `test -f` under worktree; main paths absent
- **Committed in:** `b414571` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Path hygiene only; no scope creep.

## Issues Encountered

None beyond the worktree path fix above.

## Known Stubs

Wave 0 intentional RED stubs (expected until later plans):

| File | Reason |
|------|--------|
| `orgs.new.integration.test.ts` | Fails until `/orgs/new` lands in 10-13 |
| `new.owner-picker.integration.test.ts` | Fails until owner Select lands in 10-11 |
| `$owner.$repo.settings.collaborators.integration.test.ts` | Fails until collaborators-panel + can_admin in 10-11 |
| `$owner.settings.members.integration.test.ts` | Fails until members/invites UI in 10-10 |

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Web Wave 0 paths named by VALIDATION.md / later UI plans are discoverable and RED (15 failing tests / 4 files)
- Ready for Wave 1+ implementation plans (10-02+)

## Self-Check: PASSED

- FOUND: `apps/web/src/routes/orgs.new.integration.test.ts`
- FOUND: `apps/web/src/routes/new.owner-picker.integration.test.ts`
- FOUND: `apps/web/src/routes/$owner.$repo.settings.collaborators.integration.test.ts`
- FOUND: `apps/web/src/routes/$owner.settings.members.integration.test.ts`
- FOUND: commit `b414571`

---
*Phase: 10-orgs-permissions*
*Completed: 2026-09-13*
