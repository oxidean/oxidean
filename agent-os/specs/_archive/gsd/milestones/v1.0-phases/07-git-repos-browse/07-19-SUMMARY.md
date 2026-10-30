---
phase: 07-git-repos-browse
plan: "19"
subsystem: git
tags: [git-cli, branch-mutate, soft-protect, argv-injection, tdd, gap-closure]
requires:
  - phase: 07-git-repos-browse
    provides: "repo.branchCreate/Rename/Delete + default-branch soft-protect (07-07)"
provides:
  - "validate_treeish rejects leading-hyphen refs (CR-02 foundation)"
  - "branch_create/rename/delete pass end-of-options before user operands"
  - "repo.branchCreate rejects option-like names with repo.invalid_ref; default soft-protect intact"
affects: [07-20-archive-argv, git-mutate-security]
actuals:
  tokens: 1472
  tasks: 2
  commits: 3
plan_head_before: 83cb3da37cb969f4c14d30167770b587ea9f76c8
tech-stack:
  added: []
  patterns:
    - "Defense in depth: API option-like reject + validate_treeish leading- + git -- before operands"
key-files:
  created:
    - .planning/phases/07-git-repos-browse/.tdd/07-19-red-evidence.json
  modified:
    - crates/oxidean-git/src/cli.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/tests/repo_branch_soft_protect.rs
key-decisions:
  - "reject_option_like_branch treats any leading- hyphen as repo.invalid_ref (covers -d/-D/-m/-M/-f and case variants)"
  - "Known flags -m/-D stay before --; user from/to/name always after end-of-options"
patterns-established:
  - "CR-02: never place user-controlled branch tokens in argv option position"
requirements-completed: [GIT-06]
coverage:
  - id: D1
    description: "Owner branchCreate(-D) fails closed; default branch still soft-protected"
    requirement: GIT-06
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_branch_soft_protect.rs#repo_branch_create_rejects_option_like_name_leaves_default_intact"
        status: pass
    human_judgment: false
  - id: D2
    description: "validate_treeish rejects trimmed refs beginning with -; branch_* use -- before operands"
    requirement: GIT-06
    verification:
      - kind: unit
        ref: "crates/oxidean-git/src/cli.rs#validate_treeish + branch_create/rename/delete argv"
        status: pass
      - kind: integration
        ref: "cargo test -p oxidean-api --test repo_branch_soft_protect"
        status: pass
    human_judgment: false
duration: 2min
completed: "2026-09-12"
status: complete
---

# Phase 07 Plan 19: CR-02 branchCreate option injection Summary

**Hardened validate_treeish + branch argv so `repo.branchCreate(-D)` cannot force-delete the soft-protected default branch (CR-02 / D-28)**

## Performance

- **Duration:** 2 min
- **Started:** 2026-09-12T19:43:42Z
- **Completed:** 2026-09-12T19:46:15Z
- **Tasks:** 2
- **Files modified:** 3 (+ RED evidence)

## Accomplishments

- Injection regression: `repo_branch_create_rejects_option_like_name_leaves_default_intact` proves `branchCreate(-D)` returns `repo.invalid_ref` and soft-protect still observes `main`
- `validate_treeish` rejects leading `-`; `branch_create` / `branch_rename` / `branch_delete` pass `--` before user operands
- API `reject_option_like_branch` fails closed before git (T-07-GC19-01/02/03)

## Task Commits

Each task was committed atomically:

1. **Task 1 RED:** `4e5a06a` (test) — failing injection regression
2. **Task 1 GREEN:** `f0b9fda` (feat) — validate_treeish + branch_create `--` + API reject
3. **Task 2:** `bcb5cf2` (feat) — rename/delete end-of-options

_Note: TDD tasks may have multiple commits (test → feat → refactor)_

## TDD Gate Compliance

| Gate | Commit | Status |
|------|--------|--------|
| RED | `4e5a06a` | Pass — `.tdd/07-19-red-evidence.json` → `RED_EVIDENCE_OK` (`target_test_failed`) |
| GREEN | `f0b9fda` | Pass — exact injection test exits 0 |
| REFACTOR | — | Skipped — minimal GREEN sufficient |
| Task 2 expand | `bcb5cf2` | Pass — full `repo_branch_soft_protect` binary green |

## Files Created/Modified

- `crates/oxidean-api/tests/repo_branch_soft_protect.rs` — CR-02 injection regression
- `crates/oxidean-git/src/cli.rs` — leading-`-` reject; `--` on branch create/rename/delete
- `crates/oxidean-api/src/repo/mod.rs` — `reject_option_like_branch` on create branch/start
- `.planning/phases/07-git-repos-browse/.tdd/07-19-red-evidence.json` — RED gate record

## Decisions Made

- Leading `-` is sufficient reserved-token coverage at API (no separate allowlist of `-d`/`-D`/…); matches validate_treeish
- Intentional `-m` / `-D` flags remain before `--`; operands never slide into option position

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- CR-02 closed; ready for 07-20 (archive `--` + HTTP validators / CR-01)
- Soft-protect suite remains the regression harness for GIT-06 / D-28

## Self-Check: PASSED

- FOUND: `crates/oxidean-git/src/cli.rs`, `crates/oxidean-api/src/repo/mod.rs`, `crates/oxidean-api/tests/repo_branch_soft_protect.rs`, `.tdd/07-19-red-evidence.json`
- FOUND commits: `4e5a06a`, `f0b9fda`, `bcb5cf2`
- Full `repo_branch_soft_protect` binary: 4 passed

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*
