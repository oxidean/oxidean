---
phase: 07-git-repos-browse
plan: "04"
subsystem: ui
tags: [dashboard, repo-list, default-branch, default-visibility, tanstack, octane]

requires:
  - phase: 07-git-repos-browse
    provides: "repo.create + createDefaults (07-03/07-13); repositories schema (07-02)"
provides:
  - "SignedInHome forge dashboard IA (D-13)"
  - "repo.listMine owner-only list sorted by updated_at"
  - "Account default_branch settings (D-09)"
  - "Instance default_visibility admin settings (D-08)"
affects: [07-05, 07-06, 07-15, browse-home]

actuals:
  tokens: 21145
  tasks: 2
  commits: 3
plan_head_before: 7457cd70e7fb8562608d5510d260ed831dd3f88a

tech-stack:
  added: []
  patterns:
    - "repo.listMine returns { repos: RepoPublic[] }; soft-deleted excluded; updated_at DESC"
    - "UserPublic.default_branch + optional UpdateProfileRequest.default_branch"
    - "AuthSettingsPublic.default_visibility round-trips via admin.auth.update_settings"

key-files:
  created:
    - apps/web/src/components/ui/badge.tsrx
    - apps/web/src/lib/relative-time.ts
    - .planning/phases/07-git-repos-browse/.evidence/07-04-t1-red.json
  modified:
    - apps/web/src/components/signed-in-home.tsrx
    - apps/web/src/components/signed-in-home.integration.test.ts
    - apps/web/src/routes/index.tsrx
    - apps/web/src/routes/settings/profile.tsrx
    - apps/web/src/routes/admin/auth.tsrx
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-db/src/repositories.rs
    - crates/oxidean-core/src/auth_types.rs
    - packages/api-client/src/index.ts

key-decisions:
  - "listMine requires session only (not email verified) so unverified users still see their list"
  - "Empty hero keeps a second New repository CTA per UI-SPEC; tests use getAllByRole"
  - "default_branch saved via optional UpdateProfileRequest field + dedicated Save default branch UI"

patterns-established:
  - "Home document title Repositories · Oxidean when signed-in loader tree"
  - "Visibility Badge + Lock icon for private rows"

requirements-completed: [GIT-01]

coverage:
  - id: D1
    description: "Signed-in home dashboard lists repos with empty hero and activity placeholder"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "apps/web/src/components/signed-in-home.integration.test.ts#empty list shows Create your first repository hero and activity placeholder"
        status: pass
    human_judgment: false
  - id: D2
    description: "repo.listMine returns caller-owned non-deleted repos sorted by updated_at"
    requirement: GIT-01
    verification:
      - kind: other
        ref: "rg listMine crates/oxidean-api/src/rpc.rs + cargo nextest repo_create"
        status: pass
    human_judgment: false
  - id: D3
    description: "Account default branch name settings round-trip (D-09)"
    verification:
      - kind: other
        ref: "rg 'Default branch name|Save default branch' apps/web/src/routes/settings/profile.tsrx"
        status: pass
    human_judgment: false
  - id: D4
    description: "Sys-admin instance default_visibility settings (D-08)"
    verification:
      - kind: integration
        ref: "cargo nextest -p oxidean-api admin_auth_settings"
        status: pass
    human_judgment: false

duration: 11min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 04: Dashboard + defaults Summary

**Forge signed-in home with `repo.listMine`, empty/activity states, plus account default branch and instance default visibility settings.**

## Performance

- **Duration:** 11 min
- **Started:** 2026-09-12T17:34:03Z
- **Completed:** 2026-09-12T17:44:45Z
- **Tasks:** 2
- **Files modified:** 27

## Accomplishments

- Replaced signed-in home stub with UI-SPEC dashboard: Your repositories, Badge rows, empty hero, activity placeholder only
- Added `repo.listMine` (owner-only, soft-delete excluded, `updated_at` DESC) + document title `Repositories · Oxidean`
- Wired D-09 default branch on profile and D-08 default visibility on admin auth settings (consumed by existing `repo.create`)

## Task Commits

Each task was committed atomically:

1. **Task 1 (TDD RED): failing SignedInHome dashboard IA tests** - `08b74a2` (test)
2. **Task 1 (TDD GREEN): repo.listMine + SignedInHome dashboard** - `6614768` (feat)
3. **Task 2: Account default branch + instance default visibility** - `881f056` (feat)

## TDD Gate Compliance

| Gate | Commit | Evidence |
|------|--------|----------|
| RED | `08b74a2` | `.evidence/07-04-t1-red.json` → `RED_EVIDENCE_OK` (`empty list shows Create your first repository hero and activity placeholder`) |
| GREEN | `6614768` | `vitest signed-in-home.integration.test.ts` 5/5 pass + `bun run build` |
| REFACTOR | — | skipped (no cleanup needed) |

## Files Created/Modified

- `apps/web/src/components/signed-in-home.tsrx` — dashboard IA
- `apps/web/src/components/ui/badge.tsrx` — Public/Private badges
- `crates/oxidean-api/src/repo/mod.rs` — `list_mine`
- `crates/oxidean-db/src/repositories.rs` — `list_by_owner`
- `apps/web/src/routes/settings/profile.tsrx` — Default branch name section
- `apps/web/src/routes/admin/auth.tsrx` — Default repository visibility select
- `packages/api-client/src/index.ts` — rpc-gen sync

## Decisions Made

- `listMine` gated on session only so unverified users can browse their own repos while create stays gated
- Empty state retains header + hero New repository CTAs; integration tests assert all matching roles
- `default_branch` is an optional field on `user.update_profile` with a dedicated Save CTA

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Multiple New repository roles broke CTA assertions**
- **Found during:** Task 1 GREEN
- **Issue:** Empty hero adds a second New repository control; `getByRole` threw multiple-elements
- **Fix:** Tests use `getAllByRole` and assert each CTA
- **Files modified:** `apps/web/src/components/signed-in-home.integration.test.ts`
- **Commit:** `6614768`

## Threat Flags

None — listMine returns only caller-owned repos (T-07-11); default_visibility stays behind existing admin gate (T-07-12); no new npm packages (T-07-SC).

## Self-Check: PASSED

- Created files found; RED/GREEN/Task2 commits present (`08b74a2`, `6614768`, `881f056`)
