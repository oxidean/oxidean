---
phase: 07-git-repos-browse
plan: "00"
subsystem: testing
tags: [wave0, nyquist, oxidean-git, git-01, git-05, nextest, dialect]

requires:
  - phase: 06-self-host-admin-bootstrap
    provides: auth integration harness + support::unlock_signup + require_verified patterns
provides:
  - "Wave 0 RED oxidean-git version-gate + archive-format stubs"
  - "Wave 0 RED repo_* API integration stubs (create/ACL/branch/fs)"
  - "Wave 0 dialect_repositories stub for 0007_repositories"
affects:
  - 07-01 REQUIREMENTS amend / docs
  - 07-02 schema migration 0007
  - 07-03+ CliGitBackend and repo RPC greens

actuals:
  tokens: 4634
  tasks: 1
  commits: 2

plan_head_before: 7bf483ad784ef014c0243b1637c0e710f4c1a9cc

tech-stack:
  added:
    - crates/oxidean-git (workspace member scaffold)
  patterns:
    - "Wave 0 intentional RED stubs with compile-safe placeholders until git/RPC land"
    - "repo_* nextest filters mirror VALIDATION.md Wave 0 checklist"

key-files:
  created:
    - crates/oxidean-git/Cargo.toml
    - crates/oxidean-git/src/lib.rs
    - crates/oxidean-git/src/version.rs
    - crates/oxidean-api/tests/repo_create.rs
    - crates/oxidean-api/tests/repo_private_404.rs
    - crates/oxidean-api/tests/repo_branch_soft_protect.rs
    - crates/oxidean-api/tests/repo_fs_layout.rs
    - crates/oxidean-db/tests/dialect_repositories.rs
  modified:
    - Cargo.toml
    - Cargo.lock

key-decisions:
  - "Wave 0 is RED-only — no CliGitBackend or repo RPC handlers; later 07-xx plans turn stubs green"
  - "oxidean-git exports parse_git_version/assert_git_version placeholders that Err until implementation"
  - "API stubs reuse auth_verify_gate harness patterns (support::unlock_signup, RPC cookie session)"

patterns-established:
  - "Nyquist Wave 0 for Phase 7: failing nextest paths exist before git forge implementation waves"
  - "GIT-09/10 documented in oxidean-git lib docs: Cli primary, Gix deferred, trait is the seam"

requirements-completed: [GIT-01, GIT-05, GIT-06, GIT-07, GIT-08, GIT-09, GIT-10]

coverage:
  - id: D1
    description: "oxidean-git workspace member with git_version_gate + git_archive_formats RED unit stubs"
    requirement: GIT-09
    verification:
      - kind: unit
        ref: "cargo check -p oxidean-git && cargo nextest list -p oxidean-git -E 'test(git_version_gate) | test(git_archive_formats)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "API Wave 0 stubs for verified create, email_unverified, duplicate, private 404, branch soft-protect, fs layout"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(repo_create) | test(repo_private) | test(repo_branch) | test(repo_fs)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "dialect_repositories expects 0007_repositories + default_branch / default_visibility"
    requirement: GIT-08
    verification:
      - kind: integration
        ref: "rg -n '0007_repositories|default_branch|default_visibility' crates/oxidean-db/tests/dialect_repositories.rs"
        status: pass
    human_judgment: false

duration: 3min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 00: Wave 0 Git Nyquist Stubs Summary

**Failing oxidean-git version/archive stubs plus repo_* API and dialect_repositories RED targets for Phase 7 forge surfaces**

## Performance

- **Duration:** 3 min
- **Started:** 2026-09-12T16:43:33Z
- **Completed:** 2026-09-12T16:47:12Z
- **Tasks:** 1
- **Files modified:** 10

## Accomplishments

- Added `crates/oxidean-git` workspace member with `parse_git_version` / `assert_git_version` placeholders and RED `git_version_gate` / `git_archive_formats` unit tests
- API Wave 0 stubs cover verified `repo.create`, `auth.email_unverified`, duplicate name, private→`repo.not_found`, default-branch soft-protect, and `{repos_dir}/{owner}/{name}.git` layout
- `dialect_repositories` asserts tri-dialect `0007_repositories` plus `default_branch` / `default_visibility` column presence

## Task Commits

Each task was committed atomically:

1. **Task 1: oxidean-git + API + dialect Wave 0 stubs** - `c8f39c4` (test)
2. **Follow-up: Cargo.lock for workspace member** - `3e4e7fe` (chore) — Rule 3 lockfile inclusion

_Note: Wave 0 is RED-only by design — GREEN/REFACTOR belong to later plans._

## Files Created/Modified

- `crates/oxidean-git/` — new crate scaffold (lib + version placeholders)
- `Cargo.toml` / `Cargo.lock` — workspace member wiring
- `crates/oxidean-api/tests/repo_create.rs` — GIT-01 create/unverified/duplicate RED cases
- `crates/oxidean-api/tests/repo_private_404.rs` — D-25 identical not_found RED case
- `crates/oxidean-api/tests/repo_branch_soft_protect.rs` — D-28 soft-protect RED cases
- `crates/oxidean-api/tests/repo_fs_layout.rs` — GIT-08 bare path RED case
- `crates/oxidean-db/tests/dialect_repositories.rs` — 0007 schema presence RED cases

## Decisions Made

- Wave 0 is RED-only — no CliGitBackend or repo RPC handlers; later 07-xx plans turn stubs green
- Version helpers intentionally return Err placeholders so `git_version_gate` fails until implemented
- API stubs mirror `auth_verify_gate` harness (temp sqlite + signup/login cookies)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Committed Cargo.lock after adding workspace member**
- **Found during:** Task 1 (post-commit status check)
- **Issue:** First task commit omitted `Cargo.lock` update required for the new `oxidean-git` member
- **Fix:** Follow-up `chore(07-00)` commit including lockfile; removed empty `[dev-dependencies]` from crate manifest
- **Files modified:** `Cargo.lock`, `crates/oxidean-git/Cargo.toml`
- **Verification:** `git show --stat HEAD` includes lockfile; `cargo check -p oxidean-git` still passes
- **Committed in:** `3e4e7fe`

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary for reproducible workspace builds. No scope creep.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Rust Wave 0 stub paths exist for VALIDATION.md checklist items (web stubs remain 07-16)
- Ready for 07-01 (docs/REQUIREMENTS amend) and 07-02 (0007 migration greens dialect stubs)

## Self-Check: PASSED

- FOUND: `crates/oxidean-git/src/lib.rs`, `version.rs`, all `repo_*.rs`, `dialect_repositories.rs`
- FOUND: commits `c8f39c4`, `3e4e7fe`
- FOUND: `oxidean-git` in root `Cargo.toml`; `cargo check -p oxidean-git` ok; nextest lists `repo_*`

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*
