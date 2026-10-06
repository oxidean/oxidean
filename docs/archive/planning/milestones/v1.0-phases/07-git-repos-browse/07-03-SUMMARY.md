---
phase: 07-git-repos-browse
plan: "03"
subsystem: api
tags: [git, templates, spdx, gitignore, /new, repo.create, D-02, D-03, D-04, D-08, D-12]
requires:
  - phase: 07-git-repos-browse
    provides: "repo.create tracer + CliGitBackend init_bare (07-12); /new create form tracer (07-13)"
provides:
  - "Vendored stack/gitignore/license assets + initial-commit seeder"
  - "/new template pickers, full SPDX list, D-12 duplicate copy, D-08 visibility default"
  - "docs/guides/stack-presets.md community pack contribution path"
affects:
  - 07-04-home-defaults
  - 07-15-code-browse
  - future marketplace UI
actuals:
  tokens: 19969
  tasks: 3
  commits: 8
commits: 8
plan_head_before: 7f8c09afcaf9043e1f1411f4d37a4ee0ca6519bf
tech-stack:
  added:
    - "include_dir (API asset embed)"
    - "spdx-license-list@6.12.0"
    - "tempfile (git seed worktree)"
  patterns:
    - "Allowlist pack IDs from catalog.json; reject path traversal (T-07-09)"
    - "Any template selection → single Initial commit; all none → bare empty"
    - "Native <select> for full SPDX (custom Select stalls on ~500 items)"
key-files:
  created:
    - crates/oxidean-api/src/repo/templates.rs
    - crates/oxidean-api/assets/stack-presets/
    - crates/oxidean-api/assets/gitignore/
    - crates/oxidean-api/assets/licenses/
    - apps/web/src/lib/spdx-licenses.ts
    - docs/guides/stack-presets.md
  modified:
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-git/src/cli.rs
    - crates/oxidean-core/src/repo_types.rs
    - apps/web/src/routes/new.tsrx
    - packages/api-client/src/index.ts
    - docs/ARCHITECTURE.md
key-decisions:
  - "License picker uses native select for full SPDX performance; stack/gitignore keep UI Select"
  - "Unknown SPDX IDs seed SPDX-License-Identifier stub LICENSE when text not vendored"
  - "repo.createDefaults RPC supplies default_visibility + stack/gitignore catalogs"
patterns-established:
  - "Embedded assets under crates/oxidean-api/assets/ via include_dir"
  - "GitBackend::seed_commit for template initial commits"
requirements-completed: [GIT-01]
coverage:
  - id: D1
    description: "Template create seeds Initial commit with stack/license/gitignore files"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_create.rs#repo_create_with_templates_seeds_initial_commit"
        status: pass
    human_judgment: false
  - id: D2
    description: "All-none templates leave empty bare (Quick setup path)"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_create.rs#repo_create_all_none_templates_leaves_empty_bare"
        status: pass
    human_judgment: false
  - id: D3
    description: "/new shows Stack/License/.gitignore pickers; duplicate uses exact D-12 copy"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "apps/web/src/routes/new.integration.test.ts"
        status: pass
    human_judgment: false
  - id: D4
    description: "Community pack guide documents in-repo path; marketplace UI deferred"
    requirement: GIT-01
    verification:
      - kind: other
        ref: "docs/guides/stack-presets.md + rg pack|PR|marketplace"
        status: pass
    human_judgment: false
duration: 10min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 03: Create Templates Summary

**/new ships stack + SPDX + gitignore pickers with allowlisted asset seeding and D-12 duplicate inline errors**

## Performance

- **Duration:** 10 min
- **Started:** 2026-09-12T17:22:05Z
- **Completed:** 2026-09-12T17:32:13Z
- **Tasks:** 3
- **Files modified:** 101 (incl. vendored asset tree)

## Accomplishments

- Vendored stack presets, gitignore catalog, and common SPDX texts; `repo.create` seeds an Initial commit when any template is selected
- `/new` pickers + `repo.createDefaults` for visibility default; exact D-12 duplicate field copy
- Community contribution guide at `docs/guides/stack-presets.md` (no marketplace UI)

## Task Commits

1. **Task 1: Vendored catalogs + initial-commit seeder** - `d2b00e6` (feat)
2. **Task 2: /new pickers + SPDX + duplicate inline error** - `cb7d4c8` (feat)
3. **Task 3: Stack presets contribution guide** - `6c746ba` + `2405d3e` (docs)

**Plan metadata:** `1c5eaae` (docs: complete plan)

## Files Created/Modified

- `crates/oxidean-api/src/repo/templates.rs` — catalog resolve + seed file assembly
- `crates/oxidean-api/assets/*` — stack / gitignore / license packs
- `crates/oxidean-git/src/cli.rs` — `seed_commit` via temp worktree + push
- `apps/web/src/routes/new.tsrx` — full create UX
- `apps/web/src/lib/spdx-licenses.ts` — SPDX option list
- `docs/guides/stack-presets.md` — PR contribution path

## Decisions Made

- Native `<select>` for License (full SPDX) to avoid mounting hundreds of custom Select items
- Stub LICENSE for SPDX IDs without vendored body; vendored texts for common licenses
- `repo.createDefaults` thin RPC for D-08 + catalog metadata

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Native select for SPDX performance**
- **Found during:** Task 2
- **Issue:** Custom Select with full SPDX (~500 items) hung page/tests
- **Fix:** Native `<select>` for License; keep UI Select for smaller stack/gitignore lists
- **Files modified:** `apps/web/src/routes/new.tsrx`
- **Verification:** vitest `/new` integration + `bun run build`
- **Committed in:** `cb7d4c8`

**2. [Rule 3 - Blocking] ARCHITECTURE cross-link missed first Task 3 commit**
- **Found during:** Task 3
- **Issue:** `docs/ARCHITECTURE.md` link left unstaged
- **Fix:** Follow-up docs commit with cross-link
- **Files modified:** `docs/ARCHITECTURE.md`
- **Verification:** file contains stack-presets guide link
- **Committed in:** `2405d3e`

---

**Total deviations:** 2 auto-fixed (1 missing critical UX, 1 blocking staging)
**Impact on plan:** Necessary for correctness/performance; no scope creep.

## Issues Encountered

- Integration tests needed 20s timeout due to Octane transform cost of `/new` with Select + SPDX imports

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- GIT-01 create surface complete for templates/visibility/errors
- Ready for home dashboard / browse plans consuming seeded repos

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/src/repo/templates.rs`
- FOUND: `docs/guides/stack-presets.md`
- FOUND: `apps/web/src/lib/spdx-licenses.ts`
- FOUND: commits `d2b00e6`, `cb7d4c8`, `6c746ba`, `2405d3e`

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*
