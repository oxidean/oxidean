//! Pull-request Actions events (ACT-02 / D-ACT-04 / D-ACT-05 / Phase 6).
//!
//! Phase 12 PR lifecycle must call [`dispatch_pull_request`] after:
//! - PR opened → `PullRequestAction::Opened`
//! - head SHA updated (push to PR branch) → `PullRequestAction::Synchronize`
//! - PR reopened → `PullRequestAction::Reopened`
//!
//! This module is the stable internal hook surface; there is no public HTTP trigger.
//!
//! Semantics:
//! - `pull_request` workflows are discovered at the **head** SHA (untrusted
//!   workflow text — fork runs get clamped tokens and no repo secrets).
//! - `pull_request_target` workflows are discovered at the **base** SHA and
//!   run in trusted context — only when the repo opts in
//!   (`repositories.actions_pull_request_target`).
//! - `on.pull_request{,_target}` filters (`types`/`branches`/`paths`) gate
//!   enqueue; `branches` matches the base branch, `paths` the PR diff.
//! - Fork runs may sit `waiting_approval` per
//!   `repositories.actions_fork_pr_approval` (`never`/`first-time`/`always`).

use std::path::Path;
use std::sync::Arc;

use oxidean_db::Database;
use oxidean_git::GitBackend;
use serde::{Deserialize, Serialize};

use crate::actions::discover_workflows;
use crate::actions::dispatch::enqueue_run;

/// PR lifecycle activity types that enqueue workflows (D-ACT-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestAction {
    Opened,
    Synchronize,
    Reopened,
    /// Merge or close-without-merge — `merged` on the event splits them.
    Closed,
    /// Merge-queue entry added (`merge_queue.enqueue`).
    Enqueued,
    /// Entry left the queue (manual remove, failed checks, head moved, close).
    Dequeued,
}

impl PullRequestAction {
    /// GitHub `pull_request` activity type name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Opened => "opened",
            Self::Synchronize => "synchronize",
            Self::Reopened => "reopened",
            Self::Closed => "closed",
            Self::Enqueued => "enqueued",
            Self::Dequeued => "dequeued",
        }
    }
}

/// Trusted internal payload from the PR service (not a public API).
#[derive(Debug, Clone)]
pub struct PullRequestEvent {
    pub action: PullRequestAction,
    /// Base (target) repository id — runs belong here.
    pub repository_id: String,
    /// Head repo bare path — `pull_request` discovery + checkout source.
    pub bare_repo: std::path::PathBuf,
    /// Base repo bare path — `pull_request_target` discovery + `paths` diff.
    /// Same as `bare_repo` for same-repo PRs.
    pub base_bare: std::path::PathBuf,
    pub head_sha: String,
    /// Head branch short name (`feature-x`), as GitHub reports it.
    pub head_ref: String,
    /// Base branch short name — `branches` filters match against this.
    pub base_ref: String,
    /// Base branch tip SHA at event time (`paths` diff base).
    pub base_sha: String,
    /// PR identity for the frozen event payload.
    pub pull_number: i64,
    pub pull_title: String,
    /// PR author's user id — `first-time` approval checks run against them.
    pub pull_author_id: Option<String>,
    /// Repo coords for `repository`/`pull_request.{head,base}.repo` embeds.
    pub head_owner: String,
    pub head_name: String,
    pub base_owner: String,
    pub base_name: String,
    pub base_default_branch: String,
    pub actor_id: Option<String>,
    /// The PR's `head_repo_id` — equals `repository_id` for same-repo PRs,
    /// differs for fork heads. Recorded on `pull_request` runs; claim uses it
    /// to gate secrets and the job token's write grant.
    pub head_repo_id: Option<String>,
    /// `pull_request.merged` on `closed` payloads — queue landings and
    /// `pull.merge` set it; plain closes leave it false.
    pub merged: bool,
    /// Set when the mutating actor authenticated with an Actions job token —
    /// GITHUB_TOKEN-caused events never start new runs (GitHub's recursion
    /// rule; `workflow_dispatch`/`repository_dispatch` are exempt and don't
    /// flow through this path).
    pub job_token_actor: bool,
    pub actions_enabled_instance: bool,
}

/// Soft-fail wrapper for PR lifecycle → Actions (mirrors [`crate::actions::notify_push_actions`]).
///
/// Errors are logged only — callers must not fail the PR mutation (D-ACT-05).
pub async fn notify_pull_request_actions(
    db: &Database,
    git: &dyn GitBackend,
    event: &PullRequestEvent,
) {
    if let Err(e) = dispatch_pull_request_inner(db, git, event).await {
        tracing::warn!(
            error = %e,
            repository_id = %event.repository_id,
            action = ?event.action,
            "actions pull_request dispatch failed (PR mutation already succeeded)"
        );
    }
}

/// Evaluate `on.pull_request`/`on.pull_request_target` workflows for a PR
/// lifecycle event.
///
/// Returns the number of runs enqueued. Never exposes an unauthenticated trigger.
pub async fn dispatch_pull_request(
    db: &Database,
    git: Arc<dyn GitBackend>,
    event: &PullRequestEvent,
) -> Result<usize, String> {
    dispatch_pull_request_inner(db, git.as_ref(), event).await
}

async fn dispatch_pull_request_inner(
    db: &Database,
    git: &dyn GitBackend,
    event: &PullRequestEvent,
) -> Result<usize, String> {
    if event.job_token_actor {
        // GITHUB_TOKEN parity — token-caused mutations never start runs.
        return Ok(0);
    }
    if !event.actions_enabled_instance {
        return Ok(0);
    }
    if !db
        .get_repo_actions_enabled(&event.repository_id)
        .await
        .unwrap_or(true)
    {
        return Ok(0);
    }
    if event.head_sha.is_empty() || event.head_sha.chars().all(|c| c == '0') {
        return Ok(0);
    }
    if !event.bare_repo.exists() {
        return Err(format!("bare repo missing: {}", event.bare_repo.display()));
    }

    // [skip ci]-family markers on the PR head commit suppress both
    // pull_request and pull_request_target runs. Unreadable head (un-synced
    // fork objects) can't carry a marker we can see — don't skip on error.
    if let Ok(c) = git
        .show_commit(&event.bare_repo, &event.head_sha, None, None)
        .await
    {
        if crate::actions::dispatch::message_has_skip_marker(&format!("{}\n{}", c.subject, c.body))
        {
            tracing::info!(
                repository_id = %event.repository_id,
                head_sha = %event.head_sha,
                "actions pull_request skipped — head commit carries a skip marker"
            );
            return Ok(0);
        }
    }

    let is_fork = event
        .head_repo_id
        .as_deref()
        .is_some_and(|h| h != event.repository_id);
    let action_type = event.action.as_str();

    // `paths` constraint — the PR file list via `base_sha...head_sha` on the
    // base repo (head objects were fetched in by the `refs/pull` sync).
    // Missing objects (e.g. un-synced fork head) read as unconstrained —
    // filters must never silently drop runs on data we cannot see.
    let changed: Option<Vec<String>> =
        if event.base_sha.is_empty() || event.base_sha.chars().all(|c| c == '0') {
            None
        } else {
            match git
                .diff(&event.base_bare, &event.base_sha, &event.head_sha)
                .await
            {
                Ok(d) => Some(d.files.iter().map(|f| f.path.clone()).collect()),
                Err(e) => {
                    tracing::warn!(
                        error = %e, repository_id = %event.repository_id,
                        "pull_request paths filter skipped — diff failed"
                    );
                    None
                }
            }
        };

    // Frozen event payload — `github.event` / `OXIDEAN_EVENT_PATH`.
    let actor_id = event
        .pull_author_id
        .as_deref()
        .or(event.actor_id.as_deref());
    let actor_login = match actor_id {
        Some(uid) => db
            .find_user_by_id(uid)
            .await
            .ok()
            .flatten()
            .map(|u| u.username)
            .unwrap_or_default(),
        None => String::new(),
    };
    let actor_embed = crate::actions::payload::user_embed(&actor_login);
    let base_repo = crate::actions::payload::repository_embed(
        &event.base_owner,
        &event.base_name,
        &event.base_default_branch,
    );
    let head_repo =
        crate::actions::payload::repository_embed(&event.head_owner, &event.head_name, "");
    let payload = crate::actions::payload::pull_request_payload(
        action_type,
        event.pull_number,
        &event.pull_title,
        &event.base_ref,
        &event.base_sha,
        &base_repo,
        &event.head_ref,
        &event.head_sha,
        &head_repo,
        if event.head_owner.is_empty() {
            None
        } else {
            Some(&event.head_owner)
        },
        &actor_embed,
        &base_repo,
        event.merged,
    );

    let mut n = 0;

    // ── pull_request_target — trusted base-context, opt-in only ───────────
    if db
        .get_repo_actions_pull_request_target(&event.repository_id)
        .await
        .unwrap_or(false)
        && event.base_bare.exists()
        && !event.base_sha.is_empty()
        && !event.base_sha.chars().all(|c| c == '0')
    {
        let discovered = discover_workflows(git, &event.base_bare, &event.base_sha)
            .await
            .map_err(|e| e.to_string())?;
        if !discovered.errors.is_empty() {
            for fe in &discovered.errors {
                tracing::warn!(path = %fe.path, error = %fe.message, "invalid workflow file");
            }
            n += crate::actions::dispatch::fail_discovery_runs(
                db,
                &event.repository_id,
                &discovered.errors,
                "pull_request_target",
                &event.base_sha,
                &format!("refs/heads/{}", event.base_ref),
                event.actor_id.as_deref(),
                None,
                Some(&payload),
            )
            .await?;
        }
        let hold = is_fork && fork_needs_approval(db, event).await;
        let disabled = db
            .list_disabled_action_workflows(&event.repository_id)
            .await?;
        for wf in &discovered.workflows {
            if !wf.document.triggers.pull_request_target || disabled.contains(&wf.path) {
                continue;
            }
            if !crate::actions::filters::event_matches(
                &wf.document.triggers.pull_request_target_filters,
                Some(&event.base_ref),
                None,
                changed.as_deref(),
                Some(action_type),
            ) {
                continue;
            }
            // Trusted context — the run belongs to the base repo: base branch
            // ref+sha, no fork clamp (`head_repo_id` unset). GitHub titles
            // `pull_request*` runs with the PR title, not the commit subject.
            let (run_id, _) = enqueue_run(
                db,
                &event.repository_id,
                &wf.path,
                &wf.document,
                "pull_request_target",
                &event.base_sha,
                &format!("refs/heads/{}", event.base_ref),
                event.actor_id.as_deref(),
                None,
                Some(&payload),
                hold,
                Some(event.pull_title.as_str()),
            )
            .await?;
            tracing::info!(run_id, hold, "pull_request_target run enqueued");
            n += 1;
        }
    }

    // ── pull_request — untrusted head-context ─────────────────────────────
    // GitHub runs `pull_request` workflows against a synthesized test-merge
    // commit (`github.sha` = the merge, `github.ref` = `refs/pull/N/merge`)
    // and evaluates workflow files from the merged tree. A merge that cannot
    // be built (conflict, missing objects) falls back to the head commit —
    // CI must not wedge on an unmergeable PR.
    let mut run_sha = event.head_sha.clone();
    let mut run_ref = format!("refs/pull/{}/head", event.pull_number);
    let mut wf_repo = &event.bare_repo;
    if action_type == "closed" {
        // A closed PR never gets a fresh test-merge: `refs/pull/N/merge`
        // already holds the real merge commit (merged) or the last open-state
        // test-merge (unmerged close). Rebuilding it would clobber the merge
        // SHA recorded by pull.merge.
        if let Ok(sha) = git
            .rev_parse(
                &event.base_bare,
                &format!("refs/pull/{}/merge", event.pull_number),
            )
            .await
        {
            if !sha.is_empty() {
                run_sha = sha;
                run_ref = format!("refs/pull/{}/merge", event.pull_number);
                wf_repo = &event.base_bare;
            }
        }
    } else {
        // Merge against the *current* base tip — `pull.base_sha` may lag the
        // branch if it moved between PR open and this event.
        let live_base = if event.base_bare.exists() {
            git.rev_parse(&event.base_bare, &format!("refs/heads/{}", event.base_ref))
                .await
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| event.base_sha.clone())
        } else {
            event.base_sha.clone()
        };
        if !live_base.is_empty() && !live_base.chars().all(|c| c == '0') {
            // Fork heads live in the head repo's object store — pull the commit
            // into the base repo so merge-tree can see both parents.
            if event.head_repo_id.as_deref() != Some(event.repository_id.as_str()) {
                if let Err(e) = git
                    .fetch_ref_from(&event.base_bare, &event.bare_repo, &event.head_sha)
                    .await
                {
                    tracing::warn!(error = %e, pull = event.pull_number,
                    "fork head fetch failed — test-merge unavailable");
                }
            }
            match git
                .test_merge_commit(
                    &event.base_bare,
                    &live_base,
                    &event.head_sha,
                    &format!("Merge {} into {}", event.head_sha, event.base_ref),
                )
                .await
            {
                Ok(merge_sha) => {
                    let merge_ref = format!("refs/pull/{}/merge", event.pull_number);
                    match git
                        .update_ref(&event.base_bare, &merge_ref, &merge_sha)
                        .await
                    {
                        Ok(()) => {
                            run_sha = merge_sha;
                            run_ref = merge_ref;
                            wf_repo = &event.base_bare;
                        }
                        Err(e) => tracing::warn!(error = %e, pull = event.pull_number,
                        "test-merge ref write failed — falling back to head"),
                    }
                }
                Err(e) => tracing::info!(error = %e, pull = event.pull_number,
                "test-merge unavailable — pull_request runs on head"),
            }
        }
    }
    let discovered = discover_workflows(git, wf_repo, &run_sha)
        .await
        .map_err(|e| e.to_string())?;
    if !discovered.errors.is_empty() {
        for fe in &discovered.errors {
            tracing::warn!(path = %fe.path, error = %fe.message, "invalid workflow file");
        }
        n += crate::actions::dispatch::fail_discovery_runs(
            db,
            &event.repository_id,
            &discovered.errors,
            "pull_request",
            &run_sha,
            &run_ref,
            event.actor_id.as_deref(),
            event.head_repo_id.as_deref(),
            Some(&payload),
        )
        .await?;
    }
    let hold = is_fork && fork_needs_approval(db, event).await;
    let disabled = db
        .list_disabled_action_workflows(&event.repository_id)
        .await?;
    for wf in &discovered.workflows {
        if !wf.document.triggers.pull_request || disabled.contains(&wf.path) {
            continue;
        }
        if !crate::actions::filters::event_matches(
            &wf.document.triggers.pull_request_filters,
            Some(&event.base_ref),
            None,
            changed.as_deref(),
            Some(action_type),
        ) {
            continue;
        }
        let (run_id, _) = enqueue_run(
            db,
            &event.repository_id,
            &wf.path,
            &wf.document,
            "pull_request",
            &run_sha,
            &run_ref,
            event.actor_id.as_deref(),
            event.head_repo_id.as_deref(),
            Some(&payload),
            hold,
            Some(event.pull_title.as_str()),
        )
        .await?;
        // Pin the run's head sha under a per-run ref. `refs/pull/N/merge` is
        // rebuilt on the next pull event, which orphanages the recorded merge
        // commit — a rerun would then check out a *different* commit than the
        // run row claims. The pin keeps the sha reachable so the runner's
        // `fetch <sha>` (allow-reachable-sha-in-want) always resolves.
        if let Err(e) = git
            .update_ref(
                &event.base_bare,
                &format!("refs/actions/runs/{run_id}"),
                &run_sha,
            )
            .await
        {
            tracing::warn!(error = %e, run_id, "run sha pin failed — rerun may drift to the current ref tip");
        }
        tracing::info!(run_id, hold, "pull_request run enqueued");
        n += 1;
    }
    Ok(n)
}

/// `actions_fork_pr_approval` evaluation — true when this fork run must sit
/// `waiting_approval`.
///
/// - `never` → auto-run (GitHub default).
/// - `always` → every fork run waits.
/// - `first-time` → wait unless the actor has Write+ on the base repo or a
///   prior non-held run there (GitHub's "first-time contributors" analogue).
async fn fork_needs_approval(db: &Database, event: &PullRequestEvent) -> bool {
    let policy = db
        .get_repo_actions_fork_pr_approval(&event.repository_id)
        .await
        .unwrap_or_else(|_| "never".into());
    match policy.as_str() {
        "always" => true,
        "first-time" => {
            let actor = event
                .pull_author_id
                .as_deref()
                .or(event.actor_id.as_deref());
            let Some(actor) = actor else { return true };
            // A collaborator with Write+ is never a "first-time contributor".
            if let Ok(Some(repo)) = db.find_repository_by_id(&event.repository_id).await {
                if let Ok(Some(owner)) = crate::repo::owner_ref_for_repo(db, &repo).await {
                    if let Ok(cap) =
                        crate::repo::effective_capability(db, Some(actor), &repo, &owner).await
                    {
                        if crate::repo::meets(cap, crate::repo::Capability::Write) {
                            return false;
                        }
                    }
                }
            }
            !db.actor_has_prior_approved_run(&event.repository_id, actor)
                .await
                .unwrap_or(true)
        }
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
/// Convenience for tests / harnesses with an owned bare path.
pub async fn dispatch_pull_request_for_sha(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    repository_id: &str,
    action: PullRequestAction,
    head_sha: &str,
    head_ref: &str,
    actor_id: Option<&str>,
    head_repo_id: Option<&str>,
    actions_enabled_instance: bool,
) -> Result<usize, String> {
    let event = PullRequestEvent {
        action,
        repository_id: repository_id.to_string(),
        bare_repo: bare.to_path_buf(),
        base_bare: bare.to_path_buf(),
        head_sha: head_sha.to_string(),
        head_ref: head_ref.to_string(),
        base_ref: "main".into(),
        base_sha: String::new(),
        pull_number: 1,
        pull_title: String::new(),
        pull_author_id: actor_id.map(str::to_string),
        head_owner: String::new(),
        head_name: String::new(),
        base_owner: String::new(),
        base_name: String::new(),
        base_default_branch: "main".into(),
        actor_id: actor_id.map(str::to_string),
        head_repo_id: head_repo_id.map(str::to_string),
        merged: action == PullRequestAction::Closed,
        job_token_actor: false,
        actions_enabled_instance,
    };
    dispatch_pull_request_inner(db, git, &event).await
}

/// Repo lifecycle event for domain-mutation dispatch (`release`, `issues`,
/// `issue_comment`, `create`, `delete`, `watch`, `fork`,
/// `pull_request_review`, `pull_request_review_comment`,
/// `registry_package`). Callers build the frozen payload at the mutation
/// site (where the row data is still in scope) — the GitHub-shaped webhook
/// payload doubles as `github.event`.
pub struct LifecycleEvent {
    /// Bare repo path — workflows are read from the default-branch tip.
    pub bare_repo: std::path::PathBuf,
    pub repository_id: String,
    pub default_branch: String,
    /// GitHub event name — selects the `on.<name>` subscription + `types` list.
    pub event_name: &'static str,
    /// Activity type (`published`, `opened`, `closed`, …).
    pub action: String,
    pub payload: serde_json::Value,
    pub actor_id: Option<String>,
    /// Set when the mutating actor authenticated with an Actions job token —
    /// GITHUB_TOKEN-caused events never start new runs (GitHub's recursion
    /// rule; `workflow_dispatch`/`repository_dispatch` are exempt and don't
    /// flow through this path).
    pub job_token_actor: bool,
    pub actions_enabled_instance: bool,
}

/// Whether the RPC caller authenticated with an Actions job token
/// (`ctx.pat` synthesized from `OXIDEAN_TOKEN`/GITHUB_TOKEN). Session-cookie
/// and human-PAT callers return `false`.
pub fn rpc_caller_is_job_token(ctx: &crate::rpc::RpcCtx) -> bool {
    ctx.pat
        .as_ref()
        .map(crate::pat::bearer::is_job_token)
        .unwrap_or_default()
}

/// Build a `LifecycleEvent` from an `RpcCtx` + resolved repo coords — the
/// default-branch context every RPC mutation site shares. `None` when the
/// bare repo is absent (dispatch degrades to a no-op anyway).
#[allow(clippy::too_many_arguments)]
pub fn rpc_lifecycle_event(
    ctx: &crate::rpc::RpcCtx,
    owner: &str,
    repo_name: &str,
    repository_id: &str,
    default_branch: &str,
    event_name: &'static str,
    action: &str,
    payload: serde_json::Value,
    actor_id: Option<String>,
) -> Option<LifecycleEvent> {
    let bare_repo = crate::git::bare_repo_path(&ctx.repos_dir, owner, repo_name).ok()?;
    Some(LifecycleEvent {
        bare_repo,
        repository_id: repository_id.to_string(),
        default_branch: default_branch.to_string(),
        event_name,
        action: action.to_string(),
        payload,
        actor_id,
        job_token_actor: rpc_caller_is_job_token(ctx),
        actions_enabled_instance: crate::actions::env_actions_enabled(),
    })
}

/// Soft-fail wrapper — a release/issue mutation must never fail because
/// Actions dispatch failed.
pub async fn notify_lifecycle_actions(db: &Database, git: &dyn GitBackend, event: &LifecycleEvent) {
    if let Err(e) = dispatch_lifecycle_inner(db, git, event).await {
        tracing::warn!(
            error = %e,
            repository_id = %event.repository_id,
            event = event.event_name,
            action = %event.action,
            "actions lifecycle dispatch failed (mutation already succeeded)"
        );
    }
}

/// Enqueue `on: release`/`on: issues` workflows whose `types` list (empty =
/// all activity types, GitHub's default) covers `event.action`.
async fn dispatch_lifecycle_inner(
    db: &Database,
    git: &dyn GitBackend,
    event: &LifecycleEvent,
) -> Result<usize, String> {
    if event.job_token_actor {
        // GITHUB_TOKEN parity — token-caused mutations never start runs.
        return Ok(0);
    }
    if !event.actions_enabled_instance {
        return Ok(0);
    }
    if !db.get_repo_actions_enabled(&event.repository_id).await? {
        return Ok(0);
    }
    let default_ref = format!("refs/heads/{}", event.default_branch);
    let head_sha = git
        .rev_parse(&event.bare_repo, &default_ref)
        .await
        .map_err(|e| e.to_string())?;
    let discovered = discover_workflows(git, &event.bare_repo, &head_sha)
        .await
        .map_err(|e| e.to_string())?;
    let mut n = 0;
    if !discovered.errors.is_empty() {
        for fe in &discovered.errors {
            tracing::warn!(path = %fe.path, error = %fe.message, "invalid workflow file");
        }
        n += crate::actions::dispatch::fail_discovery_runs(
            db,
            &event.repository_id,
            &discovered.errors,
            event.event_name,
            &head_sha,
            &default_ref,
            event.actor_id.as_deref(),
            None,
            Some(&event.payload),
        )
        .await?;
    }
    let disabled = db
        .list_disabled_action_workflows(&event.repository_id)
        .await?;
    let default_title =
        crate::actions::dispatch::head_commit_subject(git, &event.bare_repo, &head_sha).await;
    for wf in &discovered.workflows {
        if disabled.contains(&wf.path) {
            continue;
        }
        let Some(types) = wf
            .document
            .triggers
            .lifecycle
            .get(event.event_name)
            .map(|v| v.as_slice())
        else {
            continue;
        };
        // Empty `types:` = GitHub default — all activity types fire.
        if !types.is_empty() && !types.iter().any(|t| t == &event.action) {
            continue;
        }
        enqueue_run(
            db,
            &event.repository_id,
            &wf.path,
            &wf.document,
            event.event_name,
            &head_sha,
            &default_ref,
            event.actor_id.as_deref(),
            None,
            Some(&event.payload),
            false,
            default_title.as_deref(),
        )
        .await?;
        n += 1;
    }
    Ok(n)
}

/// `on: workflow_run` — soft-fail wrapper. A run completing must never fail
/// the runner/RPC path because downstream dispatch errored; GitHub treats
/// this fan-out as fire-and-forget too.
pub async fn notify_workflow_run_actions(
    db: &Database,
    git: &dyn GitBackend,
    repos_dir: &Path,
    run: &oxidean_db::ActionRunRow,
    env_name: &str,
) {
    if let Err(e) = dispatch_workflow_run_inner(db, git, repos_dir, run, env_name).await {
        tracing::warn!(
            error = %e,
            run_id = %run.id,
            repository_id = %run.repository_id,
            "workflow_run dispatch failed (completing run already committed)"
        );
    }
}

/// Enqueue `on: workflow_run` workflows for a completed run. GitHub
/// semantics: downstream workflows are read from the **default branch**
/// (trusted context — they run with full `GITHUB_TOKEN` scope regardless of
/// what triggered the upstream run), `branches`/`branches-ignore` match the
/// completed run's head branch, `types` filters on the activity type
/// (`completed` — the only one the engine emits), and `workflows` matches
/// the completed workflow's `name:` (its file stem when unnamed). An absent
/// `workflows` list matches every workflow, matching GitHub's runtime (the
/// docs call the key required but the schema tolerates omission).
async fn dispatch_workflow_run_inner(
    db: &Database,
    git: &dyn GitBackend,
    repos_dir: &Path,
    run: &oxidean_db::ActionRunRow,
    env_name: &str,
) -> Result<usize, String> {
    if !crate::actions::env_actions_enabled()
        || !db.get_repo_actions_enabled(&run.repository_id).await?
    {
        return Ok(0);
    }
    let repo = db
        .find_repository_by_id(&run.repository_id)
        .await?
        .ok_or_else(|| "repository missing".to_string())?;
    let owner_ref = crate::repo::owner_ref_for_repo(db, &repo)
        .await?
        .ok_or_else(|| "repository owner missing".to_string())?;
    let bare = crate::actions::dispatch::bare_repo_path(repos_dir, owner_ref.slug(), &repo.name);
    if !bare.exists() {
        return Ok(0);
    }
    // Downstream runs live on the default branch tip — never on the
    // completed run's ref (which may be a transient merge ref).
    let default_ref = format!("refs/heads/{}", repo.default_branch);
    let head_sha = git
        .rev_parse(&bare, &default_ref)
        .await
        .map_err(|e| e.to_string())?;
    if head_sha.is_empty() {
        return Ok(0);
    }
    let discovered = discover_workflows(git, &bare, &head_sha)
        .await
        .map_err(|e| e.to_string())?;

    // Frozen payload — `github.event.workflow_run` embeds the completed run.
    let actor_login = match run.triggered_by.as_deref() {
        Some(uid) => db
            .find_user_by_id(uid)
            .await
            .ok()
            .flatten()
            .map(|u| u.username)
            .unwrap_or_default(),
        None => String::new(),
    };
    let sender = crate::actions::payload::user_embed(&actor_login);
    let repository = crate::actions::payload::repository_embed(
        owner_ref.slug(),
        &repo.name,
        &repo.default_branch,
    );
    let head_branch = run
        .head_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(&run.head_ref)
        .to_string();
    // `workflow_run.pull_requests` — the PR embed(s) the completed run was
    // for (only pull_request events carry one in our payload shape).
    let upstream_payload: serde_json::Value = run
        .event_payload_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let pull_requests = upstream_payload
        .get("pull_request")
        .filter(|p| p.is_object())
        .map(|pr| {
            serde_json::json!([{
                "number": pr.get("number").cloned().unwrap_or(serde_json::Value::Null),
                "head": pr.get("head").cloned().unwrap_or(serde_json::Value::Null),
                "base": pr.get("base").cloned().unwrap_or(serde_json::Value::Null),
            }])
        })
        .unwrap_or_else(|| serde_json::json!([]));
    let conclusion = match run.status.as_str() {
        "success" => "success",
        "cancelled" => "cancelled",
        "skipped" => "skipped",
        _ => "failure",
    };
    let payload = crate::actions::payload::workflow_run_payload(
        &run.id,
        run.run_number,
        run.attempt,
        &run.workflow_name,
        &run.workflow_path,
        &head_branch,
        &run.head_sha,
        &run.event,
        conclusion,
        &run.created_at,
        run.finished_at.as_deref().unwrap_or(&run.updated_at),
        &pull_requests,
        &repository,
        &sender,
    );

    let mut n = 0;
    if !discovered.errors.is_empty() {
        for fe in &discovered.errors {
            tracing::warn!(path = %fe.path, error = %fe.message, "invalid workflow file");
        }
        n += crate::actions::dispatch::fail_discovery_runs(
            db,
            &run.repository_id,
            &discovered.errors,
            "workflow_run",
            &head_sha,
            &default_ref,
            run.triggered_by.as_deref(),
            None,
            Some(&payload),
        )
        .await?;
    }
    let disabled = db
        .list_disabled_action_workflows(&run.repository_id)
        .await?;
    let default_title = crate::actions::dispatch::head_commit_subject(git, &bare, &head_sha).await;
    // `workflows:` matches the completed workflow's name — unnamed
    // workflows match their file stem (GitHub's display fallback).
    let file_stem = Path::new(&run.workflow_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    for wf in &discovered.workflows {
        if !wf.document.triggers.workflow_run || disabled.contains(&wf.path) {
            continue;
        }
        let listed = &wf.document.triggers.workflow_run_workflows;
        if !listed.is_empty()
            && !listed
                .iter()
                .any(|w| w.trim() == run.workflow_name || w.trim() == file_stem)
        {
            continue;
        }
        // Only `completed` is a real activity type for us — `requested`/
        // `in_progress` are declared in GitHub's schema but the engine never
        // emits them, so a types list without `completed` matches nothing.
        if !crate::actions::filters::event_matches(
            &wf.document.triggers.workflow_run_filters,
            Some(&head_branch),
            None,
            None,
            Some("completed"),
        ) {
            continue;
        }
        // Downstream runs execute in trusted context: default-branch sha/ref,
        // no fork clamp, no fork-approval hold (the workflow text came from
        // the repo's own default branch).
        let (run_id, _) = enqueue_run(
            db,
            &run.repository_id,
            &wf.path,
            &wf.document,
            "workflow_run",
            &head_sha,
            &default_ref,
            run.triggered_by.as_deref(),
            None,
            Some(&payload),
            false,
            default_title.as_deref(),
        )
        .await?;
        crate::webhook::dispatch::notify_workflow_run(db, &run_id, "requested", env_name).await;
        tracing::info!(run_id, upstream = %run.id, "workflow_run run enqueued");
        n += 1;
    }
    Ok(n)
}
