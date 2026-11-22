# WI-0004 discovery — COL-08 suggested-change blocks in review comments

Roadmap line (`.ox-agent/context/product/roadmap.md:73`): "COL-08
Suggested-change code blocks in review comments." No prior spec; GitHub-parity
core bounds the scope.

## Feature shape (GitHub parity baseline)

- ````suggestion` fenced block inside a diff-anchored review comment renders as
  a suggested change (styled block, distinct from a normal code fence).
- One-click apply ("Commit suggestion") replaces the comment's anchored line
  range in the head-branch file with the suggestion body and commits to the PR
  head branch; default message "Apply suggestion from code review".
- Batch apply: collect multiple suggestions into one commit.
- LEFT-side (removed-line) comments cannot carry suggestions on GitHub.

## Codebase touchpoints found

### Review comments (server)

- `crates/oxidean-api-pulls/src/comments.rs` — `pull.comments.*` handlers.
  `PullCommentRow`/`PullCommentPublic` carry `path`, `side` (`LEFT`/`RIGHT`),
  `line`, `start_line`, `commit_sha`, `outdated`, `resolved`, `review_id`,
  `pending`. Validation already enforces: line anchors need path+side+line;
  `start_line <= line`; 1-based lines.
- `crates/oxidean-api-pulls/src/dispatch.rs` + `lib.rs` `PROCEDURES` — new RPCs
  register in both (parity test enforces match).
- `crates/oxidean-core/src/pull_types.rs` — request/response wire types
  (serde + schemars).
- `crates/oxidean-api/src/bin/rpc_gen.rs` — hand-maintained TS emitter;
  new types + `pull.suggestions.*` client methods added there, then
  `make rpc-gen` regenerates `packages/api-client`.

### Git write path

- `oxidean-api-core::repo::files::commit_changes` (GIT-19) — full push-equivalent
  fan-out: signed commit via `ctx.git.commit_files`, `record_ref_updates`,
  push webhook, `synchronize_after_push` (updates `pull.head_sha`, stale-review
  dismissal, `synchronize` webhook + Actions), mirror notify. Reusable for
  suggestion commits, but it is scoped to an `AccessibleRepo` and assumes
  direct-commit-or-new-branch targeting; a suggestion commit always targets
  the *existing* head branch (never open a new PR), so reuse the inner
  `commit_files` + fan-out pieces, likely via a small shared helper or a
  targeted call.
- `GitBackend::commit_files(bare, branch, base, message, &[FileChange],
  author, email, signing_key, actor_capability)` — `FileChange::Upsert`
  overwrites path content; runs the `update` hook so protection re-verifies.
- `GitBackend::cat_blob(repo, treeish, path)` — read head-tip file content.
- `crates/oxidean-api-pulls/src/update_branch.rs` — precedent for cross-repo
  (fork) heads: `resolve_head` finds head repo row + owner slug + caller cap;
  `live_tips` fetches base objects into head bare when needed. Permission
  model: Write+ on head repo OR Write+ on base repo (maintainer-update
  parity); head-branch protection evaluated as `ProtectionIntent::Push`.
  Mirror this for suggestions. No `maintainer_can_modify` column exists —
  the update-branch model is the in-repo precedent.

### Rendering (web)

- `apps/web/src/lib/markdown.ts` — `renderGfm` unified pipeline:
  remark-parse → remark-gfm → remark-github → rehype → sanitize →
  `rehypeHighlightFences` (Shiki; post-sanitize, trusted markup only) →
  lazy images → stringify. A ````suggestion` fence renders today as
  `<pre><code class="language-suggestion">` (langless → plain fence).
- `apps/web/src/components/repo/markdown-body.tsrx` — `MarkdownBody`/
  `MarkdownHtml` client render wrappers (dangerouslySetInnerHTML of
  sanitized HTML).
- Comment surfaces rendering `MarkdownBody`:
  - `pull-v2/pull-v2-diff-file.tsrx` `CommentNote`/`RowNotes` — diff-anchored
    review comments on the Changes tab (line + range comments, pending
    drafts). Primary surface for apply.
  - `pull-v2/pull-v2-activity-pane.tsrx` — review/conversation bodies.
  - Legacy `pull-reviews.tsrx` / `pull-conversation.tsrx` (pre-flag UI).
- `pull-v2/pull-v2-changes.tsrx` — comments query key
  `["pull","comments",owner,name,number]`; mutations via
  `apiClient.pull.comments.*` + `invalidateQueries`. `anchorSha` =
  scope/head SHA used when creating comments.

### Permissions / gates

- Apply needs: verified user; PR open; comment is RIGHT-side line-anchored
  with at least one ````suggestion` fence; Write+ on head or base repo;
  head-branch protection `evaluate_push` passes.
- Notify/webhook fan-out: committing to head = push fan-out via
  `synchronize_after_push` (head_sha refresh + stale review dismissal +
  `pull_request: synchronize` webhook + Actions), identical to a git push.

## Open questions for specification

1. Suggestion block selection: multiple ````suggestion` fences per comment —
   accept `suggestion_index` (default 0)?
2. Semantics: replace `start_line..=line` (or just `line`) of head file with
   suggestion body; empty body deletes the range.
3. Outdated/stale anchors: apply at *current* head tip (GitHub behavior) vs
   reject when `outdated`. GitHub greys the button on outdated comments in
   some views; safest parity core: allow apply at current head, surface
   `outdated` state in UI.
4. Batch: separate `applyBatch` RPC taking comment_ids (grouped per file,
   non-overlapping ranges enforced, ranges applied bottom-up so line numbers
   don't shift), one commit.
5. Co-authored-by trailer for the suggestion author (GitHub adds it).
6. UI surfaces: diff-file comments (required); activity-pane review bodies
   (nice-to-have); legacy UI out of scope.
7. REST parity: GitHub REST has no apply-suggestion endpoint → RPC only.

## Test seams

- Rust integration: `crates/oxidean-api/tests/` pattern (pull_lifecycle.rs,
  repo_file_edit.rs) — signup/login/repo/branch helpers; `pull.comments.create`
  to plant anchored comments; apply; then `repo.blob`/`cat_blob` assert.
- Web: vitest for suggestion parsing helper; happy-dom integration for the
  suggestion block UI (testing.md minimums).
