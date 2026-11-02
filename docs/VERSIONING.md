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
| `helm` | `oxidean-chart` | `deploy/helm/oxidean/Chart.yaml` `version:` |

`kind = "helm"` carriers live outside both workspaces, so workspace
completeness skips them. They take two extra registry keys: `app_component`
names the component whose current version is stamped into `Chart.yaml`
`appVersion:` after every bump (for `oxidean-chart`: `oxidean-api`), and
`image_components` maps `values.yaml` `<svc>.image.tag` pins to component ids
(the chart api/web/runner entries), so a released chart always installs the
exact image set it was cut against. Both must appear in `depends_on` so their releases
propagate a chart patch bump and refresh the stamps.

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

`main` is the **beta channel**; the production deploy is the **stable
channel**. There are no release PRs and no manual tag pushes.

- **Beta (automatic)**: `.github/workflows/release-beta.yml` runs on every
  push to `main` (plus a weekly catch-up cron and manual dispatch). When the
  plan finds bumpable work it creates annotated `<component>-v<ver>-beta.N`
  tags at the pushed commit and pushes them via `RELEASE_TOKEN`. Tag-only —
  manifests on `main` always carry the last stable version; per-beta notes
  live in the tag annotations / GitHub Releases.
- **Stable (production deploy)**: `production-deploy.yml` `promote` resolves
  the SHA, computes `plan --channel stable`, writes changelogs, applies the
  bump, commits `chore(release): cut stable release` to `main`, and pushes it
  plus the `*-vX.Y.Z` tags **as the release GitHub App** (`RELEASE_APP_ID` /
  `RELEASE_APP_PRIVATE_KEY`) — `main` is pull-request-only under the `Main`
  ruleset and the App is its sole bypass actor. It then waits for any fresh
  `ox-v*` release assets and deploys the **bump commit** — shipped binaries
  self-report the released version. Pass `cut_release: false` to re-promote
  without cutting. The very first promote on a tagless repo instead runs
  `release-tags --all`: a `0.1.0` baseline for every component.
- **Rollback** never touches versions or tags.

## Image lanes (deploy artifacts)

[`.github/workflows/images.yml`](../.github/workflows/images.yml) builds the
four Cloud service images (`oxidean-{api,web,runner,gateway}` on GHCR) and
publishes the all-or-nothing `sha-<commit>` set on every `main` push —
versioning does not define image components; images are artifacts of the
existing components:

- **Component tag → image retag**: `oxidean-api-v*` tags retag the commit's
  `sha-<sha>` **api** image, `web-v*` and `oxidean-web-v*` retag the **web**
  image (frontend app + Rust serving crate share it), `oxidean-runner-v*`
  retags the **runner** image — to `:<semver>`, plus `:latest` on stable
  (prerelease tags publish `:<semver>` only). The gateway rides `sha-*`
  tags only. Retag is a manifest copy — no rebuild; a missing `sha` image
  falls back to a real build.
- **`chore(release):` bump commits** need no rebuild either: the version.rs
  helper (`crates/oxidean-api-core/src/version.rs`) reads
  `/usr/local/share/oxidean/VERSION` before `env!("CARGO_PKG_VERSION")`, so
  `images.yml` makes `sha-<bump>` a layer on the parent sha's image
  (api VERSION file) or a pure retag (other services) — the deployed
  artifact still self-reports the released version.
- **`ox-v*` tags never touch images**: the api fetches its CLI feed at
  container boot from the runtime `OXIDEAN_CLI_DIST_URL` var (deploy tooling
  repins it per environment) — `sha-*` tags stay immutable across ox releases.
- **`oxidean-chart-v*` tags run no image lane at all**: `publish-chart`
  packages `deploy/helm/oxidean` from the tagged tree (version, appVersion,
  and image pins were stamped by `bump`) and pushes it to
  `oci://ghcr.io/<owner>/charts/oxidean`.

Tag pushes must use the **`RELEASE_TOKEN`** secret (a maintainer PAT) —
the default `GITHUB_TOKEN` does not re-trigger workflows, so tag pushes
made with it would silently skip every release lane. The tag-push lanes
fail loudly if `RELEASE_TOKEN` is unset. The production-deploy stable cut
is the one lane that also pushes a *commit* to `main`, so it authenticates
as the release GitHub App (short-lived installation token minted in the
job) — App-token pushes trigger workflows exactly like PAT pushes.

On a `*-v*.*.*` tag push, `.github/workflows/release.yml` turns the
annotated tag body into a GitHub Release; `ox-v*` tags take the binary
matrix lane in `.github/workflows/ox-cli.yml`, which ships `latest.json`
on the tag's release assets — the distribution manifest consumed by
`ox self-update` / `/cli/latest` (deploy tooling pins
`OXIDEAN_CLI_DIST_URL` to the newest `ox-v*` stable tag merged into the
deployed commit). Release **titles are the tag names verbatim** (e.g.
`wire-v0.1.0`) — the tag already embeds the component name, so a prefix
would duplicate it (`wire wire-v0.1.0`).

### Prerelease channels

Channels: `stable`, `alpha`, `beta`, `nightly`. Stable emits the bare
SemVer; `alpha`/`beta` append `-<channel>.N` (per-base counter);
`nightly` appends `-nightly.YYYYMMDDN`. Stable plans bound at the latest
stable tag — prerelease tags never truncate the stable changelog range.
The `<ver>` in `<ver>-<channel>.N` is the *pending stable version*: its
level is measured over commits not yet in a stable tag, so a shipped
prerelease never lowers the next cut's base (a `0.2.0-beta.1` followed by
patch-only work continues `0.2.0-beta.2`, never `0.1.1-beta.*`); each cut's
notes still cover only the commits since the latest tag of any channel.

- `beta`: automated on `main` via `release-beta.yml`.
- `alpha`/`nightly` (and ad-hoc per-component cuts on any channel):
  `release.yml` `workflow_dispatch` with `{component, channel}` — computes
  the plan, writes changelogs, bumps, tags, and pushes via `RELEASE_TOKEN`.

### Local inspection

Version math lives in `scripts/versioning/` (`bun
scripts/versioning/cli.ts <command>`):

| Step | Command |
|---|---|
| See pending bumps | `make release-plan` (`versioning plan --channel stable --json`) |
| Preview a beta cut | `versioning plan --channel beta --json` |
| Write changelog sections | `versioning changelog [--plan <file>]` |
| Apply version writes + lockfile regen | `versioning bump [--plan <file>]` (`--dry-run` first) |
| Create annotated `<component>-v<ver>` tags | `make release-tags` (tag body carries the changelog section) |

`--plan <file>` reuses a `plan --json` payload so one plan drives the whole
cut — recomputing after `bump` would read the bumped carriers as the base and
double-bump the tag.

## For coding agents

Commit messages you write are enforced identically — the `commit-msg`
hook and CI `commit-lint` gate apply to every author. See
[`.agents/skills/semver-conventions/SKILL.md`](../.agents/skills/semver-conventions/SKILL.md)
for the agent-oriented checklist.
