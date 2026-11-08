---
phase: 10-orgs-permissions
plan: "07"
subsystem: api
tags: [orgs, collaborators, acl, rpc, capability]

requires:
  - phase: 10-orgs-permissions/03
    provides: Capability ACL coalesce + effective_capability including collaborator grants
  - phase: 10-orgs-permissions/04
    provides: soft not_found private deny paths
  - phase: 10-orgs-permissions/06
    provides: org membership + invites for org-owned repo fixtures
provides:
  - repo.collaborators.list/add/update/remove Admin-gated RPC
  - Visibility + soft-delete gated on meets(Admin) capability
  - Collaborator raise proven on private org repos (ORG-03/04)
affects:
  - 10-08 PAT ∩ ACL
  - 10-11 collaborators UI

actuals:
  tokens: 15512
  tasks: 2
  commits: 7

plan_head_before: fe1b681727d7e60a887c7ccbb4020d87fc553714

tech-stack:
  added: []
  patterns:
    - "resolve_repo_for_admin via meets(Admin) for collaborators/visibility/soft-delete"
    - "Collaborator is per-repo grant only — never an org membership role"

key-files:
  created:
    - crates/oxidean-api/src/repo/collaborators.rs
    - .planning/phases/10-orgs-permissions/.tdd/10-07-t1-red-evidence.json
    - .planning/phases/10-orgs-permissions/.tdd/10-07-t2-red-evidence.json
  modified:
    - crates/oxidean-db/src/repo_collaborators.rs
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-core/src/repo_types.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - crates/oxidean-api/tests/repo_collaborators_acl.rs
    - crates/oxidean-api/tests/repo_private_404.rs
    - packages/api-client/src/index.ts

key-decisions:
  - "Admin gate uses Capability ACL (meets Admin), not personal owner_id equality — required for org-owned repos"
  - "Non-admin collaborator list/mutate → soft repo.not_found (T-10-01)"
  - "Branch CRUD still uses owner_id mutate helper; Admin capability migration deferred"

patterns-established:
  - "repo.collaborators.* mirrors org.members.* CRUD shape with Admin capability gate"
  - "Duplicate collaborator → repo.collaborator_exists; invalid permission → serde rpc.bad_input"

requirements-completed: [ORG-03, ORG-04]

coverage:
  - id: D1
    description: "Repo admin can add/update/remove collaborators with read|write|admin on personal and org-owned repos"
    requirement: ORG-03
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'binary(repo_collaborators_acl)'#collab_crud_on_personal_repo"
        status: pass
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'binary(repo_collaborators_acl)'#collab_crud_on_org_repo"
        status: pass
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'binary(repo_collaborators_acl)'#collab_permission_read_write_admin"
        status: pass
    human_judgment: false
  - id: D2
    description: "Collaborator grant raises private read; stranger still soft not_found"
    requirement: ORG-04
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(collab_raises)'"
        status: pass
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(repo_private_404_collaborator)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Visibility (and soft-delete) require Admin capability, not owner_id equality"
    requirement: ORG-03
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(collab_visibility)'"
        status: pass
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(repo_settings)'"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-09-14
status: complete
---

# Phase 10 Plan 07: Collaborator CRUD Summary

**Per-repo collaborator ladder (`read|write|admin`) on personal and org-owned repos, with Admin-gated visibility.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-09-14T01:04:08Z
- **Completed:** 2026-09-14T01:15:41Z
- **Tasks:** 2
- **Files modified:** 12

## Accomplishments

- Shipped `repo.collaborators.list/add/update/remove` gated by `meets(Admin)`
- Proved collaborator grants raise private org-repo read (including Member + `member_base=none`)
- Replaced visibility/soft-delete `owner_id` equality with Admin capability (org Owner + collab admin)

## Task Commits

1. **Task 1 RED:** `26a1d62` — failing collaborator CRUD ACL tests
2. **Task 1 GREEN:** `c2a703d` — implement `repo.collaborators.*` + rpc-gen
3. **Task 2 RED:** `b2f2f39` — failing visibility Admin-capability tests
4. **Task 2 GREEN:** `044e733` — gate visibility/soft-delete on Admin capability

## Files Created/Modified

- `crates/oxidean-api/src/repo/collaborators.rs` — collaborator RPC + `resolve_repo_for_admin`
- `crates/oxidean-db/src/repo_collaborators.rs` — list/insert/update/remove
- `crates/oxidean-core/src/repo_types.rs` — collaborator DTOs
- `packages/api-client/src/index.ts` — generated client methods
- `crates/oxidean-api/tests/repo_collaborators_acl.rs` — CRUD + raise + visibility matrix
- `crates/oxidean-api/tests/repo_private_404.rs` — granted private read

## Decisions Made

- Admin operations use Capability ACL so org-owned repos work (personal `owner_id` equality cannot)
- Collaborator remains a per-repo grant only (D-ORG-04) — not an organization role
- Branch mutations still use the legacy owner_id helper (Write capability migration deferred)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Test helper assumed HTTP 200 for soft denies**
- **Found during:** Task 1 GREEN
- **Issue:** `repo.not_found` maps to HTTP 404; invalid domain errors map to 400
- **Fix:** `rpc_json` accepts OK/NOT_FOUND and known 400 domain codes
- **Files modified:** `crates/oxidean-api/tests/repo_collaborators_acl.rs`
- **Commit:** `c2a703d`

**2. [Rule 3 - Blocking] Username underscores rejected by validate_username**
- **Found during:** Task 1 GREEN (`collab_permission_read_write_admin`)
- **Issue:** `u_read` etc. fail signup (alphanumeric/hyphen only)
- **Fix:** Use `uread1` / `uwrite1` / `uadmin1`
- **Files modified:** `crates/oxidean-api/tests/repo_collaborators_acl.rs`
- **Commit:** `c2a703d`

### Deferred Issues

- Plan verify filter `test(collab)` also matches Wave-0 stub `git_smart_collaborator_classic_pat_push` (PAT ∩ ACL later). Intent tests verified with `test(collab_) | test(repo_private) | test(repo_settings)` (14 passed). Logged in `deferred-items.md` + WINDOWS.md.

## TDD Gate Compliance

| Task | RED commit | GREEN commit | RED evidence |
|------|------------|--------------|--------------|
| T1 | `26a1d62` | `c2a703d` | `.tdd/10-07-t1-red-evidence.json` → `RED_EVIDENCE_OK` |
| T2 | `b2f2f39` | `044e733` | `.tdd/10-07-t2-red-evidence.json` → `RED_EVIDENCE_OK` |

## Known Stubs

None in this plan's deliverables. Pre-existing `git_smart_collaborator_classic_pat_push` Wave-0 stub remains (out of scope).

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/src/repo/collaborators.rs`
- FOUND: `10-07-SUMMARY.md` (this file)
- FOUND commits: `26a1d62`, `c2a703d`, `b2f2f39`, `044e733`
