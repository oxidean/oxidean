# Project-Level Rules

> Project-specific specialisation and corrections. Loaded after `org.md` and
> `team.md` as strict-additive guidance; contradictions with broader policy
> are rejected. Populated by practices-discovery and the self-learning loop.
>
> Use sparingly: most teams don't need a project layer. Reach for it
> only when this specific project needs stable, durable guidance beyond the
> team practice (for example, package-specific release checks or an additional
> regression suite for a legacy component).

## Way of Working

<!-- Project-specific specialisation. Example: -->
<!-- This monorepo requires package-scoped branch names and a package owner -->
<!-- review in addition to the team's normal merge policy. -->

## Walking Skeleton

<!-- Project-specific specialisation. Example: -->
<!-- The walking skeleton must exercise the legacy service adapter as well -->
<!-- as the new service boundary. -->

## Testing Posture

<!-- Project-specific specialisation. -->

- CI/Docker-related changes are verified locally with docker + act before shipping; deadlines are ASAP-but-quality-gated — solid tests are required, not optional (learned 2026-10-14) <!-- cid:261014-semver-rollout:scope-definition:e0b8c754d11a5bb705baa2e6ade501fd1cf383cb6451fe582dcfeeb0a7f41278 -->

- Prefer Docker for debugging and testing (docker compose stack, make up/smoke, act for CI lanes) over bare-metal equivalents (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:reverse-engineering:8508fcf430cb00d7d1bfb8bd9281251b6f46fbfb684b492f7ce6ac26e1f56a52 -->

## Guard Policy

<!-- Project-specific. Mode: strict, relaxed, or off. Strict here holds for every intent and cannot be changed from chat. A section under the retired Change Control heading, written by an earlier release, is still read. -->

## Deployment

<!-- Project-specific specialisation. -->

## Code Style

<!-- Project-specific specialisation. -->

## Tech Stack

<!-- Technology choices locked for this project. -->

## Decided

<!-- Decisions made in earlier stages that should not be re-asked. -->
<!-- Format: DECIDED: [decision] (Stage [slug], [date]) -->

## Scope Overrides

<!-- Custom scope rules for this project. -->

## Forbidden

<!-- Populated by practices-discovery affirmation gate. -->
<!-- Format: NEVER [behavior] (affirmed [date]) -->
<!-- Example: NEVER throw exceptions across service layer boundaries (affirmed 2026-05-17) -->

- NEVER commit secrets — no `.env`, tokens, or private keys in the repo, CI config/logs, or docs examples; examples live in `.env.example` / `docs/dev-auth.env.example` (AGENTS.md hard boundary, PR checklist). (affirmed 2026-10-18)

- NEVER hand-edit generated sources as if they were the source of truth — `packages/api-client/src/index.ts`, `docs/openapi.yaml`; change the Rust producers and regenerate. (affirmed 2026-10-18)

- NEVER add AI or tooling attribution (`Co-authored-by`, `Generated with`, `Made-with`, bot generator signatures) to commits or PRs — `.cursor/rules/no-ai-attribution.mdc`. (affirmed 2026-10-18)

- NEVER reintroduce `Swatinem/rust-cache` or per-job Rust toolchain installs into `ci.yml` — the digest-pinned `oxidean-ci:*` images are the dep cache by design (AGENTS.md / ci.yml header). (affirmed 2026-10-18)

## Mandated

<!-- Populated by practices-discovery affirmation gate. -->
<!-- Format: ALWAYS [behavior] (affirmed [date]) -->
<!-- Example: ALWAYS use Result<T,E> for fallible operations in service layer (affirmed 2026-05-17) -->

- ALWAYS write every commit subject in the SemVer grammar `type(optional-scope)[!]: subject` (allowed types in `versioning.toml` `[grammar]`; `release` scope reserved) — enforced by `.githooks/commit-msg` (`make setup`) and CI `commit-lint` on the PR range and PR title; subjects drive per-component version bumps and changelogs. (affirmed 2026-10-18)

- ALWAYS regenerate generated artifacts from their producers and commit the regenerated output in the same change — `make rpc-gen` after RPC/type changes (`packages/api-client`), `make openapi-gen` (`docs/openapi.yaml`), and the skill/route/stack-preset/workflow-template sync targets; CI `rpc-sync` runs each check with `git diff --exit-code` semantics. (affirmed 2026-10-18)

- ALWAYS keep dialect-specific SQL inside `crates/oxidean-db`; API, web, and any new crate must not branch on DB dialect (migrations are written ×3 in `migrations/{postgres,mysql,sqlite}` with filename-parity tests). (affirmed 2026-10-18)

- ALWAYS reach `main` through a pull request — the `Main` ruleset admits only PR merges; the sole bypass actor is the release App for the stable-cut `chore(release):` commit. (affirmed 2026-10-18)

- ALWAYS pass `make web-lint` and `make web-format-check` before merging `apps/web` changes; CI `web-octane` gates both (type-aware oxlint is the web type gate — plain `tsc` does not parse `.tsrx`). (affirmed 2026-10-18)

- ALWAYS declare new Makefile targets `.PHONY` with a runnable recipe under `scripts/` paths that exist — `scripts/check-makefile.sh` and the CI `makefile-lint` job enforce it. (affirmed 2026-10-18)

- ALWAYS register coverage evidence for surfaces the manifests govern: every new/changed `apps/web` route needs a route-coverage row, and new/changed high-risk interactive UI needs a `*.browser.test.tsx` or documented stack-browser evidence — the change-aware `route-coverage` and `browser-coverage` CI jobs fail on fresh skip-only rows. (affirmed 2026-10-18)

- ALWAYS install JS dependencies via `bun install --frozen-lockfile` (workspace) or `npm ci` (`.railway/`) — the committed lockfiles are the dependency contract (promoted at interview Q5; enforced across `ci.yml`/`ci-base` and `.railway/.npmrc`). (affirmed 2026-10-18)

- ALWAYS deploy the all-or-nothing `sha-<sha>` image set; channel tags (`:staging`/`:production`/`:main`) move only via deploy tooling after a green deploy — `images.yml`/`production-deploy.yml` run `cancel-in-progress: false` so a tag set is never half-published (promoted at interview Q5). (affirmed 2026-10-18)

- ALWAYS keep `railway config apply` off CI and IaC-apply credentials out of repo/CI secrets — `scripts/railway-apply.sh` refuses to run under CI and apply tokens never enter the repo or CI (promoted at interview Q5). (affirmed 2026-10-18)

- ALWAYS require a production promote SHA to be an ancestor of `main` with the latest CI run green; rehearse changes with `dry_run` before mutating (promoted at interview Q5; enforced by `production-deploy.yml`). (affirmed 2026-10-18)

- ALWAYS document every new `OXIDEAN_*` env var in `.env.example` and `docs/CONFIGURATION.md` in the same PR that introduces it — PR-checklist requirement, affirmed hard rule for this intent's new crate config (promoted at interview Q5). (affirmed 2026-10-18)

- ALWAYS authenticate protocol/machine transports with PAT/pubkey-class credentials — session cookies are never accepted on transport surfaces (D-12 precedent: git transports accept Basic-PAT/pubkey only; load-bearing for this intent's new VCS transport; promoted at interview Q5). (affirmed 2026-10-18)

- ALWAYS use the `oxidean-release` GitHub App (stable cuts) or `RELEASE_TOKEN` (beta/dispatch tag lanes) for release-automation pushes — never `GITHUB_TOKEN`, which cannot re-trigger tag lanes (FR3.4; promoted at interview Q5). (affirmed 2026-10-18)

## Corrections

<!-- Project-specific corrections from human feedback. -->
<!-- Format: NEVER/ALWAYS [behavior] (learned [date]) -->
- Treat "uniform versioning" as one semver practice with per-component version numbers (all starting at the same baseline, e.g. 0.1.0), where a component bumps only when changes hit it and a shared-dependency change counts as a hit on dependents — confirm with the user rather than assuming a single shared version (learned 2026-10-14) <!-- cid:261014-semver-rollout:intent-capture:cc8a97843155c5e0ae79fed94a02343bfc7e1c52ce07a38c78f9719c2aa647a3 -->
- Prefer a two-tier changelog layout — per-application/crate changelogs plus one global consolidated changelog that references the inner ones (learned 2026-10-14) <!-- cid:261014-semver-rollout:intent-capture:4a87e98964b8943cb05393e23515845e008ca70aeac02b6ca7acb8989e131da1 -->
- Keep solution-level questions (automation tool choice, commit-prefix grammar, hook distribution, CI enforcement depth) out of ideation stages — they land in requirements-analysis and domain-design (learned 2026-10-14) <!-- cid:261014-semver-rollout:intent-capture:5095a3d3c9d2434b11a71f93d28308f5d13546e9b6e3aff0d44cec4ab28552c6 -->
- Every Makefile target must be .PHONY-declared with a runnable recipe; scripts/check-makefile.sh enforces .PHONY coverage, non-empty recipes, and scripts/ path validity — keep new targets compliant (learned 2026-10-14) <!-- cid:261014-semver-rollout:scope-definition:d266bb7ec53daccdb6e215eafeab3cbb51b9c2626d98510329e70d8f73b4691d -->
- ALWAYS write ensemble-spoke contribution files verbatim at the orchestrator when a dispatched agent reports it cannot write — preserving the `**Collaborator:** <slug>` first line the engine checks as completion evidence (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:practices-discovery:40dc2e892e1ed031326bea2ba5076ad419c40050ea1cf63ac04ab2e673a902c8 -->
- When adding a new API surface, prefer paths under the existing `/api/` prefix — the Traefik, Caddy, and oxidean-web proxy-table entries already cover it, so a top-level namespace costs three-layer edge-parity work for no benefit (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:requirements-analysis:87758defc0ff33f98b1ebb129dde1e8be69ab1b6253b85ba556b0d150c2d14fa -->
- review markdown files must render `## Review` + `**Verdict:**`/`### Findings` table with `R-NN` finding ids; the engine rejects non-`R-` ids and any H1/H2 heading after the verdict (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:user-stories:36f65147861b900d325c5e123357a25664d6b3be68f8d7d31551104a527a3a99 -->
- keep story ids stable during mob integration — fold demoted/merged stories into group notes rather than renumbering, so AC references never dangle (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:user-stories:8ba2c6154c0c79d2223bc04a61fbe8ef63f20913cc339f5531dcb7d8862850cc -->
- For machine-facing features (CLI/MCP/RPC), render refined mockups as annotated terminal/envelope transcripts plus component specs for any web controls — ASCII flows carry developer-experience design better than visual comps. (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:refined-mockups:9ae3fd5116e54dbed977c0ca02b3c7b973b13ac8a0b0648ce87772efaa8334c5 -->
- CLI RPC output rides Out::Rpc {envelope, pretty} — the variant that prints envelopes verbatim under --json and drives rpc::error_exit_code. Out::Report is local-only and always exits 0; anchoring new RPC-backed commands on it makes every failure exit 0. (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:refined-mockups:0441b1623c621904c1efffbb19a342b7cc11f54a4fb642f1be674f6412a52cc3 -->
- Repo-targeted RPC args and MCP tool schemas use separate {owner, name} fields; a combined "repo":"owner/name" string does not exist on the wire and would fail-open fg_repo_access_check (it extracts owner/name and returns Ok when absent). (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:refined-mockups:c23feac42887c25f471973302a2581cb5b1072a87872ced0f5dc9cf161070df8 -->
- Mixed-media object routes (bytes on success, JSON envelope on failure) return real HTTP statuses — 404/403/413/401 with a typed error body; the typed JSON-RPC surface keeps the single-200 + {ok:false} envelope convention. (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:refined-mockups:6434c5be8e99f64f6de7a8d717db8ff0f65439ba26e72ba7b387d9b347d552bc -->
- Per-repo feature flags have two distinct denial scopes: an absent flag row (never-enabled) denies the surface entirely (including status reads); an enabled=false row denies writes only — reads still succeed. Flattening both to deny-all breaks 'disabled but readable' lifecycles. (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:domain-design:88d59956c8b4f92f99f1acb6d2b5bcbc826921ffa901701287a58f668d2b6a52 -->
- Content-addressed stores use a GLOBAL entry row keyed by hash plus per-repo link rows (repo_id + hash) inserted in the referencing transaction; referenced-ness is computed at GC time by joining links — never a stored refcount counter, which drifts under cascade deletes. Also clone LFS's GC gaps closed: temp→rename→row order, row-less-file/tmp sweep, boot-time lock clear. (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:domain-design:eb3170ef0c191d30b71a6ecec8bfcc395b92ebdc2b190ecc39da3d78a629c440 -->
- When one crate must serve both the lean CLI and the heavy server, ship a cargo feature split (client default = no DB/git deps; server = stores/persistence/interop) rather than unconditional deps — otherwise ox transitively compiles sqlx×3 + oxidean-git. (learned 2026-10-18) <!-- cid:261018-agentic-vcs-protocol:domain-design:49f0deed5ace494ceb99aaca0b83af2e23398736e4102949c1361b774883c44d -->
