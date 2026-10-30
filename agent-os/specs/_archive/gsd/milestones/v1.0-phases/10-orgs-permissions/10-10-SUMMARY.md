---
phase: 10-orgs-permissions
plan: "10"
subsystem: ui
tags: [orgs, members, invites, member-base, octane, lookup, autocomplete]

requires:
  - phase: 10-orgs-permissions
    provides: org.get/listMine/members/invites RPCs + user.lookup (10-05/09/13)
provides:
  - "Org overview at /{org} with member count + ACL-filtered repos"
  - "/{org}/settings member_base None|Read|Write (Admin+)"
  - "Members/invites UI with live username lookup (no emails)"
affects: [10-11 collaborators UI, 10-12 invite accept polish]

actuals:
  tokens: 17481
  tasks: 2
  commits: 2

plan_head_before: 0f5af7895b22313d9f046741409275a11425f779

tech-stack:
  added:
    - "repo.listByOwner RPC for org/user overview repo lists"
  patterns:
    - "SSR org loaders in ssr-org.ts (Cookie-forward)"
    - "MemberLookup combobox via user.lookup + Query enabled≥2"
    - "Admin+ gate via org.listMine role (Owner|Admin)"

key-files:
  created:
    - apps/web/src/routes/$owner.tsrx
    - apps/web/src/routes/$owner.settings.tsrx
    - apps/web/src/routes/$owner.settings.members.tsrx
    - apps/web/src/components/org/member-lookup.tsrx
    - apps/web/src/components/org/org-settings-nav.tsrx
    - apps/web/src/lib/ssr-org.ts
  modified:
    - apps/web/src/components/chrome.tsrx
    - apps/web/src/routes/$owner.settings.members.integration.test.ts
    - crates/oxidean-core/src/repo_types.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts

key-decisions:
  - "Added repo.listByOwner so overview can list ACL-filtered repos (no listMine coverage for org-owned repos)"
  - "Admin+ settings/members visibility from org.listMine role, not OrgPublic.can_admin"
  - "MemberLookup never renders email fields from UserLookupHit"

patterns-established:
  - "Org settings nav (General | Members) shared component"
  - "Rivet @if bodies must be a single root element (wrap sibling sections)"

requirements-completed: [ORG-01, ORG-02]

coverage:
  - id: D1
    description: "Org overview lists repos + member count; Settings link for Admin+"
    requirement: ORG-01
    verification:
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
    human_judgment: false
  - id: D2
    description: "member_base_permission Select None|Read|Write on org settings"
    requirement: ORG-02
    verification:
      - kind: integration
        ref: "apps/web/src/routes/$owner.settings.members.integration.test.ts#member_base_permission Select None | Read | Write"
        status: pass
    human_judgment: false
  - id: D3
    description: "Members page Add member + live lookup + invites Revoke/Keep AlertDialog"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "apps/web/src/routes/$owner.settings.members.integration.test.ts"
        status: pass
    human_judgment: false

duration: 10min
completed: 2026-09-14
status: complete
---

# Phase 10 Plan 10: Org Overview & Members UI Summary

**Org overview, member_base settings, and members/invites management UI with live username lookup — Admins can manage membership in the browser.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-09-14T01:35:29Z
- **Completed:** 2026-09-14T01:45:00Z
- **Tasks:** 2
- **Files modified:** 14

## Accomplishments

- Shipped `/{org}` overview (display name, slug, member count, ACL-filtered repos) and Admin+ Settings/Members links
- Shipped `/{org}/settings` with display name + `member_base_permission` None|Read|Write and helper copy (D-ORG-02b)
- Shipped members table, Add member via `user.lookup` autocomplete, and email invites with Revoke invite / Keep invite AlertDialog

## Task Commits

1. **Task 1: Org overview + settings member_base** - `19fe42d` (feat)
2. **Task 2: Members + invites UI with live lookup** - `1593875` (feat)

## Files Created/Modified

- `apps/web/src/routes/$owner.tsrx` — org overview route
- `apps/web/src/routes/$owner.settings.tsrx` — general settings + member_base
- `apps/web/src/routes/$owner.settings.members.tsrx` — members + invites
- `apps/web/src/components/org/member-lookup.tsrx` — live username autocomplete
- `apps/web/src/components/org/org-settings-nav.tsrx` — General | Members nav
- `apps/web/src/lib/ssr-org.ts` — Cookie-forward org SSR helpers
- `apps/web/src/components/chrome.tsrx` — New organization account menu link
- `crates/oxidean-api` + `packages/api-client` — `repo.listByOwner` for overview lists

## Decisions Made

- Introduced `repo.listByOwner` so org overview can list public (and member-accessible private) repos; `repo.listMine` only covers personal owner_id
- Gate Settings/Members on `org.listMine` Owner|Admin role rather than extending OrgPublic

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] Added repo.listByOwner**
- **Found during:** Task 1
- **Issue:** No RPC listed org-owned repos; overview could not satisfy public-repos must-have
- **Fix:** Thin ACL-filtered `repo.listByOwner` + `make rpc-gen`
- **Files modified:** `crates/oxidean-core/src/repo_types.rs`, `crates/oxidean-api/src/repo/mod.rs`, `rpc.rs`, `rpc_gen.rs`, `packages/api-client`
- **Commit:** `19fe42d`

**2. [Rule 1 - Bug] Rivet adjacent JSX in members Admin block**
- **Found during:** Task 2 (build)
- **Issue:** `@if (canAdmin)` contained two sibling `<section>` roots → Octane parse error
- **Fix:** Wrap sections in a single parent `<div>`
- **Files modified:** `$owner.settings.members.tsrx`
- **Commit:** `1593875`

**3. [Rule 3 - Blocking] Vitest cold-compile timeout**
- **Found during:** Task 2 verify
- **Issue:** Default 5s timeout insufficient for first dynamic import of large `.tsrx`
- **Fix:** Raise per-test timeout to 30s in members integration suite
- **Files modified:** `$owner.settings.members.integration.test.ts`
- **Commit:** `1593875`

## Auth Gates

None.

## Known Stubs

None — members/settings/overview routes are wired to live RPCs.

## Threat Flags

None beyond plan threat model (T-10-03 lookup no-email; T-10-09 Admin+ UI hide + server enforce).

## Self-Check: PASSED

- FOUND: `apps/web/src/routes/$owner.tsrx`
- FOUND: `apps/web/src/routes/$owner.settings.tsrx`
- FOUND: `apps/web/src/routes/$owner.settings.members.tsrx`
- FOUND: `apps/web/src/components/org/member-lookup.tsrx`
- FOUND: `19fe42d`, `1593875`
