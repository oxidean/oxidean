//! `check.*` shared helper — `annotation_count` stays in core (#162):
//! `webhook::dispatch` consumes it across the domain boundary. The
//! `check.*` handlers moved to `oxidean-api-check`.

use oxidean_db::{CheckRunRow, Database};

/// Total annotations visible on a check run — stored
/// `check_run_annotations` plus, for Actions-bound runs, the job's
/// log-extracted `action_annotations` (GitHub shows workflow-command
/// annotations on the check).
pub async fn annotation_count(db: &Database, run: &CheckRunRow) -> i64 {
    let stored = db
        .list_check_run_annotations(&run.id)
        .await
        .map(|a| a.len() as i64)
        .unwrap_or(0);
    let job_notes = match &run.job_id {
        Some(job_id) => db
            .list_action_annotations_for_job(job_id)
            .await
            .map(|a| a.len() as i64)
            .unwrap_or(0),
        None => 0,
    };
    stored + job_notes
}
