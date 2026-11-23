# WI-0002 independent review follow-up

The read-only review compared the CODEOWNERS implementation with the WI-0002 specification and ADR.

## Findings actioned

- Updated the Rust RPC client generator and regenerated `packages/api-client/src/index.ts` so branch-protection inputs/results and pull CODEOWNERS status match the Rust DTOs.
- Changed invalid repository paths from silently uncovered to an evaluation error; added a regression test.
- Added support for a trailing `/` directory rule and documented it in the specification.
- Recorded the actor that refreshes the pull head as `requested_by` for newly inserted automatic review requests.
- Excluded banned users from direct and team owner eligibility.
- Switched CODEOWNERS changed-path enumeration to a NUL-delimited name-only Git diff capped at 1 MiB and the configured path count, instead of materializing patch content. Disabled rename detection so both old and new paths are checked; non-UTF-8 and control-character paths fail closed.
- Distinguished an absent CODEOWNERS file on a valid base commit from an unavailable base commit; the latter now blocks required evaluation and its diagnostic is rendered even when no file is present.
- Serialized review-order allocation with review insertion. The latest-review selector now prioritizes that sequence over transaction-start timestamps; this matters on PostgreSQL, where an earlier transaction may insert after a later one. New ordered reviews take precedence over legacy rows, while legacy-to-legacy timestamp ties fail closed.
- Preserved notifications for successful review-request inserts when a later insert fails; strengthened the notification and stale-path regression assertions.
- Verified the false default in each dialect migration and the applied SQLite column metadata. Documented that code-owner enforcement applies to pull merges, not direct pushes.

## Findings assessed against the contract

- Slash-free patterns are intentionally repository-root-relative, as required by the specification; this differs from some `.gitignore` matching expectations.
- CODEOWNERS and changed paths are intentionally evaluated at the PR's stored base commit, as required by the source-selection contract.
- Pull status returns declared user/team handles without expanding team membership. Team owners still fan out to individual review requests, so the existing review-request list exposes eligible reviewer usernames; this privacy tradeoff is documented in the ADR and PR description.
- Creation notifies requested reviewers through the existing `pr_opened` fan-out; head refresh emits `pr_review_requested` for newly added requests.

## Low-severity notes assessed

- Owner handles resolve using the canonical spelling stored by the account/team records; case-insensitive username resolution is not part of this contract.
- Evaluation intentionally uses the pull's stored base commit. A base-only advance is observed when the pull is next synchronized or edited.
- Existing reviewer requests remain across new commits; repeated refreshes do not re-notify already-requested owners, matching the additive/idempotent request contract.

## Remaining external gates

The PR is stacked on PR #288. Full remote CI and a final review verdict on the latest pushed head are still required before merge readiness. No merge was attempted.
