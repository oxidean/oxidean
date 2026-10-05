//! Pull-request Actions events (ACT-02 / D-ACT-04 / D-ACT-05).
//!
//! Phase 12 PR lifecycle must call [`dispatch_pull_request`] after:
//! - PR opened → `PullRequestAction::Opened`
//! - head SHA updated (push to PR branch) → `PullRequestAction::Synchronize`
//! - PR reopened → `PullRequestAction::Reopened`
//!
//! This module is the stable internal hook surface; there is no public HTTP trigger.

use std::path::Path;
use std::sync::Arc;

use oxidean_db::Database;
use oxidean_git::GitBackend;
use serde::{Deserialize, Serialize};

use crate::actions::dispatch::enqueue_run;
use crate::actions::discover_workflows;

/// PR lifecycle activity types that enqueue workflows in v1 (D-ACT-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestAction {
    Opened,
    Synchronize,
    Reopened,
}

/// Trusted internal payload from Phase 12 PR service (not a public API).
#[derive(Debug, Clone)]
pub struct PullRequestEvent {
    pub action: PullRequestAction,
    pub repository_id: String,
    /// Bare repo path for workflow discovery at `head_sha`.
    pub bare_repo: std::path::PathBuf,
    pub head_sha: String,
    pub head_ref: String,
    pub actor_id: Option<String>,
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

/// Evaluate `on.pull_request` workflows for a PR lifecycle event.
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

    let discovered = discover_workflows(git, &event.bare_repo, &event.head_sha)
        .await
        .map_err(|e| e.to_string())?;
    for fe in &discovered.errors {
        tracing::warn!(path = %fe.path, error = %fe.message, "skipping invalid workflow file");
    }

    let mut n = 0;
    for wf in &discovered.workflows {
        if !wf.document.triggers.pull_request {
            continue;
        }
        enqueue_run(
            db,
            &event.repository_id,
            &wf.path,
            &wf.document,
            "pull_request",
            &event.head_sha,
            &event.head_ref,
            event.actor_id.as_deref(),
        )
        .await?;
        n += 1;
    }
    Ok(n)
}

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
    actions_enabled_instance: bool,
) -> Result<usize, String> {
    let event = PullRequestEvent {
        action,
        repository_id: repository_id.to_string(),
        bare_repo: bare.to_path_buf(),
        head_sha: head_sha.to_string(),
        head_ref: head_ref.to_string(),
        actor_id: actor_id.map(str::to_string),
        actions_enabled_instance,
    };
    dispatch_pull_request_inner(db, git, &event).await
}
