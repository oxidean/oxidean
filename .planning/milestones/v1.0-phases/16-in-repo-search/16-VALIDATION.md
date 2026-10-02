---
phase: "16"
slug: "in-repo-search"
status: complete
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-16"
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
