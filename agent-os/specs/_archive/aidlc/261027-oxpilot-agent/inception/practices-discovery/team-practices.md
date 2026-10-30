# Team Practices

> Integrated from the affirmed baseline in `aidlc/spaces/default/memory/team.md`,
> fresh evidence, the three blind support reviews (quality, developer,
> devsecops), and the confirmed interview answers Q1–Q7
> (`practices-discovery-questions.md`). Where the baseline had drifted, this
> text states current practice — the interview resolved every open drift.

## Way of Working

We use **trunk-based development** on `main`. Every change reaches `main`
through a pull request — the `Main` ruleset blocks direct pushes (the sole
bypass actor is the `oxidean-release` GitHub App, used only for the
stable-cut `chore(release):` bump commit during production promote).

Our branches are short-lived and named `type/<slug>` — `feat/`, `fix/`,
`chore/`, `docs/`, `ci/` — matching the commit-grammar types, often
carrying the issue number. We run parallel lines of work in **git
worktrees** under sibling checkouts — feature, fix, CI, and
agent-dispatched `cursor/*` lines. We **squash-merge** PRs: `main` history
is linear, one commit per PR with the `(#NNN)` suffix — `git log --merges`
is empty. AI-DLC Bolt worktrees are the exception to `type/<slug>`: they
keep the engine's own `bolt-<id8>_<slug>` naming because the engine owns
them.

Every commit subject follows the enforced SemVer grammar
`type(optional-scope)[!]: subject`. A `commit-msg` hook checks locally
(installed by `make setup`, bypassable with `--no-verify`), and CI's
`commit-lint` job checks the PR range plus the PR title — the gate cannot
be skipped. Subjects are load-bearing: they drive per-component SemVer
bumps and changelogs via the `versioning.toml` registry, so we pick
accurate types/scopes. `chore(release):` is reserved for release tooling.

The `Main` ruleset's required-status-checks list is part of the merge
contract: adding, renaming, or removing a CI job — including matrix value
sets (`shard`, `dialect`, `svc`) — in `ci.yml` or `images.yml` requires
updating the ruleset's check list in the same change, or every PR blocks
(AGENTS.md, CONTRIBUTING.md, `docs/TESTING.md` § Required status checks).

The affirmed guard policy for this intent is `relaxed`, matching the
intent's standing-autonomy instruction (self-drive to a green-CI PR).

## Walking Skeleton

**Skeleton on for this intent — affirmed.** This intent runs scope
`feature`, which ships `skeleton: on`, and the interview confirmed we take
the scope's ceremony rather than overriding it: the first Construction
unit is a thin end-to-end `/oxpilot` chat round-trip — page →
session-gated `oxpilot.*` RPC → stubbed provider → streamed reply through
`/api/` — that proves the seams connect before breadth lands. The affirmed
**off** stance stays the recorded default for `classic`-style work: the
maturity argument (a fully gated 23-job CI pipeline, shared
nextest-archive sharding, stack e2e with dev-auth stubs, protocol smokes,
a weighted coverage gate, a promotion-gated production deploy, and per-PR
ephemeral Railway environments) still holds, and on a `classic` scope we
skip the ceremony and keep the slice as unit-ordering wisdom.

## Testing Posture

- **Methodology**: test-after
- **Ordering**: We implement the change first, then write and run the
  tests the change-type minimum requires (`docs/CODE_PRACTICES.md`
  § Testing expectations), and the full CI suite must be green before
  the PR can merge.
- **TDD option**: an in-repo `tdd` skill (`.agents/skills/tdd/`) exists
  and may be invoked deliberately for a unit, but red-first is not the
  team's mandated posture.
- **Coverage**: CI enforces a **weighted composite gate at floor 0.70**
  (`coverage-weighted` job, `COVERAGE_WEIGHTED_FLOOR`, composed by
  `scripts/coverage-weighted.sh` — itself contract-tested by
  `coverage-weighted.contract.sh` via `make coverage-contract`):
  web unit 0.25 + web integration 0.40 (v8 line coverage) + e2e 0.35 —
  where the e2e third is a **frozen 7-path presence checklist**
  (`scripts/coverage-e2e-checklist.sh` checks `[[ -f ]]`: 2 stack-browser
  + 2 stack + 3 protocol-smoke paths), not executed coverage; e2e
  correctness is enforced by the separate `e2e-stack` / `actions-e2e` /
  `smoke-protocol` job gates. Adding a checklist item grows a hard path
  dependency for every future PR — an `/oxpilot` stack test lands in
  job-gates unless we deliberately grow the denominator. The composite
  **consumes web numbers only**: Rust coverage is collected
  (`cargo-llvm-cov`, artifacts uploaded) but **not gated** — affirmed for
  this intent (Q3): `oxidean-api-oxpilot` lands **collect-only** with no
  new numeric floor; the change-type minimums + green CI carry its
  testing, and we record that decision consciously rather than
  normalizing crate-level coverage blindness. Cross-dialect execution in
  CI is the `db-dialects` job, which runs **three `dialect_*` binaries**
  per dialect — `dialect_probe`, `dialect_vcs`, `dialect_actions` — plus a
  `workflow_conformance` leg replayed on real Postgres **and** MySQL with
  per-test provisioned `oxt_<uuid>` databases via
  `OXIDEAN_TEST_DATABASE_URL` (unset falls back to sqlite pass-through).
  The parity story is narrower than folklore says: of the **23**
  `dialect_*` files in `crates/oxidean-db/tests/`, only **4** gate on
  `DATABASE_URL` (`dialect_probe`, `dialect_auth`, `dialect_vcs`,
  `dialect_actions`); the other ~19 are sqlite-only migration/schema
  assertions that execute for real in the `api-rust` shards.
  **`dialect_auth` is dead coverage**: it gates on `DATABASE_URL` but is
  absent from the `db-dialects` `-E` filter — it has never run against
  Postgres/MySQL in CI (housekeeping: join the filter or document the
  exclusion). If this intent adds `oxidean-db` tables, a
  `dialect_oxpilot.rs` must join the `-E` filter **and** the TESTING.md
  job table in the same change or it is dead-on-arrival (same job — no
  ruleset change needed). No performance-test infrastructure exists (no
  benches, no load harness) — recorded debt for NFR work; note SSE
  streams, LLM round-trips, and computer-use sessions are
  latency/long-lived surfaces the suite cannot measure, so NFR planning
  should at least define smoke-level expectations (stream first-byte,
  session teardown) rather than invent a harness.
- **UI coverage is change-aware**: route- and browser-coverage manifests
  mean every new/changed `apps/web` route needs test evidence (the
  route-coverage manifest carries **82** hand-maintained rows), and
  new/changed high-risk interactive UI (Checkbox / RadioGroup / Select /
  Switch / portals / `form.Subscribe`) needs Chromium
  `*.browser.test.tsx` proof — fresh skip-only rows fail CI, and
  `kind: "browser"` entries are source-verified (mount, interact, assert
  markers). Manifest `skip` rows are the only sanctioned skip: zero
  `#[ignore]` and zero `it.skip`/`test.skip`/`describe.skip` in the tree
  (re-verified by tree scan). For `/oxpilot` specifically the same-change
  registration set is bigger than one manifest row: a `route-coverage`
  row with real happy-dom mount or stack-browser evidence (skip-only
  fails for added routes; file existence is enforced, a mount is
  doc-convention), the `check-route-sync` triangle (`oxidean-web`
  `shells.rs` `SHELLS` + an Astro page emitting the dist shell +
  `route-params.ts` `RESERVED_TOP_SEGMENTS` — a new top-level segment
  omitted there misbinds as `/{owner}`), a `browser-coverage` row if the
  UI uses high-risk controls (a model picker or dialog almost certainly
  does), and the generated `routeTree` sync. The `settings/ai.tsrx`
  route — manifest row + happy-dom test — is the precedent shape for
  oxpilot provider settings.
- **Tooling**: cargo-nextest for the Rust workspace (`.config/nextest.toml`,
  `ci` profile: no fail-fast, 90s slow-timeout, **`retries = 1`** — a
  bounded per-test retry covering lanes with no shell retry such as
  `coverage-weighted` and `db-dialects`; one shared nextest archive from
  `rust-build` — built `--workspace --features server` so the
  feature-gated vcs suite exists in it — feeds the 5-way `api-rust`
  shards, `db-dialects`, `vcs-server`, and archive consumers, one compile
  per run; `api-rust` shards additionally re-run whole on failure via a
  shell-level retry — coarse flake tolerance). Vitest 5 for TS with five
  projects in `apps/web` (`unit` on node, `integration` on happy-dom with
  `fileParallelism: false`, `browser` on Playwright Chromium,
  `e2e-stack` and `e2e-stack-browser` gated on `E2E_STACK=1`) plus
  `@oxidean/api-client`'s own node-env suite; the `web-browser` job runs
  the browser project as **5 internal shards** with a retry keyed
  strictly on `VITEST_BROWSER_CONNECTION_CLOSED` (Chromium
  session-lifetime bound). Playwright drives real-Chromium component and
  full-stack e2e; wiremock supplies outbound-HTTP fixtures — connector
  tests belong in the crate's `tests/` against mocked HTTP, never live
  providers; Rust integration tests open real SQLite tempdir DBs (or
  fresh provisioned per-test DBs when `OXIDEAN_TEST_DATABASE_URL` points
  at a throwaway Postgres/MySQL); protocol smokes run real `git`, `ssh`,
  and `git-lfs` client binaries against a live Compose stack (fail-closed
  in CI under `SMOKE_REQUIRE_STACK=1`). **SSE streaming has no proven
  test-type precedent**: happy-dom cannot exercise real SSE. The assigned
  layers are an HTTP-level `*.stack.test.ts` (node env,
  `OXIDEAN_E2E_API_ORIGIN`) asserting event-stream frames against the
  live API for the stream contract, happy-dom for the client
  append/fallback state, and optional stack-browser click-through for the
  user-visible flow.
- **Local/CI asymmetry**: `make test` runs nextest + web
  unit/integration (plus the `oxidean-vcs --features server` leg);
  browser, stack e2e, and smokes are separate targets — a green local
  `make test` proves far less than a green CI run, and smoke scripts
  skip-pass locally without Docker but fail-closed under `CI`.
- **Test types / minimums**: per change type — pure helper → unit;
  Query/session UI → happy-dom integration; high-risk UI → Chromium
  browser test + manifest row; auth/RPC contract → Rust integration
  and/or stack e2e; dialect-touching DB code → `dialect_*` test (noting
  the CI-execution gap — and that a new gated binary only counts once it
  joins the `db-dialects` `-E` filter). Suites: colocated Rust units plus
  `crates/*/tests/` integration (**130** API test files — and
  `procedures_list_matches_dispatch` is a *per-crate* convention, so
  `oxidean-api-oxpilot` ships its own copy), web
  unit/integration/component tests, stack e2e with Mailpit/OIDC/
  Resend-WorkOS stubs (`make test-e2e-stack`), forge protocol smokes,
  compose bring-up smoke per dialect, a VCS-server leg, and an Actions
  pipeline e2e.

## Deployment

We **deploy on merge to `main`**: `images.yml` builds all four cloud
service images (api, web, runner, gateway) in GitHub Actions, gates the
GHCR push until every image is green, publishes the all-or-nothing
`sha-<sha>` tag set, and deploys that set to the **Railway staging**
environment. Same-repo PRs get `pr-<n>` images bound to an ephemeral
Railway PR environment — teardown is best-effort on close plus the
hourly scheduled `pr-env-sweep.yml` backstop for leaked envs (the sweep
moved to its own workflow in #167); fork PRs publish nothing. Railway
pulls images and never builds from the repo — every environment runs
identical, digest-stable bits, and `.railway/railway.ts` (TypeScript IaC)
seeds each environment's channel tag (`:staging`, `:production`, `:main`)
over managed Postgres.

**Production is a manual promote**, not continuous deployment:
`production-deploy.yml` (workflow_dispatch, GitHub Environment
`Oxidean / production` — required reviewers supported) promotes a chosen
SHA that must be an ancestor of `main` **with the latest CI run green**.
A promote optionally cuts the stable release first (changelog +
per-component version bump + `*-v*` tags, pushed by the release App
through the ruleset bypass) and then repins each service's image — api,
web, and gateway; the runner is not part of the production set. A
`rollback` action restores each service's previous deployment and image
ref via `deploymentRollback`. Post-deploy health is probed at
`https://app.oxidean.dev/health`, and promote/rollback post GitHub
Deployment records since Railway doesn't emit them for image repins.
`dry_run` plans without mutating.

Releases are **per-component SemVer** driven by commit subjects
(`versioning.toml` — **38 components + virtual `wire`**: the #162
domain-crate split gave each `oxidean-api-*` crate its own component,
scope set, and `depends_on` edges). Every `main` push runs the beta lane
(annotated `<component>-v<semver>-beta.N` prerelease tags), `*-v*` tag
pushes create GitHub Releases, and `ox-v*` tags fire the cross-platform
`ox` CLI binary matrix (Cross.toml + nfpm packages). Stable cuts happen
inside production promote **or** via the maintainer dispatch lane:
`release.yml` `workflow_dispatch` offers `channel: stable` on
`release/*` branches. Self-host deployments use the same images via
Docker Compose / Traefik / nfpm packages — one codebase for cloud and
self-host.

The deploy pipeline's security posture: workflow `GITHUB_TOKEN` defaults
to `contents: read` and elevates per job (no `pull_request_target`
anywhere); fork PRs get a read-only token, publish nothing, and skip
artifact cleanup; same-repo PRs publish only `pr-*` images to ephemeral
Railway environments. `pr-env-sweep.yml` mints narrowly-scoped
release-App tokens per leg and degrades loudly on a missing grant — the
best least-privilege minting pattern in the repo. Deploy credentials are
scoped by blast radius — repo secrets cover staging and PR environments,
while the production token is an environment-scoped project token on the
`Oxidean / production` GitHub Environment. Railway IaC apply runs from
operator machines only (`railway-apply.sh` refuses under CI); apply
tokens never enter the repo or CI. Release pushes use the
`oxidean-release` GitHub App (stable cuts) or `RELEASE_TOKEN`
(beta/dispatch lanes) — never `GITHUB_TOKEN`, which cannot re-trigger
tag lanes.

Supply-chain contract: `bun install --frozen-lockfile` (bun.lock carries
sha512 integrity) and `npm ci` (`.railway/package-lock.json`) are the
only dependency-install paths; `Cargo.lock` is committed and CI tools
install via `cargo install --locked`. CI runs inside self-owned
`oxidean-ci:*` images resolved to `@sha256` digests per run — our one
digest-pinning point. Docker base images and GitHub Actions stay
version-tag-pinned rather than digest-pinned (weekly `ci-base` rebuilds
absorb base-image patch drift). Released `ox` binaries ship per-target
`.sha256` + `SOURCE_COMMIT`; image builds and container boots verify
those checksums fail-closed. **Dependency freshness runs through
dependabot** (affirmed, Q1): `.github/dependabot.yml` batches weekly
Monday updates across cargo, bun (grouped, `oxlint-tsgolint` pinned out —
a solo bump breaks `make web-lint`), github-actions, docker-compose, and
docker ecosystems; `docker/ci-base` is **structurally excluded** — the
docker `directories:` list omits it and its `FROM`s are `ARG`-indirected
anyway, so the pins hand-bump by deliberate one-line PRs (see Mandated
rules). The recorded debt is reworded to the real gap: **no
CVE-alerting/triage path** — nothing invokes `cargo audit`,
`cargo deny`, `bun audit`, an SBOM tool, or a secret scanner in CI or
hooks, and GitHub-side Dependabot security-updates/secret scanning are
repo settings, not file-verifiable; no new gate is added by this
decision. Runtime hardening note: service images run as root, and the
self-host runner mounts `docker.sock` read-write for `docker://` jobs
(opt-in labels, documented) — accepted risk, recorded debt, unchanged and
deliberately not merged with this intent's new credential surface.

## Code Style

- **Rust**: edition 2021, `cargo fmt` enforced by the CI `rust-fmt` gate
  (no committed rustfmt.toml — defaults); `clippy --workspace
  --all-targets -D warnings` (plus a second `oxidean-vcs --all-features`
  pass), `-D dead_code`, `cargo machete`, and `RUSTDOCFLAGS="-D
  warnings"` all gate in the `dead-code` job. Three error idioms by
  layer, not one: `thiserror` enums inside libraries (`GitError`,
  `AuthError`); the `oxidean-db::Database` façade deliberately returns
  `Result<T, String>` — errors matched by literal equality/substring in
  `db_err` adapters, so error text is a contract and is never reworded
  casually; API handlers return `Result<T, AppError>` where
  `AppError{code,message,data}` + the untagged `RpcResponse` is the wire
  contract and codes are stable `domain.reason` snake-dotted strings
  consumed by UI branching (transport denials stay `auth.*`/`rpc.*`;
  domain codes come from the domain layer — `oxpilot.<reason>` for this
  intent). No anyhow/eyre anywhere — binaries fail fast with
  `eprintln!` + `std::process::exit(1)`. Internals never leak:
  `tracing::error!` the detail, return a generic message.
  `unwrap`/`expect` outside tests is a soft avoid (accepted uses exist
  in `main.rs`); the hard gate remains clippy.
- **Layers** (post-#162 shape — affirmed as team practice): `oxidean-web`
  edge tier (static dist, meta injection, route gating, API proxy) →
  `oxidean-api` **composition shell** (`app.rs` mount table, a
  `LazyLock` `PROCEDURES` aggregation of the core residual + all 25
  domain lists, `gate` run once, then first-claim-wins `dispatch`
  fan-out, MCP, codegen bins) over **`oxidean-api-core`** (shared
  substrate: `RpcCtx`, `AppState`, gate, session/PAT auth,
  `rpc_dispatch` — the in-process dispatch handle that lets the REST
  façade and edge routes call assembled dispatch without depending on
  the shell — and residual `system.health/echo/db_probe/manifest`,
  `invites.*`, `search.global`) and **25 `oxidean-api-*` domain
  crates** (27 `oxidean-api-*` dirs total counting core and the
  `oxidean-api-rest` façade), each a disjoint `PROCEDURES` const +
  `dispatch() -> Option<RpcResponse>` that may depend on
  `oxidean-api-core` **only among `oxidean-api*` crates** — foundational
  deps (`oxidean-core`, `-db`, `-git`) are allowed and used. Dependency
  direction and disjointness are machine-enforced by
  `check-api-domain-graph.mjs` → `oxidean_db::Database` façade +
  `oxidean_git::GitBackend`; `oxidean-core` is types only, no I/O.
  There is no `services/` layer — services are per-domain internals and
  named structs on `AppState`/`RpcCtx` (`SessionService`,
  `PendingAuthStore`, `*Limiter`, `Arc<dyn EmailSender>`,
  `Option<SharedVcs>`). REST `/api/v1` is a pure translation façade over
  the same `dispatch` (`oxidean-api-rest` `RouteDef` is the single
  source for router + OpenAPI, with an oracle test asserting `procedure`
  backlinks). Protocol surfaces (smart HTTP, LFS, SSH, OCI/npm/generic,
  OAuth, MCP, actions, `/api/vcs` transport) are sibling route stacks
  sharing ACL/audit. New RPC surfaces land as a **domain crate** —
  mandated, Q5: `oxpilot.*` lands in a new `oxidean-api-oxpilot`, never
  in core's residual — and new subsystems get a **trait+adapter seam**:
  settings-rebuildable → the `Arc<RwLock<Arc<dyn …>>>` `email_slot`
  shape; optional/validated-at-boot → the `Option<Shared*>` `vcs` shape
  with a domain wire error (`oxpilot.disabled`); slot *types* live in
  `api-core` when core infra must see them (the `SharedVcs`-in-core
  precedent), logic in the domain crate; agent-internal tool calls ride
  `RpcCtx.rpc_dispatch` rather than duplicating domain logic. LLM
  provider adapters and computer-use connectors follow the same
  `Arc<dyn …>`-on-`RpcCtx` shape. Known housekeeping: the agent-facing
  rule surfaces (`.cursor/rules/rust-crates.mdc`,
  `docs/CODE_PRACTICES.md` boundary table) still teach the pre-#162
  monolith — update during implementation, not blocking.
- **Modules & naming**: domain dirs use `mod.rs` façades — private `mod`
  declarations + aliased `pub use` re-exports, `pub(crate)` for
  cross-module internals; `lib.rs` is a flat `pub mod` list + selective
  `pub use`; binaries live in `src/bin/`; `oxidean-core` keeps one
  `*_types.rs` per domain with glob re-export. Suffix taxonomy: `*Row`
  db rows, `*Request`/`*Response`/`*Public` wire DTOs,
  `*Service`/`*Store`/`*Limiter` api internals; db verbs
  `insert_/find_*_by_/list_/update_/count_/delete_/consume_/revoke_`;
  `// --- domain ---` banners section the large `Database` façade;
  `#[allow(clippy::too_many_arguments)]` is accepted on wide inserts
  today (forward policy deferred to design).
- **RPC conventions**: procedures are `domain.verb` dotted strings.
  Existing names mix snake_case and camelCase
  (`auth.two_factor.totp_begin` vs `user.getPublicProfile`) — those
  names are wire-stable and stay frozen; **new procedures are
  `domain.snake_case`** (so `oxpilot.session_create`, not
  `oxpilot.createSession`). Registration checklist for a new procedure
  family in an existing crate: `PROCEDURES` entry + `dispatch` arm,
  `oxidean-core` `*Request`/`*Response` types, a `proc_classes`
  classification note, handler returning `Result<T,AppError>`, then
  `make rpc-gen` with the regenerated client committed in the same
  change — the `procedures_list_matches_dispatch` and
  `rest_spec_procedures` scan tests fail on drift. Standing up the
  **new crate** adds: workspace `Cargo.toml` `members` entry (explicit
  list), a `crates/oxidean-api` path dep + `rpc.rs`
  `extend_from_slice` + fan-out arm, a `versioning.toml` `[[component]]`
  (`scopes`, `depends_on = ["oxidean-api-core"]`, tag_prefix,
  changelog_path, globs) **plus** the new id added to `oxidean-api`'s
  `depends_on`, a per-crate `CHANGELOG.md`, and the crate's own
  `procedures_list_matches_dispatch` test; literal `[package] version`,
  not `version.workspace = true`. The crate skeleton follows the
  domain-crate contract exactly (`oxidean-api-issues` is canonical):
  `lib.rs` = doc comment + `mod dispatch;` + handler modules +
  `pub use dispatch::dispatch;` + `pub const PROCEDURES`; `dispatch.rs`
  = the copy-paste `arm!` macro over `match procedure { … _ => None }`
  returning `Option<RpcResponse>`. `proc_classes` classification is
  fail-closed — unclassified procs are `SessionOnly` (refused to
  PAT/`ox`/MCP bearers); affirmed for this intent (Q4): `oxpilot.*`
  **stays `SessionOnly` in v1** — the surface is session-driven by
  design — pinned with a `VCS_PROCEDURES`-style `OXPILOT_PROCEDURES`
  const in `oxidean-core` plus a closed-set test asserting both
  directions (PROCEDURES↔arms↔classification), revisit if `ox`/MCP
  access is ever wanted. Requests carry `Oxidean-RPC-Version` (SemVer
  compat; `"1"` is a 0.x legacy alias); `system.manifest` advertises
  procedures + capabilities.
- **Web / TypeScript**: Octane `.tsrx` islands on Astro shells — **never
  React JSX**. The type gate is `make web-lint`, which runs **six
  commands**: `oxlint --type-aware` via `oxlint-tsgolint` + `@tsrx/oxc`,
  `check-octane-dom-races.ts`, `check-internal-anchors.ts`,
  `check-template-semicolons.ts`, `check-route-sync.ts` (the
  SHELLS/Astro/`RESERVED_TOP_SEGMENTS` triangle), and
  `check-pending-before-notfound.ts`, plus the post-build
  `web-render-check` scanning `dist` for stray-semicolon and not-found
  render defects; formatting is `oxfmt` for `.tsrx`/`.ts` and Prettier
  for `.astro` (`make web-format-check`). No ESLint. `knip` gates dead
  exports. Kebab-case files, PascalCase components, `@/` alias over deep
  relatives, `import type` for type-only imports. `src/lib/*-queries.ts`
  are TanStack `queryOptions` factories with `queryKey` tuple consts —
  the `/oxpilot` page mirrors `settings/ai.tsrx`
  (`useQuery(authSessionQueryOptions())`, `usePageRedirect` auth gate)
  plus an `oxpilot-queries.ts` factory; the generated client's result is
  a discriminated `{ok:true,data}|{ok:false,error}` — callers branch on
  `res.ok` / `res.error.code`; `useState`/zustand own local UI state
  only — TanStack Query owns server/session data. For streamed data,
  the proven path is the job-log SSE design (snapshot frame →
  wake-channel appends → fallback tick, `KeepAlive` 15s — the
  `stream_job_log` handler lives in `api-rest` only because it shares
  download auth helpers; it is not a `RouteDef`/OpenAPI surface) —
  **not** the older `refetchInterval`/`wait_ms` poll patterns that
  coexist in the Actions UI. The oxpilot SSE chat route mounts in
  `app.rs` under `/api/` with the handler in `oxidean-api-oxpilot`.
  Tests colocate by suffix: `*.unit.test.ts`, `*.integration.test.ts(x)`,
  `*.browser.test.tsx`, `e2e/stack*/` (plus thematic variants like
  `*.gate.test.ts`). We run both web gates before committing `apps/web`
  changes.
- **Drift gates**: any duplicated or generated surface ships with its
  own sync check — `rpc-sync-check`, `openapi-sync-check`,
  `check-route-sync`, `skill-sync-check`, `check-workflow-templates`,
  `check-stack-presets`, `api-domain-graph-check`, `cli-parity-check`,
  `check-makefile.sh`, and `include_str!` source-scan tests (the
  api-domain-graph and stack-presets checks are members of the
  `rpc-sync` job's `git diff --exit-code` set). Generated sources
  (`packages/api-client`, `docs/openapi.yaml`) are read-only — we
  change the Rust producers and regenerate in the same change.
- **Monorepo conventions**: Bun **1.4.4** is the package manager
  (frozen lockfile; `packageManager` pin) with Turborepo orchestration;
  GNU Make is the task façade — every target is `.PHONY`-declared with
  a runnable recipe (`checkmake` + `scripts/check-makefile.sh`). We
  prefer the smallest change that fits an existing pattern and existing
  workspace deps over new frameworks or libraries. In-repo skills under
  `.agents/skills/` (octane, rust-async-patterns, rust-best-practices,
  tdd, semver-conventions, …) are the documented how-we-work and are
  read before inventing workflows; `tmp/` is gitignored scratch. A new
  crate registers a `versioning.toml` component/scope (literal
  `[package] version`, not `version.workspace = true`) so the commit
  grammar covers it.
- **Boundaries & comments**: dialect SQL lives only in
  `crates/oxidean-db` (migrations authored ×3 in
  `migrations/{postgres,mysql,sqlite}` with filename-parity tests —
  this intent's transcript/session tables follow the same ×3
  discipline); no secrets in repo, CI, or docs examples; no AI/tool
  attribution on commits or PRs; new `OXIDEAN_*` env vars are grouped
  and documented in `.env.example` + `docs/CONFIGURATION.md` in the
  same PR; new edge paths stay under the existing `/api/` prefix so the
  four hand-synced path sets (API_PREFIXES, Traefik labels, Caddyfile
  matchers, astro dev proxy) need no edit. Credential custody follows
  the codebase's enforced split (affirmed, Q7): verifier-class secrets
  hash-at-rest (`token_hash` SHA-256 precedent), retrievable secrets
  AES-256-GCM via `crypto.rs`'s env-key chain under a named
  `OXIDEAN_OXPILOT_*` var (fail-closed when unset), and agent-reachable
  surfaces never return stored secret material (RPC, MCP, SSE, logs);
  new credential-accepting surfaces ride the D-26 `FailedAuthLimiter`
  pattern (noting it is per-process, single-replica). Audit events for
  security-relevant operations — session lifecycle, tool-approval
  decisions, connector connect/disconnect, credential mint/revoke —
  ride `audit.rs` fire-and-forget (audit failure never fails the
  operation), and `detail` never carries secret material — extended
  naturally to transcripts/provider calls, the exfiltration edge this
  feature uniquely owns. Comments explain *why* (threat model, dialect
  quirk, Octane pitfall) and cite decision IDs (D-12, G-11.1-15) that
  `docs/ARCHITECTURE.md` records.
