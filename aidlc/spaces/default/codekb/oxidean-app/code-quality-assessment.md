# Code Quality Assessment — Oxidean

## Test Coverage

### Test Inventory

| Area | Location | Size |
|---|---|---|
| API integration tests | `crates/oxidean-api/tests/` | 107 files + `tests/support/mod.rs` shared harness (env locks, sysadmin seeding) |
| DB dialect-parity tests | `crates/oxidean-db/tests/` | ~30 `dialect_*`/`factory_reset_*`/`sqlite_paths` files (same suite across pg/mysql/sqlite) |
| CLI integration tests | `crates/oxidean-cli/tests/` | 4 files: `commands_e2e`, `manifest_gate`, `rpc_http`, `self_update_http` |
| Inline Rust units | `#[cfg(test)]` in every crate | ~1,252 `#[test]`/`#[tokio::test]` attrs under `crates/` (44 in `oxidean-git/cli.rs` alone; 5 thorough proxy tests in `oxidean-web/proxy.rs`; migration filename-parity test in `migrate.rs`) |
| Web tests | `apps/web/src/**/*.{unit,integration,browser}.test.*` | ~183 `*.test.*` files (happy-dom integration + Chromium DOM-race browser) |
| Web e2e | `apps/web/e2e/{stack,stack-browser}/` | HTTP e2e (smtp/oidc/workos) + full-stack Playwright + visual baselines; `commands.ts` shared flow lib |
| API client tests | `packages/api-client/src/index.test.ts` | generated-client smoke |
| Protocol smokes | `scripts/` | `smoke-git-https/ssh/lfs/packages`, `ci-compose-smoke`, `smoke-protocol-ci`, `e2e-actions-pipeline` — run real `git`/`ssh`/`git-lfs` client binaries |

### Frameworks & Harnesses

- **cargo-nextest** (`.config/nextest.toml`, `ci` profile: no fail-fast,
  90s slow-timeout, 5-way `count:N/5` partitioning in CI);
  `cargo-llvm-cov` optional collection.
- **Vitest 5** projects in `apps/web/vitest.config.ts`: `unit`,
  `integration` (happy-dom), `browser` (`@vitest/browser-playwright`
  Chromium), `e2e-stack`, `e2e-stack-browser`;
  `@octanejs/testing-library` + `@testing-library/{dom,jest-dom}`.
- **wiremock** Rust HTTP mocks; real client binaries in protocol smokes.

### Coverage Gates

- `@vitest/coverage-v8` per-project (`test:coverage[:unit|:integration]`,
  reports `apps/web/coverage/{unit,integration}`).
- **Weighted gate** `scripts/coverage-weighted.sh`: unit 0.25 /
  integration 0.40 / e2e-checklist 0.35, floor `0.70`
  (`COVERAGE_WEIGHTED_FLOOR`); e2e checklist
  `scripts/coverage-e2e-checklist.sh`.
- Route-coverage manifest `apps/web/src/test/route-coverage.manifest.ts`
  + `scripts/route-coverage-check.ts` (every `.tsrx` page needs a test or
  documented skip); `browser-coverage.manifest.ts` +
  `scripts/browser-coverage-check.ts` (high-risk UI Chromium proof).
- **Rust coverage is collect-only — no gate** (residual debt).

## Linting & Static Checks

| Check | Tool | Gate |
|---|---|---|
| Rust format | `cargo fmt --check` | CI `rust-fmt` (no committed rustfmt.toml/clippy.toml) |
| Rust dead code/deps | `cargo machete` + `clippy -D dead_code` | `make dead-code-check` |
| Web lint | oxlint `--type-aware` (`oxlint-tsgolint`, `@tsrx/oxc`) + `scripts/check-octane-dom-races.ts` | `make web-lint` — **the** web type gate; plain `tsc --noEmit` doesn't parse `.tsrx` (`@tsrx/typescript-plugin` peer-pinned to TS 5.9 vs app TS 7) |
| Web format | oxfmt + `prettier --check` (`.astro`) | `make web-format-check` |
| TS dead code | knip (custom `.tsrx`/`.astro`/`.css` compilers, `knip.ts`) | `make dead-code-check` |
| Makefile | checkmake (`checkmake.ini`) | `makefile-lint` — first CI job, everything `needs` it |
| Commit grammar | SemVer `type(scope): subject` linter | `.githooks/commit-msg` (`make setup`) + CI `commit-lint` (PR range + title) |
| Contract drift | `check-{rpc-sync,openapi-sync,skill-sync,route-sync,cli-parity,workflow-templates,stack-presets,internal-anchors,makefile}` | dedicated CI jobs (`rpc-sync` regenerates + `git diff --exit-code`) |

## CI/CD

`.github/workflows/`:

| Workflow | Trigger | Jobs / purpose |
|---|---|---|
| `ci.yml` | PRs + main | ~20 jobs: makefile-lint → ci-image digest resolution → commit-lint, rust-fmt, rust-build (shared nextest archive), api-rust (5-way partition), web-octane (lint/format/test/build), dead-code, web-browser, route-coverage, browser-coverage, coverage-weighted (floor 0.70), e2e-stack, actions-e2e, rpc-sync, compose config-check, compose-smoke (pg/mysql/sqlite matrix), smoke-protocol (live git HTTPS+SSH+LFS+packages), db-dialects, build-metrics, rust-artifacts-cleanup |
| `ci-base.yml` | docker/ci-base changes | builds `ghcr.io/<owner>/oxidean-ci:{dev,release,cov,tools}`; `ci.yml` resolves `@sha256` digests per run — **no cross-run Rust cache by design** |
| `images.yml` | pushes/tags | GHCR service images api/web/runner/gateway (sha/pr/tag channels, Railway deploys) |
| `ox-cli.yml` | path-filtered pushes/tags | cross-platform `ox` build matrix (gnu/musl/darwin/windows/freebsd), installer + nfpm package smokes, `ox-v*` releases |
| `release.yml` / `release-beta.yml` | component tags | per-component SemVer tag → GitHub Release (driven by `versioning.toml` registry) |
| `production-deploy.yml` | manual dispatch | promote-by-SHA to Railway prod behind environment gate |

PR template + ISSUE_TEMPLATE present.

## Documentation Quality

Excellent. `README.md` (badges, quickstart) + operator corpus in `docs/`:
`ARCHITECTURE.md` (component map + **decision IDs** like D-12/GIT-09
referenced from code comments), `API.md` (auth model, RPC/HTTP tables, PAT
scopes, rate limits — one `<!-- VERIFY -->` marker on a prod base URL),
`MCP.md`, `WEBMCP.md`, `GIT-PROTOCOL.md`, `CONFIGURATION.md` (~90
`OXIDEAN_*` env vars tabled), `database.md`, `DEVELOPMENT.md`,
`TESTING.md`, `VERSIONING.md`, `CLI.md`, `CLI-PARITY.md` (gh 2.96.0
parity), `DEPLOYMENT.md`, `SECURITY.md`, `CONTRIBUTING.md`, `AGENTS.md`,
`docs/guides/*`, generated `docs/openapi.yaml`, read-only
`docs/archive/planning/`. Rust module heads uniformly carry `//!` docs
naming the decision IDs they implement; `.cursor/rules/` +
`.agents/skills/` encode conventions for agents.

## Technical Debt

Recorded once here; cross-referenced from `architecture.md` and
`component-inventory.md`. Ordered by relevance to the VCS intent.

1. **`rpc_gen.rs` is a hand-maintained mirror** — the ~6,100-line
   `crates/oxidean-api/src/bin/rpc_gen.rs` embeds the TS client template
   with per-procedure typed methods; NOT derived from `PROCEDURES`
   automatically — every new RPC requires a manual template edit (guarded
   only by `make rpc-sync-check` + `procedures_list_matches_dispatch`).
   **A whole new `vcs.*` family means a large template addition** — or a
   refactor to metadata-driven codegen first.
2. **REST/OpenAPI is opt-in per procedure** — new RPC procedures don't
   appear in `/api/v1`/`openapi.yaml` without a hand-added `RouteDef`
   row in `rest/spec.rs`.
3. **Migration triple-write** — every schema change hand-ported to
   `migrations/{postgres,mysql,sqlite}/` (50 files ×3 today); only
   filename parity is test-enforced, SQL bodies can drift semantically.
4. **Monolith hotspots** — `oxidean-db/src/lib.rs` 4,941 LOC;
   `oxidean-cli/src/{args,commands}.rs` ~4,155/~4,053 LOC (hand-rolled
   parser, no clap); `oxidean-git/src/cli.rs` 5,943 LOC;
   `oxidean-api/src/{rpc.rs 1,946, mcp.rs 1,820}`;
   `packages/api-client/src/index.ts` ~6,000 LOC generated. A new
   protocol family lands inside several of these.
5. **Dual notification modules** — `notify/` (fan-out) vs `notification/`
   (RPC): intentional but confusingly named; don't repeat for VCS events.
6. **Per-process limiters/stores** — `login_begin_limiter`,
   `git_auth_limiter`, `lookup_limiter`, `two_factor_store`,
   `SshAuthLimiter` live in `AppState`; single-node only — a new server
   component inherits in-memory state (no horizontal scaling story).
7. **Env-config sprawl** — ~90 `OXIDEAN_*` vars parsed ad-hoc in
   `AppState::new`/`main`/`mcp`; a new subsystem should group its vars.
8. **Deferred `GixGitBackend`** — documented-but-unimplemented in
   `oxidean-git`; the trait seam exists precisely for engine swaps.
9. **Greedy top-level git route** — `/{owner}/{repo_git}` pattern
   constrains URL design for any side-by-side protocol (check
   `is_api_git_path` `proxy.rs:82` + Traefik `PathRegexp` first).
10. **Rust coverage ungated** — collected via cargo-llvm-cov but no CI
    floor (`docs/TESTING.md` residual).
11. **Hygiene is good**: only 5 TODO/FIXME/HACK across all crates, 0 in
    `apps/web/src`, 0 `todo!`/`unimplemented!`; one `<!-- VERIFY -->`
    marker in `docs/API.md`.

## Quality Summary

Unusually gated posture: type-aware lint, dead-code triple gate, weighted
web coverage floor (0.70), generated-contract drift gates on every
hand-mirrored surface, dialect-parity DB tests, commit-grammar +
per-component SemVer release machinery, digest-pinned CI images, and a
promotion-gated prod deploy. For the VCS intent the load-bearing debt is
the **5 parallel registration surfaces** per RPC family (PROCEDURES,
dispatch arm, proc_classes, rpc_gen template, optional RouteDef) and the
**3-place edge-parity sync** (API_PREFIXES, Traefik, Caddyfile) — both
gate-enforced but manually maintained.
