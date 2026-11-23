# Discovery: COL-06 CODEOWNERS-enforced review requests

## Existing product surface

- The roadmap tracks COL-06 as `CODEOWNERS-enforced review requests` (COLLAB-V2-03); the work item has no further acceptance criteria yet.
- Team support is being introduced by the adjacent COL-05 work item. Teams are org-local, have stable slugs, contain org members, and can receive repository access grants. Team slugs follow `^[a-z0-9][a-z0-9-]{0,99}$`.
- Pull requests already support individual review requests through `pull.reviewRequests.{list,add,remove}`. Adding a request is write-gated, stores a user id, and sends the existing `pr_review_requested` notification.
- Pull creation already gathers requested reviewer ids and includes them in its notification fan-out. Reviews, requested reviewers, branch-protection rules, and merge evaluation are separate existing surfaces.
- The COL-05 implementation adds the team domain and RPCs but does not provide CODEOWNERS parsing or team-based pull-review requests.
- No CODEOWNERS implementation or tracked file is present in the current worktree. Archived v1 planning explicitly deferred CODEOWNERS from both pull-request and branch-protection phases; it is not a binding current design.

## Relevant constraints and patterns

- Keep dialect-specific SQL inside `oxidean-db`; follow the current per-dialect migration and test patterns.
- Change Rust RPC producers and regenerate the API client with `make rpc-gen`.
- Security-relevant mutations use existing authorization helpers and record audit events without making audit failures fail the operation.
- User-facing screens are Octane `.tsrx`; UI work requires `make web-lint`, `make web-format-check`, and browser coverage where applicable.
- PRs require green CI and resolved review threads before merge. No workflow job changes are expected for this work item.

## Design questions for the next stages

1. Separate two meanings that the title alone does not settle: automatically requesting reviewers based on CODEOWNERS, and requiring an owner approval before a protected pull request can merge.
2. Define the accepted CODEOWNERS path and syntax, including user and team owners, precedence, invalid entries, and whether unsupported GitHub syntax is rejected or ignored.
3. Define when ownership is evaluated (PR creation, changed-file updates, or both), how obsolete requests are handled, and whether existing manual requests are preserved.
4. Reuse existing review-request storage/notifications where possible; avoid adding persistent team-request rows unless the existing user-request model cannot represent the required behavior.

## Recommended investigation

Specify the smallest coherent slice against the new team model: parse the repository's canonical CODEOWNERS file, resolve supported owners, request the matching team members through the existing review-request path, and state explicitly whether merge enforcement is part of this work item or a separately configured branch-protection capability. Verify matching and failure behavior with tests before implementation.