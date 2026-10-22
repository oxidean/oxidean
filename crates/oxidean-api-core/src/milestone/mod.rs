//! Milestone domain shared layer — `milestone_to_public` is consumed by
//! `issue` enrichment. The `milestone.*` + `issue.milestone.set` handlers
//! live in `oxidean-api-issues` (issue #162).

use oxidean_core::{AppError, MilestonePublic};
use oxidean_db::MilestoneRow;

pub fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("milestone db error: {e}");
        AppError::new("milestone.internal", "milestone operation failed")
    }
}

pub fn milestone_not_found() -> AppError {
    AppError::new("milestone.not_found", "Milestone not found")
}

pub fn milestone_to_public(
    row: &MilestoneRow,
    creator_username: Option<String>,
) -> MilestonePublic {
    MilestonePublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        number: row.number,
        title: row.title.clone(),
        description: row.description.clone(),
        state: row.state.clone(),
        due_on: row.due_on.clone(),
        creator_id: row.creator_id.clone(),
        creator_username,
        closed_at: row.closed_at.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
        open_issues: row.open_issues,
        closed_issues: row.closed_issues,
    }
}
