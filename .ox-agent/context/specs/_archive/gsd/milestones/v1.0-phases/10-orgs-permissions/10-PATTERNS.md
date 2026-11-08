# Phase 10: Orgs & Permissions - Pattern Map

**Mapped:** 2026-09-14
**Files analyzed:** 16
**Analogs found:** 15 / 16

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/oxidean-api/src/repo/acl.rs` | service | request-response | *(self — expand stub)* | exact |
| `crates/oxidean-api/src/repo/mod.rs` | controller | CRUD | *(self — mutate gates)* | exact |
| `crates/oxidean-api/src/routes/git_smart_http.rs` | middleware | request-response | *(self — git ACL)* | exact |
| `crates/oxidean-api/src/routes/repo_raw.rs` | controller | file-I/O | `repo/acl.rs` + raw routes | exact |
| `crates/oxidean-db/migrations/*/0010_orgs_acl.sql` | migration | CRUD | `0007_repositories.sql` + `0008_pats.sql` | role-match — Phase 09 owns `0009_ssh_keys` |
| `crates/oxidean-db/src/repositories.rs` | model | CRUD | *(self — owner_id)* | exact |
| `crates/oxidean-db/src/organizations.rs` (new) | model | CRUD | `repositories.rs` + `email_tokens.rs` | role-match |
| `crates/oxidean-api/src/org/` (new module) | controller | CRUD | `repo/mod.rs` + `auth/admin.rs` | role-match |
| `crates/oxidean-api/src/org/invite.rs` (or auth-adjacent) | service | request-response | `auth/verify_reset.rs` | exact |
| `crates/oxidean-core` org/ACL types | model | transform | `repo_types.rs` + `auth_types.rs` | role-match |
| `crates/oxidean-core/src/auth_types.rs` | utility | transform | *(self — RESERVED_USERNAMES)* | exact |
| `apps/web/src/routes/orgs.new.tsrx` | route | request-response | `routes/new.tsrx` | exact |
| `apps/web/src/routes/$owner.$repo.settings.tsrx` | component | CRUD | *(self — collaborators tab)* | exact |
| Org settings / members UI | route | CRUD | `settings/profile.tsrx` + `settings-nav` + `admin/auth.tsrx` | role-match |
| `apps/web/src/routes/new.tsrx` | route | request-response | *(self — owner picker)* | exact |
| Username lookup RPC | controller | request-response | `auth/local.rs` + `profile.rs` uniqueness | partial |

## Pattern Assignments

### `crates/oxidean-api/src/repo/acl.rs` (service, request-response)

**Analog:** `crates/oxidean-api/src/repo/acl.rs` (replace stub; keep web status mapping)

**Core stub to replace** (lines 21–24):
```rust
/// Owner-only private read until Phase 10 collaborators.
pub fn can_read_as_owner(caller_user_id: Option<&str>, owner_id: &str) -> bool {
    caller_user_id == Some(owner_id)
}
```

**Anti-enumeration + resolve pattern** (lines 11–14, 32–77):
```rust
pub fn not_found() -> AppError {
    AppError::new("repo.not_found", "Repository not found")
}

/// Resolve `owner`/`name` for read. Missing OR private and caller ≠ owner → identical [`not_found`].
pub async fn resolve_repo_for_read(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    // find_user_by_username(owner) → find_repository_by_owner_name
    // if private && !can_read_as_owner → not_found()
    // …
}
```

**Planner notes:**
- Keep `not_found()` identical for missing vs unauthorized private (D-ORG-05 / D-25).
- Replace `can_read_as_owner` with capability helpers (`can_read` / `can_write` / `can_admin`) using evaluation order: Collaborator → org Owner/Admin → `member_base_permission` → public → deny.
- `resolve_repo_for_read` must resolve **shared slug** (user *or* org), not only `find_user_by_username`.
- Export new helpers; update `repo/mod.rs` re-exports.

---

### `crates/oxidean-api/src/repo/mod.rs` — mutate gates (controller, CRUD)

**Analog:** `resolve_repo_for_owner_mutate` + `update_visibility` / `branch_create`

**Owner-only mutate gate** (lines 480–493):
```rust
/// Resolve repo for owner-only mutate (D-27). Non-owner → identical [`acl::not_found`].
async fn resolve_repo_for_owner_mutate(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let _ = require_verified(ctx).await?;
    let session = require_session_user(ctx)?;
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if session.user_id != accessible.row.owner_id {
        return Err(acl::not_found());
    }
    Ok(accessible)
}
```

**Wire mutate to ACL capability** — replace `session.user_id != owner_id` with `can_admin` (or `can_write` for branch ops) from `acl.rs`. Keep `require_verified` + identical `not_found` for unauthorized.

**Create today locks owner to session user** (lines 666–720):
```rust
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoPublic, AppError> {
    let user = require_verified(ctx).await?;
    // …
    .insert_repository(&id, &user.id, &name, /* … */)
    let path = bare_repo_path(&ctx.repos_dir, &user.username, &name)?;
```

**Planner notes:** Extend `CreateRepoRequest` with owner selector; authorize create on personal self or org where caller can create; disk path uses **owner slug** via existing `bare_repo_path`.

---

### `crates/oxidean-api/src/routes/git_smart_http.rs` (middleware, request-response)

**Analog:** *(self)* — PAT Basic auth + owner-only ACL

**Imports / ACL consumers** (lines 24, 412–425):
```rust
use crate::repo::{can_read_as_owner, is_private_visibility};
// …
// Private / no-access unauth → 401 (D-21), not web not_found.
if is_private && !can_read_as_owner(Some(caller_id), &resolved.owner_id) {
    return unauthorized_basic();
}
// Push is owner-only until Phase 10.
if receive && !can_read_as_owner(Some(caller_id), &resolved.owner_id) {
    return unauthorized_basic();
}
```

**PAT FG selection still owner-centric** (lines 287–332) — `pat_allows_operation` uses `pat.user_id == owner_id` for Classic push and FG `All`. After Phase 10 ACL, PAT grant still requires repo capability **and** PAT scope; FG `Selected` already keys on `repository_ids`.

**Status mapping rule:** web → `repo.not_found`; git private/unauth → `401` + `WWW-Authenticate: Basic realm="Oxidean Git"`. Do not unify these.

**Repo resolve for git** still uses `find_user_by_username` (lines 260–282) — must become shared-slug resolve like ACL.

---

### `crates/oxidean-api/src/routes/repo_raw.rs` (controller, file-I/O)

**Analog:** uses `repo::resolve_repo_for_read` (cookie session).

Once `resolve_repo_for_read` grows org/collaborator logic, raw/archive inherit it — no parallel ACL.

---

### Migrations `0009_*` + `repositories` polymorphic owner (migration / model)

**Analog schema:** `crates/oxidean-db/migrations/postgres/0007_repositories.sql`

```sql
CREATE TABLE IF NOT EXISTS repositories (
  id             TEXT        PRIMARY KEY,
  owner_id       TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name           VARCHAR(100) NOT NULL,
  visibility     TEXT        NOT NULL DEFAULT 'public',
  -- …
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_repositories_owner_name_active
  ON repositories (owner_id, lower(name))
  WHERE deleted_at IS NULL;
```

**Join-table analog:** `0008_pats.sql` `personal_access_token_repos` (token ↔ repo M:N).

**Email-invite token analog:** `0003_email_tokens.sql`:
```sql
CREATE TABLE IF NOT EXISTS auth_email_tokens (
  id            TEXT        PRIMARY KEY,
  user_id       TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  purpose       TEXT        NOT NULL,
  token_hash    CHAR(64)    NOT NULL,
  otp_hash      CHAR(64)    NOT NULL,
  expires_at    TIMESTAMPTZ NOT NULL,
  attempt_count INTEGER     NOT NULL DEFAULT 0,
  UNIQUE (user_id, purpose)
);
```

**DB API dialect pattern** — `crates/oxidean-db/src/repositories.rs`:
- `RepositoryRow` + `map_repo!` macro
- `REPO_SELECT_{PG,MYSQL,SQLITE}` with dialect timestamp formatting
- `match pool { DbPool::Postgres | MySql | Sqlite }` — **no dialect branching in API crate**
- Facade methods on `Database` in `lib.rs` (`insert_repository`, `find_repository_by_owner_name`, …)

**Planner notes (discretion):** Prefer `owner_type` + `owner_id` (or owners table) while preserving unique `(owner, lower(name))` among non-deleted. Triple-migrate postgres/mysql/sqlite identically. Next logical number after `0008_pats`.

---

### Disk path — `bare_repo_path` (utility)

**Analog:** `crates/oxidean-api/src/git/mod.rs` lines 13–32:
```rust
/// Bare repo path: `{repos_dir}/{owner}/{name}.git` (D-30).
pub fn bare_repo_path(repos_dir: &Path, owner: &str, name: &str) -> Result<PathBuf, AppError> {
    // reject empty, `/`, `\`, `..`
    Ok(repos_dir.join(owner).join(format!("{name}.git")))
}
```

Org slug replaces username in the `owner` segment — same helper; ensure slug validation matches `validate_username`.

---

### Org RPC module (controller, CRUD)

**Analog handlers:** `repo/mod.rs` (serde input → gate → db → `AppError` codes) + `auth/admin.rs` (`require_admin`-style role gates).

**RPC dispatch:** `crates/oxidean-api/src/rpc.rs` match arms — add `org.*` next to `repo.*` / `admin.*`; regenerate client with `make rpc-gen`.

**Gate pattern:** `crates/oxidean-api/src/auth/gate.rs`:
```rust
pub async fn require_verified(ctx: &RpcCtx) -> Result<UserRow, AppError> {
    // unauthenticated → auth.unauthenticated
    // unverified → auth.email_unverified
}
```

Org create / invite / member mutate should use `require_verified`. Org Owner vs Admin capability checks live in org service (orthogonal to instance `is_admin`).

**Error / uniqueness pattern** — `auth/profile.rs` / `auth/local.rs`:
```rust
AppError::new("auth.reserved_username", "username is reserved")
AppError::new("auth.taken", "email or username already taken")
AppError::new("auth.signup_closed", "Sign-up is closed for this instance.")
```

Org slug create should call `validate_username` + `is_reserved_username` + uniqueness against **both** users and orgs (shared namespace D-ORG-01).

---

### Invite / email tokens (service, request-response)

**Analog:** `crates/oxidean-api/src/auth/verify_reset.rs`

**Token issue constants** (lines 16–25):
```rust
const PURPOSE_VERIFY: &str = "verify";
const PURPOSE_RESET: &str = "reset";
const TOKEN_BYTES: usize = 32;
const TTL_SECS: i64 = 30 * 60;
const MIN_ISSUE_INTERVAL_SECS: i64 = 60;
const MAX_ISSUES_PER_HOUR: i32 = 5;
```

**Hash-at-rest + email build** (lines 47–48, 163–197):
```rust
fn sha256_hex(data: &[u8]) -> String { /* … */ }
fn build_verify_email(to: &str, username: &str, magic: &str, otp: &str) -> OutboundEmail {
    let link = format!("{origin}/verify?token={magic}");
    // …
}
```

**Anti-enumeration on password reset** (lines 130–133): always `{ "ok": true }` for `request_password_reset`.

**Closed signup:** `auth/local.rs` lines 151–156 — `allow_signup == false` → `auth.signup_closed`. Email-invite accept path must respect this (CONTEXT D-ORG-03); username-add of existing users must work regardless.

**Planner notes:** New invite purpose/table (do not overload verify/reset `UNIQUE (user_id, purpose)` if invitees have no user yet — store email + org_id + token_hash). Reuse `OutboundEmail`, `public_origin()`, rate-limit shape, CSPRNG magic hex.

---

### Reserved usernames (utility)

**Analog:** `crates/oxidean-core/src/auth_types.rs` lines 234–307

Already includes `"orgs"`, `"org"`, `"new"`, `"settings"`, `"admin"`, etc. Org slug creation must reuse:
```rust
pub fn is_reserved_username(u: &str) -> bool { /* case-insensitive */ }
pub fn validate_username(raw: &str) -> Result<(), String> { /* 1–39, alnum+hyphen, reserved */ }
```

Signup / profile / bootstrap already map reserved → `auth.reserved_username`. Org create should emit the same or a dedicated `org.reserved_slug` that UI can treat identically.

---

### `apps/web/src/routes/new.tsrx` — owner picker (route)

**Analog:** *(self)* — owner locked to current user

**Auth wall + loader** (lines 38–65): `beforeLoad` / `loader` via `fetchSessionMe` + verify wall.

**Owner UI stub** (lines 91, 152–171):
```typescript
const ownerLabel = user?.username ? `@${user.username}` : "you";
// …
<p>Owner is you for now. Organizations come in a later phase.</p>
<Label>Owner</Label>
<p className="text-[16px] text-foreground">{ownerLabel as string}</p>
```

**Create call** (lines 103–111) — no `owner` in `CreateRepoRequest` today; add owner field after RPC change. Redirect: `` `/${res.data.owner_username}/${res.data.name}` ``.

**Form conventions (Octane):** `method="post" action="#"`, `useState` for local fields, `apiClient` mutations, error codes → field messages (`repo.name_taken`, `auth.email_unverified`).

---

### `apps/web/src/routes/$owner.$repo*.tsrx` (route / layout)

**Analog layout:** `apps/web/src/routes/$owner.$repo.tsrx`

**SSR loader** (lines 22–81): `fetchRepoGet({ owner, name })` + optional `fetchSessionMe`; `repo.not_found` → not-found chrome (anti-enumeration).

**Settings owner check** — `$owner.$repo.settings.tsrx` lines 62–66:
```typescript
const isOwner = !!repo && !!me && me.id === repo.owner_id;
const showNotFound =
  !layout ||
  layout.status === "not_found" ||
  (layout.status === "ok" && !!repo && !isOwner);
```

Replace `me.id === repo.owner_id` with capability from API (e.g. `can_admin` on `RepoPublic`) so org admins / collaborators with admin can open settings. Collaborators UI section belongs here (or sub-route); copy settings form + `AlertDialog` confirm patterns already in this file.

**Settings nav chrome:** `apps/web/src/components/settings/settings-nav.tsrx` — underline active link-row for org settings tabs (Overview | Members | …).

**Admin settings form density:** `apps/web/src/routes/admin/auth.tsrx` — Query cache updates, Switch/Select/Label, destructive confirm Dialog — good analog for org `member_base_permission` and invite management.

**Profile form:** `apps/web/src/routes/settings/profile.tsrx` — SSR loader kinds (`ready` / `error` / `unauthenticated`), `apiClient` save, reserved-username error mapping.

---

### `/orgs/new` (route)

**Analog:** `new.tsrx` (verified create form) + reserved slug handling from `signup.tsrx` (`auth.reserved_username` → user-facing copy).

Route path `/orgs/new` is free because `orgs` is reserved as a username. Prefer `createFileRoute("/orgs/new")` like `/new` / `/admin/auth`.

---

### Username live lookup (controller) — partial analog

**No prefix-search RPC exists.** Closest pieces:
- `find_user_by_username` exact match (`oxidean-db` / `auth/local.rs` signup uniqueness)
- Anti-enumeration habits from `verify_reset` / `acl::not_found`

**Planner discretion:** new `user.search` / `org.memberLookup` with prefix + rate limit; return minimal public fields only; do not leak whether private emails exist.

## Shared Patterns

### Authentication / verified gate
**Source:** `crates/oxidean-api/src/auth/gate.rs`  
**Apply to:** Org create, invite, member role changes, collaborator grants, repo create under org, mutate RPCs
```rust
pub async fn require_verified(ctx: &RpcCtx) -> Result<UserRow, AppError> { /* … */ }
```

### Web vs git ACL status mapping
**Source:** `acl.rs` + `git_smart_http.rs`  
**Apply to:** All ACL consumers
- Web/RPC/raw: unauthorized private → `repo.not_found`
- Smart HTTP: unauthorized private / push → `401` Basic
- SSH (Phase 9): git errors via shared ACL module (CONTEXT canonical ref)

### Error codes / AppError
**Source:** `repo/mod.rs`, `auth/local.rs`, `auth/profile.rs`  
**Apply to:** Org + collaborator RPCs
- Prefer stable `domain.snake_code` strings
- Map UNIQUE collisions to `*.taken`
- Reserved → `auth.reserved_username` (or parallel org code)

### Dialect SQL isolation
**Source:** `crates/oxidean-db/src/repositories.rs`, `email_tokens.rs`  
**Apply to:** All new org/member/invite/collaborator tables  
API calls `Database` methods only; triple migration files stay in sync.

### RPC + generated client
**Source:** `rpc.rs` + `make rpc-gen`  
**Apply to:** Every new procedure / DTO in `oxidean-core`  
Do not hand-edit `packages/api-client` as source of truth.

### Octane UI
**Source:** `.cursor/rules/octane-ui.mdc`, `new.tsrx`, settings routes  
**Apply to:** `/orgs/new`, org members, collaborators, `/new` picker  
`.tsrx` + `@if`/`@else`/`@for`; `onInput` for text; TanStack Query for server/session; local `useState` for forms.

### Email outbound
**Source:** `verify_reset.rs` + `crate::email::OutboundEmail`  
**Apply to:** Org email invites  
Hash tokens at rest; magic link via `OXIDEAN_PUBLIC_ORIGIN`; rate limits; soft-fail send logging where signup-style flows require it.

## No Analog Found

| File / concern | Role | Data Flow | Reason |
|----------------|------|-----------|--------|
| Username prefix autocomplete RPC | controller | request-response | No search/list-users-by-prefix API; only exact `find_user_by_username` |
| First-class org overview at `/{org}` (non-repo) | route | request-response | No bare `/$owner` profile route today — only `/$owner/$repo*`; new route needed without colliding with repo layout |
| Teams / group grants | — | — | Explicitly deferred (D-ORG-07) |

## Metadata

**Analog search scope:** `crates/oxidean-api/src/repo`, `routes/git_smart_http.rs`, `routes/repo_raw.rs`, `crates/oxidean-db/migrations`, `repositories.rs`, `email_tokens.rs`, `auth/verify_reset.rs`, `auth/gate.rs`, `auth/local.rs`, `auth/profile.rs`, `auth_types.rs`, `git/mod.rs`, `apps/web/src/routes/{new,$owner.$repo*,settings,admin}`, `components/settings`  
**Files scanned:** ~35 tracked sources (git `ls-files` gated)  
**Pattern extraction date:** 2026-09-14  
**RESEARCH.md:** not present at map time — CONTEXT + codebase only
