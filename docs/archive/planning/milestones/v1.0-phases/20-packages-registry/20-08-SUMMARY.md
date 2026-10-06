---
phase: 20-packages-registry
plan: "08"
subsystem: api
tags: [packages, rpc]
requires:
  - phase: 20-packages-registry
    provides: package metadata + ACL
provides:
  - packages.list / packages.deleteVersion session RPC
  - generated api-client packages.* helpers
affects: [20-09, 20-10, 20-11]
actuals: { tokens: 12000, tasks: 2, commits: 1 }
tech-stack: { added: [], patterns: [type-to-confirm deleteVersion] }
key-files:
  created: [crates/oxidean-core/src/package_types.rs, crates/oxidean-api/src/packages/rpc.rs]
  modified: [crates/oxidean-api/src/rpc.rs, packages/api-client/src/index.ts]
key-decisions: ["confirm string must equal name@version server-side"]
requirements-completed: [PKG-05]
coverage:
  - id: D1
    description: package_rpc list/delete green
    requirement: PKG-05
    verification: [{ kind: integration, ref: "cargo nextest run -p oxidean-api -E test(package_rpc)", status: pass }]
    human_judgment: false
plan_head_before: 81e028e
duration: 20min
completed: 2026-09-14
status: complete
---
# Phase 20 Plan 08: Packages RPC Summary
**Session RPC for listing packages and Admin type-to-confirm version delete (PKG-05).**
## Task Commits
| Task | Commit |
|------|--------|
| 1–2 | `222551b` |
## Self-Check: PASSED
