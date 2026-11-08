# Requirements Analysis — SemVer Release Automation

Sources: `ideation/intent-capture/intent-statement.md`, `ideation/scope-definition/scope-document.md`, `ideation/scope-definition/intent-backlog.md`, `codekb/oxidean-app/business-overview.md`, `codekb/oxidean-app/architecture.md`, `codekb/oxidean-app/code-structure.md`, `codekb/oxidean-app/component-inventory.md`, `codekb/oxidean-app/api-documentation.md`, `codekb/oxidean-app/dependencies.md`, and the ten confirmed answers in `requirements-analysis-questions.md` (conductor-decided under the user's standing continuation instruction; each is reversible at PR review).

## Intent Analysis

Oxidean is about to have outside users, and a named future issue depends on versioning existing. The goal is not "a version number" but a trustworthy release pipeline: every workspace member carries its own SemVer from `0.1.0`, bumps only when real changes (or dependency changes) hit it, produces a per-component changelog plus one consolidated global changelog, reports its version consistently on every consumer-facing surface (CLI `--version`, MCP `clientInfo`/`serverInfo`, `system.manifest`, `system.health`, `/cli/latest`, generated API client), and is enforced by a local commit-msg hook plus non-bypassable CI linting — so that admins, contributors, and downstream CLI/MCP users can trust and act on version numbers.

## Functional Requirements

### FR1 — Commit grammar and enforcement

**FR1.** The repository adopts Conventional Commits as the commit contract: `type(scope)!: subject` with types `feat, fix, chore, docs, style, refactor, perf, test, build, ci, revert`; `scope` is optional and resolves to component ids/aliases in the version registry; `!` on the type or a `BREAKING CHANGE:` footer marks a breaking change.

- **FR1.1.** A tracked `.githooks/commit-msg` hook validates the grammar locally. A `make setup` target sets `core.hooksPath` to `.githooks` idempotently; hook installation is documented in CONTRIBUTING and agent-facing docs. The hook is advisory ergonomics — bypassable by design.
- **FR1.2.** A required CI check lints every commit message in the PR range **and** the PR title (covers merge-commit and squash-merge flows); failures block merge. The workflow ships the check; making it a *required status check* in repo settings is a documented maintainer step. This is the non-bypassable enforcement.
- **FR1.3.** Exempt subjects: merge commits, auto-generated revert subjects, and tooling-authored release commits (which use `chore(release): …`). Exempt commits carry no bump signal — in a merge-commit flow the constituent prefixed commits carry the signals; in a squash flow the squashed commit's type carries it. Exemptions are explicit and tested.

### FR2 — Versioned component set and source of truth

**FR2.** Every Cargo crate and every Bun workspace member is an independently versioned component, each starting at `0.1.0`. The versioned set is **nine manifest components** — `oxidean-api`, `oxidean-cli` (binary `ox`), `oxidean-core`, `oxidean-db`, `oxidean-git`, `oxidean-runner`, `oxidean-web`, `@oxidean/web`, `@oxidean/api-client` — **plus one registry entry without a manifest**: the API/RPC wire contract (a SemVer constant in code). `rpc-gen`, `openapi-gen`, and `oxidean-protection-hook` are `[[bin]]` targets inside `crates/oxidean-api` — they inherit the `oxidean-api` component version, not their own.

- **FR2.1.** The authoritative version of each component is the literal `version` field in its own manifest (`Cargo.toml`, `package.json`). `version.workspace = true` inheritance is removed from all crates, and the `version` key is removed from `[workspace.package]` entirely (edition/license inheritance stays).
- **FR2.2.** A root registry file `versioning.toml` declares per component: id, manifest path (or constant path for the wire entry), change-detection globs, scope aliases, tag name, changelog path, and internal dependency edges. It never duplicates version numbers. The tool validates **registry completeness**: every Cargo workspace member and Bun workspace package maps to exactly one registry entry — unregistered members fail the check (wired into the same CI gate as the commit lint), so a future component can't silently evade versioning.
- **FR2.3.** The API/RPC wire contract carries its own SemVer constant (starting `0.1.0`), owned in `oxidean-core`, registered as the tenth entry with tag name `wire-v<semver>`. Its dependents in the registry DAG are `oxidean-api`, `oxidean-cli`, `@oxidean/api-client`, `oxidean-web`, and `@oxidean/web` (the last two host MCP pin-site consumers of the contract) — a wire bump propagates to them by the same FR3.2 rule. Its change detection is **scope-driven** (commits under the `wire` scope alias, plus breaking markers on contract-touching commits) over a curated glob list of contract-defining files pinned in the registry — it does not blanket-cover `rpc.rs` (a ~285-arm monolith mixing contract and business logic), avoiding spurious wire bumps.
- **FR2.4.** Non-component `version` fields are never rewritten: stack-preset/scaffold templates, test fixtures, and lock-file schema markers (e.g. `skills-lock.json`'s `"version": 1`). The Bun workspace root package `oxidean` is private and ships nothing — it stays versionless by policy.
- **FR2.5.** The `0.1.0` baseline is operationalized: the registry records the baseline commit, and `make release-tags` creates and pushes the initial `<component>-v0.1.0` annotated tag set on it after the versioning change merges. That maintainer push is the **0.1.0 release cut** — it fires each component's release workflow intentionally, producing the first SemVer release set. Pushing the baseline tags is a documented maintainer action, not a silent side effect of merging.

### FR3 — Dependency-aware change detection and bumping

**FR3.** A release computation resolves, per component, the conventional commits since its previous `<component>-v*` tag (or the `0.1.0` baseline), determines the affected-component set from scope-field → registry-map resolution with changed-file path globs as fallback, then propagates every hit transitively through the internal dependency DAG (Cargo path deps and Bun workspace deps, including the generated-source edge `oxidean-api` → `@oxidean/api-client`). Scope resolution rules: a mapped scope names its component set; an **unmapped** scope logs a warning and falls back to path detection; the reserved scope `release` (tooling-authored `chore(release):` commits) never maps to a component and never falls back.

- **FR3.1.** Bump table (0.x-aware): while a component is `<1.0.0` — breaking or `feat` → minor; `fix`/`perf`/`revert` → patch; `chore`/`docs`/`style`/`refactor`/`test`/`build`/`ci` → no bump. At `≥1.0.0`: breaking → major, `feat` → minor, `fix`/`perf`/`revert` → patch.
- **FR3.2.** Dependency propagation: a component with no direct commits but a bumped internal dependency receives a patch bump (the dependency-bump severity does not escalate the dependent past patch). A component with direct commits whose dependencies also bumped receives `max(direct bump, patch)`; a component with direct commits only receives its direct bump (including none).
- **FR3.3.** The bump tool rewrites only the `version` fields of in-registry manifests and the wire-version constant; it never touches other manifest fields or non-registry files. After manifest edits it regenerates `Cargo.lock` and `bun.lock` through the native toolchains (`cargo metadata`-equivalent / `bun install --lockfile-only` or equivalent) so lockfile version entries stay in sync — lockfiles are outputs of the bump, never hand-edited.
- **FR3.4.** Tags are `<component>-v<semver>` annotated tags (e.g. `ox-v0.2.0`, `oxidean-api-v0.2.0`). The rolling `ox-dist` tag is unchanged. **Tag creation is owned by the release flow**: when a version-bump commit lands on `main`, `make release-tags` (maintainer-run) or the automated release flow creates and pushes the annotated tags for every bumped component on that merge commit. Tag pushes that must cascade into tag-triggered release workflows **never use the default `GITHUB_TOKEN`** (GitHub suppresses workflow-triggering for token-pushed tags): the manual path uses the maintainer's own git credentials; the automated path (FR8.2) uses a dedicated release credential (PAT/GitHub App) or drives releases via `workflow_dispatch` instead of tag events.
- **FR3.5.** The first release computed under this system treats `0.1.0` as the baseline (see FR2.5); history before `0.1.0` is never scanned for changelog content.

### FR4 — Changelogs

**FR4.** Each component gets a `CHANGELOG.md` in Keep a Changelog format at its manifest directory, generated from its attributed conventional commits grouped by type (Features, Fixes, Performance, Breaking, etc.) with commit refs.

- **FR4.1.** A root `CHANGELOG.md` consolidates every release: per-version sections referencing the per-component changelog entries with links to the inner files.
- **FR4.2.** Changelog generation is idempotent: regenerating an already-written release section produces identical content.
- **FR4.3.** The wire-contract entry keeps its own changelog at `docs/WIRE-CHANGELOG.md` (Keep a Changelog); its entries also appear in the consolidated root changelog.
- **FR4.4.** A component bumped only by dependency propagation gets a changelog entry containing a `### Dependencies` section listing the internal dependency bumps that caused it — propagated bumps are never silent.

### FR5 — Release-pipeline integration

**FR5.** Existing tag-driven workflows are extended, not replaced:

- **FR5.1.** `ox-cli.yml` triggers on the `ox-v*.*.*` tag pattern; its path filters are corrected to include `crates/oxidean-core/**` and every internal dependency of the CLI (closing the identified gap). Release notes for the GitHub Release come from the component changelog section rather than `--generate-notes`.
- **FR5.2.** `publish-runner.yml` triggers on the `oxidean-runner-v*.*.*` component tag.
- **FR5.3.** A `make` entrypoint (e.g. `make release-check` / `make release`) runs detection, bumping, and changelog generation locally; release commits the tool authors use `chore(release):`.
- **FR5.4.** GitHub Releases are created per component tag with notes drawn from that component's changelog section. The consolidated history lives in the root `CHANGELOG.md`.
- **FR5.5.** **Every** registry entry has defined release semantics: a generic per-component release job handles `*-v*.*.*` tags — creating the GitHub Release with notes from the component's changelog section — **except `ox-v*`**, whose dedicated `ox-cli.yml` owns release creation (it attaches the binary matrix artifacts). `publish-runner.yml` only publishes the GHCR image — the generic job therefore creates the `oxidean-runner` GitHub Release (notes + image reference), and the wire entry gets a tag + changelog + release notes like every other entry.
- **FR5.6.** All `v*`-shaped assumptions inside the retargeted workflows are updated to the component tag scheme: `ox-cli.yml`'s `startsWith(github.ref, 'refs/tags/v')` job guards and `${GITHUB_REF_NAME#v}` version extraction (nfpm versions) strip the `ox-v` prefix correctly; `publish-runner.yml`'s tag event produces an image tag of the bare version (`0.2.0`), not `oxidean-runner-v0.2.0`.

### FR6 — Version-aware surfaces

**FR6.** Every consumer-facing surface reports the version of the component it represents:

- **FR6.1.** `ox --version`, CLI User-Agent, MCP `clientInfo`, self-update comparison → `oxidean-cli` version.
- **FR6.2.** `system.health`, `system.manifest` server version, MCP `serverInfo` → `oxidean-api` version.
- **FR6.3.** `/cli/latest` and the dist manifest → the **ox CLI** component version (decoupled from the API crate version — resolves the identified latent bug).
- **FR6.4.** The wire-contract SemVer unifies the Oxidean-owned version surfaces (`RPC_PROTOCOL_VERSION`, OpenAPI `info.version`) into one constant reported on `system.manifest` (as a `wire_version` field) and embedded in the generated client; compatibility checks compare SemVer. MCP `serverInfo.version` stays the `oxidean-api` component version (it names the server software); MCP `protocolVersion` negotiation keeps the external MCP spec-revision dates. `MIN_CLI_VERSION` keeps its **floor** semantics (the oldest CLI the server will serve) and becomes a SemVer floor value distinct from the current wire version — both are reported. The REST `/api/v1` URL path is a routing namespace and is unchanged. The MCP spec-revision date lists are external MCP-spec identifiers pinned in **five** places today (`oxidean-api/src/mcp.rs`, `oxidean-cli/src/mcp.rs`, `oxidean-web/src/wellknown.rs`, `apps/web/src/lib/webmcp.ts`, `apps/web/src/routes/admin/mcp.tsrx`) — they are *deduplicated to a single canonical source* (consumed by all five sites) covered by the wire entry's detection globs, but are NOT replaced by the Oxidean SemVer.
- **FR6.5.** The dead `OXIDEAN_RPC_VERSION` env configuration (documented but read by no code) is resolved — either wired to the real constant or removed from `.env.example`, compose, and `docs/CONFIGURATION.md`.
- **FR6.6.** `@oxidean/api-client` carries its own independent component version, bumped through the `oxidean-api → @oxidean/api-client` generated-source edge when the API surface it was generated from changes; the generated client embeds the wire SemVer constant. The web app carries its own version.

### FR7 — Documentation and agent guidance (Must)

**FR7.** The convention is documented for humans and agents:

- **FR7.1.** CONTRIBUTING / docs gain: the commit grammar, bump mapping, hook setup (`make setup`), and the release flow.
- **FR7.2.** `.agents/skills/` gains a semver/release skill (surfaced to Cursor/Claude harnesses via existing symlinks) so agents write conforming commits and use the release tooling; `AGENTS.md` is updated.
- **FR7.3.** `Makefile` help output covers the new targets.

### FR8 — Nice-to-haves in backlog (Could)

**FR8.** Accepted nice-to-haves, implemented where they ride the same machinery:

- **FR8.1.** PR title linting — folded into FR1.2 (Must by inclusion in CI enforcement).
- **FR8.2.** Automated release PRs / bot-opened bump branches — a CI workflow that runs the release computation and opens a PR instead of a maintainer running `make release` locally.
- **FR8.3.** Prerelease channels — the grammar accepts and the bumper computes prerelease suffixes (`-alpha.N`, `-beta.N`, `-nightly.YYYYMMDDN`) when requested.
- **FR8.4.** Annotated release tags (included by default); signed tags supported as configuration, not required.

## Non-Functional Requirements

- **NFR1.** The automation is a repo-owned Bun/TypeScript tool under `scripts/`; zero new runtime dependencies beyond the repo's existing toolchain (Bun, GitHub Actions, `gh`, nfpm).
- **NFR2.** Determinism — identical inputs produce byte-identical manifest edits and changelog output.
- **NFR3.** Precision — change detection must not fire on generated files, templates, or fixtures; every detection rule is explicitly scoped in `versioning.toml`.
- **NFR4.** Testability — the tool is covered by tests run under the existing harness (Vitest under `make test`); CI/workflow changes are verified locally with `act` and Docker per the project's testing posture.
- **NFR5.** No secrets; hook installation never mutates git config silently outside the explicit `make setup` step; enforcement cannot be weakened by a local bypass (CI is authoritative).
- **NFR6.** Backward compatibility of release artifacts — existing `ox` install/update flows keep working; only tag/trigger naming and notes generation change.

## Constraints

- Use existing project technologies: Bun/TypeScript scripts, Make targets, GitHub Actions, nfpm, `gh` — no new frameworks or services.
- Brownfield: extend `.github/workflows/ox-cli.yml` and `publish-runner.yml`; do not replace them without justification.
- All components start at `0.1.0` (user-set policy baseline).
- Every capability inside the confirmed boundary (FR1–FR7) is Must-have; the accepted nice-to-haves (FR8) are Could; quality is the gate, not the deadline.
- No AI/tooling attribution on commits or PRs (repo rule).

## Assumptions

- **A1.** Propagated dependency bumps are patch-level by default (documented rule; a breaking change in a shared crate does not force minor on dependents automatically). Ratify at PR review.
- **A2.** Merge commits and auto-generated reverts are exempt from the grammar (enforced via pattern, not opt-out flags).
- **A3.** Tooling-authored release commits use `chore(release): …` and never drive bumps themselves.
- **A4.** Both merge and squash merges are possible, so CI lints both the commit range and the PR title.
- **A5.** `OXIDEAN_RPC_VERSION` is confirmed dead during implementation; default resolution is removal from env/docs rather than wiring an override.
- **A6.** Conductor-decided answers in `requirements-analysis-questions.md` stand unless the user overturns them at PR review.

## Out of Scope

- Publishing to public registries (crates.io, npm) — deferred to a later change.
- Backfilling changelogs for history before `0.1.0`.
- New versioned artifact types (container images, distro packages) beyond the existing release packaging.
- Renegotiation/handshake protocol changes beyond reporting and compatibility-checking the wire version.

## Open Questions

- Exact wire-version constant name/location and the SemVer compatibility rule for 0.x (design-stage decision; likely "same minor" while `<1.0`).
- Whether `rpc-gen`/`openapi-gen`/`oxidean-protection-hook` appear in the published changelog surface or remain internal-only changelogs (cosmetic; both satisfy the internal-debugging rationale).
- Signed-tag configuration details (key management) — deferred to FR8.4 implementation.
