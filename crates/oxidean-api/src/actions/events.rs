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
}

impl PullRequestAction {
    /// GitHub `pull_request` activity type name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Opened => "opened",
            Self::Synchronize => "synchronize",
            Self::Reopened => "reopened",
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
        for wf in &discovered.workflows {
            if !wf.document.triggers.pull_request_target {
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
            // ref+sha, no fork clamp (`head_repo_id` unset).
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
            )
            .await?;
            tracing::info!(run_id, hold, "pull_request_target run enqueued");
            n += 1;
        }
    }

    // ── pull_request — untrusted head-context ─────────────────────────────
    let discovered = discover_workflows(git, &event.bare_repo, &event.head_sha)
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
            &event.head_sha,
            &format!("refs/pull/{}/head", event.pull_number),
            event.actor_id.as_deref(),
            event.head_repo_id.as_deref(),
            Some(&payload),
        )
        .await?;
    }
    let hold = is_fork && fork_needs_approval(db, event).await;
    for wf in &discovered.workflows {
        if !wf.document.triggers.pull_request {
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
            &event.head_sha,
            &format!("refs/pull/{}/head", event.pull_number),
            event.actor_id.as_deref(),
            event.head_repo_id.as_deref(),
            Some(&payload),
            hold,
        )
        .await?;
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
        actions_enabled_instance,
    };
    dispatch_pull_request_inner(db, git, &event).await
}

/// Repo lifecycle event for `on: release` / `on: issues` dispatch. Callers
/// build the frozen payload at the mutation site (where the release/issue
/// row data is still in scope) via `actions::payload::{release_payload,
/// issues_payload}`.
pub struct LifecycleEvent {
    /// Bare repo path — workflows are read from the default-branch tip.
    pub bare_repo: std::path::PathBuf,
    pub repository_id: String,
    pub default_branch: String,
    /// `"release"` or `"issues"` — selects the trigger + `types` list.
    pub event_name: &'static str,
    /// Activity type (`published`, `opened`, `closed`, …).
    pub action: String,
    pub payload: serde_json::Value,
    pub actor_id: Option<String>,
    pub actions_enabled_instance: bool,
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
    for wf in &discovered.workflows {
        let types: &[String] = match event.event_name {
            "release" if wf.document.triggers.release => &wf.document.triggers.release_types,
            "issues" if wf.document.triggers.issues => &wf.document.triggers.issues_types,
            _ => continue,
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
        )
        .await?;
        n += 1;
    }
    Ok(n)
}
