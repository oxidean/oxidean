# Phase 21: Social & Explore - Validation

**Nyquist / Wave 0 checklist** — every later plan `<automated>` must name a real path from this list (or create it in 21-00).

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

## Phase gate (final plan)

- [x] `cargo nextest run -p oxidean-api -E 'test(repo_stars) | test(repo_fork) | test(repo_explore) | test(user_public_profile)'` — 13 passed (2026-09-16)
- [x] `cargo nextest run -p oxidean-db -E 'test(dialect_social)'` — pass
- [x] `make rpc-sync-check` — ok
- [x] Web: vitest social filters + `make web-lint` / `make web-format-check`
- [x] Docs: `docs/API.md` lists star/explore/profile/fork procedures

## ASSUME

- Migration logical name resolved to **`0020_social`** (0016 is `pull_requests` from Phase 12).
- Phase 12 `forked_from_repo_id` + `repo.fork` / `clone_bare` extended — not duplicated.
