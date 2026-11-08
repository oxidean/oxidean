# Domain Design — Questions

Mode note: conductor-decided under the user's standing continuation instruction ("keep going … open PR with green CI ready for review", 2026-10-14). Each answer carries rationale grounded in `requirements.md` and the CodeKB scan; all are reversible at PR review.

## Q1. How is the version tooling structured?

Context: the tool must be usable identically from a maintainer's shell, a git hook, a Makefile target, and a GitHub Actions step.

- A. One Bun/TypeScript CLI under `scripts/versioning/` with subcommands (`commit-check`, `plan`, `bump`, `changelog`, `tag`, `check`), one entry point shared by hook, Make targets, and CI
- B. Separate single-purpose scripts per surface (`scripts/commit-msg.ts`, `scripts/release.ts`, …)
- C. Rust binary in the workspace (a new crate)
- X. Other (please specify)

[Answer]: A — one Bun CLI with subcommands. A single module set keeps the commit grammar parser, registry loader, detector, and changelog emitter as one tested codebase shared by all three enforcement/automation surfaces; Bun is already the repo runtime so CI adds nothing. A new Rust crate would itself need versioning and build time, and separate scripts would fork the grammar. Conductor-decided (2026-10-14); ratify at PR review.

## Q2. What does `versioning.toml` contain?

Context: FR2.2 fixes that the registry holds metadata only, never version numbers.

- A. Array-of-tables per entry: `[[component]]` with `id`, `kind` (crate|bun|contract), `manifest` (path or constant location), `tag` prefix, `changelog` path, `globs` (detection), `scopes` (aliases), `depends_on` (internal edges); plus `[baseline]` commit/tag info and `[grammar]` allowed types
- B. Flat map of component id → paths only; dependency edges derived from Cargo.toml/package.json parsing
- C. YAML instead of TOML
- X. Other (please specify)

[Answer]: A — explicit `[[component]]` tables. Deriving edges by parsing manifests is brittle across two toolchains and misses the manifest-less wire entry and the generated-source edge (`oxidean-api` → api-client); an explicit DAG in the registry is auditable and matches FR2.2/FR2.3. TOML because the repo already edits TOML heavily (Cargo) and Bun parses it natively. Conductor-decided (2026-10-14); ratify at PR review.

## Q3. What is the wire-version constant and where does it live?

Context: FR2.3/FR6.4 — one SemVer owned in the shared core, replacing the integer `RPC_PROTOCOL_VERSION` surfaces.

- A. `pub const WIRE_VERSION: &str = "0.1.0"` in `oxidean-core/src/lib.rs`; `RPC_PROTOCOL_VERSION` is replaced by it; `rpc-gen` emits it into the generated client; `MIN_CLI_VERSION` keeps floor semantics as a separate semver floor constant
- B. Keep `RPC_PROTOCOL_VERSION: u32` and add a parallel SemVer constant
- C. Wire version lives in `oxidean-api` only
- X. Other (please specify)

[Answer]: A — single `WIRE_VERSION: &str` in `oxidean-core`. Core is the one crate both api and cli already depend on (per `dependencies.md`); the generated client picks it up through the existing `make rpc-gen` pipeline. No parallel integer — one contract version, SemVer-compared. `MIN_CLI_VERSION` stays a distinct floor for the CLI binary version (FR6.4). Conductor-decided (2026-10-14); ratify at PR review.

## Q4. How does `/cli/latest` learn the ox CLI version?

Context: FR6.3 decouples the endpoint from the API crate version; the API binary has no dependency on `oxidean-cli` so it cannot embed its version.

- A. The endpoint reads the version from the served dist manifest (`latest.json`), which the release pipeline writes at `ox-v*` release time — the version is release metadata, not an embed
- B. `OXIDEAN_CLI_VERSION` env/config injected at deploy time
- C. GitHub Releases API lookup at request time
- X. Other (please specify)

[Answer]: A — dist-manifest read. `cli_dist.rs` already serves the release manifest; the version it reports should come from that manifest (written by the release flow for `ox-v*` tags), not from a `CARGO_PKG_VERSION` embed of the wrong crate. No new config, no runtime network dependency. Conductor-decided (2026-10-14); ratify at PR review.

## Q5. Where does release computation run?

Context: FR3.4/FR5.3/FR8.2 — the flow needs both a maintainer-driven path and the nice-to-have automated path, without duplicating logic.

- A. `make release-plan`/`make release` invoke the same `versioning` CLI locally; the bot-flow CI workflow (FR8.2) calls the identical command — one code path, two triggers
- B. CI-only: a workflow computes and commits bumps
- C. Local-only: maintainers always run the tool
- X. Other (please specify)

[Answer]: A — one code path, local Make targets + the automated workflow call the same CLI. Keeps the Must-have flow testable locally (docker+act per testing posture) and the Could-have bot flow a thin trigger over it. Conductor-decided (2026-10-14); ratify at PR review.

## Q6. How are the MCP spec-revision pins deduplicated?

Context: FR6.4 — five pin sites across Rust (api, cli, web crates) and TypeScript (web app), all carrying external MCP protocol dates.

- A. Canonical constant `MCP_PROTOCOL_VERSIONS` in `oxidean-core`; the three Rust consumers import it; the web app gets it through the generated `@oxidean/api-client` (rpc-gen emits it alongside the wire version)
- B. A JSON file all consumers read
- C. Leave duplicated; only document
- X. Other (please specify)

[Answer]: A — canonical `oxidean-core` constant propagated to TS via the existing rpc-gen emission. One source, zero new distribution machinery, and the wire entry's globs cover it (it's part of the contract surface). Conductor-decided (2026-10-14); ratify at PR review.

## Q7. How does CI commit-lint cover the PR surface?

Context: FR1.2 — lint every PR commit + the PR title; one grammar, three surfaces.

- A. A `commit-check` subcommand of the same `versioning` CLI: the git hook calls it on one message file; the CI job calls it per-commit over `origin/main..HEAD` plus the PR title from the event payload
- B. commitlint (`@commitlint/cli`) added as a dependency for CI
- C. Regex inline in the workflow YAML
- X. Other (please specify)

[Answer]: A — same parser everywhere (NFR1: zero new runtime deps; a YAML regex would fork the grammar). The hook is a thin shell wrapper over `bun run` so local and CI agree exactly. Conductor-decided (2026-10-14); ratify at PR review.

## Consolidated Summary Confirmation

Summary of confirmed answers:

- One Bun/TypeScript CLI at `scripts/versioning/` with subcommands shared by hook, Make, and CI.
- `versioning.toml`: explicit `[[component]]` registry (id, kind, manifest/constant path, tag prefix, changelog, globs, scope aliases, internal edges) + baseline + grammar — metadata only, no versions.
- Wire version: `WIRE_VERSION: &str` in `oxidean-core`, replaces `RPC_PROTOCOL_VERSION`, emitted into the generated client; `MIN_CLI_VERSION` stays a separate CLI floor.
- `/cli/latest` reads the version from the served dist manifest, not the API crate embed.
- One release-computation code path behind `make release-plan`/`make release`, reused by the automated release-PR flow.
- MCP spec-revision pins dedupe to `MCP_PROTOCOL_VERSIONS` in `oxidean-core`, propagated to TS via `rpc-gen`.
- Commit lint: `commit-check` subcommand — hook calls it per message; CI calls it over the PR range + PR title.

All seven answers are conductor decisions under the standing continuation instruction — reversible at PR review.

Does this all look correct before I generate the components/decisions/traceability artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
