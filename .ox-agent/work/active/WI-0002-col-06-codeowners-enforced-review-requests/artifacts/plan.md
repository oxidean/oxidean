# Implementation plan: COL-06 CODEOWNERS

## Stage 1 — Parser and matching contract

- Add a pure CODEOWNERS parser/matcher in the existing pull-request or git-domain utility layer; avoid database access in parsing.
- Red tests first for comments, blanks, root anchoring, `*` vs `**`, `?`, character classes, last-match precedence, unsupported negation/escaping, malformed lines, and configured size/count/work/fan-out limits.
- Compile patterns once and return structured matches plus parse/owner-resolution errors.
- Gate: unit suite passes; parser behavior matches the specification exactly.

## Stage 2 — Resolve owners and request reviewers

- Resolve user and same-org team owners against current users, organization membership, and write-capable repo access.
- Add the owner request resolver that reads `.github/CODEOWNERS` from the PR base revision and matches the current changed-path set.
- Wire PR creation and changed-file refresh to the existing review-request/notification path; keep recalculation idempotent and preserve manual requests.
- Add integration tests for user/team resolution, ineligible/unknown owners, author exclusion, bounded team fan-out, duplicate recomputation, manual-request preservation, and update-trigger behavior.
- Gate: existing pull-request review-request tests and new notification/integration tests pass.

## Stage 3 — Branch-protection persistence and merge evaluation

- Add `require_code_owner_reviews` with default false to protection types, all DB dialect migrations/queries, RPC serialization, and evaluation results.
- Evaluate each matched changed path against approving current owners; include current team membership and write access.
- Add structured fail-closed blockers for invalid CODEOWNERS evaluation only when the option is enabled; preserve existing review-count and stale-review behavior.
- Add DB dialect tests and merge-gate tests for disabled/enabled, non-owner, owner, team membership changes, uncovered paths, malformed file, and coexisting blockers.
- Gate: all relevant `oxidean-db` dialect tests and pull-protection integration tests pass.

## Stage 4 — Settings and client surface

- Update the existing branch-protection settings UI with an accessible opt-in toggle and default-off behavior.
- Regenerate `packages/api-client`; update OpenAPI/docs from canonical Rust producers.
- Add interaction/browser coverage and update route/browser coverage manifests if required.
- Gate: `make rpc-gen`, `make rpc-sync-check`, `make web-lint`, `make web-format-check`, and focused browser tests pass.

## Stage 5 — Full verification and delivery

- Run targeted Rust tests, dialect tests, API generation checks, web gates, and the full project test command where feasible.
- Inspect generated outputs and the final diff; update the work-item ship record and roadmap marker.
- Gate: relevant CI-equivalent checks pass, no secrets or attribution are present, PR scope matches this specification, and review threads are resolved.

## Dependencies and risks

- Stage 2 depends on the COL-05 team API/database model. This worktree is based on the current COL-05 PR head; the resulting PR will be stacked on that branch until COL-05 lands.
- Stage 3 should reuse existing branch-protection evaluation rather than create a second merge gate.
- Owner access and team membership must be rechecked at merge time; do not trust stale request records as approval authority.
- Avoid migration/job/workflow changes beyond the protection boolean; if a required CI job changes, update the main ruleset check list in the same PR.
