---
phase: 15-releases-transfer
plan: "06"
subsystem: ui
tags: [octane, releases, tanstack-query, tsrx]
requires:
  - phase: 15-releases-transfer
    provides: "release.* RPC + api-client from 15-01"
provides:
  - "Releases tab in repo chrome"
  - "/releases list, /releases/new create, /releases/$tag detail notes"
affects: [15-02, 15-05]
actuals:
  tokens: 8726
  tasks: 2
  commits: 1
plan_head_before: a4e1a419a60a881c0fb8c45d0d0d062d01423a70
tech-stack:
  added: []
  patterns:
    - "Octane Rivet routes with TanStack Query + createFileRoute for releases"
key-files:
  created:
    - apps/web/src/routes/$owner.$repo.releases.tsrx
    - apps/web/src/routes/$owner.$repo.releases.new.tsrx
    - apps/web/src/routes/$owner.$repo.releases.$tag.tsrx
  modified:
    - apps/web/src/components/repo/repo-chrome.tsrx
    - apps/web/src/routeTree.gen.ts
key-decisions:
  - "Notes-only UI; asset upload deferred to 15-02"
  - "Tag picker from repo.refs (existing tags only, D-REL-01)"
requirements-completed: [GIT-14]
coverage:
  - id: D1
    description: "Releases tab + list/new/detail notes routes"
    requirement: GIT-14
    verification:
      - kind: integration
        ref: "bun run test src/routes/$owner.$repo.releases.integration.test.ts"
        status: pass
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
    human_judgment: false
duration: 8min
completed: 2026-09-14
status: complete
---

# Phase 15 Plan 06: Releases UI Notes Summary

**Octane Releases tab lists releases, creates notes for an existing tag, and shows detail notes via renderGfm (GIT-14 / D-REL-13).**

## Performance

- **Duration:** 8 min
- **Tasks:** 2/2
- **Commits:** 1

## Accomplishments

- Added Releases nav to `RepoChrome` (`active: "releases"`)
- List/create/detail `.tsrx` routes calling `release.list` / `create` / `get`
- Tag select from `repo.refs`; Write+ gated create form
- Vitest integration green; web build green

## Task Commits

| Task | Commit |
|------|--------|
| 1 tab + list + create | (same) |
| 2 detail + Vitest | (same) |

Combined into one `feat(15-06)` commit.

## Deviations from Plan

**1. [Rule 3] Combined T1–T2 commit** — shared chrome + routes; single commit for green build+test.

## Known Stubs

None for notes UX. Asset upload controls intentionally deferred to 15-02.

## Self-Check: PASSED

- FOUND: releases routes + chrome Releases tab
- FOUND: vitest + build green
