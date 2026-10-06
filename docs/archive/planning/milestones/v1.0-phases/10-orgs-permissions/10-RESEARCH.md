# Phase 10: Orgs & Permissions - Research

**Researched:** 2026-09-14
**Domain:** Organization membership, repository collaborators, polymorphic owners, centralized forge ACL (Rust Axum RPC + Smart HTTP + future SSH)
**Confidence:** HIGH (codebase seams + locked CONTEXT); MEDIUM (invite closed-signup UX details, exact Admin destructive-reserve matrix)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
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

### Deferred Ideas (OUT OF SCOPE)
- Teams / user groups with bulk repo grants
- Org-scoped PATs
- Outside collaborator billing/seat limits (if ever cloud-metered)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| ORG-01 | User can create an organization and invite/add members | Org schema + RPC (`org.create`, members/invites); username add + email invite/accept; `/orgs/new` + members UI |
| ORG-02 | Org owner can assign member roles that control repo access | Org roles Owner/Admin/Member; `member_base_permission`; ACL maps Owner/Admin→admin, Member→base |
| ORG-03 | Repo owner can set visibility (public/private) and collaborator permissions | Existing visibility RPCs + new collaborator CRUD on personal **and** org repos; admin-gated |
| ORG-04 | Unauthorized users cannot read private repos or push without permission | Replace `can_read_as_owner` stub; wire RPC/raw/archive/Smart HTTP/(SSH) through central capabilities; preserve web `repo.not_found` vs git 401 |
</phase_requirements>

## Summary

Phase 10 replaces the Phase 7 **owner-only private ACL stub** with a real forge permission model: organizations (shared slug namespace with users), org membership roles, per-repo collaborators, and a single capability evaluator in `crates/oxidean-api/src/repo/acl.rs`. Today every read/mutate/git path still assumes `repositories.owner_id → users(id)` and `find_user_by_username` for `/{owner}/{repo}` resolution. Smart HTTP and PAT fine-grained minting additionally require `pat.user_id == owner_id` for push / FG selection — that must become “PAT subject has capability via ACL,” not “is the personal owner.”

GitHub’s documented org **base permissions** apply to members (not outside collaborators) and are overridden by higher explicit grants; Oxidean locks the Member default to **`none`** (stricter than GitHub’s common Read default for public org repos) and keeps Collaborator as a **per-repo** grant on personal and org-owned repos. Teams/units (Gitea-style) stay deferred.

**Primary recommendation:** Ship migration `0009_*` with `organizations` + members + invites + `repository_collaborators`, polymorphic `repositories.owner_type`/`owner_id`, shared slug uniqueness checks, and rewrite `acl.rs` to return `read|write|admin` used by all consumers — no new crates/npm packages.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Org create / slug reserve | API / Backend | Database / Storage | Unique slug + membership rows; disk dir created lazily on first repo |
| Member add by username | API / Backend | Browser / Client | Live lookup RPC + mutate; UI autocomplete only |
| Email invite / accept | API / Backend | Frontend Server (SSR) | Token hash at rest + email send; accept page/route |
| Org role + member_base settings | API / Backend | Browser / Client | Authority is server ACL; settings UI is presentation |
| Collaborator grants | API / Backend | Browser / Client | Per-repo table; settings Collaborators panel |
| Repo visibility toggle | API / Backend | Browser / Client | Already exists; gate by **admin** capability |
| Web browse/read ACL | API / Backend | Frontend Server (SSR) | Soft `repo.not_found`; SSR loaders call same RPC |
| Git HTTPS fetch/push ACL | API / Backend | — | Smart HTTP maps ACL→401/403; cookies ignored |
| Git SSH ACL (if present) | API / Backend | — | Must call same `acl` module (Phase 9 D-SSH-04) |
| PAT scope ∩ ACL | API / Backend | Browser / Client | FG repo picker lists ACL-accessible repos; authorize uses max(PAT, ACL) |
| Disk path `{slug}/{name}.git` | Database / Storage | API / Backend | Slug from user username or org slug; rename moves dir |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Existing `oxidean-api` / Axum RPC | in-tree | Org/collaborator procedures + ACL | Product already uses typed JSON RPC |
| Existing `oxidean-db` + sqlx migrations | in-tree | Dialect SQL for orgs/ACL tables | Dialect branching must stay in `oxidean-db` |
| Existing `oxidean-core` DTOs + specta | in-tree | Shared types / error codes | `make rpc-gen` source of truth |
| Existing `EmailSender` / lettre / Resend | in-tree | Invite emails | Reuse Phase 4–5 outbound email |
| Existing session auth + `require_verified` | in-tree | Privileged mutates | Same gates as `repo.create` / PAT create |
| System `git` CLI + Smart HTTP | in-tree | Fetch/push enforcement consumer | Phase 7–8 already wire git; ACL is the missing piece |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `uuid` | workspace (already OK) | Org/invite/collaborator row ids | Continue current ID style |
| Vitest + cargo-nextest | in-tree | Web + API tests | Nyquist sampling |
| Octane `.tsrx` + TanStack Query | in-tree | Org UI / owner picker | Follow Octane skill; Query for server state |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `owner_type` + `owner_id` on repos | Merge orgs into `users` with `type` flag (Gitea-like) | Faster slug uniqueness; conflates auth principals with orgs — worse fit for Oxidean users table |
| Separate `owners` table | `owner_type`+`owner_id` | Extra join for little gain at this scale |
| Casbin / OSO policy engine | Hand-written capability enum in `acl.rs` | Overkill; locked to central `acl.rs` |
| Gitea team/units ACL | Member base + collaborators | Teams deferred (D-ORG-07) |

**Installation:**
```bash
# No new packages — extend existing crates/apps only
make rpc-gen
make test
```

**Version verification:** No new registry packages recommended this phase. Existing `uuid` legitimacy check: `OK` (crates.io, high downloads, github.com/uuid-rs/uuid). `[VERIFIED: gsd package-legitimacy check]`

## Package Legitimacy Audit

> Phase installs **no** new external packages.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| *(none)* | — | — | — | — | — | N/A — no installs |

**Packages removed due to [SLOP] verdict:** none  
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```text
[Browser Octane UI]
  | session cookie RPC
  v
[Axum /api/rpc] ---- org.* / repo.collaborator* / user.lookup ----+
  |                                                                |
  | resolve owner slug                                             |
  v                                                                v
[Owner resolver] --> user | organization                    [EmailSender]
  |                                                          (invite mail)
  v
[repositories] --owner_type+owner_id--> user.id | organization.id
  |
  +--> [repository_collaborators]
  +--> [organization_members] + member_base_permission
  |
  v
[repo/acl.rs]  effective_capability(caller, repo) -> none|read|write|admin
  |
  +--> web RPC / raw / archive  --deny--> repo.not_found (D-25)
  +--> Smart HTTP               --deny private unauth--> 401 (D-21)
  |                              --deny scope--> 403 (D-23)
  +--> PAT mint/authorize       --intersect contents + ACL-->
  +--> SSH (Phase 9+)           --git error (D-SSH-04)-->
```

### Recommended Project Structure
```
crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0009_orgs_acl.sql
crates/oxidean-db/src/organizations.rs
crates/oxidean-db/src/org_members.rs
crates/oxidean-db/src/org_invites.rs
crates/oxidean-db/src/repo_collaborators.rs
crates/oxidean-core/src/org_types.rs          # DTOs + role enums
crates/oxidean-api/src/org/mod.rs             # org RPC handlers
crates/oxidean-api/src/repo/acl.rs            # REPLACE stub
crates/oxidean-api/src/repo/collaborators.rs  # collaborator RPC
apps/web/src/routes/orgs.new.tsrx
apps/web/src/routes/$owner.settings*.tsrx      # org settings/members (minimal)
apps/web/src/routes/$owner.$repo.settings.tsrx # Collaborators section
apps/web/src/routes/new.tsrx                  # owner picker
apps/web/src/routes/invites.$token.tsrx       # email invite accept
```

### Pattern 1: Capability enum + highest-wins evaluation
**What:** Single source of effective access for a `(caller_user_id?, repo)`.  
**When to use:** Every read/mutate/git/PAT gate after Phase 10.  
**Example:**
```rust
// Source: in-repo rewrite of crates/oxidean-api/src/repo/acl.rs (recommended)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    Read = 1,
    Write = 2,
    Admin = 3,
}

pub fn meets(have: Option<Capability>, need: Capability) -> bool {
    have.map(|h| h >= need).unwrap_or(false)
}

/// Highest-wins among applicable sources (implements D-ORG-05 intent:
/// Collaborator can raise; Owner/Admin always admin; Member uses base).
/// Personal owner (owner_type=user && owner_id==caller) => Admin.
pub fn coalesce(
    personal_owner: bool,
    org_role: Option<OrgRole>, // Owner|Admin|Member
    member_base: MemberBasePermission, // none|read|write
    collaborator: Option<Capability>,
    public_repo: bool,
) -> Option<Capability> {
    let mut best: Option<Capability> = None;
    let bump = |b: &mut Option<Capability>, c: Capability| {
        *b = Some(b.map(|x| x.max(c)).unwrap_or(c));
    };
    if personal_owner {
        bump(&mut best, Capability::Admin);
    }
    match org_role {
        Some(OrgRole::Owner) | Some(OrgRole::Admin) => bump(&mut best, Capability::Admin),
        Some(OrgRole::Member) => match member_base {
            MemberBasePermission::None => {}
            MemberBasePermission::Read => bump(&mut best, Capability::Read),
            MemberBasePermission::Write => bump(&mut best, Capability::Write),
        },
        None => {}
    }
    if let Some(c) = collaborator {
        bump(&mut best, c);
    }
    if public_repo {
        bump(&mut best, Capability::Read);
    }
    best
}
```

**Note on D-ORG-05 wording:** Treat listed sources as the **inputs** to evaluate; implement **max**/highest-wins so a Collaborator `read` cannot mask an org Admin’s `admin`, and Collaborator can still raise a Member with `none`. First-match short-circuit is an anti-pattern here. `[ASSUMED]` as interpretation of “evaluation order” vs GitHub highest-wins — aligns with “grant or raise.”

### Pattern 2: Polymorphic owner (discretion recommendation)
**What:** Prefer **`owner_type` + `owner_id`** on `repositories` over a separate owners table.  
**When to use:** Schema migration `0009`.  
**Example:**
```sql
-- Recommended shape (logical; dialect files diverge) — NOT yet in repo
-- organizations(id, slug UNIQUE, display_name, member_base_permission DEFAULT 'none', ...)
-- organization_members(org_id, user_id, role CHECK IN ('owner','admin','member'), UNIQUE(org_id,user_id))
-- organization_invites(id, org_id, email, role, token_hash UNIQUE, expires_at, invited_by, ...)
-- repository_collaborators(repo_id, user_id, permission CHECK IN ('read','write','admin'), UNIQUE(repo_id,user_id))
-- ALTER repositories: ADD owner_type TEXT NOT NULL DEFAULT 'user';
-- DROP FK repositories.owner_id -> users; keep owner_id as TEXT referencing user OR org by type
```

**Slug uniqueness:** On user signup/rename and org create/rename, reject if slug collides with `users.username` **or** `organizations.slug` (case-insensitive, same `validate_username` rules). Reuse reserved list — already includes `"org"` / `"orgs"`. `[VERIFIED: crates/oxidean-core/src/auth_types.rs:244-245]` quote: `"orgs",` / `"org",`

### Pattern 3: Owner slug resolution
**What:** Replace user-only lookup in `resolve_repo_for_read` / Smart HTTP `resolve_repo`.  
**When to use:** Any `/{owner}/{repo}` entry point.  
**Example:**
```rust
// Recommended — replace find_user_by_username-only path in acl.rs
enum OwnerRef {
    User { id: String, username: String },
    Org { id: String, slug: String },
}
// 1) find user by username
// 2) else find org by slug
// 3) else not_found / unauthorized_basic
// find_repository_by_owner_name(owner_id, name) unchanged keying on id
```

### Pattern 4: Mutate gates by capability (not `session.user_id == owner_id`)
**What:** Today `resolve_repo_for_owner_mutate` compares session to `row.owner_id`. `[VERIFIED: crates/oxidean-api/src/repo/mod.rs:480-492]`  
**When to use:** Branch CRUD, visibility, soft-delete, collaborators admin.  
**Replace with:**
- Need **write** for branch create/rename/delete (and git push)
- Need **admin** for visibility, soft-delete, collaborator management, org-destructive actions

### Anti-Patterns to Avoid
- **Leaving `can_read_as_owner` as the git gate:** Smart HTTP still calls it for private + push. `[VERIFIED: crates/oxidean-api/src/routes/git_smart_http.rs:417-426]`
- **PAT still requiring personal ownership:** Classic push and FG `All` / Selected ownership checks. `[VERIFIED: crates/oxidean-api/src/routes/git_smart_http.rs:287-315]` and `[VERIFIED: crates/oxidean-api/src/pat/mod.rs:232-237]`
- **First-match ACL that lets Collaborator lower Owner/Admin:** use highest-wins
- **Teams / unit permissions:** deferred (D-ORG-07); do not invent Gitea units
- **Hand-editing `packages/api-client`:** change Rust + `make rpc-gen`
- **Mixing JSX `return (` with Rivet `@if` in Octane:** breaks HMR/exports
- **Org-scoped PATs:** deferred; personal PATs only, authorized via ACL
- **Web 401 for private browse:** keep soft `repo.not_found` (D-25)

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Password hashing for invites | Custom crypto | Existing hex token + SHA-256 at rest (session/PAT pattern) | Proven in-tree |
| Invite email transport | New mail crate | `EmailSender` + LogSink/SMTP/Resend | Hot-rebuild settings already exist |
| Policy engine | Casbin/OSO | `Capability` in `acl.rs` | Small matrix; locked central module |
| Shared slug DB uniqueness across tables | App-only hope | Transactional dual-check + unique indexes per table | Race on concurrent signup/org create |
| Username validation for orgs | New rules | `validate_username` / reserved list | Same public URL namespace |
| Disk rename on slug change | Ad-hoc mv | Existing `rename_owner_repos_dir` | Already used for username rename |

**Key insight:** Phase 10 is mostly **schema + ACL rewrite + consumer rewiring**, not a new framework. The dangerous gaps are polymorphic owner resolution and PAT∩ACL — not UI chrome.

## Runtime State Inventory

> Schema/ownership migration phase — runtime leftovers after source edits.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | Existing `repositories.owner_id` FK→`users`; all current rows are user-owned | Migration: add `owner_type='user'`, drop user FK carefully per dialect; backfill; no user→org data rewrite required for empty orgs |
| Live service config | None specific to orgs (no graph/n8n) | None |
| OS-registered state | None | None — verified by absence of org systemd/pm2 units in product |
| Secrets/env vars | No org-specific env today; email provider settings already in DB/ENV | Invite mail uses existing email config; no new secret names required |
| Build artifacts | Generated `@oxidean/api-client` after RPC add | Regenerate via `make rpc-gen`; CI `rpc-sync-check` |
| Factory reset | `factory_reset_instance` deletes email_tokens/sessions/identities/users only | Must also wipe orgs/members/invites/collaborators (and org-owned repos) — today user DELETE cascades user-owned repos + PATs, **not** future org rows `[VERIFIED: crates/oxidean-db/src/lib.rs:509-617]` |
| Disk | `var/repos/{username}/` | Org repos create `{org_slug}/`; org slug rename must move dir like user rename |

**Nothing found in category:** Live service config / OS-registered state — none for orgs.

## Common Pitfalls

### Pitfall 1: User-only owner resolution after orgs exist
**What goes wrong:** `/{org}/{repo}` 404s even for public org repos.  
**Why it happens:** `resolve_repo_for_read` only `find_user_by_username`. `[VERIFIED: crates/oxidean-api/src/repo/acl.rs:44-51]`  
**How to avoid:** Shared owner resolver used by RPC, raw/archive, Smart HTTP, SSH.  
**Warning signs:** Tests only create user-owned fixtures.

### Pitfall 2: Soft not_found vs git 401 drift
**What goes wrong:** Private collaborator denied on web with 401, or git returns not_found JSON.  
**Why it happens:** Status mapping mixed into ACL decision.  
**How to avoid:** `acl` returns capability; callers map status (web D-25, git D-21, SSH D-SSH-04).  
**Warning signs:** Shared helper returning `AppError`/`Response` instead of capability.

### Pitfall 3: PAT still owner-centric
**What goes wrong:** Collaborators can browse in UI but cannot `git push` with classic PAT.  
**Why it happens:** `pat_allows_operation` / FG mint ownership checks.  
**How to avoid:** Authorize: authenticated user + PAT scope/contents **and** `meets(acl, need)`; FG Selected allows any repo where subject currently has ≥ requested contents; FG All = personal-owned + org repos where subject is Owner/Admin (recommend; document).  
**Warning signs:** Integration tests only cover owner PAT.

### Pitfall 4: Closed signup breaks email invites
**What goes wrong:** Self-host `allow_signup=false` cannot onboard invitees.  
**Why it happens:** Accept flow routes through public signup.  
**How to avoid (discretion recommendation):** Invite accept **bypasses `allow_signup`** to create the invited account (or link existing), still subject to email verify / password rules; username-add path never needs signup. Anti-enumeration on invite create for unknown emails stays soft-success where appropriate.  
**Warning signs:** E2E only with open signup.

### Pitfall 5: Username lookup enumerates emails/users
**What goes wrong:** Autocomplete becomes an oracle.  
**Why it happens:** Unbounded prefix search / returning emails.  
**How to avoid:** Prefix ≥2 chars, limit ≤10, rate-limit per session/IP, return username+display+avatar only; do not search by email in live lookup.  
**Warning signs:** Lookup accepts `@` email strings.

### Pitfall 6: Member base `none` + public repo confusion
**What goes wrong:** Org Members cannot read **public** org repos.  
**Why it happens:** Forgetting public ⇒ Read for everyone (incl. anonymous).  
**How to avoid:** Public visibility always grants Read in coalesce; `member_base` mainly affects **private** org repos.  
**Warning signs:** Private tests pass; public Member denied.

### Pitfall 7: MySQL soft-delete uniqueness / FK drop
**What goes wrong:** Migration fails on MySQL generated `active_name` or FK drop.  
**Why it happens:** Dialect-specific 0007 patterns.  
**How to avoid:** Mirror Phase 7 dialect care; test `make db-matrix` / dialect tests for 0009.  
**Warning signs:** Postgres-only green.

### Pitfall 8: `/new` still hard-locks owner to current user
**What goes wrong:** Cannot create org-owned repos.  
**Why it happens:** `CreateRepoRequest` has no owner field; create uses `user.id` / `user.username`. `[VERIFIED: crates/oxidean-core/src/repo_types.rs:34-50]` `[VERIFIED: crates/oxidean-api/src/repo/mod.rs:666-720]`  
**How to avoid:** Add optional `owner` slug (default self); require org create permission (Owner/Admin — recommend).  
**Warning signs:** UI picker cosmetic only.

## Code Examples

### Current stub to replace
```rust
// Source: crates/oxidean-api/src/repo/acl.rs:21-24
/// Owner-only private read until Phase 10 collaborators.
pub fn can_read_as_owner(caller_user_id: Option<&str>, owner_id: &str) -> bool {
    caller_user_id == Some(owner_id)
}
```

### Web anti-enumeration (preserve)
```rust
// Source: crates/oxidean-api/src/repo/acl.rs:11-14
/// Identical error for missing repos and unauthorized private access (anti-enumeration).
pub fn not_found() -> AppError {
    AppError::new("repo.not_found", "Repository not found")
}
```

### Recommended RPC surface (planner names — implement via specta + rpc-gen)
```text
org.create { slug, display_name? }
org.get { slug }
org.updateSettings { slug, member_base_permission?, display_name? }  # Admin+
org.listMine {}
org.members.list { slug }
org.members.add { slug, username, role }           # existing users
org.members.updateRole { slug, user_id, role }
org.members.remove { slug, user_id }
org.invites.create { slug, email, role }
org.invites.list { slug }
org.invites.revoke { slug, invite_id }
org.invites.accept { token, ... account fields if needed }
user.lookup { prefix }                             # live autocomplete
repo.collaborators.list { owner, name }
repo.collaborators.add { owner, name, username, permission }
repo.collaborators.update { owner, name, user_id, permission }
repo.collaborators.remove { owner, name, user_id }
repo.create — extend with optional owner slug
repo.listMine — extend or add listAccessible for home + PAT picker
```

### Capability → operation map (planner lock-in)
| Operation | Need |
|-----------|------|
| Browse / fetch / raw / archive / clone | `read` |
| Push / branch create·rename·delete | `write` |
| Visibility, soft-delete, collaborators, org settings (non-destructive) | `admin` |
| Org delete / transfer (if any UI) | Owner only (not mere Admin) |

### Admin vs Owner (discretion recommendation)
- **Admin:** manage members, invites, `member_base_permission`, non-destructive org profile; admin on all org repos  
- **Owner:** everything Admin can + reserve last-owner protection, org delete / ownership transfer (even if transfer UI is later)  
- Creating an org: creator becomes **Owner** member row  

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Private = personal owner only | Org roles + collaborators + member_base | Phase 10 | Real multi-user forge |
| `owner_id → users` only | Polymorphic owner_type + id | Phase 10 | Org-owned repos + shared URLs |
| PAT push iff personal owner | PAT ∩ ACL capability | Phase 10 | Collaborators can use personal PATs |
| GitHub Triage/Maintain roles | Oxidean `read\|write\|admin` only | Locked D-ORG-02c | Simpler matrix until later |
| Gitea teams/units | Deferred | D-ORG-07 | Avoid premature IA |

**Deprecated/outdated:**
- `can_read_as_owner` as production gate — keep temporarily as thin wrapper calling `meets(..., Read)` only if needed for compile churn, then delete
- ARCHITECTURE.md “owner-only until org collaborators” — update when shipping

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | ACL uses highest-wins coalesce rather than strict first-match short-circuit | Architecture Patterns | Wrong effective perms if planner implements naive order |
| A2 | Email invite accept may create accounts when `allow_signup=false` | Pitfall 4 / Discretion | Self-host onboarding broken if disallowed |
| A3 | Org Admin may manage members/invites/settings; Owner-only for delete/transfer | Code Examples | Over/under-privileged Admins |
| A4 | FG PAT `All` means personal-owned + org repos where subject is Owner/Admin | Pitfall 3 | Collaborator-only users must use Selected |
| A5 | `repo.create` gains optional owner slug; create-in-org requires Owner/Admin | Pitfall 8 | Members with write base cannot create repos (acceptable default) |
| A6 | No new npm/crates dependencies required | Standard Stack | If UI needs combobox lib, revisit legitimacy |

## Open Questions (RESOLVED)

1. **Org profile at `/{org}` vs `/{org}/settings` only** — **RESOLVED:** Minimal public org overview at `/{org}` listing public repos + members count; settings nested under `/{org}/settings` (plan 10-10). Keeps `/orgs/new` for create.

2. **Last org Owner removal** — **RESOLVED:** Block removing/demoting the last Owner with stable error `org.last_owner` (plan 10-05).

3. **Phase 9 sequencing** — **RESOLVED:** Land central `acl.rs` in Phase 10; SSH (Phase 9) and Smart HTTP both call it. Migration **0010_orgs_acl** (Phase 9 owns **0009_ssh_keys**). Parallel plan/execute OK; ACL call sites updated once in plans 10-04/10-08.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust/cargo | API/db work | ✓ | 1.100.0-nightly | — |
| Bun | Web Vitest/Octane | ✓ | 1.4.0 | — |
| git CLI | Existing forge tests | ✓ | 2.55.0 | — |
| make | rpc-gen / test | ✓ | present | — |
| Email (LogSink) | Invite tests | ✓ | in-tree | LogSink in unit tests; Mailpit in e2e stack |
| Docker Compose | Full e2e optional | not probed this session | — | API integration tests with sqlite tempfile |

**Missing dependencies with no fallback:** none for core phase work  
**Missing dependencies with fallback:** live SMTP/Resend — use LogSink in automated tests

Step 2.6: External tools required only as above (no new services).

## Validation Architecture

> `workflow.nyquist_validation` is **true** in `.planning/config.json`.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo nextest (Rust) + Vitest (web) |
| Config file | workspace Cargo / `apps/web/vitest.config.ts` |
| Quick run command | `cargo nextest run -p oxidean-api -E 'test(repo_private) or test(acl) or test(org)'` (adjust filter as tests land) |
| Full suite command | `make test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ORG-01 | Create org; add member by username; email invite accept | API integration | `cargo nextest run -p oxidean-api -E 'test(org_)'` | ❌ Wave 0 |
| ORG-02 | Owner/Admin admin; Member respects member_base none/read/write | unit + API | colocated `acl` unit tests + org ACL integration | ❌ Wave 0 |
| ORG-03 | Collaborator CRUD on personal + org repos; visibility admin-gated | API + Vitest | extend `repo_settings_*` + settings UI test | ❌ Wave 0 (extend existing) |
| ORG-04 | Private non-grantee → web `repo.not_found`; git → 401; push denied without write | API | extend `repo_private_404.rs`, `git_smart_http.rs` | ✅ stubs exist — must extend |
| ORG-04 | PAT collaborator push with classic `repo` scope | API | extend `pat_rpc` / smart http | ✅ partial — must extend |
| ORG-01 | Username lookup rate/limit shape | API unit | new | ❌ Wave 0 |
| ORG-01/03 | `/orgs/new`, owner picker, collaborators UI | Vitest integration | new route tests | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** focused nextest filter for touched crate + relevant Vitest file
- **Per wave merge:** `make test` (or at least `cargo nextest run --workspace` + web vitest for changed apps)
- **Phase gate:** Full suite green before `/gsd-verify-work`; include dialect migration smoke if 0009 touches MySQL quirks

### Wave 0 Gaps
- [ ] `crates/oxidean-api/tests/org_create_members.rs` — ORG-01/02
- [ ] `crates/oxidean-api/tests/org_invites.rs` — email invite + closed signup
- [ ] `crates/oxidean-api/tests/repo_collaborators_acl.rs` — ORG-03/04 matrix
- [ ] Extend `repo_private_404.rs` + `git_smart_http.rs` for collaborator/org Member cases
- [ ] Extend PAT authorize tests for non-owner collaborator
- [ ] `apps/web` integration tests for `/orgs/new`, owner picker, collaborators panel
- [ ] Factory reset coverage for org tables
- [ ] Unit tests in `acl.rs` for coalesce matrix (Owner/Admin/Member×base×collaborator×public)

## Security Domain

> `security_enforcement` enabled; ASVS level 1.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes (invite accept / account link) | Existing sessions; invite token hash-at-rest; `require_verified` for privileged mutates |
| V3 Session Management | yes | Existing opaque session cookies; no PAT-as-RPC |
| V4 Access Control | **yes — primary** | Central `acl.rs` capabilities; identical web not_found; git 401/403 split |
| V5 Input Validation | yes | `validate_username` for org slugs; role/permission enums; RPC serde |
| V6 Cryptography | yes (invite tokens) | SHA-256 hex like sessions/PATs; no plaintext token storage |

### Known Threat Patterns for forge ACL / orgs

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Private repo enumeration | Information disclosure | Soft `repo.not_found` for web (D-25) |
| Git private probe | Information disclosure | 401 + WWW-Authenticate (D-21), not 404 |
| Privilege escalation via Collaborator | Elevation | Admin-only collaborator manage; max capability still bounded |
| Invite token theft | Spoofing | High-entropy token, hash at rest, expiry, single-use accept |
| Username autocomplete oracle | Information disclosure | Prefix length, limit, rate-limit, no email in results |
| Closed-signup bypass abuse | Elevation | Invite-gated create only with valid invite; rate-limit invite create |
| PAT over-scope | Elevation | Intersect PAT contents/scopes with ACL capability |
| Last-owner removal | Denial of service | Reject demote/remove of final Owner |
| Cross-org slug takeover | Tampering | Shared namespace checks on user+org create/rename |

## Project Constraints (from .cursor/rules/)

| Rule | Directive for Phase 10 |
|------|------------------------|
| `oxidean-core.mdc` | One product; Bun + Cargo; Octane `.tsrx`; RPC via Rust→`make rpc-gen`; dialect SQL only in `oxidean-db`; no secrets; extend existing ACL/auth patterns |
| `rust-crates.mdc` | No dialect branching in API; `Result`+structured errors; preserve `require_verified`/admin gates; tests colocated + `crates/*/tests` |
| `rpc-codegen.mdc` | Do not hand-patch api-client; `make rpc-sync-check` clean |
| `octane-ui.mdc` | `.tsrx` + `@if`/`@else`/`@for`; Query for server state; `onInput` for text; no react→octane alias |

Skills to honor during planning/execution: `.agents/skills/octane/SKILL.md`, `rust-best-practices`, `tdd`, `codebase-design`.

## Sources

### Primary (HIGH confidence)
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — locked D-ORG-*
- `crates/oxidean-api/src/repo/acl.rs` — owner-only stub
- `crates/oxidean-api/src/repo/mod.rs` — owner mutate + create
- `crates/oxidean-api/src/routes/git_smart_http.rs` — PAT∩owner gates
- `crates/oxidean-api/src/pat/mod.rs` — FG ownership check
- `crates/oxidean-db/migrations/*/0007_repositories.sql` — user-only FK
- `crates/oxidean-core/src/auth_types.rs` — reserved `org`/`orgs`
- `crates/oxidean-db/src/lib.rs` — factory_reset wipe list
- Prior CONTEXT 07/08/09 — D-23–26, PAT owner-centric, D-SSH-04
- `.planning/config.json` — nyquist + security_enforcement

### Secondary (MEDIUM confidence)
- [GitHub: Setting base permissions for an organization](https://docs.github.com/en/organizations/managing-user-access-to-your-organizations-repositories/managing-repository-roles/setting-base-permissions-for-an-organization) — base perms apply to members not outside collaborators; higher grants override `[CITED: docs.github.com/.../setting-base-permissions-for-an-organization]`
- [GitHub: Repository roles for an organization](https://docs.github.com/en/organizations/managing-user-access-to-your-organizations-repositories/managing-repository-roles/repository-roles-for-an-organization) — role ladder context (Oxidean subset read/write/admin)
- [Gitea permissions docs](https://docs.gitea.com/usage/access-control/permissions) — team/unit model explicitly **not** chosen (D-ORG-07)

### Tertiary (LOW confidence)
- WebSearch digests on polymorphic forge owners / Gitea user-table org storage — used only as design contrast `[ASSUMED]` ecosystem lore

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new deps; extend in-tree forge stack
- Architecture: HIGH — seams and call sites verified in code; schema shape is discretion with clear recommendation
- Pitfalls: HIGH — derived from current owner-only and PAT checks
- Invite closed-signup UX: MEDIUM — discretion recommendation needs planner lock

**Research date:** 2026-09-14  
**Valid until:** 2026-10-14 (stable domain; re-check if Phase 9 SSH lands conflicting ACL helpers)
