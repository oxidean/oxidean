---
phase: "21"
slug: "social-explore"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-16"
updated: "2026-10-02"
validated_at: "2026-10-02"
---

# Phase 21: Social & Explore - Validation

**Nyquist / Wave 0 checklist** — every later plan `<automated>` must name a real path from this list (or create it in 21-00).
Retroactively reconciled to the standard contract on 2026-10-02 (ROADMAP DEBT-08); original checklist content preserved below.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo nextest (Rust) + Vitest (web) |
| **Config file** | workspace Cargo / `apps/web/vitest.config.ts` |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(repo_stars) \| test(repo_fork) \| test(repo_explore) \| test(user_public_profile)'` |
| **Full suite command** | `make test` |
| **Estimated runtime** | ~90–240 seconds targeted |

---

## Sampling Rate

- **Per task commit:** focused nextest filter + relevant Vitest file
- **Per wave merge:** `make test` + `make rpc-sync-check` after RPC/client changes
- **Phase gate:** Full automated gate green before `/gsd-verify-work` (final plan 21-07 gate log below)

---

## Wave 0 Requirements

| ID | Stub / filter | Turns green in | Status |
|----|---------------|----------------|--------|
| SOC-01 | `test(repo_stars)` in `crates/oxidean-api/tests/repo_stars.rs` | 21-01 / 21-02 | pass |
| SOC-02 | `test(user_public_profile)` in `crates/oxidean-api/tests/user_public_profile.rs` | 21-03 | pass |
| SOC-03 | `test(repo_explore)` in `crates/oxidean-api/tests/repo_explore.rs` | 21-04 | pass |
| SOC-04 | `test(repo_fork)` in `crates/oxidean-api/tests/repo_fork.rs` | 21-05 / 21-06 | pass |
| Dialect | `test(dialect_social)` in `crates/oxidean-db/tests/dialect_social.rs` | 21-01 / 21-05 | pass |
| Fork network helper | covered inside `repo_fork` (head_valid_for_base cases) | 21-05 | pass |
| Web explore | `apps/web/src/routes/explore.integration.test.ts` | 21-04 | pass |
| Web profile | `$owner.user-profile.integration.test.ts` | 21-03 | pass |
| Web chrome star/fork | `apps/web/src/components/repo/repo-chrome.social.integration.test.ts` | 21-02 / 21-06 | pass |

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------------|-----------|-------------------|-------------|--------|
| 21-00-T1 | 00 | 0 | SOC-01..04 | API + dialect RED stubs discoverable | stubs | `cargo nextest list -p oxidean-api -E 'test(repo_stars)\|test(repo_fork)\|test(repo_explore)\|test(user_public_profile)'` | ✅ | ✅ green |
| 21-00-T2 | 00 | 0 | SOC-01..04 | Web Wave 0 stubs (explore/profile/chrome) | component | `test -f` the three web test files above | ✅ | ✅ green |
| 21-01-T1 | 01 | 1 | SOC-01 | End-to-end star/unstar one path | integration | `cargo nextest run -p oxidean-api -E 'test(repo_stars)'` | ✅ | ✅ green |
| 21-01-T2 | 01 | 1 | SOC-01/04 | dialect_social parity | integration | `cargo nextest run -p oxidean-db -E 'test(dialect_social)'` | ✅ | ✅ green |
| 21-01-T3 | 01 | 1 | SOC-04 | Existing create sets fork_network_id | integration | `test(repo_fork)` / repo create cases | ✅ | ✅ green |
| 21-02-T1 | 02 | 2 | SOC-01 | user.listStarred RPC | integration | `cargo nextest run -p oxidean-api -E 'test(repo_stars_list_starred)'` | ✅ | ✅ green |
| 21-02-T2 | 02 | 2 | SOC-01 | RepoChrome Star control | component | `vitest repo-chrome.social.integration.test.ts` | ✅ | ✅ green |
| 21-03-T1 | 03 | 3 | SOC-02 | user.getPublicProfile end-to-end | integration | `cargo nextest run -p oxidean-api -E 'test(user_public_profile)'` | ✅ | ✅ green |
| 21-03-T2 | 03 | 3 | SOC-02 | Profile repo list ACL (no private leak) | integration | `test(user_public_profile_repos_acl)` | ✅ | ✅ green |
| 21-03-T3 | 03 | 3 | SOC-02 | `$owner.index` user vs org UI | component | `vitest $owner.user-profile.integration.test.ts` | ✅ | ✅ green |
| 21-04-T1 | 04 | 4 | SOC-03 | repo.explore end-to-end (public-only) | integration | `cargo nextest run -p oxidean-api -E 'test(repo_explore)'` | ✅ | ✅ green |
| 21-04-T2 | 04 | 4 | SOC-03 | /explore page + SiteHeader link | component | `vitest explore.integration.test.ts` | ✅ | ✅ green |
| 21-05-T1 | 05 | 5 | SOC-04 | GitBackend clone_bare | integration | `cargo nextest run -p oxidean-git` clone_bare cases | ✅ | ✅ green |
| 21-05-T2 | 05 | 5 | SOC-04 | repo.fork end-to-end | integration | `cargo nextest run -p oxidean-api -E 'test(repo_fork)'` | ✅ | ✅ green |
| 21-05-T3 | 05 | 5 | SOC-04 | fork_network head_valid_for_base | integration | `test(repo_fork_head_valid_for_base)` | ✅ | ✅ green |
| 21-06-T1 | 06 | 6 | SOC-04 | Fork confirm route | component | `vitest repo-chrome.social / fork` filters | ✅ | ✅ green |
| 21-06-T2 | 06 | 6 | SOC-04 | RepoChrome Fork button | component | `vitest repo-chrome.social.integration.test.ts` | ✅ | ✅ green |
| 21-07-T1 | 07 | 7 | SOC-* | API + architecture docs | docs | `rg -n 'star\|explore\|fork\|getPublicProfile' docs/API.md` | ✅ | ✅ green |
| 21-07-T2 | 07 | 7 | SOC-* | Phase gate verification | mixed | gate commands below | ✅ | ✅ green |
| 22.1 (post-close) | — | — | SOC-04 / ORG-06 | Fork installs protection hooks; failed install compensates | integration | `test(repo_fork_installs_protection_hooks)` + `test(repo_fork_failed_when_hook_install_blocked)` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

## Phase gate (final plan)

- [x] `cargo nextest run -p oxidean-api -E 'test(repo_stars) | test(repo_fork) | test(repo_explore) | test(user_public_profile)'` — 13 passed (2026-09-16)
- [x] `cargo nextest run -p oxidean-db -E 'test(dialect_social)'` — pass
- [x] `make rpc-sync-check` — ok
- [x] Web: vitest social filters + `make web-lint` / `make web-format-check`
- [x] Docs: `docs/API.md` lists star/explore/profile/fork procedures

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Star/fork counts and explore ordering in a live multi-user stack | SOC-01/03/04 | Multi-account data shaping | Seed two users + repos; star/fork across accounts; confirm `/explore` ordering and counters |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 240s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-10-02 (retroactive audit below; `21-VERIFICATION.md` passed 2026-09-19)

---

## ASSUME

- Migration logical name resolved to **`0020_social`** (0016 is `pull_requests` from Phase 12).
- Phase 12 `forked_from_repo_id` + `repo.fork` / `clone_bare` extended — not duplicated.

---

## Validation Audit 2026-10-02

Retroactive Nyquist audit (ROADMAP DEBT-08) — same class of reconcile as `22.1-09` for phases 8/14/15/20. Scope: add standard frontmatter + per-task map, confirm every mapped command still names real files/tests on disk, and confirm recorded green runs exist; suites not re-executed in this docs pass.

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 1 (draft-era doc upgraded: frontmatter added, Wave 0 checklist mapped to real task rows, fork hook-install row appended for Phase 22.1 scope) |
| Escalated | 0 |
| Manual-only | 1 (live multi-user explore — documented above) |

| Check | Result |
|-------|--------|
| `repo_stars.rs` (`repo_stars_star_unstar_idempotent`, `_anonymous_rejected`, `_private_without_read_not_found`, `_list_starred_pagination`) | ✅ present; named tests exist |
| `user_public_profile.rs` (`_get_no_email`, `_repos_acl`, `_unknown_not_found`) | ✅ present; named tests exist |
| `repo_explore.rs` (`_anonymous_public_sorted`, `_q_filter`) | ✅ present; named tests exist |
| `repo_fork.rs` (`_public_ok_network_id`, `_private_source_denied`, `_one_per_owner_network`, `_head_valid_for_base`, `_installs_protection_hooks`, `_failed_when_hook_install_blocked`) | ✅ present; named tests exist (hook cases added by 22.1) |
| `crates/oxidean-db/tests/dialect_social.rs` | ✅ present (`dialect_social_schema_presence`) |
| Web: `explore.integration.test.ts`, `$owner.user-profile.integration.test.ts`, `repo-chrome.social.integration.test.ts`, `repo-about-sidebar.social.integration.test.ts`, `repo-social-lists.unit.test.ts`, `$owner.$repo.social-lists.integration.test.ts` | ✅ all present |
| Recorded green runs | Phase gate above (13 passed, 2026-09-16); `21-VERIFICATION.md` truths PASS (verified 2026-09-19), `status: passed` |

**Verdict:** `status: validated`, `nyquist_compliant: true`. Previously unbucketed because the file had no frontmatter; content already matched disk.
