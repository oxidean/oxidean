# U2 Commit Grammar — Code Generation Plan

Implements the shared conventional-commit parser (`parse_commit`) and the
`commit-check` subcommand per contract C2. Methodology: test-after.

## Testing Contract

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

Applicable layers: business logic (parser rules), CLI surface. No data/API/
frontend layers.

## Story-to-step traceability

- FR1.1 (grammar) → Steps 2, 5
- FR1.2/NFR4 (check surface) → Steps 3, 5
- FR3.1 (scope signals feed attribution) → Step 2 (resolved[] output)
- BR2.1–BR2.7 → Steps 2–4

## Plan steps

- [x] Step 1: Verify test runner — `bun test scripts/versioning` already covers
      this tree (U1); record exact command in unit-test-instructions.md.
- [x] Step 2: Business logic — `scripts/versioning/lib/commit.ts`:
      `CommitMessage`/`ConventionalCommit` types; `parseCommit(msg, registry)`
      implementing exempt pre-check (BR2.3: `^Merge `, `^Revert "`,
      `^chore(release):`), header grammar `type(scope)!: subject` with
      registry `allowed_types` (BR2.1), breaking detection `!` +
      `BREAKING CHANGE:`/`BREAKING-CHANGE:` footers (BR2.2), scope resolution
      mapped/unmapped/reserved/absent via registry aliases (BR2.4), advisory
      warnings (>100-char subject, unmapped scope). Raw signals only — no bump
      field (C2).
- [x] Step 3: `commands/commit-check.ts` — four input modes
      (`--message`, `--file`, `--range a..b` via `git log`, `--pr-title`),
      per-commit report lines, warnings on stderr, exit 0/1/2 (BR2.6/2.7).
      `git log` enumeration injectable for tests.
- [x] Step 4: `cli.ts` — register `commit-check` subcommand + usage text.
- [x] Step 5: Tests — `lib/commit.test.ts` (parse matrix: all exempt patterns,
      each grammar violation class, breaking variants, scope states, warnings)
      + `commands/commit-check.test.ts` (modes, exit codes, range enumeration
      with injected commits, mixed-validity ranges). ~14 tests.
- [x] Step 6: Docs/traceability — code-summary, source-manifest,
      traceability.json.
