# U2 Commit Grammar — Code Summary

## What landed

- `scripts/versioning/lib/commit.ts` — `CommitMessage`/`ConventionalCommit`
  types and `parseCommit(msg, registry)` per contract C2: exempt pre-check
  (`^Merge `, `^Revert "`, `^chore(release):`), strict header grammar
  `type(scope)!: subject` over registry `allowed_types`, breaking detection
  (`!` + `BREAKING CHANGE:`/`BREAKING-CHANGE:` footers), scope resolution
  (mapped/unmapped/reserved/absent via registry aliases), advisory warnings.
  Raw signals only — no bump fields.
- `scripts/versioning/commands/commit-check.ts` — the four C2 modes
  (`--message`, `--file`, `--range`, `--pr-title`), per-commit report lines on
  stdout, warnings on stderr, exit 0/1/2. `git log` enumeration injectable.
- `cli.ts` — `commit-check` subcommand registered.

## Decisions

- `--message`/`--pr-title` values containing newlines split into subject+body
  before parsing (same shape as `--file`) — avoids false grammar failures on
  `git commit -m $'feat: x\n\nbody'`-style input.
- Grammar requires `: ` (colon+space) after the header — conventional-commit
  convention; `feat:x` is invalid.
- Parser takes the registry vocabulary as a parameter — the grammar is a
  registry edit, not a parser edit (BR2.5/spec).

## Tests

22 new tests (52 total under `bun test scripts/versioning`): grammar matrix,
exempt patterns, breaking variants, all four scope statuses, warnings,
range enumeration with injected commits, `--file` mode, exit codes.
