# Phase 7: Git Repos & Browse - Pattern Map

**Mapped:** 2026-09-12
**Files analyzed:** 28
**Analogs found:** 24 / 28

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/oxidean-git/src/backend.rs` | service | transform | `crates/oxidean-api/src/email/mod.rs` | role-match |
| `crates/oxidean-git/src/cli.rs` | service | file-I/O | `crates/oxidean-api/src/email/mod.rs` (+ RESEARCH CLI argv) | partial |
| `crates/oxidean-git/src/version.rs` | utility | request-response | `crates/oxidean-api/src/main.rs` | role-match |
| `crates/oxidean-git/src/lib.rs` | config | — | `crates/oxidean-core/src/lib.rs` | role-match |
| `Cargo.toml` (workspace member) | config | — | `Cargo.toml` | exact |
| `crates/oxidean-core` repo DTOs + `validate_repo_name` | model | transform | `crates/oxidean-core/src/auth_types.rs` | exact |
| `crates/oxidean-core` reserved `"new"` | model | transform | `crates/oxidean-core/src/auth_types.rs` | exact |
| `crates/oxidean-db/migrations/*/0007_repositories.sql` | migration | CRUD | `crates/oxidean-db/migrations/postgres/0002_auth.sql` | exact |
| settings columns (default_branch / default_visibility) | migration | CRUD | `crates/oxidean-db/migrations/postgres/0006_bootstrap_flags.sql` | exact |
| `crates/oxidean-db/src/repositories.rs` | model | CRUD | `crates/oxidean-db/src/users.rs` | exact |
| `crates/oxidean-db/src/lib.rs` factory reset + repo API | model | CRUD | `crates/oxidean-db/src/lib.rs` | exact |
| `crates/oxidean-api/src/git/` (path + ACL) | middleware | request-response | `crates/oxidean-api/src/routes/avatar.rs` + `auth/gate.rs` | role-match |
| `crates/oxidean-api` `repo.*` handlers | controller | request-response | `crates/oxidean-api/src/auth/profile.rs` + `gate.rs` | exact |
| `crates/oxidean-api/src/rpc.rs` | route | request-response | `crates/oxidean-api/src/rpc.rs` | exact |
| `crates/oxidean-api/src/routes/repo_raw.rs` | route | streaming | `crates/oxidean-api/src/routes/avatar.rs` | exact |
| `crates/oxidean-api/src/app.rs` (`repos_dir`) | config | request-response | `crates/oxidean-api/src/app.rs` | exact |
| `crates/oxidean-api/src/main.rs` (git boot gate) | config | request-response | `crates/oxidean-api/src/main.rs` | exact |
| `crates/oxidean-api/Dockerfile` | config | — | `crates/oxidean-api/Dockerfile` | exact |
| `docker-compose.yml` (`var/repos`) | config | — | `docker-compose.yml` | exact |
| `apps/web/src/components/signed-in-home.tsrx` | component | request-response | `apps/web/src/components/signed-in-home.tsrx` | exact |
| `apps/web/src/routes/new.tsrx` | component | request-response | `apps/web/src/routes/signup.tsrx` + `settings/profile.tsrx` | role-match |
| `apps/web/src/routes/$owner.$repo*.tsrx` | component | request-response | `apps/web/src/routes/signup.tsrx` (`notFound`) | partial |
| `apps/web/src/components/repo/*` | component | request-response | `apps/web/src/components/signed-in-home.tsrx` + UI wrappers | role-match |
| `apps/web/src/lib/markdown.ts` | utility | transform | — | none |
| `apps/web/src/lib/highlight.ts` | utility | transform | — | none |
| `apps/web/src/lib/session-queries.ts` | hook | request-response | `apps/web/src/lib/session-queries.ts` | exact |
| `apps/web/src/routes/admin/auth.tsrx` (reset scope) | component | request-response | `apps/web/src/routes/admin/auth.tsrx` | exact |
| `apps/web/src/routes/settings/profile.tsrx` (default branch) | component | CRUD | `apps/web/src/routes/settings/profile.tsrx` | exact |
| `crates/oxidean-api/tests/repo_*.rs` | test | request-response | `crates/oxidean-api/tests/auth_verify_gate.rs` + `support/mod.rs` | role-match |

## Pattern Assignments

### `crates/oxidean-git` — `GitBackend` trait + `CliGitBackend` (service, transform / file-I/O)

**Analog:** `crates/oxidean-api/src/email/mod.rs` (async trait + concrete adapters behind `Arc<dyn …>`)

**Imports / trait pattern** (lines 11–39):
```rust
use std::sync::Arc;
use async_trait::async_trait;
use thiserror::Error;

#[async_trait::async_trait]
pub trait EmailSender: Send + Sync {
    async fn send(&self, msg: OutboundEmail) -> Result<(), EmailError>;
}
```

**Copy for Phase 7:**
- Put `GitBackend` + `GitError` in `oxidean-git` (deep module — API never shells out).
- Ship only `CliGitBackend`; document future `GixGitBackend` in module docs (GIT-10 / D-32).
- Invoke git with `tokio::process::Command` argv arrays only (never `sh -c`) — see RESEARCH Code Examples.
- Register `Arc<dyn GitBackend>` on `AppState` the same way `Arc<dyn EmailSender>` is held today.

**Workspace wiring analog:** root `Cargo.toml` members + `oxidean-api` path deps:
```toml
members = [
  "crates/oxidean-api",
  "crates/oxidean-core",
  "crates/oxidean-db",
]
```
Add `"crates/oxidean-git"`; depend from `oxidean-api` like `oxidean-db`.

---

### `crates/oxidean-git/src/version.rs` + `main.rs` boot fail (utility / config)

**Analog:** `crates/oxidean-api/src/main.rs` fail-closed boot (lines 14–19, 27–36):

```rust
let cors = match build_cors(&env_name, cors_origins.as_deref()) {
    Ok(c) => c,
    Err(e) => {
        eprintln!("cors config error: {e}");
        std::process::exit(1);
    }
};
```

**Copy for Phase 7:** Call `assert_git_version((2, 5, 0))` before serve; `eprintln!` + `exit(1)` on missing/old git (D-33). Mirror CORS/DB fail style — no soft continue.

---

### `validate_repo_name` + reserved `"new"` (model, transform)

**Analog:** `crates/oxidean-core/src/auth_types.rs` (lines 200–258)

**Reserved list + validator:**
```rust
const RESERVED_USERNAMES: &[&str] = &[
    "admin", "api", "settings", "login", /* … */ "setup", "system-administrator",
];

pub fn validate_username(raw: &str) -> Result<(), String> {
    let u = raw.trim();
    // length, hyphen rules, alphanumeric+hyphen, reserved
    …
}
```

**Copy for Phase 7:**
- **Extend** `RESERVED_USERNAMES` with `"new"` and any other flat routes from UI-SPEC (do not remove existing).
- Add **separate** `validate_repo_name` allowing `_` and `.` (D-06) — do **not** reuse `validate_username`.
- Add repo DTOs (`CreateRepoRequest`, etc.) beside auth types (same serde + `AppError` codes pattern as `FactoryResetRequest` lines 109–121).

---

### `0007_repositories.sql` + settings columns (migration, CRUD)

**Analog (new table):** `crates/oxidean-db/migrations/postgres/0002_auth.sql` (lines 1–14) — `CREATE TABLE` + indexes + FK to `users`.

**Analog (ALTER columns):** `crates/oxidean-db/migrations/postgres/0006_bootstrap_flags.sql`:
```sql
-- logical: 0006_bootstrap_flags — allow_signup + must_change_credentials
ALTER TABLE instance_auth_settings ADD COLUMN allow_signup BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE users ADD COLUMN must_change_credentials BOOLEAN NOT NULL DEFAULT FALSE;
```

**Copy for Phase 7:**
- Tri-dialect parity under `migrations/{postgres,mysql,sqlite}/0007_*.sql`.
- `repositories` table: owner_id FK, name, visibility, description, soft-delete timestamp, updated_at; unique (owner, name) among non-deleted.
- Extra columns: user default branch; instance default visibility (D-08/D-09) via ALTER like 0006.

---

### `crates/oxidean-db/src/repositories.rs` (model, CRUD)

**Analog:** `crates/oxidean-db/src/users.rs` (lines 1–22, 92+)

```rust
//! User CRUD via `DbPool` match — dialect branching stays in this crate.

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct UserRow { /* … */ }

pub async fn insert_user(pool: &DbPool, /* … */) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => { /* sqlx */ }
        DbPool::MySql(p) => { /* … */ }
        DbPool::Sqlite(p) => { /* … */ }
    }
}
```

**Copy for Phase 7:** `RepositoryRow` + dialect `match` helpers; expose thin methods on `Database` in `lib.rs` (same facade as users/auth_settings). Extend `factory_reset_instance` (lines 370–404) to optionally wipe repo rows / leave disk to API layer per D-34 scope.

---

### `repo.*` RPC + `require_verified` (controller, request-response)

**Analog — gate:** `crates/oxidean-api/src/auth/gate.rs` (lines 20–42):
```rust
pub async fn require_verified(ctx: &RpcCtx) -> Result<UserRow, AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new("auth.unauthenticated", "not authenticated"));
    };
    // …
    if user.email_verified_at.is_none() {
        return Err(AppError::new(
            "auth.email_unverified",
            "verify your email to continue",
        ));
    }
    Ok(user)
}
```

**Analog — dispatch:** `crates/oxidean-api/src/rpc.rs` (lines 115–118, 188–207):
```rust
"user.update_profile" => match profile::update_profile(ctx, req.input).await {
    Ok(user) => RpcResponse::ok(user),
    Err(e) => RpcResponse::err(e),
},
"admin.instance.factory_reset" => match admin::factory_reset(ctx, req.input).await {
    Ok(v) => RpcResponse::ok(v),
    Err(e) => RpcResponse::err(e),
},
```

**Analog — confirmed destructive RPC:** `crates/oxidean-api/src/auth/admin.rs` (lines 182–208) — parse input, confirm phrase, call DB, structured response.

**Copy for Phase 7:**
- New module e.g. `auth/repo.rs` or `repo/mod.rs` with handlers; `repo.create` starts with `require_verified`.
- Stable codes: `auth.email_unverified`, `repo.not_found` (unified for private/missing — D-25), duplicate-name code for inline field (D-12).
- After procedures change: `make rpc-gen` (rpc-codegen rule).
- Keep bootstrap lock list behavior in `dispatch` — repo.* blocked while `needs_setup`.

---

### `routes/repo_raw.rs` — archive / raw HTTP (route, streaming)

**Analog:** `crates/oxidean-api/src/routes/avatar.rs`

**Path safety** (lines 273–306):
```rust
fn is_safe_avatar_basename(name: &str) -> bool {
    if name.is_empty() || name.contains("..") || name.contains('/') || name.contains('\\') {
        return false;
    }
    // …
}

// Extra guard: resolved path must stay under avatars dir.
let Ok(avatars_canon) = tokio::fs::canonicalize(state.uploads_dir.join("avatars")).await else {
    return StatusCode::NOT_FOUND.into_response();
};
if !file_canon.starts_with(&avatars_canon) {
    return StatusCode::NOT_FOUND.into_response();
}
```

**JSON error envelope** (lines 36–44):
```rust
fn err_response(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(serde_json::json!({
        "ok": false,
        "error": { "code": code, "message": message }
    }))).into_response()
}
```

**Mount pattern:** `crates/oxidean-api/src/app.rs` (lines 89–93) + `routes/mod.rs`:
```rust
.route("/uploads/avatars/{file}", get(avatar::serve_avatar))
```

**Copy for Phase 7:**
- `GET /api/repos/{owner}/{repo}/archive/{ref}.{zip|tar.gz}` and raw blob GET after ACL (same not-found for private).
- Basename/ref validation + canonicalize under `repos_dir`; stream `git archive` stdout (not RPC JSON).
- Register in `routes/mod.rs`; Traefik already covers `/api`.

---

### `AppState.repos_dir` + Compose volume (config)

**Analog — state:** `crates/oxidean-api/src/app.rs` (lines 26–57):
```rust
pub struct AppState {
    pub uploads_dir: PathBuf,
    // …
}
uploads_dir: PathBuf::from("var/uploads"),
pub fn with_uploads_dir(mut self, dir: PathBuf) -> Self {
    self.uploads_dir = dir;
    self
}
```

**Analog — Compose:** `docker-compose.yml` (lines 51–53):
```yaml
volumes:
  # Avatar uploads (D-18) — api CWD is `/`, so bind to /var/uploads
  - ./var/uploads:/var/uploads
```

**Analog — Dockerfile:** `crates/oxidean-api/Dockerfile` (lines 7–10) — add `git` to `apt-get install`.

**Copy for Phase 7:**
- Default `var/repos`; `with_repos_dir`; env `OXIDEAN_REPOS_DIR`.
- Bind `./var/repos:/var/repos`; bare path `{owner}/{name}.git` (D-30).
- Pass `repos_dir` into `RpcCtx` like `uploads_dir` (rpc.rs line 35).

---

### `signed-in-home.tsrx` dashboard (component, request-response)

**Analog (modify in place):** `apps/web/src/components/signed-in-home.tsrx` (lines 36–57)

```tsx
<button
  type="button"
  disabled
  aria-disabled="true"
  title={
    user.email_verified
      ? "Repository creation arrives in a later phase."
      : "Verify your email to create a repository."
  }
  className={cn(buttonVariants({ variant: "default" }))}
>
  New repository
</button>
```

**Copy for Phase 7:** Enable CTA when verified → navigate `/new`; keep disabled + verify hint when unverified (D-11). Replace stub body with repo list / empty hero / activity placeholder per UI-SPEC. Title `Repositories · Oxidean` via `index.tsrx` head when signed-in.

**Home tree analog:** `apps/web/src/routes/index.tsrx` `selectHomeTree` + `SignedInHome` render path.

---

### `routes/new.tsrx` create form (component, request-response)

**Analog — SSR gates / notFound:** `apps/web/src/routes/signup.tsrx` (lines 29–50):
```tsx
export const Route = createFileRoute("/signup")({
  beforeLoad: async () => {
    // closed signup is SSR 404
    if (cfg.ok && cfg.data.allow_signup === false) {
      throw notFound();
    }
  },
  head: () => ({ meta: [{ title: "Sign up · Oxidean" }] }),
});
```

**Analog — form local state + RPC:** `apps/web/src/routes/settings/profile.tsrx` (lines 32–99) — `useState`, `onInput`, inline `formError`, pending CTA, `apiClient.*`.

**Analog — verify wall shell:** `apps/web/src/components/auth-shell.tsrx` (lines 4–31) — title/support/children column.

**Copy for Phase 7:**
- Unverified: AuthShell-width wall → `/verify` (D-11); do not show create form.
- Verified: max-w-2xl form; duplicate name → inline field error from stable code (D-12).
- `method="post" action="#"` + `type="button"` submit per octane-ui rule.
- Selects for stack / SPDX / gitignore; visibility radios (add Radio Group per UI-SPEC).

---

### `/{owner}/{repo}*` browse routes (component, request-response)

**Analog — anti-enumeration 404:** `apps/web/src/routes/signup.tsrx` `throw notFound()` when access denied.

**Analog — Query options:** `apps/web/src/lib/session-queries.ts` (lines 18–30) — `queryOptions` + `apiClient` + error code branching.

**Copy for Phase 7:**
- Explicit file routes per UI-SPEC paths (`tree`/`blob`/`commits`/…).
- Loader/beforeLoad: private/unauthorized → identical `notFound()` (D-25).
- Add `repoListQueryOptions` / `repoTreeQueryOptions` etc. beside session helpers.
- No React `return (` mixing; Rivet `@if`/`@for` only.

**No close in-repo analog for tree/blob/diff chrome** — follow `07-UI-SPEC.md` + shadcn wrappers (`button`, `select`, `dropdown-menu`, `skeleton`).

---

### Factory reset scope radios (component + RPC)

**UI analog:** `apps/web/src/routes/admin/auth.tsrx` `FactoryResetSection` (lines 407–467):
```tsx
const res = await apiClient.admin.auth.factoryReset({
  confirmation: phrase.trim(),
});
// phrase must equal RESET; redirect /setup
```

**RPC analog:** `admin::factory_reset` + `FactoryResetRequest` (`confirmation` only today).

**Copy for Phase 7:** Extend request with scope enum (`database_only` | `database_and_repos`); Dialog + Radio Group (new shadcn primitives); keep RESET phrase; when both selected, API deletes under `repos_dir` after DB wipe.

---

### Account default branch field (component, CRUD)

**Analog:** `apps/web/src/routes/settings/profile.tsrx` save pattern (`saveProfile`, Label/Input, pending, inline errors).

**Copy for Phase 7:** New section “Default branch name” + `Save default branch`; RPC on user/settings (planner picks procedure name); default `main` (D-09).

---

### `lib/markdown.ts` / `lib/highlight.ts` (utility, transform)

**No in-repo analog.** Use RESEARCH Code Examples (`unified` + `rehype-sanitize`; Shiki `createHighlighter` + custom langs). Keep as pure `apps/web/src/lib/*` helpers consumed by blob/README components.

---

### Integration tests (test, request-response)

**Analog:** `crates/oxidean-api/tests/auth_verify_gate.rs` (require_verified) + `tests/support/mod.rs` (temp DB helpers) + `tests/profile_avatar.rs` (`with_uploads_dir`).

**Copy for Phase 7:** `with_repos_dir(temp)`; seed verified user; assert `repo.create` / private 404 / branch soft-protect / archive bytes. Git crate: unit tests for version parse + `ls_tree` against temp bare repo.

## Shared Patterns

### Authentication / verification
**Source:** `crates/oxidean-api/src/auth/gate.rs`
**Apply to:** `repo.create` and any verified-only mutations
```rust
let user = gate::require_verified(ctx).await?;
```

### Anti-enumeration ACL
**Source:** CONTEXT D-25 + signup `notFound()` pattern
**Apply to:** All repo read RPC + archive/raw HTTP + browse loaders
- Missing **or** private non-owner → identical `repo.not_found` / HTTP 404 / `throw notFound()`
- Never emit `repo.forbidden` or “private repository” copy

### Filesystem path safety
**Source:** `crates/oxidean-api/src/routes/avatar.rs` (basename + canonicalize)
**Apply to:** `repos_dir/{owner}/{name}.git`, archive/raw path params
- Reject `..`, absolute escapes; resolve under configured root

### RPC dispatch + codegen
**Source:** `crates/oxidean-api/src/rpc.rs` + `make rpc-gen`
**Apply to:** All new `repo.*` / extended `admin.instance.factory_reset`
- Match arm → handler → `RpcResponse::ok/err`
- Stable error codes for UI inline fields

### Volume-backed storage
**Source:** `AppState.uploads_dir` + Compose `./var/uploads:/var/uploads`
**Apply to:** `OXIDEAN_REPOS_DIR` default `var/repos`

### Swappable backend trait
**Source:** `crates/oxidean-api/src/email/mod.rs` `EmailSender`
**Apply to:** `GitBackend` in `oxidean-git`

### Octane forms / Query
**Source:** `settings/profile.tsrx`, `session-queries.ts`, octane-ui rule
**Apply to:** `/new`, settings, branch forms, browse Query caches
- `onInput` for text; Query for server domain; local state for forms

### Destructive confirm
**Source:** `admin::factory_reset` + `FactoryResetSection`
**Apply to:** Factory reset scope, delete branch, soft-delete repo
- Explicit confirmation string; disabled CTA until match

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `apps/web/src/lib/markdown.ts` | utility | transform | No unified/remark pipeline in repo yet — use RESEARCH snippet |
| `apps/web/src/lib/highlight.ts` | utility | transform | No Shiki/highlighter yet — use RESEARCH + TextMate grammars |
| `apps/web/src/components/ui/dialog` (+ radio-group, badge, …) | component | request-response | Not in `components/ui/` yet — add official shadcn Base UI per UI-SPEC |
| Full tree/blob/diff chrome | component | request-response | First forge browse surface — copy UI-SPEC IA; reuse Button/Select/Skeleton/Dropdown only |

## Metadata

**Analog search scope:** `crates/oxidean-{api,core,db}`, `apps/web/src/{routes,components,lib}`, root `Cargo.toml`, `docker-compose.yml`, `Dockerfile`
**Files scanned:** ~90 tracked source paths (glob + grep)
**Tracked-source gate:** All named analogs verified via `git ls-files`
**Pattern extraction date:** 2026-09-12
