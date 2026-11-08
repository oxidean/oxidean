# Plan — SemVer Rollout (imported record)

> Imported 2026-10-31. Source: AI-DLC intent `261014-semver-rollout`
> ([`../_archive/aidlc/261014-semver-rollout/`](../_archive/aidlc/261014-semver-rollout/)).
> Completed-work record — the AI-DLC pipeline ran 14/15 stages (last:
> ci-pipeline).

## Status: shipped via #131 (merged 2026-10-14)

Uniform SemVer practice across the workspace: automated per-component
versioning + changelogs, SemVer-prefixed commit subjects enforced locally
and in CI, releases driven by tag pushes.

## Delivered

- `versioning.toml` — component registry, commit grammar, `depends_on` edges
- `.githooks/commit-msg` (`make setup`) + CI `commit-lint` on PR range/title
- `scripts/versioning/` — bump detection, changelog generation, `version-check`
- `docs/VERSIONING.md` — grammar, bump semantics, release flow
- Two-tier changelogs: per-component files + one consolidated global
- Release automation pushing via the `oxidean-release` GitHub App
  (the Main-ruleset bypass actor)

## Follow-through

Fixes landed after the intent closed: #132 (semver baseline), #133 (ox-v*
tag pinning for CLI dist), #153 (release titles), #155 (App-token bump
push), #157 (prerelease base range), #178 (README version badge), #180
(required-status-checks upkeep).
