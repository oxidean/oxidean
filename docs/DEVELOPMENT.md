<!-- generated-by: gsd-doc-writer -->
# Development

Guide for working on Oxidean locally: Bun + Turborepo for the web app and packages, Cargo for the API and database crates, and Make targets for RPC codegen, Compose overlays, and tests.

Related: [ARCHITECTURE.md](ARCHITECTURE.md), [CONFIGURATION.md](CONFIGURATION.md), [database.md](database.md), [dev-auth.md](dev-auth.md), [CODE_PRACTICES.md](CODE_PRACTICES.md), [CONTRIBUTING.md](../CONTRIBUTING.md), [AGENTS.md](../AGENTS.md).

## Local setup

1. **Prerequisites** — [Bun](https://bun.sh) matching `packageManager` in root `package.json` (`bun@1.4.0`), Rust/`cargo` (stable), **`protoc`** ([protobuf-compiler](https://grpc.io/docs/protoc-installation/) / `apt install protobuf-compiler`) for Actions runner proto codegen (`crates/oxidean-api/proto/runner.proto`), and Docker Compose for full-stack / overlay work. Optional: `cargo-nextest` (`cargo install cargo-nextest --locked`) so `make test` uses nextest instead of `cargo test`. Local builds also accept `PROTOC` or `~/.cache/protoc-*/bin/protoc` (see `crates/oxidean-api/build.rs`).
2. **Clone and install**

```bash
git clone git@github.com:oxidean/oxidean.git
cd Oxidean
corepack enable
bun install
cargo metadata -q
```

3. **Environment** — Copy Compose/local defaults (never commit secrets):

```bash
cp .env.example .env
```

For host `make` / Vite development (API on `127.0.0.1:8080`, Vite on `:3000`), prefer the commented local block in `.env.example`: `OXIDEAN_ENV=development`, a host-reachable `DATABASE_URL` (e.g. Postgres on `localhost:5432`), and optional `OXIDEAN_CORS_ORIGINS=http://localhost:3000`. Full variable reference: [CONFIGURATION.md](CONFIGURATION.md).

4. **RPC client once** — Before the first web run:

```bash
make rpc-gen
```

5. **Start local API + web** — `make dev` prints the two-terminal workflow (it also runs `rpc-gen`):

```bash
# terminal 1
OXIDEAN_ENV=development API_BIND=127.0.0.1:8080 cargo run -p oxidean-api --bin oxidean-api

# terminal 2
bun run --filter @oxidean/web dev
```

Or bring up the Compose stack instead: `make up` (Traefik on `:80`). See [Compose overlays](#compose-overlays) below.

## Local API + Vite proxy

Without Traefik, the browser talks to Vite on port **3000**. `apps/web/vite.config.ts` proxies same-origin paths to the API so cookies and RPC stay on one origin:

| Path | Target (default) |
|------|------------------|
| `/api/rpc/ws` | `ws://127.0.0.1:8080` (WebSocket) |
| `/api/rpc`, `/api/auth`, `/api/user` | `http://127.0.0.1:8080` |
| `/uploads`, `/health` | `http://127.0.0.1:8080` |

Override the proxy upstream with `OXIDEAN_E2E_API_ORIGIN` (trailing slash stripped) when running stack e2e against a non-default API origin.

Session cookies use `oxidean_session`. With `OXIDEAN_ENV=development`/`dev`, the cookie is not marked `Secure`, which matches `http://localhost:3000`.

## RPC codegen sync

Rust procedure names and shared types (`crates/oxidean-core`, `crates/oxidean-api/src/rpc.rs`) are authoritative. `rpc-gen` regenerates `@oxidean/api-client`:

```bash
make rpc-gen          # cargo run -q -p oxidean-api --bin rpc-gen
make rpc-sync-check   # regenerates then git diff --exit-code packages/api-client
```

- Binary: `crates/oxidean-api/src/bin/rpc_gen.rs`
- Output package: `packages/api-client` (treat generated client sources as codegen output; do not hand-edit as the source of truth)
- Web import path: `apps/web/src/lib/api-client.ts` → `@oxidean/api-client`
- CI job `rpc-sync` in `.github/workflows/ci.yml` runs `make rpc-sync-check`

After changing RPC handlers or shared DTOs, run `make rpc-gen` and commit the updated client with your change.

## Compose overlays

Root Compose files (validated in CI `compose` job):

| Target | Files / notes |
|--------|----------------|
| `make up` | `docker-compose.yml` — Traefik `:80`, web, api, Postgres |
| `make up-mysql` | `docker-compose.yml` + `docker-compose.mysql.yml` (`--profile mysql`) |
| `make up-sqlite` | `docker-compose.yml` + `docker-compose.sqlite.yml`; writes `.env.sqlite` with `OXIDEAN_SQLITE_HOST_DIR` |
| `make up-dev-auth` | `docker-compose.dev-auth.yml` (`--profile dev-auth`) — Mailpit, OIDC mock, Resend/WorkOS stubs |
| `make down` / `down-mysql` / `down-sqlite` / `down-dev-auth` / `down-with-dev-auth` | Matching teardown |
| `make smoke` / `smoke-mysql` / `smoke-sqlite` | Bring-up smoke asserting dialect |
| `make logs` | Follow Compose logs |

Dev-auth quick path:

```bash
cp docs/dev-auth.env.example .env.dev-auth
make up-dev-auth
# Mailpit UI http://127.0.0.1:8025 — OIDC http://127.0.0.1:9090/default — stubs http://127.0.0.1:9092
```

Dialect ops (`db-migrate`, `db-switch-dialect`, `db-matrix`): [database.md](database.md). Auth stubs: [dev-auth.md](dev-auth.md).

## Coding layout

```
oxidean/
├── apps/web/                 # @oxidean/web — Octane / TanStack Start (Vite :3000)
│   ├── src/routes/           # UI routes (incl. admin/, settings/)
│   ├── src/components/       # UI + chrome
│   ├── src/lib/              # api-client wrapper, helpers
│   └── vite.config.ts        # Dev proxy to API
├── packages/api-client/      # Generated TS RPC client (make rpc-gen)
├── crates/
│   ├── oxidean-api/         # Axum API, auth, email, rpc-gen binary
│   ├── oxidean-core/        # Shared domain / RPC types (no I/O)
│   └── oxidean-db/          # Multi-dialect sqlx + migrations/
├── scripts/                  # check-rpc-sync, compose-smoke, dev-auth e2e, db helpers
├── docs/                     # Operator + architecture docs
├── brand/                    # Product mark / brand assets
├── deploy/                   # Optional Traefik extras
├── var/                      # Runtime state (SQLite, uploads; gitignored)
├── Makefile                  # Primary developer entrypoints
├── package.json              # Bun workspaces: apps/*, packages/*
├── turbo.json                # build / dev / test / lint pipeline
└── Cargo.toml                # Rust workspace members under crates/
```

**Bun workspaces** — Root `workspaces`: `apps/*`, `packages/*`. Turborepo (`turbo run …`) orchestrates package scripts; `build` depends on `^build`, `dev` is persistent/uncached, `test` depends on `^build`.

**Cargo workspace** — Members: `oxidean-api`, `oxidean-core`, `oxidean-db`. Keep dialect branching inside `oxidean-db` only.

## Build commands

### Root npm/Bun scripts (`package.json`)

| Command | Description |
|---------|-------------|
| `bun run dev` | `turbo run dev` — workspace `dev` tasks (persistent) |
| `bun run build` | `turbo run build` — builds packages with `dist/**` outputs |
| `bun run test` | `turbo run test` — Vitest across workspaces (after `^build`) |
| `bun run lint` | `turbo run lint` — workspace lint tasks |

### Package-scoped (examples)

| Command | Description |
|---------|-------------|
| `bun run --filter @oxidean/web dev` | Vite dev server for the web app |
| `bun run --filter @oxidean/web build` | `vite build` |
| `bun run --filter @oxidean/web test` | Vitest (unit / integration / e2e projects) |
| `bun run --filter @oxidean/web test:e2e:stack` | Stack e2e Vitest projects |
| `bun run --filter @oxidean/web lint` | `oxlint --type-aware --deny-warnings` via `@tsrx/oxc` + `oxlint-tsgolint` |
| `bun run --filter @oxidean/web format` | `oxfmt --write` |
| `bun run --filter @oxidean/web format:check` | `oxfmt --check` |
| `bun run --filter @oxidean/api-client test` | api-client Vitest |

### Make targets (preferred day-to-day)

| Command | Description |
|---------|-------------|
| `make help` | List targets |
| `make dev` | `rpc-gen` + print API/web two-terminal commands |
| `make rpc-gen` / `make rpc-sync-check` | Regenerate / verify `@oxidean/api-client` |
| `make up` / `make down` / `make logs` | Default Compose stack |
| `make up-mysql` / `up-sqlite` / `up-dev-auth` | Dialect and auth overlays |
| `make test` | `cargo nextest` (or `cargo test`) + `bun run test` |
| `make web-lint` / `make web-format-check` | Web oxlint (type-aware) / oxfmt check |
| `make test-e2e-stack` | `./scripts/dev-auth/run-stack-e2e.sh` |
| `make smoke` / `smoke-mysql` / `smoke-sqlite` | Compose bring-up smoke |
| `make db-migrate` / `db-switch-dialect` / `db-matrix` | Migrations and dialect probe |

Turbo task graph: `turbo.json` (`build`, `dev`, `test`, `lint`).

## Rust build performance

Profiles in the root `Cargo.toml` already favor fast iteration: `dev` emits `line-tables-only` debuginfo and no dep debuginfo at all; `release` uses `lto = "thin"` + `strip = "symbols"`. For a full debugger session, use the opt-in `debugging` profile (`cargo build --profile debugging`).

Optional local speedups:

- **mold linker** — `sudo apt install mold` then `export RUSTFLAGS="-C link-arg=-fuse-ld=mold"` (CI and the Docker builds already link with mold).
- **Nightly-only boosters** — the repo builds on stable, so these belong in your user config (`$CARGO_HOME/config.toml`), not the repo: the parallel frontend (`[build] rustflags = "-Zthreads=8"`) and the Cranelift codegen backend for `dev` (`rustup component add rustc-codegen-cranelift-preview --toolchain nightly` + `-Zcodegen-backend=cranelift`). See <https://doc.rust-lang.org/cargo/guide/build-performance.html>.

The Docker builds use [cargo-chef](https://github.com/LukeMathWalker/cargo-chef) + BuildKit cache mounts; local `docker compose build` reuses deps automatically. In CI, layer caches persist via the `type=gha` backend, and `cargo build --timings` HTML from the `build-metrics` job shows the per-crate critical path.

## Code style

- **JavaScript / TypeScript / TSRX** — Lint and format with [`@tsrx/oxc`](https://oxc.tsrx.dev/guide/getting-started) in `@oxidean/web`: type-aware `oxlint` (`oxlint-tsgolint`) and `oxfmt` (scripts `lint`, `format`, `format:check`; Make `web-lint` / `web-format-check`). CI `web-octane` runs lint + format check. No ESLint or Prettier. Prefer existing patterns in `apps/web` (TypeScript, Octane/TanStack, Tailwind v4). Full `tsc --noEmit` is not the gate yet (`.tsrx` needs `@tsrx/typescript-plugin`, which still peers TypeScript 5.9.x while this app uses TypeScript 7) — type-aware oxlint is the enforced substitute.
- **Rust** — Use standard `rustfmt` / `cargo fmt` and Clippy locally (`cargo clippy --workspace`). There is no committed `rustfmt.toml` / `clippy.toml`; CI currently gates on `cargo nextest`, not fmt/clippy.
- **Generated client** — Do not reformat or hand-patch `packages/api-client` as a substitute for updating Rust + `make rpc-gen`.
- **Env files** — Keep secrets out of git (`.env`, `.env.dev-auth`, `.env.sqlite`).

## Branch conventions

Default branch: `main` (CI runs on push to `main` and on all pull requests).

Recent work uses descriptive prefixes such as `cursor/…` and conventional-commit style subjects (`feat(…)`, `fix(…)`, `docs(…)`, `ci:`, `test:`). Prefer short topic branches off `main` with a clear purpose.

## PR process

See [CONTRIBUTING.md](../CONTRIBUTING.md) for PR conventions and `.github/pull_request_template.md` for the checklist GitHub pre-fills. The gates below are what CI (`.github/workflows/ci.yml`) enforces:

- Open a PR against `main`; all workflow jobs must pass.
- **api-rust** — `cargo nextest run --workspace --profile ci`
- **web-octane** — `bun install --frozen-lockfile`, `bun run --filter @oxidean/web lint`, `format:check`, `bun run test`, `turbo run build --filter=@oxidean/web`
- **e2e-stack** — `make test-e2e-stack` (API + Mailpit/OIDC/stubs + web)
- **rpc-sync** — `make rpc-sync-check` (commit regenerated `packages/api-client` if you changed RPC)
- **compose** — `docker compose … config` for default, MySQL, SQLite, and dev-auth overlays
- **db-matrix** — dialect probe for postgres / mysql / sqlite

Locally before push: `make rpc-sync-check`, `make test`, and when touching auth/email paths `make test-e2e-stack` or the relevant Compose smoke target.
