# Team-Level Rules

> This team's affirmed practices and corrections. Loaded after `org.md` as
> strict-additive guidance; contradictions with broader policy are rejected.
> Populated by the practices-discovery affirmation gate. Edit at the gate,
> not directly.

## Way of Working

We use **trunk-based development** on `main`. Every change reaches `main`
through a pull request — the `Main` ruleset blocks direct pushes (the sole
bypass actor is the `oxidean-release` GitHub App, used only for the stable-cut
`chore(release):` bump commit during production promote).

Our branches are short-lived and named `type/<slug>` — `feat/`, `fix/`,
`chore/`, `docs/`, `ci/` — matching the commit-grammar types, often carrying
the issue number (`fix/large-diff-render-110`, `feat/account-2fa-130`). We run
parallel lines of work in **git worktrees** under sibling checkouts
(`git worktree list` shows ~14 active worktrees: feature, fix, CI, and
agent-dispatched `cursor/*` lines). We **squash-merge** PRs: `main` history is
linear, one commit per PR with the `(#NNN)` suffix — `git log --merges` is
empty. AI-DLC Bolt worktrees are the exception to `type/<slug>`: they keep the
engine's own `bolt-<id8>_<slug>` naming because the engine owns them.

Every commit subject follows the enforced SemVer grammar
`type(optional-scope)[!]: subject`. A `commit-msg` hook checks locally
(installed by `make setup`, bypassable with `--no-verify`), and CI's
`commit-lint` job checks the PR range plus the PR title — the gate cannot be
skipped. Subjects are load-bearing: they drive per-component SemVer bumps and
changelogs via the `versioning.toml` registry, so we pick accurate
types/scopes. `chore(release):` is reserved for release tooling.

The affirmed guard policy for this intent is `relaxed`, matching the intent's
standing-autonomy instruction.

## Walking Skeleton

**Skeleton off for this intent — affirmed.** The active scope (`classic`)
ships `skeleton: off`, and the codebase already has a mature, fully-gated
pipeline — a buildable stack, e2e harnesses, smoke suites, and a
promotion-gated deploy all exist — so a bootstrap-slice ceremony would repeat
infrastructure we already have. We skip the skeleton checkpoint and take the
explicit Construction-entry choice (continue automatically under the standing
instruction). The slice wisdom survives as sequencing, not ceremony:
delivery-planning should still order the first Construction unit as a thin
end-to-end slice of the new VCS path (e.g. a repo-create → changeset → fetch
round-trip through API → new crate → storage) as an ordinary unit-ordering
decision.

## Testing Posture

- **Methodology**: test-after
- **Ordering**: We implement the change first, then write and run the tests
  the change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing
  expectations), and the full CI suite must be green before the PR can merge.
- **TDD option**: an in-repo `tdd` skill (`.agents/skills/tdd/`) exists and
  may be invoked deliberately for a unit, but red-first is not the team's
  mandated posture.
- **Coverage**: CI enforces a **weighted composite gate at floor 0.70**
  (`coverage-weighted` job, `COVERAGE_WEIGHTED_FLOOR`, composed by
  `scripts/coverage-weighted.sh`): web unit 0.25 + web integration 0.40 (v8
  line coverage) + e2e 0.35 — where the e2e third is a **7-path presence
  checklist** (`scripts/coverage-e2e-checklist.sh`), not executed coverage;
  e2e correctness is enforced by the separate `e2e-stack` / `actions-e2e` /
  `smoke-protocol` job gates. The composite **consumes web numbers only**:
  Rust coverage is collected (`cargo-llvm-cov`, artifacts uploaded) but
  **not gated**, so this intent's new VCS crate lands with no enforced
  coverage signal — collect-only Rust coverage is recorded debt, and we do
  not invent a new numeric floor for it (the change-type minimums + green CI
  carry its testing). Cross-dialect execution in CI is limited to
  `dialect_probe` per dialect (`db-dialects` job): the ~30 `dialect_*` parity
  files are `DATABASE_URL`-gated and **skip-pass green** in the `api-rust`
  shards — full parity is local-capable only, also recorded debt. No
  performance-test infrastructure exists (no benches, no load harness) —
  recorded debt for NFR work, not a blocker.
- **UI coverage is change-aware**: route- and browser-coverage manifests mean
  every new/changed `apps/web` route needs test evidence, and new/changed
  high-risk interactive UI (Checkbox / RadioGroup / Select / Switch /
  portals / `form.Subscribe`) needs Chromium `*.browser.test.tsx` proof —
  fresh skip-only rows fail CI, and `kind: "browser"` entries are
  source-verified (mount, interact, assert markers). Manifest `skip` rows are
  the only sanctioned skip: zero `#[ignore]` and zero `it.skip`/`test.skip`
  in the tree.
- **Tooling**: cargo-nextest for the Rust workspace (`.config/nextest.toml`,
  `ci` profile: no fail-fast, 90s slow-timeout; one shared nextest archive
  from `rust-build` feeds the 5-way `api-rust` shards, `db-dialects`, and
  archive consumers — one compile per run; shards re-run whole on failure —
  coarse flake tolerance). Vitest 5 for TS with five projects in `apps/web`
  (`unit` on node, `integration` on happy-dom with `fileParallelism: false`,
  `browser` on Playwright Chromium, `e2e-stack` and `e2e-stack-browser` gated
  on `E2E_STACK=1`) plus `@oxidean/api-client`'s own node-env suite.
  Playwright drives real-Chromium component and full-stack e2e; wiremock
  supplies outbound-HTTP fixtures; Rust integration tests open real SQLite
  tempdir DBs; protocol smokes run real `git`, `ssh`, and `git-lfs` client
  binaries against a live Compose stack (fail-closed in CI under
  `SMOKE_REQUIRE_STACK=1`).
- **Local/CI asymmetry**: `make test` runs only nextest + web
  unit/integration; browser, stack e2e, and smokes are separate targets — a
  green local `make test` proves far less than a green CI run, and smoke
  scripts skip-pass locally without Docker but fail-closed under `CI`.
- **Test types / minimums**: per change type — pure helper → unit;
  Query/session UI → happy-dom integration; high-risk UI → Chromium browser
  test + manifest row; auth/RPC contract → Rust integration and/or stack e2e;
  dialect-touching DB code → `dialect_*` test (noting the CI-execution gap —
  the probe is the only enforced cross-dialect run). Suites: colocated Rust
  units plus `crates/*/tests/` integration (107 API test files), web
  unit/integration/component tests, stack e2e with Mailpit/OIDC/Resend-WorkOS
  stubs (`make test-e2e-stack`), forge protocol smokes, compose bring-up
  smoke per dialect, and an Actions pipeline e2e.

## Guard Policy

<!-- Affirmed by the team. Mode: strict, relaxed, or off. Strict here holds for every intent and cannot be changed from chat. A section under the retired Change Control heading, written by an earlier release, is still read. -->

## Deployment

We **deploy on merge to `main`**: `images.yml` builds all four cloud service
images (api, web, runner, gateway) in GitHub Actions, gates the GHCR push
until every image is green, publishes the all-or-nothing `sha-<sha>` tag set,
and deploys that set to the **Railway staging** environment. Same-repo PRs get
`pr-<n>` images bound to an ephemeral Railway PR environment torn down on
close; fork PRs publish nothing. Railway pulls images and never builds from
the repo — every environment runs identical, digest-stable bits, and
`.railway/railway.ts` (TypeScript IaC) seeds each environment's channel tag
(`:staging`, `:production`, `:main`) over managed Postgres.

**Production is a manual promote**, not continuous deployment:
`production-deploy.yml` (workflow_dispatch, GitHub Environment
`Oxidean / production` — required reviewers supported) promotes a chosen SHA
that must be an ancestor of `main` **with the latest CI run green**. A promote
optionally cuts the stable release first (changelog + per-component version
bump + `*-v*` tags, pushed by the release App through the ruleset bypass) and
then repins each service's image — api, web, and gateway; the runner is not
part of the production set. A `rollback` action restores each service's
previous deployment and image ref via `deploymentRollback`. Post-deploy health
is probed at `https://app.oxidean.dev/health`, and promote/rollback post
GitHub Deployment records since Railway doesn't emit them for image repins.
`dry_run` plans without mutating.

Releases are **per-component SemVer** driven by commit subjects
(`versioning.toml`, 10 components + virtual `wire`): every `main` push runs
the beta lane (annotated `<component>-v<semver>-beta.N` prerelease tags),
`*-v*` tag pushes create GitHub Releases, and `ox-v*` tags fire the
cross-platform `ox` CLI binary matrix (Cross.toml + nfpm packages). Stable
cuts happen inside production promote **or** via the maintainer dispatch lane:
`release.yml` `workflow_dispatch` offers `channel: stable` on `release/*`
branches. Self-host deployments use the same images via Docker Compose /
Traefik / nfpm packages — one codebase for cloud and self-host.

The deploy pipeline's security posture: workflow `GITHUB_TOKEN` defaults to
`contents: read` and elevates per job; fork PRs get a read-only token and
publish/deploy nothing; same-repo PRs publish only `pr-*` images to ephemeral
Railway environments torn down on close. Deploy credentials are scoped by
blast radius — repo secrets cover staging and PR environments, while the
production token is an environment-scoped project token on the
`Oxidean / production` GitHub Environment. Railway IaC apply runs from
operator machines only (`railway-apply.sh` refuses under CI); apply tokens
never enter the repo or CI. Release pushes use the `oxidean-release` GitHub
App (stable cuts) or `RELEASE_TOKEN` (beta/dispatch lanes) — never
`GITHUB_TOKEN`, which cannot re-trigger tag lanes.

Supply-chain contract: `bun install --frozen-lockfile` (bun.lock carries
sha512 integrity) and `npm ci` (`.railway/package-lock.json`) are the only
dependency-install paths; `Cargo.lock` is committed and CI tools install via
`cargo install --locked`. CI runs inside self-owned `oxidean-ci:*` images
resolved to `@sha256` digests per run — our one digest-pinning point. Docker
base images and GitHub Actions stay version-tag-pinned rather than
digest-pinned (weekly `ci-base` rebuilds absorb base-image patch drift).
Released `ox` binaries ship per-target `.sha256` + `SOURCE_COMMIT`; image
builds and container boots verify those checksums fail-closed. We run no
SAST/DAST, dependency-vulnerability, or secret scanning, and no update bot —
dependency freshness is manual, an acknowledged status quo recorded as debt
rather than assumed deliberate policy. Runtime hardening note: service images
run as root, and the self-host runner mounts `docker.sock` read-write for
`docker://` jobs (opt-in profile, documented) — accepted risk, recorded debt.

## Code Style

- **Rust**: edition 2021, `cargo fmt` enforced by the CI `rust-fmt` gate (no
  committed rustfmt.toml — defaults); `clippy --workspace --all-targets -D
  warnings`, `-D dead_code`, `cargo machete`, and `RUSTDOCFLAGS="-D warnings"`
  all gate in the `dead-code` job. Three error idioms by layer, not one:
  `thiserror` enums inside libraries (`GitError`, `AuthError`); the
  `oxidean-db::Database` façade deliberately returns `Result<T, String>` —
  errors matched by literal equality/substring in `db_err` adapters, so error
  text is a contract and is never reworded casually; API handlers return
  `Result<T, AppError>` where `AppError{code,message,data}` + the untagged
  `RpcResponse` is the wire contract and codes are stable `domain.reason`
  snake-dotted strings consumed by UI branching. No anyhow/eyre anywhere —
  binaries fail fast with `eprintln!` + `std::process::exit(1)`. Internals
  never leak: `tracing::error!` the detail, return a generic message.
  `unwrap`/`expect` outside tests is a soft avoid (accepted uses exist in
  `main.rs`); the hard gate remains clippy.
- **Layers**: `oxidean-web` edge tier (static dist, meta injection, route
  gating, API proxy) → `oxidean-api` (`app.rs` Router/`AppState` → `rpc.rs`
  `RpcCtx` + one `match` dispatch → domain modules returning
  `Result<T,AppError>`) → `oxidean_db::Database` façade +
  `oxidean_git::GitBackend`; `oxidean-core` is types only, no I/O. There is
  no `services/` layer — services are per-domain internals and named structs
  on `AppState`/`RpcCtx` (`SessionService`, `PendingAuthStore`, `*Limiter`).
  REST `/api/v1` is a pure translation façade over the same `dispatch`
  (`rest/spec.rs` `RouteDef` is the single source for router + OpenAPI).
  Protocol surfaces (smart HTTP, LFS, SSH, OCI/npm/generic, OAuth, MCP,
  actions) are sibling route stacks sharing ACL/audit. New subsystems get a
  **trait+adapter seam** — the `Arc<dyn GitBackend>`/`CliGitBackend` and
  `Arc<dyn EmailSender>` precedent — not inline code in `oxidean-api`; this
  is the established shape the new VCS crate follows.
- **Modules & naming**: domain dirs use `mod.rs` façades — private `mod`
  declarations + aliased `pub use` re-exports, `pub(crate)` for cross-module
  internals; `lib.rs` is a flat `pub mod` list + selective `pub use`;
  binaries live in `src/bin/`; `oxidean-core` keeps one `*_types.rs` per
  domain with glob re-export. Suffix taxonomy: `*Row` db rows,
  `*Request`/`*Response`/`*Public` wire DTOs, `*Service`/`*Store`/`*Limiter`
  api internals; db verbs `insert_/find_*_by_/list_/update_/count_/delete_/
  consume_/revoke_`; `// --- domain ---` banners section the large `Database`
  façade; `#[allow(clippy::too_many_arguments)]` is accepted on wide inserts
  today (forward policy deferred to design — see `evidence.md`).
- **RPC conventions**: procedures are `domain.verb` dotted strings. Existing
  names mix snake_case and camelCase (`auth.two_factor.totp_begin` vs
  `user.getPublicProfile`) — those names are wire-stable and stay frozen;
  **new procedures are `domain.snake_case`** (so `vcs.*` names will be
  snake_case). Registration checklist for a new procedure: `PROCEDURES`
  entry, dispatch arm, `oxidean-core` `*Request`/`*Response` types, handler
  returning `Result<T,AppError>`, then `make rpc-gen` with the regenerated
  client committed in the same change — the `procedures_list_matches_dispatch`
  scan test fails on drift. Requests carry `Oxidean-RPC-Version` (SemVer
  compat; `"1"` is a 0.x legacy alias); `system.manifest` advertises
  procedures + capabilities.
- **Web / TypeScript**: Octane `.tsrx` islands on Astro shells — **never
  React JSX**. The type gate is `make web-lint` (type-aware `oxlint
  --deny-warnings` via `oxlint-tsgolint` + `@tsrx/oxc`, plus the Octane
  DOM-race heuristic); formatting is `oxfmt` for `.tsrx`/`.ts` and Prettier
  for `.astro` (`make web-format-check`). No ESLint. `knip` gates dead
  exports. Kebab-case files, PascalCase components, `@/` alias over deep
  relatives, `import type` for type-only imports. `src/lib/*-queries.ts` are
  TanStack `queryOptions` factories with `queryKey` tuple consts; the
  generated client's result is a discriminated
  `{ok:true,data}|{ok:false,error}` — callers branch on `res.ok` /
  `res.error.code`; `useState`/zustand own local UI state only — TanStack
  Query owns server/session data. Tests colocate by suffix:
  `*.unit.test.ts`, `*.integration.test.ts(x)`, `*.browser.test.tsx`,
  `e2e/stack*/` (plus thematic variants like `*.gate.test.ts`). We run both
  web gates before committing `apps/web` changes.
- **Drift gates**: any duplicated or generated surface ships with its own
  sync check — `rpc-sync-check`, `openapi-sync-check`, `check-route-sync`,
  `skill-sync-check`, `check-workflow-templates`, `cli-parity-check`,
  `check-makefile.sh`, and `include_str!` source-scan tests. Generated
  sources (`packages/api-client`, `docs/openapi.yaml`) are read-only — we
  change the Rust producers and regenerate in the same change.
- **Monorepo conventions**: Bun 1.4.0 is the package manager (frozen
  lockfile); GNU Make is the task façade — every target is `.PHONY`-declared
  with a runnable recipe (`checkmake` + `scripts/check-makefile.sh`). We
  prefer the smallest change that fits an existing pattern and existing
  workspace deps over new frameworks or libraries. In-repo skills under
  `.agents/skills/` (octane, rust-async-patterns, rust-best-practices, tdd,
  semver-conventions, …) are the documented how-we-work and are read before
  inventing workflows; `tmp/` is gitignored scratch. A new crate registers a
  `versioning.toml` component/scope so the commit grammar covers it.
- **Boundaries & comments**: dialect SQL lives only in `crates/oxidean-db`
  (migrations authored ×3 in `migrations/{postgres,mysql,sqlite}` with
  filename-parity tests); no secrets in repo, CI, or docs examples; no
  AI/tool attribution on commits or PRs; new `OXIDEAN_*` env vars are grouped
  and documented in `.env.example` + `docs/CONFIGURATION.md` in the same PR.
  Comments explain *why* (threat model, dialect quirk, Octane pitfall) and
  cite decision IDs (D-12, G-11.1-15) that `docs/ARCHITECTURE.md` records.
## Forbidden

<!-- Team-specific forbidden patterns -->

## Mandated

<!-- Team-specific mandates -->

## Corrections

<!-- Self-learning loop appends here. -->
