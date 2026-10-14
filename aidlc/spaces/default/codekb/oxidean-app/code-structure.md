# Code Structure — Oxidean monorepo

## Repository Layout

```
oxidean-app/
├── crates/            # Cargo workspace (7 members, resolver 2)
├── apps/web/          # @oxidean/web — Astro + Octane SPA
├── packages/api-client/  # @oxidean/api-client — generated TS RPC client
├── docker/            # oxidean-runner image + compose assets
├── deploy/{cloud,traefik}/  # gateway (Caddy) / dev-proxy topologies
├── packaging/nfpm.yaml      # deb/rpm/apk/pkg.tar.zst packaging
├── scripts/           # ~40 check/release/deploy scripts (TS + sh)
├── docs/            # ARCHITECTURE, API, CLI, CONFIGURATION, TESTING, openapi.yaml …
├── .github/workflows/ # ci.yml, ox-cli.yml, production-deploy.yml, publish-runner.yml
├── .railway/railway.ts    # Railway service defs + watchPatterns (IaC)
├── .cursor/ .claude/ .agents/  # AI-DLC harness + skills (engine, knowledge, hooks)
├── aidlc/             # AI-DLC workspace (memory, intents, codekb)
├── tmp/  var/  brand/ # gitignored scratch, coverage artifacts, brand assets
└── Cargo.toml package.json turbo.json Makefile knip.ts Cross.toml checkmake.ini
```

## Crate Organization (`crates/`)

| Crate | Kind | Notable source layout |
|---|---|---|
| `oxidean-api` | lib + 4 bins (`oxidean-api`, `rpc-gen`, `openapi-gen`, `oxidean-protection-hook`) | `src/rpc.rs` (PROCEDURES const + ~285 dispatch arms + `check_version_header` + `MIN_CLI_VERSION`), `src/rest/` (per-domain `RouteDef` modules: admin/issues/orgs/pulls/releases/repos/statuses/users + `spec.rs`/`openapi.rs`), `src/mcp.rs`, `src/routes/` (incl. `cli_dist.rs`), `src/bin/{rpc_gen,openapi_gen}.rs`, `assets/{cli,stack-presets}` embedded via `include_dir`, `tests/` (105 integration files), `proto/runner.proto` + `build.rs` (prost-build), `Dockerfile` (cargo-chef) |
| `oxidean-cli` | lib + bin (`ox`) | `src/{main,args,commands,http,rpc,self_update,manifest,mcp}.rs`, `skill/SKILL.md` (embedded, served as `/cli/skill.md`), `tests/` (4 files incl. `manifest_gate.rs`, `self_update_http.rs`) |
| `oxidean-core` | lib | `src/lib.rs` — domain types, RPC envelope (`RpcRequest`/`RpcResponse`), `RPC_PROTOCOL_VERSION`, `ManifestResponse`/`HealthResponse` (schemars) |
| `oxidean-db` | lib + bin (`migrate`) | `src/` + `migrations/{postgres,mysql,sqlite}/` (46 postgres migrations, mirrored per dialect) — sole dialect-branching crate |
| `oxidean-git` | lib | `GitBackend` trait + `CliGitBackend`; `src/version.rs` (git ≥ 2.5.0 boot gate) |
| `oxidean-runner` | bin | `src/main.rs` — Actions runner, custom JSON protocol; `docker/oxidean-runner/Dockerfile` |
| `oxidean-web` | bin | `src/{main,proxy,wellknown}.rs` — static-shell server, meta injection, auth gates, `SHELLS` route corpus, `/health`, API prefix proxy |

## Frontend / Package Organization

- `apps/web/` — Astro static shells in `src/pages/**`; Octane `.tsrx` islands
  (213 files) in `src/{routes,components,lib}`; `src/lib/session-queries.ts`
  (TanStack Query helpers), `src/lib/webmcp.ts`; `e2e/{stack,stack-browser}/`;
  `vitest.config.ts` (5 projects); `public/sw.js` (commit-SHA cache busting).
- `packages/api-client/` — `src/index.ts` (5427 lines, AUTO-GENERATED —
  never hand-edited) + hand-written `src/{languages,scope-catalog}.ts` +
  `src/index.test.ts`.

## File Classification

| Class | Patterns / locations | Treatment |
|---|---|---|
| Rust source | `crates/*/src/**/*.rs` | edition 2021; module `//!` docs; `#[cfg(test)]` colocated units |
| Rust integration tests | `crates/*/tests/*.rs` (137 files total) | cargo-nextest; some gate version surfaces (`manifest_gate.rs`, `rpc_http.rs`) |
| Octane components | `apps/web/**/*.tsrx` | NOT React JSX — `@if`/`@else`/`@for`, `function Comp() @{…}` |
| Web tests | `*.unit.test.ts`, `*.integration.test.{ts,tsx}`, `*.browser.test.{ts,tsx}`, `e2e/**` | Vitest 5 projects (node/happy-dom/Chromium/stack) |
| Generated (do-not-edit) | `packages/api-client/src/index.ts`, `docs/openapi.yaml`, `/cli/skill.md` serving path | regenerated via `make rpc-gen` / `openapi-gen`; CI drift gates |
| Embedded assets | `crates/oxidean-api/assets/{cli,stack-presets}/**`, `crates/oxidean-cli/skill/` | `include_dir` into binary; preset manifests contain scaffold-only versions — excluded from workspace |
| Build/CI config | `Cargo.toml`, `package.json`, `turbo.json`, `Makefile`, `nfpm.yaml`, `docker-compose*.yml` (5), `.railway/railway.ts`, `.github/workflows/*.yml`, `knip.ts`, `.config/nextest.toml`, `checkmake.ini`, `Cross.toml` | root manifests own build/test/lint/release wiring |
| Git hooks | **none** — `core.hooksPath` unset, no husky/lefthook | hook distribution would be greenfield |

## Code Patterns

- **Workspace version inheritance** — every crate sets
  `version.workspace = true` → all resolve to `[workspace.package] 0.1.0`;
  version surfaces are `env!("CARGO_PKG_VERSION")` embeds (~10 sites across
  `oxidean-cli` and `oxidean-api` — enumerated in `api-documentation.md`).
- **Contract-first codegen** — Rust types/constants are source of truth;
  TS client and OpenAPI doc are emitted artifacts with sync-check gates.
- **Route tables + dispatch mirrors** — REST uses `RouteDef` tables per
  domain module; RPC uses `PROCEDURES` const mirrored 1:1 against dispatch
  match arms (asserted by tests at `rpc.rs:1702-1712`).
- **Trait isolation** — `GitBackend` trait hides the system-git
  implementation; DB dialect confined to `oxidean-db`.
- **Compile-time embeds** — `env!("CARGO_PKG_VERSION")`, `include_dir`
  assets, `SOURCE_COMMIT`/`dist_commit` provenance files.
- **Version constants, not config** — `RPC_PROTOCOL_VERSION` (u32),
  `MIN_CLI_VERSION` (&str), OpenAPI `info.version`, MCP date-versions are
  all code constants; the `OXIDEAN_RPC_VERSION` env var is dead config.
- **Conventional-commit-style subjects** — ~94% of recent commits match
  `<type>(<scope>):`, informal and unenforced (drifting scopes like
  `fix(11.1)`).
