# Brainstorm: CODEOWNERS behavior

## Option A — automatic reviewer requests only

Parse a repository CODEOWNERS file and create normal review requests for owners of changed paths. Keep existing branch-protection approval rules unchanged.

- Benefits: smallest implementation; composes with existing review-request persistence and notifications.
- Costs: a maintainer can merge without an owner approval unless a separate required-review policy already happens to block it.
- Open questions: how team membership maps to the user-only request store; when requests are recalculated.

## Option B — automatic requests plus optional owner-approval enforcement (recommended)

Resolve owners from the base branch's CODEOWNERS file, request matching users/teams when a PR is created or its changed-file set changes, and let branch protection optionally require an approval from a matching owner before merge. Existing review-count and stale-review policy remain independent.

- Benefits: matches the COLLAB-V2-03 “CODEOWNERS-enforced reviews” intent while keeping enforcement opt-in through branch protection; leaves unprotected repositories unaffected.
- Costs: extends merge evaluation and review persistence; team membership changes and deleted owners need defined behavior.
- Open questions: exact grammar subset and how requested teams are represented without losing their identity.

## Option C — store resolved owners on each pull request

At creation/update, resolve patterns and snapshot owner identities on the PR. Use the snapshot for reviewer requests and merge checks.

- Benefits: stable evaluation even if CODEOWNERS or team membership later changes; no repeated parse in merge evaluation.
- Costs: requires migrations and refresh semantics on branch/file updates; snapshots can become stale and create a second source of truth.

## Convergence

Choose Option B, with a deliberately bounded GitHub-style subset and no owner snapshot table. Use the base branch's `.github/CODEOWNERS` as the sole supported location; patterns are evaluated against changed paths, later matching rules override earlier owners, and the final owner set is requested. Support repository users and same-organization teams, reject malformed owner tokens with a clear validation error, and ignore unsupported pattern syntax rather than silently treating an unknown owner as a valid reviewer. Recompute on pull creation and file-set changes. Preserve manual reviewers; remove only CODEOWNERS-derived requests that are no longer matched.

Enforcement remains opt-in: add a branch-protection switch to require at least one approval from a matching owner. Existing approval-count and stale-review behavior remain unchanged. A team approval is valid only when the reviewer is a current member of that team at merge evaluation time. If no valid owner matches a changed path or the CODEOWNERS file is invalid, do not fail open when the rule is enabled; report a structured merge blocker and require repository admins to fix the file.

## Risks to resolve in specification

- Ensure a CODEOWNERS file cannot request users or teams outside the repository's owning organization.
- Avoid recursive or quadratic glob matching on large file lists; bound file size, line count, and path count using existing repository limits.
- Keep manual review requests distinguishable from derived requests so recomputation does not remove user-authored requests.
- Avoid mutating review-request state or sending duplicate notifications on repeated equivalent updates.
