---
phase: 07-git-repos-browse
plan: "01"
subsystem: infra
tags: [git-backend, cli-git, d-14, d-32, d-33, coverage, requirements]

requires:
  - phase: 07-git-repos-browse
    provides: Wave 0 oxidean-git stubs + 07-CONTEXT D-14/D-32/D-33 locks
provides:
  - "Confirmed D-14 public URL scheme /{owner}/{repo}"
  - "Confirmed D-33 fail-boot when git missing or < 2.5"
  - "Amended GIT-09/GIT-10 CLI-primary GitBackend wording"
  - "PROJECT.md git engine mental model aligned to CliGitBackend"
  - "07-COVERAGE.md GitBackend identity + INTEGRATE/OPT-OUT matrix"
affects:
  - 07-02 schema + validators
  - 07-12 tracer GitBackend CLI create
  - 07-17 fail-boot git gate + Dockerfile

actuals:
  tokens: 1436
  tasks: 3
  commits: 6

plan_head_before: 2825b21174228525d65e7cdd32b4b379f9af39bb

tech-stack:
  added: []
  patterns:
    - "GitBackend is the identity noun; CliGitBackend shipped adapter; GixGitBackend future"
    - "One-way doors (URL scheme, git boot floor) confirmed before tracer implementation"

key-files:
  created: []
  modified:
    - .planning/REQUIREMENTS.md
    - .planning/PROJECT.md
    - .planning/phases/07-git-repos-browse/07-COVERAGE.md

key-decisions:
  - "D-14: owner_repo_path — public URLs /{owner}/{repo} with reserved-name denylist"
  - "D-33: fail_boot_git — refuse API boot if git missing or < 2.5"
  - "D-32: CLI-primary GitBackend; promote GitBackend noun; demote engines to adapters"
  - "assumption_delta: promote GitBackend as identity noun (Cli now, Gix later)"

patterns-established:
  - "Planning docs must not claim gitoxide-first for Phase 7 implementation"
  - "07-COVERAGE.md is the system-git capability INTEGRATE/OPT-OUT gate"

requirements-completed: [GIT-09, GIT-10]

coverage:
  - id: D1
    description: "Human confirmed D-14 public URL scheme /{owner}/{repo} (owner_repo_path)"
    requirement: GIT-05
    verification: []
    human_judgment: true
    rationale: "One-way public URL contract — human decision checkpoint"
  - id: D2
    description: "Human confirmed D-33 fail-boot when git missing or < 2.5 (fail_boot_git)"
    requirement: GIT-09
    verification: []
    human_judgment: true
    rationale: "One-way operator boot contract — human decision checkpoint"
  - id: D3
    description: "GIT-09/GIT-10 and PROJECT amended to CLI-primary GitBackend; COVERAGE matrix present"
    requirement: GIT-09
    verification:
      - kind: other
        ref: "rg -n 'CliGitBackend|system .git. CLI|GitBackend' .planning/REQUIREMENTS.md .planning/ROADMAP.md .planning/phases/07-git-repos-browse/07-COVERAGE.md && ! rg -n 'gitoxide-first|primarily via gitoxide' .planning/REQUIREMENTS.md .planning/ROADMAP.md"
        status: pass
    human_judgment: false

duration: 1min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 01: One-way Gates + REQUIREMENTS Amend Summary

**Locked D-14 `/{owner}/{repo}` and D-33 fail-boot git≥2.5; amended GIT-09/10 + PROJECT to CLI-primary `GitBackend` with `07-COVERAGE` matrix**

## Performance

- **Duration:** 1 min
- **Started:** 2026-09-12T16:54:07Z
- **Completed:** 2026-09-12T16:54:50Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Confirmed one-way doors: `owner_repo_path` (D-14) and `fail_boot_git` (D-33)
- Rewrote GIT-09/GIT-10 to system `git` CLI behind `GitBackend` (`CliGitBackend` now; `GixGitBackend` later)
- Aligned PROJECT.md Active / Constraints / Key Decisions away from gitoxide-preferred
- Ensured `07-COVERAGE.md` documents GitBackend identity + INTEGRATE/OPT-OUT table

## Task Commits

Each task was committed atomically:

1. **Task 1: Confirm public URL scheme /{owner}/{repo} (D-14)** — checkpoint decision — selected `owner_repo_path` (no code commit)
2. **Task 2: Confirm fail-boot when git missing or < 2.5 (D-33)** — checkpoint decision — selected `fail_boot_git` (no code commit)
3. **Task 3: Amend GIT-09/ROADMAP/PROJECT + ensure COVERAGE.md** — `d3e3c23` (docs)

**Plan metadata:** `6f4c5d6` (docs: complete plan)

## Files Created/Modified

- `.planning/REQUIREMENTS.md` — GIT-09/GIT-10 CLI-primary + swappable GitBackend wording
- `.planning/PROJECT.md` — mental model, constraints, key decision for CLI-primary GitBackend
- `.planning/phases/07-git-repos-browse/07-COVERAGE.md` — identity-noun clarification (Cli now / Gix later)

## Decisions Made

- **owner_repo_path** — Adopt `/{owner}/{repo}` with reserved-name denylist (D-14)
- **fail_boot_git** — Fail boot if `git` missing or older than 2.5 (D-33)
- **GitBackend promote** — Identity noun is `GitBackend`; engines are adapters only (assumption_delta)

## Deviations from Plan

### Auto-fixed Issues

None - plan executed exactly as written.

**Note:** ROADMAP Phase 7 Goal and success criterion 4 already used CLI-primary + `GitBackend` language before Task 3 (planner draft); no ROADMAP file edit was required in the task commit. Verify still passes (`! rg gitoxide-first|primarily via gitoxide` on REQUIREMENTS/ROADMAP).

---

**Total deviations:** 0 auto-fixed
**Impact on plan:** N/A — ROADMAP already matched D-32

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Tracer (07-12) and fail-boot ops (07-17) may implement confirmed D-14/D-33 contracts
- Schema plan 07-02 can proceed with GitBackend as the seam noun
- No planning artifact still mandates gitoxide-first for Phase 7

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*

## Self-Check: PASSED
