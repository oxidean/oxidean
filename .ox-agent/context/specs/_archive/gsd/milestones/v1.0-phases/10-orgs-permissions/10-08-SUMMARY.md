---
phase: 10-orgs-permissions
plan: "08"
subsystem: api
tags: [orgs, acl, smart-http, pat, capability, git]

requires:
  - phase: 10-orgs-permissions/04
    provides: effective_capability + meets() central ACL
  - phase: 10-orgs-permissions/07
    provides: collaborators CRUD + Admin-gated visibility/soft-delete
provides:
  - Smart HTTP fetch/push gated on meets(Read|Write) with private 401 (D-21)
  - Branch mutate gated on meets(Write)
  - PAT authorize ∩ ACL; classic push without owner_id equality
  - FG Selected mint via ACL; FG All = personal + org Owner/Admin (A4)
affects:
  - 10-09 UI permission surfaces
  - Phase 09 SSH call sites (when present)

actuals:
  tokens: 10292
  tasks: 2
  commits: 6

plan_head_before: 617a695d50e532a54816c01e895e66477c2c4808

tech-stack:
  added: []
  patterns:
    - "Smart HTTP: effective_capability then PAT scope/contents (intersect)"
    - "FG All authorize via fg_all_covers_repo (A4), not owner_id equality"
    - "FG Selected mint requires meets(contents need)"

key-files:
  created:
    - .planning/phases/10-orgs-permissions/.tdd/10-08-t1-red-evidence.json
    - .planning/phases/10-orgs-permissions/.tdd/10-08-t2-red-evidence.json
  modified:
    - crates/oxidean-api/src/repo/acl.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/routes/git_smart_http.rs
    - crates/oxidean-api/src/pat/mod.rs
    - crates/oxidean-api/tests/git_smart_http.rs
    - crates/oxidean-api/tests/repo_branch_soft_protect.rs
    - crates/oxidean-api/tests/pat_rpc.rs

key-decisions:
  - "ACL deny on Smart HTTP stays 401 Basic (D-21); PAT scope deny stays 403 (D-23)"
  - "FG All = personal-owned + org Owner/Admin only (A4); collaborators use Selected"
  - "Classic PAT push authorized by repo scope ∩ meets(Write), not pat.user_id == owner_id"

patterns-established:
  - "owner_ref_for_repo + fg_all_covers_repo shared helpers for PAT mint/authorize"
  - "resolve_repo_for_owner_mutate now Write capability (branch CRUD)"

requirements-completed: [ORG-04, ORG-03]

coverage:
  - id: D1
    description: "Smart HTTP private anon/non-grantee → 401; Write collaborator classic PAT can receive-pack; Read cannot"
    requirement: ORG-04
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(git_smart)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Branch create gated on Write capability for collaborators"
    requirement: ORG-04
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api --test repo_branch_soft_protect repo_branch_write_collaborator_can_create_read_cannot"
        status: pass
    human_judgment: false
  - id: D3
    description: "PAT ∩ ACL: FG Selected collaborator mint/push; FG All org Owner receive-pack (A4)"
    requirement: ORG-04
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(pat_) | test(git_smart)'"
        status: pass
    human_judgment: false

duration: 8min
completed: 2026-09-14
status: complete
---

# Phase 10 Plan 08: Non-UI ACL Consumers Summary

**Smart HTTP, branch mutate, and PAT mint/authorize now intersect Capability ACL so collaborators can push with scope and strangers still get private 401.**

## Performance

- **Duration:** 8 min
- **Started:** 2026-09-14T01:18:05Z
- **Completed:** 2026-09-14T01:26:05Z
- **Tasks:** 2
- **Files modified:** 7 (+ 2 RED evidence)

## Accomplishments

- Branch CRUD uses `meets(Write)` instead of `owner_id` equality (T-10-14)
- Smart HTTP private/push uses `effective_capability` + 401/403 mapping (D-21 / D-23)
- Classic PAT push no longer requires `pat.user_id == repositories.owner_id` (T-10-13)
- FG Selected mint checks ACL for contents need; FG All authorize uses A4 (personal + org Owner/Admin)

## Task Commits

Each task was committed atomically (TDD RED → GREEN):

1. **Task 1 RED:** `ecefae1` — failing Smart HTTP ACL + Write branch tests
2. **Task 1 GREEN:** `68b0fe0` — wire Smart HTTP + branch mutate to Capability ACL
3. **Task 2 RED:** `d561047` — failing PAT ∩ ACL FG Selected/All tests
4. **Task 2 GREEN:** `b3c6c3e` — intersect PAT authorize/mint with Capability ACL

## TDD Gate Compliance

| Task | RED commit | GREEN commit | RED evidence |
|------|------------|--------------|--------------|
| T1 | `ecefae1` | `68b0fe0` | `.tdd/10-08-t1-red-evidence.json` → `RED_EVIDENCE_OK` |
| T2 | `d561047` | `b3c6c3e` | `.tdd/10-08-t2-red-evidence.json` → `RED_EVIDENCE_OK` |

## Files Created/Modified

- `crates/oxidean-api/src/repo/acl.rs` — `owner_ref_for_repo`, `fg_all_covers_repo` (A4)
- `crates/oxidean-api/src/repo/mod.rs` — Write-gated `resolve_repo_for_owner_mutate`
- `crates/oxidean-api/src/routes/git_smart_http.rs` — ACL + async `pat_allows_operation`
- `crates/oxidean-api/src/pat/mod.rs` — FG Selected mint via `meets(contents need)`
- `crates/oxidean-api/tests/git_smart_http.rs` — collaborator / FG All / non-grantee cases
- `crates/oxidean-api/tests/repo_branch_soft_protect.rs` — Write collaborator branchCreate
- `crates/oxidean-api/tests/pat_rpc.rs` — Selected collaborator mint + Read/Write mismatch

## Decisions Made

- Keep Smart HTTP ACL denials as **401** Basic (D-21); reserve **403** for insufficient PAT scope/contents (D-23)
- Document FG All semantics (A4) in `fg_all_covers_repo` / mint docstring; collaborators must use Selected
- No Phase 09 SSH server code in-tree — ACL helpers ready; SSH call-site deferred

## Deviations from Plan

### Auto-fixed Issues

None - plan executed as written.

**Note:** Task 1 GREEN removed classic `pat.user_id == owner_id` push check so collaborator receive-pack could go green under Task 1 verify; Task 2 completed FG All/Selected mint∩authorize (T-10-13 full surface). Not a Rule 1–3 fix — sequencing overlap with plan verify filter.

## Auth Gates

None.

## Known Stubs

None — Wave 0 `assert!(false)` stubs replaced with real integration tests.

## Threat Flags

None beyond plan `<threat_model>` (T-10-13, T-10-01, T-10-14 mitigated).

## Self-Check: PASSED

- FOUND SUMMARY + key source files
- FOUND commits: ecefae1, 68b0fe0, d561047, b3c6c3e
