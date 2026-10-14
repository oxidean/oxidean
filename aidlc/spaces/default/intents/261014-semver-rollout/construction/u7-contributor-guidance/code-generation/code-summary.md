# U7 Contributor Guidance — Code Summary

## What landed

- `docs/VERSIONING.md` (new, canonical reference — BR7.1/BR7.5) —
  independent component/version model (`crate`/`bun`/`contract` kinds,
  `0.1.0` baseline, `<tag_prefix><semver>` tags, `ox-v*` dedicated lane);
  the enforced grammar `type(optional-scope)[!]: subject`; allowed types
  named from `versioning.toml` `[grammar]`; scope rules (single token,
  mapped/unmapped/`release`-reserved, path-union attribution) with the
  registry named authoritative and only an illustrative alias subset
  (BR7.3); breaking markers (`!`, both `BREAKING` footer spellings);
  exemptions (`Merge`, `Revert "`, `chore(release):`); the 0.x-vs-≥1.0
  bump table (breaking = minor during 0.x); dependency-edge patch
  propagation incl. the `wire` fan-out; local enforcement (`make setup`
  hook, `--no-verify`, `commit-check --message`); changelog locations
  (per-component, root, `docs/WIRE-CHANGELOG.md`); and the release flow —
  `make release-plan` → `versioning changelog` → `versioning bump` →
  `make release-tags`, the `release-pr.yml` automated lane (weekly +
  dispatch `{channel}`, `chore(release):` PRs), `release.yml` tag lanes +
  `{component, channel}` dispatch, `ox-cli.yml` binary matrix +
  `latest.json` publishing, and the `RELEASE_TOKEN` requirement with the
  `GITHUB_TOKEN` prohibition explained.
- `CONTRIBUTING.md` — "Commit convention" block under Pull requests
  (grammar one-liner, `make setup`, `commit-lint`, pointer to
  VERSIONING.md); VERSIONING.md added to the docs map.
- `docs/DEVELOPMENT.md` — the descriptive "conventional-commit style"
  line replaced by the enforced convention (`make setup`,
  `make commit-check`, CI `commit-lint`, pointer); `commit-lint` added to
  the CI gates list.
- `docs/TESTING.md` — `scripts/versioning` Bun-test surface documented;
  `commit-lint` row added to the CI jobs table.
- `AGENTS.md` — Contribution-standards paragraph: enforced grammar,
  hook + CI gate named, links to VERSIONING.md and the new skill;
  `semver-conventions` added to the skills examples line.
- `.agents/skills/semver-conventions/SKILL.md` (new — BR7.2) — house-style
  frontmatter (trigger wording: writing commit messages, choosing
  type/scope, cutting releases), the grammar, scope-picking rules, the
  bump table, a pre-commit checklist, and a maintainer release pointer.
- `.cursor/rules/oxidean-core.mdc` — bullet naming the enforced grammar,
  hook, CI gate, VERSIONING.md, and the skill.
- `scripts/versioning/docs.test.ts` — 11-case docs-shape guard: topic
  coverage anchors, scope-authority sentence, no-duplicate-scope-table
  (asserts against the live registry — ≤8 illustrative aliases vs >10
  registered), `RELEASE_TOKEN`/GITHUB_TOKEN naming, entry-point links,
  skill frontmatter, cursor-rule pointer.

## Decisions

- **Scope authority wording**: the spec cited a `versioning scopes`
  subcommand that doesn't exist — docs name `versioning.toml` itself as
  the authority (same BR7.3 intent: reference, don't duplicate).
- **Docs stay pointers**: CONTRIBUTING/AGENTS/DEVELOPMENT carry one-line
  grammar summaries + links; only VERSIONING.md restates details.
- **The guard test parses the live registry** for the no-duplication
  assertion — a future doc that hard-codes the table fails CI-adjacent
  `bun test scripts/versioning`.

## Deviations

- `versioning scopes` subcommand named in BR7.3 does not exist →
  `versioning.toml` named as authority instead (recorded for review).

## Verification

- `bun test scripts/versioning` — 126 pass / 0 fail (11 new docs-shape
  cases).
- `make version-check` — ok, 10 components, 1 pending-baseline warning.
- BR7.4 dogfood: every touched file matched against all component
  `globs` — zero matches, so this unit's commits induce no bump.
