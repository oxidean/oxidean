//! Actions → commit_statuses publisher (D-ACT-15 / D-ACT-16).
//!
//! Context format locked for Phase 13 required checks: `{workflow_name} / {job_id}`
//! where `job_id` is the workflow job key (YAML `jobs.<id>`), not the DB row UUID.
//!
//! Phase 13 / branch protection queries statuses via existing session RPC
//! `repo.commitStatus.list` (Read ACL) — no separate Actions status procedure.

use oxidean_db::Database;
use uuid::Uuid;

/// Build the stable required-check context name (D-ACT-15).
pub fn status_context(workflow_name: &str, job_key: &str) -> String {
    format!("{workflow_name} / {job_key}")
}

/// Map Actions job status → classic commit status state.
pub fn job_status_to_commit_state(job_status: &str) -> &'static str {
    match job_status {
        "queued" | "in_progress" => "pending",
        "success" => "success",
        // Skipped/neutral jobs pass checks on GitHub — a required check
        // whose job was skipped by `if:` or unreachable `needs:` is green.
        "skipped" | "neutral" => "success",
        "failure" => "failure",
        "cancelled" => "error",
        _ => "pending",
    }
}

fn target_url_for_run(
    public_origin: Option<&str>,
    owner: &str,
    repo: &str,
    run_id: &str,
) -> Option<String> {
    let origin = public_origin?.trim().trim_end_matches('/');
    if origin.is_empty() {
        return None;
    }
    Some(format!("{origin}/{owner}/{repo}/actions/runs/{run_id}"))
}

/// Upsert a commit status for one Actions job transition.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_job_status(
    db: &Database,
    repository_id: &str,
    head_sha: &str,
    workflow_name: &str,
    job_key: &str,
    job_status: &str,
    description: &str,
    target_url: Option<&str>,
    creator_id: Option<&str>,
) -> Result<(), String> {
    if head_sha.is_empty() || head_sha.chars().all(|c| c == '0') {
        return Ok(());
    }
    let context = status_context(workflow_name, job_key);
    let state = job_status_to_commit_state(job_status);
    let id = Uuid::new_v4().to_string();
    db.upsert_commit_status(
        &id,
        repository_id,
        head_sha,
        &context,
        state,
        description,
        target_url,
        creator_id,
    )
    .await?;
    Ok(())
}

/// After enqueue: publish each job's *current* status — most are `queued`,
/// but `if:`-skipped legs land `skipped` (commit `success` on GitHub) and
/// validation-failed legs land `failure`. Publishing a flat `queued` would
/// strand those checks at `pending` forever.
#[allow(clippy::too_many_arguments)]
pub async fn publish_queued_for_run(
    db: &Database,
    repository_id: &str,
    head_sha: &str,
    workflow_name: &str,
    run_id: &str,
    public_origin: Option<&str>,
    owner_slug: Option<&str>,
    repo_name: Option<&str>,
    creator_id: Option<&str>,
) -> Result<(), String> {
    let target = match (owner_slug, repo_name) {
        (Some(o), Some(n)) => target_url_for_run(public_origin, o, n, run_id),
        _ => None,
    };
    for job in db.list_action_jobs_for_run(run_id).await? {
        let desc = match job.status.as_str() {
            "skipped" => "Skipped",
            "failure" => "Failed",
            "cancelled" => "Cancelled",
            _ => "Waiting for a runner",
        };
        upsert_job_status(
            db,
            repository_id,
            head_sha,
            workflow_name,
            &job.job_key,
            &job.status,
            desc,
            target.as_deref(),
            creator_id,
        )
        .await?;
    }
    Ok(())
}

/// After run cancellation: every job the cancel flipped to `cancelled`
/// publishes commit state `error` — GitHub cancels a run by failing its
/// pending checks rather than leaving them `pending` forever. Jobs that
/// finished or skipped before the cancel already carry their state
/// (re-publishing is an idempotent upsert, so fail-fast-cancelled legs are
/// safe to re-mark).
pub async fn publish_cancelled_for_run(
    db: &Database,
    run_id: &str,
    public_origin: Option<&str>,
) -> Result<(), String> {
    let run = db
        .find_action_run_by_id(run_id)
        .await?
        .ok_or_else(|| "action run not found".to_string())?;
    let target = public_origin
        .map(|o| o.trim().trim_end_matches('/'))
        .filter(|o| !o.is_empty())
        .map(|o| format!("{o}/actions/runs/{}", run.id));
    for job in db.list_action_jobs_for_run(run_id).await? {
        if job.status != "cancelled" {
            continue;
        }
        upsert_job_status(
            db,
            &run.repository_id,
            &run.head_sha,
            &run.workflow_name,
            &job.job_key,
            "cancelled",
            "Cancelled",
            target.as_deref(),
            run.triggered_by.as_deref(),
        )
        .await?;
        crate::webhook::dispatch::notify_workflow_job(db, &job.id, "completed").await;
        crate::webhook::dispatch::notify_deployment_job_status(db, &job.id).await;
    }
    Ok(())
}

/// After UpdateTask: refresh status from job row + parent run.
pub async fn publish_from_job_update(
    db: &Database,
    job_id: &str,
    job_status: &str,
    public_origin: Option<&str>,
) -> Result<(), String> {
    let job = db
        .find_action_job_by_id(job_id)
        .await?
        .ok_or_else(|| "action job not found".to_string())?;
    let run = db
        .find_action_run_by_id(&job.run_id)
        .await?
        .ok_or_else(|| "action run not found".to_string())?;
    let desc = match job_status {
        "in_progress" => "Running",
        "success" => "Completed successfully",
        "failure" => "Failed",
        "cancelled" => "Cancelled",
        _ => "Updated",
    };
    // Best-effort target URL without owner/name lookup when origin unset.
    let target = public_origin
        .map(|o| o.trim().trim_end_matches('/'))
        .filter(|o| !o.is_empty())
        .map(|o| format!("{o}/actions/runs/{}", run.id));
    upsert_job_status(
        db,
        &run.repository_id,
        &run.head_sha,
        &run.workflow_name,
        &job.job_key,
        job_status,
        desc,
        target.as_deref(),
        run.triggered_by.as_deref(),
    )
    .await?;
    // Terminal transitions emit `workflow_job` completed — this publisher is
    // the choke point for runner finalize, deadline sweeps, fail-fast, and
    // needs-gate skip/fail decisions.
    if matches!(
        job_status,
        "success" | "failure" | "cancelled" | "skipped" | "neutral"
    ) {
        crate::webhook::dispatch::notify_workflow_job(db, job_id, "completed").await;
        crate::webhook::dispatch::notify_deployment_job_status(db, job_id).await;
    }
    Ok(())
}
