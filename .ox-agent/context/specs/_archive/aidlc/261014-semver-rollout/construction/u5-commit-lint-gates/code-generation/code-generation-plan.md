# U5 Commit Lint Gates — Code Generation Plan

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

Deliver the two enforcement surfaces for the U2 grammar: the opt-in local
`commit-msg` hook (FR1.1, BR5.1–BR5.2) and the authoritative `ci-commit` CI job
(FR1.2, FR8.1, FR2.2-CI; BR5.3–BR5.6).

## Steps

1. `.githooks/commit-msg` — portable sh script; resolves repo root via
   `git rev-parse --show-toplevel`, execs
   `bun scripts/versioning/cli.ts commit-check --file "$1"`. Exit propagates
   (1 aborts the commit; warnings pass). Executable bit set.
2. `Makefile` — `setup` target: `git config core.hooksPath .githooks`; warns
   with the old value when a foreign hooksPath existed (BR5.2). Also a
   `commit-check` convenience target for the local loop.
3. `.github/workflows/ci.yml` — new `commit-lint` job, `needs: [makefile-lint]`:
   `actions/checkout@v7` with `fetch-depth: 0`, `oven-sh/setup-bun@v2`
   (bun-version 1.4.0), `bun install --frozen-lockfile`, then:
   - PR: `commit-check --range base.sha..head.sha` and
     `commit-check --pr-title <title>`;
   - push: `commit-check --range before..after` (skip when `before` is all
     zeros — first push of a branch);
   - `cli.ts check` (registry completeness).
   Permissions: `contents: read` only.
4. Tests (`scripts/versioning/commands/commit-check.gate.test.ts` +
   `scripts/versioning/hook.test.ts`):
   - hook script: exits 1 on an invalid fixture message, 0 on valid, 0 with
     warnings; runs from a subdirectory (repo-root resolution).
   - `make setup`: sets `core.hooksPath` in a temp git repo; warns on a foreign
     pre-existing value; idempotent on re-run.
   - CI job static assertions: `ci.yml` contains `commit-lint` with
     `fetch-depth: 0`, the `--pr-title` step gated on `pull_request`, the range
     step covering both event shapes, and `permissions: contents: read` at job
     or workflow level.

## Notes

- No source edits after the review request — pre-review self-check happens
  before the request is emitted.
- `cli.ts` is not touched: commands self-register.
