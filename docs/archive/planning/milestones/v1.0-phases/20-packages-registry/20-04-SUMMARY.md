---
phase: 20-packages-registry
plan: "04"
subsystem: api
tags: [packages, generic, registry, axum]

requires:
  - phase: 20-packages-registry
    provides: CA store + ACL∩PAT + 0015_packages schema
provides:
  - Generic PUT/GET/DELETE/list at /generic/{owner}/{name}/…
  - Axum mounts for /v2 /npm /generic
  - OCI discovery GET /v2/
affects: [20-05, 20-06, 20-07, 20-08]

actuals:
  tokens: 9590
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns: [generic immutable version files, nest registry prefixes, PAT Basic on /generic]

key-files:
  created:
    - crates/oxidean-api/src/packages/generic.rs
    - crates/oxidean-api/src/packages/oci.rs
    - crates/oxidean-api/src/packages/npm.rs
  modified:
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-api/src/packages/mod.rs
    - crates/oxidean-api/tests/generic_registry.rs
    - crates/oxidean-db/src/packages.rs
    - crates/oxidean-db/src/lib.rs

key-decisions:
  - "Per-file immutability within a version (409 on same filename); multi-file versions allowed"
  - "GET /v2 and /v2/ discovery mounted on app router; nested /v2 handles subpaths only"

patterns-established:
  - "packages/{generic,oci,npm}.rs nested from app.rs under path prefixes"

requirements-completed: [PKG-03, PKG-04]

coverage:
  - id: D1
    description: Generic PUT/GET/DELETE with 409 overwrite and republish after delete
    requirement: PKG-03
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-api -E 'test(generic_registry)'
        status: pass
    human_judgment: false
  - id: D2
    description: List versions JSON + /v2/ discovery from Axum
    requirement: PKG-03
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-api -E 'test(generic_registry_list)'
        status: pass
    human_judgment: false

plan_head_before: 3283b21d7bcd7938be4135a65e7ec06f52ed6d97
duration: 12min
completed: 2026-09-14
status: complete
---

# Phase 20 Plan 04: Generic Registry Tracer Summary

**End-to-end generic publish/pull/delete/list through ACL + CA store, with `/v2`/`/npm`/`/generic` mounted on Axum.**

## Performance

- **Duration:** ~12 min
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- Tracer path: PUT/GET `/generic/{owner}/{name}/{version}/{filename}` with PAT Basic (`package:write`)
- Immutable file overwrite → 409; DELETE version (Admin ∩ package:write) then republish allowed
- Anonymous GET for public packages; list returns versions/files JSON
- OCI discovery `GET /v2/` returns 200 + `Docker-Distribution-API-Version`; npm stub 404

## Task Commits

| Task | Commit | Description |
|------|--------|-------------|
| 1 (RED) | `1ef4c6c` | Failing generic_registry integration tests |
| 1–2 (GREEN) | `e1bc3f5` | Generic handlers + mounts + list/metadata |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Overlapping `/v2` nest vs discovery routes**
- **Found during:** Task 1 green
- **Issue:** Nesting `oci::router()` with `/` plus app-level `/v2` panicked on overlapping routes
- **Fix:** App mounts discovery on `/v2` and `/v2/`; nested router only serves `/{*rest}` stubs
- **Files modified:** `oci.rs`, `app.rs`
- **Commit:** `e1bc3f5`

## TDD Gate Compliance

- RED: `generic_registry_put_file` failed 404 vs 201 — `RED_EVIDENCE_OK`
- GREEN: all five `generic_registry_*` tests pass
- Tracer verify re-run: pass (end-of-phase human verify deferred)

## Self-Check: PASSED

- FOUND: crates/oxidean-api/src/packages/generic.rs
- FOUND: crates/oxidean-api/src/app.rs mounts
- FOUND: commits `1ef4c6c`, `e1bc3f5`
