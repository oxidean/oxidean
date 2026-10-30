# U5 Commit Lint Gates — Unit Test Instructions

Contract: `test-after`. Strategy: `standard` (5–8 tests per component).

## Suite

`bun test scripts/versioning` stays green; new tests live in
`scripts/versioning/hook.test.ts` and a static CI-shape assertion file.

## Cases

1. Hook: valid message file → exit 0; invalid subject → exit 1 and the parser's
   failure line prints; warning-only commits (unmapped scope) → exit 0.
2. Hook: invoked from a subdirectory of the repo → still resolves the repo root
   and runs.
3. `make setup` in a temp git repo → `core.hooksPath` becomes `.githooks`;
   pre-existing foreign hooksPath → warning naming the old value, still sets
   `.githooks`; second run is a no-op pass.
4. `.githooks/commit-msg` is executable in the index (mode 100755).
5. `ci.yml` shape: `commit-lint` job exists; checkout uses `fetch-depth: 0`;
   PR-only `--pr-title` step; `--range` step for both `pull_request` and `push`;
   `versioning check` runs; job-level `permissions: contents: read`.

## Boundaries

- The hook is exercised end-to-end in a temp git repo (real `git commit`
  against `core.hooksPath`), not just by shelling the script.
- CI assertions are static YAML checks, not act runs (act coverage for CI lives
  in Build & Test).
