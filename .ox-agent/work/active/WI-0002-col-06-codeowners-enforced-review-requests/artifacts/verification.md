# WI-0002 verification record

## Local gates

- `cargo test -p oxidean-api --test pull_reviews`: 4 passed, including base-revision lookup, team fan-out, author exclusion, stale-current-head approval behavior, notification idempotency, stale-path presence, and fail-closed missing-base evaluation.
- `cargo test -p oxidean-api --test branch_protection_rpc`: 2 passed.
- `cargo test -p oxidean-api --test branch_protect_push`: 12 passed.
- `cargo test -p oxidean-db --test dialect_branch_protection`: 1 passed; migration text asserts false/zero defaults for all dialects and SQLite schema metadata reports default `0`.
- `cargo test -p oxidean-db --test dialect_orgs`: 4 passed.
- `cargo test -p oxidean-api-core pull::codeowners::tests`: 5 passed, including submission-order precedence over PostgreSQL transaction-start timestamps, same-second selection, fail-closed ambiguous legacy ties, and partial request-insert retention; `cargo test -p oxidean-api-core pull::merge_ops::tests`: 2 passed, covering transaction-time inversion and stale-review filtering; `cargo test -p oxidean-api-core pull::codeowners_rules::tests`: 8 passed, including control-character path rejection.
- `cargo test -p oxidean-db --test dialect_pulls`: 4 passed; the tests check the counter schema in all dialect migrations, increasing SQLite review orders, and a PostgreSQL transaction-start/insert-order inversion. The PostgreSQL case is DATABASE_URL-gated and skipped locally because DATABASE_URL is unset; CI must exercise it. `cargo test -p oxidean-db --test dialect_actions migrate_reseats_pre_rebase_versions`: 1 passed with migration 107 in the repaired ledger.
- `cargo test -p oxidean-db concurrent_review_inserts_store_orders_in_row_insertion_order --lib`: passed with 32 simultaneous submissions; persisted order follows row insertion order.
- `cargo test -p oxidean-db`: all tests passed, including unit, migration, and factory-reset coverage.
- `cargo clippy -p oxidean-api-core -p oxidean-db -p oxidean-api-pulls -p oxidean-api --all-targets -- -D warnings`: passed. `cargo test -p oxidean-api-core`: 234 passed; `cargo test -p oxidean-api --test pull_reviews`: 4 passed.
- `cargo test -p oxidean-git changed_paths_`: 3 passed, covering bounded output, both paths of a rename, and exact NUL-delimited quoted/Unicode/control-character paths; `commit_exists_distinguishes_missing_commits` passed.
- `cargo fmt --all -- --check`, `git diff --check`, web lint, web format, and the focused pull-protection integration test (1 passed) passed. RPC/client sync, OpenAPI sync, targeted Clippy (`oxidean-api-core` and `oxidean-git`), Rust formatting, and `git diff --check` passed. The pull-protection route integration test passed (1 test); web lint and format checks passed.
- Browser component tests: 51 files, 119 tests passed.
- Clean-user-facing-text audit of the updated WI-0002 spec, implementation, review, and verification records found no flagged spans; stylometry scores were low-density and scored CLEAN.

## Delivery state

PR #290 is open at `feat/col-06-codeowners` against `feat/col-05-org-teams` (PR #288). The latest update shares submission-order-aware latest-review selection across ordinary approval counts and CODEOWNERS checks, with fail-closed handling for ambiguous legacy ties, notification-safe partial request handling, missing-base fail-closed behavior, NUL-delimited rename-safe path enumeration, control-character rejection, migration-default assertions, and the diagnostic UI/documentation clarifications. The branch remains stacked on PR #288. Remote CI and the CodeRabbit verdict must be rechecked on the updated head before merge readiness. No merge was attempted.

The three-dialect migration adds an opt-in boolean with a false default and is additive. Rollback is to disable the protection setting and revert the application change; no data deletion is required.
