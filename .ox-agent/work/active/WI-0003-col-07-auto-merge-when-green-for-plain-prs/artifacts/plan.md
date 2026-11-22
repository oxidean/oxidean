# WI-0003 implementation plan — COL-07 auto-merge-when-green

Spec: `artifacts/spec.md`. Direction per `context/decisions/auto-merge-core-orchestrator.md` + `auto-merge-semantics.md`. Six stages; each has its own gate; later stages may run per-seam without cross-blocking.

## Stage 1 — data model + core types

- `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0105_pull_auto_merge.sql`: 4 cols on `pull_requests` + `allow_auto_merge` on `repositories` (dialect idioms per `0081_merge_queue.sql`/`0097_issue_pull_pins.sql`).
- `oxidean-db/src/pulls.rs`: `PullRow` + `map_pull!` + `PULL_COLS{,_PG,_MYSQL}` + `RepoMergeSettingsRow`; `set_pull_auto_merge` / `clear_pull_auto_merge` / `list_armed_pulls_for_head`; `set_repo_merge_settings` 4-flag signature (+ call sites).
- `oxidean-core/src/pull_types.rs`: `RepoMergeSettings.allow_auto_merge`, `UpdateRepoMergeSettingsRequest.allow_auto_merge`, `PullAutoMergePublic`, `PullPublic.auto_merge`, `PullAutoMergeEnableRequest`.
- `oxidean-core/src/actions/events.rs`: `PullRequestAction::{AutoMergeEnabled,AutoMergeDisabled}` + `as_str`.
- `oxidean-core/src/proc_classes.rs`: `pull.auto_merge.enable|disable` → `PullsWrite` (+ `lib.rs` PROCEDURES strings counted where asserted).
- `oxidean-api-core/src/pull/mod.rs::to_public`: populate `auto_merge` embed (resolve `set_by` username).

**Gate 1**: `cargo check -p oxidean-db -p oxidean-core -p oxidean-api-core`; `cargo nextest run -p oxidean-db` green.

## Stage 2 — shared merge executor extraction (refactor, no behavior change)

- Move `closing_refs` (`oxidean-api-pulls/src/closing_refs.rs`) to `oxidean-api-core::pull::closing_refs` (sole caller is the merge path).
- Extract api-pulls `merge()` post-gate body (~L335-599: ensure objects → git merge by method → `mark_pull_merged` → `sync_merge_ref` → `record_pr_merge`/`record_authored_merge` → mirror/quota → closing-refs → delete-branch(+`delete` event) → `emit_pull_event_db("closed")` + `PullRequestAction::Closed` + `pr_merged` fanout) into `oxidean-api-core::pull::merge_exec::execute(deps…, coords, row, method, commit_title, commit_message, delete_branch, actor, job_token_actor)`. Params shaped on `MqDeps` + `QueueScope` where they fit.
- api-pulls `merge()` keeps parse/ACL/settings/gate + `to_public` response, delegates execution; `merge()` also clears auto-merge cols via executor.
- `merge_settings_get`/`update` carry `allow_auto_merge`.

**Gate 2**: `cargo nextest run -p oxidean-api` existing `pull_merge.rs` + `pull_merge_settings.rs` + `merge_queue.rs` green unchanged; `cargo clippy` clean.

## Stage 3 — orchestrator + RPC

- `oxidean-api-core/src/auto_merge/mod.rs`: `maybe_fire(deps, repo_id, head_sha)`, `maybe_fire_pull(deps, scope/accessible-lite, pull)`, `disable_for_pull(deps, pull, sender_login, sender_id)` — all soft-fail; enabler capability resolved at fire time; `required_merge_queue` skip; gate via `evaluate_merge_gate` (needs an AccessibleRepo for the *enabler* — resolve via `resolve_repo_for_read`-equivalent db path since no RpcCtx; build the struct field-wise like `QueueScope`).
- `oxidean-api-pulls/src/auto_merge.rs`: `enable` (Write+, repo flag, method allowed, open, not queue-required, stores cols + emits `auto_merge_enabled`, then inline `maybe_fire_pull` attempt) + `disable` (enabler-or-Write+, clears, emits `auto_merge_disabled`); register in `dispatch.rs` + `lib.rs` PROCEDURES.
- `repo.mergeSettings.*` already updated in Stage 2.

**Gate 3**: targeted new API test — enable/disable ACL + validation errors — green.

## Stage 4 — seams

Fire seams: `check.runs.create`/`update` completed branches (`api-check/handlers.rs`); `project_check_run_for_job` completed transition (`api-core/webhook/dispatch.rs`); `upsert_job_status` (`api-core/actions/statuses.rs`); `repo.commitStatus.create` (`api-repo/commit_status.rs`); `pull.reviews.submit`; `pull.update` draft true→false.

Disable seams: `synchronize_pull_after_head_move` (`api-core/pull/mod.rs`); `pull.update` `head_changed` + `base_changed`; `pull.close` (silent `clear_pull_auto_merge`).

**Gate 4**: `cargo nextest run -p oxidean-api` incl. new `pull_auto_merge.rs` covering: fire via commitStatus; fire via check-run completion; fire via review submit; disable-on-push; retarget disable; close clears; explicit disable; draft gate; queue-required reject; flag-off reject; method reject.

## Stage 5 — REST, client, UI

- `oxidean-api-rest/src/github.rs`: `auto_merge` embed `{enabled_by, merge_method, commit_title, commit_message}` when armed.
- `make rpc-gen` → `packages/api-client` regenerated; `make rpc-sync-check`.
- `apps/web`: `merge-settings-panel.tsrx` "Allow auto-merge" checkbox; `pull-merge-panel.tsrx` + `pull-v2-merge-card.tsrx` enable/disable affordance (armed status + disable button, or enable button using selected method; hidden when `queue.required`).

**Gate 5**: `make web-lint`, `make web-format-check` green; vitest as relevant.

## Stage 6 — tests/docs/roadmap

- `crates/oxidean-api/tests/pull_auto_merge.rs` (written alongside Stage 4, finalized here), `docs/API.md` procedure rows if the doc enumerates procs, roadmap COL-07 row updated to shipped.

**Gate 6**: full `cargo nextest run -p oxidean-api -p oxidean-db -p oxidean-api-core -p oxidean-core` green; PR body test plan matches.

## Risks / assumptions

- Executor extraction is the largest single diff — mitigated by Gate 2 pinning existing coverage before any new behavior lands.
- Armed-pull lookup is `repo_id + head_sha` on the BASE repo (Actions PR runs project check rows into the base repo's check tables) — verify dialect SQL in Stage 4 tests with a fork-shaped fixture if cheap, else same-repo covers the contract.
- `pull.auto_merge.disable` on an unarmed PR returns `pull.invalid_state` ("auto-merge is not enabled").
- Enabler loses repo access between arm and fire: pull stays armed, warn logged (no wedging; a future write-actor can disable).
