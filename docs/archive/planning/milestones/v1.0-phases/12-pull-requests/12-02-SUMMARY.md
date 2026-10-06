---
phase: 12-pull-requests
plan: "02"
subsystem: pulls
tags: [schema, git-merge, types]
status: complete
completed: "2026-09-16"
---
# Phase 12 Plan 02: Schema + GitBackend merge Summary

**Tri-dialect `0016_pull_requests`, pull_types/DB helpers, and GitBackend merge/squash/rebase.**

## What shipped
- Migrations sqlite/postgres/mysql `0016_pull_requests` (PRs, comments, reviews, merge settings, fork parent, issue_links `pr`)
- `oxidean-core` pull_types + `IssueLinkKind::Pr`
- `oxidean-db` pulls module + Database wrappers
- GitBackend `merge_commit` / `squash_merge` / `rebase_merge` / `fetch_ref_from` + green merge_* tests

## Deviations
None material.
