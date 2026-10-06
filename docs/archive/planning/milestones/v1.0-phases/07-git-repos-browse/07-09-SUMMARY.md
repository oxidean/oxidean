---
phase: 07-git-repos-browse
plan: "09"
subsystem: api
tags: [git, visibility, soft-delete, settings, rpc, nextest, tsrx]

requires:
  - phase: 07-git-repos-browse
    provides: "repositories.deleted_at + owner ACL mutate; AlertDialog from branches UI"
provides:
  - "repo.updateVisibility / repo.softDelete owner-only RPCs (D-26 / D-35)"
  - "Owner settings UI with Save visibility toast + typed soft-delete dialog"
affects:
  - 07-git-repos-browse
  - 07-10-purge-job

actuals:
  tokens: 11309
  tasks: 2
  commits: 3

plan_head_before: 58c0790f9431376923076dfda12123f0c757b979

tech-stack:
  added: []
  patterns:
    - "Owner mutate via resolve_repo_for_owner_mutate; soft-delete confirmName server-side match"
    - "Soft-delete sets deleted_at only — bare dir remains until 07-10 purge"

key-files:
  created:
    - crates/oxidean-api/tests/repo_settings_visibility_delete.rs
    - apps/web/src/routes/$owner.$repo.settings.tsrx
    - .planning/phases/07-git-repos-browse/.tdd/07-09-red-evidence.json
  modified:
    - crates/oxidean-core/src/repo_types.rs
    - crates/oxidean-db/src/repositories.rs
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts
    - apps/web/src/components/repo/repo-chrome.tsrx
    - apps/web/src/routeTree.gen.ts

key-decisions:
  - "RPC names camelCase repo.updateVisibility / repo.softDelete to match existing repo.* surface"
  - "Visibility UI uses Public/Private segmented buttons (same as /new) instead of new radio-group wrapper"
  - "ASSUME soft-delete retention default 14 days until purge job in 07-10"

patterns-established:
  - "repo.confirm_mismatch for typed name soft-delete; disk purge never synchronous in this plan"

requirements-completed: [GIT-01, GIT-08]

coverage:
  - id: D1
    description: "Owner toggles private↔public via repo.updateVisibility"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_settings_visibility_delete.rs#repo_settings_owner_toggles_visibility"
        status: pass
    human_judgment: false
  - id: D2
    description: "Non-owner visibility update → repo.not_found (T-07-23)"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_settings_visibility_delete.rs#repo_settings_non_owner_update_visibility_not_found"
        status: pass
    human_judgment: false
  - id: D3
    description: "Soft-delete with confirmName; get/list omit; bare dir remains (D-35)"
    requirement: GIT-08
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_settings_visibility_delete.rs#repo_settings_soft_delete_hides_row_keeps_disk"
        status: pass
    human_judgment: false
  - id: D4
    description: "Owner settings UI: Save visibility toast + Delete repository dialog copy"
    requirement: GIT-01
    verification:
      - kind: other
        ref: "bun run build (apps/web); rg Save visibility|Delete repository|Only you can see"
        status: pass
    human_judgment: true
    rationale: "Visual toast and dialog UX need human confirmation beyond build + string presence"

duration: 6min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 09: Repo settings visibility & soft-delete Summary

**Owner-only `repo.updateVisibility` / `repo.softDelete` with settings UI (Save visibility toast + typed danger-zone delete; disk purge deferred)**

## Performance

- **Duration:** 6 min
- **Started:** 2026-09-12T18:44:55Z
- **Completed:** 2026-09-12T18:50:15Z
- **Tasks:** 2
- **Files modified:** 13

## Accomplishments

- Owner can flip public/private; non-owner gets unified `repo.not_found`
- Soft-delete requires exact `confirmName`; row hidden from get/listMine; bare `.git` stays on disk
- Settings route `/{owner}/{repo}/settings` with UI-SPEC E8/E11 copy and toast **Visibility updated.**

## TDD Gate Compliance

| Gate | Commit | SHA |
|------|--------|-----|
| RED | `test(07-09): add failing tests for visibility toggle and soft-delete` | `8640f07` |
| GREEN | `feat(07-09): implement updateVisibility and softDelete RPCs` | `0d8f671` |
| REFACTOR | skipped (no cleanup needed) | — |

RED evidence: `.planning/phases/07-git-repos-browse/.tdd/07-09-red-evidence.json` — `RED_EVIDENCE_OK` / `target_test_failed` for `repo_settings_owner_toggles_visibility`.

## Task Commits

1. **Task 1 RED:** failing `repo_settings_*` tests — `8640f07` (test)
2. **Task 1 GREEN:** RPCs + DB + rpc-gen — `0d8f671` (feat)
3. **Task 2:** Settings UI — `4f3cfbb` (feat)

## Files Created/Modified

- `crates/oxidean-api/tests/repo_settings_visibility_delete.rs` — visibility + soft-delete integration tests
- `crates/oxidean-db/src/repositories.rs` — `update_visibility` / `soft_delete`
- `crates/oxidean-api/src/repo/mod.rs` — RPC handlers
- `apps/web/src/routes/$owner.$repo.settings.tsrx` — owner settings surface
- `packages/api-client/src/index.ts` — generated client methods

## Decisions Made

- CamelCase procedure names aligned with `repo.branchCreate` et al.
- Reused `/new` Public/Private button pattern instead of adding shadcn radio-group (Badge already present; Switch/radios both allowed by UI-SPEC E8)

## Deviations from Plan

### Auto-fixed Issues

None - plan executed as written aside from intentional UI control choice below.

### Intentional deviations

**1. [Scope] Skipped new `radio-group` shadcn wrapper**
- **Found during:** Task 2
- **Issue:** Plan listed `radio-group.tsx`; Badge already exists; UI-SPEC E8 allows Switch or radios
- **Fix:** Used same Public/Private segmented buttons as `/new` (no new package/component)
- **Files modified:** `apps/web/src/routes/$owner.$repo.settings.tsrx`
- **Verification:** build green; acceptance string rg passes
- **Committed in:** `4f3cfbb`

---

**Total deviations:** 1 intentional (UI control reuse)
**Impact on plan:** No behavior gap; threat T-07-SC satisfied by not adding registry packages.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Ready for 07-10 disk purge job against soft-deleted rows
- Settings nav only shown on settings page today; other chrome call sites can pass `showSettings` for owners later

## Self-Check: PASSED

- FOUND: `apps/web/src/routes/$owner.$repo.settings.tsrx`
- FOUND: `crates/oxidean-api/tests/repo_settings_visibility_delete.rs`
- FOUND: commits `8640f07`, `0d8f671`, `4f3cfbb`

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*
