---
phase: 20-packages-registry
plan: "10"
subsystem: ui
tags: [packages, octane, tanstack-query]
requires:
  - phase: 20-packages-registry
    provides: packages.list / deleteVersion RPC
provides:
  - Owner and repo packages Octane pages
  - Type-to-confirm delete dialog
affects: [20-12]
actuals: { tokens: 12000, tasks: 2, commits: 1 }
tech-stack: { added: [], patterns: [Rivet .tsrx + Query session helpers] }
key-files:
  created:
    - apps/web/src/routes/$owner.packages.tsrx
    - apps/web/src/routes/$owner.$repo.packages.tsrx
    - apps/web/src/components/packages/delete-version-dialog.tsrx
  modified: [apps/web/src/routeTree.gen.ts]
key-decisions: ["Delete confirm matches server name@version string"]
requirements-completed: [PKG-05]
coverage:
  - id: D1
    description: Owner/repo packages Vitest green
    requirement: PKG-05
    verification: [{ kind: automated_ui, ref: "vitest $owner.packages + $owner.$repo.packages", status: pass }]
    human_judgment: false
plan_head_before: 9d20fdf
duration: 15min
completed: 2026-09-14
status: complete
---
# Phase 20 Plan 10: Packages UI Summary

**Owner and repo-linked packages pages with type-to-confirm version delete (PKG-05).**

## Task Commits

| Task | Commit |
|------|--------|
| 1–2 | `a5825e2` |

## Self-Check: PASSED
