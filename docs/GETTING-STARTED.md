<!-- generated-by: gsd-doc-writer -->
# Getting started

Get Oxidean running locally: install prerequisites, clone the monorepo, then bring up the stack with Docker Compose (`make up`) or run the API and web app on the host (`make dev`).

## Prerequisites

| Tool | Requirement | Notes |
|------|-------------|--------|
| [Bun](https://bun.sh) | `bun@1.4.0` | Pinned in root `package.json` `packageManager`. Enable Corepack or install Bun so `bun install` matches the lockfile. |
| Rust / `cargo` | Stable Rust with edition **2021** | Workspace members under `crates/`. Compose API image builds with `rust:1-bookworm`. No `rust-toolchain.toml` in-repo — use a current stable toolchain that can build the workspace. |
| Docker + Docker Compose | Docker Engine with Compose v2 (`docker compose`) | Required for `make up`, dialect overlays, smoke tests, and `make up-dev-auth`. |
| Git | Any recent Git | Clone and local workflow. |

Optional but useful:

- `cargo-nextest` — preferred by `make test` (falls back to `cargo test` if missing)
- A free host port **80** for Traefik (Compose), or **3000** / **8080** for host `make dev`

## Installation steps

1. Clone the repository and enter the project root:

```bash
git clone git@github.com:oxidean/oxidean.git
cd Oxidean
```

2. Install JavaScript workspace dependencies and confirm the Cargo workspace resolves:

```bash
corepack enable
bun install
cargo metadata -q
```

3. Copy Compose / local environment defaults (do not commit `.env`):

```bash
cp .env.example .env
```

Default `.env` targets the Compose Postgres service (`postgres://…@postgres:5432/…`). For host-only `make dev` against a DB on localhost, adjust `DATABASE_URL`, `OXIDEAN_ENV=development`, and CORS as commented in `.env.example`.

## First run

### Option A — Docker Compose (recommended)

Brings up Traefik on `:80`, web, API, and PostgreSQL:

```bash
make up
make smoke
```

Open [http://localhost](http://localhost) (Traefik `Host(localhost)`). Tear down with `make down`.

Other dialects:

```bash
make up-mysql && make smoke-mysql
make up-sqlite && make smoke-sqlite
```

Details: [database.md](database.md).

### Option B — Host development (`make dev`)

Prints the two-terminal commands after regenerating the TypeScript RPC client:

```bash
make rpc-gen
# terminal 1
OXIDEAN_ENV=development API_BIND=127.0.0.1:8080 cargo run -p oxidean-api --bin oxidean-api
# terminal 2
bun run --filter @oxidean/web dev
```

Or run `make dev` to regenerate the client and echo the same commands. Vite serves the web app on `:3000` and proxies `/api`, `/uploads`, and `/health` to the API on `:8080`.

List all Make targets with `make help`.

## Common setup issues

| Issue | What you see | Fix |
|-------|----------------|-----|
| **Docker / Compose missing** | `docker: command not found` or Compose errors from `make up` | Install Docker Engine and Compose v2 so `docker compose` works. Host-only path: use Option B (`make rpc-gen` + two terminals) with a reachable `DATABASE_URL`. |
| **Port already in use** | Bind failures on **80** (Traefik), **2222** (Git SSH), **3000** (Vite/web), or **8080** (API) | Stop the conflicting process, or change binds (`API_BIND`, Vite port) for host dev. Compose exposes Traefik as `80:80` and SSH as `2222:2222` (`OXIDEAN_SSH_ENABLED=false` to disable SSH). |
| **DB dialect mismatch** | Boot exit when `OXIDEAN_DB_DIALECT` disagrees with `DATABASE_URL`, or smoke expects another dialect | Keep scheme and dialect aligned (`postgres://` → postgres, `mysql://` → mysql, `sqlite:` → sqlite). Default stack is Postgres; use `make up-mysql` / `make up-sqlite` (and matching smoke targets) instead of mixing overlays. See [CONFIGURATION.md](CONFIGURATION.md) and [database.md](database.md). |
| **Missing `.env`** | Compose/API using unexpected defaults or empty secrets | `cp .env.example .env` and edit before `make up`. Never commit `.env`. |

## Next steps

- [../CONTRIBUTING.md](../CONTRIBUTING.md) — how to contribute (humans + agents)
- [CODE_PRACTICES.md](CODE_PRACTICES.md) — coding conventions
- [../AGENTS.md](../AGENTS.md) — agent orientation (Octane ≠ React)
- [DEVELOPMENT.md](DEVELOPMENT.md) — day-to-day builds, lint, and contributor workflow
- [TESTING.md](TESTING.md) — unit, integration, and e2e commands
- [CONFIGURATION.md](CONFIGURATION.md) — environment variables and Compose overlays
- [guides/administration.md](guides/administration.md) — administering users, sessions, and invites
- [dev-auth.md](dev-auth.md) — Mailpit, OIDC mock, and Resend/WorkOS stubs (`make up-dev-auth`)
- [ARCHITECTURE.md](ARCHITECTURE.md) — system layout and request path
- [database.md](database.md) — migrations and dialect switching
