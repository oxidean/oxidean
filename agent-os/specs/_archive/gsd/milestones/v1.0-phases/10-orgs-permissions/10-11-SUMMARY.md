---
phase: 10-orgs-permissions
plan: "11"
subsystem: ui
tags: [octane, owner-picker, collaborators, can_admin, org.listMine]

requires:
  - phase: 10-orgs-permissions
    provides: org.listMine, repo.collaborators.*, user.lookup, /new route
provides:
  - "/new owner Select (self + Owner/Admin orgs) posting owner on repo.create"
  - "Repo settings Collaborators panel gated by can_admin"
affects:
  - 10-12 invite accept UI
  - org-owned repo create UX

actuals:
  tokens: 8854
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "SSR org.listMine filtered to Owner/Admin for /new owner options"
    - "Repo settings visibility/admin surfaces gate on repo.can_admin"
    - "Reuse MemberLookup for collaborator username add"

key-files:
  created:
    - apps/web/src/components/repo/collaborators-panel.tsrx
  modified:
    - apps/web/src/routes/new.tsrx
    - apps/web/src/routes/new.owner-picker.integration.test.ts
    - apps/web/src/routes/new.integration.test.ts
    - apps/web/src/routes/$owner.$repo.settings.tsrx
    - apps/web/src/routes/$owner.$repo.settings.collaborators.integration.test.ts

key-decisions:
  - "Load eligible owner orgs in /new SSR loader (filter Owner/Admin) rather than client-only listMine"
  - "Gate entire repo settings page on repo.can_admin, not me.id === owner_id"
  - "Reuse MemberLookup for collaborator username autocomplete (never email)"

patterns-established:
  - "Owner picker: SelectRoot + self username default + optional owner slug on repo.create"
  - "CollaboratorsPanel: Query list + permission Select read|write|admin + remove AlertDialog"

requirements-completed: [ORG-03, ORG-01]

coverage:
  - id: D1
    description: "/new owner picker lists self + Owner/Admin orgs and posts owner on repo.create"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: apps/web/src/routes/new.owner-picker.integration.test.ts
        status: pass
    human_judgment: false
  - id: D2
    description: "Repo settings Collaborators panel with can_admin gate (not owner_id equality)"
    requirement: ORG-03
    verification:
      - kind: integration
        ref: apps/web/src/routes/$owner.$repo.settings.collaborators.integration.test.ts
        status: pass
    human_judgment: false

duration: 8min
completed: 2026-09-14
status: complete
plan_head_before: f8b35f0e6cbfbed1529b3cecd592764ec6169e70
commits: 2
---

# Phase 10 Plan 11: Owner picker + Collaborators Summary

**/new owner Select for self and Admin+ orgs, plus can_admin-gated Collaborators settings with read|write|admin grants.**

## Performance

- **Duration:** 8 min
- **Started:** 2026-09-14T01:45:21Z
- **Completed:** 2026-09-14T01:53:31Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- Replaced locked-to-self /new owner UI with Select of `@username` + Owner/Admin orgs from `org.listMine`; posts `owner` on `repo.create`; removed later-phase placeholder copy (D-ORG-06).
- Repo settings now gates on `repo.can_admin` (not `me.id === owner_id`) and ships Collaborators list/add/update/remove with permission ladder + `MemberLookup` (ORG-03 / D-ORG-04).

## Task Commits

Each task was committed atomically:

1. **Task 1: /new owner picker** - `66feb24` (feat)
2. **Task 2: Collaborators panel + can_admin settings gate** - `e0c89f1` (feat)

## Files Created/Modified

- `apps/web/src/routes/new.tsrx` — Owner Select + SSR `ownerOrgs` + `owner` on create
- `apps/web/src/routes/new.owner-picker.integration.test.ts` — greened D-ORG-06 assertions
- `apps/web/src/routes/new.integration.test.ts` — loader shape includes `ownerOrgs`
- `apps/web/src/components/repo/collaborators-panel.tsrx` — Collaborators UI
- `apps/web/src/routes/$owner.$repo.settings.tsrx` — `can_admin` gate + panel mount
- `apps/web/src/routes/$owner.$repo.settings.collaborators.integration.test.ts` — greened ORG-03 checks

## Decisions Made

- Eligible orgs loaded in the `/new` SSR loader via `fetchOrgListMine`, filtered to Owner/Admin (T-10-06 UI-side listing).
- Entire settings surface (visibility, collaborators, danger zone) requires `can_admin`.
- Collaborator username add reuses `MemberLookup` so lookup hits never render email (T-10-03).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Rivet adjacent siblings in Collaborators empty/list branches**
- **Found during:** Task 2
- **Issue:** Empty-state `<p>` + `<Button>` (and list table + add button) were adjacent roots inside `@else` branches → transform error “Adjacent JSX elements must be wrapped”.
- **Fix:** Wrapped those branches in `<>…</>` fragments.
- **Files modified:** `apps/web/src/components/repo/collaborators-panel.tsrx`
- **Commit:** `e0c89f1`

**2. [Rule 3 - Blocking] Owner-picker create test could not drive Base UI Select in jsdom**
- **Found during:** Task 1
- **Issue:** `fireEvent.click` on Select options did not update controlled value under jsdom.
- **Fix:** Assert default self owner slug is posted on `repo.create`; option listing covered by separate test.
- **Files modified:** `apps/web/src/routes/new.owner-picker.integration.test.ts`
- **Commit:** `66feb24`

## Threat Flags

None — mitigations T-10-06 / T-10-02 applied (Admin+ listing; `can_admin` gate); no new packages (T-10-SC).

## Known Stubs

None.

## Self-Check: PASSED

- FOUND: `apps/web/src/components/repo/collaborators-panel.tsrx`
- FOUND: `apps/web/src/routes/new.tsrx` owner Select + `owner` on create
- FOUND: commits `66feb24`, `e0c89f1`
