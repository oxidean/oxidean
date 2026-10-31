# Phase 12 Plan 05: Reviews Summary

**Approve / request changes / comment reviews with author ACL, dismiss, and optional review requests.**

## What Landed
- `pull.reviews.*` + `pull.reviewRequests.*` RPC and DB helpers
- Author cannot Approve / Request changes; latest submissions stack; dismiss Write+
- Conversation-tab review panel UI

## Verification
- `cargo nextest run -p oxidean-api -E 'test(pull_reviews)'`
- `make web-lint` / `make web-format-check`
- Vitest pulls integration (reviews stub green)

## Commits
- `f88d1b7` feat(12-05): pull reviews submit, dismiss, and request UI
