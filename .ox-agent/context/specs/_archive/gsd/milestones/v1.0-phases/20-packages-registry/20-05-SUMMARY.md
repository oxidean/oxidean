---
phase: 20-packages-registry
plan: "05"
subsystem: api
tags: [packages, oci, docker, registry]

requires:
  - phase: 20-packages-registry
    provides: generic tracer mounts + CA store + ACL
provides:
  - OCI Distribution Spec end-1…10 under /v2/{owner}/{image}
  - Bearer token realm at /v2/token
affects: [20-08, 20-09, 20-12]

actuals:
  tokens: 12170
  tasks: 3
  commits: 1

tech-stack:
  added: []
  patterns: [OCI upload sessions, opaque Bearer mint, digest-immutable manifests]

key-files:
  created: []
  modified:
    - crates/oxidean-api/src/packages/oci.rs
    - crates/oxidean-api/tests/oci_registry.rs

key-decisions:
  - "Repository path is {owner}/{image} two-segment (nested image names deferred to path dispatcher)"
  - "Bearer tokens are opaque in-process; PAT Basic also accepted directly on /v2"

patterns-established:
  - "OCI handlers reuse packages/{acl,auth,store}"

requirements-completed: [PKG-01, PKG-04]

coverage:
  - id: D1
    description: OCI push/pull/tags/delete + cookie ignored
    requirement: PKG-01
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-api -E 'test(oci_registry)'
        status: pass
    human_judgment: false

plan_head_before: 5f4189c322cfbff912b8ed717d8c125ee5a9832b
duration: 15min
completed: 2026-09-14
status: complete
---

# Phase 20 Plan 05: OCI Registry Summary

**Fully featured OCI Distribution Spec under `/v2` with Bearer realm, blob uploads, manifests, tags list, delete, and digest/tag immutability rules.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 3 (combined GREEN commit)
- **Files modified:** 2

## Accomplishments

- `/v2/token` mints short-lived Bearer from PAT Basic
- Blob upload POST/PATCH/PUT; manifest GET/PUT/DELETE; tags/list
- Public anon pull; Cookie alone → 401 + WWW-Authenticate
- Referrers API returns 404 (deferred)

## Task Commits

| Task | Commit | Description |
|------|--------|-------------|
| 1–3 | `240ade9` | OCI handlers + greened oci_registry tests |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Axum forbids catch-all mid-route**
- **Found during:** Task 2
- **Issue:** `/{*name}/blobs/...` invalid in axum 0.8
- **Fix:** Use `/{owner}/{image}/...` two-segment repository names
- **Commit:** `240ade9`

**2. [Rule 2] Combined RED/GREEN** — Wave 0 stubs replaced and greened in one commit due to session scope; tests assert full behavior.

## Self-Check: PASSED

- FOUND: oci.rs, oci_registry tests, commit `240ade9`
