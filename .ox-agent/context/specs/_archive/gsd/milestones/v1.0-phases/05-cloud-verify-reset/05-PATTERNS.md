# Phase 5: Cloud Verify & Reset - Pattern Map

**Mapped:** 2026-09-10
**Files analyzed:** 28
**Analogs found:** 27 / 28

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0003_email_tokens.sql` | migration | CRUD | `crates/oxidean-db/migrations/sqlite/0002_auth.sql` (sessions table) | exact |
| `crates/oxidean-db/src/email_tokens.rs` | model | CRUD | `crates/oxidean-db/src/sessions.rs` | exact |
| `crates/oxidean-db/src/lib.rs` | model | CRUD | same file — `create_session` / `find_session_by_token_hash` facade | exact |
| `crates/oxidean-db/src/users.rs` | model | CRUD | same file — `update_profile` / `UserRow.email_verified_at` | exact |
| `crates/oxidean-api/src/auth/verify_reset.rs` | service | request-response | `crates/oxidean-api/src/auth/session.rs` + `local.rs` (issue/send) | exact |
| `crates/oxidean-api/src/auth/gate.rs` | utility | request-response | `crates/oxidean-api/src/auth/admin.rs` (`require_admin`) | exact |
| `crates/oxidean-api/src/auth/local.rs` | service | request-response | same file — `signup` / `me` / `logout_all` / `user_to_public` | exact |
| `crates/oxidean-api/src/auth/external.rs` | service | request-response | same file — `ExternalIdentity` + `link_or_create_user` | exact |
| `crates/oxidean-api/src/auth/workos.rs` | service | request-response | same file — `finish` → `ExternalIdentity` | exact |
| `crates/oxidean-api/src/auth/oidc.rs` | service | request-response | same file — `finish` → `ExternalIdentity` | exact |
| `crates/oxidean-api/src/auth/mod.rs` | config | — | same file — module re-exports | exact |
| `crates/oxidean-api/src/rpc.rs` | route | request-response | same file — `auth.*` match arms | exact |
| `crates/oxidean-api/src/app.rs` | middleware | request-response | same file — `rpc_status` | exact |
| `crates/oxidean-api/src/main.rs` | config | CRUD | same file — `maybe_seed_admin` | exact |
| `crates/oxidean-core/src/auth_types.rs` | model | transform | same file — `UserPublic` + `RESERVED_USERNAMES` | exact |
| `crates/oxidean-api/src/bin/rpc_gen.rs` | config | transform | same file — generated `UserPublic` + `auth.*` client | exact |
| `packages/api-client/src/index.ts` | config | request-response | same file (via rpc-gen; do not hand-edit) | exact |
| `apps/web/src/components/ui/input-otp.tsx` | component | transform | `apps/web/src/components/ui/input.tsx` | role-match |
| `apps/web/src/components/verify-banner.tsx` | component | request-response | `apps/web/src/routes/dashboard.tsx` (profile-incomplete banner) | exact |
| `apps/web/src/routes/verify.tsx` | route | request-response | `apps/web/src/routes/login.tsx` + `auth-shell.tsx` | exact |
| `apps/web/src/routes/reset-password.tsx` | route | request-response | `apps/web/src/routes/login.tsx` / `signup.tsx` | exact |
| `apps/web/src/routes/login.tsx` | route | request-response | same file — local form + Label-size cross-link | exact |
| `apps/web/src/routes/__root.tsx` | route | event-driven | same file — `SiteHeader` / `<main>` stacking | exact |
| `apps/web/src/routes/dashboard.tsx` | route | request-response | same file — link row + incomplete banner | exact |
| `apps/web/src/components/chrome.tsx` | component | request-response | same file — `AccountActions` `auth.me` load | role-match |
| `crates/oxidean-api/tests/auth_verify_reset.rs` | test | request-response | `crates/oxidean-api/tests/auth_session.rs` | exact |
| `crates/oxidean-api/tests/auth_verify_gate.rs` | test | request-response | `crates/oxidean-api/tests/admin_auth_settings.rs` + `auth_session.rs` | exact |
| `crates/oxidean-db/tests/dialect_auth.rs` | test | CRUD | same file — migrate + session round-trip | exact |

## Pattern Assignments

### `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0003_email_tokens.sql` (migration, CRUD)

**Analog:** `crates/oxidean-db/migrations/sqlite/0002_auth.sql` (mirror postgres/mysql siblings)

**Core pattern** (sessions hash-at-rest + FK + index — lines 16–26):
```sql
CREATE TABLE IF NOT EXISTS sessions (
  id           TEXT    PRIMARY KEY,
  user_id      TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash   TEXT    NOT NULL UNIQUE,
  expires_at   TEXT    NOT NULL,
  remember_me  INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  last_seen_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
```

**Copy for Phase 5:** Ship three dialect-parity `0003_email_tokens.sql` files; use `token_hash` + `otp_hash` CHAR/TEXT(64), `purpose`, `UNIQUE (user_id, purpose)`, index on `token_hash`. Match dialect timestamp styles from `0002_auth` (`TEXT` sqlite / `TIMESTAMPTZ` postgres / `TIMESTAMP` mysql).

---

### `crates/oxidean-db/src/email_tokens.rs` (model, CRUD)

**Analog:** `crates/oxidean-db/src/sessions.rs`

**Imports / row shape** (lines 1–16):
```rust
//! Session CRUD via `DbPool` match — opaque token hashes only (D-11, D-13).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub expires_at: String,
    // ...
}
```

**Core CRUD pattern** — dialect `match` + bind (lines 69–122 create; 125–150 find_by_token_hash):
```rust
pub async fn create(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    token_hash: &str,
    expires_at: &str,
    remember_me: bool,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => { /* $1..$5 */ }
        DbPool::MySql(p) => { /* ? */ }
        DbPool::Sqlite(p) => { /* ?1..?5 */ }
    }
    Ok(())
}
```

**Copy for Phase 5:** Same `DbPool` match; expose `upsert_by_user_purpose`, `find_by_token_hash`, `find_by_otp_hash`, `delete` / increment `attempt_count`. Map timestamps with the same `to_char` / `DATE_FORMAT` / `strftime` select constants as sessions.

---

### `crates/oxidean-db/src/lib.rs` (model, CRUD)

**Analog:** same file — sessions/users facades (lines 97–167)

```rust
pub async fn create_session(
    &self,
    id: &str,
    user_id: &str,
    token_hash: &str,
    expires_at: &str,
    remember_me: bool,
) -> Result<(), String> {
    let pool = self.require_pool()?;
    sessions::create(pool, id, user_id, token_hash, expires_at, remember_me).await
}
```

**Copy for Phase 5:** Add thin `Database` methods that call `email_tokens::*` and new `users::set_email_verified_at` / `clear_email_verified_at` — never raw SQL in API.

---

### `crates/oxidean-db/src/users.rs` (model, CRUD)

**Analog:** same file — `UserRow.email_verified_at` already selected (lines 7–80)

```rust
pub struct UserRow {
    // ...
    pub email_verified_at: Option<String>,
    // ...
}
```

**Copy for Phase 5:** Add `set_email_verified_at(id, at)` and `clear_email_verified_at(id)` with three-dialect `UPDATE` matching `update_profile` style. Column already exists in `0002_auth` — no migration for the column itself.

---

### `crates/oxidean-api/src/auth/verify_reset.rs` (service, request-response)

**Analog:** `crates/oxidean-api/src/auth/session.rs` (hash-at-rest CSPRNG) + `local.rs` (email send + session mint)

**Imports / hash-at-rest core** (`session.rs` lines 1–13, 72–106, 194–206):
```rust
use sha2::{Digest, Sha256};
// ...
const TOKEN_BYTES: usize = 32;

/// Mint a new session: CSPRNG token → cookie; SHA-256 hex → DB.
pub async fn create(...) -> Result<(String, Cookie<'static>), AuthError> {
    let mut token_bytes = [0u8; TOKEN_BYTES];
    rand::fill(&mut token_bytes);
    let raw_token = bytes_to_hex(&token_bytes);
    let token_hash = sha256_hex(raw_token.as_bytes());
    // persist token_hash only
}

fn sha256_hex(data: &[u8]) -> String {
    bytes_to_hex(&Sha256::digest(data))
}
```

**Email send pattern** (`local.rs` lines 195–205):
```rust
let welcome = OutboundEmail {
    to: email.clone(),
    subject: "Welcome to Oxidean".into(),
    text: format!(...),
    html: None,
};
if let Err(e) = ctx.email.send(welcome).await {
    tracing::error!(error = %e, "welcome email failed");
}
```

**Session mint + revoke_all after reset** (`local.rs` lines 112–123, 280–294):
```rust
async fn issue_session(
    ctx: &mut RpcCtx,
    user_id: &str,
    remember_me: bool,
) -> Result<Cookie<'static>, AppError> { /* sessions.create → CookieChange::Set */ }

pub async fn logout_all(ctx: &mut RpcCtx) -> Result<(), AppError> {
    ctx.sessions.revoke_all(&ctx.db, &session.user_id).await...;
    ctx.set_cookie = Some(CookieChange::Clear);
}
```

**Password hash on reset** — reuse `crates/oxidean-api/src/auth/password.rs` lines 13–31 (`MIN_PASSWORD_LEN = 8`, `hash_password_str`).

**Magic-link base URL** — copy `public_origin` from `crates/oxidean-api/src/routes/auth_callbacks.rs` lines 25–42 (`OXIDEAN_PUBLIC_ORIGIN`, strip trailing slash; never build from untrusted Host alone for email bodies).

**Error handling:** Map failures to stable `auth.*` codes via `AppError::new` like `local.rs` (`auth.rate_limited`, invalid/expired, SSO-only). Anti-enumeration: always `Ok` on `request_password_reset` after email normalize (`local.rs` `normalize_email` lines 49–55).

---

### `crates/oxidean-api/src/auth/gate.rs` (utility, request-response)

**Analog:** `crates/oxidean-api/src/auth/admin.rs` (`require_admin`)

**Auth/guard pattern** (lines 71–90):
```rust
async fn require_admin(ctx: &RpcCtx) -> Result<(), AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new(
            "auth.unauthenticated",
            "not authenticated",
        ));
    };
    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    if !user.is_admin {
        return Err(AppError::new(
            "admin.forbidden",
            "You need admin access to manage auth settings.",
        ));
    }
    Ok(())
}
```

**Copy for Phase 5:** Same session → load user → gate; deny with `auth.email_unverified` / “verify your email to continue” when `email_verified_at.is_none()`. Prefer returning `UserRow` for privileged handlers.

---

### `crates/oxidean-api/src/auth/local.rs` (service, request-response)

**Analog:** same file

**`user_to_public` / `me`** (lines 17–28, 297–311) — add `email_verified: row.email_verified_at.is_some()`:
```rust
pub fn user_to_public(row: &UserRow) -> UserPublic {
    UserPublic {
        id: row.id.clone(),
        // ...
        profile_incomplete: is_placeholder_username(&row.username),
    }
}
```

**Signup auto-send verify (D-23):** After existing welcome send (lines 192–205), call verify-issue helper (two messages OK per RESEARCH). Keep open signup (AUTH-05) — no invite checks.

**Reset success path:** After password update, `revoke_all` then `issue_session` + `CookieChange::Set` (mirror login lines 258–260, not logout_all’s Clear).

---

### `crates/oxidean-api/src/auth/external.rs` + `workos.rs` + `oidc.rs` (service, request-response)

**Analog:** `ExternalIdentity` (external.rs lines 8–15) and finish constructors:

```rust
pub struct ExternalIdentity {
    pub provider: String,
    pub provider_subject: String,
    pub email: String,
    pub display_name: Option<String>,
}
```

**workos.rs** (lines 142–148) — today omits WorkOS `user.email_verified`; add field and map it.

**oidc.rs** (lines 262–268) — map `claims.email_verified() == Some(true)`.

**link_or_create_user** (external.rs lines 154–229) — after create/link, if `identity.email_verified`, call `set_email_verified_at`. If false/absent, leave null (reuse local verify flows).

---

### `crates/oxidean-api/src/rpc.rs` (route, request-response)

**Analog:** same file — auth dispatch (lines 94–117)

```rust
"auth.signup" => match local::signup(ctx, req.input).await {
    Ok(user) => RpcResponse::ok(user),
    Err(e) => RpcResponse::err(e),
},
"auth.me" => match local::me(ctx).await {
    Ok(user) => RpcResponse::ok(user),
    Err(e) => RpcResponse::err(e),
},
other => RpcResponse::err(AppError::new(
    "rpc.unknown_procedure",
    format!("unknown procedure: {other}"),
)),
```

**Copy for Phase 5:** Add arms for `auth.request_verify`, `auth.resend_verify`, `auth.verify`, `auth.request_password_reset`, `auth.reset_password`, and env-gated `auth.dev.privileged_ping` (unknown_procedure outside allowlist). Import `verify_reset` / `gate`.

---

### `crates/oxidean-api/src/app.rs` (middleware, request-response)

**Analog:** `rpc_status` (lines 156–169)

```rust
fn rpc_status(resp: &RpcResponse) -> StatusCode {
    match resp {
        RpcResponse::Ok { .. } => StatusCode::OK,
        RpcResponse::Err { error, .. } if error.code == "rpc.unknown_procedure" => {
            StatusCode::NOT_FOUND
        }
        RpcResponse::Err { error, .. } if error.code == "auth.unauthenticated" => {
            StatusCode::UNAUTHORIZED
        }
        RpcResponse::Err { error, .. } if error.code == "admin.forbidden" => {
            StatusCode::FORBIDDEN
        }
        RpcResponse::Err { .. } => StatusCode::BAD_REQUEST,
    }
}
```

**Copy for Phase 5:** Add `auth.email_unverified` → `FORBIDDEN` alongside `admin.forbidden`.

---

### `crates/oxidean-api/src/main.rs` (config, CRUD)

**Analog:** `maybe_seed_admin` (lines 9–47)

```rust
db.create_user(
    &id,
    &email,
    &username,
    Some(&password_hash),
    &username,
    "",
    None,
    true,
)
.await?;
```

**Copy for Phase 5:** After insert, set `email_verified_at = now` (D-04) via new DB helper so seeded admin passes `require_verified`.

---

### `crates/oxidean-core/src/auth_types.rs` (model, transform)

**Analog:** same file — `UserPublic` (lines 23–36) + `RESERVED_USERNAMES` (lines 95–126)

```rust
pub struct UserPublic {
    pub id: String,
    pub email: String,
    pub username: String,
    pub display_name: String,
    pub bio: String,
    pub avatar_url: Option<String>,
    pub is_admin: bool,
    pub profile_incomplete: bool,
}
```

**Copy for Phase 5:** Add `pub email_verified: bool`. Append `"verify"` and `"reset-password"` to `RESERVED_USERNAMES`.

---

### `crates/oxidean-api/src/bin/rpc_gen.rs` → `packages/api-client/src/index.ts` (config, transform)

**Analog:** rpc_gen template `UserPublic` + auth client (rpc_gen.rs lines 46–55; api-client lines 36–45, 128–133)

```typescript
export type UserPublic = {
  id: string;
  email: string;
  username: string;
  display_name: string;
  bio: string;
  avatar_url?: string | null;
  is_admin: boolean;
  profile_incomplete: boolean;
};
// auth.me / signup / login return UserPublic
```

**Copy for Phase 5:** Extend generated type + add client methods for verify/reset/ping; run `cargo run -p oxidean-api --bin rpc-gen` — do not hand-edit api-client.

---

### `apps/web/src/components/ui/input-otp.tsx` (component, transform)

**Analog:** `apps/web/src/components/ui/input.tsx` (thin local wrapper)

```tsx
export function Input({ className, ...props }: ComponentProps<"input">) {
  return (
    <input
      data-slot="input"
      className={cn(
        "h-11 w-full rounded-md border border-input bg-card px-3 text-[14px] ...",
        className,
      )}
      {...props}
    />
  );
}
```

**No in-repo OTP analog.** Wrap npm `input-otp` `OTPInput` with Octane/Tailwind slots (8 digits, `inputMode="numeric"`, `autoComplete="one-time-code"`, `onComplete` auto-submit). Do not pull a shadcn registry block.

---

### `apps/web/src/components/verify-banner.tsx` (component, request-response)

**Analog:** dashboard profile-incomplete banner (`dashboard.tsx` lines 68–80)

```tsx
{user.profile_incomplete ? (
  <p
    role="status"
    className="mb-6 rounded-md border border-border bg-card px-4 py-3 text-[14px] text-foreground"
  >
    Choose a username to finish setup.{" "}
    <a href="/settings/profile" className="font-semibold text-primary ...">
      Profile
    </a>
  </p>
) : null}
```

**Copy for Phase 5:** Same surface tokens (`border-border` + `bg-card`); full-width strip under header; `role="status"`; Resend + Enter code actions; gate on `email_verified === false`.

**Me load pattern:** `chrome.tsx` `AccountActions` (lines 129–146) — `apiClient.auth.me()` with cancel flag.

---

### `apps/web/src/routes/verify.tsx` + `reset-password.tsx` (route, request-response)

**Analog:** `apps/web/src/routes/login.tsx` + `apps/web/src/components/auth-shell.tsx`

**Route / head / AuthShell** (`login.tsx` lines 1–15, 100–103):
```tsx
export const Route = createFileRoute("/login")({
  component: LoginPage,
  head: () => ({ meta: [{ title: "Sign in · Oxidean" }] }),
});

return (
  <AuthShell title="Sign in" support={mode ? support : "Loading…"}>
    {error ? <AuthErrorBanner message={error} /> : null}
    {/* form */}
  </AuthShell>
);
```

**returnTo** — `apps/web/src/lib/return-to.ts` (`safeReturnTo` / `readReturnToFromLocation`); verify anonymous magic link → `/login?returnTo=/verify?token=…`.

**Pending CTA:** `Button` disabled + `"Working…"` (login lines 149–151). Motion: reuse `oct-auth-enter` from AuthShell.

**Reset SSO mode:** Branch on `provider_config.mode` like login’s WorkOS/OIDC panels (lines 155–165) — no email form when not local.

---

### `apps/web/src/routes/login.tsx` (route, request-response)

**Analog:** same file — Label-size cross-link (lines 167–176)

```tsx
<p className="text-[14px] text-muted-foreground">
  New to Oxidean?{" "}
  <a href={signupHref} className="font-normal text-foreground underline-offset-4 hover:underline">
    Create an account
  </a>
</p>
```

**Copy for Phase 5:** After password field (before Remember me), local-only **Forgot password?** → `/reset-password`.

---

### `apps/web/src/routes/__root.tsx` (route, event-driven)

**Analog:** same file (lines 68–77)

```tsx
function RootComponent() {
  return (
    <div className="flex min-h-screen flex-col">
      <SiteHeader />
      <main className="oxidean-main flex-1">
        <Outlet />
      </main>
      <SiteFooter />
    </div>
  );
}
```

**Copy for Phase 5:** Mount `VerifyBanner` between `SiteHeader` and `<main>` so it appears on all signed-in pages.

---

### `apps/web/src/routes/dashboard.tsx` (route, request-response)

**Analog:** same file — link row (lines 91–113)

```tsx
<div className="mt-8 flex flex-wrap gap-3">
  <a href="/settings/profile" className={cn(buttonVariants({ variant: "secondary" }))}>
    Profile
  </a>
  {/* ... */}
</div>
```

**Copy for Phase 5:** Add disabled primary **New repository** Button; hint text differs unverified vs verified (UI-SPEC). Reuse `buttonVariants` / `disabled` + `aria-disabled`.

---

### `crates/oxidean-api/tests/auth_verify_reset.rs` + `auth_verify_gate.rs` (test, request-response)

**Analog:** `crates/oxidean-api/tests/auth_session.rs` (lines 1–65 harness)

```rust
async fn test_app(db: Database) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development");
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

fn rpc_req(body: &str) -> Request<Body> { /* POST /api/rpc + Oxidean-RPC-Version */ }
fn rpc_req_with_cookie(body: &str, cookie: &str) -> Request<Body> { /* + cookie */ }
fn session_cookie_from_response(res: &...) -> String { /* parse Set-Cookie */ }
```

**Gate tests:** Assert `auth.email_unverified` + HTTP 403 (extend status mapping); env-gated ping unknown outside allowlist. Prefer recording/`LogSink` for mail assertions like signup welcome tests.

---

### `crates/oxidean-db/tests/dialect_auth.rs` (test, CRUD)

**Analog:** same file (lines 15–76)

```rust
#[tokio::test]
async fn migrate_auth_and_user_round_trip() {
    let Some(url) = database_url() else { return; };
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    // create_user → create_session → find_by_token_hash → delete
}
```

**Copy for Phase 5:** After migrate, upsert email token + find by hash + set/clear `email_verified_at` on three dialects.

---

## Shared Patterns

### Authentication / session cookie
**Source:** `crates/oxidean-api/src/auth/session.rs`, `local.rs`
**Apply to:** verify consume (session must match token user), reset success (mint + revoke others), privileged ping
```rust
// Require session
let Some(session) = &ctx.session else {
    return Err(AppError::new("auth.unauthenticated", "not authenticated"));
};
// Mint after successful auth
let cookie = issue_session(ctx, &user.id, false).await?;
ctx.set_cookie = Some(CookieChange::Set(cookie));
```

### Privileged gate (admin → verified)
**Source:** `crates/oxidean-api/src/auth/admin.rs` `require_admin`
**Apply to:** `gate.rs`, `auth.dev.privileged_ping`, future `repo.create`
```rust
// Deny with stable code + short message; map HTTP via rpc_status
AppError::new("auth.email_unverified", "verify your email to continue")
```

### Error handling / RPC codes
**Source:** `crates/oxidean-api/src/auth/local.rs`, `app.rs` `rpc_status`
**Apply to:** all new auth RPCs
- Unauthenticated → `auth.unauthenticated` (401)
- Forbidden gate → `auth.email_unverified` (403) — extend `rpc_status`
- Rate limit → `auth.rate_limited`
- Bad input → `rpc.bad_input` / domain `auth.*`
- Unknown env-gated ping → `rpc.unknown_procedure` (404)

### Email outbound
**Source:** `crates/oxidean-api/src/email/mod.rs` + `local.rs` welcome send
**Apply to:** verify/reset templates
```rust
pub struct OutboundEmail {
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
}
// LogSink | Smtp | Resend via EmailSender; never log plaintext OTP/token
```

### Validation
**Source:** `local.rs` `normalize_email`, `password.rs` `MIN_PASSWORD_LEN`, `return-to.ts` / `sanitize_return_to`
**Apply to:** reset request email, OTP digit pattern, reset password ≥ 8, magic-link returnTo

### Hash-at-rest secrets
**Source:** `session.rs` CSPRNG + SHA-256 hex
**Apply to:** magic link (32-byte hex) and OTP (SHA-256 of digit string); never store plaintext

### Frontend auth chrome
**Source:** `AuthShell` / `AuthErrorBanner`, `apiClient` with credentials
**Apply to:** `/verify`, `/reset-password`, banner, forgot link
- Titles: `Page · Oxidean`
- Pending: disabled + `Working…`
- Cross-links: Label-size underline links

### Client generation
**Source:** `rpc_gen.rs` → `packages/api-client`
**Apply to:** any DTO/RPC surface change — regenerate; do not hand-edit generated index

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `apps/web/src/components/ui/input-otp.tsx` (OTP behavior) | component | transform | No OTP/slot control in repo; use `input.tsx` wrapper style + RESEARCH/UI-SPEC `input-otp` usage. Closest role-match only. |

## Metadata

**Analog search scope:** `crates/oxidean-{api,db,core}`, `apps/web/src/{routes,components,lib}`, `packages/api-client`, `crates/oxidean-{api,db}/tests`, `crates/oxidean-db/migrations`
**Files scanned:** ~87 Rust/TS/SQL sources under crates + apps/web + packages
**Tracked-source gate:** All named analogs verified via `git ls-files` (non-empty)
**Pattern extraction date:** 2026-09-10
