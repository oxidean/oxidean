# Practices Discovery — Evidence Log

> Integrated record (Step 5) by aidlc-pipeline-deploy-agent: what the lead
> draft inspected and what each inferred practice rests on, what the three
> blind support reviews verified or objected to, how the interview resolved
> each open question, and what remains unresolved or deferred. Interview
> answers were conductor-decided under the standing continuation instruction
> (2026-10-18) and are open to reversal at PR review.

## Memory-layer baseline before discovery

- `aidlc/spaces/default/memory/team.md` — all sections are template comments;
  **no prior affirmed practices**. This run is effectively a first
  affirmation; nothing below contradicts an existing baseline.
- `aidlc/spaces/default/memory/org.md` — framework defaults: trunk-based
  dev + squash-merge; `skeleton` selected per scope flag; Testing Posture
  default `test-after` with per-scope floors (`classic` adds an 80%
  line-coverage floor + CI-before-merge); deploy-on-merge to staging with
  manual production gate; code style deferred to project linter configs;
  `## Guard Policy` empty at org level.
- `aidlc/spaces/default/memory/project.md` — learned rules already on file:
  Docker preferred for debugging/testing (compose stack, `make up/smoke`,
  `act`); every Makefile target `.PHONY` + runnable recipe
  (`scripts/check-makefile.sh`); uniform versioning = one SemVer practice
  with per-component numbers from a shared baseline; two-tier changelog
  layout. Intent record (`aidlc-state.md`): scope `classic` →
  `guard_policy: relaxed`, `skeleton: off` (`.claude/scopes/aidlc-classic.md`),
  Test Strategy `Standard`.

## Sources inspected

| Source | What it provided |
|---|---|
| `git log --oneline -30`, `git log --merges`, `git worktree list`, `git branch -a` | Commit grammar in practice (`fix(ui):`, `feat(auth):`, `ci:`, `perf(web):`); `(#NNN)` suffixes + **zero merge commits** → PR squash-merges; ~14 active worktrees on `feat/ fix/ chore/ ci/ docs/ cursor/` branches, several issue-numbered |
| `.githooks/commit-msg` | Local SemVer-grammar enforcement via `bun scripts/versioning/cli.ts commit-check`; `--no-verify` bypass noted in header |
| `.github/workflows/ci.yml` | ~20-job PR/main pipeline: makefile-lint → digest-pinned `oxidean-ci:*` image resolution → commit-lint, rust-fmt, rust-build (shared nextest archive), api-rust (5 shards), web-octane (lint/format/test/build), dead-code (knip+machete+clippy+rustdoc), web-browser (Chromium), route-coverage, browser-coverage, coverage-weighted (**floor 0.70**, unit .25/integration .40/e2e-checklist .35), e2e-stack, actions-e2e, rpc-sync (rpc/openapi/skill/workflow-template drift gates), compose config check, compose-smoke (pg/mysql/sqlite), smoke-protocol (real git/ssh/lfs), db-dialects, build-metrics, artifact cleanup |
| `.github/workflows/ci-base.yml`, `docker/ci-base/` | Self-owned CI images `ghcr.io/<owner>/oxidean-ci:{dev,release,cov,tools}`, content-hash tags resolved to `@sha256` per run; deliberate no-cross-run Rust cache |
| `.github/workflows/images.yml` | Merge-to-main → all-or-nothing `sha-<sha>` GHCR image set → Railway **staging** deploy; same-repo PRs → `pr-<n>` images + ephemeral Railway env; `chore(release):*` pushes → release-layer retag; tag lanes → semver channel tags |
| `.github/workflows/production-deploy.yml` | Manual `workflow_dispatch` promote/rollback on `Oxidean / production` environment; promote requires SHA ∈ ancestors of `main` **and** latest CI green; optional stable cut (changelog+bump+tags via release App ruleset bypass); health probe `app.oxidean.dev/health`; GitHub Deployment records; `dry_run` mode |
| `.github/workflows/release-beta.yml`, `release.yml`, `ox-cli.yml` | `main` = beta channel (prerelease `-beta.N` tags per push); `<component>-v*` tags → GitHub Releases; `release.yml` dispatch also offers `channel: stable` on `release/*` branches (maintainer lane); `ox-v*` → cross-platform CLI binary matrix; prereleases never write `latest.json` |
| `.railway/railway.ts` + `.railway/README.md` | Railway TypeScript IaC: managed Postgres + api/web/runner/gateway, all image-sourced; per-environment channel tags (`:production`/`:staging`/`:main`); secrets via `preserve()` |
| `Makefile` (~90 targets), `checkmake.ini` | Task façade: `test`, `rpc-gen`, `rpc-sync-check`, `web-lint`, `web-format-check`, `dead-code-check`, `coverage-*`, `route-/browser-coverage-check`, `smoke*`, `up*`, `cloud-*`, `makefile-lint`; checkmake + `check-makefile.sh` enforcement |
| `docker-compose*.yml` (5 files), `deploy/`, `docker/`, `packaging/nfpm.yaml`, `Cross.toml` | Self-host stack (Postgres default; mysql/sqlite overlays; dev-auth stubs w/ Mailpit+OIDC); Traefik edge + Caddy cloud gateway; nfpm deb/rpm/apk; `cross` CLI matrix |
| `versioning.toml`, `docs/VERSIONING.md` | 10-component + virtual `wire` SemVer registry; commit grammar (`feat fix chore docs style refactor perf test build ci revert`, `release` reserved, `!`/`BREAKING CHANGE:` markers, exemptions `Merge …`/`Revert "…"`/`chore(release):`); tag prefixes; `depends_on` bump cascade |
| `docs/CODE_PRACTICES.md`, `CONTRIBUTING.md`, `.github/pull_request_template.md`, `AGENTS.md`, `.cursor/rules/` | Change-type→test minimums; monorepo boundary table; Rust style (Result+thiserror, no unwrap outside tests); Octane `.tsrx` conventions + DOM-race rules; PR checklist (tests, no secrets, env-var docs); no-AI-attribution rule; in-repo skills inventory (incl. `tdd`) |
| `.config/nextest.toml`, `apps/web/.oxlintrc.json`, `apps/web/package.json`, `knip.ts` | nextest `ci` profile; oxlint `--type-aware --deny-warnings` + oxfmt/prettier scripts; knip config with custom `.tsrx`/`.astro` compilers |
| `aidlc/spaces/default/codekb/oxidean-app/` (all 8 artifacts) | business-overview, architecture (seam map, decision IDs, 3-place edge parity), code-structure, technology-stack (versions/pins), dependencies (internal DAG + sync edges), code-quality-assessment (test inventory, gates, debt incl. ungated Rust coverage), component-inventory, api-documentation |
| `<record>/inception/reverse-engineering/{developer-scan.md,memory.md}` | Full rescan scope + recorded observations (human granted standing autonomy for this intent; Docker preferred for verification) |

## Finding → evidence map

| Practice claim | Rests on |
|---|---|
| Trunk-based, PR-only main, squash-merge | `(#NNN)` commit suffixes; empty `git log --merges`; "Main ruleset" comments in `production-deploy.yml` (release App = sole bypass); org.md default matches |
| `type/<slug>` short-lived branches, worktree-parallel work | `git worktree list` (~14 worktrees, `feat/ fix/ chore/ ci/ docs/ cursor/`); issue-numbered slugs |
| SemVer commit grammar is mandatory | `.githooks/commit-msg` + `commit-check` CLI + CI `commit-lint` (range + PR title) + `versioning.toml [grammar]` |
| Skeleton off this run | scope `classic` ⇒ `skeleton: off` (`.claude/scopes/aidlc-classic.md`); mature existing pipeline/e2e infra (code-quality-assessment); affirmed Q2 |
| test-after methodology | No red-first/TDD mandate found in docs, hooks, or CI; change-type→minimum test table (`CODE_PRACTICES.md`) and PR-template test-plan presume implementation exists first; org.md default for unaffirmed posture is also `test-after`; affirmed Q3 |
| Weighted coverage floor 0.70 (web composite) | `ci.yml` `coverage-weighted` env `COVERAGE_WEIGHTED_FLOOR: "0.70"`; `scripts/coverage-weighted.sh` composition; affirmed Q3 as the stated floor |
| Rust coverage ungated; dialect-parity CI-execution limited to probe | code-quality-assessment §Coverage Gates + debt item 10; quality review (DATABASE_URL gating, `db-dialects` probe-only) |
| nextest + Vitest + Playwright stacks | `.config/nextest.toml`, `apps/web/vitest.config.ts` (5 projects), `docs/TESTING.md`; quality review verified archive/shard/project details |
| Merge-to-main ⇒ staging auto-deploy; production = gated promote | `images.yml` push-to-main lane; `production-deploy.yml` dispatch + environment gate + CI-green precondition; devsecops verified end-to-end |
| Railway image-pull model, channel tags | `.railway/README.md` tag model; `railway.ts` channel seeding; `images.yml` publish contract |
| Beta-on-main releases; stable at promote **or** maintainer dispatch lane | `release-beta.yml` header; `production-deploy.yml` stable-cut block; `release.yml` `channel: stable` dispatch on `release/*` (devsecops correction, affirmed Q4) |
| Deploy security posture + supply-chain contract | devsecops review of all seven workflows, `scripts/railway-*.sh`, `ghcr-images.sh`, lockfiles, checksum paths; affirmed Q4 |
| Lint/format/style stack | `web-octane`/`rust-fmt`/`dead-code`/`makefile-lint` jobs; `CODE_PRACTICES.md`; `.oxlintrc.json`; `knip.ts`; devsecops + developer verified |
| Layer model, three error idioms, RPC conventions, naming taxonomy, trait+adapter + drift-gate idioms | developer review of `oxidean-api/src/{app,rpc,main}.rs`, `rest/mod.rs`, `auth/`, `repo/mod.rs`, `oxidean-core`, `oxidean-db`, `oxidean-git`; affirmed Q6 |

## Support reviews

Three blind reviews ran in parallel; each saw only the lead draft plus
evidence paths. Every OBJECT was resolved in the integrated artifacts —
all adopted.

- **aidlc-quality-agent** (`contributions/aidlc-quality-agent.md`) —
  verified every Testing Posture claim (test-after is real; weighted-gate
  weights verbatim in `coverage-weighted.sh`; manifest checkers
  source-verify mount/interact/assert markers; shared nextest archive;
  Vitest project topology; real-deps-over-mocks; zero-skip discipline).
  OBJECTs adopted in `## Testing Posture`: (1) dialect-parity claim
  corrected — `dialect_*` files are `DATABASE_URL`-gated and skip-pass in
  CI, `dialect_probe` is the only enforced cross-dialect run; (2) the e2e
  third of the composite is a 7-path presence checklist and the composite
  consumes web numbers only — both now stated; (3) the new VCS crate lands
  with no enforced coverage floor — now flagged in the Coverage bullet.
  Additional findings adopted: no test-placement gate, no perf-test
  infrastructure, local/CI `make test` asymmetry, org-80%-line vs
  repo-0.70-weighted unit mismatch (resolved Q3 in the repo's favor).
- **aidlc-developer-agent** (`contributions/aidlc-developer-agent.md`) —
  verified the Rust/web gate and monorepo-convention claims against
  `Makefile` targets and config files. OBJECTs adopted via a rewritten
  `## Code Style` (affirmed Q6): (1) "prefer `Result` + `thiserror`"
  corrected to three error idioms by layer; (2) the real layer model added
  (edge → `RpcCtx`/dispatch → domain modules → `Database` façade +
  `GitBackend`; core = types only; no `services/` layer); (3) RPC naming
  and registration conventions added — including the snake/camel drift and
  `domain.snake_case` for new names with existing names frozen wire-stable;
  (4) naming/module taxonomy added (`mod.rs` façades, suffix taxonomy,
  kebab-case `.tsrx`, `*-queries.ts`, test-suffix taxonomy); (5)
  trait+adapter seam and self-checking drift-gate idioms added as the
  shape the new crate follows. Its design-level gaps are routed to
  `## Deferred to later stages` below.
- **aidlc-devsecops-agent** (`contributions/aidlc-devsecops-agent.md`) —
  verified deploy mechanics end-to-end against all seven workflows (noting
  the dispatch-named `security.yml`, `e2e-production.yml`,
  `binary-e2e.yml`, `frontend-deploy.yml`, `sdk-publish.yml` do not exist),
  plus lint/format gates and the secrets posture (`preserve()` IaC vars,
  local-only compose defaults, scoped CI/deploy credentials). OBJECTs
  adopted: (1) "stable cuts happen only inside production promote"
  corrected — `release.yml` dispatch offers `channel: stable` on
  `release/*` branches; (2) the security-posture and supply-chain
  paragraphs added to `## Deployment` (suggested wording adopted); (3)
  scanning absence now stated as acknowledged status quo and recorded
  debt; (4) the four deploy rules promoted to `## Mandated` (Q5); (5)
  `install.sh` fail-open checksum and (minor) root containers/`docker.sock`
  recorded under `## Recorded Debt`. Its remaining human-level questions
  are carried under `## Uncertainties explicitly not resolved`.

## Interview resolutions

All six questions were answered under the standing continuation
instruction ("skip human verification until we have a sound implementation
on a PR with green CI and reply to questions … through recommendation") —
conductor decisions recorded 2026-10-18, ratify at PR review.

| # | Question | Answer | Rationale / where encoded |
|---|---|---|---|
| Q1 | Way of working affirmed? | **A — affirmed as discovered.** Trunk-based `main`, PR-only squash-merge, `type/<slug>` branches (+ optional issue) for human/agent work; Bolt worktrees keep engine `bolt-<id8>_<slug>` naming; `relaxed` guard policy stands | Every claim rests on in-repo evidence; GitHub-side ruleset details corroborated by workflow comments + release history. Encoded in `## Way of Working` (Bolt-naming + guard-policy sentences added) |
| Q2 | Walking skeleton? | **A — skeleton off** with the slice-first sequencing note carried to delivery-planning | The ceremony's value (a human checkpoint proving pieces connect) is redundant with standing autonomy + the green-CI requirement; the sequencing wisdom survives as a normal unit-ordering decision. Encoded in `## Walking Skeleton` |
| Q3 | Testing posture / coverage floor? | **A — affirm repo reality.** `test-after`; implement-then-test per change-type with full CI green; floor = the enforced 0.70 weighted composite (web); new crate ships tests per CODE_PRACTICES minimums but no new numeric Rust floor; Rust collect-only and perf-absence recorded as debt | An 80% line floor with no measuring infrastructure is unenforceable noise; the honest record is "0.70 weighted on web + Rust collect-only". Encoded in `## Testing Posture` |
| Q4 | Deployment model + security posture affirmed? | **A — affirmed with the correction and additions.** Staging-on-merge + gated manual promote (ancestor-of-main + latest-CI-green, dry-run/rollback); `release.yml` maintainer stable-cut lane acknowledged; token-scoping/fork-isolation/checksum contract added; scanning absence recorded as accepted status quo | Recording "no scanning" as an acknowledged posture is strictly better than silence; creates no new gates and can be revisited as its own intent. Encoded in `## Deployment` (three paragraphs) |
| Q5 | Which candidate constraints become hard rules? | **A — promote the enforced + stated ones, record the rest as debt.** Mandated gained the four deploy rules (frozen-lockfile/`npm ci` installs; all-or-nothing `sha-<sha>` deploys; `railway config apply` never on CI; promote = ancestor-of-main + latest-CI-green), the env-var documentation rule, the transport-auth rule (PAT/pubkey-class, never session cookies — D-12), and the release-push rule (never `GITHUB_TOKEN` — FR3.4). Forbidden unchanged | A hard rule doesn't need a CI gate to be hard; the PR checklist and recorded decision precedents (D-12, FR3.4) are already the team's stated constraints, and this intent's new transport/config surfaces make them directly load-bearing. Encoded in `discovered-rules.md` |
| Q6 | Developer-corrected Code Style affirmed? | **A — affirmed.** Layer model stated; three error idioms recorded by layer; RPC conventions fixed (`vcs.*` names snake_case; existing camelCase frozen wire-stable); naming taxonomy + `mod.rs` façade convention; trait+adapter seam as the new crate's shape; every new duplicated/generated surface ships its own sync check; new crate registers a `versioning.toml` component/scope | The corrected section is what the code actually does; the draft's single-idiom error claim would have generated wrong code in the new crate. Encoded in `## Code Style` |

## Uncertainties explicitly not resolved

- Fork-PR external contributor flow (evidence shows same-repo assumptions;
  fork lanes get reduced gates — publish-free affirmed as structural, but
  the contributor *experience* is undocumented).
- Actual merge-button mechanics (squash inferred from history; GitHub
  settings not visible in-repo).
- Release cadence *timing* (beta is per-push; stable is operator-chosen —
  no schedule evidence).
- **Scanning absence** — no SAST/DAST, dependency-vulnerability, or secret
  scanning, and no update bot: affirmed as acknowledged status quo, but
  whether it is deliberate policy or a gap to close (cheapest first step:
  scheduled `cargo audit` + GitHub secret scanning/push protection) is
  unresolved.
- **`install.sh` fail-open checksum** — keep for old-POSIX tolerance or
  make fail-closed like the image/boot paths; recorded debt, undecided.
- **Digest-pinning granularity** — Docker base images and GitHub Actions
  stay tag-pinned (`oxidean-ci:*` is the only digest-resolved surface);
  whether to move to digest pinning is unresolved. Related: docker CLI /
  compose / buildx downloads are version-pinned but not checksum-verified.
- **GitHub-side settings invisibility** — secret scanning / push
  protection, Dependabot alerts, `*-v*` tag rulesets, required reviewers
  on the `Oxidean / production` environment (if unset, the "manual gate"
  is single-operator), and the exact `Main` ruleset config are all
  corroborated by comments/history but not visible in-repo.
- **`RELEASE_TOKEN` tenure** — accepted long-lived maintainer PAT vs
  migrating the beta/dispatch lanes to the release App (short-lived
  installation tokens); undecided.
- **Artifact assurance ceiling** — sha256 checksums are the whole
  integrity contract today (no signing/attestation/SBOM); whether `ox`
  releases gain signatures for a product that is itself a forge is
  undecided.
- **Performance-tooling absence** — no criterion benches, no load/soak
  harness anywhere; whether this intent must bootstrap any for protocol
  NFRs (clone/fetch latency, throughput) is deferred to NFR work.
- **Dialect-parity CI-execution gap** — only `dialect_probe` is enforced
  in CI; the rest of the `dialect_*` suite needs a local `DATABASE_URL`.
  A new VCS crate with schema changes inherits a parity gate that mostly
  doesn't execute; whether to close the gap for it is unresolved.
- **Test-placement gate absence** — no `check-test-placement.sh`; misnamed
  `*.spec.ts`-style files silently never run (knip catches them only as
  dead code).

## Deferred to later stages

Design-level gaps the developer review surfaced — solution-level decisions
for **requirements-analysis / domain-design**, not team practices (per the
learned project rule and the interview file's preamble):

- **Crate topology** — the intent wants server+cli+mcp logic in one crate,
  deviating from one-role-per-crate (`api`/`cli`/`runner`/`web` separate,
  MCP inside api). Confirm the deviation and the integration shape: new
  arms in the single `rpc.rs` `match` dispatch, or a namespaced
  sub-dispatch.
- **New-crate error type** — `thiserror` enum (documented preference +
  `GitError` precedent) vs `Result<T, String>` (`oxidean-db` parity); and
  whether its persistence goes through `oxidean-db` (keeps the ×3 dialect
  seam) or a crate-local store (breaks dialect parity).
- **Decision-ID prefix** — the ID namespace for the new work (e.g.
  `VCS-*`) plus its `docs/ARCHITECTURE.md` entry; the citation convention
  expects registered IDs.
- **New-crate dependency bar** — "prefer crates already in the workspace"
  needs a stated threshold; the VCS domain may tempt new deps (e.g. gix).
- **`too_many_arguments` policy** — keep accepting
  `#[allow(clippy::too_many_arguments)]` on wide functions, or require
  params structs in new code.
