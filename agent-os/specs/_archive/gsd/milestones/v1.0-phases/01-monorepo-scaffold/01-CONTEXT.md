# Phase 1: Monorepo Scaffold - Context

**Gathered:** 2026-09-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Developers and operators can bring up a Rust backend + Octane TanStack Start frontend via Docker Compose with ShadCN/Base UI/Tailwind v4 and typed RPC codegen regenerating in local development.

**Requirements:** PLAT-01, PLAT-04, PLAT-05, PLAT-06, PLAT-10, PLAT-11

**Success criteria (from ROADMAP):**
1. Operator can start the Oxidean stack with Docker Compose and reach a healthy web UI and API
2. Web UI is an OctaneJS app on TanStack Start (`@octanejs/tanstack-start`)
3. UI uses ShadCN + Base UI components and Tailwind CSS v4 with CSS-based configuration
4. Backend forge/API process is a Rust service reachable from the UI
5. Changing a Rust RPC procedure regenerates the TypeScript client/types in watch-friendly local development

</domain>

<decisions>
## Implementation Decisions

### A — Workspace layout
- **D-01:** Monorepo layout: `apps/web` (TanStack Start / Octane), `crates/*` (Rust), `packages/api-client` (generated TS client)
- **D-02:** Rust crates for Phase 1: `oxidean-api` (HTTP/WS server + RPC mount), `oxidean-core` (shared domain/types), `oxidean-db` (uniform DB adapter; Postgres default in Compose — full multi-dialect proof is Phase 2)
- **D-03:** JS orchestration via Turborepo; Rust via Cargo workspace; root `Makefile` for operator/dev entrypoints (`make dev`, Compose, rpc-gen, etc.)

### B — Package manager
- **D-04:** Prefer **Bun via Corepack** for JS workspace; if Bun/Corepack path is unsuitable, fall back to **Corepack pnpm + Turborepo**
- **D-05:** Lockfiles and CI must match the chosen JS package manager; document the fallback in README/Makefile

### C — Compose topology
- **D-06:** Default Compose stack: `web` + `api` + `postgres`
- **D-07:** MySQL via Compose **profile** (not default); SQLite via env/file path (not a Compose DB service)
- **D-08:** `oxidean-db` is the uniform adapter boundary so API code does not dialect-branch ad hoc
- **D-09:** **Traefik** reverse proxy in Compose for unified ingress
- **D-10:** Local `make dev` uses Vite/TanStack **dev proxy** to the API (in addition to Compose/Traefik path)

### D — RPC transport & client
- **D-11:** Transports: **HTTP + WebSocket**
- **D-12:** Mount paths: `/api/rpc` (HTTP) and `/api/rpc/ws` (WebSocket)
- **D-13:** Phase 1 procedures: `system.health` and `system.echo` (typed request/response codegen proof)
- **D-14:** Procedure naming: **dotted namespaces from day one** (`system.*`)
- **D-15:** Codegen: **watch regen** during `make dev` writing into `packages/api-client`, **plus** explicit `make`/turbo regen; **CI fails if generated client is out of sync**
- **D-16:** Errors: typed app errors with stable `code` + `message` + optional `data`; typed error union in the client
- **D-17:** Phase 1 WS usage: **transport only** — same `system.health` / `system.echo` over `/api/rpc/ws`; **no subscriptions** yet
- **D-18:** Protocol versioning: header `Oxidean-RPC-Version: 1`; server **rejects mismatch**; paths stay unversioned; **no multi-version routing** in Phase 1
- **D-19:** Browser: **explicit CORS** for local + Compose; credentials supported for future session cookies
- **D-20:** CORS policy: **dev = allow any origin**; **prod/Compose = require** `OXIDEAN_CORS_ORIGINS` (comma-separated allowlist)
- **D-21:** `packages/api-client` exports generated client + **TanStack Query helpers** (`queryOptions` / `mutationOptions`); **no custom hooks package**

### E — Scaffold proof surface
- **D-22:** Product shape (intent): marketing landing at `/` with header (branding, public search, auth); separate `/status` for status report + history
- **D-23:** Phase 1 slice: **credible marketing draft** on `/` (real Oxidean positioning copy + mark in header); Search/Auth are **placeholders**; `/status` is **health-focused** live `system.health` only — **no echo UI**, **no status history persistence**
- **D-24:** `system.echo` proven via **tests/CI only** (HTTP + WS); not exposed in UI
- **D-25:** `/status` is **public** in Phase 1; auth gate + history come later
- **D-26:** `/status` linked from **footer** (“Status”) — keeps header marketing-focused *(defaulted when user said continue working; change if undesired)*

### Claude's Discretion
- Exact rspc/specta (or equivalent) crate versions and Axum vs alternative HTTP stack, as long as D-11–D-21 hold
- ShadCN/Base UI starter set and landing/status visual polish within “credible draft” (full brand/theme system is Phase 3)
- Compose healthcheck wiring (prefer probing API readiness compatible with `system.health`)
- Whether `/status` UI uses HTTP only while WS is covered by tests (recommended)
- Makefile / turbo task naming, as long as `make dev`, explicit rpc-gen, and CI sync check exist
- `.env.example` shape for `OXIDEAN_CORS_ORIGINS` and DB URLs

</decisions>

<specifics>
## Specific Ideas

- Homepage is a **marketing landing**, not a diagnostic console
- Header includes **public search**, **branding**, and **auth** (placeholders acceptable in Phase 1)
- Logged-in / richer **status report + history** is the longer-term `/status` vision; Phase 1 only needs live health
- Brand mark: `brand/oxidean-mark.png` (blue/orange X on black)
- Do not name the product bare “Octane”

</specifics>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Product & requirements
- `.planning/PROJECT.md` — product identity, stack constraints, dual-mode Docker Compose distribution
- `.planning/REQUIREMENTS.md` — PLAT-01, PLAT-04, PLAT-05, PLAT-06, PLAT-10, PLAT-11 (Phase 1 mapped)
- `.planning/ROADMAP.md` — Phase 1 goal, success criteria, dependency order (Phase 2 multi-DB, Phase 3 brand/theme)

### Brand
- `brand/oxidean-mark.png` — primary mark for header/favicon until vector set exists
- `brand/README.md` — brand asset notes

### Stack pointers (external)
- Octane TanStack Start package: `@octanejs/tanstack-start` (OctaneJS monorepo)
- UI: ShadCN + Base UI + Tailwind CSS v4 (CSS-based config as source of truth)

No phase-local SPEC.md or ADRs yet — decisions above are the implementation lock.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `brand/oxidean-mark.png` — use in landing header / favicon scaffold
- Planning docs only otherwise; **no application source tree yet** (greenfield scaffold)

### Established Patterns
- None in-repo yet — this phase establishes monorepo, Compose, and RPC conventions

### Integration Points
- Traefik (Compose) and Vite/TanStack proxy (`make dev`) both must reach `/api/rpc` and `/api/rpc/ws`
- `apps/web` consumes `@oxidean/api-client` (or equivalent package name) via TanStack Query helpers
- CI must run codegen sync check + echo transport tests + Compose bring-up path for PLAT-01

</code_context>

<deferred>
## Deferred Ideas

- Full **status history** persistence and richer status report UX — after auth / storage maturity (not Phase 1)
- **Auth gate** on `/status` when logged-in-only history exists — auth phases
- Real **public search** and **auth** header behavior — later phases (placeholders only now)
- Brand shell polish, system/light/dark theme persistence — **Phase 3**
- Multi-DB operator proof (SQLite + Postgres + MySQL migrations/flows) — **Phase 2** (`oxidean-db` adapter lands in Phase 1 for shape only as needed)
- RPC **subscriptions** over WebSocket — later than Phase 1 transport proof
- Multi-version RPC protocol routing — not until external/multi-client version coexistence is required
- OAuth — out of v1 per PROJECT.md

</deferred>

---

*Phase: 01-monorepo-scaffold*
*Context gathered: 2026-09-09*
