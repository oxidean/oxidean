---
name: semver-conventions
description: Oxidean SemVer commit grammar and release conventions. Use when writing commit messages, staging commits, choosing a scope/type, cutting or tagging releases, or updating changelogs — the commit-msg hook and CI commit-lint enforce this grammar.
---

# SemVer conventions (Oxidean)

Every component (Cargo crate, Bun workspace, the `wire` RPC contract) is
versioned **independently** starting at `0.1.0`. Bumps and changelogs are
derived from commit subjects — write accurate subjects; they are not
decoration.

Canonical reference: `docs/VERSIONING.md`. Machine source of truth:
`versioning.toml` (types, scopes, components, tag prefixes, dependency
edges). Never hand-copy registry tables into docs or prompts — read the
file or run `make version-check`.

## Commit grammar (enforced)

```
type(optional-scope)[!]: subject
```

- **type** — must be one of `versioning.toml` `[grammar].allowed_types`:
  `feat`, `fix`, `chore`, `docs`, `style`, `refactor`, `perf`, `test`,
  `build`, `ci`, `revert`.
- **scope** — optional single token aliasing a registry component
  (`api`, `cli`/`ox`, `wire`/`rpc`, `web`/`ui`, `core`, `db`, … — the
  authoritative list is each component's `scopes` in `versioning.toml`).
  `release` is reserved. Multi-component commits: use **no scope** and let
  changed paths attribute the work.
- **`!`** — breaking change (or `BREAKING CHANGE:` / `BREAKING-CHANGE:`
  footer).
- Exempt subjects (no type required): `Merge …`, `Revert "…"`,
  `chore(release): …`.

## What your subject does

| Subject signal | Bump (0.x) | Bump (≥1.0) |
|---|---|---|
| `feat` | minor | minor |
| `fix`, `perf`, `revert` | patch | patch |
| `!` / breaking footer | minor | major |
| grammar-invalid | patch (path-attributed) | patch |
| `docs`, `chore`, `test`, `style`, `refactor`, `build`, `ci` | none | none |

Bumps propagate as patch floor through `depends_on` edges — a `wire` or
`oxidean-core` change can bump API/CLI/web.

## Checklist before committing

1. Subject matches `type(scope): summary` — imperative, concise (<100
   chars is advisory-clean).
2. Type reflects the real change (a `feat` disguised as `docs` starves
   the changelog; a `docs` disguised as `feat` inflates it).
3. Scope is a single registry alias — or omit it for cross-cutting
   commits.
4. Self-check: `bun scripts/versioning/cli.ts commit-check --message
   "<subject>"` or `make commit-check` (recent range).
5. The `commit-msg` hook (installed via `make setup`) rejects bad
   subjects; `--no-verify` bypasses locally but CI `commit-lint` still
   gates the PR range and PR title — the PR title must also carry the
   prefix.
6. Doc/guidance changes themselves are `docs:`-typed commits.

## Releases (automated)

`main` is the beta channel: `release-beta.yml` tags `<component>-v<ver>-beta.N`
on every push that bumps something (tag-only — manifests keep the last stable
version). The production deploy (`production-deploy.yml` promote) creates the
stable bump — changelog + `chore(release):` commit + `*-vX.Y.Z` tags — then
deploys the bump commit. Tag pushes ride `RELEASE_TOKEN` (never
`GITHUB_TOKEN`); ad-hoc `alpha`/`nightly`/single-component cuts go through
`release.yml` `workflow_dispatch`. Details: `docs/VERSIONING.md`.
