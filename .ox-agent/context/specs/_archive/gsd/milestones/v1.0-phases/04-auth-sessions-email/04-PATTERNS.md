# Phase 4: Auth Sessions & Email - Pattern Map

**Mapped:** 2026-09-10
**Files analyzed:** 38
**Analogs found:** 32 / 38

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/oxidean-core/src/auth_types.rs` (or extend `lib.rs`) | model | transform | `crates/oxidean-core/src/lib.rs` | exact |
| `crates/oxidean-db/migrations/*/0002_auth.sql` | migration | CRUD | `crates/oxidean-db/migrations/*/0001_init.sql` | exact |
| `crates/oxidean-db/src/users.rs` | model | CRUD | `crates/oxidean-db/src/probe.rs` | exact |
| `crates/oxidean-db/src/sessions.rs` | model | CRUD | `crates/oxidean-db/src/probe.rs` | exact |
| `crates/oxidean-db/src/auth_identities.rs` | model | CRUD | `crates/oxidean-db/src/probe.rs` | exact |
| `crates/oxidean-db/src/auth_settings.rs` | model | CRUD | `crates/oxidean-db/src/probe.rs` | exact |
| `crates/oxidean-db/src/lib.rs` | config | — | `crates/oxidean-db/src/lib.rs` | exact (extend) |
| `crates/oxidean-db/src/migrate.rs` | utility | batch | `crates/oxidean-db/src/migrate.rs` | exact (parity test) |
| `crates/oxidean-api/src/auth/mod.rs` | service | request-response | `crates/oxidean-api/src/rpc.rs` | role-match |
| `crates/oxidean-api/src/auth/local.rs` | service | request-response | `crates/oxidean-api/src/rpc.rs` (`system.echo` validate+err) | role-match |
| `crates/oxidean-api/src/auth/workos.rs` | service | request-response | — | none (external SDK) |
| `crates/oxidean-api/src/auth/oidc.rs` | service | request-response | — | none (external crate) |
| `crates/oxidean-api/src/auth/session.rs` | service | request-response | `crates/oxidean-api/src/cors.rs` (env Secure flags) | partial |
| `crates/oxidean-api/src/auth/password.rs` | utility | transform | — | none (argon2; RESEARCH) |
| `crates/oxidean-api/src/email/mod.rs` | service | event-driven | — | none (new trait) |
| `crates/oxidean-api/src/email/log_sink.rs` | service | event-driven | `tracing` usage in `rpc.rs` | partial |
| `crates/oxidean-api/src/email/smtp.rs` | service | request-response | — | none (lettre) |
| `crates/oxidean-api/src/email/resend.rs` | service | request-response | — | none (reqwest) |
| `crates/oxidean-api/src/routes/auth_callbacks.rs` | controller | request-response | `crates/oxidean-api/src/app.rs` | exact |
| `crates/oxidean-api/src/routes/avatar.rs` | controller | file-I/O | `crates/oxidean-api/src/app.rs` | role-match |
| `crates/oxidean-api/src/rpc.rs` | controller | request-response | `crates/oxidean-api/src/rpc.rs` | exact (extend) |
| `crates/oxidean-api/src/app.rs` | config | request-response | `crates/oxidean-api/src/app.rs` | exact (extend) |
| `crates/oxidean-api/src/lib.rs` | config | — | `crates/oxidean-api/src/lib.rs` | exact |
| `crates/oxidean-api/src/cors.rs` | middleware | request-response | `crates/oxidean-api/src/cors.rs` | exact (credentials ready) |
| `crates/oxidean-api/src/main.rs` | config | — | `crates/oxidean-api/src/main.rs` | exact (admin seed) |
| `crates/oxidean-api/src/bin/rpc_gen.rs` | config | transform | `crates/oxidean-api/src/bin/rpc_gen.rs` | exact |
| `crates/oxidean-api/tests/auth_signup.rs` | test | request-response | `crates/oxidean-api/tests/rpc_http.rs` | exact |
| `crates/oxidean-api/tests/auth_session.rs` | test | request-response | `crates/oxidean-api/tests/rpc_db_probe.rs` | exact |
| `crates/oxidean-api/tests/profile_avatar.rs` | test | file-I/O | `crates/oxidean-api/tests/rpc_db_probe.rs` | role-match |
| `crates/oxidean-db/tests/dialect_auth.rs` | test | CRUD | `crates/oxidean-db/tests/dialect_probe.rs` | exact |
| `packages/api-client/src/index.ts` | config | request-response | `packages/api-client/src/index.ts` | exact (regen) |
| `apps/web/src/routes/login.tsx` | route | request-response | `apps/web/src/routes/status.tsx` | exact |
| `apps/web/src/routes/signup.tsx` | route | request-response | `apps/web/src/routes/status.tsx` | exact |
| `apps/web/src/routes/dashboard.tsx` | route | request-response | `apps/web/src/routes/status.tsx` | exact |
| `apps/web/src/routes/settings/profile.tsx` | route | CRUD | `apps/web/src/routes/status.tsx` + `ui/input.tsx` | role-match |
| `apps/web/src/routes/admin/auth.tsx` | route | CRUD | `apps/web/src/routes/status.tsx` + `ui/select.tsx` | role-match |
| `apps/web/src/components/chrome.tsx` | component | request-response | `apps/web/src/components/chrome.tsx` | exact (extend) |
| `apps/web/src/routes/index.tsx` | route | — | `apps/web/src/routes/index.tsx` | exact (enable CTA) |
| `apps/web/src/components/ui/{label,checkbox,textarea,dropdown-menu}.tsx` | component | — | `apps/web/src/components/ui/button.tsx` / `input.tsx` | role-match |
| `apps/web/src/components/avatar-preview.tsx` (local) | component | — | `apps/web/src/components/oxidean-mark.tsx` | role-match |

## Pattern Assignments

### `crates/oxidean-core/src/auth_types.rs` (model, transform)

**Analog:** `crates/oxidean-core/src/lib.rs`

**Imports / serde DTOs** (lines 1–18):
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self { /* … */ }
}
```

**Core pattern:** Add `UserPublic`, `ProviderMode`, `AuthSettingsPublic`, profile DTOs beside existing `HealthResponse` / `DbProbeResponse`. Keep `AppError::new("auth.*", …)` codes. Re-export from `lib.rs` like other types.

**Error handling:** Same `AppError` — no new error envelope.

---

### `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0002_auth.sql` (migration, CRUD)

**Analog:** `crates/oxidean-db/migrations/*/0001_init.sql`

**Postgres** (full file pattern):
```sql
-- logical: 0001_init — instances (D-09/D-10)
CREATE TABLE IF NOT EXISTS instances (
  id          INTEGER     PRIMARY KEY,
  dialect     TEXT        NOT NULL,
  probe_count BIGINT      NOT NULL DEFAULT 0,
  probed_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

**MySQL / SQLite:** Same logical name comment + dialect-native types (`VARCHAR`/`TIMESTAMP` vs `TEXT`/`strftime`). Mirror three files with identical **filenames** — enforced by `migration_parity` in `migrate.rs` lines 51–60.

**Core pattern:** Add `0002_auth.sql` to all three dirs with `users`, `sessions`, `auth_identities`, `instance_auth_settings`. Prefer app-normalized lowercase email (`TEXT`/`VARCHAR`) over CITEXT for parity. Keep `CREATE TABLE IF NOT EXISTS`.

---

### `crates/oxidean-db/src/{users,sessions,auth_identities,auth_settings}.rs` (model, CRUD)

**Analog:** `crates/oxidean-db/src/probe.rs`

**Imports** (lines 1–7):
```rust
use oxidean_core::DbProbeResponse;
use sqlx::Row;

use crate::dialect::Dialect;
use crate::pool::DbPool;
```

**Core dialect-branch pattern** (lines 9–23 sketch — Postgres arm):
```rust
pub async fn probe(pool: &DbPool, dialect: Dialect) -> Result<DbProbeResponse, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("INSERT INTO instances … ON CONFLICT …")
                .bind(dialect.as_str())
                .execute(p)
                .await
                .map_err(|e| format!("db probe failed: {e}"))?;
            // SELECT + try_get → Ok(DbProbeResponse { … })
        }
        DbPool::MySql(p) => { /* ? placeholders, ON DUPLICATE KEY */ }
        DbPool::Sqlite(p) => { /* ?1, ON CONFLICT, strftime */ }
    }
}
```

**Error handling:** Return `Result<T, String>` with `"… failed: {e}"`; map to `AppError` only in API layer. **Never** branch on dialect in `oxidean-api`.

**Wire-up:** Export modules from `lib.rs` (lines 3–6 pattern: `pub mod dialect; pub mod migrate; …`). Add thin `Database` methods that require pool or return `"database not configured"` like `probe()` (lines 76–80).

---

### `crates/oxidean-api/src/rpc.rs` — extend dispatch (controller, request-response)

**Analog:** itself — `crates/oxidean-api/src/rpc.rs`

**Core match + validation** (lines 23–68):
```rust
pub async fn dispatch(db: &Database, req: RpcRequest) -> RpcResponse {
    match req.procedure.as_str() {
        "system.health" => { /* RpcResponse::ok(…) */ }
        "system.echo" => {
            let echo: EchoRequest = match serde_json::from_value(req.input) {
                Ok(v) => v,
                Err(e) => {
                    return RpcResponse::err(AppError::new(
                        "rpc.bad_input",
                        format!("invalid echo input: {e}"),
                    ))
                }
            };
            // payload size check → RpcResponse::err("rpc.payload_too_large", …)
            RpcResponse::ok(EchoResponse { message: echo.message })
        }
        "system.db_probe" => match db.probe().await {
            Ok(result) => RpcResponse::ok(result),
            Err(e) if e == "database not configured" => RpcResponse::err(AppError::new(
                "db.not_configured",
                "no database configured for this instance",
            )),
            Err(e) => {
                tracing::error!("db probe failed: {e}");
                RpcResponse::err(AppError::new("db.probe_failed", "database probe failed"))
            }
        },
        other => RpcResponse::err(AppError::new(
            "rpc.unknown_procedure",
            format!("unknown procedure: {other}"),
        )),
    }
}
```

**Session-aware extension (planner):** Change signature toward RESEARCH `RpcCtx` (db + email + settings + `Option<ResolvedSession>` + cookie change). Add arms: `auth.signup|login|logout|logout_all|me|provider_config`, `user.get_profile|update_profile`, `admin.auth.get_settings|update_settings`. Keep dotted namespaces and `AppError` codes.

---

### `crates/oxidean-api/src/app.rs` — mount routes + cookies (config / controller)

**Analog:** itself — `crates/oxidean-api/src/app.rs`

**Router + AppState** (lines 15–55):
```rust
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
}

pub fn router(db: Database, cors: CorsLayer) -> Router {
    let state = AppState { db };
    Router::new()
        .route("/health", get(health))
        .route("/api/rpc", post(rpc_http))
        .route("/api/rpc/ws", get(rpc_ws))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn rpc_http(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RpcRequest>,
) -> impl IntoResponse {
    // VERSION_HEADER check → BAD_REQUEST
    let resp = rpc::dispatch(&state.db, body).await;
    let status = match &resp {
        RpcResponse::Ok { .. } => StatusCode::OK,
        RpcResponse::Err { error, .. } if error.code == "rpc.unknown_procedure" => StatusCode::NOT_FOUND,
        RpcResponse::Err { .. } => StatusCode::BAD_REQUEST,
    };
    (status, Json(resp))
}
```

**Core pattern to copy:**
1. Expand `AppState` (email handle, auth settings cache, upload path).
2. Mount `GET /api/auth/workos/start|callback`, `GET /api/auth/oidc/start|callback`, `POST /api/user/avatar`, optional static `/uploads/avatars/*`.
3. In `rpc_http`: parse Cookie → resolve session → dispatch → attach `Set-Cookie` when login/logout requests it.
4. Keep CORS + Trace layers; add CookieManagerLayer if using `tower-cookies`.

**Auth/guard:** No middleware guard yet — session resolution inside `rpc_http` / route handlers; admin checks inside `admin.auth.*` handlers via `is_admin`.

---

### `crates/oxidean-api/src/cors.rs` (middleware — credentials already ready)

**Analog:** itself

**Credentials pattern** (lines 13–21, 45–49):
```rust
Ok(CorsLayer::new()
    .allow_credentials(true)
    .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
    .allow_headers(allowed_headers())
    .allow_origin(/* mirror_request in dev; AllowOrigin::list in prod */))
```

**Apply to:** Cookie sessions — do **not** switch to `AllowOrigin::any()` with credentials. May need `COOKIE` in allowed headers only if reading non-simple headers (cookies are automatic). Keep `OXIDEAN_ENV` → Secure cookie flag alignment with `main.rs` / session service.

---

### `crates/oxidean-api/src/auth/*` + `email/*` (service)

**Closest in-repo for handler shape:** `rpc.rs` input deserialize + `AppError` + `tracing::error!` on internal failure.

**Provider facade / EmailSender traits:** No in-repo trait analogs — use RESEARCH Pattern 2–3. Local provider uses password helpers (RESEARCH argon2 excerpt). Log sink:
```rust
tracing::info!(target: "oxidean.mail", to = %msg.to, subject = %msg.subject, "outbound email (log sink)");
```
(mirrors `tracing::error!` style in `rpc.rs` line 60).

**WorkOS / OIDC / lettre / Resend:** No analogs — follow RESEARCH Code Examples; keep dialect SQL out of these modules (call `oxidean-db` only).

---

### `crates/oxidean-api/src/bin/rpc_gen.rs` + `packages/api-client` (codegen client)

**Analog:** `crates/oxidean-api/src/bin/rpc_gen.rs` → emits `packages/api-client/src/index.ts`

**Client call pattern** (`packages/api-client/src/index.ts` lines 42–68):
```typescript
async function rpcCall<T>(opts: CreateClientOptions, procedure: string, input: unknown): Promise<RpcResult<T>> {
  const res = await fetchFn(`${opts.baseUrl.replace(/\/$/, "")}/api/rpc`, {
    method: "POST",
    credentials: opts.credentials ?? "include",
    headers: {
      "content-type": "application/json",
      [RPC_VERSION_HEADER]: String(RPC_VERSION),
    },
    body: JSON.stringify({ procedure, input }),
  });
  return (await res.json()) as RpcResult<T>;
}

export function createClient(opts: CreateClientOptions) {
  return {
    system: {
      health: () => rpcCall<HealthResponse>(opts, "system.health", {}),
      echo: (input: EchoRequest) => rpcCall<EchoResponse>(opts, "system.echo", input),
      dbProbe: () => rpcCall<DbProbeResponse>(opts, "system.db_probe", {}),
    },
  };
}
```

**Core pattern:** Extend template with `auth` / `user` / `admin.auth` namespaces + Query/Mutation helpers mirroring `systemHealthQueryOptions` (lines 74–83). **Never** hand-edit `index.ts` without updating `rpc_gen.rs`. Keep default `credentials: "include"`.

**Test analog:** `packages/api-client/src/index.test.ts` — mock `fetch`, assert version header + procedure path.

---

### `crates/oxidean-api/tests/auth_*.rs` (test, request-response)

**Analog:** `crates/oxidean-api/tests/rpc_http.rs` + `rpc_db_probe.rs`

**HTTP oneshot harness** (`rpc_http.rs` lines 8–70):
```rust
fn test_app() -> axum::Router {
    let cors = build_cors("development", None).expect("cors");
    router(Database::skipped(), cors)
}

#[tokio::test]
async fn system_health_ok() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(r#"{"procedure":"system.health","input":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // parse JSON → assert v["ok"] / v["data"]
}
```

**DB-backed round-trip** (`rpc_db_probe.rs` lines 54–72):
```rust
let dir = tempfile::tempdir().expect("tempdir");
let url = format!("sqlite:{}", dir.path().join("oxidean.db").display());
let db = Database::connect(&url).await.expect("connect sqlite");
db.migrate().await.expect("migrate sqlite");
let app = app_with(db);
```

**Apply to:** `auth_signup` / `auth_session` / `profile_avatar` — same cors+router, version header, JSON body; for sessions assert `Set-Cookie` / Cookie round-trip; for avatar POST multipart to `/api/user/avatar`.

---

### `crates/oxidean-db/tests/dialect_auth.rs` (test, CRUD)

**Analog:** `crates/oxidean-db/tests/dialect_probe.rs`

**Gated + serial pattern** (lines 10–37):
```rust
fn database_url() -> Option<String> { /* DATABASE_URL or None → skip */ }

#[tokio::test]
async fn migrate_and_probe_round_trip() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    // assert dialect-specific behavior
}
```

**Apply to:** After migrate, insert user/session via db helpers; prove signup/session on each dialect when CI sets `DATABASE_URL`.

---

### `apps/web/src/routes/{login,signup,dashboard,settings/profile,admin/auth}.tsx` (route)

**Analog:** `apps/web/src/routes/status.tsx`

**Route + head + client** (lines 1–13):
```tsx
import { createFileRoute } from "@octanejs/tanstack-router";
import { createClient, systemHealthQueryOptions } from "@oxidean/api-client";
import { useEffect, useState } from "octane";

export const Route = createFileRoute("/status")({
  component: StatusPage,
  head: () => ({ meta: [{ title: "Status · Oxidean" }] }),
});

const client = createClient({
  baseUrl: typeof window !== "undefined" ? window.location.origin : "http://127.0.0.1:8080",
  credentials: "include",
});
```

**UI chrome patterns:**
- Page title: `font-[family-name:var(--font-display)] text-[24px] font-semibold` (Heading role) — status line 70–72.
- Body muted: `text-[16px] text-muted-foreground`.
- Titles: `Page · Oxidean` (UI-SPEC).
- Auth column: `max-w-md` + `OxideanMark size={48}` (mark analog: `oxidean-mark.tsx`).
- Forms: `Button` / `Input` from `@/components/ui/*` (`h-11`, CVA variants).
- Admin selects: `SelectRoot` / `SelectTrigger` / `SelectPopup` / `SelectItem` from `ui/select.tsx`.
- Loading / error phases: local `useState` phase union like Status (lines 15–19, 32–52) — prefer inline `text-destructive` banners over toasts (UI-SPEC).

**Shell:** Routes render inside `__root.tsx` `SiteHeader` + `SiteFooter` — do not duplicate chrome.

---

### `apps/web/src/components/chrome.tsx` + landing CTAs (component)

**Analog:** itself — AccountActions currently disabled

**Placeholder to replace** (lines 26–41):
```tsx
<Button variant="ghost" className={…} disabled title="Coming soon">
  Sign in
</Button>
<Button variant="secondary" className={…} disabled title="Coming soon">
  Sign up
</Button>
```

**Core pattern:** Enable as `Link`/`navigate` to `/login` and `/signup` (keep `h-11`, ghost/secondary). Signed-in: replace with Dropdown Menu (new shadcn) — trigger ≥44px; menu items Profile / Dashboard / Auth settings (admin) / Log out. Reuse `ThemeSelect` placement before account group. Squircle avatar: `oxidean-squircle` class from mark.

**Landing** (`routes/index.tsx` lines 75–77): enable Get started → `/signup` (drop `disabled` / Coming soon); same at closing CTA ~146.

---

### `apps/web/src/components/ui/{label,checkbox,textarea,dropdown-menu}.tsx` (component)

**Analog:** `button.tsx` / `input.tsx`

**CVA + cn** (`button.tsx` lines 1–43):
```tsx
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";
// data-slot="…", h-11 touch targets, focus-visible:ring-2 focus-visible:ring-ring
```

**Input** (`input.tsx` lines 4–14): border-input, bg-card, text-[14px], disabled muted.

**Apply to:** Official shadcn Base UI path (preset base-nova) — match `h-11`, `rounded-md`, Label 14px / Body 16px. No third-party registries.

---

### `apps/web/public/sw.js` (do not narrow)

**Analog:** lines 42–50 — keep `/api/` bypass for auth callbacks + avatar:

```js
if (url.pathname.startsWith("/api/") || url.pathname === "/health") {
  event.respondWith(fetch(event.request));
  return;
}
```

If serving `/uploads/avatars/*` publicly, decide cache policy deliberately (default: network or short cache; never `/api/*`).

---

### `crates/oxidean-api/src/main.rs` — admin seed / env (config)

**Analog:** itself — ENV bootstrap for CORS + DB (lines 11–61)

```rust
let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
// DATABASE_URL → connect + OXIDEAN_AUTO_MIGRATE
```

**Apply to:** After migrate, if `OXIDEAN_ADMIN_EMAIL` + `OXIDEAN_ADMIN_PASSWORD` and no users → create `is_admin` user (RESEARCH discretion). Wire email provider ENV (`OXIDEAN_SMTP_URL`, `OXIDEAN_RESEND_API_KEY`, WorkOS keys) into `AppState`.

## Shared Patterns

### RPC procedure dispatch
**Source:** `crates/oxidean-api/src/rpc.rs`
**Apply to:** All `auth.*` / `user.*` / `admin.auth.*` handlers
- Match on `req.procedure.as_str()`
- `serde_json::from_value` → `rpc.bad_input`
- Domain errors → `AppError::new("auth.…"|`user.…"|`admin.…", message)`
- Unknown → `rpc.unknown_procedure`
- Internal DB failures → log + generic code (never leak sqlx details)

### Multi-dialect DB boundary
**Source:** `crates/oxidean-db/src/probe.rs` + `pool.rs` + `migrate.rs`
**Apply to:** users/sessions/identities/settings
- All SQL dialect branching stays in `oxidean-db`
- Migrations: three files, same names, `migration_parity` test
- API calls `Database` / module fns with `&DbPool` only

### CORS + credentials cookies
**Source:** `crates/oxidean-api/src/cors.rs` + `packages/api-client` `credentials: "include"`
**Apply to:** Login/logout Set-Cookie; browser session persistence
- `allow_credentials(true)` already set
- Cookie: `oxidean_session`; `HttpOnly; Path=/; SameSite=Lax`; `Secure` when not development

### HTTP integration tests
**Source:** `crates/oxidean-api/tests/rpc_http.rs`, `rpc_db_probe.rs`
**Apply to:** auth_signup, auth_session, profile_avatar
- `build_cors("development")` + `router(...)` + `ServiceExt::oneshot`
- Always send `Oxidean-RPC-Version: 1`
- SQLite tempfile + migrate for persistence tests

### Web route + API client
**Source:** `apps/web/src/routes/status.tsx`
**Apply to:** login, signup, dashboard, profile, admin auth
- `createFileRoute` + `head` title `… · Oxidean`
- `createClient({ baseUrl: window.location.origin, credentials: "include" })`
- Phase-style local state for loading/error; Heading/Body typography tokens

### Chrome & CTAs
**Source:** `apps/web/src/components/chrome.tsx`, `routes/index.tsx`
**Apply to:** D-17 wiring
- Enable Sign in / Sign up / Get started; signed-in account menu with this-device logout
- Keep `h-11` touch targets and ghost/secondary variants

### Error codes
**Source:** `oxidean_core::AppError` + existing `rpc.*` / `db.*` codes
**Apply to:** Auth domain — prefer stable codes (`auth.invalid_username`, `auth.reserved_username`, `db.not_configured`, …) matching RESEARCH validation sketch

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `crates/oxidean-api/src/auth/workos.rs` | service | request-response | No WorkOS/OIDC usage yet — use RESEARCH + official `workos` SDK |
| `crates/oxidean-api/src/auth/oidc.rs` | service | request-response | No OIDC client in repo — use `openidconnect` RESEARCH sketch |
| `crates/oxidean-api/src/auth/password.rs` | utility | transform | No password hashing — use RESEARCH argon2 excerpt |
| `crates/oxidean-api/src/email/smtp.rs` | service | request-response | No mailer — use RESEARCH lettre example |
| `crates/oxidean-api/src/email/resend.rs` | service | request-response | No HTTP mail — use RESEARCH Resend + User-Agent |
| `crates/oxidean-api/src/email/mod.rs` | service | event-driven | No `EmailSender` trait yet — RESEARCH Pattern 3 |

Planner should pull those from `04-RESEARCH.md` Code Examples / Architecture Patterns.

## Metadata

**Analog search scope:** `crates/oxidean-api`, `crates/oxidean-db`, `crates/oxidean-core`, `packages/api-client`, `apps/web/src`, `apps/web/public/sw.js`
**Files scanned:** ~45 primary sources (RPC, CORS, app, migrations, pool/dialect/probe, web routes/chrome/ui, api-client, tests)
**Pattern extraction date:** 2026-09-10
)
