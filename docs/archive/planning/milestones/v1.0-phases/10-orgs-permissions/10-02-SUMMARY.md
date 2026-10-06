---
phase: 10-orgs-permissions
plan: "02"
subsystem: database
tags: [orgs, acl, migration, 0010_orgs_acl, polymorphic-owner, org-01, d-org-01, d-org-02a]

requires:
  - phase: 10-orgs-permissions
    provides: Wave 0 dialect_orgs RED stubs (10-00) + schema door decision proceed_0010
provides:
  - "Tri-dialect 0010_orgs_acl (organizations, members, invites, collaborators, owner_type)"
  - "Database insert organization + Owner membership helpers"
  - "oxidean-core org_types DTOs for later org.create"
affects:
  - 10-13 org.create tracer
  - 10-04/10-05 ACL + members
  - 10-06 invites
  - 10-07 collaborators

actuals:
  tokens: 9094
  tasks: 1
  commits: 5

plan_head_before: 0019a5b207a54deb7b6fa4bea4f1cada9779bffb

tech-stack:
  added: []
  patterns:
    - "Polymorphic repositories.owner_type + owner_id (no FK to users) with dialect-specific FK drop/rebuild"
    - "Org ACL tables in single 0010 migration (Phase 09 keeps 0009_ssh_keys)"

key-files:
  created:
    - crates/oxidean-db/migrations/postgres/0010_orgs_acl.sql
    - crates/oxidean-db/migrations/mysql/0010_orgs_acl.sql
    - crates/oxidean-db/migrations/sqlite/0010_orgs_acl.sql
    - crates/oxidean-db/src/organizations.rs
    - crates/oxidean-db/src/org_members.rs
    - crates/oxidean-core/src/org_types.rs
  modified:
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-db/tests/dialect_orgs.rs
    - crates/oxidean-core/src/lib.rs

key-decisions:
  - "Human chose proceed_0010 — migration 0010_orgs_acl + owner_type/owner_id polymorphic repos"
  - "Invite/collaborator tables created empty in 0010; RPCs deferred"
  - "No org.create / rpc-gen / /orgs/new in this plan (10-13)"

patterns-established:
  - "SQLite polymorphic owner: rebuild repositories table under PRAGMA foreign_keys=OFF"
  - "Org helpers mirror pats/repositories DbPool match + Database facade"

requirements-completed: []  # ORG-01 schema foundation only; create RPC/UI greens in 10-13

coverage:
  - id: D1
    description: "Tri-dialect 0010_orgs_acl lands org ACL tables + repositories.owner_type"
    requirement: ORG-01
    verification:
      - kind: unit
        ref: "cargo test -p oxidean-db --lib migration_parity"
        status: pass
      - kind: integration
        ref: "cargo test -p oxidean-db --test dialect_orgs"
        status: pass
    human_judgment: false
  - id: D2
    description: "Database helpers insert organization + Owner membership on sqlite"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "cargo test -p oxidean-db --test dialect_orgs#dialect_orgs_migrate_0010_schema_presence"
        status: pass
    human_judgment: false
  - id: D3
    description: "org_types DTOs compile (OrgRole, MemberBasePermission, OwnerType, OrgPublic, CreateOrgRequest)"
    requirement: ORG-01
    verification:
      - kind: unit
        ref: "cargo test -p oxidean-db --test dialect_orgs (pulls oxidean-core)"
        status: pass
    human_judgment: false

duration: 4min
completed: 2026-09-13
status: complete
---

# Phase 10 Plan 02: 0010 Orgs ACL Schema Door Summary

**Tri-dialect `0010_orgs_acl` + org insert/Owner membership helpers and `org_types` DTOs after human `proceed_0010` (no RPC/UI).**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-13T23:48:24Z
- **Completed:** 2026-09-13T23:52:16Z
- **Tasks:** 1 (Task 0 decision-only; Task 1 implemented)
- **Files modified:** 9

## Accomplishments

- Confirmed schema door via human decision `proceed_0010` (Task 0)
- Landed organizations / organization_members / organization_invites / repository_collaborators on postgres, mysql, sqlite
- Polymorphic `repositories.owner_type` (`user`|`org`) with user FK dropped; existing rows backfill as `user`
- Green `dialect_orgs` insert org + Owner membership + `owner_type` default assertion

## Task Commits

1. **Task 0: Confirm D-ORG-01 schema door** — decision only (no commit); user chose `proceed_0010`
2. **Task 1: 0010_orgs_acl + DB helpers + org_types** — `2968ee0` (feat)

**Plan metadata:** `36ed457` (docs: complete plan)

## Files Created/Modified

- `crates/oxidean-db/migrations/*/0010_orgs_acl.sql` — tri-dialect org ACL schema
- `crates/oxidean-db/src/organizations.rs` — insert / find by id|slug
- `crates/oxidean-db/src/org_members.rs` — insert Owner membership
- `crates/oxidean-db/src/lib.rs` — Database facade wiring
- `crates/oxidean-core/src/org_types.rs` — OrgRole, MemberBasePermission, OwnerType, DTOs
- `crates/oxidean-db/tests/dialect_orgs.rs` — Wave 0 stubs turned green

## Decisions Made

- Proceed with RESEARCH discretion: `0010_orgs_acl` + `owner_type`/`owner_id` (not owners table; not 0009)
- Shared slug uniqueness: UNIQUE(slug) + lower(slug) index; cross-table check remains at RPC (10-13)
- Teams not implemented (D-ORG-07)

## Deviations from Plan

None - plan executed exactly as written after `proceed_0010`.

## Auth Gates

None.

## Known Stubs

None that block this plan's goal. Invite/collaborator **tables** exist without CRUD helpers/RPCs — intentional for later plans (10-06/10-07).

## Threat Flags

None beyond plan threat model (UNIQUE slug; dual-check vs users deferred to 10-13).

## Verification

```text
cargo test -p oxidean-db --lib migration_parity  # ok
cargo test -p oxidean-db --test dialect_orgs     # ok (2 passed)
```

## Self-Check: PASSED

- FOUND: all 0010 SQL + organizations.rs + org_members.rs + org_types.rs
- FOUND: commit `2968ee0`
