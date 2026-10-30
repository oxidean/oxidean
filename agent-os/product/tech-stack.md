# Tech stack

One Bun + Turborepo workspace (`apps/*`, `packages/*`) and one Rust Cargo
workspace (`crates/*`). Do not invent alternatives; extend existing patterns.

| Layer | Tech |
|-------|------|
| Web UI | **Octane** (`.tsrx`) islands on **Astro** static shells (`apps/web/src/pages/**`); TanStack Query / Form via `@octanejs/*`; file pickers via `@octanejs/dropzone`; ShadCN + Base UI + Tailwind CSS v4 |
| Web serving | **`oxidean-web`** Rust binary (`crates/oxidean-web`) — serves Astro `dist`, injects theme/title/session metas per request, gates protected routes, reverse-proxies API prefixes via `OXIDEAN_API_ORIGIN` |
| API | Rust Axum + typed JSON RPC (`oxidean-api`), versioned `Oxidean-RPC-Version` wire contract |
| Domain types | `oxidean-core` — types only, no I/O |
| Persistence | `oxidean-db` — one `Database` façade over Postgres / MySQL / SQLite (sqlx); dialect SQL lives only here |
| Git engine | `oxidean-git` `GitBackend` trait; shipped `CliGitBackend` drives system `git` ≥ 2.5; future `gix` adapter documented as an option |
| TS RPC client | `@oxidean/api-client` — **generated** by `make rpc-gen`; never hand-edited |
| CLI | `ox` (`crates/oxidean-cli`) — companion CLI over the same RPC surface |
| Email | `EmailSender` trait: log sink (unconfigured), SMTP (`OXIDEAN_SMTP_URL`), Resend |
| Runtime | Docker Compose (local + self-host, Traefik edge); Oxidean Cloud on Railway (Caddy gateway, same images, `.railway/railway.ts` IaC) |
| Package manager | Bun 1.4.0 (`bun install --frozen-lockfile`; `npm ci` only inside `.railway/`); GNU Make is the task façade |
| CI | GitHub Actions inside self-owned digest-pinned `oxidean-ci:*` images; Rust via cargo-nextest, web via Vitest 5 + Playwright Chromium |

Canonical docs: [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md),
[`docs/API.md`](../../docs/API.md), [`docs/CONFIGURATION.md`](../../docs/CONFIGURATION.md).
