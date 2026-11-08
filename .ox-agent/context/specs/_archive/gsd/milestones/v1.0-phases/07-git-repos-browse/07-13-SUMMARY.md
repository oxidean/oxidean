---
phase: 07-git-repos-browse
plan: "13"
subsystem: ui
tags: [octane, /new, repo.create, quick-setup, D-01, D-10, D-11, D-14]

requires:
  - phase: 07-git-repos-browse
    provides: "repo.create RPC + api-client (07-12); Wave 0 /new + SignedInHome stubs (07-16)"
provides:
  - "Octane /new create form with verify wall (D-01, D-05, D-07, D-11)"
  - "Empty /{owner}/{repo} Quick setup Code home (D-10, D-14, D-15)"
  - "SignedInHome New repository → /new when verified"
affects:
  - 07-03 full create templates/SPDX
  - 07-04 SignedInHome dashboard IA
  - 07-15 Code tree/blob browse

actuals:
  tokens: 6292
  tasks: 1
  commits: 3

plan_head_before: 5014bcbbdb160b3181c271069ffe3ce75111cd2d

tech-stack:
  added: []
  patterns:
    - "/$owner/$repo layout + .index leaf mirrors /setup Outlet pattern"
    - "Visibility segmented Buttons until radio-group lands in 07-09"
    - "Create success uses window.location.assign to /{owner}/{repo} (D-14)"

key-files:
  created:
    - apps/web/src/routes/new.tsrx
    - apps/web/src/routes/$owner.$repo.tsrx
    - apps/web/src/routes/$owner.$repo.index.tsrx
  modified:
    - apps/web/src/components/signed-in-home.tsrx
    - apps/web/src/routeTree.gen.ts

key-decisions:
  - "Added $owner.$repo.tsrx Outlet layout so .index Quick setup registers like /setup"
  - "Template/license/gitignore stay None-only copy placeholders (ASSUME GIT-01 empty; full pickers 07-03)"
  - "Public/Private via button group — no new shadcn radio-group (07-09)"

patterns-established:
  - "Repo URL scheme /{owner}/{repo} with empty Quick setup before tree (07-15)"
  - "Unverified /new uses AuthShell wall + Verify email → /verify (D-11)"

requirements-completed: [GIT-01]

coverage:
  - id: D1
    description: "Unverified /new shows Verify your email wall; create form hidden"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "apps/web/src/routes/new.integration.test.ts#unverified session shows Verify your email wall"
        status: pass
    human_judgment: false
  - id: D2
    description: "Verified SignedInHome New repository links to /new; unverified disabled+hint"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "apps/web/src/components/signed-in-home.integration.test.ts#verified: enabled New repository navigates to /new"
        status: pass
    human_judgment: false
  - id: D3
    description: "Empty Code home shows Quick setup first-push guide at /{owner}/{repo}"
    requirement: GIT-01
    verification:
      - kind: other
        ref: "rg Quick setup apps/web/src/routes/$owner.$repo.index.tsrx + bun run build"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 13: Minimal /new + Empty Code Quick Setup Summary

**Octane `/new` create tracer with verify wall and empty `/{owner}/{repo}` Quick setup so `repo.create` is reachable end-to-end in the UI.**

## Performance

- **Duration:** 5 min
- **Started:** 2026-09-12T17:15:16Z
- **Completed:** 2026-09-12T17:19:50Z
- **Tasks:** 1
- **Files modified:** 5

## Accomplishments
- Verified users open `/new`, create empty public/private repos via `repo.create`, land on Quick setup
- Unverified sessions get AuthShell verify wall; home CTA stays disabled with hint
- Regenerated route tree for `/new` and `/$owner/$repo/`

## Task Commits

Each task was committed atomically:

1. **Task 1: Minimal /new + empty Code Quick setup route** - `e83d452` (feat)

**Plan metadata:** `f31225b` (docs: complete plan)

## Files Created/Modified
- `apps/web/src/routes/new.tsrx` — create form + D-11 verify wall; navigates to `/{owner}/{repo}`
- `apps/web/src/routes/$owner.$repo.tsrx` — Outlet layout for repo browse surfaces
- `apps/web/src/routes/$owner.$repo.index.tsrx` — empty Code Quick setup (HTTPS remote guide)
- `apps/web/src/components/signed-in-home.tsrx` — verified Link to `/new`
- `apps/web/src/routeTree.gen.ts` — generated `/new` + `/$owner/$repo` routes

## Decisions Made
- Layout parent `$owner.$repo.tsrx` required for `.index` child (same as `/setup`)
- None-only template placeholders deferred to 07-03; visibility buttons until 07-09 radio-group
- `packages/api-client` unchanged — `repo.create` already present from 07-12

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added `$owner.$repo.tsrx` layout**
- **Found during:** Task 1 (route registration)
- **Issue:** Plan listed only `$owner.$repo.index.tsrx`; TanStack needs a parent Outlet route for the index leaf (mirrors `setup.tsrx`)
- **Fix:** Minimal layout exporting `Outlet` at `/$owner/$repo`
- **Files modified:** `apps/web/src/routes/$owner.$repo.tsrx`
- **Verification:** `bun run build` emits `_owner._repo` chunks; routeTree includes both routes
- **Committed in:** `e83d452` (Task 1)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Required for D-14 route to exist; no scope creep beyond layout shell.

## Known Stubs
| File | Line | Stub | Reason |
|------|------|------|--------|
| `apps/web/src/routes/new.tsrx` | ~206 | Stack/License/.gitignore “None” placeholders | Intentional per plan ASSUME (GIT-01 empty); full pickers in 07-03 |
| `apps/web/src/routes/$owner.$repo.index.tsrx` | — | No tree/README; nav links non-routed | Intentional tracer; full Code IA in 07-15 / 07-04 |

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- UI create path closed for humans; 07-03 can expand templates; 07-15 can replace empty Quick setup with tree when commits exist
- Full dashboard list IA remains 07-04

## Self-Check: PASSED

- FOUND: `apps/web/src/routes/new.tsrx`
- FOUND: `apps/web/src/routes/$owner.$repo.tsrx`
- FOUND: `apps/web/src/routes/$owner.$repo.index.tsrx`
- FOUND: `apps/web/src/components/signed-in-home.tsrx`
- FOUND: `.planning/phases/07-git-repos-browse/07-13-SUMMARY.md`
- FOUND: commit `e83d452`

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*
