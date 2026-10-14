# Versioning — Oxidean SemVer model

Oxidean versions every component **independently**. Each Cargo crate under
`crates/*`, each Bun workspace under `apps/*` / `packages/*`, and the RPC
wire contract carry their own SemVer. Everything starts at **`0.1.0`** and
bumps are derived from commit subjects — there is no hand-edited version
bump flow.

- The registry is [`versioning.toml`](../versioning.toml) — the single
  machine-readable source for component ids, manifests, tag prefixes,
  changelog paths, scope aliases, dependency edges, and the commit grammar.
- Validate the registry + workspace completeness: `make version-check`.
- Lint commits against the grammar locally: `make commit-check`
  (or `bun scripts/versioning/cli.ts commit-check --message "<subject>"`).

## Components

`[[component]]` entries in `versioning.toml` name each versioned thing:

| Kind | Examples | Version lives in |
|------|----------|------------------|
| `crate` | `oxidean-core`, `oxidean-api`, `ox` | `Cargo.toml` `version = "…"` (literal, never `workspace = true`) |
| `bun` | `web`, `api-client` | `package.json` `"version"` |
| `contract` | `wire` (the RPC wire contract) | `crates/oxidean-core/src/lib.rs` `WIRE_VERSION` |

Tags are `<tag_prefix><semver>` — e.g. `oxidean-api-v0.2.0`, `ox-v0.1.1`,
`wire-v0.1.0`. Tag prefixes are registered per component in
`versioning.toml` (`tag_prefix`); `ox-v*` has its own binary-release lane
in `ox-cli.yml`.

Version numbers live in the per-component manifests only — never in
`versioning.toml` — so the registry stays stable while versions move.

## Commit grammar (enforced)

Every non-exempt commit subject must match:

```
type(optional-scope)[!]: subject
```

- **`type`** — one of the `allowed_types` in `versioning.toml`
  `[grammar]`: `feat`, `fix`, `chore`, `docs`, `style`, `refactor`,
  `perf`, `test`, `build`, `ci`, `revert`.
- **`scope`** — optional, exactly one token (no commas/whitespace).
  Multi-scope commits use no scope and let changed paths attribute the
  work instead.
- **`!`** — breaking-change marker on the header; equivalent footer forms
  `BREAKING CHANGE:` / `BREAKING-CHANGE:` in the body count too.
- **Subject** — free text after `: `. >100 chars is an advisory warning.

### Scopes

A scope token is an **alias** for a registry component. `versioning.toml`
is authoritative: each `[[component]]` `scopes = […]` list is the full
alias table, resolved at attribution time. (Illustrative only: `api` →
`oxidean-api`, `cli`/`ox` → `ox`, `wire`/`rpc` → `wire`, `web`/`ui` →
`web`.)

Scope is a hint, not a requirement:

- Mapped scope → that component gets the commit's bump evidence.
- Unmapped scope → warning; attribution falls back to changed paths.
- `release` is **reserved** (`chore(release): …`) — attributes to no
  component and never feeds the next bump plan.
- No scope → changed paths attribute the commit (a commit may touch
  several components; the union of path-matched components counts).

### Exemptions

Subjects matching these are exempt — no type required, always valid:

- `Merge …` (merge commits)
- `Revert "…"` (reverts)
- `chore(release): …` (release tooling commits, by contract)

Everything else — including `git commit -m "typo"` one-liners — must
carry the `type:` prefix.

### What induces a bump

| Commit signal | Bump during 0.x | Bump at ≥1.0 |
|---|---|---|
| `feat` | minor | minor |
| `fix`, `perf`, `revert` | patch | patch |
| `!` / BREAKING footer | **minor** (0.x rule) | major |
| grammar-invalid subject | patch (attributed via paths) | patch |
| `docs`, `chore`, `test`, `style`, `refactor`, `build`, `ci` | none | none |

Bumps **propagate**: when a component bumps, every registry `depends_on`
dependent records a propagated patch floor. The `wire` contract sits on
the dependency edges of the API, CLI, `api-client`, and `web` — a wire
bump cascades to all of them.

## Local enforcement

```bash
make setup          # core.hooksPath=.githooks — installs the commit-msg hook
```

`.githooks/commit-msg` runs `commit-check` on every commit; an invalid
subject rejects the commit with the grammar error. `--no-verify` bypasses
the hook (use sparingly — CI still checks). Check any message before
committing:

```bash
bun scripts/versioning/cli.ts commit-check --message "feat(api): add thing"
make commit-check    # lints HEAD~10..HEAD
```

CI enforces the same grammar in the `commit-lint` job on
`.github/workflows/ci.yml` (PR branch range + PR title).

## Changelogs

Every component owns a Keep-a-Changelog file at its registered
`changelog_path` (`crates/<crate>/CHANGELOG.md`,
`apps/<app>/CHANGELOG.md`, `packages/<pkg>/CHANGELOG.md`). The wire
contract's changelog is [`docs/WIRE-CHANGELOG.md`](WIRE-CHANGELOG.md), and
the consolidated cross-component log is the root
[`CHANGELOG.md`](../CHANGELOG.md). Changelogs are generated from the bump
plan — never hand-edit generated sections; see *Release flow*.

## Release flow

Version math lives in `scripts/versioning/` (`bun
scripts/versioning/cli.ts <command>`):

| Step | Command |
|---|---|
| See pending bumps | `make release-plan` (`versioning plan --channel stable --json`) |
| Write changelog sections | `versioning changelog` (per-component + root + wire) |
| Apply version writes + lockfile regen | `versioning bump` (`--dry-run` first) |
| Create annotated `<component>-v<ver>` tags | `make release-tags` (tag body carries the changelog section) |

The **automated lane**: `.github/workflows/release-pr.yml` (weekly cron +
`workflow_dispatch {channel}`) runs plan → changelog → bump and opens a
`chore(release):` PR. Merging it and pushing the resulting `*-v*` tags
with **`RELEASE_TOKEN`** cuts the releases.

Tag pushes must use the **`RELEASE_TOKEN`** secret (a maintainer PAT) —
the default `GITHUB_TOKEN` does not re-trigger workflows, so tag pushes
made with it would silently skip every release lane. The tag-push jobs
fail loudly if `RELEASE_TOKEN` is unset.

On a `*-v*.*.*` tag push, `.github/workflows/release.yml` turns the
annotated tag body into a GitHub Release; `ox-v*` tags take the binary
matrix lane in `.github/workflows/ox-cli.yml`, which ships `latest.json`
on the tag's release assets — the distribution manifest consumed by
`ox self-update` / `/cli/latest` (deploy tooling pins
`OXIDEAN_CLI_DIST_URL` to that tag's download URL).

### Prerelease channels

Channels: `stable`, `alpha`, `beta`, `nightly`. Stable emits the bare
SemVer; `alpha`/`beta` append `-<channel>.N` (per-base counter);
`nightly` appends `-nightly.YYYYMMDDN`.

- Automated prereleases: `release-pr.yml` dispatch with `channel`.
- Single-component cuts: `release.yml` `workflow_dispatch` with
  `{component, channel}` — computes the plan for one component, writes
  its changelog, bumps, tags, and pushes via `RELEASE_TOKEN`, which fires
  that component's normal tag lane.

## For coding agents

Commit messages you write are enforced identically — the `commit-msg`
hook and CI `commit-lint` gate apply to every author. See
[`.agents/skills/semver-conventions/SKILL.md`](../.agents/skills/semver-conventions/SKILL.md)
for the agent-oriented checklist.
