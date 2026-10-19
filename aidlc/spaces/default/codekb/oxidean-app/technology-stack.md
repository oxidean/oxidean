# Technology Stack — Oxidean

Version table for languages, frameworks, libraries, and toolchain.
Dependency *relationships* (internal graph, generated-source edges) are in
`dependencies.md`; per-component manifests in `component-inventory.md`.

## Languages & Runtimes

| Language | Version / edition | Where |
|---|---|---|
| Rust | edition 2021 (workspace `resolver = "2"`; profiles: dev/debugging/release `lto="thin"`+`strip`) | `crates/*` |
| TypeScript | `^7.0.2` | `apps/web`, `packages/api-client`, `scripts/` |
| Octane (`.tsrx`) | `octane ^0.11.0` + `@octanejs/astro 0.0.41` | `apps/web` UI runtime — **not React** |
| Astro | `^7.3.8` | `apps/web` static shells |
| Bun | `1.4.0` (`packageManager` pin) | package manager / runtime for scripts & web build |

## Build & Monorepo Tooling

| Tool | Version | Role |
|---|---|---|
| Cargo workspaces | resolver 2 | Rust build; `[workspace.dependencies]` shared `serde`/`serde_json`/`schemars`/`tokio` |
| Bun workspaces | 1.4.0 | `apps/*`, `packages/*` |
| Turborepo | `^2.11.7` | build/dev/test/lint orchestration (`turbo.json`) |
| GNU Make | — | ~90-target façade (`Makefile`, checked by checkmake `checkmake.ini`) |
| cargo-chef | — | multi-stage Docker builds (`crates/oxidean-api/Dockerfile`, CI base images) |
| `cross` | — | cross-compile CLI matrix (`Cross.toml`) |
| nfpm | — | deb/rpm/apk packaging (`packaging/nfpm.yaml`) |
| prost-build | `0.14` | `proto/runner.proto` codegen (`build.rs`) |
| `versioning.toml` | — | 10-component SemVer registry + virtual `wire` component; commit-subject-driven bumps (`docs/VERSIONING.md`) |

## Rust Libraries

| Crate | Version | Used for |
|---|---|---|
| axum | `0.8` (ws + multipart) | HTTP framework (`oxidean-api`, `oxidean-web`) |
| tokio | `1` (process/net/signal) | async runtime |
| sqlx | `0.9` (postgres+mysql+sqlite+migrate+macros+chrono, rustls) | persistence (`oxidean-db`) |
| serde / serde_json | `1` / `1` | serialization |
| schemars | `1.2` (preserve_order) | JSON-schema derivation (RPC types → OpenAPI/TS) |
| tower-http | `0.7` (cors/trace/fs/compression) | middleware |
| reqwest | `0.13` (rustls, json/stream/http2/system-proxy) | HTTP client |
| russh + ssh-key | `0.64` (aws-lc-rs) + `=0.7.0-rc.11` | in-process git SSH server |
| prost / prost-build | `0.14` | runner protocol types |
| utoipa-swagger-ui | `10` (axum+vendored) | Swagger UI `/api/v1/docs` |
| lettre | `0.11.23` | SMTP email |
| workos / openidconnect | `3.4` / `4.0` | auth providers |
| webauthn-rs | `0.5.2` | passkeys/2FA |
| argon2 / password-hash | `0.6` / `0.6` | password hashing (phc) |
| aes-gcm | `0.11` | actions secrets + TOTP secrets at rest |
| sha2 / sha1 / hmac | `0.11` / `0.11` / `0.13` | hashing/signing (PAT sha256 lookup, git OIDs) |
| rand / getrandom / uuid | `0.10.2` / `0.4` / `1.26` | tokens/IDs |
| chrono | `0.4.45` | timestamps |
| cookie / url | `0.18.2` / `2.5.8` | HTTP primitives |
| image / zip | `0.25` / `8.6` | avatar transcode, archives/template packs |
| include_dir | `0.7.4` | embedded assets (cli dist, stack-presets, workflow templates, skill) |
| thiserror | `2` | error types |
| async-trait | `0.1.92` | `GitBackend`/`EmailSender` trait objects |
| tracing / tracing-subscriber | `0.1` / `0.3` (env-filter) | logging |
| hyper / hyper-util / tower | `1` / `0.1` / `0.5` | `oxidean-web` WS upgrade splice |
| data-encoding / base64 | `2` / `0.23` | encodings |
| tempfile / futures-util | `3` / `0.3` | tests/async utils |

### CLI-only deps

`anstyle`, `dialoguer 0.12`, `indicatif 0.18.6`, `termimad 0.35.5`
(markdown), `jaq-core/std/json 3.x` (`--jq`), `keyring 4.2` (OS credential
store). Deliberately **no clap** — parser is hand-rolled.

## Frontend Libraries (`apps/web`, `packages/api-client`)

| Package | Version | Purpose |
|---|---|---|
| `@octanejs/*` | base-ui `^0.1.64`, dropzone `^0.0.29`, lucide `^0.1.57`, recharts `0.1.63`, tanstack-form `^0.0.59`, tanstack-query `^0.1.63`, zustand `^0.1.63` | UI primitives, uploads, icons, charts, forms, server/session data, local state |
| `@simplewebauthn/browser` | `^13.1.0` | passkey ceremonies |
| tailwindcss + `@tailwindcss/vite` | `^4.3.3` | styling |
| tailwind-merge / class-variance-authority / clsx | — | class composition |
| shiki | `4.5.0` | syntax highlighting |
| unified + remark-*/rehype-* | `11` | markdown (GFM, sanitize) |
| qrcode / spdx-license-list | — / `6.12` | TOTP QR, license metadata |
| vite | `^8.3.4` | web build |

## Test & Quality Tooling

| Tool | Version | Role |
|---|---|---|
| cargo-nextest | — (`.config/nextest.toml`, `ci` profile: no fail-fast, 90s slow-timeout, 5-way `count:N/5` partition) | Rust test runner |
| cargo-llvm-cov | — | Rust coverage (collect-only, no gate) |
| Vitest | `^5.0.3` (+`@vitest/coverage-v8`, `@vitest/browser-playwright`) | web runner, 5 projects |
| Playwright | `^1.64` | browser/e2e tests |
| `@octanejs/testing-library` + `@testing-library/{dom,jest-dom}` | — | component tests |
| oxlint type-aware | `oxlint-tsgolint 7.0.2002`, `@tsrx/oxc ^0.20.0` | web lint gate (`make web-lint`); plain `tsc` is not the type gate (`.tsrx` plugin pinned to TS 5.9) |
| oxfmt + prettier | `3.9.10` (+`prettier-plugin-astro`) | `make web-format-check` |
| knip | `^6.40.0` | dead exports (`knip.ts`, `.tsrx`/`.astro`/`.css` compilers) |
| cargo-machete + clippy `-D dead_code` | — | `make dead-code-check` |
| happy-dom | `^20.14.6` | integration test DOM |
| wiremock / tempfile | — | Rust HTTP fixtures / fs fixtures |
| pixelmatch / pngjs | — | visual baselines |
| checkmake | — | Makefile lint |

## Platform Pins & Floors

| Pin | Value | Location |
|---|---|---|
| Bun | `1.4.0` | `package.json` `packageManager` |
| git (runtime floor) | ≥ `2.5.0` | `crates/oxidean-git/src/version.rs` boot gate |
| `gh` CLI parity reference | `2.96.0` | `docs/CLI-PARITY.md` + `scripts/check-cli-parity.sh` |
| `ssh-key` | `=0.7.0-rc.11` exact | russh interop pin |
| Wire version | `0.1.0` (SemVer `WIRE_VERSION`, legacy `"1"` accepted) | `crates/oxidean-core/src/lib.rs` |

## Infrastructure / Deploy

- Docker: 5 compose files (postgres:16-alpine / mysql overlays,
  dev-auth stack with Mailpit+OIDC mock+stubs), cargo-chef API image,
  GHCR `oxidean-runner`, Caddy file-gateway (`deploy/cloud/Caddyfile`),
  Traefik v3.3 Compose edge (Docker provider).
- Railway: `.railway/railway.ts` service defs + `watchPatterns`;
  `production-deploy.yml` promote-by-SHA behind environment gate.
- CI: GitHub Actions (`ci.yml`, `ci-base.yml`, `images.yml`, `ox-cli.yml`,
  `release.yml`, `release-beta.yml`, `production-deploy.yml`); PR lanes
  run inside self-owned digest-pinned `ghcr.io/<owner>/oxidean-ci:*`
  images (`docker/ci-base/Dockerfile`) — no cross-run Rust cache by
  design. Details in `code-quality-assessment.md`.
- No dependabot/renovate config.

## Stack Implications for the New VCS Crate

- **Rust edition 2021, same workspace** — inherit `tokio`, `serde`,
  `schemars`, `thiserror`, `tracing` from `[workspace.dependencies]`; use
  `sha2`/`data-encoding` (already vendored) for content addressing.
- Reuse `async-trait` + `Arc<dyn …>` for the engine boundary
  (`GitBackend` precedent) and `sqlx` only **through** `oxidean-db`'s
  façade — never add dialect SQL in the new crate.
- If it serves Axum routes, reuse `axum 0.8`/`tower-http 0.7`; if it
  ships a client/CLI surface, match `oxidean-cli` patterns (hand-rolled
  args, `reqwest`, `keyring`).
