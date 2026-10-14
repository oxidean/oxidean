# Domain Design — Architecture Decision Records

Scope: `261014-semver-rollout` — per-component SemVer automation. Sources: `inception/requirements-analysis/requirements.md`, `inception/requirements-analysis/requirements-analysis-questions.md`, `codekb/oxidean-app/*`.

## ADR-001: Per-manifest version authority with a metadata-only registry

- **Context** — Ten versioned entries (9 manifests + the manifest-less wire contract) need a single source of truth. Cargo workspace inheritance (`version.workspace = true`) cannot express independent versions; a central `versions.toml` duplicates numbers into a second authority that can drift.
- **Decision** — Each component's own manifest (`Cargo.toml`/`package.json`) holds the literal version and is authoritative; `[workspace.package]` loses its `version` key; `versioning.toml` holds only metadata (ids, kinds, manifest/constant paths, tag prefixes, changelog paths, globs, scope aliases, dependency edges, baseline) — never numbers. The wire entry's "manifest" is the `WIRE_VERSION` constant location.
- **Consequences** — `cargo`/`npm`/docs read the shipped truth directly; bump = manifest rewrite + lockfile regen, no sync layer. Cost: the workspace inheritance convenience is lost (crates no longer move in lockstep — which is exactly the intent).
- **Alternatives Rejected** — central `versions.toml` as source of truth (double authority, drift risk); keep workspace inheritance for crates (can't express independent versions).

## ADR-002: Component-prefixed annotated tags

- **Context** — Ten versioned entries (9 manifests + the wire contract) need unambiguous release tags; existing automation assumes flat `v*.*.*` tags.
- **Decision** — Tags are `<component>-v<semver>` (annotated): `ox-v0.2.0`, `oxidean-api-v0.1.1`, `wire-v0.1.0`. Workflow internals (`refs/tags/v` guards, `${GITHUB_REF_NAME#v}` extraction) are updated to strip the component prefix; the rolling `ox-dist` tag is unchanged.
- **Consequences** — Tag→component is self-describing; per-component release lanes coexist; requires retargeting existing workflow triggers and guards (FR5.6 enumerates the sites).
- **Alternatives Rejected** — `component@version` (clashes visually with npm version syntax, awkward in URLs); flat `v*` train (collapses independent versioning); per-branch releases (worse).

## ADR-003: Hybrid scope + path attribution with reserved `release` scope

- **Context** — Ten components share one history; attribution must be reliable without demanding a perfect scope on every commit.
- **Decision** — Conventional `scope` resolves to components via registry aliases when present; absent or unmapped scopes fall back to changed-file globs (unmapped additionally logs a warning); the reserved scope `release` maps to nothing and never falls back (tooling commits). Exempt subjects (merges, auto-reverts, `chore(release):`) carry no bump signal.
- **Consequences** — Self-healing: sloppy commits still attribute via paths; deliberate intent via scopes wins; the release tooling's own commits can't feed back into bump detection.
- **Alternatives Rejected** — scope-only (rejects merges/bot commits — brittle); path-only (can't express "this core change matters to api" intent, and a shared-file edit is ambiguous).

## ADR-004: Patch-level propagation for dependency bumps

- **Context** — A shared-crate change must bump dependents (user requirement), but the propagation severity needs a rule: a `feat` in core shouldn't force a `feat`-scale release of cli.
- **Decision** — Propagated-only bumps are always **patch**; a component with both direct commits and bumped deps takes `max(direct, patch)`; direct-only takes the direct bump (including none).
- **Consequences** — Dependents churn at patch level (accurate: their own surface didn't change semantically); internal debuggability preserved — every shipped dep change is traceable in the dependent's changelog `### Dependencies` section.
- **Alternatives Rejected** — propagate the dep's own bump level (over-escalates: a breaking change in core would minor-bump every dependent even when their public surface is unchanged); no propagation (violates the user's explicit requirement).

## ADR-005: One repo-owned Bun/TypeScript versioning tool

- **Context** — Off-the-shelf tools fit half the repo each: changesets (Bun/npm only), release-plz/cargo-release (Cargo only), git-cliff (changelog only, no dependency-propagation model).
- **Decision** — A single Bun CLI under `scripts/versioning/` with subcommands (`commit-check`, `plan`, `bump`, `changelog`, `tag`, `check`) shared by the git hook, Make targets, and CI jobs.
- **Consequences** — One grammar/parser/testsuite serves all surfaces; zero new runtime dependencies; full control over the dependency-aware propagation that no tool offers. Cost: we own the tool's maintenance.
- **Alternatives Rejected** — two half-tools glued (config drift, two grammars, still no propagation); git-cliff + wrapper (cliff has no DAG model — wrapper does everything anyway).

## ADR-006: Wire contract versioned as a SemVer constant in oxidean-core

- **Context** — `RPC_PROTOCOL_VERSION: u32 = 1`, `MIN_CLI_VERSION = "0.1.0"`, OpenAPI `info.version = "1"` are three divergent contract-adjacent surfaces; the user wants the wire contract versioned.
- **Decision** — `pub const WIRE_VERSION: &str` in `oxidean-core/src/lib.rs`, starting `0.1.0`, registered as a manifest-less entry in `versioning.toml` (tag `wire-v*`, changelog `docs/WIRE-CHANGELOG.md`, scope-driven detection over a curated contract-file glob). `rpc-gen` emits it into the generated client; `system.manifest` reports it as `wire_version`. `MIN_CLI_VERSION` keeps floor semantics as a separate constant. MCP `protocolVersion` negotiation keeps external spec dates — unchanged by this version.
- **Consequences** — One contract version, SemVer-compared, dependency-propagated like any entry; the integer protocol constant retires cleanly.
- **Alternatives Rejected** — keep integer + add parallel SemVer (two truths, worse); wire version living in `oxidean-api` (cli can't see it — cli doesn't depend on api).

## ADR-007: `/cli/latest` reads the CLI version from the served dist manifest

- **Context** — The endpoint embeds the API crate's `CARGO_PKG_VERSION` while serving CLI binaries — under independent versions that's a latent bug; the API binary has no dep on `oxidean-cli` to embed the right one.
- **Decision** — The endpoint reports the version field of the dist manifest it serves (`latest.json`), which the `ox-v*` release flow writes with the real CLI version.
- **Consequences** — Correct version without cross-crate coupling or runtime config; a dev/unreleased manifest reports the dev version honestly.
- **Alternatives Rejected** — `OXIDEAN_CLI_VERSION` env/config injection (deploy-time coupling, drift); GitHub Releases API lookup at request time (runtime network dependency for a static fact).

## ADR-008: Tracked `.githooks/` + `core.hooksPath` via `make setup`; CI is authoritative

- **Context** — The hook must reach contributors without trusting them to run it; enforcement must survive `--no-verify`.
- **Decision** — `.githooks/commit-msg` (tracked) calls the shared `commit-check`; `make setup` sets `core.hooksPath` idempotently and documents it; the CI job lints every PR commit + the PR title and is wired as a required check (documented maintainer step).
- **Consequences** — Zero new dependencies; local fast feedback; the gate is non-bypassable because it lives server-side.
- **Alternatives Rejected** — husky/lint-staged (adds JS hook machinery for one check); documented manual `cp` (nobody runs it); auto-mutating git config on `bun install` (silent environment mutation — violates the no-surprise constraint).

## ADR-009: Baseline tag push is the intentional 0.1.0 release cut; cascades never use GITHUB_TOKEN

- **Context** — First release needs a lower bound for commit scanning; tag pushes from Actions' default token do not trigger tag workflows (GitHub anti-recursion), which would silently break the "tag → release" chain if CI pushed tags.
- **Decision** — `versioning.toml` records the baseline commit; after the versioning change merges, the maintainer runs `make release-tags` (their own credentials) to push the `<component>-v0.1.0` set — deliberately firing the first release wave. The automated path (FR8.2) uses a dedicated release credential or `workflow_dispatch`, never default `GITHUB_TOKEN`.
- **Consequences** — An explicit, reviewed 0.1.0 cut; no silent mass-release on merge; no dead-trigger traps later.
- **Alternatives Rejected** — inert baseline tags (breaks the "tag → release" story for the first release); workflow-pushed tags with GITHUB_TOKEN (releases never fire — hidden failure); PAT as the only path (requires secret setup for the Must-have flow — maintainer push avoids that dependency).

## ADR-010: MCP spec-revision pins dedupe to one oxidean-core constant via rpc-gen

- **Context** — MCP protocol spec dates are pinned in five places across Rust and TS; a spec bump requires editing all five and missing one is an easy, silent bug.
- **Decision** — Canonical `MCP_PROTOCOL_VERSIONS` in `oxidean-core`; Rust consumers (`oxidean-api`, `oxidean-cli`, `oxidean-web`) import it; the TS consumer (`apps/web` webmcp + admin probe) receives it through the generated `@oxidean/api-client` emission. These are external spec identifiers — NOT replaced by `WIRE_VERSION`.
- **Consequences** — One edit propagates everywhere through machinery that already exists; the wire entry's globs cover the canonical source so pin changes bump the contract version.
- **Alternatives Rejected** — shared JSON file (new distribution plumbing for TS+Rust); leave duplicated (repeatedly flagged as tech debt in the scan).

## ADR-011: Wire compatibility follows SemVer caret semantics; OpenAPI reports it

- **Context** — Two checks need a decided compatibility rule: (a) client↔server wire compat today compares integer `RPC_PROTOCOL_VERSION` for equality; under SemVer `WIRE_VERSION` a rule is required for "which client versions may talk to which server versions", and (b) OpenAPI `info.version` is hardcoded `"1"` at `rest/spec.rs` — an unversioned surface (FR6.4) with no owning decision.
- **Decision** — Compatibility is standard SemVer caret semantics: while `<1.0.0`, compatible means same `0.MINOR` (any `0.MINOR.x`); at `>=1.0.0`, compatible means same `MAJOR`. The server-side check (today's `RPC_PROTOCOL_VERSION`/`MIN_CLI_VERSION` comparison sites) enforces this on `WIRE_VERSION`/`MIN_CLI_VERSION`; the generated client emits the same predicate so `@oxidean/api-client` can warn pre-request. OpenAPI `info.version` renders `WIRE_VERSION` (the wire entry owns the field via VersionSurfaceWiring) — the REST path prefix `/api/v1` is a separate, unchanged stability marker.
- **Consequences** — During 0.x every wire minor is a compat boundary, which matches the 0.x bump policy (`feat`/breaking → minor). At 1.0 the boundary relaxes to major-only with no code change. One predicate, two enforcement points, no drift.
- **Alternatives Rejected** — exact-match equality (would make `0.1.0` vs `0.1.1` incompatible — patch bumps would break every client); range-table negotiation (complexity for zero consumers today); leaving OpenAPI at `"1"` (a lying version surface).
