# WI-0003 implementation — COL-07 auto-merge-when-green

## What shipped

GitHub-style auto-merge for plain PRs, built on the existing merge
machinery rather than a parallel system.

- **Data**: migration `0105_pull_auto_merge` (pg/mysql/sqlite parity) —
  `pull_requests.auto_merge_method`, `auto_merge_set_by`,
  `auto_merge_commit_title`, `auto_merge_commit_message` (nullable method
  = armed); `repo_merge_settings.allow_auto_merge` (default 0).
  `PullRow`/`RepoMergeSettingsRow` + dialect column lists + `map_pull!`.
- **Types**: `PullAutoMergePublic{method,set_by,set_by_username,
  commit_title?,commit_message?}` on `PullPublic.auto_merge`;
  `RepoMergeSettings.allow_auto_merge`;
  `UpdateRepoMergeSettingsRequest.allow_auto_merge`;
  `PullAutoMergeEnableRequest{owner,name,number,method,commit_title?,
  commit_message?}`; `PullRequestAction::{AutoMergeEnabled,
  AutoMergeDisabled}`; PAT `PullsWrite` for both new procs.
- **Shared executor**: `oxidean-api-core::pull::merge_exec::execute` —
  the post-gate body of `pull.merge` (git merge, `mark_pull_merged`,
  refs, activity records, mirrors, quota, closing refs, optional branch
  delete + `delete` event, `pull_request` `closed` webhook + Actions,
  `pr_merged` notifications) — reused verbatim by the fire path with the
  *enabler* as actor. `closing_refs` moved to core (rail + merge use it).
- **Gate**: `evaluate_merge_gate` refactored to `(db, repos_dir,
  accessible, eff, row, method)` — no `RpcCtx` — so non-RPC seams call it.
- **Orchestrator** `oxidean-api-core::auto_merge`:
  - `maybe_fire_for_head(deps, repo_id, head_sha)` — re-evaluates every
    armed pull on that head (check/status seams).
  - `maybe_fire_pull(deps, scope, pull)` — re-reads the row, requires
    open+armed+repo flag on, computes the *enabler's* live capability
    (none → hold), skips queue-required bases, runs the shared gate,
    merges via the shared executor; the merge clears the arm.
  - `disable_for_pull(db, git, repos_dir, env, job_token, scope, pull,
    sender*)` — clears + `auto_merge_disabled` webhook + Actions.
- **RPCs**: `pull.auto_merge.enable` (Author or Write+; repo flag on;
  open pull; drafts may arm; method must be repo-allowed; queue-required
  bases rejected `pull.merge_queue_required`; re-arm replaces; emits
  `auto_merge_enabled` + Actions + `pr_auto_merge_enabled` activity;
  immediate fire when already green) and `pull.auto_merge.disable`
  (author/enabler/Write+; unarmed → `pull.invalid_state`).
- **Fire seams**: `emit_suite_completed_if_due` (check suites),
  `repo.commitStatus.create`, `pull.reviews.submit`, `.dismiss`,
  `pull.comments.resolve`, `pull.update` (draft→ready + any edit).
- **Disarm seams**: `synchronize_pull_after_head_move` (covers push /
  `pull.updateBranch` / `repo.syncFork`), `pull.update` head-move +
  retarget, `pull.close` (silent clear), explicit `pull.auto_merge.disable`,
  merge consumption inside the executor.
- **REST**: `auto_merge` embed now returns GitHub's object shape
  (`enabled_by` user, `merge_method`, `commit_title`, `commit_message`)
  instead of null.
- **Client**: `pull.autoMerge.enable/disable`, `PullAutoMergePublic`,
  `PullAutoMergeEnableRequest`, `auto_merge` on `PullPublic`,
  `allow_auto_merge` on both merge-settings types (rpc-gen template).
- **UI**: `merge-settings-panel.tsrx` "Allow auto-merge" checkbox;
  `pull-merge-panel.tsrx` + `pull-v2-merge-card.tsrx` — "Enable
  auto-merge" beside the merge button (uses selected method), armed
  banner with enabler/method + Disable.

## Decisions made

- Queue-required bases reject arming (`pull.merge_queue_required`) — the
  queue owns those merges; GitHub shows "merge when ready" there.
- Repo setting flipped off mid-flight holds arms (no merge, no disarm).
- Enabler capability is re-evaluated at fire time; loss of all repo
  access holds the arm.
- Drafts may arm; the gate blocks the merge until ready (`pull.update`
  fires on the flip).

## Test evidence

`crates/oxidean-api/tests/pull_auto_merge.rs` — 7 integration tests:
enable validation (flag/method/state/disable-idempotency), fires on
required status green (armed method honored, merged_by = enabler),
fires on check-run completion, fires on review approval, disarms on
head push (green on new head does not merge), draft arm + ready-fire,
queue-required rejection. All 7 pass.

Full `cargo nextest -p oxidean-api` (1089) and touched-crate suites
(db/core/api-core/pulls/check/repo/merge-queue/rest, 431) green;
`make rpc-sync-check`, `make web-lint`, `make web-format-check` clean.
