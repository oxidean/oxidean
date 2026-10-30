---
phase: 13-branch-protection
plan: "08"
subsystem: ui
tags: [pull-merge, merge-blocked, docs, branch-protection]
requires:
  - phase: 13-branch-protection
    provides: "pull.merge_blocked + structured reasons"
  - phase: 12-pull-requests
    provides: "PullMergePanel / PR detail"
provides:
  - "PR merge blocker UX (PR-08 / D-24)"
  - "API + ARCHITECTURE docs for protection/status/hooks"
affects: [19-actions-runners, 17-notifications]
actuals:
  tokens: 15000
  tasks: 2
  commits: 1
plan_head_before: 887888d5f39326a629c14e333f722611979ccc2a
tech-stack:
  added: []
  patterns: ["error.data.reasons → labeled blocker list on merge panel"]
key-files:
  modified:
    - apps/web/src/components/repo/pull-merge-panel.tsrx
    - apps/web/src/routes/$owner.$repo.pull.protection.integration.test.ts
    - docs/API.md
    - docs/ARCHITECTURE.md
key-decisions:
  - "Display reasons after failed merge (server remains authoritative)"
  - "Document soft-protect coexistence and Phase 19 status consumers"
requirements-completed: [PR-08, ORG-06]
coverage:
  - id: D1
    description: "PR merge panel shows structured protection blockers"
    requirement: PR-08
    verification:
      - kind: integration
        ref: "apps/web/src/routes/$owner.$repo.pull.protection.integration.test.ts"
        status: pass
      - kind: other
        ref: "make web-lint && make web-format-check"
        status: pass
    human_judgment: false
  - id: D2
    description: "Docs cover branchProtection, commitStatus, merge_blocked, hooks"
    requirement: ORG-06
    verification:
      - kind: other
        ref: "rg branchProtection|commitStatus|merge_blocked docs/API.md docs/ARCHITECTURE.md"
        status: pass
    human_judgment: false
duration: 20min
completed: 2026-09-16
status: complete
---

# Phase 13 Plan 08: PR Blockers + Docs Summary

**PR detail lists `pull.merge_blocked` reasons; API/ARCHITECTURE document protection CRUD, statuses, merge blocks, and shared bare-repo hooks.**

## Accomplishments

- `PullMergePanel` extracts `error.data.reasons` and labels reviews/checks/conversations/up_to_date/locked/etc.
- Docs: `repo.branchProtection.*`, `repo.commitStatus.*`, `pull.merge_blocked`, Smart HTTP/SSH hook enforcement, soft-protect coexistence.
- Vitest `pull.protection` green; API nextest protection suite 11/11 pass.

## Deviations from Plan

None - plan executed as written.

## Self-Check: PASSED
