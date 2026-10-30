# U4 Changelog & Release Orchestration — Code Summary

## Delivered

- `scripts/versioning/lib/changelog.ts` — Keep-a-Changelog rendering:
  `## [<toVersion>] - <date>` sections bucketed into Features / Fixes /
  Performance / Reverts, `### Dependencies` for `propagatedFrom` entries,
  `### Changed` maintenance fallback for forced-note entries, and a
  version-keyed `upsertSection` that replaces in place (never duplicates).
  `renderRootSection` consolidates each cut in the root `CHANGELOG.md`
  with component version deltas and tag names.
- `scripts/versioning/commands/changelog.ts` — `changelog [--since]
  [--channel] [--dry-run]`; reuses `buildPlan` from `commands/plan.ts`
  (never a divergent plan); writes per-component `changelog_path` files,
  `docs/WIRE-CHANGELOG.md` for `wire`, and the root `CHANGELOG.md`.
- `scripts/versioning/commands/release-tags.ts` — annotated
  `git tag -a --cleanup=verbatim` for `<component>-v<semver>` with the
  changelog section as the annotation body; `--all` performs the FR2.5
  baseline cut at carrier versions; existing tags are skipped.
  `--cleanup=verbatim` preserves `##` headings git would strip as comments.
- Changelog stubs: root `CHANGELOG.md` + all ten `changelog_path` files.
- `.github/workflows/release.yml` — generic push lane for `*-v*.*.*`
  (excludes `ox-v*`, which owns its lane): resolves component + semver
  from the ref, marks prereleases (`x.y.z-*`), creates the GitHub Release
  from the tag annotation. `workflow_dispatch {component, channel}` runs
  plan → changelog → bump → tag on a `release/*` branch and pushes via
  `RELEASE_TOKEN` (never the default `GITHUB_TOKEN`, FR3.4).
- `.github/workflows/release-pr.yml` — dispatch/weekly-cron lane: plan →
  changelog → bump on a `release/*` branch, opens a `chore(release):` PR.
- `ox-cli.yml` — retargeted to `ox-v*.*.*` tags; added the missing
  `crates/oxidean-core/**` path filter (FR5.1); the release job writes
  `latest.json` (`{schema_version, version, dist_commit, targets[]}`) on
  stable cuts only and uploads it into the `ox-dist` payload; prerelease
  tags mark the GitHub Release prerelease and skip `latest.json` (C5).
- `publish-runner.yml` — retargeted to `oxidean-runner-v*.*.*`; image tag
  uses the prefix-stripped semver; its GitHub Release comes from the
  generic lane.
- `Makefile` — `release-plan`, `changelog`, `release-tags` (ARGS passthrough).

## Decisions

- Tag annotations carry the changelog section verbatim — release notes in
  every lane are `git tag -l --format='%(contents)'`, so notes and tags can
  never drift.
- `latest.json` is written only by the stable `ox-v*` release job into the
  `ox-dist` asset payload; rolling `ox-dist` refreshes do not write it, so
  `/cli/latest` reports `0.0.0-dev` until the first stable cut lands (C5).
- The dispatch lane pushes all newly created tags, not just the requested
  component's — propagated dependents would otherwise be orphaned.

## Verification

`bun test scripts/versioning` — 115 pass, 0 fail. New coverage: section
render/buckets/idempotent upsert, command-level changelog on a temp git
repo, annotated tag creation + skip-existing + dry-run, workflow YAML
shape (trigger matrix, prefix strip, latest.json gating, RELEASE_TOKEN).
All five workflow files parse clean and their embedded `run:` scripts
pass `bash -n`.
