# CI Config — semver-release-automation

## Pipeline inventory (as built)

| Workflow | Trigger | Role |
|---|---|---|
| `ci.yml` | push `main`, all PRs, dispatch | merge gates (see quality-gates.md) |
| `release.yml` | push `*-v*.*.*` tags; `workflow_dispatch {component, channel}` | GitHub Release per tag; single-component prerelease cuts |
| `release-pr.yml` | weekly cron (Mon 09:00 UTC); dispatch `{channel}` | plan → changelog → bump → `chore(release):` PR |
| `ox-cli.yml` | `ox-v*.*.*` tags (+PR legs unchanged) | binary matrix, `ox-dist` rolling release, `latest.json` on stable cuts |
| `publish-runner.yml` | `oxidean-runner-v*.*.*` tags, dispatch | GHCR runner image `oxidean-runner:<semver>` |

## Wiring decisions (executed in U4/U5, recorded here)

- **Tag grammar**: `<tag_prefix><semver>`; `ox-v*` is isolated so the CLI
  binary lane never races generic component lanes.
- **Credential**: tag pushes use `secrets.RELEASE_TOKEN` (maintainer PAT);
  `GITHUB_TOKEN` is forbidden for cascade pushes because it suppresses
  workflow triggers. `release.yml` fails loudly when unset.
- **Latest.json**: release-owned — written only on stable `ox` cuts into
  the `ox-dist` rolling release; `/cli/latest` reads it (U6). Prerelease
  cuts never touch it.
- **Reserved scope**: bot commits use `chore(release):` — grammar-exempt,
  attributes to no component, never feeds the next plan.
- **Commit gate**: `commit-lint` job = `versioning commit-check --range`
  (PR base..head or push before..after; skipped on branch creation) +
  `--pr-title` + `versioning check`.

## Provenance

- Tag annotation bodies carry the component changelog section
  (`release-tags` writes them; `release.yml` reads
  `git tag -l --format='%(contents)'`).
- `SOURCE_COMMIT` may still be embedded as a dist provenance drop-in.

## What this stage changed

Nothing — the pipeline was authored inside U4/U5 code generation and
locally verified in Build and Test (YAML parse, `act -l` job graphs,
embedded-bash `-n`). This file is the stage's audit record of the CI
design rather than new configuration.

## Sources

Consumes the per-unit `code-summary.md` files under
`construction/*/code-generation/` and this stage's upstream
`build-and-test/build-and-test-summary.md` +
`build-and-test/test-results.md` (the build-test-results surface).

