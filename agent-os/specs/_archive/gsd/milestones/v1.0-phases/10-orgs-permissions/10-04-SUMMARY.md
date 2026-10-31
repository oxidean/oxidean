---
phase: 10-orgs-permissions
plan: "04"
subsystem: api
tags: [acl, capability, org-permissions, repo.not_found, rpc-gen]

requires:
  - phase: 10-orgs-permissions-03
    provides: OwnerRef slug resolve + org-owned repo.create
provides:
  - "Capability enum + highest-wins coalesce in repo/acl.rs"
  - "effective_capability DB-backed evaluation"
  - "resolve_repo_for_read gated by meets(Read)"
  - "RepoPublic.can_admin / can_write for settings UI"
affects:
  - 10-05 mutate ACL
  - 10-07 collaborators
  - Smart HTTP / PAT FG selection

actuals:
  tokens: 7804
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Highest-wins Capability coalesce (personal owner, org role×member_base, collaborator, public)"
    - "ACL decisions never embed HTTP status — callers map not_found / 401"

key-files:
  created:
    - crates/oxidean-db/src/repo_collaborators.rs
  modified:
    - crates/oxidean-api/src/repo/acl.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - crates/oxidean-core/src/repo_types.rs
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-db/src/org_members.rs
    - crates/oxidean-db/src/organizations.rs
    - crates/oxidean-api/tests/repo_private_404.rs
    - crates/oxidean-api/tests/repo_collaborators_acl.rs
    - packages/api-client/src/index.ts

key-decisions:
  - "D-ORG-05 as highest-wins coalesce (A1) — Collaborator raises, cannot lower Owner/Admin"
  - "resolve_repo_for_read always requires meets(Read); public visibility bumps to Read in coalesce"
  - "can_read_as_owner retained for Smart HTTP until a later plan rewires git ACL"
  - "Collaborator CRUD integration tests #[ignore] until plan 07"

patterns-established:
  - "Pattern: effective_capability(db, caller, repo, owner_ref) → Option<Capability>"
  - "Pattern: AccessibleRepo carries capability for RepoPublic can_admin/can_write"

requirements-completed: [ORG-02, ORG-04]

coverage:
  - id: D1
    description: "Highest-wins Capability coalesce matrix (personal/org/member_base/collaborator/public)"
    requirement: ORG-02
    verification:
      - kind: unit
        ref: "cargo nextest run -p oxidean-api -E 'test(coalesce)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Private org Owner can read + can_admin; stranger soft repo.not_found (D-25)"
    requirement: ORG-04
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_private_404.rs#repo_private_404_org_non_member_soft_not_found"
        status: pass
    human_judgment: false
  - id: D3
    description: "Public org anonymous read OK with member_base none"
    requirement: ORG-04
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_private_404.rs#repo_private_404_org_public_anonymous_ok"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-09-14
status: complete
plan_head_before: 263f3026cc4a2c930ed55ad0688ea53215559c9a
commits: 2
---

# Phase 10 Plan 04: Capability ACL Summary

**Central Capability coalesce replaces owner-only private ACL; web reads use meets(Read) with soft `repo.not_found` and `RepoPublic.can_admin`.**

## Performance

- **Duration:** 6 min
- **Started:** 2026-09-14T00:30:53Z
- **Completed:** 2026-09-14T00:36:39Z
- **Tasks:** 2/2
- **Files modified:** 11

## Accomplishments

- Capability / OrgRole / MemberBasePermission + highest-wins `coalesce` with green Wave 0 matrix unit tests
- Dialect-safe DB lookups for org member role, member_base_permission, and repository collaborators
- `resolve_repo_for_read` rewritten through `effective_capability`; raw/archive already call it (D-ORG-05)
- `RepoPublic.can_admin` / `can_write` shipped via core + `make rpc-gen`

## Task Commits

1. **Task 1: Capability coalesce + DB lookups** - `7c86ba9` (feat)
2. **Task 2: resolve_repo_for_read + RepoPublic.can_admin** - `c9b942b` (feat)

## Files Created/Modified

- `crates/oxidean-api/src/repo/acl.rs` — Capability ACL + effective_capability + resolve rewrite
- `crates/oxidean-db/src/repo_collaborators.rs` — find_collaborator dialect helper
- `crates/oxidean-db/src/org_members.rs` / `organizations.rs` / `lib.rs` — role + member_base lookups
- `crates/oxidean-core/src/repo_types.rs` — can_admin / can_write on RepoPublic
- `crates/oxidean-api/src/bin/rpc_gen.rs` + `packages/api-client` — client types
- `crates/oxidean-api/tests/repo_private_404.rs` — org Owner vs stranger + public anon
- `crates/oxidean-api/tests/repo_collaborators_acl.rs` — ignored until plan 07

## Decisions Made

- Highest-wins coalesce (ASSUME A1) over first-match short-circuit of D-ORG-05 prose
- Keep `can_read_as_owner` for Smart HTTP until git ACL plan; web path fully on Capability
- Leave collaborator grant integration tests ignored until CRUD RPCs (plan 07)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] rpc-gen template missing can_admin fields**
- **Found during:** Task 2
- **Issue:** Updating `RepoPublic` in core alone left `rpc_gen.rs` template / api-client without `can_admin`/`can_write`
- **Fix:** Extended the TypeScript template in `rpc_gen.rs` and re-ran `make rpc-gen`
- **Files modified:** `crates/oxidean-api/src/bin/rpc_gen.rs`, `packages/api-client/src/index.ts`
- **Commit:** `c9b942b`

## Known Stubs

| File | Line | Stub | Reason |
|------|------|------|--------|
| `crates/oxidean-api/tests/repo_collaborators_acl.rs` | (all) | `#[ignore]` Wave 0 collab CRUD | Plan 07 owns collaborator RPCs |
| `crates/oxidean-api/tests/repo_private_404.rs` | collaborator_granted_read | `#[ignore]` | Needs collaborator grant path (plan 07) |
| Smart HTTP (`git_smart_http.rs`) | can_read_as_owner | Still owner-id stub | Deferred — not this plan's web ACL surface |

## Self-Check: PASSED

- FOUND: crates/oxidean-api/src/repo/acl.rs
- FOUND: crates/oxidean-db/src/repo_collaborators.rs
- FOUND: crates/oxidean-core/src/repo_types.rs
- FOUND: packages/api-client/src/index.ts
- FOUND: 7c86ba9
- FOUND: c9b942b
