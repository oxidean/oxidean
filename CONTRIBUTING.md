# Contributing to Oxidean

Thanks for helping build Oxidean — a self-hostable forge with one codebase for cloud and on-prem.

This guide is for **human contributors** and **coding agents**. Agents should also read [AGENTS.md](AGENTS.md).

## Before you start

1. Read [docs/GETTING-STARTED.md](docs/GETTING-STARTED.md) and [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).
2. Follow [docs/CODE_PRACTICES.md](docs/CODE_PRACTICES.md) (style, boundaries, testing expectations).
3. UI work: Octane is **not** React — see [AGENTS.md](AGENTS.md) and [`.agents/skills/octane/SKILL.md`](.agents/skills/octane/SKILL.md).

## Prerequisites

- [Bun](https://bun.sh) matching root `package.json` `packageManager` (currently `bun@1.4.0`)
- Rust stable + Cargo
- Docker Compose for full-stack / auth e2e
- Optional: `cargo-nextest` for faster Rust tests

```bash
corepack enable
bun install
cargo metadata -q
cp .env.example .env
make rpc-gen
```

## Development loop

```bash
make help                  # all targets
make rpc-gen               # after RPC / DTO changes
make test                  # Rust + Vitest
make test-e2e-stack        # API + Mailpit/OIDC stubs + browser
make web-lint              # oxlint type-aware (apps/web)
make web-format-check      # oxfmt --check (apps/web)
make up && make smoke      # Compose stack
```

Local two-terminal API + web: see `make dev` / [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

Auth stubs without cloud secrets: [docs/dev-auth.md](docs/dev-auth.md).

## Pull requests

1. Prefer small, focused PRs that match an existing roadmap phase or a clear bugfix.
2. Include tests for behavior changes (see [docs/TESTING.md](docs/TESTING.md)).
3. After changing RPC procedures or shared types: run `make rpc-gen` and commit `@oxidean/api-client` updates together.
4. Do not commit secrets (`.env`, tokens, private keys). Use examples under `docs/` and `.env.example`.
5. Keep UI in `.tsrx` Octane style; do not introduce a parallel React app or alias React to Octane.
6. Before opening a PR that touches `apps/web`: `make web-lint` and `make web-format-check` must pass (CI `web-octane` gates them). Fix type-aware oxlint diagnostics; do not leave formatting drift.
7. Run what CI runs locally when practical: `make test`, `make rpc-sync-check`, and stack e2e for auth/UI paths.

CI workflow: [`.github/workflows/ci.yml`](.github/workflows/ci.yml) (Rust nextest, web lint/format, Vitest, stack e2e, RPC sync, Compose validate).

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).

## Questions / docs map

| Doc | Audience |
|-----|----------|
| [README.md](README.md) | Operators / evaluators — product overview + Compose quick start |
| [docs/GETTING-STARTED.md](docs/GETTING-STARTED.md) | First run — Compose or host `make dev` |
| [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) | Ops — Compose images, Traefik, forge volumes/ports |
| [docs/database.md](docs/database.md) | Ops — Postgres / MySQL / SQLite |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | Env vars |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | System design |
| [docs/API.md](docs/API.md) | RPC + git / LFS / SSH / packages surfaces |
| [docs/guides/stack-presets.md](docs/guides/stack-presets.md) | In-repo `/new` stack presets |
| [docs/guides/administration.md](docs/guides/administration.md) | Sys-admins / scoped admins — users, sessions, activity, invites |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | Contributors — local API + web dev, monorepo layout |
| [docs/TESTING.md](docs/TESTING.md) | Test layers |
| [docs/dev-auth.md](docs/dev-auth.md) | Local auth/email stubs |
| [docs/CODE_PRACTICES.md](docs/CODE_PRACTICES.md) | Humans + agents — conventions |
| [AGENTS.md](AGENTS.md) | Agents — stack, Octane, commands |

Per-package READMEs live next to each crate, app, and package.
