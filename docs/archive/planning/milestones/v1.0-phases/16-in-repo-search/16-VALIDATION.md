---
phase: "16"
slug: "in-repo-search"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-16"
updated: "2026-10-02"
validated_at: "2026-10-02"
---

# Phase 16 — Validation Strategy

> Per-phase validation contract. Seeded from `16-RESEARCH.md`. Wave 0 closed by plan `16-00`. Gate green as of `16-03`.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo nextest (Rust) + Vitest (web) |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(repo_search)'` |
| **Full suite command** | `make test` |
| **Phase gate** | Quick run + `cargo nextest run -p oxidean-git -E 'test(grep) \| test(log_search)'` + `make rpc-sync-check` + Vitest search route + `bun --cwd apps/web run build` |

## Sampling Rate

- **Per task commit:** focused nextest / Vitest filter
- **Per wave merge:** `repo_search` nextest cluster
- **Phase gate:** Full automated gate green before `/gsd-verify-work`

## Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GIT-18 | Code search finds seeded content | API | `cargo nextest run -p oxidean-api -E 'test(repo_search_code)'` | ✅ |
| GIT-18 | Commit search by message/author | API | `… test(repo_search_commits)` | ✅ |
| GIT-18 | Issue search by title/body | API | `… test(repo_search_issues)` | ✅ |
| GIT-18 | PR search by title/body | API | `… test(repo_search_pulls)` | ✅ |
| GIT-18 | Private unauthorized soft not_found | API | `… test(repo_search_acl)` | ✅ |
| GIT-18 | Timeout/truncate soft behavior | API | `… test(repo_search_limits)` | ✅ |
| GIT-18 | Search UI tabs + results | Vitest | `bun --cwd apps/web exec vitest run src/routes/\$owner.\$repo.search.integration.test.ts` | ✅ |

## Wave 0 Feedback

Missing tests must be stubbed in `16-00` before feature plans turn them green.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (closed by `16-00`)
- [x] No watch-mode flags
- [x] Feedback latency < 120s for focused filters
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-10-02 (retroactive audit below; `16-VERIFICATION.md` passed 2026-09-19)

---

## Validation Audit 2026-10-02

Retroactive Nyquist audit (ROADMAP DEBT-08) — same class of reconcile as `22.1-09` for phases 8/14/15/20. Scope: confirm every mapped command still names real files/tests on disk and that recorded green runs exist; suites not re-executed in this docs pass.

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 1 (`status: complete` → `validated`; stale lifecycle value reconciled) |
| Escalated | 0 |
| Manual-only | 0 |

| Check | Result |
|-------|--------|
| `crates/oxidean-api/tests/repo_search.rs` (`repo_search_code` / `_commits` / `_issues` / `_pulls` / `_acl` / `_limits`) | ✅ present; all six named tests exist |
| `crates/oxidean-git` `grep_*` + `log_search_*` tests | ✅ present (`grep_finds_seeded_line_and_empty_on_miss`, `grep_skips_binary_with_i_and_truncates`, `log_search_matches_message_and_author`) |
| `apps/web/src/routes/$owner.$repo.search.integration.test.ts` (+ `search.integration.test.ts`) | ✅ present |
| Recorded green runs | `16-VERIFICATION.md` truths PASS (verified 2026-09-19), `status: passed` |

**Verdict:** `status: validated`, `nyquist_compliant: true`. Prior `complete` lifecycle value was why audit-milestone §5.5 bucketed this phase NOT-VALIDATED; map content already matched disk.
