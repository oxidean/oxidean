# Specification: COL-06 CODEOWNERS-enforced review requests

## Goal

Use repository ownership rules to request the right pull-request reviewers and, when explicitly configured, require a code-owner approval before merge. The feature builds on COL-05 organization teams and current individual review requests.

## Scope

- One canonical source: `.github/CODEOWNERS` from the pull request base branch.
- Parse a bounded GitHub-style subset: blank/comment lines; root-relative patterns; `*`, `**`, `?`, character classes, and leading `/`; one or more owners; later matching lines replace earlier owners for that path.
- Owners: existing `@username` or `@<repository-owner>/<team-slug>` from the owning organization. Owners must have write-capable access to the repository; team owners are expanded to current eligible members for user-request and notification compatibility.
- Automatically request owners for matching changed paths on PR creation and whenever the changed-file set is refreshed.
- Add `require_code_owner_reviews` to branch protection, default false. When enabled, each changed path covered by CODEOWNERS needs an approving review from one of that path's current owners. Existing approval-count and stale-review requirements remain independent.
- Regenerate the RPC client and update user-facing branch-protection documentation and UI.

## Out of scope

- Root or `docs/` CODEOWNERS fallback locations; negation/escaped syntax; non-owner team review API principals; team-wide approval quorum; REST mirror; ruleset redesign; auto-removal of existing reviewer requests.

## Contracts

1. **Source selection:** read only `.github/CODEOWNERS` at the base commit of the PR. A missing file means no CODEOWNERS requests and no code-owner merge requirement.
2. **Pattern semantics:** patterns are repository-root-relative; leading `/` is equivalent to root anchoring; `*` does not cross `/`; `**` may cross `/`; `?` matches one non-slash character; character classes match one non-slash character. Blank lines and lines whose first non-space character is `#` are ignored. Unsupported negation or escaping is invalid. The last valid matching line supplies owners for a path.
3. **Validation:** malformed pattern or owner tokens are retained as evaluation errors, never silently mapped to another owner. Unknown user/team or a user/team without write access cannot become an owner. When enforcement is enabled, any evaluation error blocks merge with a structured reason. When enforcement is disabled, requests for resolvable owners may still be issued and errors are exposed in the PR status surface without blocking merge.
4. **Review requests:** requests are computed from the resolved owner set across changed paths. Repeated recalculation with an unchanged set has no database or notification side effect. Existing manual requests remain. Recalculation may add newly matching eligible owners but does not remove existing requests.
5. **Team owner expansion:** include current team members who individually have write-capable access. At merge evaluation, a team owner is satisfied only by an approving reviewer who is still a team member and still has write-capable access.
6. **Merge check:** when `require_code_owner_reviews` is false, current merge behavior is unchanged. When true, every changed path matched by at least one valid rule requires an approval from one of its rule's owners; an uncovered path has no synthetic owner requirement. Pending, commented, dismissed, or stale approvals do not satisfy the condition. Return a stable blocker reason and preserve other protection reasons.
7. **Limits:** cap CODEOWNERS file bytes, line count, owner count per line, and changed path count using documented constants; reject excess input deterministically. Compile patterns once per evaluation and avoid repeated parsing per path.
8. **Persistence and compatibility:** migrate all supported database dialects for the branch-protection boolean with default false; preserve existing rule rows and existing clients through serde defaults. No new request or owner snapshot table in this tranche.

## Acceptance criteria

- AC1: A PR uses the base commit's `.github/CODEOWNERS`, even if the PR head changes or adds a different CODEOWNERS file.
- AC2: Unit tests cover supported glob syntax, path separators, last-match precedence, comments/blank lines, invalid patterns, and unsupported negation/escape behavior.
- AC3: PR creation and changed-file refresh request matching eligible users and team members; duplicates are not re-notified; manual requests remain intact.
- AC4: Missing CODEOWNERS has no effect. Unknown owners and owners without write access are surfaced as evaluation errors and never requested.
- AC5: With the branch-protection flag off, existing merge tests remain unchanged. With it on, each matched path requires at least one current owner approval; a non-owner approval does not count.
- AC6: Team approvals use membership and write access at evaluation time; removing a reviewer from the team or revoking their write access makes that approval insufficient.
- AC7: Invalid CODEOWNERS evaluation blocks merge only when enforcement is enabled and returns a stable structured blocker alongside other protection failures.
- AC8: All dialect migrations and persistence tests pass; generated API client and OpenAPI outputs are in sync.
- AC9: Branch-protection settings UI exposes the opt-in toggle accessibly, defaults it off, and browser tests cover the interaction and rendered state.
- AC10: Relevant Rust/API/database tests, `make rpc-gen`, `make rpc-sync-check`, `make web-lint`, `make web-format-check`, and browser-coverage gates pass.

## Rollout and rollback

Default the new setting to false so deploys do not change merge behavior. Operators can enable it per protected branch after adding and validating `.github/CODEOWNERS`. Rollback is disabling the setting; the migration remains additive and the parser can remain available for review requests.
