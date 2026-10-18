# Code Structure — Oxidean monorepo

## Repository Layout

```
oxidean-app/
├── crates/            # Cargo workspace (7 members, resolver 2)
├── apps/web/          # @oxidean/web — Astro shells + Octane .tsrx islands
├── packages/api-client/  # @oxidean/api-client — generated TS RPC client
├── docker/            # ci-base + oxidean-runner images, release layer
├── deploy/{cloud,traefik}/  # Caddy cloud gateway / dev-proxy topologies
├── packaging/nfpm.yaml      # deb/rpm/apk packaging
├── scripts/           # ~40 check/smoke/coverage scripts (TS + sh)
├── docs/            # ARCHITECTURE, API, MCP, GIT-PROTOCOL, CONFIGURATION …
├── .github/workflows/ # ci.yml, ci-base.yml, images.yml, ox-cli.yml, release*.yml …
├── .railway/railway.ts    # Railway service defs + watchPatterns (IaC)
├── docker-compose*.yml    # 5 compose files (pg/mysql/sqlite/dev-auth)
├── .cursor/ .claude/ .agents/  # AI-DLC harness shells + vendored skills
├── aidlc/             # AI-DLC workspace (memory, intents, codekb)
├── tmp/  var/  brand/ # gitignored scratch, coverage artifacts, brand assets
└── Cargo.toml package.json turbo.json versioning.toml Makefile knip.ts
    Cross.toml checkmake.ini .config/nextest.toml .githooks/commit-msg
```

## Crate Organization (`crates/`)

| Crate | Kind | Notable source layout |
|---|---|---|
| `oxidean-api` | lib + 4 bins (`oxidean-api`, `rpc-gen`, `openapi-gen`, `oxidean-protection-hook`) | `src/app.rs` (route mount table `app.rs:268-407`, `AppState`, `RpcCtx`), `src/rpc.rs` (`PROCEDURES` const `rpc.rs:150-457` = 306 entries + dispatch match + `rpc_status`), `src/mcp.rs` (self-contained MCP server, ~1,820 LOC), `src/rest/` (`RouteDef` tables in `rest/{repos,issues,pulls,orgs,users,releases,statuses,admin}.rs` + `spec.rs` route table → OpenAPI), `src/routes/git_smart_http.rs` + `src/git/http_backend.rs` (CGI bridge), `src/ssh/` (russh listener), `src/lfs/`, `src/packages/` (oci/npm/generic), `src/oauth/`, `src/pat/` (bearer + rate_limit), `src/repo/` (acl + files), `src/mirror/`, `src/jobs/`, `src/bin/{rpc_gen,openapi_gen,protection_hook}.rs`, `build.rs` (prost-build), `proto/runner.proto`, `assets/{cli,stack-presets,workflow-templates}` via `include_dir`, `Dockerfile` (cargo-chef), `tests/` (107 integration files + `support/mod.rs` harness) |
| `oxidean-core` | lib | `src/lib.rs` — domain/DTO types, wire envelope `RpcRequest`/`RpcResponse`/`AppError`, `WIRE_VERSION` + `wire_compatible` (SemVer gate, `0.1.0`), `MCP_PROTOCOL_VERSIONS`; `src/proc_classes.rs` — procedure→PAT-scope classification (fail-closed `SessionOnly`) |
| `oxidean-db` | lib + bin (`migrate`) | `src/lib.rs` façade `Database` (4,941 LOC, ~50 domain modules, `Clone` + `skipped()` no-DB mode), `src/pool.rs` (`DbPool` enum over sqlx Pg/MySql/Sqlite), `src/dialect.rs` (URL→dialect + `in_placeholders`), `src/migrate.rs` (per-dialect `sqlx::migrate!` + filename-parity test), `src/repositories.rs` (row-mapping macro idiom), `migrations/{postgres,mysql,sqlite}/` (50 numbered SQL files each), `tests/` (~30 dialect-parity files) |
| `oxidean-git` | lib | `src/backend.rs` `GitBackend` async trait (~40 methods, `backend.rs:410-771`), `src/cli.rs` `CliGitBackend` (5,943 LOC; argv-only spawns, never `sh -c`; `cat-file --batch`; protection-hook installer), `src/version.rs` (boot `git --version` ≥2.5 gate) |
| `oxidean-cli` | lib + bin (`ox`) | `src/args.rs` hand-rolled parser (~4,155 LOC; `Command` enum + USAGE table, NOT clap; `ox api` raw-RPC escape hatch `args.rs:3386`), `src/commands.rs` (~4,053 LOC; `required_procedures` manifest gate `commands.rs:481`), `src/rpc.rs` (typed `/api/rpc` client), `src/config.rs` (XDG config + keyring), `src/mcp.rs` (`ox mcp install/status/remove` writes harness MCP configs), `src/skill.rs`, `src/self_update.rs`, `src/device.rs` (RFC 8628), `tests/` (4 files) |
| `oxidean-web` | bin | `src/main.rs` (env wiring, health probe gate), `src/proxy.rs` (`API_PREFIXES` + `is_api_git_path` `proxy.rs:16-86`, forwarded headers, WS upgrade splice), `src/session.rs` (session-check probe + stamp), `src/{shells,inject,wellknown}.rs` |
| `oxidean-runner` | bin | `src/main.rs` — register→poll→exec→report loop, host exec + `docker://` step labels; `docker/oxidean-runner/Dockerfile` |

## Frontend / Package Organization

- `apps/web/` — Astro static shells `src/pages/**/*.astro` ($param syntax);
  Octane `.tsrx` islands `src/{routes,components,lib}` — 226 `.tsrx` total
  (142 under `src/components/`); `src/lib/session-queries.ts` (TanStack
  Query helpers), `src/lib/webmcp.ts`; `e2e/{stack,stack-browser}/`;
  `vitest.config.ts` (5 projects); `src/test/` route/browser coverage
  manifests; `public/`.
- `packages/api-client/` — `src/index.ts` (~6,000 lines, AUTO-GENERATED —
  never hand-edited) + hand-written `src/scope-catalog.ts` + tests; emitted
  by `rpc-gen`.

## File Classification

| Class | Patterns / locations | Treatment |
|---|---|---|
| Rust source | `crates/*/src/**/*.rs` | edition 2021; module `//!` docs naming decision IDs (D-12, GIT-09 …); `#[cfg(test)]` colocated units (~1,252 test attrs workspace-wide) |
| Rust integration tests | `crates/*/tests/*.rs` (141 files: 107 api + ~30 db + 4 cli) | cargo-nextest, `tests/support/mod.rs` shared harness (env locks, sysadmin seeding) |
| Octane components | `apps/web/**/*.tsrx` | NOT React JSX — `function Comp() @{…}`, `@if`/`@else`/`@for`, native `onInput` |
| Web tests | `*.unit.test.ts`, `*.integration.test.{ts,tsx}`, `*.browser.test.{ts,tsx}`, `e2e/**` | Vitest 5 projects (node/happy-dom/Chromium/stack) — ~183 `*.test.*` files |
| Generated (do-not-edit) | `packages/api-client/src/index.ts`, `docs/openapi.yaml` | regenerated via `make rpc-gen` / `openapi-gen`; CI drift gates |
| Embedded assets | `crates/oxidean-api/assets/{cli,stack-presets,workflow-templates}/**`, `crates/oxidean-cli/skill/` | `include_dir` into binary |
| Build/CI config | root manifests + `docker-compose*.yml` ×5 + `.github/workflows/*.yml` ×7 + `versioning.toml` + `packaging/nfpm.yaml` | root manifests own build/test/lint/release wiring |
| Migrations | `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/NNNN_*.sql` ×50 | hand-ported ×3; filename parity enforced by `migrate.rs` test; bodies can drift |
| Git hooks | `.githooks/commit-msg` (installed by `make setup`) | SemVer `type(scope): subject` grammar enforced locally + CI `commit-lint` |

## Code Patterns

- **Per-component SemVer registry** — `versioning.toml` declares 10
  components; a virtual `wire` component pins `crates/oxidean-core/src/lib.rs
  #WIRE_VERSION` and `oxidean-api`, `oxidean-web`, `ox`, `api-client`, `web`
  all `depends_on = ["wire"]`. Commit subjects (`type(scope): subject`)
  drive bumps + changelogs (`docs/VERSIONING.md`).
- **Contract-first codegen** — Rust types/constants are source of truth;
  TS client and OpenAPI doc are emitted artifacts with sync-check gates.
  `rpc_gen.rs` embeds a ~6,100-line hand-maintained TS template (drift
  guarded by `make rpc-sync-check` + `procedures_list_matches_dispatch`
  test scanning `rpc.rs` match arms vs `PROCEDURES`).
- **Dispatch-table idiom** — every dynamic surface is a static table
  mirrored against dispatch: `PROCEDURES`↔match arms, `RouteDef` rows→
  `rpc::dispatch` re-entry (no duplicated business logic), `tool_defs()`→
  `build_rpc_input`, `required_procedures` per CLI command vs
  `system.manifest`.
- **Trait isolation** — `GitBackend` async trait hides system-git;
  `Database` façade hides dialects; `EmailSender`/`SessionService` traits
  for side-effects. New engines are expected behind `Arc<dyn Trait>` on
  `AppState`.
- **Env-dir + refcount content stores** — `OXIDEAN_REPOS_DIR`,
  `OXIDEAN_LFS_DIR` (OID-sharded + refcount + GC job), `OXIDEAN_PACKAGES_DIR`
  (digest-sharded + refcount + quotas): the pattern a new object store
  clones (`AppState::new` env parsing at `app.rs:95-152`).
- **Credential precedence + limiters** — PAT sha256 lookup shared across
  transports; `FailedAuthLimiter` (`pat/rate_limit.rs`) plus per-process
  limiters on `AppState`.
- **Fail-closed PAT scoping** — `proc_classes` classifies every procedure;
  unlisted = `SessionOnly` (machine tokens can't call it by default).
