# WI-0004 brainstorm — candidate directions

Direction space for ````suggestion` blocks in review comments. The fixed
GitHub-parity target: suggestion fences render as suggested changes;
one-click apply commits the replacement to the PR head branch; optional
batch apply.

## Option A — server-authoritative apply (RECOMMENDED)

Two new RPCs in `oxidean-api-pulls`:

- `pull.suggestions.apply` `{owner, name, number, comment_id,
  suggestion_index?, message?}` → `{commit_sha, branch}`
- `pull.suggestions.applyBatch` `{owner, name, number, comment_ids[],
  message?}` → `{commit_sha, branch, applied}`

Server parses ````suggestion` fences out of the stored comment body
(single source of truth — the client cannot spoof suggestion content),
reads the head-tip file via `cat_blob`, splices `start_line..=line`,
commits via `GitBackend::commit_files` on the head repo bare (fork-safe —
head bare, not base), then runs the same post-push fan-out
`update_branch`/`commit_changes` use (`record_ref_updates`,
`synchronize_after_push`, push webhook, Actions, mirror).

Permission model mirrors `update_branch` exactly: Write+ on head repo OR
Write+ on base repo (maintainer-update parity — no
`maintainer_can_modify` column exists, and update_branch already chose
this model), head-branch protection as `ProtectionIntent::Push`,
`evaluate_push` + the `update` hook re-check.

UI: a `parseSuggestionBlocks(body)` helper splits comment markdown into
segments; `SuggestionBlock` component renders the suggestion source in a
styled block (suggestion chrome + highlighted or plain code) with an
"Apply suggestion" button and a "Commit suggestion" message dialog;
batch = checkbox/"Add to batch" per block plus one commit action.
Surfaces: pull-v2 diff-file comment threads (primary) + activity-pane
review bodies (same component reused). Legacy pre-flag UI out of scope.

Pros: atomic single commit per apply (batch = one commit); permission +
protection identical to existing write paths; suggestion content
server-verified; testable end-to-end at the RPC layer.
Cons: new RPC surface + codegen touch; suggestion parsing duplicated
Rust/TS (Rust for apply, TS for render) — kept trivial (fence scan).

## Option B — client-computed apply via repo.file.update

Client parses the suggestion, fetches the file via `repo.blob`, splices
client-side, calls `repo.file.update` targeting the head branch.

Pros: zero new RPC surface.
Cons: two round trips → TOCTOU on head movement (file.update's base-sha
guard would mostly just fail, retryable but racy); no single-commit
batch (one commit per file); permission path diverges — repo.file.update
resolves capability on the *target* repo, so base-repo maintainers could
NOT apply to fork heads (breaks the update_branch parity model); the
client dictates replacement content (server trusts it). Rejected — the
maintainer-parity gap and missing batch commit are real regressions vs
GitHub.

## Option C — Option A + embedded mini-diff rendering

Same server design; UI renders each suggestion as a real `-old/+new`
mini-diff (fetch the file's current lines, diff client-side).

Pros: closest visual parity to GitHub's "suggested change" card.
Cons: needs the replaced lines client-side — either embed the anchored
hunk (comment rows carry no context lines today; deriving them from
pull.files patches is fragile for outdated comments) or an extra blob
fetch per suggestion. Defer — render the suggestion body as a styled
code block in v1 (matches GitHub's fallback when context can't load);
a diff preview is a cheap follow-up.

## Decisions surfaced for specification

- RPC naming: `pull.suggestions.apply` / `pull.suggestions.applyBatch`
  (new sub-namespace) vs `pull.comments.applySuggestion` — leaning
  `pull.suggestions.*` (a suggestion is a derived resource, not a comment
  mutation).
- `suggestion_index` (default 0) selects among multiple fences.
- Apply at *current* head tip even when `outdated` (GitHub behavior);
  reject `side=LEFT`, general/file-level comments, closed/merged PRs,
  pending drafts, and comments without a parseable fence.
- Batch: group by path; reject overlapping ranges; apply ranges
  descending so line numbers don't shift; empty suggestion = delete range.
- Default message "Apply suggestion from code review" /
  "Apply suggestions from code review"; append `Co-authored-by:
  <suggester-display> <noreply@oxidean.local>` when author ≠ applier —
  FORGE_NOREPLY_EMAIL precedent exists.
- No DB migration needed — all anchor data already stored.
- No REST endpoint (GitHub has none for this).
