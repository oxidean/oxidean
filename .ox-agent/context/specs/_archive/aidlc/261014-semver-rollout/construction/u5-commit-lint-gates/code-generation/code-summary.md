# U5 Commit Lint Gates — Code Summary

## What landed

- `.githooks/commit-msg` — sh hook resolving the repo root via
  `git rev-parse --show-toplevel`, execing
  `bun $ROOT/scripts/versioning/cli.ts commit-check --file "$1"` (BR5.1, BR2.5
  shared parser). Executable in the tree; `--no-verify` bypasses.
- `Makefile` — `setup` installs `core.hooksPath=.githooks`, warning (stderr)
  when a foreign hooksPath existed (BR5.2); `commit-check` convenience target;
  both in `.PHONY` + `help`.
- `.github/workflows/ci.yml` — `commit-lint` job (`needs: makefile-lint`,
  `permissions: contents: read`): `fetch-depth: 0` checkout, bun 1.4.0,
  `commit-check --range` over `base..head` (PR) or `before..after` (push,
  skipped on branch creation), `commit-check --pr-title` on `pull_request`,
  and `cli.ts check` for registry completeness (BR5.3–BR5.5).
- `scripts/versioning/hook.test.ts` — 8 tests: real `git commit` through the
  hook in a temp repo (valid pass / invalid fail / warning pass /
  subdirectory / `--no-verify`), `make setup` install + foreign-value warning
  + idempotency, and static `ci.yml` shape assertions.

## Decisions

- **ci.yml in-place** rather than a second workflow file — one pipeline file
  keeps the job ordering/`needs:` wiring visible.
- **Hook tests symlink** `scripts/versioning`, `versioning.toml`, and
  `.githooks` into a temp repo so `git rev-parse` resolution exercises the
  real tree hermetically — no commits touch the real index.
- **`make commit-check`** lints `HEAD~10..HEAD` as a sane local default.

## Deviations

None — the spec's three surfaces (hook, setup, CI) all shipped.
