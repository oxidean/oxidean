---
phase: "12"
slug: "pull-requests"
status: executed
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-16"
updated: "2026-09-16"
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
