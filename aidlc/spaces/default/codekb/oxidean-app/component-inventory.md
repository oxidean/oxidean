# Component Inventory — Oxidean monorepo

10 components (7 Cargo crates + 2 Bun packages + workspace root), all
registered in `versioning.toml` for per-component SemVer (a virtual `wire`
component tracks `WIRE_VERSION`; `oxidean-api`, `oxidean-web`, `ox`,
`api-client`, `web` all `depends_on = ["wire"]`). Internal edge semantics
in `dependencies.md`; wire contracts in `api-documentation.md`; the
"Intent touch" column marks what the agentic-VCS intent (new dedicated
crate + RPC/MCP/CLI surfaces) is expected to extend.

| Component | Manifest | Type | Internal deps | Intent touch |
|---|---|---|---|---|
| `oxidean-api` | `crates/oxidean-api/Cargo.toml` | lib + 4 bins | `oxidean-core`, `oxidean-db`, `oxidean-git` | High — routes/RPC/MCP mounts for the new protocol |
| `oxidean-cli` | `crates/oxidean-cli/Cargo.toml` | lib + bin (`ox`) | `oxidean-core` | High — new `ox vcs …` command family |
| `oxidean-core` | `crates/oxidean-core/Cargo.toml` | lib | — | High — VCS wire types + `proc_classes` entries |
| `oxidean-db` | `crates/oxidean-db/Cargo.toml` | lib + bin (`migrate`) | `oxidean-core` | High — new tables ×3 dialects + façade methods |
| `oxidean-git` | `crates/oxidean-git/Cargo.toml` | lib | — | None — sibling, not substrate (trait is the model) |
| `oxidean-runner` | `crates/oxidean-runner/Cargo.toml` | bin | — | Low/none |
| `oxidean-web` | `crates/oxidean-web/Cargo.toml` | bin | — | Med — `API_PREFIXES`/`is_api_git_path` for any new prefix |
| `@oxidean/web` | `apps/web/package.json` | app (private) | `@oxidean/api-client` | Med — optional UI surfaces for the new protocol |
| `@oxidean/api-client` | `packages/api-client/package.json` | lib (private, generated) | peer `@octanejs/tanstack-query` | Med — regenerated once `vcs.*` procedures land |
| `oxidean` (root) | `package.json` + `Cargo.toml` + `versioning.toml` | workspace root | Bun workspaces `apps/*`, `packages/*`; Cargo members | Med — new crate must join `[workspace.members]` + `versioning.toml` |

## oxidean-api

- **Responsibility**: the backend. Axum 0.8 server hosting typed JSON-RPC
  (306 procedures), `/api/v1` REST + OpenAPI, MCP endpoint (20 tools),
  OAuth2 provider + WorkOS/OIDC, git Smart HTTP/LFS/russh SSH, package
  registries (OCI/npm/generic), `/cli/*` distribution, Actions runner
  protocol, background jobs.
- **Extra binaries**: `rpc-gen`, `openapi-gen`, `oxidean-protection-hook`;
  embeds `assets/{cli,stack-presets,workflow-templates}`.
- **Key seams for intent**: `router_with_state` mount table
  (`app.rs:268-407`) for new routes; `AppState` `Arc<dyn …>` slot pattern
  (`app.rs:62,186`); `main.rs` aux-listener spawn precedent (SSH,
  `main.rs:83`); `jobs/mod.rs` for GC/background tasks.
- **Health: at-risk** — largest crate (`rpc.rs` 1,946 LOC, `mcp.rs`
  1,820 LOC, 107 test files); five parallel registration surfaces per
  procedure family (see `api-documentation.md` §1).

## oxidean-cli

- **Responsibility**: `ox` companion CLI — hand-rolled parser/dispatch
  (NOT clap), typed RPC client (Bearer PAT), XDG config + keyring
  credentials, RFC 8628 device auth, `ox mcp`/`ox skill` harness
  installers, git credential helper, self-update, `ox api` raw-RPC
  escape hatch (`args.rs:3386`).
- **Key seams for intent**: `Command` enum variants + USAGE table rows
  (`args.rs` ~262) for an `ox vcs …` family; `required_procedures`
  manifest gate (`commands.rs:481`) wires each command to server
  procedures.
- **Health: at-risk** — `args.rs`/`commands.rs` are ~4,155/~4,053 LOC
  monoliths; adding a command family means touching large files.

## oxidean-core

- **Responsibility**: shared domain/DTO types, wire envelope
  (`RpcRequest`/`RpcResponse`/`AppError`), `WIRE_VERSION` +
  `wire_compatible` (SemVer `0.1.0`, legacy `"1"` accepted),
  `MCP_PROTOCOL_VERSIONS`, `proc_classes` (procedure→PAT-scope
  classification, fail-closed `SessionOnly`). No I/O.
- **Key seams for intent**: new VCS wire/DTO types live here;
  **every token-callable `vcs.*` procedure needs a `proc_classes`
  entry** or PATs can't call it.
- **Health: healthy** — highest fan-out (api/cli/db/web path-depend on it).

## oxidean-db

- **Responsibility**: sole persistence crate — sqlx 0.9 multi-dialect
  (Postgres/MySQL/SQLite) behind façade `Database` (`Clone`,
  `skipped()` no-DB mode); ~50 domain modules + row types;
  `migrations/{postgres,mysql,sqlite}/` 50 files each (filename parity
  enforced by test); `migrate` bin. Hard boundary: only crate allowed
  dialect SQL.
- **Key seams for intent**: new VCS tables = migration ×3 dialects + a
  `Database` method module + row types; call sites accept `Database` by
  value, never `&DbPool`.
- **Health: healthy** — `lib.rs` is a 4,941 LOC façade hotspot;
  ~30 dialect-parity test files + 3-dialect CI matrix.

## oxidean-git

- **Responsibility**: `GitBackend` async trait (~40 methods,
  `backend.rs:410-771`) + `CliGitBackend` (argv-only spawns of system
  git ≥2.5, never `sh -c`; `cat-file --batch`; protection-hook
  installer); boot `git --version` gate.
- **Key seams for intent**: **the trait is the canonical model** for the
  new VCS engine boundary — `GixGitBackend` is even documented-but-
  unimplemented, proving the seam's purpose. The new crate should NOT
  depend on or modify this one; it defines an analogous boundary.
- **Health: healthy** — `cli.rs` 5,943 LOC; 44 inline test attrs.

## oxidean-runner

- **Responsibility**: self-hosted Actions runner; register→poll→exec→
  report loop against `/api/actions/*` (prost types from
  `proto/runner.proto`); host exec + `docker://` step labels; GHCR image.
- **Health: healthy** — no intent touch identified.

## oxidean-web

- **Responsibility**: Rust binary serving Astro `dist` — meta injection
  (theme/title/session), protected-route gates via
  `/api/auth/session-check`, `/health`, `/.well-known/{webmcp,mcp}`,
  API prefix proxy (`API_PREFIXES` + `is_api_git_path` `proxy.rs:16-86`,
  WS upgrade splice).
- **Key seams for intent**: any new protocol path prefix must be added
  to `API_PREFIXES` (or `is_api_git_path` semantics re-checked) —
  part of the 3-place edge-parity sync with Traefik + Caddyfile
  (prefix-boundary tests exist in `proxy.rs`).
- **Health: healthy** — 5 thorough proxy tests colocated.

## @oxidean/web

- **Responsibility**: the web UI — Astro static shells (`src/pages/**`)
  + Octane `.tsrx` islands (226 files: 142 under `src/components/`,
  rest in `src/routes/` + `src/lib/`); TanStack Query/Form via
  `@octanejs/*`; WebMCP browser surface; ~183 test files across
  unit/integration/browser/e2e projects.
- **Key seams for intent**: optional — new UI surfaces consume
  `@oxidean/api-client` like everything else; `.tsrx` is NOT React JSX.
- **Health: healthy**.

## @oxidean/api-client

- **Responsibility**: generated typed RPC client (`src/index.ts` ~6,000
  lines, AUTO-GENERATED) + hand-written `scope-catalog.ts`; namespaced
  `createClient` method groups + `wireCompatible`; TanStack Query
  helpers. Emitted by `rpc-gen`; drift-gated.
- **Key seams for intent**: regenerated automatically once `vcs.*`
  procedures are added to the `rpc_gen.rs` template — the template
  itself is the manual step.
- **Health: healthy**.

## oxidean

- **Responsibility**: workspace root — Bun `workspaces: [apps/*,
  packages/*]` (`packageManager: bun@1.4.0`), Cargo workspace (7
  members, `resolver="2"`, shared deps/profiles), `versioning.toml`
  component registry, `turbo.json`, `Makefile` (~90 targets).
- **Key seams for intent**: a new crate needs `[workspace.members]` +
  shared-dep decisions + `versioning.toml` registration (11th component;
  decide whether it `depends_on = ["wire"]` if it shares wire types).
- **Health: healthy**.

## Dependency Edge Summary

```
oxidean-core ◄── oxidean-api, oxidean-cli, oxidean-db, oxidean-web   (highest fan-out)
oxidean-db   ◄── oxidean-api
oxidean-git  ◄── oxidean-api
@oxidean/api-client ◄── @oxidean/web            (workspace:*)
oxidean-api --rpc-gen──► @oxidean/api-client    (generated-source edge)
(new VCS crate) ◄── { oxidean-api or aux listener; oxidean-core types; oxidean-db façade }
```

Full graph incl. codegen edges, build deps, and versioning edges:
`dependencies.md`.
