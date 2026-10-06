---
phase: 07-git-repos-browse
plan: "12"
subsystem: api
tags: [git, CliGitBackend, repo.create, bare-repos, require_verified]
requires:
  - phase: 07-git-repos-browse
    provides: "0007 repositories schema + validate_repo_name + proceed_locked (07-02); Wave 0 RED stubs (07-00)"
provides:
  - "GitBackend trait + CliGitBackend init_bare (argv-only)"
  - "repo.create RPC with require_verified + bare path under OXIDEAN_REPOS_DIR"
  - "AppState Arc<dyn GitBackend> + repos_dir (default var/repos)"
affects:
  - 07-03-create-ux
  - 07-04-home-defaults
  - 07-13-browse-ui
  - 07-17-boot-docker
actuals:
  tokens: 8490
  tasks: 1
  commits: 8
commits: 8
plan_head_before: 4364375c5f1a1a3475b9860d2cc1c4ebfb593731
tech-stack:
  added: []
  patterns:
    - "GitBackend deep module; CliGitBackend via tokio::process argv arrays"
    - "Bare init: git init --bare + symbolic-ref HEAD (no --initial-branch)"
    - "repos_dir mirrors uploads_dir AppState pattern"
key-files:
  created:
    - crates/oxidean-git/src/backend.rs
    - crates/oxidean-git/src/cli.rs
    - crates/oxidean-api/src/git/mod.rs
    - crates/oxidean-api/src/repo/mod.rs
  modified:
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/tests/repo_create.rs
    - crates/oxidean-api/tests/repo_fs_layout.rs
    - crates/oxidean-core/src/repo_types.rs
    - packages/api-client/src/index.ts
key-decisions:
  - "Duplicate create returns stable repo.name_taken for inline /new UI (D-12)"
  - "CreateRepoRequest.visibility optional — omit uses instance default_visibility else public (D-08)"
  - "Fail-boot git gate + Dockerfile/Compose left to 07-17 (D-33)"
patterns-established:
  - "Arc<dyn GitBackend> on AppState/RpcCtx like EmailSender"
  - "bare_repo_path helper keeps path joins out of handlers"
requirements-completed: [GIT-01, GIT-08, GIT-09, GIT-10]
coverage:
  - id: D1
    description: "Verified user creates empty public repo via repo.create (DB + DTO)"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_create.rs#repo_create_verified_happy_path"
        status: pass
    human_judgment: false
  - id: D2
    description: "Unverified repo.create returns auth.email_unverified"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_create.rs#repo_create_unverified_email_unverified"
        status: pass
    human_judgment: false
  - id: D3
    description: "Bare repo at {repos_dir}/{owner}/{name}.git with HEAD → refs/heads/main"
    requirement: GIT-08
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_fs_layout.rs#repo_fs_layout_bare_path_under_repos_dir"
        status: pass
    human_judgment: false
  - id: D4
    description: "AppState holds Arc<dyn GitBackend> as CliGitBackend only; future Gix documented"
    requirement: GIT-10
    verification:
      - kind: other
        ref: "rg trait GitBackend crates/oxidean-git/src/backend.rs + CliGitBackend in app.rs"
        status: pass
    human_judgment: false
duration: 4min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 12: Create Tracer (GitBackend CLI) Summary

**Verified `repo.create` greens end-to-end: DB row + bare git under `OXIDEAN_REPOS_DIR` via `CliGitBackend` argv-only init.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-12T17:06:47Z
- **Completed:** 2026-09-12T17:10:52Z
- **Tasks:** 1 (tracer)
- **Files modified:** 18

## Accomplishments

- Landed `GitBackend` + `CliGitBackend::init_bare` (`git init --bare` + `symbolic-ref HEAD`)
- Wired `repos_dir` / `OXIDEAN_REPOS_DIR` and `Arc<dyn GitBackend>` on `AppState` / `RpcCtx`
- Implemented `repo.create` with `require_verified`, name validation, optional description/visibility, `repo.name_taken`
- Greened `repo_create` (happy / unverified / duplicate) and `repo_fs_layout` HEAD checks

## Task Commits

1. **Task 1: End-to-end empty public create via GitBackend CLI** — `f10d98b` (feat)

**Plan metadata:** `256bc1c` (docs: complete plan)

## Files Created/Modified

- `crates/oxidean-git/src/backend.rs` — `GitBackend` + `GitError`
- `crates/oxidean-git/src/cli.rs` — `CliGitBackend::init_bare`
- `crates/oxidean-api/src/git/mod.rs` — `bare_repo_path` layout helper
- `crates/oxidean-api/src/repo/mod.rs` — `repo.create` handler
- `crates/oxidean-api/src/app.rs` / `rpc.rs` — state + RPC registration
- `crates/oxidean-api/tests/repo_create.rs` / `repo_fs_layout.rs` — integration greens
- `packages/api-client/src/index.ts` — `repo.create` + DTOs via rpc-gen

## Decisions Made

- Stable duplicate code: `repo.name_taken` (message matches UI-SPEC D-12 copy)
- Optional `visibility` on create input; falls back to instance `default_visibility` then public
- Boot fail-gate / Dockerfile / Compose volume deferred to 07-17 per plan prohibition

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] AuthSettingsRow test missing `default_visibility`**
- **Found during:** Task 1 (compile for nextest)
- **Issue:** 07-02 added `default_visibility` to `AuthSettingsRow` but admin unit test initializer omitted the field, blocking `oxidean-api` lib-test compile
- **Fix:** Set `default_visibility: "public"` in the test fixture
- **Files modified:** `crates/oxidean-api/src/auth/admin.rs`
- **Commit:** `f10d98b`

## Tracer Feedback Gate

- Mode: `HUMAN_VERIFY_MODE=end-of-phase` + automated-only `<verify>`
- Re-ran `cargo nextest run -p oxidean-api -E 'test(repo_create) | test(repo_fs)'` — **4 passed**
- ⚡ Tracer verified end-to-end — no expansion tasks in this plan

## Known Stubs

None that block this plan's goal. Wave 0 stubs remain for later plans (`repo.get` private 404, branch soft-protect, `assert_git_version`, archive formats).

## Self-Check: PASSED

- FOUND: `crates/oxidean-git/src/backend.rs`
- FOUND: `crates/oxidean-git/src/cli.rs`
- FOUND: `crates/oxidean-api/src/repo/mod.rs`
- FOUND: `crates/oxidean-api/src/git/mod.rs`
- FOUND: commit `f10d98b`
