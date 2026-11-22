# WI-0003 discovery — COL-07 auto-merge-when-green for plain PRs

Roadmap row (`context/product/roadmap.md`, COL-07): merge queue already shipped; the open tail is GitHub-style auto-merge on ordinary PRs — repo setting to allow it, a Write+ actor enables it with a merge method, the merge fires automatically once required checks pass + approvals are satisfied, disable-on-push semantics, cancellation. `auto_merge` is stubbed `null` on the REST PR surface.

## Existing machinery to extend (never parallel-build)

- **Merge gate** (approvals, threads, statuses+check runs, up-to-date, draft, method policy): `crates/oxidean-api-core/src/pull/merge_ops.rs` — `evaluate_merge_gate`, shared by `pull.merge` and `merge_queue.enqueue`.
- **`pull.merge` RPC handler** (settings check → gate → git merge → `mark_pull_merged` → refs/events/closing-refs/delete-branch): `crates/oxidean-api-pulls/src/merge_ops.rs` ~L268-572.
- **Merge-queue orchestrator + non-RPC seam deps** (`MqDeps`, `QueueScope`, `repo_coords`): `crates/oxidean-api-core/src/merge_queue/mod.rs`; DB in `crates/oxidean-db/src/merge_queue.rs`; handlers in `crates/oxidean-api-merge-queue`.
- **Check-suite completion seam** (fires queue resolution): `crates/oxidean-api-check/src/handlers.rs` `emit_suite_completed_if_due` ~L955-980 → `merge_queue::after_suite_completed`.
- **Commit-status write seam**: `crates/oxidean-api-repo/src/commit_status.rs` `create` ~L63 (`repo.commitStatus.create`); Actions' internal publisher writes rows directly at `crates/oxidean-api-core/src/actions/statuses.rs` ~L63.
- **PR-head-moved seam** (disable-on-push hook point): `crates/oxidean-api-core/src/pull/mod.rs` `synchronize_pull_after_head_move` ~L386 → `merge_queue::after_pull_head_moved`.
- **PR close / retarget seams**: `crates/oxidean-api-pulls/src/lifecycle.rs` `close` ~L263, `update` ~L404 (`head_changed`/`base_changed` → `dequeue_entry_for_pull`).
- **Review submit seam**: `crates/oxidean-api-pulls/src/reviews.rs` `reviews_submit` ~L68.
- **Repo merge settings**: `RepoMergeSettingsRow` `crates/oxidean-db/src/pulls.rs` ~L41 + `get/update_repo_merge_settings` ~L1008-1073; core types `RepoMergeSettings`/`UpdateRepoMergeSettingsRequest` `crates/oxidean-core/src/pull_types.rs` ~L271-287; UI `apps/web/src/components/repo/merge-settings-panel.tsrx`.
- **Pull model**: `PullRow` + `map_pull!` + `PULL_COLS` `crates/oxidean-db/src/pulls.rs`; `PullPublic` `crates/oxidean-core/src/pull_types.rs` ~L91; `to_public` `crates/oxidean-api-core/src/pull/mod.rs` ~L770.
- **`pull_request` webhook + Actions activity types**: `emit_pull_event`/`emit_pull_event_db` `crates/oxidean-api-core/src/pull/mod.rs` ~L99-186; `PullRequestAction` enum `crates/oxidean-api-core/src/actions/events.rs` ~L34.
- **REST stub**: `crates/oxidean-api-rest/src/github.rs` ~L578 `"auto_merge": Value::Null`.
- **Merge UI**: legacy `apps/web/src/components/repo/pull-merge-panel.tsrx`; v2 `apps/web/src/components/repo/pull-v2/pull-v2-merge-card.tsrx` (`pr-new-ui` flag).
- **Proc class registry** (PAT scope classes): `crates/oxidean-core/src/proc_classes.rs` ~L294-312 (`PullsWrite`).
- **Migrations**: `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/` — next seq **0105** (see `0081_merge_queue.sql` for the idiom).

## Design facts established

- All merge admissibility already funnels through `evaluate_merge_gate`; auto-merge re-uses it verbatim — it is the "green" definition.
- The merge-queue orchestrator lives in `oxidean-api-core` precisely so non-RPC seams (check completion, post-receive, pull sync, lifecycle) can reach it without domain-crate cycles. Auto-merge needs the same home: `oxidean-api-core::auto_merge` (or a sibling section of `pull/`), driven by `MqDeps`-style deps.
- `pull.merge`'s post-gate body is RPC-bound; the auto-merge fire path needs the same execute-and-fan-out sequence callable without an `RpcCtx` (actor = the enabling user). Extract a shared executor in core — same pattern as queue `land_group`.
- Trigger seams to re-evaluate "is it green now?": check-suite completion, commit-status upsert (both writers), review submit, draft→ready flip. Cancellation seams: head push (`synchronize_*`), retarget (`pull.update` base change), close, explicit disable.
- GitHub parity details: repo toggle `allow_auto_merge` beside the three method toggles; `pull_request` webhook + Actions activity types `auto_merge_enabled`/`auto_merge_disabled`; REST PR field `auto_merge:{enabled_by,merge_method}` (GitHub enable/disable itself is GraphQL-only upstream — no REST verb needed, just populate the field).
- Merge-queue-required bases reject auto-merge enable (route through the queue) — mirrors `pull.merge_queue_required`.
- New RPC names follow `domain.snake_case` (standing decision); register procs in `proc_classes.rs` (`PullsWrite`) + `rpc_gen`.
- Tests: new `crates/oxidean-api/tests/pull_auto_merge.rs` next to `pull_merge.rs`/`merge_queue.rs`; web gates `make web-lint` + `make web-format-check`; client regen via `make rpc-gen`.
