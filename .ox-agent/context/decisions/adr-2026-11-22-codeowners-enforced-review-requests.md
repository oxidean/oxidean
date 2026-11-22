# ADR: CODEOWNERS review requests and protection (COL-06 / WI-0002)

Status: proposed in implementation
Date: 2026-11-22
Context: roadmap COL-06 (COLLAB-V2-03). COL-05 supplies organization teams; pull requests already support individual review requests and branch protection already evaluates required reviews.

## Decisions

1. **Canonical file and source revision.** Read only `.github/CODEOWNERS` from the pull request's base branch. An absent file means no automatic requests and no owner-specific merge requirement. Do not read a CODEOWNERS file introduced only by the pull request being evaluated.
2. **Bounded GitHub-style syntax.** Support comments/blank lines, repository-root-relative glob patterns, leading `/`, `*`, `**`, `?`, and character classes, followed by one or more owner tokens. Later matching rules replace earlier owner sets for a path. Unsupported negation/escaping syntax and malformed patterns are invalid rather than interpreted differently from their apparent meaning.
3. **Owner identities.** Support `@username` and same-organization `@org/team-slug` owners. Users and teams must resolve and have write-capable access to the repository; teams must belong to the repository's owning organization. Owner identity is validated when rules are evaluated, not when the repository is created.
4. **Review requests.** On PR creation and changed-file refresh, resolve matching owners and request them through the existing review-request and notification flow. Team requests fan out to current team members who have write-capable repo access. Never request the pull request author; a matched path with no other eligible owner produces an evaluation error and cannot satisfy required owner approval. The operation is idempotent; it does not duplicate notifications on unchanged owner sets. Existing manually requested reviewers are preserved. Auto-requests are additive and can be removed through the existing reviewer-management surface.
5. **Optional enforcement.** Add a `require_code_owner_reviews` branch-protection setting, default false. When enabled, every changed path covered by a valid rule must have an approving review from at least one current owner for that path. Owner approvals must be on the current head even when ordinary stale-review dismissal is disabled. Existing required approval counts and stale-review rules continue to apply independently. A current team member may satisfy a team owner; current membership and write access are checked at merge evaluation.
6. **Fail closed when enabled.** A malformed pattern, unresolved owner, or file/owner evaluation error produces a structured protection blocker when the setting is enabled. A path not covered by any rule does not acquire a synthetic owner requirement. With the setting disabled, parse/resolve failures do not block merges, but should be surfaced through the pull-request status surface.
7. **No reviewer snapshot table.** Re-evaluate owners against the base revision and current changed-path list at merge time. Persist only the existing user review requests; do not create a parallel team-request or owner-snapshot store in this tranche. This avoids new migration state and prevents membership snapshots from becoming stale.
8. **Bounded resource use.** Limit CODEOWNERS input to 65,536 bytes, 3,000 lines, 255 characters per pattern, and 20 owners per line; limit changed paths to 1,000, aggregate pattern-token × path-character work to 50,000,000 units, distinct declared owners to 500, team-member expansion to 200 members per team, and unique reviewer requests to 500. Parse once per evaluation and match each changed path against compiled patterns.
9. **Status privacy.** Pull status exposes the declared `@username` and `@org/team` handles, not the expanded team-member usernames.
10. **Scope.** No REST mirror, team-wide approval quorum, branch protection UI redesign, CODEOWNERS discovery in root/docs, and no ruleset changes beyond the branch-protection boolean. Update generated RPC clients and documentation in the same PR.

## Consequences

- PR creation/update, reviewer requests, and merge evaluation share one owner resolver so requests and enforcement cannot use different matching rules.
- Manual and automatic requests are represented in the existing user request model. Pull CODEOWNERS status displays declared handles rather than team-member expansions.
- Team-owner fan-out uses individual user requests, so the existing Read+ review-request list shows those requested usernames. A future first-class team request model is needed to preserve private team membership in that surface.
- The prior v1 branch-protection decision that excluded CODEOWNERS is superseded for COL-06. Existing protections remain unchanged until an administrator opts in.
- Team membership changes take effect immediately for merge evaluation; requested users are not silently removed when membership or patterns change.

## Deferred questions

- Whether to add broader GitHub CODEOWNERS syntax (multiple candidate files, negation, escaped `#`) after compatibility evidence.
- Whether team requests should become first-class principals in a future API instead of expanding to eligible members.
