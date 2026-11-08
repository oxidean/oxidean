---
phase: "07"
slug: "git-repos-browse"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-12"
updated: "2026-09-13"
---

# Phase 07 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Plan IDs below match every `07-*-PLAN.md` in this phase (07-00 … 07-21).
> `nyquist_compliant: true` after Nyquist auditor gap fill (2026-09-13).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust: cargo nextest + `cargo test`; Web: Vitest 5 (unit / integration / e2e) |
| **Config file** | `apps/web/vitest.config.ts`; Makefile `make test` |
| **Quick run command** | `cargo nextest run -p oxidean-git --lib` && `cd apps/web && bun run test:unit` |
| **Full suite command** | `make test` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run targeted nextest filter + relevant Vitest project
- **After every plan wave:** Run `make test`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 07-00-T1 | 07-00 | 0 | GIT-* | — | Nyquist stub paths discoverable | stubs | `cargo check -p oxidean-git` + nextest list `repo_*` | ✅ | ✅ green |
| 07-01-T1 | 07-01 | 1 | GIT-09, GIT-10 | T-07-03 | CLI-primary docs; no gitoxide-first | docs | `rg CliGitBackend/GitBackend` REQUIREMENTS/ROADMAP/COVERAGE | ✅ | ✅ green |
| 07-02-T1 | 07-02 | 2 | GIT-08 | — | repositories schema + validate_repo_name | unit/integration | `cargo test -p oxidean-db --test dialect_repositories` | ✅ | ✅ green |
| 07-16-T1 | 07-16 | 0 | GIT-01 | — | Wave 0 `/new` wall + home CTA stubs | integration | `vitest … new.integration + signed-in-home` | ✅ | ✅ green |
| 07-17-T1 | 07-17 | 3 | GIT-09 | T-07 / D-33 | Boot rejects missing/old git; Compose volume | unit + ops | `nextest … git_version_gate` + Dockerfile/Compose rg | ✅ | ✅ green |
| 07-12-T1 | 07-12 | 4 | GIT-01, GIT-08 | — | Verified create + bare `{owner}/{name}.git` | integration | `nextest -p oxidean-api -E 'test(repo_create)\|test(repo_fs)'` | ✅ | ✅ green |
| 07-13-T1 | 07-13 | 5 | GIT-01 | — | `/new` + empty Code Quick setup reachable | integration | `vitest … new.integration.test.ts` + web build | ✅ | ✅ green |
| 07-03-T* | 07-03 | 6 | GIT-01 | — | Templates/SPDX/gitignore + duplicate inline | integration | `nextest … test(repo_create)` + `/new` vitest | ✅ | ✅ green |
| 07-04-T* | 07-04 | 7 | GIT-01 | — | Signed-in home list + defaults | integration | `vitest … signed-in-home` + `repo_create`/`profile` | ✅ | ✅ green |
| 07-05-T1 | 07-05 | 8 | GIT-05 | T-07-acl | Private non-owner → identical `repo.not_found` | unit/integration | `nextest … test(repo_private)\|test(repo_create)` + git `--lib` | ✅ | ✅ green |
| 07-14-T1 | 07-14 | 8 | GIT-05 | T-07-14 | Sanitize last; tsrx/ripple grammars | unit | `vitest … markdown.test + highlight.test` | ✅ | ✅ green |
| 07-15-T1 | 07-15 | 9 | GIT-05 | D-25 | Code/tree/blob UI + private 404 | integration | `vitest … $owner.$repo.integration.test.ts` | ✅ | ✅ green |
| 07-06-T* | 07-06 | 10 | GIT-05 | T-07-18 | Commits/compare/blame soft caps | unit/integration | `nextest -p oxidean-git --lib` + `test(repo_)` | ✅ | ✅ green |
| 07-07-T1 | 07-07 | 11 | GIT-06 | T-07-branch | Owner branch CRUD; default soft-protect | integration | `nextest … test(repo_branch)` | ✅ | ✅ green |
| 07-18-T1 | 07-18 | 12 | GIT-06 | — | Branches/Tags UI + Dialog/AlertDialog | build | `bun --cwd apps/web run build` | ✅ | ✅ green |
| 07-08-T* | 07-08 | 13 | GIT-07 | T-07-SC | zip + tar.gz archive HTTP + clone box | unit/integration | `nextest … test(repo_archive)\|test(git_archive)` + `vitest … clone-box.integration` | ✅ | ✅ green |
| 07-09-T* | 07-09 | 14 | GIT-01 | — | Visibility toggle + soft-delete | integration | `nextest … test(repo_)` + web build | ✅ | ✅ green |
| 07-10-T* | 07-10 | 14 | GIT-08 | D-36–38 | Orphan reconcile, gc, factory-reset scope | integration + docs | `nextest … orphan\|gc\|factory_reset` + CONFIGURATION rg | ✅ | ✅ green |
| 07-11-T1 | 07-11 | 15 | GIT-09, GIT-10 | T-07-03 | ARCHITECTURE CliGitBackend + future Gix | docs | `rg GitBackend\|CliGitBackend\|GixGitBackend` docs | ✅ | ✅ green |
| 07-11-T2 | 07-11 | 15 | GIT-09, GIT-10 | T-07-SC | VALIDATION map + rpc-gen client sync | smoke | `rpc-gen` + web build + `test(repo_)` + git `--lib` | ✅ | ✅ green |
| 07-19-T* | 07-19 | gap | GIT-06 | T-07-GC19 / CR-02 | option-like branchCreate rejected; default soft-protect intact; validate_treeish leading-`-` + branch argv `--` | integration | `cargo test -p oxidean-api --test repo_branch_soft_protect` | ✅ | ✅ green |
| 07-20-T* | 07-20 | gap | GIT-07, GIT-05 | T-07-GC20 / CR-01 | option-like archive treeish rejected; no `--output` file write | integration | `cargo test -p oxidean-api --test repo_archive` | ✅ | ✅ green |
| 07-21-T1 | 07-21 | gap | GIT-01 | T-07-GC21 / WR-01 | create git failure soft-deletes row; name reusable | integration | `cargo test -p oxidean-api --test repo_create` | ✅ | ✅ green |
| 07-21-T2 | 07-21 | gap | GIT-05 | WR-03 / D-17 | parseRefAndPath longest-prefix hierarchical refs | unit | `bunx vitest run src/lib/repo-browse.unit.test.ts` (cwd `apps/web`) | ✅ | ✅ green |
| 07-14-HL | 07-14 | 8 | GIT-05 / UI-SPEC | — | `.tsrx` / `.ripple` Shiki grammars load + highlight | unit | `bunx vitest run src/lib/highlight.test.ts` (cwd `apps/web`) | ✅ | ✅ green |
| 07-14-MD | 07-14 | 8 | GIT-05 / D-18 | — | Safe Markdown README (sanitize + ReadmePanel) | unit/integration | `bunx vitest run src/lib/markdown.test.ts src/components/repo/readme-panel.integration.test.ts` (cwd `apps/web`) | ✅ | ✅ green |
| 07-08-CB | 07-08 | 13 | D-22 | — | Clone box HTTPS URL + SSH placeholder | integration | `bunx vitest run src/components/repo/clone-box.integration.test.ts` (cwd `apps/web`) | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Plan index (every `07-*-PLAN.md`)

| Plan | Title (objective) | SUMMARY |
|------|-------------------|---------|
| 07-00 | Wave 0 Rust Nyquist stubs | ✅ |
| 07-01 | CLI-first contracts + COVERAGE | ✅ |
| 07-02 | Repositories schema + validators | ✅ |
| 07-03 | Templates / SPDX / create expansion | ✅ |
| 07-04 | Dashboard home + account defaults | ✅ |
| 07-05 | ACL-safe tree/blob/raw APIs | ✅ |
| 07-06 | History browse (commits/compare/blame) | ✅ |
| 07-07 | Branch CRUD + soft-protect API | ✅ |
| 07-08 | Git archive + clone box | ✅ |
| 07-09 | Visibility + soft-delete settings | ✅ |
| 07-10 | Ops lifecycle (orphan/gc/reset docs) | ✅ |
| 07-11 | ARCHITECTURE / VALIDATION / rpc-gen | ✅ |
| 07-12 | Create tracer (CliGitBackend E2E) | ✅ |
| 07-13 | `/new` + Quick setup UI | ✅ |
| 07-14 | Markdown sanitize + Shiki grammars | ✅ |
| 07-15 | Code / tree / blob Octane routes | ✅ |
| 07-16 | Wave 0 web integration stubs | ✅ |
| 07-17 | Git version gate + Compose volume | ✅ |
| 07-18 | Branches / Tags UI | ✅ |
| 07-19 | CR-02 branchCreate option injection | ✅ |
| 07-20 | CR-01 archive argv + WR-02 raw slash | ✅ |
| 07-21 | Create compensate + hierarchical parseRefAndPath | ✅ |

---

## Wave 0 Requirements

Conceptual checklist after Phase 7 execution history (stubs turned green by later plans):

- [x] `crates/oxidean-git` crate + version gate unit tests
- [x] `crates/oxidean-api/tests/repo_*.rs` integration harness (temp repos_dir + DB)
- [x] Tri-dialect migration `0007_repositories` (+ account default_branch / instance default_visibility as needed)
- [x] Web integration tests for SignedInHome CTA → `/new` and unverified wall
- [x] Dockerfile installs `git`; Compose volume for `var/repos`
- [x] Extend reserved username list with `"new"` (and other flat routes)
- [x] Docs: amend GIT-09 wording; ARCHITECTURE GitBackend section

---

## Manual-Only Verifications

None remaining — former manual rows promoted after automated green (2026-09-13):

| Behavior (promoted) | Requirement | Automated Command | Former reason |
|---------------------|-------------|-------------------|---------------|
| Syntax highlighting for `.tsrx` / `.ripple` | GIT-05 / UI-SPEC | `bunx vitest run src/lib/highlight.test.ts` (cwd `apps/web`) | Visual fidelity → grammar + highlight HTML asserts |
| Safe Markdown README render | GIT-05 / D-18 | `bunx vitest run src/lib/markdown.test.ts src/components/repo/readme-panel.integration.test.ts` (cwd `apps/web`) | Sanitizer edge cases → unit + ReadmePanel integration |
| Clone/download box UX (HTTPS + SSH placeholder) | D-22 | `bunx vitest run src/components/repo/clone-box.integration.test.ts` (cwd `apps/web`) | UI copy → CloneBox integration |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 120s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** Nyquist auditor gap fill 2026-09-13 (plans 07-19..21 mapped; manuals promoted)

---

## Validation Audit

**Date:** 2026-09-13  
**Auditor:** gsd-nyquist-auditor  
**Scope:** Fix all gaps — map 07-19/20/21; verify + promote former Manual-Only rows

### Commands run

| Command | Result |
|---------|--------|
| `cargo test -p oxidean-api --test repo_branch_soft_protect` | PASS (4 tests) |
| `cargo test -p oxidean-api --test repo_archive` | PASS (4 tests) |
| `cargo test -p oxidean-api --test repo_create` | PASS (6 tests) |
| `bunx vitest run src/lib/repo-browse.unit.test.ts src/lib/highlight.test.ts src/lib/markdown.test.ts src/components/repo/readme-panel.integration.test.ts src/components/repo/clone-box.integration.test.ts` (cwd `apps/web`) | PASS (5 files / 20 tests) |

### Outcomes

- **07-19 / 07-20 / 07-21:** Existing tests COVERED requirements; Per-Task Map + plan index updated (FILLED).
- **Manual → automated:** highlight / markdown+readme-panel / clone-box all green; removed from Manual-Only (FILLED).
- **Implementation changes:** none (read-only).
- **Escalations:** none.
- **Frontmatter:** `status: validated`, `nyquist_compliant: true`.
