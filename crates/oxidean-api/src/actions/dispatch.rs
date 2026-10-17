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
    // Enqueue-time expression context (Phase 2): github/env/vars/inputs.
    let prov = crate::actions::eval_ctx::Provenance {
        event,
        head_sha,
        head_ref,
        triggered_by,
        run_id: &run_id,
    };
    let (mut ctx, mut eval_warnings) =
        crate::actions::eval_ctx::build_context(db, repository_id, doc, &prov).await;
    // `name:` interpolates — the run's display name and `github.workflow`.
    let mut name_notes = Vec::new();
    let workflow_name = crate::actions::eval_ctx::interp_or_raw(&doc.name, &ctx, &mut name_notes);
    eval_warnings.extend(name_notes);
    crate::actions::eval_ctx::set_workflow_name(&mut ctx, &workflow_name);
    db.insert_action_run(
        &run_id,
        repository_id,
        workflow_path,
        &workflow_name,
        event,
        head_sha,
        head_ref,
        &workflow_name,
        triggered_by,
        head_repo_id,
    )
    .await?;
    // Workflow `concurrency:` — recorded on the run; enforcement lands with
    // the scheduler phase.
    if let Some(group_src) = doc.concurrency.as_ref().and_then(|c| c.group.as_deref()) {
        let mut notes = Vec::new();
        let group = crate::actions::eval_ctx::interp_or_raw(group_src, &ctx, &mut notes);
        eval_warnings.extend(notes);
        if let Err(e) = db.set_action_run_concurrency(&run_id, &group).await {
            tracing::warn!(error = %e, run_id, "failed to record run concurrency group");
        }
    }
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
    let all_warnings: Vec<String> = doc.warnings.iter().cloned().chain(eval_warnings).collect();
    let log_dir = if all_warnings.is_empty() {
        None
    } else {
        Some(crate::actions::logs::resolve_log_dir())
    };
    let mut job_ids = Vec::new();
    let mut job_keys = Vec::new();
    for job in &doc.jobs {
        let job_id = Uuid::new_v4().to_string();
        // Per-job expression evaluation. Errors in `if`/`runs-on` fail the
        // job loudly; errors in display fields degrade to ::warning::.
        let mut notes: Vec<String> = Vec::new();
        let mut job_status = "queued";
        let mut job_err: Option<String> = None;
        // `jobs.<id>.if` — eval error fails the job; false skips it.
        if let Some(src) = &job.if_expr {
            match oxidean_actions_expr::eval_bool(src, &ctx) {
                Ok(true) => {}
                Ok(false) => {
                    job_status = "skipped";
                    notes.push(format!("job '{}' skipped — if evaluated to false", job.id));
                }
                Err(e) => job_err = Some(format!("if '{src}' evaluation failed: {e}")),
            }
        }
        // `runs-on` labels interpolate — evaluated values drive label
        // matching on the claim path.
        let runs_on_eval: Vec<String> = job
            .runs_on
            .iter()
            .map(|s| crate::actions::eval_ctx::interp_or_raw(s, &ctx, &mut notes))
            .collect();
        // `name` interpolates for display.
        let name = job
            .name
            .as_deref()
            .map(|s| crate::actions::eval_ctx::interp_or_raw(s, &ctx, &mut notes))
            .unwrap_or_else(|| job.id.clone());
        // `timeout-minutes` → absolute deadline.
        let timeout_at = match &job.timeout_minutes {
            Some(src) => match crate::actions::eval_ctx::eval_minutes(src, &ctx) {
                Ok(m) => Some(crate::actions::eval_ctx::deadline_rfc3339(m)),
                Err(e) => {
                    notes.push(format!("timeout-minutes evaluation failed: {e}"));
                    None
                }
            },
            None => None,
        };
        // `concurrency:` — job-level group wins over the workflow's.
        let concurrency_src = job
            .concurrency
            .as_ref()
            .and_then(|c| c.group.as_deref())
            .or_else(|| doc.concurrency.as_ref().and_then(|c| c.group.as_deref()));
        let concurrency_group = concurrency_src
            .map(|s| crate::actions::eval_ctx::interp_or_raw(s, &ctx, &mut notes))
            .filter(|g| !g.is_empty());
        if let Some(err) = job_err {
            job_status = "failure";
            notes.push(err);
        }
        let runs_on = serde_json::to_string(&runs_on_eval).unwrap_or_else(|_| "[]".into());
        // Durable payload: capture everything fetch_task needs at enqueue so
        // claim never re-reads the bare repo (head could move / be GC'd).
        let payload_json = serde_json::json!({
            "version": 1,
            "job_key": job.id,
            "name": name,
            "runs_on": runs_on_eval,
            "steps": job.steps,
            "env": job.env,
            "workflow_env": doc.env,
            "outputs": job.outputs,
            "contents_permission": crate::actions::effective_contents_permission(doc, job).as_str(),
        })
        .to_string();
        let sha = crate::auth::session::sha256_hex(payload_json.as_bytes());
        db.insert_action_payload(&sha, repository_id, workflow_path, head_sha, &payload_json)
            .await?;
        let mut insert = oxidean_db::JobInsert::basic(&job_id, &run_id, &job.id, &name, &runs_on);
        insert.payload_sha = Some(sha);
        insert.status = job_status.into();
        insert.timeout_at = timeout_at;
        insert.concurrency_group = concurrency_group;
        insert.outputs_json = job
            .outputs
            .as_ref()
            .map(|o| o.to_string())
            .unwrap_or_else(|| "{}".into());
        db.insert_action_job(insert).await?;
        // Validation warnings + eval notes + skip/failure annotations land
        // in the job log — visible without a runner.
        let mut note = String::new();
        for w in &all_warnings {
            note.push_str(&format!("::warning::{w}\n"));
        }
        for n in &notes {
            let kind = if job_status == "failure" {
                "error"
            } else {
                "notice"
            };
            note.push_str(&format!("::{kind}::{n}\n"));
        }
        if !note.is_empty() {
            let dir = log_dir
                .clone()
                .unwrap_or_else(crate::actions::logs::resolve_log_dir);
            if let Err(e) =
                crate::actions::logs::append_job_log(&dir, &run_id, &job_id, note.as_bytes()).await
            {
                tracing::warn!(error = %e, run_id, "failed to write dispatch warnings");
            }
        }
        job_ids.push(job_id);
        job_keys.push(job.id.clone());
    }
    // Jobs skipped or failed at enqueue are already terminal — roll the run
    // up immediately so all-skipped workflows finish `success`.
    if let Err(e) = db.recompute_action_run_status(&run_id).await {
        tracing::warn!(error = %e, run_id, "post-enqueue run rollup failed");
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
