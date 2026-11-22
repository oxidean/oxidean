# WI-0002 independent review follow-up

The read-only review compared the CODEOWNERS implementation with the WI-0002 specification and ADR.

## Findings actioned

- Updated the Rust RPC client generator and regenerated `packages/api-client/src/index.ts` so branch-protection inputs/results and pull CODEOWNERS status match the Rust DTOs.
- Changed invalid repository paths from silently uncovered to an evaluation error; added a regression test.
- Added support for a trailing `/` directory rule and documented it in the specification.
- Recorded the actor that refreshes the pull head as `requested_by` for newly inserted automatic review requests.

## Findings assessed against the contract

- Slash-free patterns are intentionally repository-root-relative, as required by the specification; this differs from some `.gitignore` matching expectations.
- CODEOWNERS and changed paths are intentionally evaluated at the PR's stored base commit, as required by the source-selection contract.
- Pull status returns declared user/team handles without expanding team membership. Team owners still fan out to individual review requests, so the existing review-request list exposes eligible reviewer usernames; this privacy tradeoff is documented in the ADR and PR description.
- Creation notifies requested reviewers through the existing `pr_opened` fan-out; head refresh emits `pr_review_requested` for newly added requests.

## Remaining external gates

The PR is stacked on PR #288. Full remote CI and a final review verdict on the latest pushed head are still required before merge readiness. No merge was attempted.
