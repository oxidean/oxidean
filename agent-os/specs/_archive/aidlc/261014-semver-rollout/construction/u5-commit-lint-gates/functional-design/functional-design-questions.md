# Functional Design — U5 Commit Lint Gates — Questions

Unit: `u5-commit-lint-gates` (kind: packaging). Scope: `.githooks/commit-msg` + `make setup` install, `ci-commit` CI job (range + title), registry completeness gate in CI. FRs: FR1.1, FR1.2, FR8.1.

## Q1. Local hook semantics — block or warn?

- A) Block: exit 1 on grammar-invalid (bypassable via `--no-verify` or by not running `make setup`); warnings print but pass.
- B) Warn-only locally.

[Answer]: A — block. The hook is opt-in (installed by `make setup` via `core.hooksPath`), so its non-zero exit is the intended discipline; `--no-verify` remains the deliberate escape hatch. CI is the authoritative gate regardless.

## Q2. CI range — which commits does the gate lint?

- A) `base.sha..head.sha` from the PR event (the PR's own commits — what the author controls).
- B) merge-base of origin/main and HEAD.

[Answer]: A — `github.event.pull_request.base.sha..head.sha` on pull_request events (also covers synchronize). Push-to-main events lint `before..after`. The PR's own commits are what the author controls; linting history outside the PR would fail on other people's old commits.

## Q3. The gate's contents — what does `ci-commit` run?

- A) `commit-check --range` + `commit-check --pr-title` + `versioning check` (registry completeness) in one job.
- B) Split into separate jobs.

[Answer]: A — one `ci-commit` job running all three checks (single gate in branch protection; FR1.2 + FR8.1 + FR2.2's CI side). Failure lines from any check fail the job.

## Q4. `make setup` hook install — overwrite existing hooksPath?

- A) `git config core.hooksPath .githooks` unconditionally; if a prior hooksPath existed and wasn't `.githooks`, warn and print the old value.
- B) Refuse to touch an existing hooksPath.

[Answer]: A with the warning — `make setup` is the repo's documented bootstrap; a foreign hooksPath is surfaced, not silently clobbered.

## Q5. Exempt commits in the CI range scan?

- A) Same parser path — merge/auto-revert/`chore(release):` commits report `ok (exempt)`, never fail (BR2.3 parity; hook and CI identical).
- B) CI is stricter than the hook.

[Answer]: A — identical parser path (BR2.5); no divergence between hook and CI.

---

## Consolidated Summary Confirmation

- `commit-msg` hook blocks invalid subjects (exit 1); opt-in via `make setup` + `core.hooksPath`; `--no-verify` bypass; CI authoritative.
- CI lints `base.sha..head.sha` per PR; push events lint `before..after`.
- One `ci-commit` job: range check + PR-title check + registry completeness (`versioning check`).
- `make setup` warns when replacing a foreign `core.hooksPath`.
- Exempt commits pass in CI exactly as locally.

Does this all look correct before I generate the functional-design artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
