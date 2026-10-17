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

#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]
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
            None,
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

#[allow(clippy::too_many_arguments)]
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
    head_repo_id: Option<&str>,
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
        head_repo_id,
    )
    .await?;
    // Fail loudly: a workflow that names features we can't execute becomes a
    // failed validation run instead of silently mis-executing.
    if !doc.unsupported.is_empty() {
        let msg = format!(
            "unsupported workflow features: {}",
            doc.unsupported.join(", ")
        );
        let job_id = Uuid::new_v4().to_string();
        db.insert_action_job(oxidean_db::JobInsert::basic(
            &job_id,
            &run_id,
            "__validation",
            &msg,
            "[]",
        ))
        .await?;
        if let Err(e) = crate::actions::logs::append_job_log(
            &crate::actions::logs::resolve_log_dir(),
            &run_id,
            &job_id,
            format!("::error::{msg}\n").as_bytes(),
        )
        .await
        {
            tracing::warn!(error = %e, run_id, "failed to write validation log");
        }
        db.update_action_job_status(&job_id, "failure").await?;
        if let Err(e) = db.recompute_action_run_status(&run_id).await {
            tracing::warn!(error = %e, run_id, "validation run rollup failed");
        }
        let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
        crate::webhook::dispatch::notify_workflow_run(db, &run_id, "requested", &env_name).await;
        if let Err(e) = crate::actions::statuses::upsert_job_status(
            db,
            repository_id,
            head_sha,
            &doc.name,
            "__validation",
            "failure",
            &msg,
            None,
            triggered_by,
        )
        .await
        {
            tracing::warn!(error = %e, run_id, "failed to publish validation status");
        }
        return Ok((run_id, vec![job_id]));
    }
    let log_dir = if doc.warnings.is_empty() {
        None
    } else {
        Some(crate::actions::logs::resolve_log_dir())
    };
    let mut job_ids = Vec::new();
    let mut job_keys = Vec::new();
    for job in &doc.jobs {
        let job_id = Uuid::new_v4().to_string();
        let runs_on = serde_json::to_string(&job.runs_on).unwrap_or_else(|_| "[]".into());
        let name = job.name.clone().unwrap_or_else(|| job.id.clone());
        // Durable payload: capture everything fetch_task needs at enqueue so
        // claim never re-reads the bare repo (head could move / be GC'd).
        let payload_json = serde_json::json!({
            "version": 1,
            "job_key": job.id,
            "name": name,
            "runs_on": job.runs_on,
            "steps": job.steps,
            "env": job.env,
            "workflow_env": doc.env,
            "contents_permission": crate::actions::effective_contents_permission(doc, job).as_str(),
        })
        .to_string();
        let sha = crate::auth::session::sha256_hex(payload_json.as_bytes());
        db.insert_action_payload(&sha, repository_id, workflow_path, head_sha, &payload_json)
            .await?;
        let mut insert = oxidean_db::JobInsert::basic(&job_id, &run_id, &job.id, &name, &runs_on);
        insert.payload_sha = Some(sha);
        db.insert_action_job(insert).await?;
        if let Some(dir) = &log_dir {
            let note: String = doc
                .warnings
                .iter()
                .map(|w| format!("::warning::{w}\n"))
                .collect();
            if let Err(e) =
                crate::actions::logs::append_job_log(dir, &run_id, &job_id, note.as_bytes()).await
            {
                tracing::warn!(error = %e, run_id, "failed to write dispatch warnings");
            }
        }
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

#[allow(clippy::too_many_arguments)]
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
            None,
        )
        .await?;
        n += 1;
    }
    Ok(n)
}

pub fn bare_repo_path(repos_dir: &Path, owner: &str, name: &str) -> PathBuf {
    repos_dir.join(owner).join(format!("{name}.git"))
}
