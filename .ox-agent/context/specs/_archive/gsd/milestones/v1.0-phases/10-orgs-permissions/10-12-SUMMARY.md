---
phase: 10-orgs-permissions
plan: "12"
subsystem: database
tags: [factory-reset, organizations, acl, docs, rpc-sync, validation]

requires:
  - phase: 10-orgs-permissions
    provides: 0010_orgs_acl tables, Capability ACL, org.*/collaborators RPCs, UI
provides:
  - "factory_reset_instance wipes repositories + organizations (cascades ACL tables)"
  - "ARCHITECTURE/API/CONFIGURATION document polymorphic owners, Capability ACL, PAT∩ACL"
  - "10-VALIDATION Wave 0 closed; nyquist_compliant true; phase gate green"
affects:
  - phase-10-closeout
  - self-host admin factory reset

actuals:
  tokens: 8057
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "factory_reset deletes repositories then organizations before auth wipe (polymorphic owner_id no longer cascades from users)"
    - "Phase docs replace owner-only ACL stub with Capability coalesce + invite closed-signup notes"

key-files:
  created:
    - crates/oxidean-db/tests/factory_reset_orgs.rs
    - .planning/phases/10-orgs-permissions/.tdd/10-12-t1-red-evidence.json
  modified:
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-api/tests/factory_reset_scope.rs
    - docs/ARCHITECTURE.md
    - docs/API.md
    - docs/CONFIGURATION.md
    - .planning/phases/10-orgs-permissions/10-VALIDATION.md

key-decisions:
  - "Wipe all repository rows (user + org) then organizations before users — polymorphic repos no longer FK-cascade from users"
  - "No new org env vars; invites reuse EmailSender + OXIDEAN_PUBLIC_ORIGIN"
  - "Mark nyquist_compliant true after Wave 0 gaps checked and phase gate green"

patterns-established:
  - "T-10-15: factory_reset_instance must DELETE repositories + organizations explicitly"
  - "Phase closeout: docs + VALIDATION + nextest/rpc-sync/web build gate"

requirements-completed: [ORG-01, ORG-02, ORG-03, ORG-04]

coverage:
  - id: D1
    description: "factory_reset_instance clears organizations, members, invites, collaborators, and all repository rows"
    requirement: ORG-01
    verification:
      - kind: unit
        ref: crates/oxidean-db/tests/factory_reset_orgs.rs#factory_reset_wipes_orgs_members_invites_collaborators_and_repos
        status: pass
      - kind: integration
        ref: crates/oxidean-api/tests/factory_reset_scope.rs#factory_reset_wipes_org_acl_and_repository_rows
        status: pass
    human_judgment: false
  - id: D2
    description: "Docs describe Capability ACL, polymorphic owners, invites, PAT∩ACL; owner-only stub removed"
    requirement: ORG-02
    verification:
      - kind: other
        ref: "rg -n 'Capability|organization|collaborator|member_base' docs/ARCHITECTURE.md docs/API.md"
        status: pass
    human_judgment: false
  - id: D3
    description: "Phase 10 automated gate green (org/collab/private/git_smart/pat/coalesce + migration_parity + rpc-sync + web build)"
    requirement: ORG-04
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(org_) | test(collab) | test(repo_private) | test(git_smart) | test(pat_) | test(coalesce)'"
        status: pass
      - kind: other
        ref: "make rpc-sync-check && bun run --cwd apps/web build"
        status: pass
    human_judgment: false

duration: 7min
completed: 2026-09-14
status: complete
plan_head_before: caa3afd4da82c9cdfeb731c155f41dd5574568ea
commits: 3
---

# Phase 10 Plan 12: Factory reset + docs closeout Summary

**Factory reset wipes org ACL + all repos; ARCHITECTURE/API document Capability ACL and PAT∩ACL; Wave 0 VALIDATION closed with green phase gate.**

## Performance

- **Duration:** 7 min
- **Started:** 2026-09-14T01:55:17Z
- **Completed:** 2026-09-14T02:02:41Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- Extended `factory_reset_instance` to `DELETE FROM repositories` then `organizations` (cascades collaborators / members / invites) before auth wipe — fixes orphaned org rows after polymorphic `owner_id` (T-10-15).
- Documented shared-slug owners, Capability coalesce, closed-signup invite accept, and PAT∩ACL; removed ARCHITECTURE owner-only stub language.
- Refreshed `10-VALIDATION.md` (all Wave 0 checkboxes, `nyquist_compliant: true`); phase gate 66 nextest + migration_parity + rpc-sync-check + web build green.

## Task Commits

Each task was committed atomically:

1. **Task 1 RED: factory reset org wipe tests** - `70bb509` (test)
2. **Task 1 GREEN: wipe orgs/repos in factory_reset_instance** - `b486d58` (feat)
3. **Task 2: Docs + VALIDATION + phase gate** - `678783d` (docs)

_Note: TDD Task 1 produced RED + GREEN commits (no REFACTOR)._

## TDD Gate Compliance

| Task | RED | GREEN | REFACTOR | Evidence |
|------|-----|-------|----------|----------|
| T1 | ✓ `70bb509` + RED_EVIDENCE_OK | ✓ `b486d58` | — | `.tdd/10-12-t1-red-evidence.json` |

## Files Created/Modified

- `crates/oxidean-db/tests/factory_reset_orgs.rs` — DB integration: seed org ACL + repos → reset → empty
- `crates/oxidean-api/tests/factory_reset_scope.rs` — RPC factory_reset also clears org/repo rows
- `crates/oxidean-db/src/lib.rs` — `factory_reset_instance` deletes repositories + organizations first
- `docs/ARCHITECTURE.md` — Capability ACL + orgs section; PAT∩ACL
- `docs/API.md` — `org.*` / collaborators / `user.lookup` / factory_reset docs
- `docs/CONFIGURATION.md` — no new org env vars; invite mail reuse
- `.planning/phases/10-orgs-permissions/10-VALIDATION.md` — Wave 0 closed
- `.planning/phases/10-orgs-permissions/.tdd/10-12-t1-red-evidence.json` — RED evidence

## Decisions Made

- Explicit `DELETE` order: repositories → organizations → auth tables (polymorphic repos no longer cascade from users).
- Docs-only closeout for CONFIGURATION (invite/email already covered by existing sender + public origin).
- Set `nyquist_compliant: true` as this plan's VALIDATION refresh (phase gate green).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Web build command flag**
- **Found during:** Task 2 (phase gate)
- **Issue:** Plan verify used `bun --cwd apps/web run build`, which prints Bun CLI help instead of running the package script.
- **Fix:** Ran `bun run --cwd apps/web build` (EXIT 0).
- **Files modified:** none (verify invocation only)
- **Verification:** Vite build completed successfully
- **Committed in:** n/a (no code change)

---

**Total deviations:** 1 auto-fixed (blocking verify invocation)
**Impact on plan:** None on shipped artifacts; gate still proven green.

## Issues Encountered

None beyond the Bun cwd flag above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 10 plans 00–13 complete; ORG-01…04 ship gate green.
- Ready for `/gsd-verify-work` / milestone advance; no teams in this phase.

## Self-Check: PASSED

- FOUND: `crates/oxidean-db/tests/factory_reset_orgs.rs`
- FOUND: `crates/oxidean-db/src/lib.rs` factory_reset org wipe
- FOUND: `docs/ARCHITECTURE.md` Capability ACL (no owner-only stub)
- FOUND: `10-12-SUMMARY.md` (this file)
- FOUND commits: `70bb509`, `b486d58`, `678783d`

---
*Phase: 10-orgs-permissions*
*Completed: 2026-09-14*
