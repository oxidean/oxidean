# Developer Code Scan — Oxidean monorepo (intent `261014-semver-rollout`)

## Developer Code Scan Results

### Scan Coverage
- **Analyzed deeply**: `Cargo.toml` (workspace root), `Cargo.lock` (oxidean-* entries), `package.json` (root), `turbo.json`, all 7 `crates/*/Cargo.toml`, `apps/web/package.json`, `packages/api-client/package.json`, `.github/workflows/ox-cli.yml` (full 655 lines), `.github/workflows/ci.yml` (full 641 lines), `.github/workflows/production-deploy.yml`, `.github/workflows/publish-runner.yml`, `packaging/nfpm.yaml`, `crates/oxidean-cli/src/self_update.rs`, `crates/oxidean-cli/src/manifest.rs`, `crates/oxidean-cli/src/main.rs`, `crates/oxidean-cli/src/args.rs` (version surfaces), `crates/oxidean-api/src/routes/cli_dist.rs`, `crates/oxidean-api/src/rpc.rs` (header check, manifest, PROCEDURES), `crates/oxidean-api/src/mcp.rs` (initialize handshake), `crates/oxidean-api/src/rest/mod.rs`, `crates/oxidean-api/src/bin/rpc_gen.rs` + `openapi_gen.rs`, `crates/oxidean-core/src/lib.rs`, `crates/oxidean-git/src/version.rs`, `crates/oxidean-web/src/{main,wellknown}.rs`, `crates/oxidean-runner/src/main.rs`, `crates/oxidean-api/Dockerfile`, `apps/web/Dockerfile`, `docker/oxidean-runner/Dockerfile`, `crates/oxidean-api/assets/cli/install.{sh,ps1}`, `docker-compose.yml`, `.config/nextest.toml`, `apps/web/vitest.config.ts`, `docs/{ARCHITECTURE,CLI,TESTING,CONFIGURATION}.md` (relevant sections), `docs/openapi.yaml` (header), `scripts/check-{cli-parity,skill-sync,route-sync,rpc-sync,openapi-sync}.*`, `skills-lock.json`, `.cursor/hooks.json`, `.railway/railway.ts` (service defs + watchPatterns), `AGENTS.md`, `CONTRIBUTING.md`, `.gitignore`, git history/tag state (`git tag -l`, `git log` prefix survey — 100 commits, ~94 conventional-prefixed, only tag is `ox-dist`)
- **Skimmed only**: `apps/web/src/{pages,routes,components,lib}` (directory census + version grep, not file-by-file), `crates/oxidean-api/src/` submodules (dir listing; rpc/mcp/rest/routes read, domain dirs skimmed), `crates/oxidean-db/src/` + `migrations/{postgres,mysql,sqlite}` (46 postgres migrations), `crates/oxidean-api/tests/` + `crates/oxidean-db/tests/` + `crates/oxidean-cli/tests/` + `crates/oxidean-git/tests/` (file lists, not contents), `crates/oxidean-api/assets/stack-presets/**` (version fields surveyed), `apps/web/e2e/`, `deploy/{cloud,traefik}/`, `docs/archive/`, `scripts/` (remaining ~35 scripts skimmed by name/header comments), `Makefile` (target list, recipes not read line-by-line), `knip.ts`, `.agents/skills/` + `.cursor/skills/` (name census only), `tmp/`, `var/`, `brand/`, `aidlc/` engine state

### Packages Found
| Package | Manifest | Type | Language | Purpose |
|---|---|---|---|---|
| `oxidean-api` | `crates/oxidean-api/Cargo.toml` | lib + 4 bins (`oxidean-api`, `rpc-gen`, `openapi-gen`, `oxidean-protection-hook`) | Rust | Axum API server: typed JSON-RPC dispatch (283 procedures), `/api/v1` REST facade, `/api/mcp`, OAuth/OIDC, git Smart HTTP/LFS/SSH, package registries (OCI/npm/generic), `/cli/*` ox distribution, Actions runner protocol |
| `oxidean-cli` | `crates/oxidean-cli/Cargo.toml` | lib + bin (`ox`) | Rust | `ox` companion CLI: typed RPC client, self-update, MCP client config, embedded agent `skill/SKILL.md`, device auth |
| `oxidean-core` | `crates/oxidean-core/Cargo.toml` | lib | Rust | Shared domain types, RPC envelope, `RPC_PROTOCOL_VERSION`, `ManifestResponse`/`HealthResponse` contract types (schemars) |
| `oxidean-db` | `crates/oxidean-db/Cargo.toml` | lib + bin (`src/bin/migrate.rs`) | Rust | Multi-dialect sqlx persistence (Postgres/MySQL/SQLite), migrations, sole dialect-branching crate |
| `oxidean-git` | `crates/oxidean-git/Cargo.toml` | lib | Rust | `GitBackend` trait + `CliGitBackend` (system git ≥ 2.5 gate in `version.rs`) |
| `oxidean-runner` | `crates/oxidean-runner/Cargo.toml` | bin | Rust | Self-hosted Actions runner, custom JSON protocol, published to GHCR |
| `oxidean-web` | `crates/oxidean-web/Cargo.toml` | bin | Rust | Static-shell + middleware server for the SPA (meta injection, auth gates, `/health`, well-known, API prefix proxy) |
| `@oxidean/web` | `apps/web/package.json` | app | TS/Octane (`.tsrx`) + Astro | Web UI; 213 `.tsrx` files; private, not published |
| `@oxidean/api-client` | `packages/api-client/package.json` | lib | TS | Generated typed RPC client (`src/index.ts` 5427 lines, AUTO-GENERATED header; hand-written `languages.ts`, `scope-catalog.ts`) |
| `oxidean` (root) | `package.json` | workspace root | — | private, no `version` field; Bun `workspaces: [apps/*, packages/*]` |

### Build System
- **Type**: Cargo workspace (resolver 2) + Bun workspaces + Turborepo + GNU Make (~40 targets) + cargo-chef multi-stage Dockerfiles + `cross` for cross-compile + nfpm packaging
- **Config Files**: `Cargo.toml` (`[workspace]`, `[workspace.package]`, `[workspace.dependencies]`, dev/debugging/release profiles — `lto="thin"`, `strip="symbols"`), `package.json` + `bun.lock` (`packageManager: bun@1.4.0`), `turbo.json` (build/dev/test/lint), `Makefile`, `Cross.toml`, `checkmake.ini`, `knip.ts`, `.config/nextest.toml`, `apps/web/{astro.config.mjs,vitest.config.ts,tsconfig.json}`, `packages/api-client/vitest.config.ts`, `packaging/nfpm.yaml`, `docker-compose*.yml` (5 files), `.railway/railway.ts` (Railway IaC)
- **Build Dependencies** (internal edges — the dependency-aware change-detection graph):
  - `oxidean-api` → `oxidean-core`, `oxidean-db`, `oxidean-git` (path deps)
  - `oxidean-cli` → `oxidean-core` (path dep)
  - `oxidean-db` → `oxidean-core` (path dep)
  - `oxidean-git`, `oxidean-runner`, `oxidean-web`, `oxidean-core` → no internal deps
  - `@oxidean/web` → `@oxidean/api-client` (`workspace:*`); `@oxidean/api-client` → peer `@octanejs/tanstack-query`
  - Generated-source edge: `packages/api-client/src/index.ts` ← `oxidean-api --bin rpc-gen` (drift-gated by `make rpc-sync-check`); `docs/openapi.yaml` ← `openapi-gen` (`make openapi-sync-check`); `/cli/skill.md` + `ox skill` embed ← `crates/oxidean-cli/skill/SKILL.md` (`make skill-sync-check`); `crates/oxidean-web` `SHELLS` ↔ `apps/web/src/pages/**` route corpora (`scripts/check-route-sync.ts`)
  - Docker images: `api` (cargo-chef + optional `OXIDEAN_CLI_DIST_URL` ox-matrix staging), `web` (Bun build → `oxidean-web` Rust runtime), `oxidean-runner` (GHCR), `deploy/cloud` gateway (Caddy)

### APIs Discovered
- Typed JSON RPC — `POST /api/rpc` + WS `/api/rpc/ws` (`crates/oxidean-api/src/rpc.rs`) — 283 procedures in `PROCEDURES` const, ~285 dispatch match arms; envelope `RpcRequest`/`RpcResponse` + `Oxidean-RPC-Version` header check (`check_version_header`, rpc.rs:105)
- REST facade — `/api/v1/**` (`crates/oxidean-api/src/rest/`) — ~69 route literals across `RouteDef` tables (meta/users/orgs/repos/issues/pulls/releases/statuses/admin); generates `docs/openapi.yaml` (`info.version: '1'`) + serves `/api/v1/openapi.json` + Swagger UI
- MCP endpoint — `POST /api/mcp` (`crates/oxidean-api/src/mcp.rs`) — JSON-RPC `initialize`/`tools/list`/`tools/call`/`resources/list`/`prompts/list`, 18 tool defs, date-versioned protocol `"2025-06-18"` etc.
- CLI distribution — `GET /cli/{install.sh,install.ps1,skill.md,latest,bin/{*file}}` (`routes/cli_dist.rs`) — unauthenticated
- OAuth/OIDC — `/oauth/{authorize,token,userinfo,device/code}`, `/api/auth/{workos,oidc}/{start,callback}`, well-known discovery
- Git protocols — Smart HTTP `/{owner}/{repo}.git`, LFS `info/lfs` batch, in-process russh SSH (`OXIDEAN_SSH_PORT`)
- Package registries — `/v2` (OCI), `/npm`, `/generic` path prefixes
- Actions runner protocol — `/api/actions/**` custom JSON + `proto/runner.proto` (prost-build in `build.rs`)
- WebMCP — browser-side `apps/web/src/lib/webmcp.ts` (same `MCP_PROTOCOL_VERSIONS` list duplicated in `crates/oxidean-web/src/wellknown.rs:15` — "keep in sync" comment)
- Meta — `/health` (api + web with `Oxidean-Health-Probe` header gate), Atom feeds, avatar/uploads, template packs, release assets

### Version Inventory (intent-specific enumeration — every place a version lives or is consumed)

**Declared versions (source of truth locations):**
1. `Cargo.toml:22` — `[workspace.package] version = "0.1.0"` — single version for ALL 7 crates via `version.workspace = true` (each crate manifest); `Cargo.lock` confirms all `oxidean-*` at `0.1.0`
2. `apps/web/package.json:3` — `"version": "0.1.0"` (private)
3. `packages/api-client/package.json:3` — `"version": "0.1.0"` (private)
4. Root `package.json` — NO `version` field
5. `crates/oxidean-core/src/lib.rs:57` — `pub const RPC_PROTOCOL_VERSION: u32 = 1` (wire protocol version, integer not semver)
6. `crates/oxidean-api/src/rpc.rs:121` — `pub const MIN_CLI_VERSION: &str = "0.1.0"` (oldest compatible `ox`; hand-bumped, tested at rpc.rs:1722, tests/rpc_http.rs:122, cli tests/manifest_gate.rs:36)
7. `crates/oxidean-api/src/bin/rpc_gen.rs:13` — emits `export const RPC_VERSION = 1` → `packages/api-client/src/index.ts:3` (+ `RPC_VERSION_HEADER = "Oxidean-RPC-Version"`)
8. `crates/oxidean-api/src/rest/spec.rs:335` — OpenAPI `info.version = "1"` → `docs/openapi.yaml:5` (`version: '1'`); served live at `/api/v1/openapi.json` via `rest/openapi.rs`
9. MCP date-versions: `apps/web/src/lib/webmcp.ts:2` and `crates/oxidean-web/src/wellknown.rs:15` — `MCP_PROTOCOL_VERSIONS = ["2024-11-05","2025-03-26","2025-06-18"]`; `crates/oxidean-cli/src/mcp.rs` initialize sends `"2025-06-18"`, `crates/oxidean-api/src/mcp.rs:~318` negotiates last-shared
10. `skills-lock.json` — `"version": 1` schema + per-skill `computedHash` pins for vendored `.agents/skills/*`
11. Tool pins: `docker/oxidean-runner/Dockerfile:40` `DOCKER_CLI_VERSION=29.9.0`; `docs/CLI-PARITY.md:4` `gh 2.96.0` comparison pin (checked by `scripts/check-cli-parity.sh`); `bun@1.4.0` (`package.json`, `setup-bun` `bun-version: "1.4.0"` ×8 in ci.yml); `NFPM_VER=2.47.0` (ox-cli.yml:385); `checkmake.ini`
12. Stack-preset scaffold templates (embedded via `include_dir` into `oxidean-api` binary, served to new repos): `crates/oxidean-api/assets/stack-presets/**` contain `version = "0.1.0"`/`"0.0.0"`/`"0.0.1"`/`"1.0.0"` in preset `Cargo.toml`/`package.json` files — workspace-`exclude`d; these are user-facing scaffold defaults, not repo versions
13. `.github/ISSUE_TEMPLATE/bug_report.yml:71` — `id: version` field, placeholder `v0.1.0`

**`env!("CARGO_PKG_VERSION")` embeds (all currently = `0.1.0`, resolve to the emitting crate's version once workspace inheritance splits):**
- `crates/oxidean-cli/src/main.rs:27` — `ox --version` → `ox {ver}`
- `crates/oxidean-cli/src/commands.rs:834-835` — version Out::Report text + `json.data.version`; `:801` feeds `manifest::compat_notices(&manifest, CARGO_PKG_VERSION)`
- `crates/oxidean-cli/src/http.rs:51,93`, `rpc.rs:112`, `self_update.rs:68` — `User-Agent: ox/{ver}`
- `crates/oxidean-cli/src/mcp.rs:1273` — MCP `clientInfo.version`
- `crates/oxidean-cli/src/self_update.rs:171` — `current` side of `needs_binary_update(current, latest.version, local_sha, expected_sha)` — string inequality OR sha256 diff triggers swap
- `crates/oxidean-api/src/rpc.rs:417` — `system.manifest` → `server_version` (also asserted :1721, tests/rpc_http.rs:120)
- `crates/oxidean-api/src/rpc.rs:468` — `system.health` → `version` (rendered by `apps/web/src/routes/status.tsrx:93`)
- `crates/oxidean-api/src/mcp.rs:332` — MCP `serverInfo.version`
- `crates/oxidean-api/src/routes/cli_dist.rs:131` — `GET /cli/latest` → `"version"` — **WARNING: this reports `oxidean-api`'s version but describes `oxidean-cli` (ox) binaries** — currently correct only because both inherit `0.1.0`
- Tests embedding it: `crates/oxidean-cli/tests/{self_update_http.rs:38,95, commands_e2e.rs:540}`

**Version-adjacent contract constants:**
- `Oxidean-RPC-Version` header — sent by generated client (`RPC_VERSION`) and `crates/oxidean-cli/src/rpc.rs:111`; enforced server-side `rpc.rs:105-117` (`rpc.version_mismatch`)
- `OXIDEAN_RPC_VERSION` env var — set in `.env.example:19`, `.env`, `.env.dev-auth`, `docker-compose.yml:63`, documented `docs/CONFIGURATION.md:17,255` — **NOT read by any Rust code** (dead/pass-through knob; the header value is compile-time)
- `apps/web/public/sw.js:15` — `CACHE_NAME = "oxidean-shell-__OXIDEAN_SW_BUILD__"` (commit-SHA fingerprint substituted at build, not semver)
- `crates/oxidean-cli/skill/SKILL.md` frontmatter — `compatibility: ox 0.1+` (semver range claim in the agent skill doc)
- `SOURCE_COMMIT` / `dist_commit` — git-SHA provenance file in the ox dist dir (`cli_dist.rs:110-124`, Dockerfile `OXIDEAN_SOURCE_COMMIT`, Railway `RAILWAY_GIT_COMMIT_SHA`)
- `crates/oxidean-git/src/version.rs` — runtime `git --version` ≥ 2.5.0 boot gate (semver-tuple parser, analogous shape to CLI `semver_tuple`)

**Release/tag machinery (existing pipeline the intent wires into):**
- `.github/workflows/ox-cli.yml` — triggers: `push` main + `tags: ["v*.*.*"]` (line 36), PR, dispatch; path-filtered to `crates/oxidean-cli/**`, `crates/oxidean-api/src/routes/cli_dist.rs`, `crates/oxidean-api/assets/cli/**`, `crates/oxidean-api/Dockerfile`, `packaging/**`, `scripts/ox-*`, `Cargo.{toml,lock}`, the workflow itself. Jobs: `build` (10-target matrix + FreeBSD VM), `install-linux` (7 distro legs), `install-macos`, `install-windows`, `packages` (nfpm deb/rpm/apk/pkg.tar.zst × amd64/arm64), `publish-dist` (rolling `ox-dist` GH release on main+tags, then staging-api redeploy via `scripts/railway-staging-api-redeploy.sh`), `release` (tag-only `gh release create "$TAG" --generate-notes`, title `ox {tag}`), `image-matrix` / `image-matrix-pr` (digest-lockstep proof the API image staged the matrix)
- Package version derivation `ox-cli.yml:368-375`: tag → `${GITHUB_REF_NAME#v}`; non-tag → `0.0.0.dev.${GITHUB_RUN_ID}` (deliberately `+`/`-`-free for apk/pacman); `packaging/nfpm.yaml` `@PKG_VERSION@` + `version_schema: none`
- `.github/workflows/publish-runner.yml` — `ghcr.io/<owner>/oxidean-runner`; triggers on main path changes + `v*.*.*` tags; metadata-action emits branch/tag/sha/`latest` tags
- `.github/workflows/production-deploy.yml` — manual `workflow_dispatch` promote-by-SHA / rollback to Railway prod (`Oxidean / production` environment gate); requires latest CI success on the SHA — **no version input**
- `.railway/railway.ts` — api service `watchPatterns: [crates/oxidean-api/**, crates/oxidean-cli/**, crates/oxidean-core/**, crates/oxidean-db/**, crates/oxidean-git/**, packages/api-client/**]` (this list correctly includes transitive deps — a working precedent for dependency-aware detection), staging autodeploy+wait-for-CI, prod promote-by-SHA
- Docker image tags: compose uses local `:ci` tags (`oxidean-api:ci`, `oxidean-web:ci`); GHCR runner gets semver tag from git tag; **no GHCR publish for api/web images**
- Git state: `git tag -l` → only `ox-dist` (rolling); **no `v*` semver tags exist yet**; release notes come from `--generate-notes` (PR/commit derived, no changelog files anywhere — only `tmp/demo-app/CHANGELOG.md` fixture)
- Git hooks: **none configured** — `core.hooksPath` unset, `.git/hooks` samples only, no husky/lefthook/pre-commit configs; `.cursor/hooks.json` + `.claude/hooks/` are harness hooks (AI-DLC), not git hooks
- Commit-prefix status quo: ~94/100 recent subjects match `<type>(<scope>):` — `docs:`(26), `fix|feat(web|cli|api|cloud|ci|auth|db|scripts|github|planning)`, `chore`, `test`, `refactor`, `perf(build)`, `revert`, `fix(11.1)` — informal, unenforced, scope vocabulary drifts

### Frameworks & Libraries
- Rust edition 2021; axum `0.8` (+ws,multipart), tokio `1`, sqlx `0.9` (pg/mysql/sqlite+migrate), serde `1`/serde_json, schemars `1.2`, tower-http `0.7`, reqwest `0.13` (rustls), russh `0.64` + ssh-key `=0.7.0-rc.11`, prost `0.14` + prost-build, utoipa-swagger-ui `10`, lettre `0.11.23`, openidconnect `4.0`, workos `3.4`, argon2 `0.6`, aes-gcm `0.11`, image `0.25`, zip `8.6`, include_dir `0.7.4`, thiserror `2`, dialoguer `0.12`, indicatif `0.18`, keyring `4.2`, jaq `-core 3.1/-std 3.0/-json 2.0`, termimad `0.35`
- Frontend: astro `^7.3.8`, octane `^0.11.0` + `@octanejs/*` (`astro 0.0.41`, `base-ui ^0.1.64`, `tanstack-query ^0.1.63`, `tanstack-form ^0.0.59`, `dropzone`, `lucide`, `recharts`, `zustand`, `vite-plugin ^0.3.2`, `testing-library`), tailwindcss `^4.3.3`, unified/remark/rehype/shiki markdown stack, spdx-license-list `6.12.0`
- Tooling: bun `1.4.0`, turbo `^2.11.7`, typescript `^7.0.2`, vitest `^5.0.3` (+`browser-playwright`, `coverage-v8`), playwright `^1.64.0`, oxlint type-aware (`oxlint-tsgolint 7.0.2002`, `@tsrx/oxc ^0.20.0`), oxfmt, knip `^6.40.0`, happy-dom `^20`
- Cargo-side tools: nextest, cargo-llvm-cov, cargo-machete, clippy (workspace `-D warnings`), rustdoc `-D warnings`, cargo-chef, `cross`, nfpm `2.47.0`, mold, protoc

### Test Coverage
- **Test Directories**: `crates/oxidean-api/tests/` (105 integration files), `crates/oxidean-db/tests/` (27 dialect-parity files), `crates/oxidean-cli/tests/` (4: `commands_e2e`, `manifest_gate`, `rpc_http`, `self_update_http`), `crates/oxidean-git/tests/disk_usage.rs`, colocated `#[cfg(test)]` units in every crate; `apps/web/src/**/*.unit.test.ts`, `*.integration.test.{ts,tsx}`, `*.browser.test.{ts,tsx}` (155 test files total), `apps/web/e2e/{stack,stack-browser}/`, `packages/api-client/src/index.test.ts`
- **Test Frameworks**: cargo-nextest (`.config/nextest.toml` — `default` + `ci` profiles, fail-fast=false in CI), `cargo test` fallback; Vitest 5 projects `unit`(node)/`integration`(happy-dom)/`browser`(Playwright Chromium)/`e2e-stack`/`e2e-stack-browser` (`apps/web/vitest.config.ts`); wiremock for HTTP fixtures; docker-compose dev-auth stack (Mailpit/OIDC/Resend/WorkOS stubs) for e2e
- **Coverage Config**: present — `scripts/coverage-weighted.sh` + `coverage-e2e-checklist.sh` + `coverage-weighted.contract.sh` (weighted gate: unit 25% / integration 40% / e2e checklist 35%, floor `COVERAGE_WEIGHTED_FLOOR=0.70`), `make coverage-rust` (cargo-llvm-cov → `var/coverage/rust-*` artifacts), `make coverage-web` (vitest v8), route-coverage manifest + browser-coverage change-aware gates (`scripts/route-coverage-check.ts`, `browser-coverage-check.ts`, `ui-coverage-change.contract.sh`)

### Code Quality Indicators
- **Linting**: oxlint `--type-aware` + oxfmt (`apps/web/package.json` scripts, `make web-lint`/`web-format-check`), `scripts/check-octane-dom-races.ts`; clippy `-D warnings` + cargo-machete + `-D dead_code` + rustdoc gate (`make dead-code-check`, ci `dead-code` job); knip (`knip.ts` with `.tsrx`/`.astro` compilers); checkmake (`checkmake.ini`, `make makefile-lint` — the first CI job everything `needs`)
- **CI/CD**: `.github/workflows/ci.yml` (12 jobs: makefile-lint → api-rust 3-shard nextest, web-octane, dead-code, web-browser, route-coverage, browser-coverage, coverage-weighted, e2e-stack, actions-e2e, rpc-sync, compose, docker-ci-images, compose-smoke ×3 dialects, smoke-protocol, db-matrix ×3 dialects, build-metrics); `ox-cli.yml` (7 jobs above); `production-deploy.yml`; `publish-runner.yml`. No dependabot/renovate config.
- **Documentation**: strong — `README.md`, `CONTRIBUTING.md`, `AGENTS.md`, `ROADMAP.md`, `SECURITY.md`, `SUPPORT.md`, `docs/` (ARCHITECTURE, API, CLI, CLI-PARITY, CODE_PRACTICES, CONFIGURATION, database, DEPLOYMENT, dev-auth, DEVELOPMENT, GETTING-STARTED, GIT-PROTOCOL, MCP, TESTING, WEBMCP + `guides/` + `openapi.yaml`), module-level `//!` doc comments throughout Rust sources, generated docs drift-gated in CI (`rpc-sync-check`, `openapi-sync-check`, `skill-sync-check`, `cli-parity-check`, route-sync, stack-presets sync, internal-anchors)

### Technical Debt Signals
- **`env!("CARGO_PKG_VERSION")` conflation** — `/cli/latest` (`cli_dist.rs:131`) reports `oxidean-api`'s version while serving `oxidean-cli` binaries; `system.manifest` `server_version`/`system.health` `version`/MCP `serverInfo` are all `oxidean-api`; `ox --version`, UA headers, `clientInfo`, self-update `current` are all `oxidean-cli`. Today all resolve to the one workspace version — per-component versioning must decide each surface's semantics (server vs CLI vs dist-channel)
- **`ox-cli.yml` path filter misses `crates/oxidean-core/**`** (lines 26-35, 38-47) though `oxidean-cli` depends on it — a core change won't trigger the CLI build/release pipeline; same class of gap the intent's dependency-aware detection must fix
- **`OXIDEAN_RPC_VERSION` env var is dead config** — documented (`.env.example:19`, `docker-compose.yml:63`, `docs/CONFIGURATION.md:17`) but never read; the wire version is compile-time `RPC_PROTOCOL_VERSION`
- **`MIN_CLI_VERSION` is a hand-bumped const** (`rpc.rs:121`) with hardcoded `"0.1.0"` asserts in tests — needs policy for when it bumps
- **Dev-version scheme `0.0.0.dev.{run_id}`** (ox-cli.yml:375) is packaging-only; `version_schema: none` in `packaging/nfpm.yaml:6` means nfpm doesn't validate semver shape
- **No changelog infrastructure** — `gh release create --generate-notes` only; no CHANGELOG.md, cliff/release-plz config, or tag convention beyond `v*.*.*` glob
- **No commit-msg enforcement** — no git hooks at all (`core.hooksPath` unset, no husky/lefthook); informal conventional prefixes (~94% adherence, drifting scopes like `fix(11.1)`)
- **No semver git tags exist** — only rolling `ox-dist`; `ox` release job has never produced a tagged release from this repo state
- **Duplicated MCP protocol-version lists** — `apps/web/src/lib/webmcp.ts:2` vs `crates/oxidean-web/src/wellknown.rs:15` (comment-flagged manual sync, no CI gate)
- **Version skew between server-advertised `version` and staged binaries** — `/cli/latest` reports crate version but `dist_commit` (SOURCE_COMMIT) is the real provenance; `ox self-update` already compensates by comparing sha256 too (self_update.rs:50-57)
- Very low TODO/FIXME density (5 hits in crates, 0 in apps/web+packages); stack-preset templates' internal `version` fields (`0.0.0`/`1.0.0`) are scaffold defaults — out of version scope but will collide with naive repo-wide version rewrites

## Handoff Summary
- **Intent-relevant finding**: the repo already ships a complete tag-triggered release pipeline (`.github/workflows/ox-cli.yml`: `v*.*.*` tags → 10-target build matrix → nfpm packages → `gh release create --generate-notes` + rolling `ox-dist` refresh + staging redeploy), and **every versioned surface currently funnels into exactly two sources**: `Cargo.toml:22` `workspace.package.version = "0.1.0"` (all 7 crates via `version.workspace = true`, consumed through ~10 `env!("CARGO_PKG_VERSION")` sites across `oxidean-cli` and `oxidean-api`) and the two `"version": "0.1.0"` package.json fields (`apps/web`, `packages/api-client`). Wire-contract versions are separate constants: `RPC_PROTOCOL_VERSION = 1` (`oxidean-core/src/lib.rs:57`, mirrored into generated `packages/api-client/src/index.ts:3`), `MIN_CLI_VERSION = "0.1.0"` (`oxidean-api/src/rpc.rs:121`), OpenAPI `info.version = '1'` (`rest/spec.rs:335`), and date-versioned MCP protocol lists. Version consumers: `ox --version`, `User-Agent: ox/{v}`, `ox self-update` (version+sha256 compare vs `/cli/latest`), `system.manifest` (`server_version`, `min_cli_version`, `protocol_version`), `system.health` → web status page, MCP `clientInfo`/`serverInfo`, nfpm `@PKG_VERSION@`, GHCR runner tags, and the SKILL.md `compatibility: ox 0.1+` claim.
- **Risks / follow-up**: (1) splitting workspace inheritance into per-crate versions makes `/cli/latest`'s `version` ambiguous — it reports the API's version but ships CLI binaries; self-update semantics (version OR sha256 change → swap) and `MIN_CLI_VERSION` bump policy need explicit design decisions; (2) dependency-aware change detection has a concrete precedent AND a concrete gap — `.railway/railway.ts` watchPatterns already list transitive crate deps for the api image, but `ox-cli.yml` path filters omit `crates/oxidean-core/**`; (3) the internal dep graph to drive propagation: api→{core,db,git}, cli→{core}, db→{core}, web-app→api-client; core is the highest-fan-out node; (4) commit-prefix enforcement lands on a history that is ~94% conventional but never enforced — no git hooks exist at all, so hook distribution (core.hooksPath/bootstrap) is greenfield; (5) dead `OXIDEAN_RPC_VERSION` env config and hand-synced MCP version lists are pre-existing debt the design should either absorb or explicitly exclude; (6) `0.0.0.dev.*` nfpm dev-versioning and `version_schema: none` may conflict with strict SemVer packaging once real versions flow.
