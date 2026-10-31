# Phase 6: Self-Host Admin Bootstrap - Pattern Map

**Mapped:** 2026-09-11
**Files analyzed:** 28
**Analogs found:** 25 / 28

> **Tracked-source gate:** Analogs below are git-**tracked** paths only. The working tree has untracked `.tsrx` renames and WIP `bootstrap.rs` / `auth_bootstrap.rs` — do **not** treat those as canonical analogs. Prefer the tracked `.tsx` / committed Rust modules named here; executor may land edits on `.tsrx` if that rename is already in flight.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/oxidean-db/migrations/{sqlite,postgres,mysql}/0006_bootstrap_flags.sql` | migration | transform | `crates/oxidean-db/migrations/sqlite/0004_email_token_issue_count.sql` (+ pg/mysql twins) | exact |
| `crates/oxidean-db/src/auth_settings.rs` | model | CRUD | same file | exact |
| `crates/oxidean-db/src/users.rs` | model | CRUD | same file (`set_password_hash`, `map_user`, `update_user_profile`) | exact |
| `crates/oxidean-core/src/auth_types.rs` | model | transform | same file (`ProviderConfigPublic`, `UserPublic`, reserved list) | exact |
| `crates/oxidean-api/src/auth/seed.rs` | service | CRUD | same file | exact |
| `crates/oxidean-api/src/auth/bootstrap.rs` | service | request-response | `seed.rs` + `local.rs` signup | role-match |
| `crates/oxidean-api/src/auth/local.rs` | service | request-response | same file (`signup`, `provider_config`) | exact |
| `crates/oxidean-api/src/auth/admin.rs` | service | CRUD | same file (`require_admin`, `update_settings`) | exact |
| `crates/oxidean-api/src/rpc.rs` | route | request-response | same file (`auth.dev.privileged_ping` env gate) | role-match |
| `crates/oxidean-api/src/main.rs` | config | request-response | same file (`OXIDEAN_AUTO_MIGRATE` + fail-closed seed) | exact |
| `crates/oxidean-api/src/routes/auth_callbacks.rs` | route | request-response | `crates/oxidean-api/src/auth/gate.rs` (`require_verified`) | role-match |
| `crates/oxidean-api/tests/auth_bootstrap.rs` (+ credentials tests) | test | request-response | `crates/oxidean-api/tests/auth_signup.rs` | role-match |
| `crates/oxidean-db/tests/dialect_auth.rs` | test | CRUD | same file | exact |
| `apps/web/src/lib/ssr-auth.ts` | utility | request-response | `packages/api-client/src/index.ts` (`CreateClientOptions.fetch`) + `apps/web/src/lib/api-client.ts` | partial |
| `apps/web/src/routes/index.tsrx` | route | request-response | `apps/web/src/routes/index.tsx` | exact |
| `apps/web/src/routes/setup.tsrx` | route | request-response | `apps/web/src/routes/signup.tsx` + `apps/web/src/components/auth-shell.tsx` | role-match |
| `apps/web/src/routes/setup.credentials.tsrx` | route | request-response | `apps/web/src/routes/reset-password.tsx` | role-match |
| `apps/web/src/routes/signup.tsrx` | route | request-response | `apps/web/src/routes/signup.tsx` | exact |
| `apps/web/src/routes/dashboard.tsrx` | route | request-response | `apps/web/src/routes/dashboard.tsx` (replace soft page with `notFound()`) | partial |
| `apps/web/src/routes/login.tsrx` | route | request-response | `apps/web/src/routes/login.tsx` | exact |
| `apps/web/src/routes/admin/auth.tsrx` | route | CRUD | `apps/web/src/routes/admin/auth.tsx` | exact |
| `apps/web/src/components/ui/switch.tsrx` | component | request-response | `apps/web/src/components/ui/checkbox.tsx` | exact |
| `apps/web/src/components/chrome.tsrx` | component | request-response | `apps/web/src/components/chrome.tsx` | exact |
| `apps/web/src/lib/bootstrap.ts` | utility | request-response | `apps/web/src/lib/return-to.ts` (client helper shape; SSR supersedes) | partial |
| `packages/api-client/src/index.ts` | utility | request-response | same file (`make rpc-gen`) | exact |
| `docs/CONFIGURATION.md`, `docs/ARCHITECTURE.md`, `docs/API.md`, `.env.example` | config | — | same files | exact |
| `.planning/REQUIREMENTS.md` | config | — | same file (AUTH-06/07 reframe) | exact |

## Pattern Assignments

### `crates/oxidean-db/migrations/*/0006_bootstrap_flags.sql` (migration, transform)

**Analog:** `crates/oxidean-db/migrations/sqlite/0004_email_token_issue_count.sql` (and postgres/mysql twins)

**Core pattern** (sqlite lines 1-2; postgres uses `INT`, mysql `INT`):
```sql
-- logical: 0004_email_token_issue_count — soft rate-limit issue counter (D-19)
ALTER TABLE auth_email_tokens ADD COLUMN issue_count INTEGER NOT NULL DEFAULT 1;
```

**Apply:** Same `-- logical:` header + triple dialect files. Add `instance_auth_settings.allow_signup` (default false) and `users.must_change_credentials` (default false). Mirror boolean typing per dialect like other auth columns in `0002_auth.sql`.

---

### `crates/oxidean-db/src/auth_settings.rs` (model, CRUD)

**Analog:** same file

**Imports / row shape** (lines 7-16):
```rust
pub struct AuthSettingsRow {
    pub provider_mode: String,
    pub email_provider: String,
    pub from_address: Option<String>,
    pub oidc_issuer: Option<String>,
    pub oidc_client_id: Option<String>,
    pub workos_client_id: Option<String>,
    pub updated_at: String,
}
```

**Core pattern** (lines 18-45 `map_settings!` + dialect `SETTINGS_SELECT_*` + `update` binds):
```rust
macro_rules! map_settings {
    ($row:expr) => {{
        let row = $row;
        AuthSettingsRow {
            provider_mode: row
                .try_get("provider_mode")
                .map_err(|e| format!("auth settings row: {e}"))?,
            // ... try_get each column ...
        }
    }};
}
```

**Apply:** Extend row, selects, and `update(...)` signature with `allow_signup: bool` (or `i32`/`bool` per dialect). Keep singleton `WHERE id = 1`.

---

### `crates/oxidean-db/src/users.rs` (model, CRUD)

**Analog:** same file

**Row mapping** (lines 8-56) — extend `UserRow` + `map_user!` + `USER_SELECT_*` for `must_change_credentials`.

**Password update** (lines 446-489):
```rust
pub async fn set_password_hash(
    pool: &DbPool,
    id: &str,
    password_hash: &str,
) -> Result<UserRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1",
            )
            // ...
```

**Apply:** Add dialect-safe `update_user_email` (or combined credentials update) beside `set_password_hash` / `update_user_profile` (lines 267+). Clear `must_change_credentials` in the same write as successful confirm.

---

### `crates/oxidean-core/src/auth_types.rs` (model, transform)

**Analog:** same file

**DTOs to extend** (lines 64-114, 126-149):
```rust
pub struct UserPublic {
    // ...
    pub email_verified: bool,
    // Phase 6: must_change_credentials: bool
}

pub struct BootstrapStatus {
    pub needs_setup: bool,
}

pub struct ProviderConfigPublic {
    pub mode: ProviderMode,
    // Phase 6: allow_signup: bool
}

pub struct AuthSettingsPublic { /* ... */ }
pub struct UpdateAuthSettingsRequest { /* ... */ }
// Phase 6: allow_signup on both public + update request
```

**Reserved usernames** (lines 151-184) — add `"system-administrator"`; keep `"setup"` reserved.

**Wizard request already present** (lines 86-92):
```rust
pub struct BootstrapSetupRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}
```
Extend with `allow_signup: bool` for wizard Switch.

---

### `crates/oxidean-api/src/auth/seed.rs` (service, CRUD)

**Analog:** same file (primary ENV seed pattern)

**Core pattern** (lines 12-54):
```rust
pub async fn maybe_seed_admin(db: &Database) -> Result<(), String> {
    let email = match std::env::var("OXIDEAN_ADMIN_EMAIL") {
        Ok(v) if !v.is_empty() => v.trim().to_ascii_lowercase(),
        _ => return Ok(()),
    };
    let password = match std::env::var("OXIDEAN_ADMIN_PASSWORD") {
        Ok(v) if !v.is_empty() => v,
        _ => return Ok(()),
    };
    // ...
    let username = match db.find_user_by_username("admin").await? {
        None => "admin".to_string(),
        Some(_) => "admin1".to_string(),
    };
```

**Apply:** Fixed username `"system-administrator"`; set `must_change_credentials=true`; parse/write `OXIDEAN_ALLOW_SIGNUP` like `main.rs` AUTO_MIGRATE (`true`/`1`, default **false**); keep auto-verify via `set_email_verified_at`.

---

### `crates/oxidean-api/src/auth/bootstrap.rs` (service, request-response)

**Analog:** `crates/oxidean-api/src/auth/seed.rs` (create sys-admin) + `crates/oxidean-api/src/auth/local.rs` (RPC parse/session/errors)

> File is **untracked WIP** on disk — planner/executor should treat it as **create/stabilize** using tracked patterns below, not copy from the untracked blob as source of truth.

**Error / gate pattern from `local.rs`** (lines 127-152):
```rust
pub async fn signup(ctx: &mut RpcCtx, input: serde_json::Value) -> Result<UserPublic, AppError> {
    // ...
    let req: SignupRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid signup input: {e}"))
    })?;
    validate_username(&req.username).map_err(map_username_err)?;
    if is_reserved_username(&username) {
        return Err(AppError::new(
            "auth.reserved_username",
            "username is reserved",
        ));
    }
```

**Session cookie issuance:** reuse existing `issue_session` / cookie helpers used by signup/login in `local.rs` (same module family).

**Apply:** `bootstrap_status` / `bootstrap_setup`; reserved-username bypass for wizard (see RESEARCH); persist `allow_signup` on setup; new `auth.confirm_admin_credentials` (or equivalent) for ENV forced-change — mirror `verify_reset::reset_password` validation + `set_password_hash` / profile username update.

---

### `crates/oxidean-api/src/auth/local.rs` (service, request-response)

**Analog:** same file

**Public config** (lines 329-333):
```rust
pub async fn provider_config(ctx: &RpcCtx) -> Result<oxidean_core::ProviderConfigPublic, AppError> {
    let mode = resolve_provider_mode(ctx).await.unwrap_or(ProviderMode::Local);
    Ok(oxidean_core::ProviderConfigPublic { mode })
}
```

**Apply:** Include `allow_signup` from settings (fail closed if unset). After bootstrap, reject `auth.signup` when `allow_signup == false` (in addition to empty-instance setup gate). Tracked HEAD signup has no `needs_setup` check yet — add setup + allow_signup gates using `AppError::new` codes (`auth.setup_required` / product code for closed signup).

---

### `crates/oxidean-api/src/auth/admin.rs` (service, CRUD)

**Analog:** same file

**Auth guard** (lines 71-90):
```rust
async fn require_admin(ctx: &RpcCtx) -> Result<(), AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new(
            "auth.unauthenticated",
            "not authenticated",
        ));
    };
    // ...
    if !user.role.is_sys_admin() {
        return Err(AppError::new(
            "admin.forbidden",
            "You need system admin access to manage auth settings.",
        ));
    }
    Ok(())
}
```

**Update path** (lines 141-173): parse `UpdateAuthSettingsRequest` → `db.update_auth_settings` → `settings_to_public`.

**Apply:** Thread `allow_signup` through DTO map, update, and admin UI RPC.

---

### `crates/oxidean-api/src/rpc.rs` (route, request-response)

**Analog:** same file — early procedure gate pattern via `auth.dev.privileged_ping`

**Conditional procedure** (HEAD lines 141-152):
```rust
"auth.dev.privileged_ping" => {
    if !verify_reset::privileged_ping_env_allowed(&ctx.env_name) {
        RpcResponse::err(AppError::new(
            "rpc.unknown_procedure",
            format!("unknown procedure: {}", req.procedure),
        ))
    } else {
        match verify_reset::privileged_ping(ctx).await {
            Ok(v) => RpcResponse::ok(v),
            Err(e) => RpcResponse::err(e),
        }
    }
}
```

**Apply:** At top of `dispatch`, when `needs_setup`, allowlist only `auth.bootstrap_status`, `auth.bootstrap_setup`, `system.health` (+ readiness if routed here). Reject others with a clear `AppError` code. Wire new confirm-credentials procedure next to other `auth.*` arms.

---

### `crates/oxidean-api/src/main.rs` (config, boot)

**Analog:** same file

**ENV bool parse** (lines 49-51):
```rust
let auto_migrate = std::env::var("OXIDEAN_AUTO_MIGRATE")
    .map(|v| v == "true" || v == "1")
    .unwrap_or(true);
```

**Fail-closed seed** (lines 66-69):
```rust
if let Err(e) = seed::maybe_seed_admin(&db).await {
    eprintln!("admin seed failed: {e}");
    std::process::exit(1);
}
```

**Apply:** Keep fail-closed. Parse `OXIDEAN_ALLOW_SIGNUP` with same `true`/`1` rule and `unwrap_or(false)` inside seed (or pass into seed).

---

### `crates/oxidean-api/src/routes/auth_callbacks.rs` (route, request-response)

**Analog:** `crates/oxidean-api/src/auth/gate.rs` for early hard reject shape

**Core gate** (gate.rs lines 20-42):
```rust
pub async fn require_verified(ctx: &RpcCtx) -> Result<UserRow, AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new(
            "auth.unauthenticated",
            "not authenticated",
        ));
    };
    // ...
}
```

**Apply:** SSO starts already need empty-instance redirect to `/setup` (D-09/D-10). Implement `reject_if_setup_required` early-return `Redirect` on WorkOS/OIDC start handlers — same “check before side effects” as `require_verified`.

---

### `crates/oxidean-api/tests/auth_bootstrap.rs` (+ credentials) (test, request-response)

**Analog:** `crates/oxidean-api/tests/auth_signup.rs`

**Harness** (lines 28-59):
```rust
async fn app_with_recorder(db: Database) -> (axum::Router, Arc<RecordingSender>) { /* ... */ }

fn rpc_req(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

#[tokio::test]
async fn signup_sets_cookie_and_sends_welcome() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("signup.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
```

**Seed assertion** (lines 191-214 `seeded_admin_is_auto_verified`): set `OXIDEAN_ADMIN_*`, call `maybe_seed_admin`, assert role + verified.

**Apply:** Tempdir SQLite + migrate + RPC oneshot. Cover: partial ENV → needs_setup; wizard `allow_signup`; strict RPC allowlist; forced credentials; signup blocked when closed. Prefer a tracked shared test helper for ENV mutex once bootstrap lands (avoid racy env in parallel nextest).

---

### `apps/web/src/lib/ssr-auth.ts` (utility, request-response)

**Analog (partial):** `packages/api-client/src/index.ts` + `apps/web/src/lib/api-client.ts`

**Browser client** (`api-client.ts` lines 1-7):
```typescript
export const apiClient = createClient({
  baseUrl: typeof window !== "undefined" ? window.location.origin : "http://127.0.0.1:8080",
  credentials: "include",
});
```

**Fetch override** (`packages/api-client/src/index.ts` lines 123-145):
```typescript
export type CreateClientOptions = {
  baseUrl: string;
  fetch?: typeof fetch;
  credentials?: RequestCredentials;
};

async function rpcCall<T>(/* ... */) {
  const fetchFn = opts.fetch ?? fetch;
  const res = await fetchFn(url, {
    method: "POST",
    credentials: opts.credentials ?? "include",
    headers: {
      "content-type": "application/json",
      [RPC_VERSION_HEADER]: String(RPC_VERSION),
    },
    body: JSON.stringify({ procedure, input }),
  });
```

**Apply:** New module with `createServerFn` + `getRequestHeader("cookie")` cookie-forward fetch (RESEARCH Pattern 2). **No tracked in-app `createServerFn` usage** — see No Analog Found. Expose `fetchBootstrapStatus` / `fetchSessionMe` / public `allow_signup` helpers for route `beforeLoad`/`loader`.

---

### `apps/web/src/routes/setup.tsrx` (route, request-response)

**Analog:** `apps/web/src/routes/signup.tsx` + `apps/web/src/components/auth-shell.tsx`

**Route shell** (`signup.tsx` lines 1-14):
```typescript
import { createFileRoute } from "@octanejs/tanstack-router";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
// ...
export const Route = createFileRoute("/signup")({
  component: SignupPage,
  head: () => ({ meta: [{ title: "Sign up · Oxidean" }] }),
});
```

**AuthShell layout** (`auth-shell.tsx` lines 4-32):
```typescript
export function AuthShell({
  title,
  support,
  children,
  className,
}: {
  title: string;
  support: string;
  children?: unknown;
  className?: string;
}) {
  return (
    <div className="relative overflow-hidden px-4 py-16">
      {/* radial mesh + OxideanMark 48 + Heading + Body support */}
```

**Form submit / error codes** (`signup.tsx` lines 73-99): pending flag, `AuthErrorBanner`, map RPC codes to copy.

**Apply:** Title/support/CTA from UI-SPEC; add Switch for `allow_signup`; call `auth.bootstrap_setup`; success → `/`. Prefer SSR `beforeLoad` redirect when `!needs_setup`.

---

### `apps/web/src/routes/setup.credentials.tsrx` (route, request-response)

**Analog:** `apps/web/src/routes/reset-password.tsx`

**Route + view state** (lines 12-57):
```typescript
export const Route = createFileRoute("/reset-password")({
  component: ResetPasswordPage,
  head: () => ({
    meta: [
      { title: "Reset password · Oxidean" },
      // ...
    ],
  }),
});

type View =
  | { kind: "loading" }
  | { kind: "request" }
  | { kind: "redeem"; token: string | null };
```

**Apply:** AuthShell + Switch “Keep current password”; reject username equal to `system-administrator`; honor `safeReturnTo` from `apps/web/src/lib/return-to.ts` (lines 5-22).

---

### `apps/web/src/routes/index.tsrx` (route, request-response)

**Analog:** `apps/web/src/routes/index.tsx`

**Current marketing CTAs** (lines 74-77) — gate on `allow_signup`:
```typescript
<Link to="/signup" preload="intent" className={cn(buttonVariants())}>
  Get started
</Link>
```

**Apply:** SSR priority from UI-SPEC: `needs_setup` → `/setup`; else session → `SignedInHome`; else marketing. First HTML must match final tree (no client-only flash). Omit Get started links when signup closed.

---

### `apps/web/src/routes/dashboard.tsrx` (route, request-response)

**Analog:** `apps/web/src/routes/dashboard.tsx` (anti-pattern to remove)

**Soft gate today** (lines 8-28):
```typescript
export const Route = createFileRoute("/dashboard")({
  component: DashboardPage,
  head: () => ({ meta: [{ title: "Dashboard · Oxidean" }] }),
});
// useEffect → auth.me → login?returnTo=/dashboard
```

**Apply:** Replace with `notFound()` from `@octanejs/tanstack-router` (no soft redirect). No tracked `notFound()` usage in apps/web yet — see No Analog Found.

---

### `apps/web/src/routes/signup.tsrx` / `login.tsrx` (route, request-response)

**Analogs:** tracked `signup.tsx` / `login.tsx`

**Login cross-link** (`login.tsx` lines 175-183):
```typescript
New to Oxidean?{" "}
<a href={signupHref} /* ... */>
  Create an account
</a>
```

**Apply:** `/signup` → `notFound()` when `allow_signup === false` (SSR/`beforeLoad`). Login omits Create an account when closed. Keep `safeReturnTo` after login / forced-change (D-22).

---

### `apps/web/src/components/chrome.tsrx` (component, request-response)

**Analog:** `apps/web/src/components/chrome.tsx`

**Logged-out Sign up** (lines 189-210):
```typescript
<Link to="/login" /* ... */>Sign in</Link>
<Link
  to="/signup"
  preload="intent"
  className={cn(buttonVariants({ variant: "secondary" }), /* ... */)}
>
  Sign up
</Link>
```

**Apply:** Fetch public `allow_signup` (via `provider_config`); **omit** Sign up entirely when false/unknown (fail closed per UI-SPEC). While `needs_setup`, omit Sign in/Sign up account CTAs.

---

### `apps/web/src/routes/admin/auth.tsrx` (route, CRUD)

**Analog:** `apps/web/src/routes/admin/auth.tsx`

**Form placement** (lines 265-271): after Auth provider `ModeSelect`, add Switch + Label **Allow open signup** + helper from UI-SPEC. Persist via existing `admin.auth.update_settings` once DTO includes the field.

**Phase / forbidden pattern** (lines 48-52, 227-231): keep sys-admin gate UX.

---

### `apps/web/src/components/ui/switch.tsrx` (component, request-response)

**Analog:** `apps/web/src/components/ui/checkbox.tsx`

**Full wrapper** (lines 1-31):
```typescript
import { Checkbox as CheckboxPrimitive } from "@octanejs/base-ui/checkbox";
import { Check } from "@octanejs/lucide";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export function Checkbox({
  className,
  ...props
}: ComponentProps<typeof CheckboxPrimitive.Root>) {
  return (
    <CheckboxPrimitive.Root
      data-slot="checkbox"
      className={cn(
        "peer relative inline-flex size-5 shrink-0 /* ... */",
        className,
      )}
      {...props}
    >
```

**Apply:** Same hand-authored Base UI wrapper for `@octanejs/base-ui/switch`; `data-slot="switch"`; ≥44px hit target; primary for checked track.

---

### `apps/web/src/lib/bootstrap.ts` (utility, request-response)

**Analog:** `apps/web/src/lib/return-to.ts` (small client helper)

**safeReturnTo** (lines 5-22) — keep for post-login / post-confirm redirects.

**Apply:** Client `redirectIfNeedsSetup` is superseded by SSR gates; delete or demote to PE only (D-21). Do not use as security boundary.

---

### Docs / `.env.example` / REQUIREMENTS

**Analogs:** existing files.

**CONFIGURATION table pattern** (`docs/CONFIGURATION.md` lines 22-23): document `OXIDEAN_ALLOW_SIGNUP` next to `OXIDEAN_ADMIN_*`; default false; `true`/`1` only.

**.env.example** (lines 34-35): add commented `OXIDEAN_ALLOW_SIGNUP=false`.

**REQUIREMENTS:** reframe AUTH-06/07 as empty-instance (D-02); AUTH-05 interaction with `allow_signup`.

---

## Shared Patterns

### AppError + RPC codes
**Source:** `crates/oxidean-api/src/auth/local.rs`, `admin.rs`
**Apply to:** bootstrap, signup gate, confirm credentials, RPC allowlist
```rust
return Err(AppError::new(
    "auth.setup_required",
    "Complete instance setup before signing up.",
));
```

### Fail-closed boot
**Source:** `crates/oxidean-api/src/main.rs` lines 66-69
**Apply to:** seed failure when both ENV set

### ENV bool parse
**Source:** `crates/oxidean-api/src/main.rs` lines 49-51
**Apply to:** `OXIDEAN_ALLOW_SIGNUP` (default **false**)

### AuthShell + AuthErrorBanner
**Source:** `apps/web/src/components/auth-shell.tsx`
**Apply to:** `/setup`, `/setup/credentials`

### safeReturnTo
**Source:** `apps/web/src/lib/return-to.ts` lines 5-22
**Apply to:** login, forced credential change success

### Session cookies
**Source:** `crates/oxidean-api/src/auth/session.rs` lines 15-20
```rust
pub const SESSION_COOKIE_NAME: &str = "oxidean_session";
pub const SESSION_PRESENCE_COOKIE_NAME: &str = "oxidean_signed_in";
```
**Apply to:** SSR Cookie forward (`oxidean_session`); presence hint PE only

### Triple-dialect SQL helpers
**Source:** `crates/oxidean-db/src/{auth_settings,users}.rs`
**Apply to:** new columns + email update helper

### Integration test RPC harness
**Source:** `crates/oxidean-api/tests/auth_signup.rs`
**Apply to:** bootstrap / allow_signup / credentials / RPC allowlist tests

### Sys-admin settings guard
**Source:** `crates/oxidean-api/src/auth/admin.rs` `require_admin`
**Apply to:** post-bootstrap `allow_signup` admin toggle

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `apps/web/src/lib/ssr-auth.ts` (`createServerFn` / `getRequestHeader`) | utility | request-response | No tracked app usage of TanStack Start server fns; use RESEARCH Pattern 2 + `@octanejs/tanstack-start` exports |
| `apps/web/src/routes/dashboard.tsrx` / closed `/signup` `notFound()` | route | request-response | No tracked `notFound()` / `beforeLoad` in `apps/web`; use `@octanejs/tanstack-router` exports per RESEARCH Don't Hand-Roll |
| `crates/oxidean-api/src/auth/bootstrap.rs` (as self-analog) | service | request-response | Untracked WIP only — use tracked `seed.rs` + `local.rs` patterns above |

## Metadata

**Analog search scope:** `crates/oxidean-api`, `crates/oxidean-db`, `crates/oxidean-core`, `apps/web/src` (tracked `.tsx`), `packages/api-client`, `docs/`, `.env.example`
**Files scanned:** ~120 tracked paths in those trees; plus HEAD blobs for deleted `.tsx` working-tree renames
**Pattern extraction date:** 2026-09-11
**Note:** Prefer tracked analogs over working-tree `.tsrx` / untracked bootstrap until those land in git
