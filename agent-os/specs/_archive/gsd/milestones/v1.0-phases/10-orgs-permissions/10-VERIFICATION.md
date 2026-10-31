---
phase: 10-orgs-permissions
verified: "2026-09-19T18:14:28Z"
status: passed
status_note: Automated fingerprint refresh — existing test/VALIDATION evidence accepted as proof (no conversational UAT).
score: 3/3 must-haves verified
covered_files:
  - .planning/REQUIREMENTS.md
  - .planning/ROADMAP.md
  - .planning/phases/10-orgs-permissions/10-00-PLAN.md
  - .planning/phases/10-orgs-permissions/10-00-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-01-PLAN.md
  - .planning/phases/10-orgs-permissions/10-01-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-02-PLAN.md
  - .planning/phases/10-orgs-permissions/10-02-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-03-PLAN.md
  - .planning/phases/10-orgs-permissions/10-03-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-04-PLAN.md
  - .planning/phases/10-orgs-permissions/10-04-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-05-PLAN.md
  - .planning/phases/10-orgs-permissions/10-05-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-06-PLAN.md
  - .planning/phases/10-orgs-permissions/10-06-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-07-PLAN.md
  - .planning/phases/10-orgs-permissions/10-07-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-08-PLAN.md
  - .planning/phases/10-orgs-permissions/10-08-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-09-PLAN.md
  - .planning/phases/10-orgs-permissions/10-09-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-10-PLAN.md
  - .planning/phases/10-orgs-permissions/10-10-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-11-PLAN.md
  - .planning/phases/10-orgs-permissions/10-11-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-12-PLAN.md
  - .planning/phases/10-orgs-permissions/10-12-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-13-PLAN.md
  - .planning/phases/10-orgs-permissions/10-13-SUMMARY.md
  - .planning/phases/10-orgs-permissions/10-CONTEXT.md
  - .planning/phases/10-orgs-permissions/10-UI-SPEC.md
  - .planning/phases/10-orgs-permissions/10-VALIDATION.md
  - .planning/phases/10-orgs-permissions/deferred-items.md
  - apps/web/src/components/chrome.tsrx
  - apps/web/src/components/org/member-lookup.tsrx
  - apps/web/src/components/repo/collaborators-panel.tsrx
  - apps/web/src/routes/$owner.$repo.settings.collaborators.integration.test.ts
  - apps/web/src/routes/$owner.$repo.settings.tsrx
  - apps/web/src/routes/$owner.settings.members.tsrx
  - apps/web/src/routes/$owner.settings.tsrx
  - apps/web/src/routes/$owner.tsrx
  - apps/web/src/routes/invites.$token.tsrx
  - apps/web/src/routes/new.owner-picker.integration.test.ts
  - apps/web/src/routes/new.tsrx
  - apps/web/src/routes/orgs.new.integration.test.ts
  - apps/web/src/routes/orgs.new.tsrx
  - crates/oxidean-api/src/org/invites.rs
  - crates/oxidean-api/src/org/members.rs
  - crates/oxidean-api/src/org/mod.rs
  - crates/oxidean-api/src/pat/mod.rs
  - crates/oxidean-api/src/repo/acl.rs
  - crates/oxidean-api/src/repo/collaborators.rs
  - crates/oxidean-api/src/repo/mod.rs
  - crates/oxidean-api/src/routes/git_smart_http.rs
  - crates/oxidean-api/src/rpc.rs
  - crates/oxidean-api/src/user/lookup.rs
  - crates/oxidean-api/tests/factory_reset_scope.rs
  - crates/oxidean-api/tests/git_smart_http.rs
  - crates/oxidean-api/tests/org_create_members.rs
  - crates/oxidean-api/tests/org_invites.rs
  - crates/oxidean-api/tests/pat_rpc.rs
  - crates/oxidean-api/tests/repo_branch_soft_protect.rs
  - crates/oxidean-api/tests/repo_collaborators_acl.rs
  - crates/oxidean-api/tests/repo_create.rs
  - crates/oxidean-api/tests/repo_private_404.rs
  - crates/oxidean-core/src/auth_types.rs
  - crates/oxidean-core/src/lib.rs
  - crates/oxidean-core/src/org_types.rs
  - crates/oxidean-core/src/repo_types.rs
  - crates/oxidean-db/migrations/mysql/0010_orgs_acl.sql
  - crates/oxidean-db/migrations/postgres/0010_orgs_acl.sql
  - crates/oxidean-db/migrations/sqlite/0010_orgs_acl.sql
  - crates/oxidean-db/src/lib.rs
  - crates/oxidean-db/src/org_invites.rs
  - crates/oxidean-db/src/org_members.rs
  - crates/oxidean-db/src/organizations.rs
  - crates/oxidean-db/src/repo_collaborators.rs
  - crates/oxidean-db/src/repositories.rs
  - crates/oxidean-db/src/users.rs
  - crates/oxidean-db/tests/dialect_orgs.rs
  - crates/oxidean-db/tests/factory_reset_orgs.rs
  - docs/API.md
  - docs/ARCHITECTURE.md
  - docs/CONFIGURATION.md
  - packages/api-client/src/index.ts
covered_digest: "v1:sha256:70ad10f920a8eaad218eaef4520c6f84e3ed9a4d1123fa7a87df0a96aca1fcf4"
behavior_unverified: 0
overrides_applied: 0
decision_coverage: "{'honored': 9, 'total': 9, 'not_honored': []}"
---

# Phase 10: Orgs & Permissions Verification Report

**Phase Goal:** Users can collaborate via organizations and repository permissions with private data truly private  
**Verified:** 2026-09-14T02:07:44Z  
**Status:** passed  
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

Roadmap success criteria (contract). Plan-level truths that restate these SCs are folded into the SC wording; supporting plan artifacts were spot-checked under Artifacts / Key Links.

| # | Truth | Status | Evidence |
| --- | ------- | ---------- | -------------- |
| 1 | User can create an organization and invite/add members | ✓ VERIFIED | `org.create` inserts org + Owner membership (`org/mod.rs`); UI `orgs.new.tsrx` → `apiClient.org.create` → `/{slug}`; members add/update via `org/members.rs`; invites create/accept via `org/invites.rs` + `/invites/$token`. Tests: `org_create_creator_is_owner`, `org_members_add_by_username`, `org_invites_create`, `org_invites_accept_closed_signup_creates_or_links_account` — all ok. |
| 2 | Org owner can assign member roles that control repo access; repo owner can set visibility and collaborator permissions | ✓ VERIFIED | Roles Owner/Admin/Member + `member_base_permission` coalesce in `acl.rs`; `org.members.updateRole` + `org.updateSettings`; collaborators CRUD Admin-gated; settings UI gates on `can_admin`. Tests: `org_members_update_role`, `org_member_base_none_denies_private_repo_read`, `collab_visibility_change_requires_admin`, `collab_crud_on_org_repo`, `coalesce_member_base_none_yields_none` — all ok. |
| 3 | Unauthorized users cannot read private repos or push without permission | ✓ VERIFIED | `resolve_repo_for_read` → identical `repo.not_found` for missing/unauthorized private; Smart HTTP private unauth → 401; Write required for push; PAT ∩ ACL. Tests: `repo_private_404_org_non_member_soft_not_found`, `git_smart_private_anon_401_www_authenticate`, `git_smart_private_non_grantee_401`, `git_smart_collaborator_read_cannot_push`, `git_smart_collaborator_classic_pat_push` — all ok. |

**Score:** 3/3 truths verified (0 present, behavior-unverified)

### Decision Coverage

All trackable CONTEXT.md decisions are honored by shipped artifacts (9/9). Non-blocking gate.

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | ----------- | ------ | ------- |
| `crates/oxidean-db/migrations/*/0010_orgs_acl.sql` | Tri-dialect org ACL schema | ✓ VERIFIED | postgres/mysql/sqlite present; orgs, members, invites, collaborators, `owner_type` |
| `crates/oxidean-db/src/organizations.rs` | Org insert helpers | ✓ VERIFIED | Wired via `Database` facade; used by `org.create` |
| `crates/oxidean-core/src/org_types.rs` | Org DTOs | ✓ VERIFIED | Used by API + generated client |
| `crates/oxidean-api/src/org/mod.rs` | `org.create` / settings / listMine | ✓ VERIFIED | Registered in `rpc.rs`; DB insert + Owner membership |
| `crates/oxidean-api/src/org/members.rs` | Member CRUD + roles | ✓ VERIFIED | list/add/updateRole/remove |
| `crates/oxidean-api/src/org/invites.rs` | Email invites + accept | ✓ VERIFIED | Hash-at-rest; `ctx.email.send`; closed-signup accept |
| `crates/oxidean-api/src/repo/acl.rs` | Capability coalesce + read resolve | ✓ VERIFIED | 435 lines; unit coalesce matrix; `effective_capability` uses org_members + collaborators DB helpers |
| `crates/oxidean-api/src/repo/collaborators.rs` | Collaborator CRUD | ✓ VERIFIED | Admin via `resolve_repo_for_admin` |
| `crates/oxidean-api/src/routes/git_smart_http.rs` | Git fetch/push ACL | ✓ VERIFIED | `meets(Read\|Write)` after owner resolve |
| `crates/oxidean-api/src/pat/mod.rs` | PAT ∩ ACL | ✓ VERIFIED | `effective_capability` + `meets` |
| `crates/oxidean-api/src/user/lookup.rs` | Username autocomplete | ✓ VERIFIED | ≤10; never email; rate-limited |
| `apps/web/src/routes/orgs.new.tsrx` | Create org UI | ✓ VERIFIED | Calls `org.create`; navigates to `/{slug}` |
| `apps/web/src/routes/$owner.settings.members.tsrx` | Members + invites UI | ✓ VERIFIED | Wired to `org.members.*` / `org.invites.*` + `MemberLookup` |
| `apps/web/src/routes/$owner.settings.tsrx` | member_base settings | ✓ VERIFIED | Select none/read/write → `org.updateSettings` |
| `apps/web/src/routes/new.tsrx` | Owner picker | ✓ VERIFIED | Self + Owner/Admin orgs → `repo.create` owner |
| `apps/web/src/components/repo/collaborators-panel.tsrx` | Collaborators UI | ✓ VERIFIED | Used from settings; Admin-gated |
| `apps/web/src/routes/$owner.$repo.settings.tsrx` | Visibility + can_admin | ✓ VERIFIED | `canAdmin = !!repo?.can_admin` (not `me.id === owner_id`) |
| `packages/api-client/src/index.ts` | Generated client | ✓ VERIFIED | `org.*`, collaborators, `user.lookup`; `make rpc-sync-check` ok |
| `docs/API.md` / `ARCHITECTURE.md` | Org/ACL docs | ✓ VERIFIED | Capability ACL documented; owner-only private ACL language absent |
| `.planning/.../10-VALIDATION.md` | Phase gate map | ✓ VERIFIED | `nyquist_compliant: true`; req→test map complete |

### Key Link Verification

Automated `verify.key-links` often fails on cross-crate path strings; manual wiring checked:

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| `orgs.new.tsrx` | `org/mod.rs` | `apiClient.org.create` after rpc-gen | ✓ WIRED | Client method + RPC dispatch `"org.create"` |
| `org/mod.rs` | `organizations.rs` | `insert_organization` + Owner membership | ✓ WIRED | Facade methods on `ctx.db` |
| `acl.rs` | org_members / repo_collaborators | `find_org_member_role` / `find_repo_collaborator` | ✓ WIRED | In `effective_capability` |
| `git_smart_http.rs` | `acl.rs` | `effective_capability` + `meets` | ✓ WIRED | Import + fetch/push gates |
| `pat/mod.rs` | `acl.rs` | capability intersect | ✓ WIRED | Import + authorize path |
| `invites.rs` | email | `OutboundEmail` + `ctx.email.send` | ✓ WIRED | Magic link `/invites/{token}` |
| `$owner.settings.members.tsrx` | api-client | `org.members.*` / `org.invites.*` / MemberLookup | ✓ WIRED | Direct client calls + lookup component |
| `new.tsrx` | `repo.create` owner | `apiClient.repo.create({ owner })` | ✓ WIRED | Owner/Admin orgs from `listMine` |
| `$owner.$repo.settings.tsrx` | `can_admin` | settings visibility gate | ✓ WIRED | Replaces owner_id equality |
| `lib.rs` | `factory_reset_instance` | DELETE organizations (+ cascades) | ✓ WIRED | Automated key-link verified |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| Org overview / settings | `OrgPublic` | `org.get` / DB `organizations` | Yes | ✓ FLOWING |
| Members list | `OrgMemberPublic[]` | `org.members.list` → `organization_members` | Yes | ✓ FLOWING |
| Invites list | invite rows | `org.invites.list` → `organization_invites` | Yes | ✓ FLOWING |
| Repo settings ACL flags | `can_admin` | `resolve_repo_for_read` → `effective_capability` | Yes | ✓ FLOWING |
| Collaborators panel | collaborator rows | `repo.collaborators.list` | Yes | ✓ FLOWING |
| Owner picker | `ownerOrgs` | `org.listMine` filtered Owner/Admin | Yes | ✓ FLOWING |
| Member lookup | `users[]` | `user.lookup` username prefix (no email) | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Create org → Owner | `cargo test -p oxidean-api --test org_create_members org_create_creator_is_owner -- --exact` | ok | ✓ PASS |
| Add member | `… org_members_add_by_username` | ok | ✓ PASS |
| Invite create/accept | `… org_invites_create` / `org_invites_accept_closed_signup_…` | ok | ✓ PASS |
| member_base denies read | `… org_member_base_none_denies_private_repo_read` | ok | ✓ PASS |
| Role update | `… org_members_update_role` | ok | ✓ PASS |
| Visibility Admin gate | `… collab_visibility_change_requires_admin` | ok | ✓ PASS |
| Collab CRUD org repo | `… collab_crud_on_org_repo` | ok | ✓ PASS |
| Private soft-404 | `… repo_private_404_org_non_member_soft_not_found` | ok | ✓ PASS |
| Git private 401 | `… git_smart_private_anon_401_www_authenticate` | ok | ✓ PASS |
| Push without Write | `… git_smart_collaborator_read_cannot_push` | ok | ✓ PASS |
| PAT collaborator push | `… git_smart_collaborator_classic_pat_push` | ok | ✓ PASS |
| Coalesce unit | `cargo test -p oxidean-api --lib repo::acl::coalesce_tests::coalesce_member_base_none_yields_none -- --exact` | ok | ✓ PASS |
| Factory reset orgs | `cargo test -p oxidean-db --test factory_reset_orgs -- --exact` | ok | ✓ PASS |
| Vitest org UI routes | `bun run test --` (4 integration files) | 15 passed | ✓ PASS |
| RPC client sync | `make rpc-sync-check` | ok | ✓ PASS |

### Probe Execution

| Probe | Command | Result | Status |
| ----- | ------- | ------ | ------ |
| — | — | No phase-declared `scripts/*/tests/probe-*.sh` | SKIP |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| ORG-01 | 00,01,02,03,05,06,09,10,11,12,13 | Create org; invite/add members | ✓ SATISFIED | Schema + `org.create` + members/invites RPC/UI + tests |
| ORG-02 | 00,04,05,10,12 | Roles control repo access | ✓ SATISFIED | Coalesce + member_base + role RPCs + tests |
| ORG-03 | 00,01,03,07,08,11,12 | Visibility + collaborator permissions | ✓ SATISFIED | Collaborators CRUD + visibility Admin + UI panel |
| ORG-04 | 00,04,07,08,12 | Unauthorized cannot read/push private | ✓ SATISFIED | Soft-404 web + git 401/Write gates + PAT∩ACL |

**Orphaned requirements:** None for Phase 10. ORG-05 / ORG-06 map to Phase 13 (Pending) — correctly out of scope.

All plan `requirements:` IDs are subsets of ORG-01..04 — every ID accounted for.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| `$owner.settings.members.integration.test.ts` | — | Existence-only Vitest assertions (`toBeTruthy` on export) | ⚠️ Warning | UI smoke only; ORG behaviors covered by API integration tests |
| `deferred-items.md` | — | Shared-namespace dual-check on signup/rename (resolved) | ℹ️ Info | `auth.signup` + profile/bootstrap rename use `login_slug_taken` / org-slug checks; `status: resolved` in deferred-items.md |

No unresolved `TBD`/`FIXME`/`XXX` debt markers in phase implementation files. No teams tables/RPCs. No dialect SQL in `oxidean-api`. ARCHITECTURE no longer claims owner-only private ACL.

### Test Quality Audit

| Test File | Linked Req | Active | Skipped | Circular | Assertion Level | Verdict |
|-----------|-----------|--------|---------|----------|-----------------|---------|
| `org_create_members.rs` | ORG-01/02 | 11 | 0 | 0 | Behavioral | OK |
| `org_invites.rs` | ORG-01 | 6 | 0 | 0 | Behavioral | OK |
| `repo_collaborators_acl.rs` | ORG-03/04 | 6 | 0 | 0 | Behavioral | OK |
| `repo_private_404.rs` | ORG-04 | 6 | 0 | 0 | Behavioral | OK |
| `git_smart_http.rs` | ORG-04 | 13 | 0 | 0 | Behavioral | OK |
| `acl.rs` coalesce_tests | ORG-02 | 9 | 0 | 0 | Value | OK |
| `*.integration.test.ts` (web) | ORG-01/03 | 15 | 0 | 0 | Existence (members/settings) / stronger elsewhere | WARNING (UI) |

**Disabled tests on requirements:** 0  
**Circular patterns detected:** 0  
**Insufficient assertions:** 1 WARNING (web members/settings existence tests) — not a blocker; API tests prove reqs.

### Human Verification Required

N/A — Roadmap truths are behaviorally proven by named API/git tests and Vitest route discovery. No PLAN `<human-check>` blocks. Optional UAT backstops in `10-VALIDATION.md` (live email invite UX, visual polish) remain discretionary and do not block phase status.

### Gaps Summary

None. Phase goal achieved: organizations, roles/`member_base`, visibility/collaborators, and private-data enforcement (web soft-404 + git/PAT ACL) are present, wired, and covered by passing behavioral tests.

**Resolved follow-up** (from `deferred-items.md`): shared-namespace dual-check on `auth.signup` / username rename is in tree (`login_slug_taken` / org-slug checks); deferred items marked `status: resolved`.

---

_Verified: 2026-09-14T02:07:44Z_  
_Verifier: Claude (gsd-verifier)_

## Automated re-verification (2026-09-19T18:14:28Z)

- Mode: fingerprint refresh (`covered_files` + `covered_digest`)
- Policy: existing phase VERIFICATION must-haves + SUMMARY/test evidence treated as sufficient; conversational UAT not re-run
- Covered inputs: 86 files

