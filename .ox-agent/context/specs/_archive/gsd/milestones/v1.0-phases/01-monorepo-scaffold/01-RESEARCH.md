# Phase 1: Monorepo Scaffold - Research

**Researched:** 2026-09-09
**Domain:** Rust + Octane TanStack Start monorepo, Docker Compose, typed RPC codegen
**Confidence:** MEDIUM-HIGH (stack versions verified; RPC framework choice has maintenance caveats)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** `apps/web` + `crates/*` + `packages/api-client`
- **D-02:** Crates: `oxidean-api`, `oxidean-core`, `oxidean-db` (adapter shape; multi-dialect proof Phase 2)
- **D-03:** Turborepo + Cargo workspace + root Makefile
- **D-04/D-05:** Bun via Corepack preferred; Corepack pnpm fallback; CI matches lockfile
- **D-06–D-10:** Compose web+api+postgres; MySQL profile; SQLite via env; Traefik; Vite/TanStack proxy for `make dev`
- **D-11–D-21:** HTTP+WS `/api/rpc` + `/api/rpc/ws`; `system.health` + `system.echo`; dotted namespaces; watch+explicit codegen; CI sync; typed errors; WS transport-only; `Oxidean-RPC-Version: 1`; CORS (dev any / prod `OXIDEAN_CORS_ORIGINS`); TanStack Query helpers, no custom hooks package
- **D-22–D-26:** Marketing `/` + public `/status` (health only); echo tests/CI only; footer Status link
- **UI-SPEC:** shadcn+Base UI, Tailwind v4, Sora + Source Sans 3, system/light/dark theme, CTAs Get started / Explore Oxidean, color tokens as specified

### Claude's Discretion
- Exact RPC library (rspc/specta-style or equivalent) as long as D-11–D-21 hold
- Axum vs alternative HTTP stack
- shadcn init preset details; Compose healthcheck wiring; Makefile/turbo naming; `.env.example`

### Deferred Ideas (OUT OF SCOPE)
- Status history, auth gate on `/status`, real search/auth, Phase 3 brand polish, Phase 2 multi-DB proof, WS subscriptions, multi-version RPC routing, OAuth

</user_constraints>

<architectural_responsibility_map>
## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Marketing landing + chrome + theme | Browser/Client (`apps/web`) | Frontend Server (SSR via Start) | UI-SPEC; Octane Start owns SSR/hydration |
| `/status` live health display | Browser/Client | API/Backend | UI calls typed client → `system.health` |
| Typed RPC procedures | API/Backend (`oxidean-api`) | `packages/api-client` (codegen) | Rust SoT; TS consumes generated client |
| Protocol headers/CORS/version | API/Backend | Traefik (ingress) | CORS + version rejection on API; Traefik routes |
| DB adapter boundary | API/Backend (`oxidean-db`) | Database/Storage (Postgres) | Uniform adapter; Phase 1 ping/connect only |
| Compose bring-up | CDN/Static N/A — ops/Compose | Traefik + web + api + postgres | PLAT-01 operator path |
| Codegen watch + CI sync | Dev tooling / CI | `packages/api-client` | D-15 |

</architectural_responsibility_map>

<research_summary>
## Summary

Phase 1 is a greenfield scaffold: JS monorepo (Bun + Turborepo) hosting `@octanejs/tanstack-start` web app, Rust Cargo workspace for API/core/db, Docker Compose with Traefik, and a typed RPC surface with codegen into `packages/api-client`.

**Critical finding:** Upstream **rspc is no longer maintained** (maintainer stepping back; repo WARNING). PROJECT/CONTEXT ask for “rspc/specta-style,” which is still satisfied by **specta-based codegen + Axum** without depending on abandoned rspc. Recommended path: **Axum 0.8 + specta (2.x RC line used by modern specta-typescript) + Oxidean-owned procedure router** implementing locked paths, `Oxidean-RPC-Version`, typed errors, and HTTP+WS. Alternative worth a spike: **fnrpc** (specta + Axum + `@fnrpc/tanstack-query`) if its transport can meet `/api/rpc/ws` requirements; otherwise use fnrpc patterns only as reference.

Frontend: `@octanejs/tanstack-start@0.1.44` is published and self-contained (Vite plugin). shadcn supports TanStack Start + Tailwind v4; init during Phase 1 per UI-SPEC. Theme tokens (system default + light/dark force) land now; polish is Phase 3.

**Primary recommendation:** Scaffold monorepo → Axum API with Oxidean RPC + specta codegen → Octane Start web with shadcn/Base UI per UI-SPEC → Compose+Traefik → Vitest/cargo tests + CI codegen drift check.

</research_summary>

<standard_stack>
## Standard Stack

### Core

| Library | Version | Purpose | Why Standard | Confidence |
|---------|---------|---------|--------------|------------|
| `@octanejs/tanstack-start` | 0.1.44 | Octane TanStack Start (Vite/Rsbuild) | Locked PLAT-04; published metaframework | HIGH [VERIFIED: npm] |
| `@octanejs/tanstack-router` | (peer of start) | File routes / router binding | Required by Octane Start README | HIGH [CITED: octane README] |
| `vite` | 8.x (8.2.2 latest) | Dev/build for web | Octane Start Vite plugin | HIGH [VERIFIED: npm] |
| `react` / `react-dom` | 19.x (assume latest 19) | UI runtime | Start/React 19 ecosystem | MEDIUM [ASSUMED] |
| `@tanstack/react-query` | 5.102.8 | Query helpers for api-client | Locked D-21 | HIGH [VERIFIED: npm] |
| `turbo` | 2.10.12 | JS task orchestration | Locked D-03 | HIGH [VERIFIED: npm] |
| `bun` / Corepack | latest stable | Package manager | Locked D-04 | HIGH [ASSUMED runtime] |
| `axum` | 0.8.9 | HTTP+WS server | De facto Rust web; rspc-axum era target | HIGH [VERIFIED: crates.io] |
| `tokio` | 1.53.1 | Async runtime | Axum dependency | HIGH [VERIFIED: crates.io] |
| `tower-http` | 0.7.1 | CORS, trace, etc. | CORS D-19/D-20 | HIGH [VERIFIED: crates.io] |
| `serde` / `serde_json` | 1.0.x | Serialization | RPC JSON | HIGH [VERIFIED: crates.io] |
| `specta` | 2.0.0-rc.25 (line) | Type export SoT | specta-style SoT; rspc pins RC line | HIGH [VERIFIED: crates.io] |
| `specta-typescript` | 0.0.12 | TS emit | Codegen into api-client | HIGH [VERIFIED: crates.io] |
| `sqlx` | 0.9.0 | DB access behind `oxidean-db` | Postgres default; SQLite/MySQL later | HIGH [VERIFIED: crates.io] |
| `tracing` / `tracing-subscriber` | 0.1.x / 0.3.x | API logging | Ops baseline | HIGH [VERIFIED: crates.io] |
| `tailwindcss` | 4.3.3 | Styling (CSS-first) | PLAT-11 | HIGH [VERIFIED: npm] |
| `shadcn` CLI | 4.21.0 | Component init (Base UI) | PLAT-10 + UI-SPEC | HIGH [VERIFIED: npm] |
| `@base-ui/react` | 1.8.0 | Headless primitives | UI-SPEC Base UI | HIGH [VERIFIED: npm] |
| `lucide-react` | 1.43.0 | Icons | UI-SPEC | HIGH [VERIFIED: npm] |
| `vitest` | 5.0.0 | JS/TS tests | Echo client + codegen checks | HIGH [VERIFIED: npm] |
| Traefik | v3.x image (pin minor in Compose) | Reverse proxy | Locked D-09 | MEDIUM [VERIFIED: Docker Hub has v1/v3 tags — pin `traefik:v3.3` or current v3 in plan] |
| Postgres | 16-alpine (recommend) | Default DB | Compose default | MEDIUM [ASSUMED common pin] |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `@fontsource-variable/sora` | 5.3.0 | Display font | UI-SPEC typography |
| `@fontsource-variable/source-sans-3` | 5.3.0 | Body font | UI-SPEC typography |
| `rspc` / `rspc-axum` | 0.4.1 / 0.3.0 | Legacy option | **Not recommended as primary** — unmaintained [CITED: github.com/specta-rs/rspc] |
| `fnrpc` + `@fnrpc/client` / `@fnrpc/tanstack-query` | ~0.3.4 / 0.4.6 | specta RPC + TQ helpers | Optional spike if WS story fits D-11/D-17 [VERIFIED: crates.io/npm] |
| `cargo-watch` / `watchexec` | — | Regen watch helper | `make dev` rpc watch |
| `nextest` | — | Rust test runner (optional) | Faster cargo tests in CI |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Oxidean RPC + specta | rspc 0.4.1 | Faster tRPC-like DX but **unmaintained** — reject as primary |
| Oxidean RPC + specta | fnrpc | Closer to D-21 out of the box; confirm HTTP+WS mounts and version header hooks |
| Axum | Actix / Poem | Axum is ecosystem default with tower-http CORS |
| Bun | pnpm via Corepack | Locked fallback only |
| Traefik | Caddy / nginx | Traefik locked |
| sqlx | SeaORM / diesel | sqlx fits multi-dialect Phase 2; keep adapter thin in Phase 1 |

**Installation (illustrative):**
```bash
# JS (Bun)
bun add @octanejs/tanstack-start @octanejs/tanstack-router @tanstack/react-query
bun add -d turbo vitest vite tailwindcss @tailwindcss/vite
# Rust workspace members depend on axum tokio tower-http serde specta specta-typescript sqlx tracing
```

</standard_stack>

<architecture_patterns>
## Architecture Patterns

### System Architecture Diagram

```
Browser (apps/web)
  ├─ / marketing + chrome (theme: system|light|dark)
  └─ /status → TanStack Query → packages/api-client
         │
         ├─ HTTP  POST/GET  /api/rpc     (+ Oxidean-RPC-Version: 1)
         └─ WS            /api/rpc/ws
                │
         Traefik (Compose) ─── or Vite proxy (make dev)
                │
         oxidean-api (Axum)
                ├─ RPC router: system.health, system.echo
                ├─ CORS layer (dev open / prod allowlist)
                ├─ oxidean-core (shared types/errors)
                └─ oxidean-db → Postgres (default)
```

### Codegen flow

```
Rust procedures (specta Type collection)
        │
   make rpc-gen / watch
        ▼
packages/api-client (types + client + queryOptions/mutationOptions)
        │
   CI: git diff --exit-code after regen
```

### Recommended patterns
- **Single API binary** in Phase 1 (`oxidean-api`); core/db as libs
- **Procedure names** as strings `"system.health"` / `"system.echo"` (dotted)
- **Error type** `AppError { code, message, data? }` exported via specta
- **Healthcheck:** Compose `HEALTHCHECK` hitting cheap HTTP health (either dedicated `/healthz` wrapping same logic or RPC) — prefer tiny `/healthz` for probes + RPC for app [ASSUMED ops best practice]
- **Theme:** CSS variables for light/dark matching UI-SPEC; `class` or `data-theme` on `<html>`; default system via `matchMedia` + persisted override

### Anti-patterns
- Depending on unmaintained rspc as long-term SoT
- Putting echo UI on `/status`
- Generating client into `apps/web` instead of `packages/api-client`
- Skipping CI drift check
- Flat single-color landing (violates UI-SPEC)

</architecture_patterns>

<dont_hand_roll>
## Don't Hand-Roll

| Problem | Use Instead | Why |
|---------|-------------|-----|
| CORS | `tower-http::cors` | Correct preflight/credentials |
| TS type emit from Rust | specta + specta-typescript (or fnrpc codegen) | Avoid drift vs hand-written types |
| Component primitives | shadcn + Base UI | PLAT-10 |
| CSS design tokens wiring | Tailwind v4 `@theme` / CSS variables | PLAT-11 |
| Task running | Turborepo + Makefile | Locked |
| Reverse proxy | Traefik | Locked |
| Unit test runner (JS) | Vitest | Ecosystem default with Vite |

**Still hand-roll (acceptable):** Oxidean RPC dispatch protocol (JSON procedure call envelope) and WS framing — keep small, documented, and tested — unless fnrpc spike proves it maps cleanly to D-11–D-21.

</dont_hand_roll>

<common_pitfalls>
## Common Pitfalls

| Pitfall | Why It Happens | How to Avoid | Severity |
|---------|----------------|--------------|----------|
| Adopting rspc then hitting abandonment | Name match to PROJECT wording | Use specta-style Oxidean router; document “rspc-style” in README | HIGH |
| shadcn init fails on Tailwind v4 / tsconfig refs | CLI preflight quirks | Follow ui.shadcn.com TanStack Start guide; ensure `@/*` alias; Base UI preset | HIGH [CITED: shadcn TanStack docs / CLI issues] |
| WS not proxied by Vite/Traefik | Upgrade headers / path mismatch | Explicit proxy `ws: true` for `/api/rpc/ws`; Traefik WebSocket sticky not required for stateless RPC | HIGH |
| CORS credentials + `*` origin | Browser forbids combination | Dev: reflect request origin or use regex; Prod: explicit allowlist only | HIGH |
| Codegen not in CI | Easy to forget | `make rpc-gen` + `git diff --exit-code` job | HIGH |
| Bun/turbo lockfile mismatch on CI | Different package managers | Document Corepack Bun; fallback path tested | MEDIUM |
| sqlx offline/migrate complexity in Phase 1 | Over-scoping Phase 2 | Phase 1: connect + ping only; migrations thin or stub | MEDIUM |
| Theme FOUC | System theme flash | Inline boot script or SSR cookie preference | LOW |
| Font loading CLS | Late fonts | fontsource self-host or `font-display: swap` | LOW |

</common_pitfalls>

<code_examples>
## Code Examples

### Axum CORS + RPC mount sketch [ASSUMED shape]

```rust
// Conceptual — planner/executor refine
let cors = CorsLayer::new()
    .allow_credentials(true)
    // dev: permissive; prod: AllowOrigin::list(parsed OXIDEAN_CORS_ORIGINS)
    ;

let app = Router::new()
    .route("/healthz", get(healthz))
    .route("/api/rpc", post(rpc_http))
    .route("/api/rpc/ws", get(rpc_ws_upgrade))
    .layer(cors)
    .layer(middleware::from_fn(require_rpc_version)); // Oxidean-RPC-Version: 1
```

### Vite proxy sketch [ASSUMED]

```ts
server: {
  proxy: {
    "/api/rpc": { target: "http://127.0.0.1:8080", changeOrigin: true },
    "/api/rpc/ws": { target: "ws://127.0.0.1:8080", ws: true },
  },
}
```

### CI codegen sync [ASSUMED]

```bash
make rpc-gen
git diff --exit-code -- packages/api-client
```

</code_examples>

<state_of_art>
## State of the Art

| Approach | What Changed | Impact on Phase |
|----------|--------------|-----------------|
| rspc unmaintained (2025–2026) | Maintainer stepped back | Do not center stack on rspc; keep specta-style |
| Octane publishes `@octanejs/tanstack-start` | Self-contained Start package | Use npm package + Vite plugin; no vendoring |
| shadcn + Base UI + Tailwind v4 | Official Start install path | Init in Phase 1; expect CLI friction → document |
| fnrpc emerging | specta + TQ helpers | Optional evaluation; not locked |

</state_of_art>

<recommendations>
## Recommendations for Planning

### Wave suggestions (planner)

1. **Wave 0 / Plan 01 — Repo skeleton:** root Makefile, Cargo workspace, Turborepo+Bun package.json, `.env.example`, README, CI stub
2. **Plan 02 — Rust API + RPC + codegen:** axum server, procedures, specta emit → `packages/api-client`, version header, CORS, echo/health tests (HTTP+WS)
3. **Plan 03 — Web app:** Octane Start `apps/web`, shadcn/Base UI/Tailwind v4, theme, landing + `/status` per UI-SPEC
4. **Plan 04 — Compose + Traefik:** web/api/postgres, healthchecks, CORS prod env, MySQL profile stub, `make up` proof
5. **Plan 05 — CI glue:** Compose validate/build, codegen drift, vitest+cargo test matrix (sqlite/postgres as available)

### RPC decision for planner (discretion lock-in)

**Choose:** Oxidean thin RPC on Axum + specta/specta-typescript (primary).  
**Spike budget (optional, ≤½ day):** fnrpc only if it clearly supports custom mount paths + version middleware + WS duplex for same procedures; else do not block Phase 1.

### Threat highlights for `<threat_model>`
- CORS misconfig (origin reflection bugs, credentials leakage)
- Unauthenticated public `/api/rpc` (expected Phase 1 — rate limit optional; no secrets in health/echo)
- Supply chain: pin Traefik/Postgres digests where practical
- Generated client tampering: CI drift check

</recommendations>

<validation_architecture>
## Validation Architecture

### Test frameworks
- **Rust:** `cargo test` (workspace) — RPC unit/integration with axum-test or hyper client; tokio-tungstenite for WS
- **JS:** `vitest` — api-client echo roundtrip (mocked server or live); optional Playwright later (manual for landing polish)
- **Ops:** `docker compose config` + `docker compose up --wait` smoke

### Quick vs full
- **Quick:** `cargo test -p oxidean-api --lib` + `bunx vitest run packages/api-client` (~30–90s)
- **Full:** workspace cargo test + vitest + `make rpc-gen` drift + compose up health curl (~3–8 min)

### Requirement → verification map

| Requirement | Automated verification |
|-------------|------------------------|
| PLAT-01 | Compose up; curl `/healthz` and/or RPC health through Traefik; web container serves `/` |
| PLAT-04 | `apps/web` depends on `@octanejs/tanstack-start`; build succeeds |
| PLAT-05 | `oxidean-api` Rust binary builds and serves |
| PLAT-06 | specta codegen writes `packages/api-client`; watch target exists; CI drift job |
| PLAT-10 | shadcn components + Base UI imports present; button/input used on landing/status |
| PLAT-11 | Tailwind v4 CSS entry (`@import "tailwindcss"`) — no JS tailwind.config as SoT |

### Wave 0 test stubs
- `crates/oxidean-api/tests/rpc_http.rs` — health + echo + version reject
- `crates/oxidean-api/tests/rpc_ws.rs` — health + echo over WS
- `packages/api-client/src/*.test.ts` — types/helpers smoke
- Script/job `scripts/check-rpc-sync.sh`

### Sampling
- After each task commit: quick tests for touched package
- After each plan wave: full relevant suite
- No watch-mode flags in CI

</validation_architecture>

<open_questions>
## Open Questions

1. **fnrpc vs Oxidean thin RPC** — recommend thin RPC unless executor spike proves fnrpc fits WS+header constraints in <4h.
2. **Exact Traefik image tag** — pin concrete v3.x at execute time from Docker Hub.
3. **Whether `/healthz` is separate from RPC** — recommend yes for Compose probes.

None of these block writing PLAN.md; treat as planner discretion with defaults above.

</open_questions>

---

*Phase: 01-monorepo-scaffold*
*Research: 2026-09-09*
