---
phase: 07-git-repos-browse
verified: "2026-09-19T18:14:28Z"
status: passed
status_note: Automated fingerprint refresh — existing test/VALIDATION evidence accepted as proof (no conversational UAT).
score: 7/7 must-haves verified
covered_files:
  - .env.example
  - .planning/REQUIREMENTS.md
  - .planning/phases/07-git-repos-browse/07-00-PLAN.md
  - .planning/phases/07-git-repos-browse/07-00-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-01-PLAN.md
  - .planning/phases/07-git-repos-browse/07-01-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-02-PLAN.md
  - .planning/phases/07-git-repos-browse/07-02-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-03-PLAN.md
  - .planning/phases/07-git-repos-browse/07-03-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-04-PLAN.md
  - .planning/phases/07-git-repos-browse/07-04-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-05-PLAN.md
  - .planning/phases/07-git-repos-browse/07-05-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-06-PLAN.md
  - .planning/phases/07-git-repos-browse/07-06-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-07-PLAN.md
  - .planning/phases/07-git-repos-browse/07-07-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-08-PLAN.md
  - .planning/phases/07-git-repos-browse/07-08-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-09-PLAN.md
  - .planning/phases/07-git-repos-browse/07-09-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-10-PLAN.md
  - .planning/phases/07-git-repos-browse/07-10-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-11-PLAN.md
  - .planning/phases/07-git-repos-browse/07-11-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-12-PLAN.md
  - .planning/phases/07-git-repos-browse/07-12-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-13-PLAN.md
  - .planning/phases/07-git-repos-browse/07-13-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-14-PLAN.md
  - .planning/phases/07-git-repos-browse/07-14-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-15-PLAN.md
  - .planning/phases/07-git-repos-browse/07-15-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-16-PLAN.md
  - .planning/phases/07-git-repos-browse/07-16-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-17-PLAN.md
  - .planning/phases/07-git-repos-browse/07-17-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-18-PLAN.md
  - .planning/phases/07-git-repos-browse/07-18-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-19-PLAN.md
  - .planning/phases/07-git-repos-browse/07-19-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-20-PLAN.md
  - .planning/phases/07-git-repos-browse/07-20-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-21-PLAN.md
  - .planning/phases/07-git-repos-browse/07-21-SUMMARY.md
  - .planning/phases/07-git-repos-browse/07-CONTEXT.md
  - .planning/phases/07-git-repos-browse/07-COVERAGE.md
  - .planning/phases/07-git-repos-browse/07-REVIEW.md
  - .planning/phases/07-git-repos-browse/07-SECURITY.md
  - .planning/phases/07-git-repos-browse/07-UAT.md
  - .planning/phases/07-git-repos-browse/07-VALIDATION.md
  - apps/web/src/components/repo/clone-box.integration.test.ts
  - apps/web/src/components/repo/clone-box.tsrx
  - apps/web/src/components/repo/path-breadcrumb.integration.test.ts
  - apps/web/src/components/repo/path-breadcrumb.tsrx
  - apps/web/src/components/repo/readme-panel.integration.test.ts
  - apps/web/src/lib/highlight.test.ts
  - apps/web/src/lib/highlight.ts
  - apps/web/src/lib/markdown.ts
  - apps/web/src/lib/repo-browse.ts
  - apps/web/src/lib/repo-browse.unit.test.ts
  - apps/web/src/routes/$owner.$repo.blame.$.tsrx
  - apps/web/src/routes/$owner.$repo.blob.$.tsrx
  - apps/web/src/routes/$owner.$repo.branches.tsrx
  - apps/web/src/routes/$owner.$repo.index.tsrx
  - apps/web/src/routes/$owner.$repo.settings.tsrx
  - apps/web/src/routes/$owner.$repo.tags.tsrx
  - apps/web/src/routes/$owner.$repo.tree.$.tsrx
  - apps/web/src/routes/new.tsrx
  - crates/oxidean-api/src/app.rs
  - crates/oxidean-api/src/main.rs
  - crates/oxidean-api/src/repo/acl.rs
  - crates/oxidean-api/src/repo/mod.rs
  - crates/oxidean-api/src/repo/templates.rs
  - crates/oxidean-api/src/routes/repo_raw.rs
  - crates/oxidean-api/src/rpc.rs
  - crates/oxidean-api/tests/repo_archive.rs
  - crates/oxidean-api/tests/repo_branch_soft_protect.rs
  - crates/oxidean-api/tests/repo_create.rs
  - crates/oxidean-core/src/repo_types.rs
  - crates/oxidean-db/migrations/mysql/0007_repositories.sql
  - crates/oxidean-db/migrations/postgres/0007_repositories.sql
  - crates/oxidean-db/migrations/sqlite/0007_repositories.sql
  - crates/oxidean-db/src/repositories.rs
  - crates/oxidean-git/src/backend.rs
  - crates/oxidean-git/src/cli.rs
  - crates/oxidean-git/src/lib.rs
  - crates/oxidean-git/src/version.rs
  - docker-compose.yml
  - docs/ARCHITECTURE.md
  - docs/CONFIGURATION.md
covered_digest: "v1:sha256:52d4f5fd7872eda628ab6e760b3e5b325ea4bd5a66a95596551ad04c0fdec0fb"
behavior_unverified: 0
overrides_applied: 0
next_action: Verification passed — continue.
decision_coverage: "{'honored': 38, 'total': 38, 'not_honored': []}"
re_verification: "{'previous_status': 'passed', 'previous_score': '7/7', 'gaps_closed': ['Stale covered_digest refresh after UAT complete (prior report 2026-09-13T01:32:57Z) — live content drift from Phase 08 edits to shared files; must-haves reconfirmed green'], 'gaps_remaining': [], 'regressions': []}"
advisory: "[]"
---

# Phase 7: Git Repos & Browse Verification Report

**Phase Goal:** Users can create filesystem-backed repos and browse history in the UI via system `git` CLI behind a `GitBackend` seam, with a documented future gitoxide path

**Verified:** 2026-09-13T21:40:32Z  
**Status:** passed  
**Re-verification:** Yes — digest refresh after UAT complete (prior report `passed` / stale fingerprint at 2026-09-13T01:32:57Z)  
**UAT:** `07-UAT.md` — `status: complete`, **5/5 passed**, 0 issues (not reopened)  
**Security:** `07-SECURITY.md` — `threats_open: 0`  
**Validation:** `07-VALIDATION.md` present  
**Next action:** Verification passed — continue.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | ------- | ---------- | -------------- |
| 1 | Authenticated (and verified, on cloud) user can create a public or private repository | ✓ VERIFIED | `repo.create` + `/new` visibility; WR-01 `compensate_failed_create` → `soft_delete_repository`; `repo_create_git_failure_soft_deletes_row_allows_recreate` **PASS** |
| 2 | User can browse files, commits, branches, and tags in the web UI and download a source archive for a ref | ✓ VERIFIED | tree/blob/commits/branches/tags routes; clone-box archives; `repo_archive_zip_and_tar_gz_nonempty_for_seeded_ref` **PASS**; WR-03 `parseRefAndPath(..., knownRefs)` on tree/blob/blame; UAT 5/5 **PASS** |
| 3 | User can create, rename, and delete branches from the web UI where permitted | ✓ VERIFIED | Soft-protect + CR-02: `reject_option_like_branch`, `validate_treeish` leading `-`, branch argv `--`; injection + soft-protect named tests **PASS** |
| 4 | Repository objects live on the local filesystem (volume-backed), and git ops use system `git` CLI with docs allowing future gitoxide swap | ✓ VERIFIED | `OXIDEAN_REPOS_DIR` + Compose `./var/repos:/var/repos`; `CliGitBackend`; `assert_git_version` fail-boot; ARCHITECTURE Cli now / Gix later; no `GixGitBackend` impl |
| 5 | Private/non-access returns identical `repo.not_found` (D-23–D-25) | ✓ VERIFIED | `resolve_repo_for_read` / `acl.rs` unified `repo.not_found` |
| 6 | Default-branch rename/delete via intended APIs returns soft-protect error | ✓ VERIFIED | Soft-protect checks + `repo_branch_soft_protect_blocks_default_rename_and_delete` **PASS**; CR-02 bypass closed |
| 7 | Archive/treeish argv cannot be interpreted as git CLI options | ✓ VERIFIED | `validate_treeish` / `validate_archive_treeish` / `validate_ref` reject leading `-`; archive `--` before treeish; `repo_archive_rejects_option_like_treeish_no_output_file` **PASS** |

**Score:** 7/7 truths verified (0 behavior-unverified — UAT 5/5 complete; named regression tests still green after Phase 08 shared-file churn)

### Deferred Items

None.

### Advisory (New Scope, Unevidenced)

None — re-verification Step 7 found no new-scope unevidenced blockers. Shared files touched by Phase 08 still satisfy Phase 07 must-haves (named tests **PASS**). CR-01/CR-02 and WR-01..03 remain closed.

### Gap Closure Status (CR/WR)

| ID | Prior | Now | Evidence |
| ---- | ----- | --- | -------- |
| CR-02 | FAILED (branchCreate `-D` bypass) | **CLOSED** | 07-19; injection test **PASS**; `--` on create/rename/delete |
| CR-01 | FAILED (`--output=` archive write) | **CLOSED** | 07-20; injection test **PASS**; archive `--` + HTTP leading-`-` |
| WR-01 | Advisory (orphan name lock) | **CLOSED** | 07-21; recreate test **PASS**; `compensate_failed_create` |
| WR-02 | Advisory (raw `/` ban) | **CLOSED** | 07-20; `validate_ref` allows `/`, rejects leading `-` |
| WR-03 | Advisory (first-segment-only parse) | **CLOSED** | 07-21; longest-prefix + vitest **PASS**; knownRefs in tree/blob/blame |

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | ----------- | ------ | ------- |
| `crates/oxidean-git/src/cli.rs` | CliGitBackend + safe argv | ✓ VERIFIED | Leading-`-` in `validate_treeish`; `--` on branch_* and archive |
| `crates/oxidean-git/src/backend.rs` | GitBackend trait | ✓ VERIFIED | Trait + future Gix docs |
| `crates/oxidean-git/src/version.rs` | assert_git_version ≥2.5 | ✓ VERIFIED | Called from `main.rs` → exit(1) |
| `crates/oxidean-db/migrations/*/0007_repositories.sql` | repos schema | ✓ VERIFIED | sqlite/postgres/mysql present |
| `crates/oxidean-api/src/repo/mod.rs` | create + branch + compensate | ✓ VERIFIED | `reject_option_like_branch`; `compensate_failed_create` |
| `crates/oxidean-api/src/routes/repo_raw.rs` | raw + archive HTTP | ✓ VERIFIED | Leading-`-` reject; slashy refs allowed (WR-02) |
| `apps/web/src/lib/repo-browse.ts` | hierarchical parse | ✓ VERIFIED | `parseRefAndPath(splat, knownRefs?)` longest-prefix |
| `apps/web/src/lib/repo-browse.unit.test.ts` | WR-03 unit coverage | ✓ VERIFIED | 9 tests **PASS** |
| `apps/web/src/components/repo/path-breadcrumb.tsrx` | long-path truncate | ✓ VERIFIED | truncate + title; UAT #3 **PASS** |
| `apps/web/src/routes/new.tsrx` | create UI | ✓ VERIFIED | rpc-gen client create; UAT wrap **PASS** |
| `apps/web/src/routes/$owner.$repo.{tree,blob,blame,branches,tags,settings}*` | browse UI | ✓ VERIFIED | knownRefs wired on tree/blob/blame |
| `docs/ARCHITECTURE.md` | GitBackend docs | ✓ VERIFIED | CliGitBackend / GixGitBackend section |
| `docker-compose.yml` / `.env.example` | volume-backed repos | ✓ VERIFIED | `./var/repos:/var/repos` + `OXIDEAN_REPOS_DIR` |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| `repo/mod.rs` | `CliGitBackend` | `ctx.git.branch_*` / `init_bare` | ✓ WIRED | Dyn trait on RpcCtx |
| `repo/mod.rs` | `soft_delete_repository` | `compensate_failed_create` | ✓ WIRED | WR-01 on init/seed Err |
| `main.rs` | `version.rs` | `assert_git_version` | ✓ WIRED | Boot call |
| `new.tsrx` | `repo.create` | apiClient | ✓ WIRED | |
| `branches.tsrx` | `repo.branch_*` | apiClient | ✓ WIRED | |
| `clone-box.tsrx` | archive HTTP | `/api/repos/.../archive/` | ✓ WIRED | integration **PASS** |
| `repo_raw.rs` | `git.archive` | `serve_archive` | ✓ WIRED | CR-01 hardened |
| `tree/blob/blame.$.tsrx` | `repo-browse.ts` | `parseRefAndPath(..., knownRefs)` | ✓ WIRED | WR-03 / D-17 |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| Tree/blob UI | tree entries / blob bytes | `repo.tree` / `repo.blob` → CLI | Yes | ✓ FLOWING |
| Archive download | zip/tar.gz bytes | `serve_archive` → `git.archive` | Yes (safe argv) | ✓ FLOWING |
| Branch list | refs | `list_refs` | Yes | ✓ FLOWING |
| /new create | repo row + bare dir | DB insert + `init_bare` (+ compensate) | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| CR-02 injection closed | `cargo test -p oxidean-api --test repo_branch_soft_protect repo_branch_create_rejects_option_like_name_leaves_default_intact -- --exact` | 1 passed | ✓ PASS |
| Soft-protect default rename/delete | `cargo test -p oxidean-api --test repo_branch_soft_protect repo_branch_soft_protect_blocks_default_rename_and_delete -- --exact` | 1 passed | ✓ PASS |
| CR-01 `--output=` rejected | `cargo test -p oxidean-api --test repo_archive repo_archive_rejects_option_like_treeish_no_output_file -- --exact` | 1 passed | ✓ PASS |
| Archive zip/tar.gz nonempty | `cargo test -p oxidean-api --test repo_archive repo_archive_zip_and_tar_gz_nonempty_for_seeded_ref -- --exact` | 1 passed | ✓ PASS |
| WR-01 create compensate | `cargo test -p oxidean-api --test repo_create repo_create_git_failure_soft_deletes_row_allows_recreate -- --exact` | 1 passed | ✓ PASS |
| WR-03 parseRefAndPath | `bunx vitest run src/lib/repo-browse.unit.test.ts` | 9 passed | ✓ PASS |
| Clone-box + highlight + breadcrumb | `bunx vitest run ...clone-box... ...highlight... ...path-breadcrumb...` | 10 passed | ✓ PASS |

### Probe Execution

| Probe | Command | Result | Status |
| ----- | ------- | ------ | ------ |
| — | — | No phase-declared `scripts/*/tests/probe-*.sh` | SKIP |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| GIT-01 | 00,02,03,04,09,12,13,16,21 | Create public/private repo | ✓ SATISFIED | create RPC + /new + WR-01 compensate |
| GIT-05 | 00,05,06,07,14,15,18,20,21 | Browse files/commits/branches/tags | ✓ SATISFIED | browse APIs + routes + WR-02/WR-03 |
| GIT-06 | 00,07,18,19 | Branch create/rename/delete where permitted | ✓ SATISFIED | Soft-protect + CR-02 injection closed |
| GIT-07 | 00,08,20 | Download source archive | ✓ SATISFIED | Happy-path + CR-01 injection closed |
| GIT-08 | 00,02,05,09,10,12,17 | Filesystem / volume-backed objects | ✓ SATISFIED | repos_dir + Compose volume + bare layout |
| GIT-09 | 00,01,11,12,17 | System git CLI ≥2.5 + CliGitBackend | ✓ SATISFIED | version gate + Cli adapter |
| GIT-10 | 00,01,11,12 | Swappable GitBackend; Gix future docs | ✓ SATISFIED | ARCHITECTURE + trait seam; no gix primary |

Orphaned phase requirements: none (GIT-02..04 are Phase 8/9 — not Phase 7).

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| — | — | No unreferenced `TBD`/`FIXME`/`XXX` in gap-closure / browse paths | — | — |
| — | — | Prior CR-01/CR-02 / WR-01..03 patterns **remain resolved** | — | Closed by 07-19..21 |

No self-evidencing debt markers; advisory list empty.

### Test Quality Audit

| Test File | Linked Req | Active | Skipped | Circular | Assertion Level | Verdict |
|-----------|-----------|--------|---------|----------|-----------------|---------|
| `repo_branch_soft_protect.rs` | GIT-06 | yes | none | no | Behavioral | OK — CR-02 injection |
| `repo_archive.rs` | GIT-07 | yes | none | no | Behavioral | OK — CR-01 `--output` |
| `repo_create.rs` | GIT-01 | yes | none | no | Behavioral | OK — WR-01 compensate |
| `repo-browse.unit.test.ts` | GIT-05 | yes | none | no | Value | OK — hierarchical + fallback |
| `repo_raw.rs` validate_ref_tests | GIT-05 | yes | none | no | Value | OK — WR-02 slash + leading `-` |
| `clone-box.integration.test.ts` | GIT-07 | yes | none | no | Behavioral | OK — HTTPS/SSH/archive |
| `highlight.test.ts` | GIT-05 | yes | none | no | Value | OK — `.ts` / `.tsrx` / `.ripple` |
| `path-breadcrumb.integration.test.ts` | GIT-05 | yes | none | no | Behavioral | OK — truncate/wrap |

**Disabled tests on requirements:** 0  
**Circular patterns detected:** 0  
**Insufficient assertions:** 0

### Decision Coverage

All trackable CONTEXT.md decisions are honored by shipped artifacts. (38/38 honored; non-blocking)

### Prohibitions

| Prohibition | Status | Notes |
| ----------- | ------ | ----- |
| Must not allow delete/rename of default branch from Phase 7 API | ✓ held | Soft-protect + CR-02 injection regression **PASS** |
| Must not allow user treeish as git CLI options | ✓ held | CR-01 archive injection regression **PASS** |
| Must not shell via `sh -c` | ✓ held | argv `Command` arrays only |
| Must not implement GixGitBackend body | ✓ held | docs only |
| Must not return archive bytes via JSON RPC | ✓ held | HTTP streaming |
| Must not reveal private via distinct errors | ✓ held | unified not_found |

### Human Verification Required

None — `07-UAT.md` is `status: complete` with **5/5 passed** (0 issues). UAT was not reopened.

### Gaps Summary

No open gaps. All seven roadmap/must-have truths verify in code and named tests. **CR-01**, **CR-02**, **WR-01**, **WR-02**, and **WR-03** remain closed. Fingerprint refreshed so `verification.status` is no longer stale. Phase status is **passed**.

---

_Verified: 2026-09-13T21:40:32Z_  
_Verifier: Claude (gsd-verifier)_

## Automated re-verification (2026-09-19T18:14:28Z)

- Mode: fingerprint refresh (`covered_files` + `covered_digest`)
- Policy: existing phase VERIFICATION must-haves + SUMMARY/test evidence treated as sufficient; conversational UAT not re-run
- Covered inputs: 92 files

