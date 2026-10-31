---
phase: 10-orgs-permissions
plan: "06"
subsystem: api
tags: [orgs, invites, email, closed-signup, rpc, octane]

requires:
  - phase: 10-orgs-permissions/05
    provides: org.members CRUD + Admin+ gates + username add under closed signup
provides:
  - org.invites.create/list/revoke/accept RPC with hash-at-rest tokens
  - Closed-signup-safe invite account provisioning (A2)
  - /invites/$token Octane accept page
affects:
  - 10-10 members/invites UI
  - 10-12 factory reset + docs

actuals:
  tokens: 19647
  tasks: 2
  commits: 7

tech-stack:
  added: []
  patterns:
    - "Invite tokens: 32-byte CSPRNG hex, SHA-256 at rest, 7-day TTL, single-use"
    - "Accept bypasses allow_signup only with valid unused invite (A2)"
    - "EmailSender OutboundEmail magic link via public_origin()"

key-files:
  created:
    - crates/oxidean-db/src/org_invites.rs
    - crates/oxidean-api/src/org/invites.rs
    - apps/web/src/routes/invites.$token.tsrx
    - .planning/phases/10-orgs-permissions/.tdd/10-06-red-evidence.json
  modified:
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-core/src/org_types.rs
    - crates/oxidean-api/src/org/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - crates/oxidean-api/tests/org_invites.rs
    - packages/api-client/src/index.ts
    - apps/web/src/routeTree.gen.ts

key-decisions:
  - "Invite accept creates verified local users without checking allow_signup when redeeming a valid token"
  - "Create/list omit plaintext token; magic only in outbound email link"
  - "Existing invite-email account must sign in (org.invite_login_required) rather than password-steal on accept"

patterns-established:
  - "org.invites.* mirrors members Admin+ gates + Owner-only Owner role grants"
  - "Invite rate limit: 5/hour per inviter + 60s reissue interval per org+email"

requirements-completed: [ORG-01]

coverage:
  - id: D1
    description: "Admin+ create/list/revoke email invites with hash-at-rest tokens"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(org_invite)'#org_invites_create"
        status: pass
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(org_invite)'#org_invites_list_omits_plaintext_token"
        status: pass
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(org_invite)'#org_invites_revoke"
        status: pass
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(org_invite)'#org_invites_token_hash_at_rest"
        status: pass
    human_judgment: false
  - id: D2
    description: "Accept provisions membership and may create account when allow_signup is false"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(org_invite)'#org_invites_accept_closed_signup_creates_or_links_account"
        status: pass
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(org_invite)'#org_invites_accept_expired_token_fails"
        status: pass
    human_judgment: false
  - id: D3
    description: "/invites/$token accept page builds and is routed"
    requirement: ORG-01
    verification:
      - kind: other
        ref: "bun run build (apps/web) + routeTree /invites/$token"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-09-14
status: complete
plan_head_before: cc7f121d46378793a7b9b01b2dcd7a3a9f1a1a7b
commits: 7
---

# Phase 10 Plan 06: Email Invites Summary

**Email invite create/list/revoke/accept with SHA-256 hash-at-rest tokens and closed-signup-safe account provisioning (ORG-01 / D-ORG-03 / A2).**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-09-14T00:50:13Z
- **Completed:** 2026-09-14T01:02:00Z
- **Tasks:** 2
- **Files modified:** 12

## Accomplishments

- Shipped `org.invites.*` RPC with Admin+ gates, EmailSender magic links, and 7-day single-use tokens
- Accept creates verified local users when `allow_signup=false` without opening public signup
- `/invites/$token` Octane page for logged-in accept or invite-gated username/password join

## Task Commits

1. **Task 1 RED: failing org.invites lifecycle tests** - `69b4947` (test)
2. **Task 1 GREEN: implement org.invites create/list/revoke/accept** - `4cc919a` (feat)
3. **Task 2: /invites/$token accept page** - `1c59d24` (feat)

**Plan metadata:** `863e887` (docs: complete plan)

## Files Created/Modified

- `crates/oxidean-db/src/org_invites.rs` — tri-dialect invite CRUD / revoke / accept / rate helpers
- `crates/oxidean-api/src/org/invites.rs` — RPC handlers + email template + closed-signup provision
- `crates/oxidean-api/tests/org_invites.rs` — integration coverage including closed signup + expiry
- `apps/web/src/routes/invites.$token.tsrx` — accept UI
- `packages/api-client/src/index.ts` — generated client (`make rpc-gen`)

## Decisions Made

- Valid invite redeem creates email-verified accounts without consulting `allow_signup`
- Token plaintext only in outbound mail (`/invites/{magic}`); list/create DTOs never include token/hash
- Existing account for invite email requires login (`org.invite_login_required`)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] RecordingSender counted signup welcome/verify mail**
- **Found during:** Task 1 GREEN
- **Issue:** Tests asserted `sent.len() == 1` but signup also sends welcome + verify
- **Fix:** Select invite message by `/invites/` body marker
- **Files modified:** `crates/oxidean-api/tests/org_invites.rs`
- **Commit:** `4cc919a`

**2. [Rule 3 - Blocking] Absolute Write briefly hit main repo**
- **Found during:** Task 1 RED
- **Issue:** Relative Write resolved to main checkout instead of worktree
- **Fix:** Restored Wave 0 stub on main; rewrote tests with absolute wt-10 path
- **Files modified:** worktree `org_invites.rs` only
- **Commit:** `69b4947`

## TDD Gate Compliance

| Task | RED | GREEN | REFACTOR | Evidence |
|------|-----|-------|----------|----------|
| T1 invites RPC | ✓ `69b4947` | ✓ `4cc919a` | — | `.tdd/10-06-red-evidence.json` RED_EVIDENCE_OK |

## Threat Flags

None — surfaces match plan threat model (T-10-11, T-10-12, T-10-03, T-10-SC).

## Known Stubs

None.

## Self-Check: PASSED

- FOUND: crates/oxidean-db/src/org_invites.rs
- FOUND: crates/oxidean-api/src/org/invites.rs
- FOUND: apps/web/src/routes/invites.$token.tsrx
- FOUND: 69b4947, 4cc919a, 1c59d24
