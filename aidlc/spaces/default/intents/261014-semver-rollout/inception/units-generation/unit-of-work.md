# Units of Work — semver-release-automation

Sources: `inception/domain-design/components.md`, `inception/domain-design/decisions.md`, `inception/requirements-analysis/requirements.md`, `ideation/scope-definition/intent-backlog.md`. Decomposition approved per `units-generation-questions.md` (cohesion seams → 7 bolt-sized units).

## Unit Definitions

| Unit ID | Directory | Name | Kind | Complexity | Deployment |
|---|---|---|---|---|---|
| U1 | `u1-version-registry` | Version Registry | library | M | embedded |
| U2 | `u2-commit-grammar` | Commit Grammar | library | M | embedded |
| U3 | `u3-change-detection` | Change Detection & Bump Engine | library | L | embedded |
| U4 | `u4-changelog-release` | Changelog & Release Orchestration | packaging | L | embedded |
| U5 | `u5-commit-lint-gates` | Commit Lint Gates (hook + CI) | packaging | M | embedded |
| U6 | `u6-version-surfaces` | Version Surface Wiring | library | L | embedded |
| U7 | `u7-contributor-guidance` | Contributor Guidance | | S | embedded |

All units land on one branch and merge as a single PR — `embedded` deployment throughout; nothing deploys standalone.

> **ID disambiguation:** these `U{n}` ids are new identifiers — they do not correspond positionally to `intent-backlog.md`'s proto-units U1–U14. Mapping: proto U1→U1; U2→U2; U3+U4→U3; U5+U6+U12+U14→U4; U7+U8+U11→U5; U9→U6; U10→U7; U13→U3.

## U1 — Version Registry

**Components:** VersionRegistry (domain-design).

**Responsibilities:**
- Author `versioning.toml` — ten `[[component]]` entries (9 manifests + wire), each with `id`, `kind`, `manifest`/`constant` path, `tag_prefix`, `changelog` path, detection `globs`, scope `aliases`, internal `depends_on` edges; plus `[baseline]` commit, allowed types, reserved scopes. Metadata only — no version numbers (ADR-001).
- Migrate manifests to literal versions: remove `version.workspace = true` from all 7 crates, remove `version` from `[workspace.package]`, set each crate/package to literal `0.1.0`. The wire entry's registry row points at the constant path, but the `WIRE_VERSION` constant itself — including its initial `0.1.0` — is created by U6; U1 touches no Rust source.
- Implement the registry loader + validator inside `scripts/versioning/` — the `check` subcommand: schema validation, cycle rejection in `depends_on`, and **registry completeness** (every Cargo workspace member + every Bun workspace package maps to exactly one entry; fails otherwise).

**Boundaries:** owns the registry file format and the manifest version fields; owns no commit parsing or bump logic.

**Notes:** the completeness check is the FR2.2 guarantee that future components can't evade versioning — it must run standalone (CI calls it in U5's lint job).

## U2 — Commit Grammar

**Components:** CommitGrammar (domain-design).

**Responsibilities:**
- Conventional-commit parser in `scripts/versioning/`: `type(scope)!: subject`; types `feat, fix, chore, docs, style, refactor, perf, test, build, ci, revert`; breaking via `!` or `BREAKING CHANGE:` footer.
- Scope resolution signal: mapped scope → component ids (registry aliases); unmapped → warn + path-fallback signal; reserved `release` scope → maps to nothing, never falls back.
- Exempt-subject detection: merge commits, auto-generated revert subjects, `chore(release):` tooling commits — exempt commits carry no bump signal (FR1.3).
- `commit-check` subcommand: validate one message (hook input) or a range/PR title — shared by U5's hook and CI job so local and CI can never disagree (ADR-005/Q7).

**Boundaries:** pure parse + validate; no filesystem/git history access (ChangeDetector feeds it commit data).

## U3 — Change Detection & Bump Engine

**Components:** ChangeDetector, DependencyGraph, BumpPlanner, ManifestWriter (domain-design).

**Responsibilities:**
- `plan` subcommand (first of U3's two CLI surfaces — `bump` is the other): resolve each entry's commit range (last `<component>-v*` tag or baseline commit → HEAD); parse via U2; attribute via scope-map first, path globs fallback; dedupe per entry; wire entry uses scope-driven detection over its curated contract glob list (FR2.3).
- DependencyGraph: DAG from registry `depends_on` edges (Cargo path deps, Bun workspace deps, `oxidean-api`→`@oxidean/api-client` generated-source edge, wire dependents); transitive closure of dependents.
- BumpPlanner: 0.x-aware bump table (FR3.1); propagation rule — propagated-only = patch, direct+deps = max(direct, patch), direct-only = direct incl. none (FR3.2); prerelease suffix computation `-alpha.N`/`-beta.N`/`-nightly.YYYYMMDDN` when requested (FR8.3); emits the release plan.
- ManifestWriter + `bump` subcommand (U3's second surface alongside `plan`): rewrite only registered manifest `version` fields + the wire constant (FR3.3); regenerate `Cargo.lock`/`bun.lock` via native toolchains; deterministic output (NFR2).

**Boundaries:** produces a plan + edited manifests; does not create tags, changelogs, or releases.

## U4 — Changelog & Release Orchestration

**Components:** ChangelogWriter, ReleaseOrchestrator (domain-design).

**Responsibilities:**
- `changelog` subcommand: Keep a Changelog per component (grouped Features/Fixes/Performance/Breaking with commit refs), `docs/WIRE-CHANGELOG.md` for the wire entry, consolidated root `CHANGELOG.md` linking inner files (FR4, FR4.1); idempotent regeneration (FR4.2); `### Dependencies` section on propagation-only bumps (FR4.4).
- `tag` subcommand + `make release-plan`/`make release`/`make release-tags` targets: `<component>-v<semver>` annotated tags (ADR-002); baseline `<component>-v0.1.0` set on the recorded baseline commit — the maintainer push is the 0.1.0 cut (FR2.5); `chore(release):` tooling commits; never default `GITHUB_TOKEN` for cascades (FR3.4); signed-tag config supported (FR8.4).
- Release-workflow alignment: `ox-cli.yml` → `ox-v*.*.*` trigger, `crates/oxidean-core/**` + all CLI internal deps added to path filters, `ox-v` prefix stripping for version extraction, changelog-derived notes instead of `--generate-notes` (FR5.1, FR5.6); `publish-runner.yml` → `oxidean-runner-v*.*.*` trigger with bare-version image tags (FR5.2, FR5.6); generic `*-v*.*.*` component release job creating GitHub Releases per tag (excluding `ox-v*`), incl. runner notes+image release and wire lane (FR5.4, FR5.5); automated release-PR workflow on the same plan path (FR8.2).

**Boundaries:** consumes U3's plan; owns tags, notes, changelogs, and workflow YAML — not detection logic.

## U5 — Commit Lint Gates

**Components:** CommitLintGate (domain-design).

**Responsibilities:**
- Tracked `.githooks/commit-msg` calling U2's `commit-check` (advisory, bypassable by design); `make setup` sets `core.hooksPath` idempotently + documents it (FR1.1, ADR-008).
- CI job linting every PR-range commit **and** the PR title (FR1.2 + FR8.1), plus U1's registry completeness check in the same gate (FR2.2); documented maintainer step to mark the check required.

**Boundaries:** enforcement surfaces only; grammar lives in U2, registry in U1.

## U6 — Version Surface Wiring

**Components:** VersionSurfaceWiring (domain-design).

**Responsibilities:**
- `WIRE_VERSION: &str` in `oxidean-core` replacing `RPC_PROTOCOL_VERSION`; emitted into `@oxidean/api-client` via rpc-gen; caret-style compat predicate (`<1.0` same `0.MINOR`; `≥1.0` same `MAJOR`) at the server `Oxidean-RPC-Version` check and in the generated client warning path (ADR-011); `MIN_CLI_VERSION` stays a distinct floor.
- Surfaces: `system.manifest` reports `wire_version` + api component version + floor; `ox --version`/User-Agent/MCP `clientInfo`/`serverInfo`/self-update read their own component versions (FR6.1–FR6.2); `/cli/latest` reads `latest.json` written by the `ox-v*` release flow (ADR-007); OpenAPI `info.version` renders `WIRE_VERSION` (ADR-011); MCP spec-revision pins deduped to a canonical `MCP_PROTOCOL_VERSIONS` in `oxidean-core` propagated to TS via rpc-gen (ADR-010); dead `OXIDEAN_RPC_VERSION` removed from `.env.example`, compose, docs (FR6.5).

**Boundaries:** code rewiring inside existing crates/apps; registry identity of the wire entry comes from U1.

## U7 — Contributor Guidance

**Components:** ContributorGuidance (domain-design).

**Responsibilities:**
- CONTRIBUTING + `docs/`: commit grammar, bump mapping, `make setup` hook install, release flow (FR7.1).
- `.agents/skills/semver-release/` skill (surfaced via existing `.claude/skills` symlinks) so agents write conforming commits and run the tooling; `AGENTS.md` updated (FR7.2).
- `Makefile` help covers new targets (FR7.3).

**Boundaries:** documentation only — describes U2's grammar, U4's release flow, U5's gates.
