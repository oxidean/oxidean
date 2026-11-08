# Contract Design — Contract Summary

Scope: `261014-semver-rollout`. Boundaries derived from `inception/units-generation/unit-of-work-dependency.md`; component/entity shapes from `inception/domain-design/components.md`; behaviour guarantees from `inception/requirements-analysis/requirements.md` (FR1–FR8, NFR1–NFR6).

Every boundary here is a dev-time/build-time surface (files, CLI, git tags, JSON) — no network timeouts, retries, or SLAs apply. The failure contract is uniform: **fail-closed, typed error list, atomic writes, idempotent regeneration**.

## Contracts

| # | Provider Unit | Consumer | Mechanism | Owner |
|---|---|---|---|---|
| C1 | U1 version-registry | U2, U3, U4, U5, U6 | shared-schema: `versioning.toml` (TOML file) | U1 |
| C2 | U2 commit-grammar | U3, U5 | shared-schema (parsed commit) + CLI contract (`commit-check`) | U2 |
| C3 | U3 change-detection | U4 (+ release-PR workflow) | shared-schema: release-plan JSON | U3 |
| C4 | U4 changelog-release | External: GitHub Actions + tag consumers | string-format tag grammar + workflow trigger matrix | U4 |
| C5 | U4 changelog-release | U6 (`/cli/latest`) + External: CLI installers | shared-schema: `latest.json` dist manifest | U4 |
| C6 | U6 version-surfaces | External: CLI/MCP clients, admins | JSON RPC field contract (`system.manifest`) + generated-client constant | U6 |

## C1 — `versioning.toml` registry schema (U1 → U2,U3,U4,U5,U6)

The single source of truth for component metadata. Metadata only — never version numbers (ADR-001).

```yaml
schema: versioning-toml
version: 1
fields:
  baseline:
    commit: string        # sha — lower bound for commit scanning (FR3.5)
  grammar:
    allowed_types: [string]  # feat,fix,chore,docs,style,refactor,perf,test,build,ci,revert
    reserved_scopes: [string]  # e.g. release
    breaking_markers: ["!<type>", "BREAKING CHANGE: footer"]
  components: "[[component]] array — one per registry entry"
component:
  id: string              # e.g. ox, oxidean-api, wire
  kind: crate|bun|contract
  manifest_path: string?  # Cargo.toml / package.json path — present iff kind=crate|bun
  constant_path: string?  # file#NAME — present iff kind=contract
  tag_prefix: string      # e.g. ox-v, wire-v (tag = <tag_prefix><semver>)
  changelog_path: string  # path to component CHANGELOG.md
  globs: [string]         # changed-file attribution globs
  scopes: [string]        # commit-scope aliases resolving to this entry
  depends_on: [string]    # internal registry edges (ids); acyclic enforced
validation:
  - every Cargo workspace member and Bun workspace package maps to exactly one entry (FR2.2 completeness)
  - exactly one of manifest_path / constant_path per entry, determined by kind
  - depends_on ids resolve; DAG is acyclic
  - globs/scopes/tag prefixes are unique per entry
failure: "exit != 0 with `versioning.toml:<line>: <message>` per violation"
```

## C2 — Commit parser + `commit-check` (U2 → U3, U5)

Parser module (consumed in-process by U3's detector) plus the `commit-check` subcommand (consumed by U5's hook and CI job).

```yaml
schema: conventional-commit-parse
version: 1
input: "subject line (string) + optional commit body + changed-file list"
output:
  type: feat|fix|chore|docs|style|refactor|perf|test|build|ci|revert
  scope: string?            # raw scope token as written
  resolved: [componentIds]  # via registry aliases; empty for unmapped/reserved
  scope_status: mapped|unmapped|reserved|absent
  breaking: boolean         # ! or BREAKING CHANGE: footer
  exempt: boolean           # merge commit, auto-revert subject, chore(release):
  # raw signal only — BumpPlanner (U3) maps type+breaking+version to a bump
cli:
  commit-check:
    argv: "commit-check [--message <m> | --file <path> | --range <a..b> | --pr-title <t>]"
    stdout: "valid: ok | invalid: one `commit <sha>: <message>` error per line"
    stderr: "warnings (e.g. unmapped scope) — warnings never fail the check"
    exit: "0 valid-or-warning / 1 grammar-invalid / 2 usage"
severity:
  unmapped_scope: warning     # valid grammar; detection falls back to paths (FR3)
  reserved_scope_release: "exempt — no component, no bump, no failure"
  grammar_violation: failure  # unknown type, malformed subject → exit 1
```

## C3 — Release plan (U3 → U4)

Emitted by `plan`; consumed by `changelog`, `tag`, and the FR8.2 release-PR workflow.

```yaml
schema: release-plan
version: 1
generatedAt: iso8601
baseRef: string            # commit the plan was computed at
entries:                   # only components with a bump or forced note
  - component: string      # registry id
    fromVersion: semver
    toVersion: semver      # may carry -alpha.N / -beta.N / -nightly.YYYYMMDDN (FR8.3)
    bump: none|patch|minor|major
    direct: boolean        # has direct attributed commits
    propagatedFrom: [componentIds]   # entries whose bumps caused this one
    commits: [{sha, type, scope, breaking, subject}]
failure: "exit != 0 on: registry cycle, unregistered workspace member, unparseable manifest"
```

## C4 — Tag grammar + workflow triggers (U4 → External: GitHub)

```yaml
schema: tag-contract
format: "<component>-v<semver>"      # ox-v0.2.0, oxidean-api-v0.1.1, wire-v0.2.0
annotation: annotated                # signed optional (FR8.4 config)
exceptions: ["ox-dist"]              # rolling tag, unchanged
triggers:
  ox-v*.*.*:            ox-cli.yml            # owns its GitHub Release (binary matrix)
  oxidean-runner-v*.*.*: publish-runner.yml   # GHCR image only; generic job makes the Release
  "*-v*.*.*":           generic release job   # GitHub Release + changelog notes (all other entries)
extraction: "strip `<component>-v` prefix → bare semver for artifact versions"
credential_rule: "cascading tag pushes never use default GITHUB_TOKEN (FR3.4)"
prerelease_tags:
  matching: "<component>-v<semver-prerelease> matches the same globs"
  behaviour: "fire the release lanes; GitHub Release created with prerelease: true;
             never write latest.json (C5 stays on stable cuts only)"
dispatch_lane:
  release-pr: "FR8.2 automated path MAY drive releases via workflow_dispatch inputs
              {component, version} or a dedicated release credential instead of pushing
              tags with GITHUB_TOKEN — same plan payload (C3), same tag grammar"
```

## C5 — `latest.json` dist manifest (U4 → U6 + External: installers)

Written into the CLI dist dir by the `ox-cli.yml` `ox-v*` release job; read by `cli_dist.rs`'s `/cli/latest`.

```yaml
schema: latest-json
version: 1                    # schema marker (carried for the reserved rule below)
fields:
  version: semver             # the ox CLI component version — the ONLY field consumed today
endpoint_mapping:
  response_shape: "UNCHANGED — /cli/latest keeps its live shape
                   {version, targets: [{target, sha256, bytes}], dist_commit?}"
  version_source: "latest.json `version` field (was: api CARGO_PKG_VERSION embed — the latent bug)"
  targets_source: "directory scan, unchanged — including OXIDEAN_CLI_DIST_DIR self-host drop-ins"
  absent_or_invalid: '`version` reports "0.0.0-dev" — an honest dev marker, never the api version'
writer_lifecycle:
  writer: "ox-cli.yml ox-v* release job — writes latest.json into the dist payload it uploads"
  version_source: "the tag's component version (prefix-stripped semver)"
  not_written_by: "the rolling ox-dist main-push refresh — dev builds carry no manifest,
                   so /cli/latest reports 0.0.0-dev until the first ox-v* cut lands the file"
  prerelease_tags: "never write latest.json (C4 prerelease rule)"
```

## C6 — Version surfaces field contract (U6 → External)

```yaml
schema: version-fields
system.manifest:
  server_version: semver    # oxidean-api component version
  wire_version: semver      # WIRE_VERSION constant (replaces integer protocol)
  min_cli_version: semver   # floor — oldest servable CLI (distinct from wire_version)
  protocol_version: "1 (frozen)"   # co-emitted legacy field so pre-SemVer clients keep
                                  # their existing compare/warn path; never read by new code
compat_rule: "caret SemVer: <1.0 → same 0.MINOR; ≥1.0 → same MAJOR (ADR-011)"
transition:
  header_acceptance: >
    The server accepts `Oxidean-RPC-Version` values of `1` — the pre-SemVer
    alias, honoured as wire line `0.1.x` so deployed pre-versioning CLIs keep
    working — plus any SemVer value caret-compatible with WIRE_VERSION.
    Other values are rejected with rpc.version_mismatch as today.
  new_client_old_server: >
    A new CLI sending `0.1.0` to a pre-versioning server (exact-match `1`)
    is rejected — the one-time, documented transition edge at the 0.1.0 cut;
    resolution is a server upgrade. MIN_CLI_VERSION floor handling is
    unchanged (an older-than-floor CLI is rejected by policy, not compat).
  client_manifest_read: >
    wire_version present → caret-compare against embedded WIRE_VERSION;
    absent with protocol_version 1 → treat as wire 0.1.x (compatible with
    a 0.1.x client); absent entirely → dev build, warn only.
generated_client:
  WIRE_VERSION: emitted     # rpc-gen emits the constant + compat predicate
  client_check: "warns pre-request when wire versions are incompatible"
mcp:
  serverInfo.version: "oxidean-api component version"
  protocolVersion: "external MCP spec-revision dates — unchanged"
openapi:
  info.version: "renders WIRE_VERSION (was hardcoded \"1\")"
```

## Contract ownership rules

- **Provider owns the spec.** Each contract's definition lives with its producing unit (U1: registry, U2: grammar/parser, U3: plan, U4: tags+latest.json, U6: manifest fields).
- **Additive changes are always safe** — consumers ignore unknown fields in TOML/JSON contracts.
- **Breaking changes are atomic** — all units land in one PR; a breaking change to C1–C6 updates every consumer in the same diff. No versioning of the contracts themselves is needed while this property holds; if the tool is ever extracted, C1–C3 get a `schema_version` field (already reserved via `version: 1`).
- **Disputes resolve to requirements.md** — the FR text is the arbiter; a contract that can't satisfy an FR is a design bug, not a negotiation.

## Open questions

| Contract | Question | Blocks |
|---|---|---|
| — | None unresolved — all six contracts are fully specified for construction | — |
