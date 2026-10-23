# Testing

## Test framework and setup

Oxidean uses two test stacks:

| Layer | Framework | Config |
|-------|-----------|--------|
| Rust (`crates/*`) | [cargo-nextest](https://nexte.st/) (falls back to `cargo test`) | `.config/nextest.toml` |
| JS / TS (`apps/web`, `packages/api-client`) | Vitest `^5` | `apps/web/vitest.config.ts`, `packages/api-client/vitest.config.ts` |

**Prerequisites**

- Rust toolchain + `cargo-nextest` recommended (`cargo install cargo-nextest --locked`). Without it, `make test` falls back to `cargo test --workspace`.
- Bun `1.4.4` (workspace package manager) and `bun install`.
- For `apps/web` browser projects: Playwright Chromium (`bunx playwright install --with-deps chromium`).
- For full stack e2e: Docker (Mailpit, OIDC mock, Resend/WorkOS HTTP stubs via `docker-compose.dev-auth.yml`).

Root `bun run test` runs Turbo (`turbo run test`), which executes each package’s `test` script (`@oxidean/web` and `@oxidean/api-client`).

### Rust (nextest)

`.config/nextest.toml` defines:

- **`default`** — local runs; slow-timeout 60s (terminate after 3 periods).
- **`ci`** — `fail-fast = false`, slow-timeout 90s, `test-threads = "num-cpus"`. CI invokes `cargo nextest run --workspace --profile ci`.

Integration-style Rust tests live under `crates/oxidean-api/tests/` and `crates/oxidean-db/tests/` (plus unit tests colocated in crate sources).

### Vitest projects (`apps/web`)

`apps/web/vitest.config.ts` defines projects. Default `bun run test` runs **unit** and **integration** only (`--project` flags). **browser** is a separate Chromium component tier (`bun run test:browser` / `make test-web-browser`). Projects **e2e-stack** and **e2e-stack-browser** are included only when `E2E_STACK=1` (used by `make test-e2e-stack`).

| Project | Environment | Include pattern | Notes |
|---------|-------------|-----------------|-------|
| `unit` | `node` | `src/**/*.unit.test.ts` | Fast pure logic |
| `integration` | `happy-dom` | `src/**/*.integration.test.{ts,tsx}` | Setup: `src/test/setup-integration.ts` (global `trackDomErrors` — every test fails on Octane `insertBefore` / hierarchy races **when happy-dom surfaces them**). **Not sufficient** for Base UI Indicator mount + Octane Chromium races. |
| `browser` | Playwright Chromium | `src/**/*.browser.test.{ts,tsx}` | Real DOM component tests (mocked RPC). Setup: `src/test/setup-browser.ts`. Run via `make test-web-browser` / CI `web-browser` job. Prefer this for PAT mint / checkbox / radio Subscribe races. |
| `e2e-stack` | `node` | `e2e/stack/**/*.stack.test.ts` | Only if `E2E_STACK=1`; 60s timeout; no file parallelism |
| `e2e-stack-browser` | Playwright Chromium | `e2e/stack-browser/**/*.stack.browser.test.{ts,tsx}` | Only if `E2E_STACK=1`; commands open pages via `newGuardedPage` + `assertNoOctaneOverlay` (fails on `pageerror` / DOM races / Octane overlay). Includes `expectPatMintClickThroughFlow`. |

### `@oxidean/api-client`

Package Vitest config: `packages/api-client/vitest.config.ts` (`environment: "node"`). Script: `vitest run`. Tests live next to sources (e.g. `src/index.test.ts`) and cover RPC protocol version headers and client fetch behavior with a mock `fetch`.

### Versioning tooling (`scripts/versioning`)

The SemVer machinery (registry loader, commit grammar, plan engine, changelog, release-tag helpers, hook, docs-shape guards) is Bun-tested: `bun test scripts/versioning` — run it with `make version-check` (registry completeness) and `make commit-check` (grammar lint) for the enforcement surfaces. See [docs/VERSIONING.md](VERSIONING.md).

## Running tests

### Default suite (`make test`)

```bash
make test
```

1. `cargo nextest run --workspace` if `cargo-nextest` is on `PATH`, else `cargo test --workspace`.
2. `bun run test` → Turbo → Vitest for `@oxidean/web` (unit / integration) and `@oxidean/api-client`.

Does **not** start Docker stubs or the live API/web stack.

### JS-only / filtered Vitest

```bash
bun run test                                    # turbo: all packages with a test script
bun run --filter @oxidean/web test             # web: unit + integration
bun run --filter @oxidean/web test:unit
bun run --filter @oxidean/web test:integration
bun run --filter @oxidean/web test:browser     # Chromium component DOM-race tier
bun run --filter @oxidean/api-client test
make test-web-browser                          # same as test:browser
```

From `apps/web`:

```bash
bun run test
bun run test:unit
bun run test:integration
bun run test:browser
```

### Full stack e2e (`make test-e2e-stack`)

```bash
make test-e2e-stack
# equivalent: ./scripts/dev-auth/run-stack-e2e.sh
```

`scripts/dev-auth/run-stack-e2e.sh`:

1. Brings up `docker-compose.dev-auth.yml` (`--profile dev-auth`): Mailpit, OIDC mock, Resend/WorkOS stubs.
2. Builds and runs `oxidean-api` on SQLite (default `:18080`) and `oxidean-web` on `:13000` (built `apps/web/dist` + `/api` proxy).
3. Sets `E2E_STACK=1` and runs `bun run --filter @oxidean/web test:e2e:stack` (`e2e-stack` + `e2e-stack-browser`).
4. Tears down stubs on exit (unless `OXIDEAN_E2E_KEEP_STUBS=1`).

Logs land under `var/e2e/` (API/web logs; CI uploads this on failure).

Manual stub stack for interactive use is documented in [dev-auth.md](./dev-auth.md) (`make up-dev-auth` / `make down-dev-auth`).

### Rust-only

```bash
cargo nextest run --workspace
cargo nextest run --workspace --profile ci   # same profile as CI api-rust
cargo test -p oxidean-db --test dialect_probe -- --nocapture
```

### Real-engine runs (Postgres / MySQL)

The `oxidean-db` dialect suites and the Actions API suites run against real
Postgres/MySQL when pointed at a reachable server (Compose services, CI
service containers, or throwaway local containers — see `docker/` Compose
overlays for the standard images `postgres:18-alpine` / `mysql:9.7`).

```bash
# DB layer: dialect_probe + dialect_actions honor DATABASE_URL.
DATABASE_URL=postgres://oxidean:oxidean@127.0.0.1:5432/oxidean \
  OXIDEAN_DB_DIALECT=postgres \
  cargo nextest run -p oxidean-db --test dialect_probe --test dialect_actions
DATABASE_URL=mysql://oxidean:oxidean@127.0.0.1:3306/oxidean \
  OXIDEAN_DB_DIALECT=mysql \
  cargo nextest run -p oxidean-db --test dialect_probe --test dialect_actions
# Same pair through make (DATABASE_URL in env):
make db-matrix
```

For the API layer, `crates/oxidean-api/tests/support/mod.rs` reads
`OXIDEAN_TEST_DATABASE_URL` — a **server-level admin URL** with `CREATE
DATABASE` privilege (e.g. `postgres://oxidean:oxidean@127.0.0.1:5432/postgres`,
`mysql://root:oxidean@127.0.0.1:3306/`). When set, every test that goes through
the support helper provisions a fresh `oxt_<uuid>` database and migrates it;
when unset the historical per-file SQLite behaviour is unchanged. Provisioned
databases are intentionally **not dropped** — run against throwaway servers.

```bash
# Every actions_* API binary + the workflow-conformance corpus on both engines:
OXIDEAN_TEST_PG_URL=postgres://oxidean:oxidean@127.0.0.1:5432/postgres \
OXIDEAN_TEST_MYSQL_URL=mysql://root:oxidean@127.0.0.1:3306/ \
  make actions-db-matrix

# Single engine / single binary manually:
OXIDEAN_TEST_DATABASE_URL=postgres://oxidean:oxidean@127.0.0.1:5432/postgres \
  cargo nextest run -p oxidean-api --test workflow_conformance
```

## Writing new tests

### Web (`apps/web`)

| Kind | Naming | Location |
|------|--------|----------|
| Unit | `*.unit.test.ts` | under `src/` |
| Integration | `*.integration.test.ts(x)` | under `src/` |
| Stack HTTP e2e | `*.stack.test.ts` | `e2e/stack/` |
| Stack browser e2e | `*.stack.browser.test.ts(x)` | `e2e/stack-browser/` |

Shared setup:

- Integration: `src/test/setup-integration.ts`
- Stack: `e2e/stack/setup.ts`, `e2e/stack-browser/setup.ts`

Import from `vitest` explicitly (`globals: false` in the web Vitest config).

**Forge stack-browser matrix (D-QH-03)** — extend `apps/web/e2e/stack-browser/` only (do not revive removed `e2e/component`). Suites under `make test-e2e-stack`:

| Suite file | Covers |
|------------|--------|
| `auth-ui.stack.browser.test.tsx` | Local signup UI, WorkOS CTA, OIDC SSO, `/status`, `auth.me` dedupe |
| `forge-repo.stack.browser.test.tsx` | Seeded repo code home + Packages tab / packages list |
| `forge-mirror.stack.browser.test.tsx` | Repo settings mirror auth HTTPS→SSH toggle (Chromium `insertBefore` gate) |
| `forge-issues-releases.stack.browser.test.tsx` | Issues create→close; release create from seeded tag |
| `forge-packages-ssh-orgs.stack.browser.test.tsx` | SSH keys add/list; org members settings |
| `forge-admin.stack.browser.test.tsx` | Forge admin `/admin/lfs`, `/admin/packages`, `/admin/auth` chrome (no factory-reset click) (G-11.1-15) |

login / verify / profile also have happy-dom `*.integration.test.ts` export/render contracts (RESEARCH P1).

**Render mounts required for Octane pages (G-11.1-15)** — Wave 0 **raw-source** stubs (`import "./page.tsrx?raw"` + regex for exports / RPC names / absence of `@else if`) are **not enough** to prove a `.tsrx` page works. They miss missing `useState`, broken Rivet control flow, and hydration-time ReferenceErrors. User-facing routes under `apps/web/src/routes/` must keep at least one **happy-dom render mount** (e.g. `AdminLfsPage` / `AdminPackagesPage` via `renderWithQueryClient`) and, for admin surfaces, **stack-browser** coverage (`forge-admin.stack.browser.test.tsx` → `/admin/lfs`, `/admin/packages`, `/admin/auth`). Do not regress those routes back to raw-source-only.

**Route coverage gate (G-11.1-15 / 11.1-09)** — CI fails if any user-facing `apps/web/src/routes/**/*.tsrx` page is missing from the manifest (or lacks valid evidence). Outlet-only layouts and `__root` are marked `layoutOnly` and excluded.

| Artifact | Role |
|----------|------|
| `apps/web/src/test/route-coverage.manifest.ts` | Declares each route + `happy-dom` / `stack-browser` / `skip` evidence |
| `scripts/route-coverage-check.sh` | Discovers `.tsrx` files and fails on gaps / missing test paths |
| `make route-coverage-check` | Local + CI entrypoint |

**Adding a new page route**

1. Add the `.tsrx` under `apps/web/src/routes/`.
2. Append a row to `routeCoverageManifest` with at least one of:
   - `{ kind: "happy-dom", test: "apps/web/src/routes/….integration.test.ts" }` (file must exist and should **mount** the page — not raw-source-only)
   - `{ kind: "stack-browser", test: "apps/web/e2e/stack-browser/….stack.browser.test.tsx" }`
   - `{ kind: "skip", rationale: "…" }` (temporary; prefer real coverage)
3. Run `make route-coverage-check` before pushing.

**Browser (Chromium component) coverage gate** — complementary to route-coverage. Discovers every `apps/web/src/**/*.tsrx` that uses `Checkbox`, `RadioGroup` / `RadioGroupItem`, `form.Subscribe`, `SelectRoot` / `SelectPortal` / `SelectTrigger`, `Switch`, or `DialogPortal` / `AlertDialogPortal` / `DropdownMenu` (excluding `*.harness.tsrx` / `*.browser-harness.tsrx`). Each surface must prove real-DOM coverage; happy-dom alone does **not** count for this gate.

| Artifact | Role |
|----------|------|
| `apps/web/src/test/browser-coverage.manifest.ts` | Declares each high-risk surface + `browser` / `stack-browser` / `skip` evidence |
| `scripts/browser-coverage-check.ts` | Discovers high-risk `.tsrx`, validates evidence, proves browser tests mount + click + assert |
| `make browser-coverage-check` | Inventory entrypoint |
| `make browser-coverage-check-pr` | Change-aware vs `origin/main` (PR-equivalent; run before push) |

**Browser evidence must prove coverage** — for `kind: "browser"`, the `*.browser.test.tsx` file must:

1. Call `mountComponent` or `mountWithQueryClient` (`apps/web/src/test/browser-mount.ts`)
2. Contain the `subject` marker and reference the surface module basename (e.g. `pat-expiry-field`)
3. Interact via `clickTestId` / `clickAriaLabel` / `pickSelectOptionByTestId` / `.click(`
4. Assert with `expectNoOctaneOverlayInDocument`, `expectNoDomRaces`, or `trackDomErrors`

**Base UI Select** — do not use bare option `.click()` or trigger `.click()` in stack-browser. Use `pickSelectOptionByTestId` (browser-mount: `pointerdown` then click; stack-browser: real mouse press via `hover` + `mouse.down/up` to open, then in-page `pointerdown`+click to commit — Playwright `locator.click()` leaves `aria-expanded=false` on the SSR page). See `ui-controls.browser.test.tsx` and `expectPatMintClickThroughFlow`.

**Adding or changing high-risk interactive UI**

1. Author the `.tsrx` (Checkbox / Radio / Select / Switch / portals / `form.Subscribe`).
2. Prefer a colocated `*.browser.test.tsx` that mounts, toggles the control, and asserts no overlay / DOM race. Attribute an existing stack-browser click-through only when that suite already exercises the control (`subject` required).
3. Append a row to `browserCoverageManifest`.
4. Run `make browser-coverage-check-pr` and `make test-web-browser` before pushing.
5. Temporary `{ kind: "skip", rationale: "…" }` is allowed **only for untouched bootstrap gaps**. CI change-aware mode **fails** if a newly added or modified high-risk surface is still skip-only.

**Change-aware UI coverage (CI)** — `route-coverage` and `browser-coverage` set `UI_COVERAGE_BASE` to the PR base, or on push to `github.event.before` (full push range). Local:

```bash
make browser-coverage-check-pr
make route-coverage-check-pr
make ui-coverage-change-contract
```

### API client

Add `*.test.ts` beside the module under `packages/api-client/src/` (Vitest picks them up with the package default config).

### Rust

- Unit: `#[test]` / `#[tokio::test]` in crate sources.
- Integration: `crates/<crate>/tests/*.rs` (e.g. auth, RPC HTTP/WS, dialect probe). Prefer `cargo nextest` so slow/hanging tests follow `.config/nextest.toml` timeouts.

## Coverage requirements

Weighted forge-core gate (**D-QH-02**). Layers and weights:

| Layer | Weight | What is measured |
|-------|--------|------------------|
| Unit | **25%** (`0.25`) | Web Vitest `--project unit` line coverage (`@vitest/coverage-v8`). Rust lib units via `cargo-llvm-cov` when available (optional today; see residual below). |
| Integration | **40%** (`0.40`) | Web Vitest `--project integration` (happy-dom) line coverage. Rust `tests/` included when llvm-cov runs. |

| E2E / hydration | **35%** (`0.35`) | Interim **checklist score** (fraction of required stack-browser + stack HTTP + smoke script paths present). Not Playwright % coverage yet — forge stack-browser matrix landed in Phase 11.1-04 (see Writing new tests). |

**Weighted score**

```
score = 0.25 * unit + 0.40 * integration + 0.35 * e2e
```

**Floor:** **`0.70`** (`COVERAGE_WEIGHTED_FLOOR` default in `scripts/coverage-weighted.sh`, set in the `coverage-weighted` CI job). Bootstrap floor was `0.65` at a measured ~0.68 baseline; the ratchet to `0.70` landed after suites deepened (main measured ~0.73: unit ≈68% / integration ≈52% / e2e checklist 1.0). Do not lower without an explicit residual note.

**E2E checklist formula (interim)**

`e2e = present / total` where `total` is the item count in `scripts/coverage-e2e-checklist.sh` (auth + forge-admin stack-browser, SMTP/OIDC stack tests, git/packages smoke scripts). Missing paths lower the score. Forge stack-browser suites (repo code, issues, releases, packages list, SSH keys, org members, admin LFS/packages/auth) live under `apps/web/e2e/stack-browser/` (D-QH-03 / 11.1-04 / 11.1-08); expand the checklist when promoting additional forge paths into the weighted e2e score.

**Commands**

```bash
make coverage-web          # unit + integration Vitest coverage → var/coverage/*-summary.json
make coverage-rust         # cargo-llvm-cov when installed; otherwise skips with residual note
make coverage-weighted     # collect web (+ optional rust) then run weighted gate
make coverage-contract     # aggregator self-test (under/over floor)
./scripts/coverage-weighted.sh --unit 0.9 --integration 0.9 --e2e 0.9
./scripts/coverage-e2e-checklist.sh
```

From `apps/web`:

```bash
bun run test:coverage:unit
bun run test:coverage:integration
```

Reports: `apps/web/coverage/{unit,integration}/` (`coverage-summary.json`, `lcov.info`). Aggregator copies summaries under `var/coverage/` (gitignored).

**Residual (this wave)**

- Rust `cargo-llvm-cov` runs in the `coverage-weighted` CI job as **collection only** (`var/coverage/rust-*` artifacts): the weighted gate composition is unchanged — web unit/integration + e2e checklist drive the score and **CI does not fail on Rust coverage numbers** yet. Revisit when the Rust layer joins the weighted composition.
- Do not revive the removed Playwright component e2e project for coverage (**D-QH-03**).

## CI integration

Workflow: [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) (`name: CI`). Triggers: push to `main`, all pull requests. Concurrency cancels in-progress runs on the same ref.

| Job | What it runs |
|-----|----------------|
| `commit-lint` | `versioning commit-check` on the PR commit range + PR title; `versioning check` registry completeness |
| `rust-build` | `cargo build --workspace --bins` + `cargo nextest archive --workspace --features server` → uploads `rust-bins` + `nextest-archive` artifacts (same-run only; `rust-artifacts-cleanup` deletes them at run end) |
| `api-rust` | 5-way matrix: download `nextest-archive` → `cargo nextest run --archive-file … --profile ci --partition count:N/5` (build happens once in `rust-build`) |
| `web-octane` | `bun install --frozen-lockfile` → `bun run test` (api-client + web unit/integration) → Turbo build `@oxidean/web` |
| `route-coverage` | `make route-coverage-check` — every user-facing `.tsrx` page has happy-dom, stack-browser, or documented skip (G-11.1-15); change-aware: touched routes cannot stay skip-only |
| `web-browser` | `make test-web-browser` — Vitest Chromium component DOM-race suite (`*.browser.test.tsx`) |
| `browser-coverage` | `make browser-coverage-check` + change contract — high-risk UI (Checkbox/Radio/Select/Switch/portals/Subscribe) Chromium proof; touched/new surfaces cannot stay skip-only |
| `coverage-weighted` | Rust toolchain (llvm-tools-preview) + `cargo-llvm-cov`/`nextest` → Bun install → `make coverage-contract` → `make coverage-rust` → `make coverage-web` → e2e checklist → `scripts/coverage-weighted.sh` (floor `0.70`); uploads `var/coverage/rust-*` always, `var/coverage/` + `apps/web/coverage/` on failure |
| `e2e-stack` | Bun + Playwright + prebuilt `rust-bins` (`OXIDEAN_PREBUILT_BINS=1` — `run-stack-e2e.sh` execs `target/debug/*` instead of `cargo run`) → `make test-e2e-stack`; on failure uploads `var/e2e/` as `e2e-stack-logs` |
| `actions-e2e` | Prebuilt `rust-bins` → `./scripts/e2e-actions-pipeline.sh` (API sqlite + host runner + seeded workflow run) |
| `rpc-sync` | Prebuilt `rust-bins` (`rpc-gen`/`openapi-gen`/`ox` exec direct) → `make rpc-sync-check` / `openapi-sync-check` / `skill-sync-check` |
| `compose` | `docker compose … config` for base, MySQL/SQLite overlays, and `docker-compose.dev-auth.yml` (config-only; does not build/bring-up) |
| `compose-smoke` | Matrix `postgres` / `sqlite` / `mysql`: `./scripts/ci-compose-smoke.sh` → `make smoke` / `smoke-sqlite` / `smoke-mysql` (**D-CI-01…04**); fail-closed under `CI` / `SMOKE_REQUIRE_STACK`; image proof via `compose up --build` (**D-CI-06**); uploads `/tmp/oxidean-smoke*.json` on failure. Complements config-only `compose` and stays separate from `smoke-protocol` (**D-CI-05**) |
| `smoke-protocol` | Compose up → seeds verified owner + public repo + PAT + session cookie via RPC (**DEBT-11**), then `make smoke-git-https` + `smoke-git-ssh` + `smoke-git-lfs` + `smoke-packages` via `make smoke-protocol-ci` (**D-QH-04**), then `smoke-protection` — ORG-06/D-PKG-03 HTTPS protected-push denial on a fresh stack it re-ups itself (**DEBT-01**); fail-closed when Docker/stack absent (`CI` / `SMOKE_REQUIRE_STACK`); live `git ls-remote` + push over HTTPS **and** SSH by default (`SMOKE_SEED_FIXTURES=0` → routing/TCP-only, `SMOKE_SKIP_LS_REMOTE` defaults back to 1); `SMOKE_SKIP_LFS_CLIENT=1` (git-lfs binary transfer still opt-in) |
| `db-dialects` | Single leg — Postgres + MySQL service containers + sqlite file: `cargo nextest run --archive-file … -E 'binary_id(oxidean-db::dialect_probe) or binary_id(oxidean-db::dialect_vcs)'` per dialect with matching `DATABASE_URL` / `OXIDEAN_DB_DIALECT` (dialect probe + VCS facade/collation proofs — not a substitute for Compose bring-up) |
| `vcs-server` | Dedicated `oxidean-vcs` server-feature leg: download `nextest-archive` → `cargo nextest run --archive-file … --profile ci -E 'package(oxidean-vcs)'` — the suite is `#[cfg(all(test, feature = "server"))]`, so the archive's `--features server` flag is load-bearing (an empty match fails loud via nextest `--no-tests`) |

Default `web-octane` stays fast (no Docker auth stubs). True auth/email path coverage is the separate `e2e-stack` job. The `coverage-weighted` job enforces D-QH-02 without reviving component Playwright. Compose dialect health (Traefik `/` + `/health` + `system.db_probe`) is the `compose-smoke` matrix — not folded into `smoke-protocol`. Forge protocol edges (Smart HTTP / SSH TCP / LFS batch / packages PathPrefix) plus ORG-06 protected-push denial are the `smoke-protocol` job — not happy-dom only.

### Required status checks (merge gate)

The **`Main` branch ruleset** ([repo settings → rules](https://github.com/oxidean/oxidean/settings/rules/22764294)) requires named checks on a PR's head commit before it can be squash-merged. The required list is **every job in `ci.yml` plus `meta` from `images.yml`**, pinned to GitHub Actions (`integration_id: 15368`). Matrix legs are required by their expanded check names — `api-rust (1)`…`api-rust (5)`, `compose-smoke (postgres)` / `(sqlite)` / `(mysql)`. `strict_required_status_checks_policy` is off, so a green PR never needs the latest main merged in.

Deliberately **not** required — a required check that never runs sits at "Pending" and blocks the PR forever:

- `ox-cli.yml` and `ci-base.yml` jobs — both are `paths:`-filtered on `pull_request`, so a PR that does not touch those paths produces no checks at all.
- `images.yml`'s `build`/`publish`/`deploy-staging`/`bind-pr-env` legs — the service matrix expands to `[]` on fork PRs, so no check exists. `ci.yml`'s `docker-ci-images` already gates image compilation on every PR.
- Skip-only or hygiene jobs (`rust-artifacts-cleanup`, `repin staging cli feed`, `cleanup pr-*`) — no signal; a skipped conclusion already counts as passing.

**Adding, renaming, or removing a job — or changing a matrix value set (`shard`, `dialect`, `svc`) — in a required workflow must update the ruleset's context list in the same change.** A renamed job leaves a stale context that blocks every PR; a new job left out is silently ungated. Cross-check the live list against `grep -n '^\s*name:' .github/workflows/ci.yml`.

```bash
# list required check names today
gh api repos/oxidean/oxidean/rulesets/22764294 \
  --jq '.rules[] | select(.type=="required_status_checks").parameters.required_status_checks[].context'

# update: GET the ruleset, edit .rules[type==required_status_checks].parameters
# .required_status_checks (entries are {context, integration_id: 15368}),
# then PUT the object back:
gh api -X PUT repos/oxidean/oxidean/rulesets/22764294 --input ruleset.json
```

### CI base images (run-isolated, no cross-run Rust cache)

Most jobs run inside self-owned images published to `ghcr.io/<owner>/oxidean-ci` by [`.github/workflows/ci-base.yml`](../.github/workflows/ci-base.yml) from [`docker/ci-base/Dockerfile`](../docker/ci-base/Dockerfile). The `ci-image` job resolves each flavor's content tag to an `@sha256` digest at run start, so every leg in a run executes on byte-identical bits — an immutable input, not a cache. There is deliberately **no `Swatinem/rust-cache`**: nothing a run writes ever feeds another run's inputs; the only retained cross-run cache is the `type=gha` BuildKit layer cache used to *build* images (layers are content-keyed).

| Flavor | Contents | Consumers |
|--------|----------|-----------|
| `dev` | stable Rust + mold/protoc + `cargo-nextest`/`cargo-machete`/`cargo-chef` + bun; third-party deps pre-cooked to `/ci/target-dev` | `rust-build`, `dead-code` |
| `release` | same toolchain; release-profile deps at `/ci/target-release` | `build-metrics` |
| `cov` | + `llvm-tools-preview`, `cargo-llvm-cov`; instrumented deps at `/ci/target-cov` (`CARGO_LLVM_COV_TARGET_DIR`) | `coverage-weighted` |
| `tools` | bun + Playwright chromium + docker CLI/compose/buildx (no rustc) | web lanes, `e2e-stack`, `actions-e2e`, `rpc-sync`, `commit-lint` |

Exec-only legs (`api-rust` shards, `db-dialects`) stay on the host runner — pulling an image outweighs their ~10s `cargo-nextest` install. Container legs that drive Compose mount the host daemon socket (`e2e-stack` also runs `--network host` so published ports resolve on localhost). The dep-bake uses `cargo chef cook`, which produces *dependency-only* artifacts — workspace crates always compile from the fresh checkout, which is the isolation guarantee.

Tags: `<flavor>-<content-hash>` (content-keyed — hash covers `Cargo.toml`/`Cargo.lock`, `crates/*/Cargo.toml`, `docker/ci-base/**`, and `scripts/ci-image-tag.sh`; same-repo PRs touching those inputs may publish the hash tag, which is safe because it can only be selected by runs whose inputs hash identically) plus moving `<flavor>-latest` (main-owned only). A lockfile-ahead PR before the publisher lands falls back to `-latest` (per-crate fingerprints still hit every unchanged dep). When no tag resolves, `ci-image` polls the registry for up to 40 min (`CI_IMAGE_WAIT_SECONDS`) so a same-repo PR that changes image inputs waits for its own `ci-base` publish leg; fork PRs can't publish and skip the wait (fail fast / `-latest` fallback). rustc is pinned exactly (`RUST_IMAGE` ARG in the Dockerfile — a bump is a one-line, hash-covered PR); weekly cron + `workflow_dispatch` rebuild all flavors so Debian/apt patch drift lands deliberately. Refresh procedure: bump inputs on a branch → merge → the push-to-main leg builds and pushes both tags; or run `ci-base` via `workflow_dispatch` for a manual refresh. Bootstrap on a fresh fork: same `workflow_dispatch` (requires `packages: write`).

### Compose dialect smokes (local + CI)

| Target | Proves | Notes |
|--------|--------|-------|
| `make smoke` | Postgres Compose bring-up + `db_probe` | Same as CI `compose-smoke` / postgres |
| `make smoke-sqlite` | SQLite overlay bring-up + dialect assert | Host dir via `scripts/sqlite-host-dir.sh` (`./var` on Linux) |
| `make smoke-mysql` | MySQL overlay bring-up + dialect assert | Profile `mysql` |
| `make smoke-compose-ci` | Fail-closed CI entry (`DIALECT=…`) | Same as `./scripts/ci-compose-smoke.sh` |

Oxidean Cloud (Railway IaC + Caddy gateway) is **not** exercised in PR CI — see [DEPLOYMENT.md](DEPLOYMENT.md) and [`docs/archive/planning/milestones/v1.0-phases/22-compose-ci-deploy/22-VALIDATION.md`](archive/planning/milestones/v1.0-phases/22-compose-ci-deploy/22-VALIDATION.md). Local preview: `make cloud-plan` (requires linked Railway CLI).

### Protocol smokes (local + CI)

| Target | Proves | Notes |
|--------|--------|-------|
| `make smoke-git-https` | Traefik `/{owner}/{repo}.git` is not SPA HTML; `git ls-remote` + PAT push when seeded | Needs stack up; set `SMOKE_SKIP_LS_REMOTE=1` for routing-only |
| `make smoke-git-ssh` | TCP `OXIDEAN_SSH_PORT` (2222); scp-style `ls-remote` + `refs/heads/smoke-ssh` push when seeded (ephemeral key via `sshKey.add` + `SMOKE_SESSION_COOKIE`) | Needs SSH-enabled Compose API |
| `make smoke-git-lfs` | `.git/info/lfs` batch routing not SPA; optional git-lfs client | `SMOKE_SKIP_LFS_CLIENT=1` for routing-only |
| `make smoke-packages` | `/v2` `/npm` `/generic` PathPrefix → API | Needs running Compose API |
| `make smoke-protection` | API image ships `oxidean-protection-hook`; HTTPS push to reviews-required protected branch denied (**ORG-06** / **D-PKG-03**) | Fresh Compose up (wipes volumes); `scripts/compose-smoke-protection.sh` |
| `make smoke-protocol-ci` | All five fail-closed; seeds fixtures then runs live HTTPS/SSH client checks; `smoke-protection` re-ups a fresh stack (wipes volumes) to seed a repo and assert protected-push denial | Same entrypoint as CI `smoke-protocol`; `SMOKE_SEED_FIXTURES=0` for routing/TCP-only |

Locally without Docker, individual `make smoke-git-*` / `smoke-packages` / `smoke-protection` may skip (exit 0). Under `CI=true` or `SMOKE_REQUIRE_STACK=1`, those skips become failures.

## Dev-auth stubs (stack e2e)

See [dev-auth.md](./dev-auth.md) for interactive setup. Stack e2e depends on:

| Service | Default local endpoint | Role in tests |
|---------|------------------------|---------------|
| Mailpit | UI `http://127.0.0.1:8025`, SMTP `smtp://127.0.0.1:1025` | Capture SMTP mail |
| OIDC mock | Issuer `http://127.0.0.1:9090/default` | OIDC login without a real IdP |
| HTTP stubs | `http://127.0.0.1:9092` | Resend `POST /emails` + WorkOS AuthKit |

`e2e-stack` proves SMTP→Mailpit, Resend→stub, WorkOS stub login, and OIDC mock login over HTTP. `e2e-stack-browser` exercises signup UI, WorkOS CTA, and the D-QH-03 forge matrix (repo/packages, issues/releases, release asset upload+download, repo Danger zone rename/transfer, SSH keys, org members) against the live web/API in Chromium.

The e2e stack script also builds and attaches a native `oxidean-runner` (host execution, labels `ubuntu-latest,self-hosted`) to the API via the `OXIDEAN_RUNNER_REGISTRATION_TOKEN` bootstrap. `forge-actions-pipeline.stack.browser.test.tsx` pushes a real `.github/workflows/ci.yml` over Smart HTTP, waits for the runner to drive the run green, and asserts the run detail page streams the job log marker in Chromium.


## Actions phase gate (Phase 19)

```bash
# Docs/image/Compose presence + optional Docker build (skip-ok without Docker/stack)
make smoke-actions
# or: bash scripts/smoke-actions.sh

# Full API + native runner pipeline (no Docker required): builds
# oxidean-api + oxidean-runner, registers, pushes a workflow, and asserts a
# green run with the streamed log marker.
make test-e2e-actions
# or: bash scripts/e2e-actions-pipeline.sh

# Public-ecosystem smoke (real github.com): boots API + host runner, pushes a
# workflow that runs the real actions/setup-node (Node 20 download),
# actions/cache (cache-service-absent warning path), an artifact round-trip,
# and actions/github-script minting an OIDC JWT (RS256) via @actions/core.
# Skips exit-0 when github.com/nodejs.org are unreachable;
# E2E_PUBLIC_ACTIONS_REQUIRED=1 makes unreachable a failure.
./scripts/e2e-actions-public.sh

# Stock Gitea act_runner compatibility: builds act_runner (pin: v0.2.11) and
# runs the runner.v1 protocol end to end. ACT_RUNNER_HOST_MODE=1 runs without
# Docker; ACT_RUNNER_BIN points at a prebuilt binary.
ACT_RUNNER_HOST_MODE=1 ./scripts/e2e-act-runner.sh

# Seed only (works against any origin incl. preview/staging):
#   OXIDEAN_ORIGIN=https://<gateway> OXIDEAN_SEED_USER=… OXIDEAN_SEED_PASSWORD=… \
#     make seed-actions-demo

# Targeted API coverage
cargo nextest run -p oxidean-api -E 'test(actions_)|test(runner_)|test(commit_status)|test(actions_secrets)'
# Runner unit tests
cargo nextest run -p oxidean-runner

make rpc-sync-check
make web-lint && make web-format-check   # after apps/web Actions UI changes
```

`smoke-actions` fails closed under `CI=true` / `SMOKE_REQUIRE_STACK=1` when Docker/stack is required; otherwise prints a skip signal and exits 0 after static Dockerfile/Compose checks.
