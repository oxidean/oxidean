# U1 Version Registry — Functional Spec

Source of truth for workflows. Derived views: entities (see `entities.md`), rules (see `rules.md`).

## Purpose

The registry unit owns `versioning.toml`: its schema, the loader/validator every other unit consumes (`load_registry` — contract C1), the `check` subcommand (contract C2 + BR1.8 output), and the one-time manifest migration making literal versions authoritative.

## Data model (derived)

- `RegistryConfig`: `path`, `baseline_commit` (sha40, written once), `allowed_types` (the 11 conventional types), `reserved_scopes` (`release`), `entries[]`.
- `RegistryEntry`: `id`, `kind∈{crate,bun,contract}`, `manifest_path` xor `constant_path`, `tag_prefix`, `changelog_path`, `globs[]`, `scopes[]`, `depends_on[]`.

10 registered entries: 7 crates (`oxidean-cli`, `oxidean-core`, `oxidean-logging`, `oxidean-memory`, `oxidean-api`, `oxidean-web`, `oxidean-runner`), 2 Bun packages (`oxidean-webapp` = `apps/web`, `oxidean-api-client` = `packages/api-client`), 1 contract (`wire` → `crates/oxidean-core/src/rpc.rs#WIRE_VERSION`).

Not entries: versionless root `oxidean` package, root private `package.json`, `skills-lock.json` `"version":1` (schema marker), the three `[[bin]]` generator targets inside `oxidean-api`, scaffold templates (`crates/*/templates/**`, `install.sh` embedded files), test fixtures (`tools/ox-dev/tests/fixtures/**`).

## Workflows

### WF-1 `check` — registry validation

1. Parse `versioning.toml`; unknown keys → violation (BR1.3).
2. Field validation per entry: manifest/constant carrier per kind (BR1.1), paths exist, tag prefixes end `-v`, uniqueness of ids/scopes/tag_prefixes (BR1.7), DAG acyclicity (BR1.4).
3. Workspace enumeration: `cargo metadata` members + root `package.json` workspaces; bidirectional completeness (BR1.2); non-component manifests never match (BR1.5).
4. `baseline_commit` present, 40-hex (BR1.6).
5. Emit violations `versioning.toml:<line>: <msg>` (or `--json` list); exit 1 on any, else print per-entry ok and exit 0 (BR1.8).

### WF-2 `load_registry` — library contract (C1)

Callers get a typed `RegistryConfig`. Behaviour: read → parse → validate (same rules as WF-1 minus the workspace enumeration — enumeration is check-only, not load-time, so `load_registry` stays cheap and hermetic). On invalid input: return the error, never a partial registry. Consequence: completeness (BR1.2) is enforced only by `check` — locally via `make version-check` and authoritatively in the ci-commit job; `load_registry` callers rely on that gate having run.

### WF-3 One-time manifest migration (manual, not tooling)

1. Remove `version` from `[workspace.package]` in root `Cargo.toml`; each crate's `Cargo.toml` carries `version = "0.1.0"` literally (crate `version.workspace = true` keys become literals).
2. `apps/web/package.json` and `packages/api-client/package.json` carry `"version": "0.1.0"` literally.
3. The merge ships `baseline_commit = "pending"` — a commit cannot contain its own sha. A follow-up `chore(release):` commit on main (BR1.6) records the merge sha; `check` warns-but-passes while pending.
4. `check` must pass on the merged state — this is the gate that proves the registry describes reality.

## States

`versioning.toml` is either **absent** (pre-merge → `check` exits 1 with "registry not found"), **invalid** (violations listed, exit 1), or **valid** (loadable by all consumers). No fourth state: a registry that parses but skips validation is forbidden (strict schema).

## Interfaces consumed by other units

- `load_registry() -> Result<RegistryConfig>` — U2 detector, U3 bump planner, U4 release, U5 enforcement, U6 surfaces.
- `check` subcommand — `make version-check`, `ci-commit` workflow job.
- Per-entry fields drive: glob attribution (U2), scope resolution (U2/U5), tag format (U4), changelog paths (U3), wire constant location (U6).
