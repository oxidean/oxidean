---
phase: 20-packages-registry
plan: "06"
subsystem: api
tags: [packages, npm, registry]

requires:
  - phase: 20-packages-registry
    provides: mounts + CA store + ACL
provides:
  - npm packument GET, publish PUT, tarball GET under /npm/{owner}/
affects: [20-07, 20-08, 20-12]

actuals:
  tokens: 18000
  tasks: 2
  commits: 1

tech-stack:
  added: []
  patterns: [CouchDB publish attachments, OXIDEAN_PUBLIC_ORIGIN tarball URLs]

key-files:
  created: []
  modified:
    - crates/oxidean-api/src/packages/npm.rs
    - crates/oxidean-api/tests/npm_registry.rs
    - crates/oxidean-db/src/packages.rs

key-decisions:
  - "Package description column stores npm dist-tags JSON for package-level metadata"

requirements-completed: [PKG-02, PKG-04]

coverage:
  - id: D1
    description: npm publish/packument/tarball with PUBLIC_ORIGIN
    requirement: PKG-02
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-api -E 'test(npm_registry)'
        status: pass
    human_judgment: false

plan_head_before: 0ab6f58
duration: 12min
completed: 2026-09-14
status: complete
---

# Phase 20 Plan 06: npm Publish/Install Summary

**npm registry core: publish with attachments, packument, and PUBLIC_ORIGIN tarball URLs under `/npm/{owner}/`.**

## Task Commits
| Task | Commit |
|------|--------|
| 1–2 | `a331319` |

## Self-Check: PASSED
