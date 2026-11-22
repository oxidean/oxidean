# WI-0003 brainstorm — candidate directions

Three candidate shapes for auto-merge-when-green on plain PRs. Direction **A** is recommended.

## Option A — core orchestrator + extracted shared merge executor (recommended)

Mirror the merge-queue architecture:

- `oxidean-api-core::auto_merge` module holds enable/disable state ops and the `maybe_merge_pull` re-evaluation entry point, driven by an `MqDeps`-shaped deps struct (db, git, repos_dir, pages_dir, env_name, job_token_actor, actions_enabled).
- Extract `pull.merge`'s post-gate execute-and-fan-out body (git merge by method → `mark_pull_merged` → merge-ref sync → `record_pr_merge`/`record_authored_merge` → closing-refs → optional branch delete → `pull_request:closed` + `PullRequestAction::Closed`) into a core-level executor callable with an explicit actor; the RPC handler keeps parse/ACL/settings/gate checks and delegates.
- Re-evaluation seams (all soft-fail like merge queue): `emit_suite_completed_if_due` in oxidean-api-check, `repo.commitStatus.create` + the actions internal status publisher, `pull.reviews.submit`, `pull.update` draft→ready flip.
- Cancellation seams: `synchronize_pull_after_head_move` (disable-on-push), `pull.update` base retarget, `pull.close`, explicit `pull.auto_merge.disable`.
- DB: migration 0105 — `pull_requests.auto_merge_method TEXT NULL` (NULL = off), `pull_requests.auto_merge_set_by TEXT NULL`, `repositories.allow_auto_merge BOOL NOT NULL DEFAULT false`.
- RPC: `pull.auto_merge.enable` {owner,name,number,method,commit_title?,commit_message?} and `pull.auto_merge.disable` {owner,name,number}, both PullsWrite; `PullPublic` gains an `auto_merge` embed; `RepoMergeSettings` gains `allow_auto_merge`.
- Parity: `pull_request` webhook + Actions activity `auto_merge_enabled`/`auto_merge_disabled`; REST PR `auto_merge:{enabled_by,merge_method}` fill (no REST verb — GitHub has none).

**Tradeoffs.** Largest diff of the three (merge executor extraction touches a well-tested path), but it is the only option where *every* trigger seam can reach the orchestrator without new crate edges, and the only one that keeps exactly one merge-execution implementation. Test risk is bounded: `pull.merge`'s existing suite (`pull_merge.rs`, `pull_merge_settings.rs`) covers the extracted path unchanged.

## Option B — auto-merge as implicit merge-queue enrollment

When auto-merge is armed, internally enqueue the PR into the existing merge_queue machinery and let `land_group` do the merge.

**Rejected.** Semantics mismatch is structural, not cosmetic: the queue verifies a *merge-commit chain* on `gh-readonly-queue/*` refs and always lands "merge" — it cannot honor squash/rebase method choice; `required_merge_queue` protection is orthogonal to auto-merge (a queue-required base must reject auto-merge, not secretly queue it); queue entries are positional/train-shaped while auto-merge is per-PR immediate; dequeue reasons, events (`enqueued`/`dequeued` vs `auto_merge_enabled/disabled`), and REST shape all diverge. Would also merge under the wrong actor attribution (enqueuer vs enabling user is fine, but every entry would need queue-group formation for a single PR — wasteful and wrong-shaped).

## Option C — orchestrator inside oxidean-api-pulls (RPC-adjacent)

Keep the auto-merge re-evaluation + executor inside the pulls domain crate and have the RPC seams call it directly.

**Rejected on layering.** The decisive seams are *not* pull-domain RPCs: check-suite completion lives in `oxidean-api-check`, commit-status writes in `oxidean-api-repo` (and `oxidean-api-core::actions`), head moves in `oxidean-api-core::pull`. None of those crates may depend on `oxidean-api-pulls` (domain crates sit at the same layer and never import each other; api-check certainly cannot gain a pulls dep). Making them reach a pulls-crate orchestrator would require a new event bus or reverse deps — precisely the parallel infrastructure this item forbids.

## Disagreements / open questions carried into spec

- GitHub disables auto-merge when the head branch is pushed; the work item prescribes "disable-on-push semantics like GitHub" — spec adopts disable (not merely re-evaluate) on any head move, mirroring `head_changed` dequeue.
- Enabling auto-merge on draft PRs: GitHub permits it (merge waits on ready). Spec keeps enable legal on drafts; the gate's draft check already blocks the fire.
- Enabler permission: GitHub requires write access to enable; disabling is open to the enabler *or* any write actor. Spec: enable = Write+; disable = enabler-or-Write+.
- Whether `allow_auto_merge` toggled off cancels armed PRs: GitHub keeps armed PRs armed (the repo flag gates the *enable* affordance). Spec: repo flag gates enable RPC + UI only; armed PRs still fire.
