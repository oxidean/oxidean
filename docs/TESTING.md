<!-- generated-by: gsd-doc-writer -->
# Testing

## Test framework and setup

Oxidean uses two test stacks:

| Layer | Framework | Config |
|-------|-----------|--------|
| Rust (`crates/*`) | [cargo-nextest](https://nexte.st/) (falls back to `cargo test`) | `.config/nextest.toml` |
| JS / TS (`apps/web`, `packages/api-client`) | Vitest `^5` | `apps/web/vitest.config.ts`, `packages/api-client/vitest.config.ts` |

**Prerequisites**

- Rust toolchain + `cargo-nextest` recommended (`cargo install cargo-nextest --locked`). Without it, `make test` falls back to `cargo test --workspace`.
- Bun `1.4.0` (workspace package manager) and `bun install`.
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

## Running tests

### Default suite (`make test`)

```bash
make test
```

1. `cargo nextest run --workspace` if `cargo-nextest` is on `PATH`, else `cargo test --workspace`.
2. `bun run test` → Turbo → Vitest for `@oxidean/web` (unit / integration) and `@oxidean/api-client`.

Does **not** start Docker stubs or the live API/Vite stack.

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
2. Builds and runs `oxidean-api` on SQLite (default `:18080`), Vite on `:13000`.
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
| `api-rust` | Install nextest → `cargo nextest run --workspace --profile ci` |
| `web-octane` | `bun install --frozen-lockfile` → `bun run test` (api-client + web unit/integration) → Turbo build `@oxidean/web` |
| `route-coverage` | `make route-coverage-check` — every user-facing `.tsrx` page has happy-dom, stack-browser, or documented skip (G-11.1-15); change-aware: touched routes cannot stay skip-only |
| `web-browser` | `make test-web-browser` — Vitest Chromium component DOM-race suite (`*.browser.test.tsx`) |
| `browser-coverage` | `make browser-coverage-check` + change contract — high-risk UI (Checkbox/Radio/Select/Switch/portals/Subscribe) Chromium proof; touched/new surfaces cannot stay skip-only |
| `coverage-weighted` | Rust toolchain (llvm-tools-preview) + `cargo-llvm-cov`/`nextest` → Bun install → `make coverage-contract` → `make coverage-rust` → `make coverage-web` → e2e checklist → `scripts/coverage-weighted.sh` (floor `0.70`); uploads `var/coverage/rust-*` always, `var/coverage/` + `apps/web/coverage/` on failure |
| `e2e-stack` | Rust + Bun + Playwright → `make test-e2e-stack`; on failure uploads `var/e2e/` as `e2e-stack-logs` |
| `rpc-sync` | `make rpc-sync-check` |
| `compose` | `docker compose … config` for base, MySQL/SQLite overlays, and `docker-compose.dev-auth.yml` (config-only; does not build/bring-up) |
| `compose-smoke` | Matrix `postgres` / `sqlite` / `mysql`: `./scripts/ci-compose-smoke.sh` → `make smoke` / `smoke-sqlite` / `smoke-mysql` (**D-CI-01…04**); fail-closed under `CI` / `SMOKE_REQUIRE_STACK`; image proof via `compose up --build` (**D-CI-06**); uploads `/tmp/oxidean-smoke*.json` on failure. Complements config-only `compose` and stays separate from `smoke-protocol` (**D-CI-05**) |
| `smoke-protocol` | Compose up → `make smoke-git-https` + `smoke-git-ssh` + `smoke-git-lfs` + `smoke-packages` via `make smoke-protocol-ci` (**D-QH-04**); fail-closed when Docker/stack absent (`CI` / `SMOKE_REQUIRE_STACK`); default `SMOKE_SKIP_LS_REMOTE=1` / `SMOKE_SKIP_LFS_CLIENT=1` (routing + SSH TCP; no seeded-repo client) |
| `db-matrix` | Matrix `postgres` / `mysql` / `sqlite`: `cargo test -p oxidean-db --test dialect_probe -- --nocapture` with matching `DATABASE_URL` / `OXIDEAN_DB_DIALECT` (dialect probe only — not a substitute for Compose bring-up) |

Default `web-octane` stays fast (no Docker auth stubs). True auth/email path coverage is the separate `e2e-stack` job. The `coverage-weighted` job enforces D-QH-02 without reviving component Playwright. Compose dialect health (Traefik `/` + `/health` + `system.db_probe`) is the `compose-smoke` matrix — not folded into `smoke-protocol`. Forge protocol edges (Smart HTTP / SSH TCP / LFS batch / packages PathPrefix) are the `smoke-protocol` job — not happy-dom only.

### Compose dialect smokes (local + CI)

| Target | Proves | Notes |
|--------|--------|-------|
| `make smoke` | Postgres Compose bring-up + `db_probe` | Same as CI `compose-smoke` / postgres |
| `make smoke-sqlite` | SQLite overlay bring-up + dialect assert | Host dir via `scripts/sqlite-host-dir.sh` (`./var` on Linux) |
| `make smoke-mysql` | MySQL overlay bring-up + dialect assert | Profile `mysql` |
| `make smoke-compose-ci` | Fail-closed CI entry (`DIALECT=…`) | Same as `./scripts/ci-compose-smoke.sh` |

Oxidean Cloud (Railway IaC + Caddy gateway) is **not** exercised in PR CI — see [DEPLOYMENT.md](DEPLOYMENT.md) and [`.planning/phases/22-compose-ci-deploy/22-VALIDATION.md`](../.planning/phases/22-compose-ci-deploy/22-VALIDATION.md). Local preview: `make cloud-plan` (requires linked Railway CLI).

### Protocol smokes (local + CI)

| Target | Proves | Notes |
|--------|--------|-------|
| `make smoke-git-https` | Traefik `/{owner}/{repo}.git` is not SPA HTML; optional `git ls-remote` | Needs stack up; set `SMOKE_SKIP_LS_REMOTE=1` for routing-only |
| `make smoke-git-ssh` | TCP `OXIDEAN_SSH_PORT` (2222); optional scp-style ls-remote/push | Needs SSH-enabled Compose API |
| `make smoke-git-lfs` | `.git/info/lfs` batch routing not SPA; optional git-lfs client | `SMOKE_SKIP_LFS_CLIENT=1` for routing-only |
| `make smoke-packages` | `/v2` `/npm` `/generic` PathPrefix → API | Needs running Compose API |
| `make smoke-protection` | API image ships `oxidean-protection-hook`; HTTPS push to reviews-required protected branch denied (**ORG-06** / **D-PKG-03**) | Fresh Compose up (wipes volumes); `scripts/compose-smoke-protection.sh` |
| `make smoke-protocol-ci` | All four fail-closed against a fresh Compose up | Same entrypoint as CI `smoke-protocol` |

Locally without Docker, individual `make smoke-git-*` / `smoke-packages` may skip (exit 0). Under `CI=true` or `SMOKE_REQUIRE_STACK=1`, those skips become failures.

## Dev-auth stubs (stack e2e)

See [dev-auth.md](./dev-auth.md) for interactive setup. Stack e2e depends on:

| Service | Default local endpoint | Role in tests |
|---------|------------------------|---------------|
| Mailpit | UI `http://127.0.0.1:8025`, SMTP `smtp://127.0.0.1:1025` | Capture SMTP mail |
| OIDC mock | Issuer `http://127.0.0.1:9090/default` | OIDC login without a real IdP |
| HTTP stubs | `http://127.0.0.1:9092` | Resend `POST /emails` + WorkOS AuthKit |

`e2e-stack` proves SMTP→Mailpit, Resend→stub, WorkOS stub login, and OIDC mock login over HTTP. `e2e-stack-browser` exercises signup UI, WorkOS CTA, and the D-QH-03 forge matrix (repo/packages, issues/releases, SSH keys, org members) against the live web/API in Chromium.

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
