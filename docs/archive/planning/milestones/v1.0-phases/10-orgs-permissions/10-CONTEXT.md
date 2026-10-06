# Phase 10: Orgs & Permissions - Context

**Gathered:** 2026-09-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Users collaborate via organizations and repository permissions with private data truly private. Delivers ORG-01, ORG-02, ORG-03, ORG-04.

**Requirements:** ORG-01, ORG-02, ORG-03, ORG-04

**Success criteria (from ROADMAP):**
1. User can create an organization and invite/add members
2. Org owner can assign member roles that control repo access; repo owner can set visibility and collaborator permissions
3. Unauthorized users cannot read private repos or push without permission

**Out of scope (later phases / v2):**
- Teams as a first-class grouping (deferred)
- Branch protection (ORG-05/06 → Phase 13)
- Org-scoped PATs (deferred from Phase 8; personal PATs gain access via new ACL)
- Issues/PRs (Phases 11–12) — ACL must be ready for them later
- Repo rename/transfer (Phase 15)

**UI hint:** yes — `/orgs/new`, org settings/members, repo collaborators, `/new` owner picker.

</domain>

<decisions>
## Implementation Decisions

### A — Owner identity & slug
- **D-ORG-01:** **Shared slug namespace** for users and orgs (GitHub/Gitea-style). URLs stay `/{owner}/{repo}`; disk path `{slug}/{name}.git`; polymorphic owner in DB (`owner_type` + id or owners table). Creating an org reserves a slug like a username — **Reversibility:** one-way — URL + disk + ACL identity

### B — Role model
- **D-ORG-02a:** Org membership roles: **Owner**, **Admin**, **Member** only. **Collaborator** is a **separate per-repository grant** (not an org membership role) for out-of-org (and in-org) access on **personal** and **org-owned** repositories. Collaborators are not an org-level role — **Reversibility:** costly — ACL + UI IA
- **D-ORG-02b:** Default **Member** base permission on org private repos is **`none`**. Org-level setting: **`member_base_permission` = `none` | `read` | `write`** (GitHub-style). **Owner** and **Admin** retain full **admin** on org-owned repos regardless. Per-repo Collaborator can still grant or raise access on a single repo — **Reversibility:** costly — org settings + ACL evaluation order
- **D-ORG-02c:** Collaborator permission ladder on a repo: **`read` | `write` | `admin`** (same for personal and org-owned) — **Reversibility:** costly — capability matrix

### C — Invite / add members
- **D-ORG-03:** Phase 10 ships **both**: (1) add existing instance users by **username with live lookup/autocomplete**, and (2) **email invite + accept** for people not yet on the instance. Username add must work when `allow_signup` is false; email invite path must respect signup/email constraints (planner details closed-signup behavior) — **Reversibility:** costly — invite tables + email templates

### D — Personal + org-owned collaborators
- **D-ORG-04:** **Both** personal-owned and org-owned repositories support Collaborators in Phase 10 (ORG-03). Out-of-org people use Collaborator grants only (they are not org Members unless also invited to the org)

### E — ACL enforcement (defaults — discuss skipped)
- **D-ORG-05:** Centralize capabilities in `repo/acl.rs` (`read` / `write` / `admin`). Wire web RPC, owner-mutate, Smart HTTP fetch/push, raw/archive, and PAT FG selection through it. Keep web **`repo.not_found`** anti-enumeration (D-25) and git private **401** (Phase 8 D-21); SSH uses git errors (Phase 9 D-SSH-04). Evaluation order: repo Collaborator grant → org role (Owner/Admin) → org `member_base_permission` → public visibility → deny

### F — Org UX (defaults — discuss skipped)
- **D-ORG-06:** Minimal routes: `/orgs/new`, org overview at `/{org}` (or settings under `/{org}/settings`), members/invites UI, repo settings → Collaborators, `/new` **owner picker** (user + orgs where caller can create). Reserved usernames already include `org`/`orgs`

### G — Teams (defaults — discuss skipped)
- **D-ORG-07:** **Defer teams** entirely — not in Phase 10

### Claude's Discretion
- Exact polymorphic owner schema (owners table vs `owner_type`+`owner_id` on repositories)
- Exact invite token format, expiry, and closed-signup email-invite UX copy
- Exact live username lookup RPC (prefix search, rate limits, anti-enumeration)
- Whether org Admin may manage members/invites and org settings (assume yes except destructive transfer/delete reserved for Owner — planner may refine)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 10 goal, ORG-01…04
- `.planning/REQUIREMENTS.md` — ORG-01…04 wording
- `.planning/phases/07-git-repos-browse/07-CONTEXT.md` — D-05 user-only owner; D-23–D-26 ACL stub
- `.planning/phases/08-git-https-pats/08-CONTEXT.md` — collaborator ACL deferred; PAT owner-centric
- `.planning/phases/09-git-ssh/09-CONTEXT.md` — SSH ACL must call shared module after Phase 10

### Code mirrors
- `crates/oxidean-api/src/repo/acl.rs` — `can_read_as_owner` stub to replace
- `crates/oxidean-api/src/repo/mod.rs` — owner mutate gates
- `crates/oxidean-db/migrations/*/0007_repositories.sql` — user-only `owner_id`
- `crates/oxidean-api/src/routes/git_smart_http.rs` — git ACL consumer
- `apps/web/src/routes/$owner.$repo*.tsrx` — flat owner/repo chrome
- `apps/web/src/routes/new.tsrx` — owner locked to current user today
- `docs/ARCHITECTURE.md` — owner-only private ACL until org collaborators

</canonical_refs>

<code_context>
## Reusable assets
- Repo visibility public/private already shipped
- Soft ACL stub with identical not_found for private non-owner
- Instance roles user/admin/sys-admin (orthogonal to org roles)
- Reserved slugs include org/orgs

## Gaps
- No organizations / members / invites / collaborators tables or RPCs
- Disk path and ACL assume user username owner only
</code_context>

<deferred>
## Deferred Ideas
- Teams / user groups with bulk repo grants
- Org-scoped PATs
- Outside collaborator billing/seat limits (if ever cloud-metered)
</deferred>
