---
phase: 20-packages-registry
plan: "07"
subsystem: api
tags: [packages, npm, dist-tags, search]

requires:
  - phase: 20-packages-registry
    provides: npm publish/install
provides:
  - dist-tags, deprecate, /-/v1/search
affects: [20-12]

actuals:
  tokens: 2000
  tasks: 2
  commits: 1

tech-stack:
  added: []
  patterns: [deprecate without tarball rewrite]

key-files:
  modified:
    - crates/oxidean-api/src/packages/npm.rs
    - crates/oxidean-api/tests/npm_registry.rs

key-decisions:
  - "Deprecate updates version metadata_json only; tarball bytes unchanged"

requirements-completed: [PKG-02]

coverage:
  - id: D1
    description: dist-tags, deprecate, search green
    requirement: PKG-02
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-api -E 'test(npm_registry)'
        status: pass
    human_judgment: false

plan_head_before: a331319
duration: 2min
completed: 2026-09-14
status: complete
---

# Phase 20 Plan 07: npm Dist-tags/Deprecate/Search Summary

**D-PKG-14 completed: dist-tags CRUD, deprecate metadata, and visibility-aware search.**

## Task Commits
| Task | Commit |
|------|--------|
| 1–2 | `b54bf90` (features in `a331319`) |

## Self-Check: PASSED
