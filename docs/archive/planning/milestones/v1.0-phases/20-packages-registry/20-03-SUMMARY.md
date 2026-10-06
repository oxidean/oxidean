---
phase: 20-packages-registry
plan: "03"
subsystem: api
tags: [packages, acl, pat, blob-store]

requires:
  - phase: 20-packages-registry
    provides: 0015_packages schema + PACKAGES_DIR
provides:
  - Content-addressed blob store under OXIDEAN_PACKAGES_DIR
  - Package ACL ∩ PAT package scopes helpers
  - Classic/FG package:read/write PAT types + rpc-gen
affects: [20-04, 20-05, 20-06, 20-09]

actuals:
  tokens: 13947
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns: [CA blob store sha256 shard, hybrid ACL∩PAT, Cookie-ignored registry auth]

key-files:
  created:
    - crates/oxidean-api/src/packages/mod.rs
    - crates/oxidean-api/src/packages/store.rs
    - crates/oxidean-api/src/packages/acl.rs
    - crates/oxidean-api/src/packages/auth.rs
    - crates/oxidean-db/src/packages.rs
  modified:
    - crates/oxidean-core/src/pat_types.rs
    - crates/oxidean-api/src/pat/mod.rs
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-api/tests/package_acl.rs

key-decisions:
  - "Classic repo scope alone never grants packages (fail closed)"
  - "FG packages perm stored in scopes_json as package:read/write"

patterns-established:
  - "packages/{store,acl,auth}.rs shared by all three protocol adapters"

requirements-completed: [PKG-04]

coverage:
  - id: D1
    description: Blob store put/get + DB refcount
    requirement: PKG-04
    verification:
      - kind: unit
        ref: cargo test -p oxidean-api --lib packages::store
        status: pass
    human_judgment: false
  - id: D2
    description: package_acl matrix green
    requirement: PKG-04
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-api -E 'test(package_acl)'
        status: pass
    human_judgment: false

plan_head_before: cf7ab4c
duration: 25min
completed: 2026-09-14
status: complete
---

# Phase 20 Plan 03: Store + ACL + PAT Scopes Summary

**Shared packages subsystem: content-addressed blob store, hybrid ACL∩PAT helpers, and `package:read`/`package:write` PAT scopes with greened package_acl matrix.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 3
- **Files modified:** 11

## Accomplishments
- `packages/store.rs` — sha256 CA layout under `OXIDEAN_PACKAGES_DIR`
- `packages/acl.rs` + `auth.rs` — Capability ladder ∩ package scopes; Cookie ignored
- Classic/FG PAT extensions; `make rpc-gen` / `rpc-sync-check` clean

## Task Commits

1. **Task 1: Content-addressed package blob store** - `4b39c06` (feat)
2. **Task 2: Package ACL + registry auth helpers** - `5084820` (feat)
3. **Task 3: Extend PAT classic + FG package scopes** - `44302ee` (feat)

## Decisions Made
- Fail closed: classic `repo` alone denied for packages
- FG packages encoded via scopes_json package scopes

## Deviations from Plan
None material — api-client unchanged if DTOs already mirrored via serde on create inputs.

## Self-Check: PASSED
- FOUND: store/acl/auth + packages db module
- FOUND: package_acl nextest green
- FOUND: 4b39c06, 5084820, 44302ee
