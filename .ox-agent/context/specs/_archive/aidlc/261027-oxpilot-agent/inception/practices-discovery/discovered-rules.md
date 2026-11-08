# Discovered Rules

> Hard constraints only — each rule cites the mechanism that enforces it.
> The affirmed 2026-10-18 set in `project.md` `## Mandated` / `## Forbidden`
> was re-verified against current evidence (every affirmed rule still holds),
> and the interview (Q1–Q7, confirmed) decided every new candidate — nothing
> here is tentative.

## Mandated

- ALWAYS write every commit subject in the SemVer grammar
  `type(optional-scope)[!]: subject` — allowed types in `versioning.toml`
  `[grammar]` (`feat`, `fix`, `chore`, `docs`, `style`, `refactor`,
  `perf`, `test`, `build`, `ci`, `revert`; `release` scope reserved) —
  enforced by `.githooks/commit-msg` (installed via `make setup`) and CI
  `commit-lint` on the PR range and PR title; subjects drive
  per-component version bumps and changelogs.

- ALWAYS reach `main` through a pull request — the `Main` ruleset admits
  only PR merges (squash-merged, linear history); the sole bypass actor
  is the `oxidean-release` GitHub App for the stable-cut
  `chore(release):` commit (AGENTS.md, `docs/TESTING.md` § Required
  status checks, `production-deploy.yml` header).

- ALWAYS update the `Main` ruleset's required-status-checks list in the
  same change when adding, renaming, or removing a job — or a matrix
  value set (`shard`, `dialect`, `svc`) — in `ci.yml` or `images.yml`
  (`.cursor/rules/oxidean-core.mdc`, AGENTS.md § Required
  status checks, CONTRIBUTING.md step 7, `docs/TESTING.md` § Required
  status checks — a stale context blocks every PR; a missing one leaves
  the job ungated).

- ALWAYS regenerate generated artifacts from their producers and commit
  the regenerated output in the same change — `make rpc-gen` after
  RPC/type changes (`packages/api-client`), `make openapi-gen`
  (`docs/openapi.yaml`), and the route/skill/stack-preset/
  workflow-template/api-domain-graph sync targets; CI `rpc-sync` runs
  each check with `git diff --exit-code` semantics (`.cursor/rules/
  rpc-codegen.mdc`, AGENTS.md).

- ALWAYS keep dialect-specific SQL inside `crates/oxidean-db`; API, web,
  and any new crate must not branch on DB dialect — migrations are
  authored ×3 in `migrations/{postgres,mysql,sqlite}` with
  filename-parity tests (AGENTS.md hard boundary, `.cursor/rules/
  oxidean-core.mdc`, `docs/CODE_PRACTICES.md` boundary table).

- ALWAYS pass `make web-lint` and `make web-format-check` before merging
  `apps/web` changes; CI `web-octane` gates both — type-aware
  `oxlint --deny-warnings` is the web type gate (plain `tsc` does not
  parse `.tsrx`) (`.cursor/rules/oxidean-core.mdc`, `.cursor/rules/
  octane-ui.mdc`, PR template checklist).

- ALWAYS declare new Makefile targets `.PHONY` with a runnable recipe
  under `scripts/` paths that exist — `scripts/check-makefile.sh` and
  the CI `makefile-lint` job enforce it.

- ALWAYS register coverage evidence for surfaces the manifests govern:
  every new/changed `apps/web` route needs a `route-coverage` row, and
  new/changed high-risk interactive UI needs a `*.browser.test.tsx` or
  documented stack-browser evidence — the change-aware `route-coverage`
  and `browser-coverage` CI jobs fail on fresh skip-only rows
  (`docs/TESTING.md`, `docs/CODE_PRACTICES.md` § browser coverage).

- ALWAYS install JS dependencies via `bun install --frozen-lockfile`
  (workspace) or `npm ci` (`.railway/`) — the committed lockfiles are
  the dependency contract (used throughout `ci.yml`/`ci-base`;
  `.railway/` keeps its own `package-lock.json` isolated from Bun).

- ALWAYS deploy the all-or-nothing `sha-<sha>` image set; channel tags
  (`:staging`/`:production`/`:main`) move only via deploy tooling after
  a green deploy — `images.yml`/`production-deploy.yml` run
  `cancel-in-progress: false` so a tag set is never half-published.

- ALWAYS keep `railway config apply` off CI and IaC-apply credentials
  out of repo/CI secrets — `scripts/railway-apply.sh` refuses to run
  under CI; apply tokens never enter the repo or CI (`docs/DEPLOYMENT.md`
  D-CLOUD-07).

- ALWAYS require a production promote SHA to be an ancestor of `main`
  with the latest CI run green; rehearse changes with `dry_run` before
  mutating — enforced in `production-deploy.yml` (`git merge-base
  --is-ancestor` + `gh api actions/runs` conclusion check).

- ALWAYS document every new `OXIDEAN_*` env var in `.env.example` and
  `docs/CONFIGURATION.md` in the same PR that introduces it — PR-checklist
  requirement (`.github/pull_request_template.md`).

- ALWAYS authenticate protocol/machine transports with PAT/pubkey-class
  credentials — session cookies are never accepted on transport surfaces
  (D-12 precedent: git transports accept Basic-PAT/pubkey only;
  `architecture.md` § Data Flow).

- ALWAYS use the `oxidean-release` GitHub App (stable cuts) or
  `RELEASE_TOKEN` (beta/dispatch tag lanes) for release-automation pushes
  — never `GITHUB_TOKEN`, which cannot re-trigger tag lanes
  (`production-deploy.yml` header; FR3.4).

- ALWAYS land a new RPC family in its own `oxidean-api-*` domain crate —
  `PROCEDURES` const + `dispatch` arm + workspace `members` entry +
  `versioning.toml` `[[component]]` (`scopes`, `depends_on =
  ["oxidean-api-core"]`, tag_prefix, changelog_path, globs) + per-crate
  `CHANGELOG.md` + the shell `Cargo.toml` dep, `extend_from_slice`, and
  fan-out arm (interview Q5; machine-enforced by
  `scripts/check-api-domain-graph.mjs` — dependency direction and
  `PROCEDURES` disjointness — which auto-scans any new
  `crates/oxidean-api-*` dir).

- ALWAYS hand-bump `docker/ci-base` ARG pins (`RUST_IMAGE`/`BUN_IMAGE`/
  `NODE_IMAGE` and the `CARGO_*`/`DOCKER_*` tool ARGs) in the same change
  that needs them — two-sided per interview Q6: they are content-hash
  inputs bumped by deliberate one-line PRs, never by automation, so the
  content-tagged CI image mints exactly when intended; and dependabot
  does not cover that directory by design — the docker `directories:`
  list in `.github/dependabot.yml` structurally omits `/docker/ci-base`
  and its `FROM`s are `ARG`-indirected (`.github/dependabot.yml` L10-13
  comment + L71-80 coverage; weekly `ci-base.yml` cron still absorbs
  base-image patch drift under an unchanged tag).

- ALWAYS store retrievable secrets as AES-256-GCM ciphertext via
  `crypto.rs`'s shared box, keyed by SHA-256 of the first set var in a
  per-domain env preference chain under a named `OXIDEAN_OXPILOT_*` var
  (fail-closed when unset — the `OXIDEAN_ACTIONS_SECRETS_KEY` →
  `OXIDEAN_SECRETS_KEY` → `OXIDEAN_SESSION_SECRET` precedent), and
  verifier-class secrets as hashes at rest (`token_hash` SHA-256 +
  display `token_prefix` precedent from `0008_pats.sql` /
  `0021_actions.sql`); no plaintext-credential column ships (interview
  Q7; `crates/oxidean-api-core/src/crypto.rs`,
  `docs/CONFIGURATION.md` L88-89).

- ALWAYS use the in-repo skill under `.agents/skills/` when one matches
  the task — `.cursor/rules/use-in-repo-skills.mdc` ("Do not
  invent a parallel workflow when a project skill already covers the
  task") and `.cursor/rules/oxidean-core.mdc` make this an always-on
  rule for agents; AGENTS.md § In-repo skills mirrors it.

## Forbidden

- NEVER commit secrets — no `.env`, tokens, or private keys in the repo,
  CI config/logs, or docs examples; examples live in `.env.example` /
  `docs/dev-auth.env.example` (AGENTS.md hard boundary, PR checklist,
  `docs/CODE_PRACTICES.md` § Security & privacy).

- NEVER hand-edit generated sources as if they were the source of truth —
  `packages/api-client/src/index.ts`, `docs/openapi.yaml`; change the
  Rust producers and regenerate (AGENTS.md hard boundary, `.cursor/rules/
  rpc-codegen.mdc`).

- NEVER add AI or tooling attribution (`Co-authored-by`, `Generated
  with`, `Made-with`, bot generator signatures, model-as-author) to
  commits or PRs — `.cursor/rules/no-ai-attribution.mdc` (forbidden
  patterns enumerated), AGENTS.md.

- NEVER author React JSX in `apps/web` — UI is Octane `.tsrx` only; do
  not alias `react` → Octane or mix `return (` JSX with Rivet `@{`/`@if`
  directives in one component (AGENTS.md § Octane is not React JSX,
  `.cursor/rules/octane-ui.mdc`).

- NEVER reintroduce `Swatinem/rust-cache` or per-job Rust toolchain
  installs into `ci.yml` — the digest-pinned `oxidean-ci:*` images are
  the dep cache by design (AGENTS.md § CI images; `ci.yml` header).

- NEVER treat a local `--no-verify` commit or a skipped local gate as
  merge-safe — `commit-lint` re-checks the PR range + title and the full
  CI suite is a required status check; smoke/test scripts that skip-pass
  locally fail closed under `CI`/`SMOKE_REQUIRE_STACK=1`
  (`.githooks/commit-msg`, `docs/TESTING.md` § Protocol smokes).

- NEVER weaken established auth gates or destructive-action
  confirmations for convenience — `require_verified`, admin gates, and
  factory-reset confirmation strings are preserved contract
  (`.cursor/rules/rust-crates.mdc`, `docs/CODE_PRACTICES.md` § Security;
  audit events for security-relevant operations are fire-and-forget —
  audit failure must never fail the operation it describes).

- NEVER commit planning chatter into product docs — AI-DLC workflow
  state lives under `aidlc/`; scratch goes to gitignored `tmp/`
  (`docs/CODE_PRACTICES.md` § Docs & comments, `.cursor/rules/
  oxidean-core.mdc`, `.gitignore`).

- NEVER leave a new/changed test surface skipped by silent annotation —
  zero `#[ignore]`, zero `it.skip`/`test.skip`/`describe.skip` in the
  tree (verified by scan); the only sanctioned skip is a coverage-manifest
  `skip` row with rationale, and fresh skip-only rows on changed surfaces
  fail the change-aware CI gates (`docs/TESTING.md` § coverage gates).

- NEVER add new procedure families to `oxidean-api-core`'s residual
  `PROCEDURES` — the residual is reserved for `system.*`/`invites.*`/
  `search.global`-class cross-cutting leftovers; new families land in a
  domain crate (interview Q5 — the mirror of the Mandated domain-crate
  rule; `check-api-domain-graph.mjs` enforces disjointness).

- NEVER return stored secret material from agent-reachable surfaces —
  RPC responses, MCP, SSE frames, or logs must never emit decrypted
  connector credentials, action secrets, or provider keys; this
  operationalizes the intent's zero-credential-leak success gate
  (interview Q7; extends the `audit.rs` `detail`-never-carries-secrets
  convention to transcripts and provider calls).
