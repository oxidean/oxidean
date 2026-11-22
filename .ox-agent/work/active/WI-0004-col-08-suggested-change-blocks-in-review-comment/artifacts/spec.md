# WI-0004 spec — COL-08 suggested-change blocks in review comments

GitHub-parity core: ````suggestion` fenced blocks in diff-anchored review
comments render as suggested changes; a signed-in user with push rights on
the PR head applies one with "Apply suggestion"/"Commit suggestion" or
batches several into a single commit on the head branch.

Source decisions: `context/decisions/col-08-suggested-changes.md`.

## Scope

In:

- New RPCs `pull.suggestions.apply` (single comment) and
  `pull.suggestions.apply_batch` (multiple comments, one commit) in
  `oxidean-api-pulls` + `oxidean-core` wire types + `rpc-gen` client +
  `proc_classes` registration (`PullsWrite`).
- Server-side ````suggestion` fence extraction shared by both RPCs.
- Line-range replacement of the head-tip file and a signed commit on the
  head branch with the full push fan-out (head_sha sync, stale-review
  dismissal, push + `pull_request: synchronize` webhooks, Actions,
  mirrors) — identical to a real `git push`.
- Web UI on the pull-v2 surface: suggestion fences render as styled
  suggestion blocks (not plain code fences) inside review comments;
  per-block "Apply suggestion" → confirm/commit-message dialog; batch
  selection ("Add to batch") + one "Commit suggestions" action.
- Rust integration tests for the RPC contract; vitest for the fence
  parser; happy-dom integration coverage for the suggestion block UI.

Out:

- Embedded `-/+` mini-diff preview of the suggestion (needs context lines
  the comment model does not store) — follow-up.
- Legacy (pre-`pr-new-ui`) PR surfaces.
- REST endpoint (GitHub has none), applied-state tracking, suggestions in
  issue/general comments (GitHub only supports them on diff lines).
- `maintainer_can_modify` flag — the update-branch permission model is
  the precedent; the flag can tighten it later.

## Contracts

### Suggestion fence grammar (shared semantics, Rust + TS)

A ````suggestion` block is a fenced code block with info string
`suggestion` (exactly; `suggestion` with trailing spaces counts):

- Opener: 3+ backticks or tildes, info string trims to `suggestion`
  (case-insensitive match on the info string is NOT applied — GitHub is
  exact-lowercase; keep `suggestion` only).
- Closer: a run of the same fence char, at least as long as the opener.
- Body: lines between opener and closer, joined with `\n`. The rendered
  suggestion text drops a single trailing newline (fence bodies include
  the line's newline).

`parse_suggestion_blocks(body) -> Vec<{ body, block_index }>` in Rust
(`oxidean-api-pulls::suggestions` or core helper) and a mirror in
`apps/web/src/lib/` for rendering. Rust is authoritative for apply; the
TS twin only needs the same segmentation for display.

### `pull.suggestions.apply` — PullsWrite

Request `{owner, name, number, comment_id, suggestion_index?, message?}`.
Response `{commit_sha, branch}`.

Flow:
1. `require_verified`; `acl::resolve_for_read` on base repo; load pull;
   reject unless `state == "open"` (`pull.invalid_state`).
2. Load `pull_comments` row; must belong to this pull
   (`pull.comment_not_found`); must be line-anchored RIGHT
   (`path` + `line`, `side == "RIGHT"`, `start_line` optional
   ≤ `line`), non-pending (`pull.suggestion_invalid`).
3. Parse ````suggestion` fences from `row.body`; select index
   `suggestion_index` (default 0); out-of-range →
   `pull.suggestion_not_found`.
4. Resolve head repo (update_branch `resolve_head` pattern — same-repo
   fast path, else `find_repository_by_id` + `owner_ref_for_repo`;
   `pull.head_unavailable` when gone).
5. Permission: `meets(base.capability, Write)` OR `meets(head_cap,
   Write)` — else `not_found()` (indistinguishable denial, matching
   update_branch). Head-branch protection: `evaluate_push(eff, Push,
   head_cap)`.
6. `cat_blob(head_bare, head_tip_sha, path)` → UTF-8 file text (binary /
   non-UTF-8 → `pull.suggestion_invalid`); splice lines
   `start_line..=line` → suggestion body (empty = delete). Line range
   beyond file length → `pull.suggestion_out_of_range`.
   CRLF files: splice operates on `\n`-split lines; existing CRLF
   terminator is preserved per retained line (no normalization).
7. Commit via `ctx.git.commit_files(head_bare, head_ref, head_tip_sha,
   message, [FileChange::Upsert{path, new_bytes}], author, email,
   web_flow_key, capability_env_label(head_cap))`.
   Message: req `message` trimmed non-empty, else
   `Apply suggestion from code review`. When the suggestion author ≠
   applier, append `\n\nCo-authored-by: {author_display}
   <{FORGE_NOREPLY_EMAIL}>`.
8. Fan-out identical to `commit_changes`'s tail minus branch-create/PR
   bits: `record_ref_updates`, `webhook::dispatch::notify_push`,
   `pull::synchronize_after_push` (refreshes `pull.head_sha`, stale
   review dismissal, `synchronize` webhook + Actions), mirror notify.
   Actions push-notify skipped for job-token callers (same guard).

### `pull.suggestions.apply_batch` — PullsWrite

Request `{owner, name, number, comment_ids[], message?}`. Response
`{commit_sha, branch, applied}`.

Same gates; every comment must individually satisfy step-2 checks and
carry a ````suggestion` block at index 0. Comments grouped by `path`;
within a file, ranges must not overlap (`pull.suggestion_overlap`).
Ranges applied in descending `start_line` order so earlier splices don't
shift later anchors. One `FileChange::Upsert` per touched file, one
commit. Default message `Apply suggestions from code review`;
Co-authored-by trailers for each distinct suggestion author ≠ applier.

### Error codes (stable `domain.reason`)

`pull.suggestion_not_found` (no fence / bad index),
`pull.suggestion_invalid` (LEFT side, no line anchor, pending, binary),
`pull.suggestion_out_of_range` (lines past EOF),
`pull.suggestion_overlap` (batch), plus existing
`pull.comment_not_found`, `pull.invalid_state`, `pull.head_unavailable`,
`pull.protected`/`repo.push_denied` from `evaluate_push`.

### Web surface

- `apps/web/src/lib/suggestions.ts` — `parseSuggestionBlocks(markdown)`
  → segments `{type:"markdown"|"suggestion", text}`. Unit-tested.
- `SuggestionBlock` component — suggestion chrome (header label +
  apply affordances) around the code body (plain `<pre><code>`; Shiki
  per-suggestion highlighting is optional/nice-to-have).
- `CommentSuggestionBody` (or `SuggestionMarkdownBody`) — renders
  segments: markdown via `MarkdownBody`, suggestions via
  `SuggestionBlock`. Used by pull-v2 `CommentNote` (diff threads) and
  activity-pane review bodies where the comment carries a RIGHT-side
  line anchor.
- Apply button gated on `canInteract` + `canWrite` (Write+ on base repo
  — same prop the diff view already threads; fork-head write is the
  server's check, button may show then server-rejects with the stable
  error). Disabled for `pending` comments, LEFT-side anchors, closed
  PRs.
- "Apply suggestion" opens the commit dialog (message field prefilled
  "Apply suggestion from code review") → `apiClient.pull.suggestions.apply`
  → invalidate `pull.comments` + `pull.commits` + `pull.files` queries.
- Batch: "Add to batch" toggle per block; batch bar shows count +
  "Commit suggestions" (dialog, default message plural) → `apply_batch`.
  Batched state lives in `pull-v2-changes.tsrx` (per-tab local state).

## Acceptance criteria

AC-1 `pull.comments.create` with ````suggestion` fence on RIGHT-side line
    anchor → `pull.suggestions.apply` returns `commit_sha`; `repo.blob`
    at head shows the anchored lines replaced by the suggestion body.
AC-2 Single-line comment (no `start_line`) → exactly `line` replaced.
AC-3 `start_line..line` range → whole range replaced; empty suggestion
    body deletes the range.
AC-4 LEFT-side, general, file-level, pending, closed-PR targets each
    reject with the mapped error code.
AC-5 Caller lacking Write on both head and base repo → `not_found`-class
    denial; protected head branch rejects per `evaluate_push`.
AC-6 Applying commits as the applier (signed web-flow commit) with
    `Co-authored-by` trailer when suggestion author differs; pull
    `head_sha` advances; `synchronize` fan-out fires (assert via
    updated `pull.get` head_sha and/or activity row).
AC-7 `apply_batch` over 2+ comments in the same file yields ONE commit
    with both replacements; overlapping ranges reject; comments in
    different files batch in the same commit.
AC-8 Multi-fence comment: `suggestion_index` selects; missing index →
    `pull.suggestion_not_found`.
AC-9 Out-of-range anchor (`line` > EOF) → `pull.suggestion_out_of_range`;
    outdated comments still apply at the current head tip.
AC-10 Web: a ````suggestion` block in a diff comment renders as a
    suggestion block (not a generic fence) with the Apply affordance for
    Write+ viewers; markdown around it renders unchanged.
AC-11 `make rpc-gen` output committed; `rpc-sync-check` clean;
    `pull.suggestions.*` listed in PROCEDURES + dispatch (parity test)
    + `proc_classes` (PullsWrite) + TS client `api.pull.suggestions.*`.
AC-12 Fork-head PR: base-repo maintainer can apply to the fork head
    (update-branch parity); commit lands on the fork's head branch.

## Files expected to change

- `crates/oxidean-api-pulls/src/suggestions.rs` (new), `dispatch.rs`,
  `lib.rs` (PROCEDURES).
- `crates/oxidean-core/src/pull_types.rs` (request/response types),
  `proc_classes.rs`.
- `crates/oxidean-api/src/bin/rpc_gen.rs` + `make rpc-gen` output.
- Maybe `oxidean-api-core` — if the post-push fan-out tail is factored
  out of `commit_changes` for reuse (only if clean; otherwise replicate
  the ~6 calls like update_branch does).
- `apps/web/src/lib/suggestions.ts` (new), `markdown-body.tsrx` or a new
  `suggestion-markdown-body.tsrx`, `pull-v2-diff-file.tsrx`,
  `pull-v2-changes.tsrx`, `pull-v2-activity-pane.tsrx`.
- `crates/oxidean-api/tests/pull_suggestions.rs` (new);
  `apps/web/src/lib/suggestions.test.ts` (new); a happy-dom integration
  test beside pull-v2 suites.
- `docs/API.md` procedures list if it enumerates `pull.*` (check).
