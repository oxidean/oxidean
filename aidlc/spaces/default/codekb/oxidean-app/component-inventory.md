# Component Inventory — Oxidean monorepo

The 10 versionable components (7 Cargo crates + 2 Bun packages + workspace
root). Internal edge semantics are detailed in `dependencies.md`; wire/API
contracts in `api-documentation.md`; versions per component are the "Version
source" column — today everything funnels to `0.1.0`.

| Component | Manifest | Type | Version source | Internal deps | Health |
|---|---|---|---|---|---|
| `oxidean-api` | `crates/oxidean-api/Cargo.toml` | lib + 4 bins | `version.workspace = true` → `0.1.0` | `oxidean-core`, `oxidean-db`, `oxidean-git` | at-risk |
| `oxidean-cli` | `crates/oxidean-cli/Cargo.toml` | lib + bin (`ox`) | `version.workspace = true` → `0.1.0` | `oxidean-core` | at-risk |
| `oxidean-core` | `crates/oxidean-core/Cargo.toml` | lib | `version.workspace = true` → `0.1.0` | — | healthy |
| `oxidean-db` | `crates/oxidean-db/Cargo.toml` | lib + bin (`migrate`) | `version.workspace = true` → `0.1.0` | `oxidean-core` | healthy |
| `oxidean-git` | `crates/oxidean-git/Cargo.toml` | lib | `version.workspace = true` → `0.1.0` | — | healthy |
| `oxidean-runner` | `crates/oxidean-runner/Cargo.toml` | bin | `version.workspace = true` → `0.1.0` | — | healthy |
| `oxidean-web` | `crates/oxidean-web/Cargo.toml` | bin | `version.workspace = true` → `0.1.0` | — | healthy |
| `@oxidean/web` | `apps/web/package.json` | app (private) | `"version": "0.1.0"` | `@oxidean/api-client` | healthy |
| `@oxidean/api-client` | `packages/api-client/package.json` | lib (private) | `"version": "0.1.0"` | peer `@octanejs/tanstack-query` | healthy |
| `oxidean` (root) | `package.json` | workspace root | **no `version` field** | Bun workspaces `apps/*`, `packages/*` | healthy |

## Component Details

## oxidean-api

- **Responsibility**: the backend. Axum server hosting typed JSON-RPC (283
  procedures), `/api/v1` REST + OpenAPI, MCP endpoint, OAuth/OIDC, git Smart
  HTTP/LFS/SSH, package registries (OCI/npm/generic), `/cli/*` ox
  distribution, Actions runner protocol.
- **Extra binaries**: `rpc-gen`, `openapi-gen` (contract codegen),
  `oxidean-protection-hook`; also embeds `assets/{cli,stack-presets}`.
- **Owns version surfaces**: `MIN_CLI_VERSION`, `system.manifest`,
  `system.health`, `serverInfo`, `/cli/latest` (see
  `api-documentation.md` § Version & Contract Surfaces).
- **Health: at-risk** — largest crate (105 test files, monolithic `rpc.rs`
  with ~285 dispatch arms); `/cli/latest` version/binary conflation and
  hand-bumped `MIN_CLI_VERSION` are flagged debt; `ox-cli.yml` release
  trigger omits its `oxidean-core` transitive dep from path filters.

## oxidean-cli

- **Responsibility**: `ox` companion CLI — typed RPC client, device auth,
  self-update (version OR sha256 → swap), MCP client config, embedded agent
  `skill/SKILL.md` distributed via `/cli/skill.md` and `ox skill`.
- **Owns version surfaces**: `ox --version`, `User-Agent: ox/{v}`,
  `clientInfo.version`, self-update `current`.
- **Health: at-risk** — self-update semantics depend on `/cli/latest`
  reporting a meaningful version; per-component SemVer must decide whether
  that field is the api's or cli's version.

## oxidean-core

- **Responsibility**: shared domain types, RPC envelope
  (`RpcRequest`/`RpcResponse`), `RPC_PROTOCOL_VERSION`, manifest/health
  contract types (schemars-derived).
- **Health: healthy** — but highest fan-out: `api`, `cli`, `db` all path-depend
  on it, and `ox-cli.yml` path filters miss `crates/oxidean-core/**`
  (a core change won't trigger the CLI release pipeline).

## oxidean-db

- **Responsibility**: sole persistence crate — sqlx multi-dialect
  (Postgres/MySQL/SQLite), migrations (46 postgres + mirrors), `migrate` bin.
  Hard boundary: only crate allowed dialect SQL.
- **Health: healthy** — 27 dialect-parity test files + 3-dialect CI matrix.

## oxidean-git

- **Responsibility**: `GitBackend` trait + `CliGitBackend` shelling to
  system git; `version.rs` enforces git ≥ 2.5.0 at boot.
- **Health: healthy**.

## oxidean-runner

- **Responsibility**: self-hosted Actions runner binary; custom JSON
  protocol to `/api/actions/**`; published to GHCR via
  `publish-runner.yml` (branch/tag/sha/latest image tags — the only
  component already getting semver image tags from `v*.*.*` git tags).
- **Health: healthy** — `DOCKER_CLI_VERSION=29.9.0` pin in Dockerfile.

## oxidean-web

- **Responsibility**: Rust binary serving the Astro `dist` SPA — meta
  injection (theme/title/session), protected-route auth gates, `/health`,
  well-known endpoints, API prefix proxy (`OXIDEAN_API_ORIGIN`),
  `SHELLS` route corpus synced with `apps/web/src/pages/**`.
- **Health: healthy** — note duplicated `MCP_PROTOCOL_VERSIONS` list
  (hand-synced with `apps/web/src/lib/webmcp.ts`, no gate).

## @oxidean/web

- **Responsibility**: the web UI — Astro static shells + 213 Octane `.tsrx`
  islands; TanStack Query/Form via `@octanejs/*`; WebMCP browser surface;
  155 test files across unit/integration/browser/e2e projects.
- **Version**: private `"0.1.0"` in `apps/web/package.json`; not published
  to npm — version only surfaces in build provenance/SW cache name.
- **Health: healthy**.

## @oxidean/api-client

- **Responsibility**: generated typed RPC client (`src/index.ts`, 5427
  lines, AUTO-GENERATED) + hand-written `languages.ts`, `scope-catalog.ts`.
  Generated from `oxidean-api` by `rpc-gen`; drift-gated.
- **Health: healthy** — private package, but its `version` field is a
  versionable surface under per-component SemVer.

## oxidean

- **Responsibility**: Bun workspace root (`workspaces: [apps/*, packages/*]`),
  `packageManager: bun@1.4.0`, shared scripts; no `version` field.
- **Health: healthy** — included in the inventory because release tooling
  may need a root version policy decision (e.g., private root stays
  versionless vs. umbrella version).

## Dependency Edge Summary

```
oxidean-core ◄── oxidean-api, oxidean-cli, oxidean-db   (highest fan-out)
oxidean-db   ◄── oxidean-api
oxidean-git  ◄── oxidean-api
@oxidean/api-client ◄── @oxidean/web        (workspace:*)
oxidean-api --rpc-gen──► @oxidean/api-client (generated-source edge)
```

Full graph including generated-source edges, Docker build deps, and
`watchPatterns` precedent: `dependencies.md`.
