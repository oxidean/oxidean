# WI-0004 plan — COL-08 suggested-change blocks

Spec: `artifacts/spec.md` · ADR: `context/decisions/col-08-suggested-changes.md`

## S1 — Wire types + proc class (Rust foundation)

- `crates/oxidean-core/src/pull_types.rs`:
  `ApplyPullSuggestionRequest {owner, name, number, comment_id,
  suggestion_index?, message?}`,
  `ApplyPullSuggestionBatchRequest {owner, name, number, comment_ids,
  message?}`,
  `ApplySuggestionResponse {commit_sha, branch}`,
  `ApplySuggestionBatchResponse {commit_sha, branch, applied}`.
- `crates/oxidean-core/src/proc_classes.rs`: `pull.suggestions.apply` +
  `pull.suggestions.apply_batch` → `PullsWrite`.
- Gate: `cargo check -p oxidean-core`.

## S2 — `pull.suggestions.*` handlers

New `crates/oxidean-api-pulls/src/suggestions.rs`:

- `parse_suggestion_blocks(body) -> Vec<String>` — fenced-block scanner
  (backtick/tilde fences ≥3, info string trims to exact `suggestion`,
  closer ≥ opener length). Unit tests in-file.
- `splice_lines(text, start, end, replacement) -> Result<String>` —
  `\n`-split splice, range checks, empty replacement deletes, preserves
  trailing-newline shape of the file.
- `apply` + `apply_batch` per spec flow: comment anchor checks →
  `resolve_head` (reuse update_branch's via `pub(crate)` extraction or
  duplicate the ~30 lines — prefer extraction into a shared private
  helper inside the crate) → permission/protection → `cat_blob` → splice
  → `commit_files` (web-flow signed) → post-push fan-out
  (`record_ref_updates`, `notify_push`, `synchronize_after_push`,
  mirror) → response. Batch: group per path, descending ranges, overlap
  reject, one commit.
- Register in `dispatch.rs` + `PROCEDURES`.
- Gate: `cargo nextest run -p oxidean-api-pulls` + clippy.

## S3 — Rust integration tests

`crates/oxidean-api/tests/pull_suggestions.rs` covering AC-1..9, AC-12:
happy path single + range + delete; LEFT/general/file/pending/closed
rejects; permission denial; protected branch; co-authored trailer; batch
(one commit, two files, overlap reject); suggestion_index; out-of-range;
fork-head maintainer apply (fork setup helper may exist in
repo_fork.rs test — check).
Gate: `cargo nextest run -p oxidean-api --test pull_suggestions`.

## S4 — api-client codegen + docs

- `rpc_gen.rs`: TS types + `pull.suggestions.apply/applyBatch` client
  methods; `make rpc-gen`; commit generated `packages/api-client`.
- `docs/API.md` procedure-table row.
- Gate: `make rpc-sync-check` (or the equivalent script) clean.

## S5 — Web suggestion blocks (Octane)

- `apps/web/src/lib/suggestions.ts`: `parseSuggestionBlocks` →
  `{type:"markdown"|"suggestion", text}[]`; vitest
  `suggestions.test.ts`.
- `suggestion-markdown-body.tsrx`: segment renderer — markdown via
  `MarkdownBody`, suggestion via inline `SuggestionBlock` (styled
  header + `<pre><code>` body + Apply/Add-to-batch buttons).
- `pull-v2-diff-file.tsrx` `CommentNote`: swap `MarkdownBody` for the
  suggestion renderer when the comment is RIGHT-side line-anchored and
  `canInteract`; pass through apply/batch callbacks from
  `pull-v2-changes.tsrx` (local batch state, apply dialog with message
  field, `apiClient.pull.suggestions.*`, invalidations).
- `pull-v2-activity-pane.tsrx`: same renderer for review-comment bodies.
- happy-dom integration test beside pull-v2 suites (AC-10).
- Gates: `make web-lint` + `make web-format-check` +
  `bun run --filter @oxidean/web test` scoped files.

## S6 — Verify + ship

- `cargo fmt`; scoped nextest; web gates; docs check.
- `git fetch origin && git merge origin/main`; push branch; PR with
  summary + test plan; `@coderabbitai review` comment.

## Risks / assumptions

- `commit_files` runs the `update` hook — hooks must exist in test repos
  (repo_file_edit tests prove the path works under nextest).
- CRLF files: splice preserves retained line terminators; the replaced
  range adopts suggestion LF endings — documented behavior, not
  normalized.
- Binary/non-UTF-8 files reject `pull.suggestion_invalid`.
- Batch across comments whose ranges overlap rejects whole batch
  (fail-closed, matches GitHub refusing double-apply).
