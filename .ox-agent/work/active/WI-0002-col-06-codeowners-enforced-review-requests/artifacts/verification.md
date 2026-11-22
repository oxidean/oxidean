# WI-0002 verification record

## Local gates

- `cargo test -p oxidean-api --test pull_reviews`: 4 passed, including base-revision lookup, team fan-out, author exclusion, stale-current-head approval behavior, notification idempotency, stale-path presence, and fail-closed missing-base evaluation.
- `cargo test -p oxidean-api --test branch_protection_rpc`: 2 passed.
- `cargo test -p oxidean-api --test branch_protect_push`: 12 passed.
- `cargo test -p oxidean-db --test dialect_branch_protection`: 1 passed; migration text asserts false/zero defaults for all dialects and SQLite schema metadata reports default `0`.
- `cargo test -p oxidean-db --test dialect_orgs`: 4 passed.
- `cargo test -p oxidean-api-core pull::codeowners::tests`: 3 passed, including equal-timestamp review ordering and partial request-insert retention; `cargo test -p oxidean-api-core pull::codeowners_rules::tests`: 8 passed, including control-character path rejection.
- `cargo test -p oxidean-git changed_paths_`: 3 passed, covering bounded output, both paths of a rename, and exact NUL-delimited quoted/Unicode/control-character paths; `commit_exists_distinguishes_missing_commits` passed.
- `cargo fmt --all -- --check`, `git diff --check`, web lint, web format, and the focused pull-protection integration test (1 passed) passed. RPC/client sync, OpenAPI sync, targeted Clippy (`oxidean-api-core` and `oxidean-git`), Rust formatting, and `git diff --check` passed. The pull-protection route integration test passed (1 test); web lint and format checks passed.
- Browser component tests: 51 files, 119 tests passed.
- Clean-user-facing-text audit of the updated WI-0002 spec, implementation, review, and verification records found no flagged spans; stylometry scores were low-density and scored CLEAN.

## Delivery state

PR #290 is open at `feat/col-06-codeowners` against `feat/col-05-org-teams` (PR #288). The latest local update includes deterministic review ordering, notification-safe partial request handling, missing-base fail-closed behavior, NUL-delimited rename-safe path enumeration, control-character rejection, migration-default assertions, and the diagnostic UI/documentation clarifications. The branch remains stacked on PR #288. The verified update is ready to commit and push; remote CI and review state must be rechecked after push. No merge was attempted.

The three-dialect migration adds an opt-in boolean with a false default and is additive. Rollback is to disable the protection setting and revert the application change; no data deletion is required.
