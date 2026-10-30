# U7 Contributor Guidance — Unit Test Instructions

Docs-only unit. The testable surface is a docs-shape guard:
`scripts/versioning/docs.test.ts` (Bun test, runs with the existing
`bun test scripts/versioning` suite — same runner convention as
`hook.test.ts` / `workflows.test.ts`).

## Runner

- Command: `bun test scripts/versioning/docs.test.ts` (full suite:
  `bun test scripts/versioning`).
- No new dependencies; `readFileSync` against repo-root-relative paths.

## Coverage

1. `docs/VERSIONING.md` exists and is non-trivial (> ~100 lines).
2. BR7.1 topic coverage — the file contains anchors for every required
   topic: independent component versions, `0.1.0` baseline, commit grammar
   (`type(scope):` + the `!`/BREAKING markers), allowed types, scope
   selection + authority, exemptions, `make setup` hook, `--no-verify`,
   changelog locations (`CHANGELOG.md`, `WIRE-CHANGELOG.md`), release flow
   (`release-plan`/`release-tags`), `RELEASE_TOKEN`, prerelease channels,
   `chore(release):`.
3. BR7.3 no-duplication — VERSIONING.md names `versioning.toml` as the
   scope authority; it does NOT contain a table duplicating every
   registry `scopes` row (assert against the registry parsed from
   `versioning.toml`: at most a handful of illustrative aliases appear,
   never the full set in table form).
4. `CONTRIBUTING.md` links `docs/VERSIONING.md`.
5. `AGENTS.md` names the commit convention, the commit-msg hook/`make
   setup`, and the CI commit-lint gate.
6. `docs/DEVELOPMENT.md` and `docs/TESTING.md` reference the convention
   (`commit-check`/`version-check` or VERSIONING.md).
7. `.agents/skills/semver-conventions/SKILL.md` exists with `name:` and
   `description:` frontmatter.
8. `.cursor/rules/oxidean-core.mdc` still mentions the commit
   convention (pointer text present).

## Methodology notes

- Test-after per the contract: author the docs first, then the guard.
- Keep the existing 115+ test suite green; the new file adds its cases to
  the same suite total.
- BR7.4 dogfood check is manual-review, not a test: every file this unit
  touches is outside all component `globs` (docs/, AGENTS.md, .agents/,
  .cursor/, CONTRIBUTING.md, scripts/versioning/ — none mapped), so no
  unit-7 commit can induce a bump.
