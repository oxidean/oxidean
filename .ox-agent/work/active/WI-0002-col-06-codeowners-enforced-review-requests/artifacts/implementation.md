# WI-0002 implementation record

## Delivered behavior

- Reads only `.github/CODEOWNERS` from a valid pull request base commit and evaluates the bounded supported glob subset, including directory-tree patterns ending in `/`. A missing file is neutral only when the base commit exists; missing base commits, control-character paths, and other invalid paths return evaluation errors instead of silently becoming uncovered. Renames evaluate both old and new paths.
- Resolves same-organization users and teams with current write access; fans team owners out to eligible reviewers; excludes the pull author.
- Keeps review requests idempotent and additive, preserves manual requests, attributes refresh requests to the actor who updated the head, and notifies successfully inserted reviewers even if a later insert fails. Pull status surfaces declared owner handles and evaluation errors, including errors when the CODEOWNERS file cannot be read.
- Adds opt-in `require_code_owner_reviews` branch protection with per-path owner approval, current-head-only approvals, and current membership/access rechecks. Equal-second reviews use a persisted, database-generated submission order; ambiguous legacy ties fail closed. Enforcement applies to pull-request merges; direct pushes remain governed by the existing push-protection rules.
- Bounds parsing, matching work, and NUL-delimited name-only changed-path enumeration (1 MiB and 1,000-path caps); rename detection is disabled so both path names are evaluated. Non-UTF-8 and control-character paths fail closed. Banned users are excluded from direct and team ownership. Owner resolution, team fan-out, and reviewer requests are also bounded. Adds all-dialect persistence, API/OpenAPI/client synchronization, documentation, settings UI, and browser coverage.

## Verification

- `cargo test -p oxidean-api --test pull_reviews`: 4 passed.
- `cargo test -p oxidean-api --test branch_protection_rpc`: 2 passed.
- `cargo test -p oxidean-api --test branch_protect_push`: 12 passed.
- `cargo test -p oxidean-db --test dialect_branch_protection`: 1 passed.
- `cargo test -p oxidean-db --test dialect_orgs`: 4 passed.
- `cargo test -p oxidean-api-core`: 228 passed.
- `cargo test -p oxidean-git`: 54 unit tests and 4 integration tests passed.
- `cargo clippy -p oxidean-git -p oxidean-api-core -p oxidean-api-pulls -p oxidean-api-repo -p oxidean-api -p oxidean-db --all-targets -- -D warnings`: passed.
- `make rpc-gen` regenerated the client DTOs for code-owner branch protection and pull status; `make rpc-sync-check`, `make openapi-sync-check`, `make web-lint`, `make web-format-check`, and `make browser-coverage-check-pr` passed.
- `make test-web-browser`: 51 files, 119 tests passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

Full workspace `make test` and remote PR CI remain outstanding.
