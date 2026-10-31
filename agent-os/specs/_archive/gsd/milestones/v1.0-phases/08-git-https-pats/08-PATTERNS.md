# Phase 8: Git HTTPS & PATs - Pattern Map

**Mapped:** 2026-09-13
**Files analyzed:** 22
**Analogs found:** 20 / 22

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0008_pats.sql` | migration | CRUD | `crates/oxidean-db/migrations/postgres/0002_auth.sql` (+ `0003_email_tokens.sql`) | exact |
| `crates/oxidean-db/src/pats.rs` | model | CRUD | `crates/oxidean-db/src/sessions.rs` | exact |
| `crates/oxidean-db/src/lib.rs` | model | CRUD | same file — `create_session` / `insert_repository` facade | exact |
| `crates/oxidean-core/src/pat_types.rs` | model | transform | `crates/oxidean-core/src/repo_types.rs` | exact |
| `crates/oxidean-core/src/lib.rs` | config | transform | same file — `pub mod` + re-exports | exact |
| `crates/oxidean-core/src/auth_types.rs` | utility | transform | same file — `RESERVED_USERNAMES` / `is_reserved_username` | exact |
| `crates/oxidean-api/src/pat/mod.rs` | controller | request-response | `crates/oxidean-api/src/repo/mod.rs` (`create` + `require_verified`) | exact |
| `crates/oxidean-api/src/rpc.rs` | controller | request-response | same file — `repo.*` match arms | exact |
| `crates/oxidean-api/src/routes/git_smart_http.rs` | route | request-response / streaming | `crates/oxidean-api/src/routes/repo_raw.rs` | role-match |
| `crates/oxidean-api/src/git/http_backend.rs` | service | streaming / file-I/O | `crates/oxidean-git/src/cli.rs` (`Command` + stdio) | role-match |
| `crates/oxidean-api/src/git/mod.rs` | utility | file-I/O | same file — `bare_repo_path` | exact |
| `crates/oxidean-api/src/routes/mod.rs` | config | request-response | same file — `pub mod repo_raw` | exact |
| `crates/oxidean-api/src/app.rs` | config | request-response | same file — `.route(...)` mounting | exact |
| `crates/oxidean-api/src/repo/acl.rs` | middleware | request-response | same file — share ACL *decision*; split *status mapping* for git | exact |
| `crates/oxidean-api/src/auth/session.rs` | utility | request-response | same file — CSPRNG + SHA-256 hex (PAT mint/lookup) | exact |
| `crates/oxidean-api/src/auth/gate.rs` | middleware | request-response | same file — `require_verified` for `pat.create*` | exact |
| `crates/oxidean-api/tests/pat_rpc.rs` | test | request-response | `crates/oxidean-api/tests/repo_create.rs` + `auth_verify_gate.rs` | exact |
| `crates/oxidean-api/tests/git_smart_http.rs` | test | request-response | `crates/oxidean-api/tests/repo_private_404.rs` + `repo_create.rs` | role-match |
| `apps/web/src/routes/settings/tokens.tsrx` | route | request-response | `apps/web/src/routes/settings/profile.tsrx` | exact |
| `apps/web/src/components/settings/pat-*.tsrx` | component | request-response | `apps/web/src/routes/new.tsrx` (create + verify wall) + `$owner.$repo.settings.tsrx` (AlertDialog) | role-match |
| `apps/web/src/components/repo/clone-box.tsrx` | component | request-response | same file — extend how-to panel (D-13) | exact |
| `apps/web/src/components/chrome.tsrx` | component | request-response | same file — `/settings/profile` nav links | exact |
| `docker-compose.yml` | config | request-response | same file — Traefik API/web priority labels | exact |
| `docs/API.md` / `docs/CONFIGURATION.md` | config | transform | same files — HTTP/RPC tables + env tables | exact |
| In-memory failed-auth rate limiter (new helper under `pat/` or `routes/`) | utility | request-response | `crates/oxidean-api/src/auth/verify_reset.rs` (`MAX_REDEEM_ATTEMPTS`) | partial |
| Basic auth parse + WWW-Authenticate responses | utility | request-response | — | none |

## Pattern Assignments

### `crates/oxidean-db/migrations/*/0008_pats.sql` (migration, CRUD)

**Analog:** `crates/oxidean-db/migrations/postgres/0002_auth.sql` (sessions hash-at-rest) + `0003_email_tokens.sql` (token_hash CHAR(64)) + `0007_repositories.sql` (FK + join-style indexes)

**Schema pattern** (sessions `token_hash` UNIQUE CHAR(64) — lines 16–24 of `0002_auth.sql`):
```sql
CREATE TABLE IF NOT EXISTS sessions (
  id           TEXT        PRIMARY KEY,
  user_id      TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash   CHAR(64)    NOT NULL UNIQUE,
  expires_at   TIMESTAMPTZ NOT NULL,
  remember_me  BOOLEAN     NOT NULL DEFAULT FALSE,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

**Copy for PATs:**
- `token_hash CHAR(64) NOT NULL UNIQUE` (SHA-256 hex of full opaque string including prefix)
- `token_prefix` TEXT NOT NULL (e.g. `ona_pat_` / `ona_fg_` + short preview — list UI only)
- Soft revoke via `revoked_at TIMESTAMPTZ NULL` (prefer over hard delete for audit)
- FG join table `personal_access_token_repos (token_id, repository_id)` with `ON DELETE CASCADE`
- Tri-dialect parity: identical filenames under `postgres/`, `mysql/`, `sqlite/` — enforced by `migrate.rs` `migration_parity`

**Parity test pattern** (`crates/oxidean-db/src/migrate.rs` lines 51–60):
```rust
#[test]
fn migration_parity() {
    let postgres = migration_files("postgres");
    let mysql = migration_files("mysql");
    let sqlite = migration_files("sqlite");
    assert_eq!(postgres, mysql, "postgres and mysql migration sets diverged");
    assert_eq!(postgres, sqlite, "postgres and sqlite migration sets diverged");
}
```

---

### `crates/oxidean-db/src/pats.rs` (model, CRUD)

**Analog:** `crates/oxidean-db/src/sessions.rs`

**Imports / dialect match pattern** (lines 1–6, 69–91):
```rust
//! Session CRUD via `DbPool` match — opaque token hashes only (D-11, D-13).

use sqlx::Row;
use crate::pool::DbPool;

pub async fn create(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    token_hash: &str,
    expires_at: &str,
    remember_me: bool,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => { /* $1..$n binds */ }
        DbPool::MySql(p) => { /* ? binds */ }
        DbPool::Sqlite(p) => { /* ?1 binds; bool as 0/1 */ }
    }
}
```

**Core pattern to copy:**
- Row struct + `map_*` macro for bool dialect quirks
- Separate `*_SELECT_PG|MYSQL|SQLITE` with `to_char` / `DATE_FORMAT` / `strftime` for RFC3339
- Lookup by `token_hash`; list by `user_id` where `revoked_at IS NULL`
- Touch `last_used_at` / `last_used_ip` on successful Smart HTTP auth (mirror `touch_session`)

**Facade wiring** (`crates/oxidean-db/src/lib.rs` lines 343–360):
```rust
pub async fn create_session(
    &self,
    id: &str,
    user_id: &str,
    token_hash: &str,
    expires_at: &str,
    remember_me: bool,
) -> Result<(), String> {
    sessions::create(self.require_pool()?, id, user_id, token_hash, expires_at, remember_me).await
}
```
Add `pub mod pats;` + thin `Database::create_pat` / `find_pat_by_token_hash` / `list_pats_for_user` / `revoke_pat` / `touch_pat_last_used` the same way.

---

### `crates/oxidean-core/src/pat_types.rs` (model, transform)

**Analog:** `crates/oxidean-core/src/repo_types.rs`

**Enum + serde pattern** (lines 6–29):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepoVisibility {
    Public,
    Private,
}

impl RepoVisibility {
    pub const fn as_str(self) -> &'static str { /* ... */ }
    pub fn parse(s: &str) -> Result<Self, String> { /* ... */ }
}
```

**Copy for PATs:**
- `PatKind { Classic, FineGrained }` with `rename_all = "snake_case"` or locked string tags `classic` | `fine_grained`
- Classic scopes: `repo` only (Phase 8)
- FG: `repo_access: Selected | All`, `contents: Read | Write`
- Request/response DTOs: `CreateClassicPatRequest`, `CreateFineGrainedPatRequest`, `PatListItem` (no secret), `CreatePatResponse { token, item }`
- Re-export from `crates/oxidean-core/src/lib.rs` like `repo_types`

**Reserved usernames** (D-10 aliases) — extend `auth_types.rs` `RESERVED_USERNAMES` (lines 234–258) to include `git`, `token`, `oauth2` if missing (Open Question A2 from RESEARCH).

---

### `crates/oxidean-api/src/pat/mod.rs` (controller, request-response)

**Analog:** `crates/oxidean-api/src/repo/mod.rs` + minting from `auth/session.rs`

**Auth gate** (`gate.rs` lines 20–42):
```rust
/// Unauthenticated → `auth.unauthenticated`; unverified → `auth.email_unverified`.
pub async fn require_verified(ctx: &RpcCtx) -> Result<UserRow, AppError> { /* ... */ }
```

**Create RPC pattern** (`repo/mod.rs` lines 664–670):
```rust
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: CreateRepoRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.create input: {e}"))
    })?;
    // ...
}
```

**Token mint / hash-at-rest** (`session.rs` lines 76–107, 221–223):
```rust
/// Mint a new session: CSPRNG token → cookie; SHA-256 hex → DB.
let mut token_bytes = [0u8; TOKEN_BYTES];
rand::fill(&mut token_bytes);
let raw_token = bytes_to_hex(&token_bytes);
let token_hash = sha256_hex(raw_token.as_bytes());
// store token_hash only; return raw_token once
```

**PAT-specific mint (planner):**
1. `require_verified` on `createClassic` / `createFineGrained` only (D-24)
2. Require non-empty `note`/`name` → `pat.note_required` (D-16)
3. Prefix: `ona_pat_` + hex (classic) / `ona_fg_` + hex (FG) — D-08
4. Response includes plaintext `token` **once**; `list` returns prefix + metadata + `last_used_*` only (D-09, D-15)
5. `list` / `revoke`: session required (not necessarily verified — align with RESEARCH table)
6. Error codes: reuse `auth.*`; add `pat.not_found`, `pat.invalid_scope`, `pat.note_required`

**RPC dispatch** (`rpc.rs` lines 224–227 pattern):
```rust
"repo.create" => match repo::create(ctx, req.input).await {
    Ok(repo) => RpcResponse::ok(repo),
    Err(e) => RpcResponse::err(e),
},
```
Add `pat.createClassic`, `pat.createFineGrained`, `pat.list`, `pat.revoke` the same way; then `make rpc-gen`.

---

### `crates/oxidean-api/src/routes/git_smart_http.rs` (route, streaming)

**Analog:** `crates/oxidean-api/src/routes/repo_raw.rs` (non-RPC HTTP + path validation + AppState)

**Critical divergence from analog:** `repo_raw` resolves **session cookies** and maps ACL failures to JSON `repo.not_found`. Smart HTTP must **ignore cookies** (D-12), use Basic + PAT, and map private unauth → **401 + WWW-Authenticate** (D-21).

**Route mounting pattern** (`app.rs` lines 104–131):
```rust
Router::new()
    .route("/health", get(health))
    .route("/api/rpc", post(rpc_http))
    // ...
    .route(
        "/api/repos/{owner}/{repo}/raw/{ref}/{*path}",
        get(repo_raw::serve_raw),
    )
```
Add Smart HTTP routes on `/{owner}/{repo}.git/...` (Axum path segments that include `.git` in repo param, or a dedicated matcher). Register in `routes/mod.rs` as `pub mod git_smart_http`.

**Path safety** — reuse `bare_repo_path` (`git/mod.rs` lines 10–28):
```rust
/// Bare repo path: `{repos_dir}/{owner}/{name}.git` (D-30).
pub fn bare_repo_path(repos_dir: &Path, owner: &str, name: &str) -> Result<PathBuf, AppError> {
    // reject empty, `/`, `\`, `..`
    Ok(repos_dir.join(owner).join(format!("{name}.git")))
}
```

**ACL decision reuse** (`acl.rs` lines 19–65) — extract shared “can read / is owner / is private” logic; **do not** return `not_found()` for git:
```rust
/// Identical error for missing repos and unauthorized private access (anti-enumeration).
pub fn not_found() -> AppError {
    AppError::new("repo.not_found", "Repository not found")
}
// Web: private non-owner → not_found (404-style)
// Git: private unauth → 401 + WWW-Authenticate; insufficient scope → 403 (D-23)
```

**Status contract (planner):**
| Case | Status |
|------|--------|
| Public anon upload-pack | 200 → CGI |
| Private / no-access unauth | 401 + `WWW-Authenticate: Basic realm="Oxidean Git"` |
| Password / non-PAT secret | 401 + PAT hint (D-11) |
| Valid PAT, insufficient scope | 403 |
| receive-pack always | PAT + verified email |
| Failed-auth over limit | 429 + `Retry-After` |

**Username aliases (D-10):** accept account username (case-insensitive), `git`, `token`, `oauth2`; identity from PAT password field only.

---

### `crates/oxidean-api/src/git/http_backend.rs` (service, streaming)

**Analog:** `crates/oxidean-git/src/cli.rs` — `tokio::process::Command`, never `sh -c`

**Spawn pattern** (lines 29–37; stdin pipe example 1044–1058):
```rust
let output = Command::new("git")
    .args(args)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()
    .await
    .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;

// Streaming / piped stdin:
let mut child = Command::new("git")
    .args([/* ... */])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;
```

**CGI env (from RESEARCH — no in-tree CGI yet):**
- Binary: `/usr/lib/git-core/git-http-backend` (or resolve via `git --exec-path`)
- `GIT_PROJECT_ROOT=<OXIDEAN_REPOS_DIR>`
- `PATH_INFO=/{owner}/{repo}.git/...`
- `GIT_HTTP_EXPORT_ALL=1` (Phase 7 bare repos lack `git-daemon-export-ok`)
- Forward `Git-Protocol` → `GIT_PROTOCOL`
- Set `REMOTE_USER` when authenticated (enables receive-pack)
- Smart-only: `info/refs`, `git-upload-pack`, `git-receive-pack` — **no** dumb `/objects/`

---

### `crates/oxidean-api/tests/pat_rpc.rs` (test, request-response)

**Analog:** `crates/oxidean-api/tests/repo_create.rs` + `auth_verify_gate.rs`

**Harness pattern** (`repo_create.rs` lines 15–41, 76–94):
```rust
async fn test_app(db: Database, repos_dir: PathBuf) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir);
    router_with_state(state, build_cors("development", None).expect("cors"))
}

fn rpc_req_with_cookie(body: &str, cookie: &str) -> Request<Body> { /* Oxidean-RPC-Version: 1 */ }

// Mark verified so require_verified passes:
db.set_email_verified_at(&user_id, &now).await.expect("verify");
```

**Cases to mirror:**
- Verified create classic/FG → one-time `token` in response; list omits secret
- Unverified create → `auth.email_unverified` (see `auth_verify_gate.rs`)
- Revoke → subsequent list hides / auth fails
- Optional expiry / note required validation

---

### `crates/oxidean-api/tests/git_smart_http.rs` (test, request-response)

**Analog:** `crates/oxidean-api/tests/repo_private_404.rs` (ACL matrix) + `repo_create.rs` (bare repo + `git` subprocess)

**Copy:** `test_app` + create public/private repos; hit `GET /{owner}/{repo}.git/info/refs?service=git-upload-pack` via `oneshot` **without** cookie; assert 401 headers for private; assert cookie alone does not authenticate; assert Basic with password → PAT hint; assert 403 on read-only FG push; assert 429 after N failures.

Use `std::process::Command::new("git")` for push/ls-remote against a hyper listener when CGI path is ready (pattern already in `repo_create.rs` ~251+).

---

### `apps/web/src/routes/settings/tokens.tsrx` (route, request-response)

**Analog:** `apps/web/src/routes/settings/profile.tsrx`

**Route + loader + auth redirect** (lines 31–68):
```typescript
export const Route = createFileRoute("/settings/profile")({
  component: ProfilePage,
  head: () => ({ meta: [{ title: "Profile · Oxidean" }] }),
  loader: async (): Promise<ProfileLoaderData> => { /* ... */ },
});

useEffect(() => {
  if (loaderData?.kind === "unauthenticated") {
    window.location.assign("/login?returnTo=/settings/profile");
  }
}, [loaderData]);
```

**Octane authoring:** Rivet `@{ }` only; `@if`/`@else` (no `@else if`); forms `method="post" action="#"`; text `onInput`. Dual create flows as separate components/routes under settings (D-05), not one wizard.

**Verify wall** — copy from `apps/web/src/routes/new.tsrx` lines 118–146:
```typescript
} else if (code === "auth.email_unverified") {
  setFormError("Verify your email before creating a repository.");
}
// showWall → AuthShell + link href="/verify"
```

**Revoke confirm** — copy AlertDialog from `apps/web/src/routes/$owner.$repo.settings.tsrx` lines 219–269 (simpler confirm OK for tokens — name typing optional; D-17 only requires confirm dialog).

**Nav:** add `/settings/tokens` sibling links next to `/settings/profile` in `chrome.tsrx` / signed-in home (same href pattern as profile).

---

### `apps/web/src/components/repo/clone-box.tsrx` (component, request-response)

**Analog:** same file — extend existing HTTPS clone UI

**Existing URL helper** (lines 34–38 + `public-origin.ts`):
```typescript
const httpsUrl = httpsCloneUrl(
  publicOriginProp || storeOrigin,
  owner,
  repo,
);
```
Origin must stay `OXIDEAN_PUBLIC_ORIGIN` / store (D-19) — do not switch to `window.location.host`.

**Extend (D-13):** full how-to panel — username aliases, password=PAT, CTA to `/settings/tokens`, example `git clone` / credential prompt. Keep SSH as placeholder (Phase 9).

---

### `docker-compose.yml` (config)

**Analog:** same file lines 67–102

**Current API rule (priority 100):**
```yaml
- traefik.http.routers.api.rule=Host(`localhost`) && (PathPrefix(`/api`) || PathPrefix(`/uploads`) || Path(`/health`))
- traefik.http.routers.api.priority=100
```

**Add** API router (or widen rule) with **priority 110**:
```
Host(`localhost`) && PathRegexp(`^/[^/]+/[^/]+\.git`)
```
Same service port `8080`. Web remains priority `1` Host catch-all — without this, SPA steals `.git` (Pitfall 1).

---

### Docs (`docs/API.md`, `docs/CONFIGURATION.md`)

**Analog:** existing tables in those files.

- `API.md`: document Smart HTTP paths, Basic+PAT (not session), status codes, `pat.*` RPC procedures; explicitly state PATs are **not** RPC Bearer (D-01).
- `CONFIGURATION.md`: note clone URL uses `OXIDEAN_PUBLIC_ORIGIN`; Traefik `.git` routing; no new env required for CGI beyond existing `OXIDEAN_REPOS_DIR`.

## Shared Patterns

### Opaque secret hash-at-rest (sessions → PATs)
**Source:** `crates/oxidean-api/src/auth/session.rs` lines 76–107, 221–232  
**Apply to:** PAT create + Smart HTTP lookup  
```rust
let mut token_bytes = [0u8; TOKEN_BYTES];
rand::fill(&mut token_bytes);
let raw_token = bytes_to_hex(&token_bytes);
let token_hash = sha256_hex(raw_token.as_bytes());
// Persist token_hash only; return raw (with ona_pat_/ona_fg_ prefix) once.
```
Do **not** use Argon2 for PAT verify (latency on every git request). Do **not** call `verify_password` as a success path for git (D-11).

### `require_verified` for privileged writes
**Source:** `crates/oxidean-api/src/auth/gate.rs` lines 20–42  
**Apply to:** `pat.createClassic`, `pat.createFineGrained`, HTTPS receive-pack (email_verified_at check)  
```rust
if user.email_verified_at.is_none() {
    return Err(AppError::new(
        "auth.email_unverified",
        "verify your email to continue",
    ));
}
```

### Dialect SQL only in `oxidean-db`
**Source:** `crates/oxidean-db/src/sessions.rs` / `lib.rs` facade  
**Apply to:** all PAT persistence — API calls `Database` methods only.

### RPC registration + codegen
**Source:** `crates/oxidean-api/src/rpc.rs` match arms; rule `rpc-codegen.mdc`  
**Apply to:** all `pat.*` procedures → change Rust → `make rpc-gen` → `make rpc-sync-check`. Never hand-edit `@oxidean/api-client` as source of truth.

### Web vs git ACL response split
**Source:** `crates/oxidean-api/src/repo/acl.rs`  
**Apply to:** Smart HTTP handlers  
Share ownership/visibility decision; map to 401/403 for git, keep `repo.not_found` for web/RPC browse.

### Octane UI + AlertDialog + verify wall
**Sources:** `.agents/skills/octane/SKILL.md`; `settings/profile.tsrx`; `new.tsrx`; `$owner.$repo.settings.tsrx`  
**Apply to:** `/settings/tokens`, clone how-to, revoke confirm.

### Rate-limit spirit (partial)
**Source:** `crates/oxidean-api/src/auth/verify_reset.rs` `MAX_REDEEM_ATTEMPTS = 10`  
**Apply to:** failed Basic/PAT — RESEARCH defaults 20/IP and 10/user per 15m → 429 + Retry-After. Prefer **in-memory** counters for Phase 8 (single API replica); no new crate.

## No Analog Found

| File / Concern | Role | Data Flow | Reason |
|----------------|------|-----------|--------|
| HTTP Basic auth parse + `WWW-Authenticate` challenge helpers | utility | request-response | No Basic auth in tree today; implement from RESEARCH + git-scm http-protocol |
| Axum CGI `git-http-backend` body streaming end-to-end | service | streaming | Closest is `CliGitBackend` process spawn; full CGI env/header bridging is new |

Planner should use RESEARCH.md Smart HTTP / Traefik / scope catalog sections for these two gaps.

## Metadata

**Analog search scope:** `crates/oxidean-{api,db,core,git}/`, `apps/web/src/{routes,components,lib}/`, `docker-compose.yml`, `docs/API.md`, `docs/CONFIGURATION.md`  
**Files scanned:** ~80 tracked candidates; 3–5 strong analogs per role cluster  
**Tracked-source gate:** all named analogs verified via `git ls-files`  
**Pattern extraction date:** 2026-09-13
