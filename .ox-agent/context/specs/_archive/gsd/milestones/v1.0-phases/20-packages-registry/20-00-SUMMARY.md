---
phase: 20-packages-registry
plan: "00"
subsystem: testing
tags: [packages, oci, npm, generic, wave0, nextest]

requires:
  - phase: 08-git-https-pats
    provides: PAT auth test patterns
  - phase: 10-orgs-permissions
    provides: ACL capability ladder patterns
provides:
  - Wave 0 discoverable RED stubs for OCI/npm/generic registry, package ACL, package RPC, dialect_packages
affects: [20-02, 20-03, 20-04, 20-05, 20-06, 20-07, 20-08, 20-09]

actuals:
  tokens: 2484
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns: [Wave 0 nextest RED stubs with named filters]

key-files:
  created:
    - crates/oxidean-api/tests/oci_registry.rs
    - crates/oxidean-api/tests/npm_registry.rs
    - crates/oxidean-api/tests/generic_registry.rs
    - crates/oxidean-api/tests/package_acl.rs
    - crates/oxidean-api/tests/package_rpc.rs
    - crates/oxidean-db/tests/dialect_packages.rs
  modified: []

key-decisions:
  - "Used assert!(false) without #[ignore] so nextest list discovers stubs without --run-ignored (matches plan verify)"

patterns-established:
  - "Phase 20 registry protocol tests live under crates/oxidean-api/tests/{oci,npm,generic}_registry.rs and package_*"

requirements-completed: [PKG-01, PKG-02, PKG-03, PKG-04, PKG-05]

coverage:
  - id: D1
    description: Wave 0 OCI/npm/generic/ACL/RPC nextest stubs discoverable
    requirement: PKG-01
    verification:
      - kind: integration
        ref: cargo nextest list -p oxidean-api -E 'test(oci_registry)|test(npm_registry)|test(generic_registry)|test(package_acl)|test(package_rpc)'
        status: pass
    human_judgment: false
  - id: D2
    description: Wave 0 dialect_packages stub discoverable
    requirement: PKG-04
    verification:
      - kind: integration
        ref: cargo nextest list -p oxidean-db -E 'test(dialect_packages)'
        status: pass
    human_judgment: false

plan_head_before: 367ed7cd2326fa1e6d0eb3dd25be3cd55157c2d7
duration: 6min
completed: 2026-09-14
status: complete
---

# Phase 20 Plan 00: Wave 0 Rust stubs Summary

**Nyquist Wave 0 RED stubs for OCI, npm, generic, package ACL/RPC, and dialect packages — discoverable by nextest filters before handlers exist.**

## Performance

- **Duration:** ~6 min
- **Started:** 2026-09-14T16:55:07Z
- **Completed:** 2026-09-14T17:00:58Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- Added five API integration stub binaries covering PKG-01…05 protocol/ACL/RPC expectations (D-PKG-01, 04–06, 10, 13–16)
- Added dialect_packages stub expecting packages/versions/blobs/blob_refs/quota_overrides
- Stubs intentionally fail until later plans green them; listed by default nextest list

## Task Commits

1. **Task 1: Protocol + ACL + RPC Wave 0 stubs** - `c8ff537` (test)
2. **Task 2: dialect_packages Wave 0 stub** - `817dab5` (test)

## Files Created/Modified
- `crates/oxidean-api/tests/oci_registry.rs` — OCI Distribution Spec stubs
- `crates/oxidean-api/tests/npm_registry.rs` — npm registry API stubs
- `crates/oxidean-api/tests/generic_registry.rs` — generic/raw stubs
- `crates/oxidean-api/tests/package_acl.rs` — hybrid ACL∩PAT stubs
- `crates/oxidean-api/tests/package_rpc.rs` — list/delete RPC stubs
- `crates/oxidean-db/tests/dialect_packages.rs` — migration parity stub

## Decisions Made
- Non-ignored `assert!(false)` stubs so plan verify `nextest list` (without `--run-ignored`) finds them

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Format-string braces in stub messages**
- **Found during:** Task 1
- **Issue:** `assert!(false, ".../{owner}/...")` treated `{owner}` as format args
- **Fix:** Rewrote messages with `<owner>` angle-bracket placeholders
- **Files modified:** oci/npm/generic stub files
- **Commit:** c8ff537

## Self-Check: PASSED

- FOUND: crates/oxidean-api/tests/oci_registry.rs
- FOUND: crates/oxidean-api/tests/npm_registry.rs
- FOUND: crates/oxidean-api/tests/generic_registry.rs
- FOUND: crates/oxidean-api/tests/package_acl.rs
- FOUND: crates/oxidean-api/tests/package_rpc.rs
- FOUND: crates/oxidean-db/tests/dialect_packages.rs
- FOUND: c8ff537
- FOUND: 817dab5
