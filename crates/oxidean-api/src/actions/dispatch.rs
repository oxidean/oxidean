//! Async push → workflow enqueue (D-ACT-04 / D-ACT-05). Never fails the git push.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxidean_db::Database;
use oxidean_git::GitBackend;
use uuid::Uuid;

use crate::actions::discover_workflows;
use crate::actions::parse::WorkflowDocument;

/// Instance gate from AppState / env (D-ACT-06).
pub fn instance_actions_enabled(flag: bool) -> bool {
    flag
}

/// Parse `OXIDEAN_ACTIONS_ENABLED` (default true). Used when `AppState` is unavailable (SSH/RPC).
pub fn env_actions_enabled() -> bool {
    std::env::var("OXIDEAN_ACTIONS_ENABLED")
        .map(|v| {
            let t = v.trim().to_ascii_lowercase();
            !(t.is_empty() || t == "0" || t == "false" || t == "no" || t == "off")
        })
        .unwrap_or(true)
}

/// Dispatch Actions evaluation after a successful receive-pack.
///
/// Errors are logged only — callers must not fail the push (D-ACT-05 / T-19-09).
pub async fn notify_push_actions(
    db: &Database,
    git: Arc<dyn GitBackend>,
    repos_dir: &Path,
    repository_id: &str,
    owner_slug: &str,
    repo_name: &str,
    pusher_id: Option<&str>,
    updates: &[(String, String, String)],
    actions_enabled_instance: bool,
) {
    if let Err(e) = dispatch_push_inner(
        db,
        git.as_ref(),
        repos_dir,
        repository_id,
        owner_slug,
        repo_name,
        pusher_id,
        updates,
        actions_enabled_instance,
    )
    .await
    {
        tracing::warn!(
            error = %e,
            repository_id,
            "actions push dispatch failed (push already succeeded)"
        );
    }
}

async fn dispatch_push_inner(
    db: &Database,
    git: &dyn GitBackend,
    repos_dir: &Path,
    repository_id: &str,
    owner_slug: &str,
    repo_name: &str,
    pusher_id: Option<&str>,
    updates: &[(String, String, String)],
    actions_enabled_instance: bool,
) -> Result<(), String> {
    if !actions_enabled_instance {
        return Ok(());
    }
    if !db
        .get_repo_actions_enabled(repository_id)
        .await
        .unwrap_or(true)
    {
        return Ok(());
    }

    let bare = repos_dir.join(owner_slug).join(format!("{repo_name}.git"));
    if !bare.exists() {
        return Err(format!("bare repo missing: {}", bare.display()));
    }

    // Prefer the first non-delete branch tip from updates; fall back to HEAD.
    let (head_sha, head_ref) = pick_push_tip(updates);
    let treeish = if head_sha.chars().all(|c| c == '0') || head_sha.is_empty() {
        return Ok(()); // delete-only
    } else {
        head_sha.as_str()
    };

    let discovered = discover_workflows(git, &bare, treeish)
        .await
        .map_err(|e| e.to_string())?;
    for fe in &discovered.errors {
        tracing::warn!(path = %fe.path, error = %fe.message, "skipping invalid workflow file");
    }

    for wf in &discovered.workflows {
        if !wf.document.triggers.push {
            continue;
        }
        enqueue_run(
            db,
            repository_id,
            &wf.path,
            &wf.document,
            "push",
            treeish,
            &head_ref,
            pusher_id,
        )
        .await?;
    }
    Ok(())
}

fn pick_push_tip(updates: &[(String, String, String)]) -> (String, String) {
    for (_before, after, refname) in updates {
        if after.chars().all(|c| c == '0') {
            continue;
        }
        if refname.starts_with("refs/heads/") || refname.starts_with("refs/tags/") {
            return (after.clone(), refname.clone());
        }
    }
    if let Some((_b, after, refname)) = updates.first() {
        return (after.clone(), refname.clone());
    }
    (String::new(), String::new())
}

/// Create a queued run + jobs from a parsed workflow (no step execution).
pub async fn enqueue_run(
    db: &Database,
    repository_id: &str,
    workflow_path: &str,
    doc: &WorkflowDocument,
    event: &str,
    head_sha: &str,
    head_ref: &str,
    triggered_by: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let run_id = Uuid::new_v4().to_string();
    db.insert_action_run(
        &run_id,
        repository_id,
        workflow_path,
        &doc.name,
        event,
        head_sha,
        head_ref,
        &doc.name,
        triggered_by,
    )
    .await?;
    let mut job_ids = Vec::new();
    let mut job_keys = Vec::new();
    for job in &doc.jobs {
        let job_id = Uuid::new_v4().to_string();
        let runs_on = serde_json::to_string(&job.runs_on).unwrap_or_else(|_| "[]".into());
        let name = job.name.clone().unwrap_or_else(|| job.id.clone());
        db.insert_action_job(&job_id, &run_id, &job.id, &name, &runs_on)
            .await?;
        job_ids.push(job_id);
        job_keys.push(job.id.clone());
    }
    if let Err(e) = crate::actions::statuses::publish_queued_for_run(
        db,
        repository_id,
        head_sha,
        &doc.name,
        &run_id,
        &job_keys,
        std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok().as_deref(),
        None,
        None,
        triggered_by,
    )
    .await
    {
        tracing::warn!(error = %e, run_id = %run_id, "failed to publish queued commit statuses");
    }
    // API-04: `workflow_run` `requested`. `enqueue_run` is shared by push / PR /
    // workflow_dispatch callers that do not all thread `AppState::env_name`, so
    // resolve the same `OXIDEAN_ENV` source the app state was built from.
    let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
    crate::webhook::dispatch::notify_workflow_run(db, &run_id, "requested", &env_name).await;
    Ok((run_id, job_ids))
}

/// Test/helper path: enqueue from bare path + SHA without git receive-pack.
pub async fn dispatch_push_for_sha(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    repository_id: &str,
    head_sha: &str,
    head_ref: &str,
    pusher_id: Option<&str>,
    actions_enabled_instance: bool,
) -> Result<usize, String> {
    if !actions_enabled_instance {
        return Ok(0);
    }
    if !db.get_repo_actions_enabled(repository_id).await? {
        return Ok(0);
    }
    let discovered = discover_workflows(git, bare, head_sha)
        .await
        .map_err(|e| e.to_string())?;
    for fe in &discovered.errors {
        tracing::warn!(path = %fe.path, error = %fe.message, "skipping invalid workflow file");
    }
    let mut n = 0;
    for wf in &discovered.workflows {
        if !wf.document.triggers.push {
            continue;
        }
        enqueue_run(
            db,
            repository_id,
            &wf.path,
            &wf.document,
            "push",
            head_sha,
            head_ref,
            pusher_id,
        )
        .await?;
        n += 1;
    }
    Ok(n)
}

pub fn bare_repo_path(repos_dir: &Path, owner: &str, name: &str) -> PathBuf {
    repos_dir.join(owner).join(format!("{name}.git"))
}
