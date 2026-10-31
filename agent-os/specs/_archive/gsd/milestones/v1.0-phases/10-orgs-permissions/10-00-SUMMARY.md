---
phase: 10-orgs-permissions
plan: "00"
subsystem: testing
tags: [wave0, nyquist, orgs, permissions, acl, nextest, dialect, org-01, org-02, org-03, org-04]

requires:
  - phase: 08-git-https-pats
    provides: Wave 0 assert!(false) nextest stub pattern + repo_private_404 / git_smart_http harness files to extend
provides:
  - "Wave 0 RED org_* create/members/invite stubs (ORG-01/02)"
  - "Wave 0 RED collab_* collaborator ACL matrix stubs (ORG-03/04)"
  - "Wave 0 dialect_orgs stub for 0010_orgs_acl tri-dialect"
  - "Wave 0 coalesce_* unit stubs + extended private/git stub names"
affects:
  - 10-02 org schema migration 0010_orgs_acl
  - 10-03 org RPC greens
  - 10-04 ACL coalesce rewrite
  - 10-08 Smart HTTP / collaborator PAT greens

actuals:
  tokens: 3703
  tasks: 2
  commits: 6

plan_head_before: f86dcf2eb936ea61cb69874a0d349d70a4341160

tech-stack:
  added: []
  patterns:
    - "Wave 0 intentional RED stubs with assert!(false) until org/ACL land"
    - "org_ / collab / dialect_orgs / coalesce nextest filters mirror 10-VALIDATION.md Wave 0"
    - "Migration number 0010_orgs_acl (Phase 09 owns 0009_ssh_keys)"

key-files:
  created:
    - crates/oxidean-api/tests/org_create_members.rs
    - crates/oxidean-api/tests/org_invites.rs
    - crates/oxidean-api/tests/repo_collaborators_acl.rs
    - crates/oxidean-db/tests/dialect_orgs.rs
  modified:
    - crates/oxidean-api/src/repo/acl.rs
    - crates/oxidean-api/tests/repo_private_404.rs
    - crates/oxidean-api/tests/git_smart_http.rs

key-decisions:
  - "Wave 0 is RED-only — no production org RPC, ACL rewrite, or 0010 migrations"
  - "Use 0010_orgs_acl (not 0009) because Phase 09 SSH claims 0009_ssh_keys"
  - "No teams (D-ORG-07); collaborator is per-repo grant only"
  - "Threat stubs encode T-10-01 (web soft not_found vs git 401) and T-10-02 (Member base none + Collaborator raise)"

patterns-established:
  - "Nyquist Wave 0 for Phase 10: failing nextest paths exist before org/ACL implementation waves"

requirements-completed: []  # Wave 0 scaffolds only; greens land in later 10-xx plans

coverage:
  - id: D1
    description: "API Wave 0 stubs for org.create, members, member_base private effects"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(org_)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "API Wave 0 stubs for org invites create/list/revoke/accept + hash-at-rest"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(org_)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "API Wave 0 stubs for collaborator CRUD + soft not_found web path"
    requirement: ORG-03
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(collab)'"
        status: pass
    human_judgment: false
  - id: D4
    description: "dialect_orgs expects tri-dialect 0010_orgs_acl + owner_type"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-db -E 'test(dialect_orgs)'"
        status: pass
    human_judgment: false
  - id: D5
    description: "ACL coalesce unit stubs + extended repo_private / git_smart stub names"
    requirement: ORG-04
    verification:
      - kind: unit
        ref: "cargo nextest list -p oxidean-api -E 'test(coalesce) | test(repo_private) | test(git_smart)'"
        status: pass
    human_judgment: false

duration: 4min
completed: 2026-09-13
status: complete
---

# Phase 10 Plan 00: Wave 0 Orgs & Permissions Nyquist Stubs Summary

**Failing nextest stubs for org_*/collab/dialect_orgs/coalesce plus extended private/git names before Phase 10 implementation waves**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-13T23:28:49Z
- **Completed:** 2026-09-13T23:32:49Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments

- Org create/members Wave 0 stubs cover shared slug, creator Owner, members.add/updateRole, last Owner, member_base none/read/write (ORG-01/02)
- Invite stubs cover create/list/revoke/accept under closed signup + token hash-at-rest (ORG-01 / D-ORG-03)
- Collaborator stubs cover personal+org CRUD, permission ladder, soft not_found, Collaborator raise (ORG-03/04 / T-10-01/02)
- `dialect_orgs` expects tri-dialect `0010_orgs_acl` with organizations, members, invites, collaborators, owner_type
- ACL `coalesce_*` unit stubs + `repo_private_404` / `git_smart_http` named stubs for org Member and collaborator paths

## Task Commits

Each task was committed atomically:

1. **Task 1: Org / invite / collaborator / dialect Wave 0 stubs** - `8d7315a` (test)
2. **Task 2: ACL coalesce unit stubs + extend private/git stub names** - `86f2bfa` (test)

**Plan metadata:** `4ad4991` (docs: complete plan)

_Note: Wave 0 is RED-only by design — GREEN belongs to later 10-xx plans._

## Files Created/Modified

- `crates/oxidean-api/tests/org_create_members.rs` — ORG-01/02 create + members + member_base stubs
- `crates/oxidean-api/tests/org_invites.rs` — ORG-01 email invite + closed-signup stubs
- `crates/oxidean-api/tests/repo_collaborators_acl.rs` — ORG-03/04 collaborator matrix stubs
- `crates/oxidean-db/tests/dialect_orgs.rs` — 0010_orgs_acl migration parity stub
- `crates/oxidean-api/src/repo/acl.rs` — coalesce_* #[cfg(test)] Wave 0 stubs (production ACL unchanged)
- `crates/oxidean-api/tests/repo_private_404.rs` — org non-member + collaborator read stubs
- `crates/oxidean-api/tests/git_smart_http.rs` — collaborator PAT push + non-grantee 401 stubs

## Decisions Made

- Kept Wave 0 RED-only (`assert!(false)`); no production org RPC, ACL rewrite, or migrations
- Locked migration number to **0010_orgs_acl** (Phase 09 owns 0009_ssh_keys)
- Named dialect tests `dialect_orgs_*` so `test(dialect_orgs)` nextest filter discovers them
- Threat stubs encode T-10-01 (web `repo.not_found` vs git 401) and T-10-02 (Member base none + Collaborator raise)

## Deviations from Plan

None - plan executed exactly as written.

## Auth Gates

None.

## Known Stubs

Intentional Wave 0 RED stubs (`assert!(false)`) in all new/extended test paths — later 10-xx plans turn them green. Not product stubs that block the plan goal (Nyquist discoverability).

## Self-Check: PASSED

- FOUND: crates/oxidean-api/tests/org_create_members.rs
- FOUND: crates/oxidean-api/tests/org_invites.rs
- FOUND: crates/oxidean-api/tests/repo_collaborators_acl.rs
- FOUND: crates/oxidean-db/tests/dialect_orgs.rs
- FOUND: coalesce stubs in crates/oxidean-api/src/repo/acl.rs
- FOUND: 8d7315a, 86f2bfa in git log
