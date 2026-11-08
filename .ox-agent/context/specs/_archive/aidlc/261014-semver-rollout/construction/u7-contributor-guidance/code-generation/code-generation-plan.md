# U7 Contributor Guidance — Code Generation Plan

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

Docs-only unit — teaches the versioning convention to humans (canonical
`docs/VERSIONING.md` + entry-point links) and to agents (`AGENTS.md`, a new
in-repo skill, cursor-rule pointer updates). No code surface; the only
"testable layer" is a docs-shape guard test (business-logic layer of the
contract) asserting the required surfaces exist and reference — never
duplicate — the machine-owned truth (BR7.3).

## Steps

1. `docs/VERSIONING.md` (new, canonical reference — BR7.1/BR7.5):
   components+independent-versions model, `0.1.0` baseline, commit grammar
   (`type(scope)!: subject`, type list from `versioning.toml` `[grammar]`),
   scope selection with `versioning.toml` named the authority (illustrative
   table only — BR7.3), breaking markers (`!`, `BREAKING CHANGE:`,
   `BREAKING-CHANGE:`), exemptions (merge/revert/legacy subjects +
   reserved `release` scope), local enforcement (`make setup` hook,
   `--no-verify` bypass, `make commit-check`), where changelogs live
   (per-component `CHANGELOG.md`, root `CHANGELOG.md`,
   `docs/WIRE-CHANGELOG.md`), and the release flow
   (`make release-plan` / `versioning changelog` / `versioning bump` /
   `make release-tags`, `RELEASE_TOKEN` — GITHUB_TOKEN prohibited for tag
   pushes — prerelease channels via `release.yml` `workflow_dispatch`
   `{component, channel}`, the automated `release-pr.yml` weekly/dispatch
   lane, reserved `chore(release):` convention).
2. `CONTRIBUTING.md` — short "Commit convention" subsection under Pull
   requests: grammar one-liner + link to `docs/VERSIONING.md`
   (reference-don't-duplicate). Add `docs/VERSIONING.md` to the docs map.
3. `docs/DEVELOPMENT.md` — replace the "conventional-commit style subjects"
   descriptive line with the enforced convention + hook (`make setup`) +
   `make commit-check`; keep it a pointer, not a copy.
4. `docs/TESTING.md` — add the versioning-test surface note
   (`bun test scripts/versioning`, `make version-check`, `make commit-check`).
5. `AGENTS.md` — under Contribution standards: the commit grammar +
   SemVer rules, name the commit-msg hook and CI `commit-lint` job so
   agents know commits are checked; add `semver-conventions` to the skill
   examples line.
6. `.agents/skills/semver-conventions/SKILL.md` (new — BR7.2): YAML
   frontmatter (`name`, `description` with trigger phrasing matching the
   house skill style), the grammar, scope-picking rules (single token,
   registry aliases, unmapped fallback to paths, `release` reserved),
   what induces a bump (`feat`→minor, `fix`→patch, breaking→major,
   propagation through `depends_on`), how to self-check
   (`make commit-check`, `git commit --dry-run` behaviour of the hook),
   and the no-duplication rule.
7. `.cursor/rules/oxidean-core.mdc` — extend the existing commit-related
   bullet to name the enforced convention (hook + CI gate), pointer only.
8. `scripts/versioning/docs.test.ts` — docs-shape guard: VERSIONING.md
   exists with the BR7.1 topic coverage (grep anchors per topic);
   CONTRIBUTING/AGENTS/DEVELOPMENT/TESTING link or name the convention;
   skill file has valid frontmatter; VERSIONING.md does not hard-code the
   per-component scope table (BR7.3 — asserts the authority sentence and
   absence of a `component → scopes` table duplicating every registry row).

## Notes

- BR7.4: this unit's own commit lands as `docs:` (docs tree is outside every
  component glob — verified: `docs/**`, `AGENTS.md`, `.agents/**`,
  `.cursor/**`, `CONTRIBUTING.md` map to no component `globs`; the new test
  file under `scripts/versioning/**` also maps to no component).
- Scope-table authority: `versioning.toml` is the machine source; there is
  no `versioning scopes` subcommand — docs name the file itself (BR7.3
  wording adjusted to the real surface, flagging for review).
- u1's dir-scope claim on `scripts/versioning/` catches `docs.test.ts` —
  expect the established re-review cascade for u1 after this unit.
