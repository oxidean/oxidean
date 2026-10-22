//! Merge-gate evaluation shared by `merge_queue` (core) and the `pull.merge`
//! handler (domain). Handler surface lives in `oxidean-api-pulls` (#162).

use crate::git::bare_repo_path;
use crate::protection::{evaluate_merge, MergeEvalInput};
use crate::rpc::RpcCtx;
use oxidean_core::AppError;

use super::db_err;

async fn git_is_ancestor(
    bare: &std::path::Path,
    maybe_ancestor: &str,
    tip: &str,
) -> Result<bool, String> {
    if maybe_ancestor.is_empty() || tip.is_empty() || maybe_ancestor == tip {
        return Ok(true);
    }
    let bare_str = bare.to_str().ok_or("non-utf8 bare path")?;
    let out = tokio::process::Command::new("git")
        .args([
            "-C",
            bare_str,
            "merge-base",
            "--is-ancestor",
            maybe_ancestor,
            tip,
        ])
        .output()
        .await
        .map_err(|e| format!("git merge-base: {e}"))?;
    Ok(out.status.success())
}

/// The branch-protection merge gate shared by `pull.merge` and
/// `merge_queue.enqueue` — GitHub only admits PRs to the queue (or the merge
/// button) once every *other* protection is green: approvals, stale-review
/// dismissal, unresolved threads, required status contexts (commit statuses
/// + check runs), up-to-date-head requirements, draft state.
///
/// `merge_method` reports GitHub's eval shape — `None` from the queue path
/// (the queue decides the method; the allowed-methods check is skipped).
pub async fn evaluate_merge_gate(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    eff: &crate::protection::EffectiveProtection,
    row: &oxidean_db::PullRow,
    merge_method: Option<String>,
) -> Result<(), AppError> {
    let reviews = ctx.db.list_pull_reviews(&row.id).await.map_err(db_err)?;
    // Latest-per-user review state (D-PR-07); dismiss_stale excludes Approves on older SHAs.
    let mut latest: std::collections::BTreeMap<String, &oxidean_db::PullReviewRow> =
        std::collections::BTreeMap::new();
    for r in &reviews {
        match latest.get(&r.author_id) {
            Some(prev) if prev.submitted_at >= r.submitted_at => {}
            _ => {
                latest.insert(r.author_id.clone(), r);
            }
        }
    }
    let mut approving_reviewer_ids = Vec::new();
    for r in latest.values() {
        if r.state != "approved" {
            continue;
        }
        if eff.dismiss_stale_reviews && r.commit_sha.as_deref() != Some(row.head_sha.as_str()) {
            continue;
        }
        approving_reviewer_ids.push(r.author_id.clone());
    }
    let comments = ctx.db.list_pull_comments(&row.id).await.map_err(db_err)?;
    let unresolved = comments
        .iter()
        .filter(|c| c.path.is_some() && !c.resolved)
        .count() as i32;
    let statuses = ctx
        .db
        .list_commit_statuses(&accessible.row.id, &row.head_sha)
        .await
        .map_err(db_err)?;
    let mut status_by_context = std::collections::BTreeMap::new();
    for s in statuses {
        status_by_context.insert(s.context, s.state);
    }
    // Check runs satisfy required contexts by name (GitHub treats a
    // required check as met when a check run — not just a commit
    // status — reports the name). Completed runs map their
    // conclusion onto the status vocabulary; mid-flight reads as
    // `pending`. Check runs win over same-named commit statuses —
    // they're the newer context writer.
    if let Ok(check_runs) = ctx
        .db
        .list_check_runs_for_sha(&accessible.row.id, &row.head_sha, None, None, None, 500)
        .await
    {
        let mut latest_by_name: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        for r in &check_runs {
            // Rows come newest-first; first write per name wins.
            latest_by_name.entry(r.name.clone()).or_insert_with(|| {
                if r.status == "completed" {
                    r.conclusion.clone().unwrap_or_else(|| "failure".into())
                } else {
                    "pending".into()
                }
            });
        }
        for (name, state) in latest_by_name {
            status_by_context.insert(name, state);
        }
    }
    let path_for_anc = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let head_up_to_date = git_is_ancestor(&path_for_anc, &row.base_sha, &row.head_sha)
        .await
        .unwrap_or(true);
    let merge_input = MergeEvalInput {
        approving_review_count: approving_reviewer_ids.len() as i32,
        unresolved_review_threads: unresolved,
        last_head_pusher_id: None,
        approving_reviewer_ids,
        head_sha: row.head_sha.clone(),
        base_sha: row.base_sha.clone(),
        head_up_to_date,
        merge_method,
        is_draft: row.draft,
        status_by_context,
    };
    evaluate_merge(eff, accessible.capability, &merge_input)
}
