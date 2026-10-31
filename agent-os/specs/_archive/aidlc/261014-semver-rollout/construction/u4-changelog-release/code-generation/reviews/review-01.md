**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** READY

Review of the changelog & release orchestration unit on current bytes:
`lib/changelog.ts`, `commands/changelog.ts`, `commands/release-tags.ts`,
changelog stubs (root + 10 `changelog_path` files), `release.yml`,
`release-pr.yml`, `ox-cli.yml`, `publish-runner.yml`, `Makefile`, plus
`lib/changelog.test.ts`, `commands/changelog.test.ts`,
`commands/workflows.test.ts`.

Inspected against WF-1…WF-5, FR2.5/FR3.4/FR4.x/FR5.x/FR8.2/FR8.4, C4/C5:

1. `lib/changelog.ts` — `renderSection` buckets feat/fix/perf/revert in
   contract order, marks breaking, renders `### Dependencies` from
   `propagatedFrom`, and falls back to `### Changed` for forced-note entries.
   `upsertSection` keys on `## [<ver>]` and replaces in place; the version is
   regex-escaped fully (covers `+` build metadata and prerelease dots).
   `renderRootSection` filters to bumped entries only.
2. `commands/changelog.ts` — calls `buildPlan` (the same C3 path as
   `plan`/`bump`), writes `changelog_path` per bumped entry incl.
   `docs/WIRE-CHANGELOG.md`, creates missing files with the standard header,
   consolidates the root `CHANGELOG.md`, honors `--dry-run`. No divergent plan
   logic.
3. `commands/release-tags.ts` — annotated `git tag -a --cleanup=verbatim -F -`
   (the verbatim flag is load-bearing: `##` section headings would otherwise
   be stripped as comment lines — caught by test). `--all` performs the FR2.5
   baseline cut at carrier versions; existing tags skipped; `--dry-run`
   verified to create nothing.
4. `release.yml` — push lane on `*-v*.*.*` excludes `ox-v*` (its own lane) and
   still releases `oxidean-runner-v*` (GHCR image lives in publish-runner,
   Release here — per C4). Semver `-` gate marks prereleases. Dispatch lane
   `{component, channel}` runs plan→changelog→bump→release-tags on a
   `release/*` branch and pushes all new tags via `RELEASE_TOKEN`, which is
   presence-checked up front; no `GITHUB_TOKEN` tag pushes anywhere (FR3.4).
5. `release-pr.yml` — dispatch + weekly cron, plan→changelog→bump→
   `chore(release):` PR from `release/<channel>-<date>`; reserved `release`
   scope means the bot commit never feeds bump evidence (BR2.3).
6. `ox-cli.yml` — triggers on `ox-v*.*.*`; all `refs/tags/v` guards retargeted;
   `crates/oxidean-core/**` added to both path filters (FR5.1); package
   version strips `ox-v`. `release` job now `needs: publish-dist` so the
   rolling `ox-dist` release exists before `latest.json` uploads into it;
   `latest.json` carries `schema_version: 1`, the tag's semver, `dist_commit`,
   and per-target `sha256`/`bytes` — written on stable cuts only; prerelease
   tags mark `--prerelease` and skip the manifest (C5). Rolling `ox-dist`
   refresh on main pushes is untouched and never writes it (FR5.6).
7. `publish-runner.yml` — `oxidean-runner-v*.*.*` trigger; image tag is the
   prefix-stripped semver.
8. Makefile — `release-plan`/`changelog`/`release-tags` with `$(ARGS)`
   passthrough; `.PHONY` conventions preserved.

### Findings

| ID | Severity | Location | Finding | Resolution |
|---|---|---|---|---|
| - | - | - | No findings. Two defects were caught and fixed before this review: `##` annotations stripped by git comment-cleaning (fixed via `--cleanup=verbatim`), and a `release`/`publish-dist` ordering race on `latest.json` upload (fixed via `needs: publish-dist`). | - |

### Verification

- `make version-check` — ok (10 components, 1 pending-baseline warning).
- `bun test scripts/versioning` — 115 pass / 0 fail across 12 files.
- `cargo check --workspace` — clean.
- All five workflow files parse as YAML; all embedded `run:` blocks pass
  `bash -n`.
- Live smoke: `changelog --dry-run` listed all 10 component files + root;
  `release-tags --all --dry-run` listed all ten `*-v0.1.0` tags.

### Summary

The unit delivers WF-1 through WF-5 on the shared C3 plan path, pins C4 tag
grammar and C5 `latest.json` semantics, keeps prerelease lanes isolated from
the stable manifest, and uses `RELEASE_TOKEN` wherever a tag push must cascade.
READY.
