# WI-0002 verification record

## Local gates

- `cargo test -p oxidean-api --test pull_reviews`: 4 passed, including base-revision lookup, team fan-out, author exclusion, and stale-current-head approval behavior.
- `cargo test -p oxidean-api --test branch_protection_rpc`: 2 passed.
- `cargo test -p oxidean-api --test branch_protect_push`: 12 passed.
- `cargo test -p oxidean-db --test dialect_branch_protection`: 1 passed.
- `cargo test -p oxidean-db --test dialect_orgs`: 4 passed.
- `cargo test -p oxidean-api-core`: 228 passed, including the invalid changed-path regression.
- `cargo test -p oxidean-git`: 54 unit tests and 4 integration tests passed, including the changed-path cap that uses name-only output, caps the aggregate path payload at 1 MiB, and stops when the path limit is exceeded.
- Targeted Clippy including `oxidean-git` with `-D warnings`, Rust formatting, `git diff --check`, `make rpc-gen`, RPC client sync, OpenAPI sync, web lint/format, browser coverage, and browser component tests passed.
- Browser component tests: 51 files, 119 tests passed.
- Clean-user-facing-text audit of the updated WI-0002 spec, implementation, review, and verification records found no flagged spans; stylometry scores were low-density and scored CLEAN.

## Delivery state

PR #290 is open at `feat/col-06-codeowners` against `feat/col-05-org-teams` (PR #288). The latest branch update includes the generated RPC client types, fail-closed invalid-path handling, directory-pattern support, accurate request attribution, banned-owner filtering, and bounded changed-path enumeration. Remote CI is pending and a final review verdict on this head is not yet available. No merge was attempted.

The three-dialect migration adds an opt-in boolean with a false default and is additive. Rollback is to disable the protection setting and revert the application change; no data deletion is required.
