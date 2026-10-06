---
phase: "12"
slug: "pull-requests"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-16"
updated: "2026-10-02"
validated_at: "2026-10-02"
---

# Phase 12 — Validation Strategy

> Per-phase validation contract. Wave 0 stubs greened through plans 00–07.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo nextest + Vitest |
| **Quick run** | `cargo nextest run -p oxidean-api -E 'test(pull_)' ; cargo nextest run -p oxidean-db -E 'test(dialect_pulls) \| test(factory_reset_pulls)' ; cargo nextest run -p oxidean-git -E 'test(merge_)' ; cd apps/web && bunx vitest run 'src/routes/$owner.$repo.pulls.integration.test.ts'` |
| **Full suite** | `make test` |
| **Phase gate** | Quick run + `make rpc-sync-check` + `make web-lint` + `make web-format-check` |

## Phase Requirements → Test Map

| Req ID | Behavior | Automated Command | Status |
|--------|----------|-------------------|--------|
| PR-01 | open same-repo + fork head | `test(pull_lifecycle)` | ✅ |
| PR-02 | diff/commits | `test(pull_files)` | ✅ |
| PR-03 | comments resolve/outdated | `test(pull_comments)` | ✅ |
| PR-04 | reviews ACL | `test(pull_reviews)` | ✅ |
| PR-05 | merge + keywords | `test(pull_merge)` + git `merge_` | ✅ |
| PR-06 | close/reopen | `test(pull_lifecycle)` | ✅ |
| PR-07 | merge settings | `test(pull_merge_settings)` | ✅ |
| UI | Pulls chrome + detail | vitest pulls.integration | ✅ |
| OPS | dialect + factory reset | dialect_pulls / factory_reset_pulls | ✅ |

## Wave 0 Gaps

- [x] All Wave 0 pull_* / dialect / factory_reset / UI stubs greened

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 240s (phase gate quick run)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-10-02 (retroactive audit below; `12-VERIFICATION.md` passed 2026-09-19)

---

## Validation Audit 2026-10-02

Retroactive Nyquist audit (ROADMAP DEBT-08) — same class of reconcile as `22.1-09` for phases 8/14/15/20. Scope: confirm every mapped command still names real files/tests on disk and that recorded green runs exist; suites not re-executed in this docs pass.

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 1 (`status: executed` → `validated`; stale lifecycle value reconciled) |
| Escalated | 0 |
| Manual-only | 0 |

| Check | Result |
|-------|--------|
| `pull_lifecycle.rs` / `pull_files.rs` / `pull_comments.rs` / `pull_reviews.rs` / `pull_merge.rs` / `pull_merge_settings.rs` | ✅ all present under `crates/oxidean-api/tests/`; named `pull_*` tests exist (incl. `pull_lifecycle_fork_head_pr`, `pull_merge_merge_commit_and_closes_keyword_issue`) |
| `crates/oxidean-db/tests/dialect_pulls.rs` + `factory_reset_pulls.rs` | ✅ present (`dialect_pulls_*`, `factory_reset_pulls_wipes_pull_domain`) |
| `crates/oxidean-git` merge ops (`merge_commit*`, `squash_merge*`, `rebase_merge*`) | ✅ present in `src/cli.rs` test module |
| `apps/web/src/routes/$owner.$repo.pulls.integration.test.ts` + `pull.protection.integration.test.ts` | ✅ present |
| Recorded green runs | `12-VERIFICATION.md` truths + artifacts PASS (verified 2026-09-19; fingerprint refresh same day), `status: passed`; `21-VALIDATION.md` gate confirms `repo_fork`/`pull_*` green 2026-09-16 |

**Verdict:** `status: validated`, `nyquist_compliant: true`. Prior `executed` lifecycle value was why audit-milestone §5.5 bucketed this phase NOT-VALIDATED; map content already matched disk.
