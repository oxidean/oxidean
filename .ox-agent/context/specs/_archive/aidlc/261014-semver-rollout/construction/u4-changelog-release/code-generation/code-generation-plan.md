# U4 Changelog & Release Orchestration — Code Generation Plan

## Testing Contract

```json
{
  "version": 1,
  "methodology": "test-after",
  "source": "org",
  "ordering": "implement each applicable testable layer, then write and run that layer's tests.",
  "scope": "semver-release-automation",
  "test_strategy": "standard",
  "project_type": "brownfield",
  "applicable_notes": [
    {
      "layer": "org",
      "text": "We treat tests as a first-class deliverable in every Bolt. The specific\nmethodology (TDD, BDD, ATDD, or classic test-after) is affirmed at\npractices-discovery and recorded in `team.md` under this heading with explicit\n`Methodology` and `Ordering` fields; Code Generation resolves those fields\nindependently from coverage, tooling, and scope notes.\n\nWhen no posture has been affirmed, our default per scope is:\n- **Methodology**: test-after\n- **Ordering**: implement each applicable testable layer, then write and run\n  that layer's tests.\n- `mvp`, `enterprise`, `feature`, `infra`, `classic` add an 80% line-coverage\n  floor and CI execution before merge.\n- `bugfix`, `security-patch` add a targeted regression for the specific\n  bug/vulnerability and require the existing suite to remain green.\n- `express` uses the Minimal strategy: requirement-driven unit tests (one per\n  requirement, with a happy-path floor per component); existing tests remain\n  green.\n- `poc`, `refactor`, `workshop` add no extra new-test floor and require the\n  existing suite to remain green.\n\nThe active `Test Strategy` still applies in every scope and determines test\nvolume/types. Scope floors are additive; they never reduce or replace the\nselected strategy.\n\nBuild and Test verifies defined coverage floors and affirmed quality targets;\nthey may not be weakened to make a step pass.\n\nAffirm a stricter posture in `team.md` if the team commits to one."
    },
    {
      "layer": "project",
      "text": "- CI/Docker-related changes are verified locally with docker + act before shipping; deadlines are ASAP-but-quality-gated — solid tests are required, not optional (learned 2026-10-14)"
    }
  ],
  "obligations": {
    "strategy": "standard",
    "strategy_volume": [
      "Five to eight tests per component.",
      "Unit tests plus integration tests for key boundaries.",
      "Add E2E, performance, or security tests when requirements demand them."
    ],
    "scope_floor": [
      "Keep the existing test suite green.",
      "This scope adds no extra new-test floor beyond the selected test strategy."
    ],
    "combination_rule": "Apply every selected-strategy obligation and every scope-floor obligation; neither replaces the other, and a targeted scope regression may add the narrowest necessary test type beyond the strategy default."
  },
  "plan_profile": {
    "methodology": "test-after",
    "runner_step": "Verify the existing test runner/configuration and record the exact unit-scoped command.",
    "runner_ready_before_first_test": true,
    "testable_layers": [
      "Data model / database behavior",
      "Repository / data access",
      "Business logic",
      "API / endpoint",
      "Frontend behavior"
    ],
    "steps": [
      "Project structure and production configuration skeleton.",
      "Verify the existing test runner/configuration and record the exact unit-scoped command.",
      "Data model / database behavior - implement.",
      "Data model / database behavior - write and run its tests after implementation.",
      "Repository / data access - implement.",
      "Repository / data access - write and run its tests after implementation.",
      "Business logic - implement.",
      "Business logic - write and run its tests after implementation.",
      "API / endpoint - implement.",
      "API / endpoint - write and run its tests after implementation.",
      "Frontend behavior - implement.",
      "Frontend behavior - write and run its tests after implementation.",
      "Environment/build configuration.",
      "Documentation and traceability."
    ]
  },
  "input_sha256": "sha256:269b5fe6c215f1c174eee59ab39199d705e1364320e780e6695b357b2faffa68",
  "contract_sha256": "sha256:3a38dc4eaaa9c318909bab63ee5df202dbcf2a67191fe3789eb17b5f2bd2ae2e"
}
```

## Scope

Deliver the release orchestration: `changelog` + `release-tags` commands
(WF-1/WF-2), the release workflow lanes (WF-3, C4), `latest.json` writing
(WF-4, C5), the automated release-PR lane (WF-5, FR8.2), and changelog stub
files for all ten components.

## Steps

1. `lib/changelog.ts` — Keep-a-Changelog section render
   (`## [<ver>] - <date>`; buckets Features/Fixes/Performance/Reverts/
   Dependencies for `propagatedFrom`), idempotent replace-in-place upsert
   keyed on `## [<ver>]`, root consolidation, stub-file creation.
2. `commands/changelog.ts` — `changelog [--since] [--channel] [--dry-run]`;
   reuses `buildPlan` (never a divergent plan).
3. `commands/release-tags.ts` — annotated `git tag -a <prefix><ver>` with the
   changelog section as annotation; `--all` tags every component at carrier
   version (the FR2.5 baseline cut); skips existing tags.
4. Changelog stubs: root `CHANGELOG.md` + ten `changelog_path` files
   (incl. `docs/WIRE-CHANGELOG.md`).
5. `.github/workflows/release.yml` — generic lane: push `*-v*.*.*` (excluding
   `ox-v*`/`oxidean-runner-v*` which own their lanes) resolves the component
   and `gh release create`s with the changelog section; prerelease tags mark
   the Release prerelease and never write `latest.json`; `workflow_dispatch`
   `{component, channel}` drives `plan --channel` + bump + changelog + tag via
   `RELEASE_TOKEN` (never default `GITHUB_TOKEN` for tag pushes — FR3.4).
6. `.github/workflows/release-pr.yml` — `workflow_dispatch` runs
   plan/changelog/bump on a `release/*` branch and opens a `chore(release):`
   PR (FR8.2).
7. `ox-cli.yml` retarget: `v*.*.*` → `ox-v*.*.*` tag triggers and
   `refs/tags/v` → `refs/tags/ox-v` conditions; add the missing
   `crates/oxidean-core/**` path filter (FR5.1); the release job writes
   `latest.json` (stable tags only) and uses the ox changelog section as
   release notes.
8. `publish-runner.yml` retarget: `v*.*.*` → `oxidean-runner-v*.*.*`.
9. `Makefile` — `release-plan`, `changelog`, `release-tags` targets.
10. Tests: changelog render/idempotency/bucket mapping, `release-tags` on a
    temp git repo (annotated tag created, existing skipped), workflow YAML
    shape assertions (trigger matrix, prefix strip, `latest.json` gating).

## Notes

- No `cli.ts` edits — commands self-register.
- u1 claims `scripts/versioning/` + `Makefile`; its checkpoint will need the
  documented re-review cycle after this unit lands.
