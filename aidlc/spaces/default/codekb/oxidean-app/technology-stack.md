# Technology Stack — Oxidean

Version table for languages, frameworks, libraries, and toolchain.
Dependency *relationships* (internal graph, generated-source edges) are in
`dependencies.md`; per-component manifests in `component-inventory.md`.

## Languages & Runtimes

| Language | Version / edition | Where |
|---|---|---|
| Rust | edition 2021 (toolchain via rustup; workspace `resolver = "2"`) | `crates/*` |
| TypeScript | `^7.0.2` | `apps/web`, `packages/api-client`, `scripts/` |
| Octane (`.tsrx`) | `octane ^0.11.0` + `@octanejs/*` | `apps/web` UI runtime — **not React** |
| Astro | `^7.3.8` | `apps/web` static shells |
| Bun | `1.4.0` (`packageManager` pin; `setup-bun` `bun-version: "1.4.0"` ×8 in ci.yml) | package manager / runtime for scripts & web build |
| Node-side tooling | via Bun | turbo, vitest, knip |

## Build & Monorepo Tooling

| Tool | Version | Role |
|---|---|---|
| Cargo workspaces | resolver 2 | Rust build; `[workspace.package]`/`[workspace.dependencies]`; profiles: dev (thin debuginfo, dep opt-level 1, argon2/blake2 O3), debugging, release (`lto="thin"`, `strip="symbols"`) |
| Bun workspaces | 1.4.0 | `apps/*`, `packages/*` |
| Turborepo | `^2.11.7` | build/dev/test/lint orchestration (`turbo.json`) |
| GNU Make | — | ~40-target façade (`Makefile`, checked by checkmake) |
| cargo-chef | — | multi-stage Docker builds (`crates/oxidean-api/Dockerfile`) |
| `cross` | — | cross-compile (10-target CLI matrix; `Cross.toml`) |
| nfpm | `2.47.0` (`NFPM_VER`, ox-cli.yml:385) | deb/rpm/apk/pkg.tar.zst packaging (`version_schema: none`) |
| protoc / prost-build | `0.14` | `proto/runner.proto` codegen |
| mold | — | linker in CI |
| checkmake | — | Makefile lint (`checkmake.ini`) — first CI job |

## Rust Libraries (`[workspace.dependencies]` highlights)

| Crate | Version | Used for |
|---|---|---|
| axum | `0.8` (+ws, multipart) | HTTP framework (`oxidean-api`, `oxidean-web`) |
| tokio | `1` | async runtime |
| sqlx | `0.9` (pg/mysql/sqlite + migrate) | persistence (`oxidean-db`) |
| serde / serde_json | `1` | serialization |
| schemars | `1.2` (`preserve_order`) | JSON-schema derivation for RPC types |
| tower-http | `0.7` | middleware |
| reqwest | `0.13` (rustls) | HTTP client |
| russh + ssh-key | `0.64` + `=0.7.0-rc.11` | in-process SSH git server |
| prost / prost-build | `0.14` | runner protocol |
| utoipa-swagger-ui | `10` | Swagger UI for `/api/v1` |
| lettre | `0.11.23` | email |
| openidconnect / workos | `4.0` / `3.4` | auth providers |
| argon2 / aes-gcm | `0.6` / `0.11` | password hashing / encryption |
| image / zip | `0.25` / `8.6` | avatars, archives |
| include_dir | `0.7.4` | embedded assets (cli dist, stack-presets, skill) |
| thiserror | `2` | error types |
| dialoguer / indicatif / keyring / termimad | `0.12` / `0.18` / `4.2` / `0.35` | `ox` CLI UX |
| jaq-core/-std/-json | `3.1` / `3.0` / `2.0` | jq-style JSON processing |

## Frontend Libraries (`apps/web`, `packages/api-client`)

| Package | Version | Purpose |
|---|---|---|
| `@octanejs/astro` | `0.0.41` | Astro integration |
| `@octanejs/base-ui` | `^0.1.64` | UI primitives |
| `@octanejs/tanstack-query` | `^0.1.63` | server/session data |
| `@octanejs/tanstack-form` | `^0.0.59` | forms |
| `@octanejs/dropzone`, `lucide`, `recharts`, `zustand`, `vite-plugin`, `testing-library` | various (`vite-plugin ^0.3.2`) | uploads, icons, charts, local state |
| tailwindcss | `^4.3.3` | styling |
| unified/remark/rehype/shiki | — | markdown rendering |
| spdx-license-list | `6.12.0` | license metadata |

## Test & Quality Tooling

| Tool | Version | Role |
|---|---|---|
| cargo-nextest | — (`.config/nextest.toml`, `default`+`ci` profiles) | Rust test runner |
| cargo-llvm-cov | — | Rust coverage → `var/coverage/rust-*` |
| cargo-machete, clippy (`-D warnings`), rustdoc (`-D warnings`) | — | dead-dep/code gates (`make dead-code-check`) |
| Vitest | `^5.0.3` (+`browser-playwright`, `coverage-v8`) | web test runner, 5 projects |
| Playwright | `^1.64.0` | browser/e2e tests |
| oxlint type-aware | `oxlint-tsgolint 7.0.2002`, `@tsrx/oxc ^0.20.0` | web lint gate (`make web-lint`); plain `tsc` is not the type gate |
| oxfmt | — | web format (`make web-format-check`) |
| knip | `^6.40.0` | dead exports (`knip.ts`, `.tsrx`/`.astro` compilers) |
| happy-dom | `^20` | integration test DOM |
| wiremock | — | Rust HTTP fixtures |

## Platform / Release Pins

| Pin | Value | Location |
|---|---|---|
| Bun | `1.4.0` | `package.json`, ci.yml ×8 |
| nfpm | `2.47.0` | `ox-cli.yml:385` |
| Docker CLI (runner image) | `29.9.0` | `docker/oxidean-runner/Dockerfile:40` |
| `gh` CLI parity reference | `2.96.0` | `docs/CLI-PARITY.md:4` + `scripts/check-cli-parity.sh` |
| git (runtime floor) | ≥ `2.5.0` | `crates/oxidean-git/src/version.rs` boot gate |

## Infrastructure / Deploy

- Docker: compose stacks (`docker-compose*.yml` ×5), cargo-chef API image,
  Bun-built web assets inside `oxidean-web` runtime, GHCR for
  `oxidean-runner`, Caddy gateway in `deploy/cloud`.
- Railway: `.railway/railway.ts` service defs + `watchPatterns`
  (dependency-aware change detection precedent); staging autodeploy,
  prod promote-by-SHA via `production-deploy.yml` environment gate.
- CI: GitHub Actions — `ci.yml`, `ox-cli.yml`, `publish-runner.yml`,
  `production-deploy.yml` (detail in `code-quality-assessment.md`).
- No dependabot/renovate config.
