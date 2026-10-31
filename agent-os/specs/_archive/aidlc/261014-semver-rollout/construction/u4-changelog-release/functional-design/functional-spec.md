# U4 Changelog & Release Orchestration — Functional Spec (note for code-gen)

Unit kind is `packaging` — no required functional-design artifacts; this note captures the workflow decisions the contracts (C3/C4/C5) leave open, for code-generation.

FRs: FR2.5, FR3.4, FR4, FR4.1–FR4.4, FR5, FR5.1–FR5.6, FR8.2, FR8.4.

## Workflows

### WF-1 `changelog` — generation (FR4, FR4.1–FR4.4)

1. Compute the BumpPlan (C3 — same `plan` code path as `make release-plan`; never a divergent implementation).
2. Per entry with `bump ≠ none`: render a Keep-a-Changelog section `## [<toVersion>] - <date>` with `### Features`/`### Fixes`/`### Performance`/`### Reverts` buckets from attributed commits, plus `### Dependencies` listing `propagatedFrom` bumps (FR4.4).
3. Insert at top of the entry's `changelog_path` (after the file header); idempotent — regenerating the same version replaces the section in place, never duplicates (FR4.2).
4. Root `CHANGELOG.md` consolidates: one section per release cut linking to each component changelog section + its tag (FR4.1). Wire entry writes `docs/WIRE-CHANGELOG.md` (FR4.3).
5. `housekeeping`-only components get no section; components with `bump=none` but forced notes get `### Changed` notes only.

### WF-2 `release-tags` — annotated component tags (FR3.4, FR2.5)

- Tag grammar: `<component>-v<semver>` per C4 (`ox-v0.2.0`, `oxidean-api-v0.1.1`, `wire-v0.1.0`).
- Annotated (`git tag -a`) with the changelog section as the annotation body.
- Baseline cut (FR2.5): `make release-tags` on the merge commit creates all ten `*-v0.1.0` tags — the follow-up `chore(release):` commit recording `baseline_commit` is what makes them resolvable.
- Pushed with maintainer credentials — NEVER the default `GITHUB_TOKEN` (GitHub anti-recursion would swallow downstream tag-triggered workflows).

### WF-3 Release workflows (FR5.x)

- `ox-cli.yml` retargeted to `ox-v*` tags; component prefix stripped before asset naming; **adds the missing `crates/oxidean-core/**` path filter** (FR5.1). Release notes = the ox changelog section.
- `publish-runner.yml` → `oxidean-runner-v*` tags (FR5.2).
- Generic `release.yml`: on `*-v*.*.*` push → resolve component → create GitHub Release with that component's changelog section (FR5.3). Prerelease tags (`-alpha.N`, `-beta.N`, `-nightly.*`) create prerelease-marked Releases and NEVER touch `latest.json` (C5) — FR5.4 lane separation.
- Wire entry gets a release lane too: `wire-v*` tag → GitHub Release with the wire changelog section (FR5.5).
- `ox-dist` rolling tag unchanged — refreshes on main pushes only; the tagged `ox-v*` release path is separate (FR5.6).
- `workflow_dispatch` trigger on the release workflow with inputs `{component, channel}` → drives `plan --channel` for prerelease cuts (C4 trigger matrix).

### WF-4 `latest.json` writer (C5)

- Written by the `ox-v*` stable release flow into the dist directory: `{version, target, sha256, bytes, dist_commit}` per artifact; U6's `/cli/latest` reads it (missing file → documented default, never the API crate's `CARGO_PKG_VERSION`).
- `OXIDEAN_CLI_DIST_DIR` merge/drop-in semantics per C5.

### WF-5 Automated release PR (FR8.2)

- `release-pr.yml` workflow (dispatch/cron): runs `plan` + `changelog` + `bump` on a `release/*` branch and opens a `chore(release):` PR. Same code path as `make release` — the bot commit itself is exempt from bump signals (BR2.3) and uses reserved scope `release` (maps to nothing).

## Decisions embedded

- No GITHUB_TOKEN tag cascades anywhere — maintainer credential is a repo secret named `RELEASE_TOKEN` (documented in U7).
- Changelog regeneration is replace-in-place; sections are version-keyed, never append-duplicated.
- `dist_commit` in latest.json = the commit the dist payload was built from (not the release tag sha).
