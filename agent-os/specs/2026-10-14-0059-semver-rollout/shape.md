# SemVer Rollout — Shaping Notes (imported)

## Scope

Move the monorepo to proper SemVer: automated versioning with changelogs,
hooked into releases, CLI/MCP update surfaces, and anywhere else versioning
makes sense — uniform, starting at 0.1.0, with a git hook enforcing
semver-prefixed commits plus agent skills/helpers for the practice.

## Decisions

- Per-component version numbers under one uniform practice and baseline
- `type(optional-scope)[!]: subject` commit grammar; `release` scope reserved
- A shared-dependency change counts as a bump hit on dependents;
  `depends_on` edges mirror real Cargo deps
- `versioning.toml` registration requires an existing workspace member with
  a literal `[package] version`
- Changelog tiers: per-component files plus one global changelog
- Release pushes use the `oxidean-release` App / `RELEASE_TOKEN` — never
  `GITHUB_TOKEN`

## Context

- **Origin framework:** AI-DLC intent (brownfield,
  `semver-release-automation` scope)
- **Practice distilled into:** `agent-os/standards/commits-versioning.md`
  and the `semver-conventions` skill under `.agents/skills/`

## Outcome

Shipped; the commit grammar is load-bearing for the whole repo (local hook +
CI gate). Future versioning changes shape new specs here.
