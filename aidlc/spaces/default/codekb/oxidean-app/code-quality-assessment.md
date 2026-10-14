# Code Quality Assessment — Oxidean

## Test Coverage

### Test Inventory

| Area | Location | Size |
|---|---|---|
| API integration tests | `crates/oxidean-api/tests/` | 105 files |
| DB dialect-parity tests | `crates/oxidean-db/tests/` | 27 files (same suite across pg/mysql/sqlite) |
| CLI integration tests | `crates/oxidean-cli/tests/` | 4 files: `commands_e2e`, `manifest_gate`, `rpc_http` (shared with api surface), `self_update_http` |
| Git tests | `crates/oxidean-git/tests/disk_usage.rs` | + colocated `#[cfg(test)]` units in every crate |
| Web tests | `apps/web/src/**/*.{unit,integration,browser}.test.*` + `e2e/{stack,stack-browser}/` | 155 test files |
| API client tests | `packages/api-client/src/index.test.ts` | generated-client smoke |

### Frameworks & Harnesses

- **cargo-nextest** (`.config/nextest.toml`: `default` + `ci` profiles,
  `fail-fast=false` in CI); `cargo test` fallback.
- **Vitest** 5 projects: `unit` (node), `integration` (happy-dom),
  `browser` (Playwright Chromium), `e2e-stack`, `e2e-stack-browser`
  (`apps/web/vitest.config.ts`).
- **wiremock** for Rust HTTP fixtures; docker-compose dev-auth stack
  (Mailpit/OIDC/Resend/WorkOS stubs) for e2e (`make test-e2e-stack`).

### Coverage Gates

- `scripts/coverage-weighted.sh` + `coverage-e2e-checklist.sh` +
  `coverage-weighted.contract.sh` — weighted gate: unit 25% / integration
  40% / e2e checklist 35%, floor `COVERAGE_WEIGHTED_FLOOR=0.70`.
- `make coverage-rust` (cargo-llvm-cov → `var/coverage/rust-*`),
  `make coverage-web` (vitest v8).
- Route-coverage manifest + change-aware browser gates:
  `scripts/route-coverage-check.ts`, `browser-coverage-check.ts`,
  `ui-coverage-change.contract.sh` (`make browser-coverage-check-pr`).

## Linting & Static Checks

| Check | Tool | Gate |
|---|---|---|
| Web lint | oxlint `--type-aware` (oxlint-tsgolint + @tsrx/oxc) + `check-octane-dom-races.ts` | `make web-lint`; plain `tsc` is NOT the web type gate |
| Web format | oxfmt | `make web-format-check` |
| Rust lint | clippy `-D warnings`, rustdoc `-D warnings` | CI `dead-code` job |
| Dead code/deps | cargo-machete + `-D dead_code` + knip (`.tsrx`/`.astro` compilers) | `make dead-code-check` |
| Makefile | checkmake (`checkmake.ini`) | `makefile-lint` — **first CI job, everything `needs` it** |
| Contract drift | `rpc-sync`, `openapi-sync`, `skill-sync`, `route-sync`, `cli-parity`, stack-presets sync, internal-anchors | dedicated CI jobs |

## CI/CD

| Workflow | Trigger | Jobs / purpose |
|---|---|---|
| `ci.yml` | PRs + main | 12+ jobs: makefile-lint → api-rust (3-shard nextest), web-octane, dead-code, web-browser, route-coverage, browser-coverage, coverage-weighted, e2e-stack, actions-e2e, rpc-sync, compose, docker-ci-images, compose-smoke ×3 dialects, smoke-protocol, db-matrix ×3 dialects, build-metrics |
| `ox-cli.yml` | push main, PRs, `v*.*.*` tags, dispatch — path-filtered to `crates/oxidean-cli/**`, `cli_dist.rs`, `assets/cli/**`, api Dockerfile, `packaging/**`, `scripts/ox-*`, `Cargo.{toml,lock}` | build (10-target matrix + FreeBSD VM), install-{linux(7),macos,windows}, packages (nfpm × amd64/arm64), publish-dist (rolling `ox-dist` + staging redeploy), release (tag-only `gh release create --generate-notes`), image-matrix proofs |
| `publish-runner.yml` | main path changes + `v*.*.*` tags | `ghcr.io/<owner>/oxidean-runner` — branch/tag/sha/latest image tags |
| `production-deploy.yml` | manual `workflow_dispatch` | promote-by-SHA / rollback to Railway prod behind `Oxidean / production` env gate; requires latest CI green on the SHA; **no version input** |

Package version derivation (`ox-cli.yml:368-375`): tag →
`${GITHUB_REF_NAME#v}`; non-tag → `0.0.0.dev.${GITHUB_RUN_ID}` (no `+`/`-`
for apk/pacman); `packaging/nfpm.yaml` `@PKG_VERSION@`,
`version_schema: none`.

## Documentation Quality

Strong: `README.md`, `CONTRIBUTING.md`, `AGENTS.md`, `ROADMAP.md`,
`SECURITY.md`, `SUPPORT.md`, and `docs/` covering ARCHITECTURE, API, CLI,
CLI-PARITY, CODE_PRACTICES, CONFIGURATION, database, DEPLOYMENT, dev-auth,
DEVELOPMENT, GETTING-STARTED, GIT-PROTOCOL, MCP, TESTING, WEBMCP, `guides/`,
`openapi.yaml`. Rust sources carry module-level `//!` docs throughout.
Generated docs are drift-gated in CI (`rpc-sync-check`,
`openapi-sync-check`, `skill-sync-check`, `cli-parity-check`, route-sync,
stack-presets sync, internal-anchors).

## Technical Debt

Recorded once here; cross-referenced from `architecture.md` and
`component-inventory.md`.

1. **`env!("CARGO_PKG_VERSION")` conflation** — `/cli/latest`
   (`cli_dist.rs:131`) reports `oxidean-api`'s version while serving
   `oxidean-cli` binaries; `system.manifest`/`system.health`/MCP
   `serverInfo` are all the api crate's version; `ox --version`, UA
   headers, `clientInfo`, self-update `current` are all the cli crate's.
   Correct today only because all crates share `0.1.0` — per-component
   SemVer must assign each surface a semantics (server vs CLI vs
   dist-channel).
2. **`ox-cli.yml` path filter misses `crates/oxidean-core/**`** (lines
   26-35, 38-47) though `oxidean-cli` depends on it — a core change won't
   trigger the CLI build/release pipeline. The `.railway/railway.ts`
   `watchPatterns` list shows the correct transitive-dep pattern.
3. **`OXIDEAN_RPC_VERSION` env var is dead config** — documented
   (`.env.example:19`, `docker-compose.yml:63`, `docs/CONFIGURATION.md`)
   but never read by Rust code; the wire version is compile-time
   `RPC_PROTOCOL_VERSION`.
4. **`MIN_CLI_VERSION` is hand-bumped** (`rpc.rs:121`) with hardcoded
   `"0.1.0"` asserts in three test locations — needs a bump policy under
   SemVer.
5. **Dev-version scheme `0.0.0.dev.{run_id}`** (`ox-cli.yml:375`) is
   packaging-only; `version_schema: none` in `nfpm.yaml` means nfpm does
   not validate semver shape — may conflict with strict SemVer packaging.
6. **No changelog infrastructure** — `gh release create --generate-notes`
   only; no CHANGELOG.md, no git-cliff/release-plz config, no tag
   convention beyond the `v*.*.*` glob.
7. **No commit-msg enforcement** — no git hooks at all
   (`core.hooksPath` unset, no husky/lefthook/pre-commit); conventional
   prefixes are informal (~94/100 recent subjects, drifting scopes like
   `fix(11.1)`). Hook distribution is greenfield.
8. **No semver git tags** — `git tag -l` shows only rolling `ox-dist`;
   the tag-only release job has never run from this repo state.
9. **Duplicated MCP protocol-version lists** —
   `apps/web/src/lib/webmcp.ts:2` vs `oxidean-web/src/wellknown.rs:15`,
   comment-flagged manual sync with no CI gate.
10. **Version skew surface** — `/cli/latest` reports crate version while
    `dist_commit`/`SOURCE_COMMIT` carries real binary provenance;
    `ox self-update` already compensates by comparing sha256
    (`self_update.rs:50-57`).
11. **Stack-preset scaffold versions** — embedded preset manifests carry
    `0.0.0`/`0.0.1`/`1.0.0` template versions (workspace-`exclude`d); a
    naive repo-wide version rewrite would collide with them.
12. TODO/FIXME density very low (5 hits in crates; 0 in apps/web +
    packages).

## Quality Summary

The repo has an unusually gated quality posture: type-aware lint, dead-code
triple gate, weighted coverage floor (0.70), generated-contract drift
gates, dialect-parity DB tests, and a promotion-gated prod deploy. The
gaps cluster exactly where the active intent lands — versioning semantics,
changelog generation, tag/commit conventions, and dependency-aware release
triggering — rather than in general hygiene.
