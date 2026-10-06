---
phase: 10-orgs-permissions
plan: "03"
subsystem: api
tags: [orgs, owner-ref, polymorphic-owner, repo.create, d-org-01, org-01, org-03]
requires:
  - phase: 10-orgs-permissions
    provides: "0010 owner_type + org.create tracer (plans 02/13)"
provides:
  - "OwnerRef shared slug resolve (user then org)"
  - "repository owner_type on insert/select + polymorphic disk refs"
  - "repo.create optional owner slug (Owner/Admin org create)"
affects:
  - 10-04 ACL rewrite
  - 10-11 /new owner picker
  - Smart HTTP / browse consumers of resolve
actuals:
  tokens: 12352
  tasks: 2
  commits: 5
tech-stack:
  added: []
  patterns:
    - "OwnerRef: find user by username then org by slug"
    - "repo.create_forbidden for unauthorized owner claims"
key-files:
  created:
    - .planning/phases/10-orgs-permissions/10-03-t1-red-evidence.json
    - .planning/phases/10-orgs-permissions/10-03-t2-red-evidence.json
  modified:
    - crates/oxidean-api/src/repo/acl.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/routes/git_smart_http.rs
    - crates/oxidean-db/src/repositories.rs
    - crates/oxidean-core/src/repo_types.rs
    - packages/api-client/src/index.ts
key-decisions:
  - "User-first then org slug resolve (T-10-07); never trust client owner_id alone"
  - "Org repo create requires Owner/Admin only; Members get repo.create_forbidden (A5)"
  - "RepoPublic.owner_type exposed; owner_username remains public slug label"
patterns-established:
  - "resolve_owner_slug shared by web ACL and Smart HTTP resolve_repo"
  - "insert_repository requires explicit owner_type (user|org)"
requirements-completed: [ORG-01, ORG-03]
coverage:
  - id: D1
    description: "Org slug resolves public org-owned repos via OwnerRef"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/org_create_members.rs#org_create_owned_repo_resolves_by_org_slug"
        status: pass
    human_judgment: false
  - id: D2
    description: "Org Owner creates org-owned bare repo on disk under org slug"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_create.rs#repo_create_under_org_as_owner"
        status: pass
    human_judgment: false
  - id: D3
    description: "Org Member and other-user owner claims denied (repo.create_forbidden)"
    requirement: ORG-03
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_create.rs#repo_create_under_org_as_member_denied"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_create.rs#repo_create_under_other_user_denied"
        status: pass
    human_judgment: false
duration: 17min
completed: 2026-09-14
status: complete
plan_head_before: dbb1eace63fbd26f8cc5d9824dbf9d638532f80c
commits: 5
---

# Phase 10 Plan 03: Polymorphic Owner Resolution + Org-Owned Repo Create Summary

**Shared OwnerRef resolves user or org slugs; `repo.create` accepts optional owner with Owner/Admin-only org creates (D-ORG-01 / A5).**

## Performance

- **Duration:** 17 min
- **Started:** 2026-09-14T00:12:30Z
- **Completed:** 2026-09-14T00:30:00Z
- **Tasks:** 2
- **Files modified:** 18

## Accomplishments

- `OwnerRef` + `resolve_owner_slug` used by web `resolve_repo_for_read` and Smart HTTP `resolve_repo`
- `repositories.owner_type` plumbed through insert/select; disk refs join user or org slug
- `CreateRepoRequest.owner` + `RepoPublic.owner_type`; rpc-gen / rpc-sync-check green

## Task Commits

1. **Task 1 RED: OwnerRef resolve test** - `b129dc2` (test) + `4de7e47` (evidence schema)
2. **Task 1 GREEN: OwnerRef + owner_type plumbing** - `8b08fbf` (feat)
3. **Task 2 RED: repo.create owner auth tests** - `eab5486` (test)
4. **Task 2 GREEN: optional owner + rpc-gen** - `c25c694` (feat)

## Files Created/Modified

- `crates/oxidean-api/src/repo/acl.rs` — OwnerRef + resolve_owner_slug
- `crates/oxidean-api/src/repo/mod.rs` — create owner authorization (A5)
- `crates/oxidean-api/src/routes/git_smart_http.rs` — shared OwnerRef resolve
- `crates/oxidean-db/src/repositories.rs` — owner_type column + polymorphic disk refs
- `crates/oxidean-core/src/repo_types.rs` — CreateRepoRequest.owner, RepoPublic.owner_type
- `packages/api-client/src/index.ts` — generated client surface

## Decisions Made

- Resolve user username before org slug; missing → not_found / unauthorized_basic
- Unauthorized create owner → `repo.create_forbidden` (HTTP 403)
- Keep `can_read_as_owner` for private checks until plan 04 ACL rewrite

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Critical] Polymorphic list_repo_disk_refs**
- **Found during:** Task 1 (OwnerRef plumbing)
- **Issue:** Disk-ref query INNER JOIN users only — org-owned rows would vanish from orphan reconcile
- **Fix:** LEFT JOIN users/orgs keyed by owner_type; CASE slug selection
- **Files modified:** `crates/oxidean-db/src/repositories.rs`
- **Commit:** `8b08fbf`

**2. [Rule 3 - Blocking] Web fixtures for RepoPublic.owner_type**
- **Found during:** Task 2 (rpc-gen)
- **Issue:** Integration mocks omitted new required `owner_type`
- **Fix:** Add `owner_type: "user"` to RepoPublic fixtures
- **Files modified:** apps/web test fixtures
- **Commit:** `c25c694`

## TDD Gate Compliance

- Task 1: RED `org_create_owned_repo_resolves_by_org_slug` (404) → GREEN OwnerRef
- Task 2: RED `repo_create_under_*` (ignored owner / 200) → GREEN authorize + insert owner_type

## Self-Check: PASSED

- Key artifacts present (acl.rs, repositories.rs, repo_types.rs, api-client, SUMMARY)
- Commits verified: b129dc2, 4de7e47, 8b08fbf, eab5486, c25c694
