# WI-0003 spec — auto-merge-when-green for plain PRs (COL-07 tail)

## Goal

GitHub-style auto-merge on ordinary PRs: a repo admin allows it, a Write+
actor arms a PR with a merge method, and the PR merges automatically the
moment `evaluate_merge_gate` passes. Disable-on-push, retarget, close, and
explicit cancellation per GitHub. Extends existing merge machinery — no
parallel infrastructure (ADR: `context/decisions/auto-merge-core-orchestrator.md`).

## Data model

Migration `0105_pull_auto_merge.sql` (postgres + mysql + sqlite):

- `ALTER TABLE pull_requests ADD COLUMN auto_merge_method TEXT NULL` — NULL = not armed; else `merge|squash|rebase`.
- `ALTER TABLE pull_requests ADD COLUMN auto_merge_set_by TEXT NULL` — enabling user id (attribution + disable ACL).
- `ALTER TABLE pull_requests ADD COLUMN auto_merge_commit_title TEXT NULL`.
- `ALTER TABLE pull_requests ADD COLUMN auto_merge_commit_message TEXT NULL`.
- `ALTER TABLE repositories ADD COLUMN allow_auto_merge BOOLEAN NOT NULL DEFAULT false` (sqlite/mysql `TINYINT/INTEGER 0` per dialect idiom).

Rust:

- `oxidean-db::pulls::PullRow` + `map_pull!` + all three `PULL_COLS` variants gain the four columns.
- `RepoMergeSettingsRow` gains `allow_auto_merge: bool`; `get_repo_merge_settings` / `set_repo_merge_settings` carry it (4th flag arg).
- New DB ops: `set_pull_auto_merge(pull_id, method, set_by, title, message)`, `clear_pull_auto_merge(pull_id)`, `list_armed_pulls_for_head(repo_id, head_sha) -> Vec<PullRow>` (open pulls with `auto_merge_method IS NOT NULL` and matching `head_sha`).

## Core types (`oxidean-core`)

- `RepoMergeSettings` + `UpdateRepoMergeSettingsRequest` gain `allow_auto_merge` (request field `Option<bool>`).
- `PullPublic` gains `#[serde(default, skip_serializing_if="Option::is_none")] pub auto_merge: Option<PullAutoMergePublic>`.
- `PullAutoMergePublic { method: MergeMethod, set_by: String, set_by_username: String, commit_title: Option<String>, commit_message: Option<String> }`.
- `PullAutoMergeEnableRequest { owner, name, number, method: MergeMethod, commit_title: Option<String>, commit_message: Option<String> }`; `PullAutoMergeRefRequest = PullRefRequest` reuse for disable.
- `PullRequestAction` (actions/events.rs) gains `AutoMergeEnabled` → `auto_merge_enabled`, `AutoMergeDisabled` → `auto_merge_disabled` (both are real GitHub `pull_request` activity types that trigger workflows).
- `proc_classes.rs`: `pull.auto_merge.enable` | `pull.auto_merge.disable` → `PullsWrite`.

## RPC surface (`oxidean-api-pulls`)

- `pull.auto_merge.enable` — Write+ (`acl::resolve_for_write`), verified user.
  - Reject `pull.invalid_state` unless open.
  - Reject `repo.merge_disabled`-style `pull.auto_merge_disabled` (`"auto-merge is not allowed for this repository"`) when `!settings.allow_auto_merge`.
  - Reject `pull.merge_disabled` when the requested method is not in repo settings.
  - Reject `pull.merge_queue_required` when the base's effective protection has `required_merge_queue` (route through the queue).
  - Draft PRs may arm (merge waits for ready). Re-arming replaces method/message.
  - Sets the four columns; emits `pull_request` `auto_merge_enabled` webhook + `PullRequestAction::AutoMergeEnabled`; returns updated `PullPublic`.
  - If the gate already passes, still just arms — the fire attempt runs as part of the same handler (call `auto_merge::maybe_fire` after storing) so an already-green PR merges immediately.
- `pull.auto_merge.disable` — enabler (`auto_merge_set_by == user.id`) or Write+; clears columns, emits `auto_merge_disabled`, returns `PullPublic`. No-op-with-success when not armed? Return `pull.invalid_state` ("auto-merge is not enabled") — pick error to keep client honest.
- `repo.mergeSettings.update` accepts + `repo.mergeSettings.get` returns `allow_auto_merge`.

## Orchestrator (`oxidean-api-core::auto_merge`)

Reuse `merge_queue::MqDeps` (identical dep set) — it is the shared non-RPC seam-deps struct.

- `maybe_fire(deps, repo_id, head_sha)` — for each open armed pull on `head_sha`: load scope via `repo_coords`; skip archived repos; compute enabler's current capability on the base repo (enabler lost access → leave armed, warn); `effective_for_branch` on `pull.base_ref`; skip when `eff.required_merge_queue`; run `evaluate_merge_gate(ctx-less deps build AccessibleRepo — see executor note)` with armed method; on pass execute the merge via the shared executor with actor = `auto_merge_set_by`; on gate failure do nothing (stays armed). Soft-fail (warn) per pull, matching queue seam style.
- `maybe_fire_pull(deps, accessible-less, pull_id)` — same single-PR variant for review/update seams that already hold the row.
- `disable_for_pull(deps, pull, sender_login, sender_id)` — `clear_pull_auto_merge` + `emit_pull_event_db("auto_merge_disabled")` + `PullRequestAction::AutoMergeDisabled` notify.

### Shared merge executor (extraction)

`pull.merge`'s post-gate body (api-pulls/merge_ops.rs ~L335-599) extracts to `oxidean-api-core` — e.g. `pull::merge_exec::execute(deps-shaped args, accessible-derived coords, row, method, commit_title, commit_message, delete_branch, actor{id,username}, job_token_actor) -> Result<merge_sha, AppError>` performing: `ensure_head_objects_in_base` → git merge/squash/rebase → `mark_pull_merged` (clears auto-merge cols as part of the UPDATE or explicit `clear_pull_auto_merge`) → `sync_merge_ref` → `record_pr_merge` + `record_authored_merge` → mirror notify → quota refresh → closing-refs issue close → optional same-repo branch delete (+ `delete` ref event) → `pull_request` `closed` webhook + `PullRequestAction::Closed` + `pr_merged` notification fan-out. `pull.merge` keeps parse/ACL/settings/gate checks then delegates; `closing_refs` moves to core (single caller today).

Concurrency note (accepted): fire path re-loads the row and re-checks `state=="open"` + still-armed immediately before executing — same race envelope as `pull.merge` today; git `update_ref` is the serializer.

## Seams (all soft-fail)

Re-evaluation (call `maybe_fire` / `maybe_fire_pull`):

1. `check.runs.create` when `status=="completed"` and `check.runs.update` transition→completed (`oxidean-api-check` handlers ~L719/L803) — armed pulls on the run's head_sha.
2. `project_check_run_for_job` terminal transition (`oxidean-api-core/webhook/dispatch.rs` ~L1047 `action=="completed"`) — Actions-driven check runs, armed pulls on `run.head_sha`.
3. `upsert_job_status` (`oxidean-api-core/actions/statuses.rs` ~L63) — Actions commit statuses, armed pulls on `head_sha`.
4. `repo.commitStatus.create` (`oxidean-api-repo/commit_status.rs`) — external statuses, armed pulls on `req.sha`.
5. `pull.reviews.submit` — approval lands; `maybe_fire_pull` on this PR.
6. `pull.update` draft true→false — `maybe_fire_pull` (gate's draft reason clears).

Disable (`disable_for_pull`):

7. `synchronize_pull_after_head_move` (`oxidean-api-core/pull/mod.rs` ~L386) — head moved ⇒ disable-on-push (`auto_merge_disabled` activity, sender = pusher). Covers same-repo pushes, `pull.updateBranch`, `repo.syncFork` fork-head moves.
8. `pull.update` — `head_changed` (covered by 7 via sync? the update path mutates head directly, NOT through synchronize_pull_after_head_move — hook it too) and `base_ref` retarget ⇒ disable.
9. `pull.close` — `clear_pull_auto_merge` silently (the `closed` event subsumes it).
10. `pull.merge` success — executor clears cols (belt: `mark_pull_merged` UPDATE also clears, or explicit clear in executor).

Repo flag semantics: `allow_auto_merge=false` gates the *enable* RPC and hides the UI affordance; already-armed PRs still fire (GitHub parity).

## REST surface

`oxidean-api-rest/src/github.rs` PR mapping: replace `"auto_merge": Value::Null` with GitHub's shape `{enabled_by: <user embed>, merge_method, commit_title, commit_message}` when armed, else `null`. No new REST verbs (GitHub exposes auto-merge control via GraphQL only).

## Web UI (Octane `.tsrx`)

- `merge-settings-panel.tsrx` — "Allow auto-merge" checkbox wired to `mergeSettings.update` (admin only, existing pattern).
- `pull-merge-panel.tsrx` (legacy) — when `settings.allow_auto_merge` && `!queue.required`: armed ⇒ status line "Auto-merge (method) enabled — will merge when requirements are met" + "Disable auto-merge" button; unarmed ⇒ "Enable auto-merge" button using the selected method radio (fires `pull.auto_merge.enable`); keep direct-merge controls.
- `pull-v2-merge-card.tsrx` (`pr-new-ui` flag) — same affordance in the v2 card idiom.
- Client regen via `make rpc-gen` — never hand-edit `packages/api-client`.

## Acceptance criteria (verify checklist)

1. `repo.mergeSettings.{get,update}` round-trips `allow_auto_merge` (default false; admin-only update).
2. `pull.auto_merge.enable`: Write+ only; rejects when repo flag off, method disallowed, PR non-open, base queue-required; allowed on drafts; sets armed state visible on `pull.get` (`auto_merge` embed with method + enabler).
3. Armed PR merges automatically — same merge result shape as `pull.merge` (merged_by = enabler, `merge_commit_sha`, `merge_method`) — when the gate turns green via: `repo.commitStatus.create`, Actions `upsert_job_status`, RPC check-run completion, Actions check-run projection completion, `pull.reviews.submit`, draft→ready `pull.update`.
4. Merge honors armed method + stored commit title/message.
5. Head move (push to head branch, same-repo or fork, or `pull.update` head refresh) clears auto-merge and emits `pull_request` `auto_merge_disabled` (+ Actions activity).
6. Base retarget disables (event); `pull.close` clears armed state; explicit `pull.auto_merge.disable` clears + emits event.
7. Gate still red after a fire attempt ⇒ stays armed (no disable, no error surfaced to the seam caller).
8. `auto_merge_enabled`/`auto_merge_disabled` appear in `pull_request` webhook payloads and Actions `pull_request` activity matching.
9. REST PR JSON `auto_merge` populated while armed, null otherwise.
10. `cargo nextest run -p oxidean-api -p oxidean-db -p oxidean-api-core` (touched crates) green; `make rpc-sync-check` clean; `make web-lint` + `make web-format-check` clean.

## Explicit non-scope

- No merge-queue behavior change; no per-PR "merge when checks pass" scheduling/timers (event-driven only).
- No REST/GraphQL auto-merge verbs beyond the populated field; no `ox` CLI verbs (follow-up candidate).
- No bypass-actor semantics change: the enabler's *current* capability feeds `evaluate_merge` at fire time.
- No re-eval on base-branch push beyond what strict-status gate already does on the next trigger (armed stays armed).
