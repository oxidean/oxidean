# U2 Commit Grammar — Unit Test Instructions

## Framework

- Runner: **Bun test** (`bun:test`), same as U1 — `scripts/` sits outside
  turborepo workspaces.

## Exact unit-scoped command

```bash
bun test scripts/versioning
```

## Test files

- `scripts/versioning/lib/commit.test.ts` — parser unit tests.
- `scripts/versioning/commands/commit-check.test.ts` — subcommand surface.

## Coverage targets (Standard: 5–8 per component)

`lib/commit.ts` (~9 tests):
1. `feat(core): add X` → type=feat, scope=core → resolved ["oxidean-core"], scope_status mapped
2. `feat!: …` and `feat(core)!: …` → breaking=true
3. body `BREAKING CHANGE:` / `BREAKING-CHANGE:` footer → breaking=true
4. `Merge branch …`, `Revert "feat: …"`, `chore(release): …` → exempt,
   type absent, scope_status absent, resolved [], breaking false
5. `Feat(core): …` (bad case) / `unknown(x): …` / `no colon here` /
   `feat: ` (empty subject) → invalid, offending token named
6. `feat(a b): …` / `feat(a,b): …` → invalid multi-token scope
7. `feat(unmapped-scope): …` → valid, scope_status unmapped + warning
8. `feat: …` (no scope) → valid, scope_status absent
9. `chore(release): …` → exempt even though `release` is a reserved scope
10. subject >100 chars → warning appended, still valid

`commands/commit-check.ts` (~5 tests):
11. `--message "feat(core): x"` → exit 0, `ok`
12. `--message "bad message"` → exit 1 naming the violation
13. `--file` reads a commit-msg file (subject + body parsed together)
14. `--range` with injected `git log` output → reports each commit, exit 1 if
    any invalid; exempt commits print `ok (exempt)`
15. usage error (no mode flag) → exit 2

## Mocking / fixtures

- Registry: a two-entry in-memory `RegistryConfig` fixture (no file IO).
- `--range` mode injects the commit enumerator — no real git calls in unit
  tests.
- `--file` mode uses a temp file in `os.tmpdir()`, cleaned in `afterEach`.
