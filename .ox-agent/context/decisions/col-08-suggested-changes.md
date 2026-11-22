# COL-08 — suggested-change blocks in review comments

Status: accepted for WI-0004 implementation. Source: brainstorm-options.md
(Option A chosen over B client-computed apply, C deferred embedded mini-diff).

## Decisions

- **Server-authoritative apply.** `pull.suggestions.apply` and
  `pull.suggestions.apply_batch` in `oxidean-api-pulls` parse the
  ````suggestion` fence out of the stored comment body — the client never
  supplies replacement content. Client-computed apply via `repo.file.update`
  was rejected: it breaks maintainer-parity on fork heads, cannot batch into
  one commit, and races head movement.
- **Naming.** `pull.suggestions.*` is a derived-resource sub-namespace.
  Wire names stay camelCase (`apply`, `applyBatch`) matching the surrounding
  `pull.*` surface (`pull.updateBranch`, `pull.files.setViewed`) — the
  standing "new procedures are domain.snake_case" decision conflicts with
  the de-facto `pull.*`/`repo.*` convention; the in-namespace precedent wins
  here (flagged in findings).
- **Anchor semantics.** A suggestion applies only to RIGHT-side line
  anchors: it replaces `start_line..=line` (single line when `start_line`
  is absent) of the file at the *current* head tip; empty suggestion body
  deletes the range. `side=LEFT`, general/file-level comments, pending
  drafts, outdated comments, and non-open PRs are rejected so stale line
  numbers cannot be applied to changed content.
- **Permissions.** Mirror `pull.updateBranch`: Write+ on the head repo OR
  Write+ on the base repo; head-branch protection evaluated as
  `ProtectionIntent::Push` up front and re-verified by the `update` hook in
  `commit_files`. No `maintainer_can_modify` flag exists; the
  update-branch model is the precedent.
- **Write path.** `GitBackend::cat_blob` (head tip) → splice →
  `commit_files` on the head repo's bare (fork-safe) → the same post-push
  fan-out as `commit_changes` (`record_ref_updates`, push webhook,
  `synchronize_after_push` for head_sha/stale-review/Actions, mirror).
  Always commits to the existing head branch — never a new branch, never
  opens a PR.
- **Batch.** One commit across multiple comments; ranges grouped per file,
  applied descending so line numbers don't shift; overlapping ranges in a
  file reject the whole batch.
- **Commit message.** Caller-supplied or default "Apply suggestion from code
  review" / "Apply suggestions from code review"; `Co-authored-by:
  <suggester display> <noreply@oxidean.local>` trailer when the suggestion
  author differs from the applier (`FORGE_NOREPLY_EMAIL` precedent).
- **Rendering.** Client splits comment markdown into markdown/suggestion
  segments (shared fence scanner); suggestion segments render in a
  `SuggestionBlock` component — styled code block + "Apply suggestion" /
  "Commit suggestion" actions + batch checkbox. Embedded `-/+` mini-diff
  preview is deferred: comment rows store no context lines to diff against.
- **Surfaces.** pull-v2 Changes-tab comment threads and the activity-pane
  review bodies. Legacy pre-`pr-new-ui` surfaces are out of scope.
- **No DB migration** — `pull_comments` already stores every anchor field.
- **No REST endpoint** — GitHub exposes no REST apply-suggestion path.

## Open follow-ups

- Embedded mini-diff preview in suggestion blocks.
- "Applied" marker on comments (GitHub does not track this either).
- Legacy UI suggestion rendering (only if the pre-flag UI stays alive).

## Spec-stage refinements (2026-11-22)

- PAT/OAuth class: `PullsWrite` in `proc_classes` — the fail-closed
  default would otherwise lock token callers out.
- Error-code family `pull.suggestion_{not_found,invalid,out_of_range,
  overlap}` — stable `domain.reason` codes; permission denial stays
  indistinguishable `not_found` per repo ACL convention.
- `suggestion_index` (default 0) selects among multiple ````suggestion`
  fences in one comment.
- Suggestion fence info string is exactly `suggestion` (no case-fold,
  trimmed) — GitHub-exact.
