---
phase: 10-orgs-permissions
plan: "13"
subsystem: api
tags: [orgs, org.create, rpc, octane, orgs-new, org-01, d-org-01, tracer]

requires:
  - phase: 10-orgs-permissions
    provides: 0010 schema + organizations/org_members helpers + org_types (10-02)
provides:
  - "org.create RPC (require_verified, shared slug namespace, creator Owner)"
  - "Generated @oxidean/api-client org.create"
  - "/orgs/new Octane create form navigating to /{slug}"
affects:
  - 10-05 members RPCs
  - 10-10 org overview UI
  - 10-11 /new owner picker

actuals:
  tokens: 8443
  tasks: 1
  commits: 7

plan_head_before: ce18402f00a49b78c5ec9b5dce68d0ee7cfd7f4e

tech-stack:
  added: []
  patterns:
    - "org.* RPC module mirrors repo/pat: require_verified → validate → DbPool helpers → OrgPublic"
    - "/orgs/new mirrors /new verify wall + method=post SPA form + onInput"

key-files:
  created:
    - crates/oxidean-api/src/org/mod.rs
    - apps/web/src/routes/orgs.new.tsrx
  modified:
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts
    - crates/oxidean-api/tests/org_create_members.rs
    - apps/web/src/routes/orgs.new.integration.test.ts
    - apps/web/src/routeTree.gen.ts

key-decisions:
  - "org.slug_taken for dual user/org collisions; auth.reserved_username reused for reserved list copy"
  - "Optional display_name defaults to slug when blank"
  - "No org.get / overview stub — navigate to /{slug}; full overview deferred to plan 10"
  - "Tracer gate: HUMAN_VERIFY_MODE=end-of-phase + automated verify only — no blocking human-verify"

patterns-established:
  - "Shared slug checks: validate_username + is_reserved_username + find_user_by_username + find_organization_by_slug"
  - "Creator Owner via insert_org_owner_membership after insert_organization"

requirements-completed: [ORG-01]

coverage:
  - id: D1
    description: "org.create reserves shared slug namespace and returns OrgPublic"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(org_create)'#org_create_reserves_shared_slug_namespace"
        status: pass
    human_judgment: false
  - id: D2
    description: "org.create adds creator as Owner membership"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(org_create)'#org_create_creator_is_owner"
        status: pass
    human_judgment: false
  - id: D3
    description: "/orgs/new create form + verify wall + reserved/taken errors"
    requirement: ORG-01
    verification:
      - kind: automated_ui
        ref: "apps/web/src/routes/orgs.new.integration.test.ts"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-09-14
status: complete
---

# Phase 10 Plan 13: org.create + /orgs/new Summary

**Verified org.create end-to-end: shared-namespace slug RPC, generated client, and /orgs/new create form.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-09-13T23:55:17Z
- **Completed:** 2026-09-14T00:06:48Z
- **Tasks:** 1
- **Files modified:** 9

## Accomplishments

- `org.create` requires verified session, validates slug like usernames, rejects reserved/taken (user or org), inserts org with `member_base_permission=none`, and adds creator as Owner
- `make rpc-gen` emits `org.create` / `OrgPublic` / `CreateOrgRequest`; `rpc-sync-check` green
- `/orgs/new` Octane form (slug + optional display name) → navigate `/{slug}`; verify-email wall matches `/new`

## Task Commits

1. **Task 1: End-to-end org.create — RPC + /orgs/new** - `5d7a6d3` (feat)

**Plan metadata:** `ebe0571` / `96a4141` / `f5f951a` (docs)

## Tracer feedback gate

- **Mode:** `HUMAN_VERIFY_MODE=end-of-phase`, automated-only `<verify>`
- **Action:** Re-ran full verify after task commit — passed
- **Result:** `⚡ Tracer verified end-to-end` — no blocking human-verify invented; no expansion tasks in this plan

## Files Created/Modified

- `crates/oxidean-api/src/org/mod.rs` — `org.create` handler
- `crates/oxidean-api/src/lib.rs` / `rpc.rs` — module + dispatch
- `crates/oxidean-api/src/bin/rpc_gen.rs` + `packages/api-client/src/index.ts` — generated client
- `crates/oxidean-api/tests/org_create_members.rs` — greened `org_create_*` tests (members stubs remain RED)
- `apps/web/src/routes/orgs.new.tsrx` — create UI
- `apps/web/src/routes/orgs.new.integration.test.ts` — greened UI tests
- `apps/web/src/routeTree.gen.ts` — `/orgs/new` route registration

## Decisions Made

- Error codes: `org.slug_taken` for collisions; reuse `auth.reserved_username` / `auth.invalid_username` for reserved/invalid slugs so UI can share signup reserved copy
- Blank optional display name → slug
- Skipped org overview stub (plan allows navigation-only; overview is plan 10)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- Relative Write/StrReplace initially landed under the main checkout; reverted main and re-applied under `oxidean-wt-10` (worktree absolute paths)

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Tracer create path is green for wave expansion (members, invites, ACL, overview)
- Remaining Wave 0 stubs in `org_create_members.rs` (members.*) stay RED for later plans
- Shared-namespace dual-check: `auth.signup` / username rename now dual-check `organizations.slug` via `login_slug_taken` (same as `org.create`); see deferred-items.md `status: resolved`

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/src/org/mod.rs`
- FOUND: `apps/web/src/routes/orgs.new.tsrx`
- FOUND: commit `5d7a6d3`

---
*Phase: 10-orgs-permissions*
*Completed: 2026-09-14*
