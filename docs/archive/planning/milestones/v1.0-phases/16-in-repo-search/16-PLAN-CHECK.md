# Phase 16 — Plan Check (planner self-review)

**Date:** 2026-09-16  
**Plans:** 16-00 … 16-03  
**Schema validate:** all four `frontmatter.validate` + `verify.plan-structure` → valid

## Source coverage audit

| Source | Item | Coverage |
|--------|------|----------|
| GOAL | Search code+commits readable repos | 16-01, 16-02, 16-03 |
| GOAL | Search issues+PRs readable repos | 16-02, 16-03 |
| REQ | GIT-18 | All plans `requirements: [GIT-18]` |
| CONTEXT | D-SRCH-01…16 | Cited across plan objectives/tasks/must_haves |
| CONTEXT | Deferred (global/indexer/full grammar) | Excluded via prohibitions |
| RESEARCH | git grep / log_search / ENV / UI | 16-01…03 |

## Decision coverage

| Decision | Plan |
|----------|------|
| D-SRCH-01 GlobalSearch off | 00, 01, 03 |
| D-SRCH-02 Tabs | 03 (UI), 02 (types) |
| D-SRCH-03 Chrome entry | 03 |
| D-SRCH-04/05 ACL | 01 |
| D-SRCH-06 git grep | 01 |
| D-SRCH-07 log_search | 02 |
| D-SRCH-08 caps/timeout | 01 partial, 03 full |
| D-SRCH-09 issues | 02 |
| D-SRCH-10/11 pulls separate | 02 |
| D-SRCH-12/13 qualifiers | 02 |
| D-SRCH-14 repo.search | 01 |
| D-SRCH-15/16 UI chrome | 01 tracer, 03 full |

## Waves

```
Wave 1: 16-00
Wave 2: 16-01 (depends 00)
Wave 3: 16-02 (depends 01) — precondition Phase 12
Wave 4: 16-03 (depends 02)
```

## Residual risks

- Execute blocked until Phase 12 PR schema lands (documented precondition).
- Large-repo timeout tests may be flaky — prefer deterministic truncate assertions.
