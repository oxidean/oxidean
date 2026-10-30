//! U5 — `pr_comment` (BR8): the only agent-artifact write. The
//! ArtifactScope is THIS session's applied proposals' `pr_ref` set,
//! recomputed per call — a number outside the set is `forbidden`
//! (hard error, not a silent no-op). The comment lands through
//! `pull.comments.create` unanchored (BR8.2 — PRs live in
//! `pull_requests`/`pull_comments`; the issue-comments path would
//! answer `issue_not_found`), body pre-redacted at this boundary
//! (BR8.3 — the proc validates but never redacts).

use oxidean_api_core::oxpilot::{ToolCtx, ToolError, ToolOutput};
use oxidean_db::OxpilotSessionRow;
use serde_json::{json, Value};
use std::collections::BTreeSet;

use super::{tool_err, OxpilotTools, COMMENT_MAX};

/// `pr_comment{number, body}` → `pull.comments.create` inside the
/// session-materialized artifact set (WF7).
pub async fn pr_comment(
    tools: &OxpilotTools,
    ctx: &ToolCtx,
    args: &Value,
    session: &OxpilotSessionRow,
) -> Result<ToolOutput, ToolError> {
    let number = args["number"].as_i64().unwrap_or_default();
    let body = args["body"].as_str().unwrap_or_default();

    // BR8.1 — ArtifactScope: pr_refs of THIS session's `applied`
    // proposals, fresh per call so a just-applied proposal joins
    // immediately.
    let proposals = tools
        .state
        .db
        .find_oxpilot_proposals_by_session(&ctx.session_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "oxpilot proposal scope lookup failed");
            tool_err("internal", "internal error")
        })?;
    let scope: BTreeSet<String> = proposals
        .iter()
        .filter(|p| p.status == "applied")
        .filter_map(|p| p.pr_ref.clone())
        .collect();
    let pr_ref = format!("{}/{}#{}", session.repo_owner, session.repo_name, number);
    if !scope.contains(&pr_ref) {
        return Err(tool_err(
            "forbidden",
            format!("{pr_ref} is outside this session's materialized artifacts — pr_comment only reaches PRs this session's applied proposals created"),
        ));
    }

    // BR8.3 — bound + redact at the write boundary.
    if body.is_empty() || body.len() > COMMENT_MAX {
        return Err(tool_err("invalid_args", "body: must be 1..4096 bytes"));
    }
    let body = oxidean_core::redact(body);

    // BR8.2 — unanchored conversation comment via the shared dispatch
    // path (no path/side/line).
    let data = tools
        .dispatch_proc(
            ctx,
            "pull.comments.create",
            json!({
                "owner": session.repo_owner,
                "name": session.repo_name,
                "number": number,
                "body": body,
            }),
        )
        .await?;
    let comment_id = data["id"].as_str().unwrap_or_default().to_string();

    // BR9.1 — post-commit audit.
    tools
        .audit(
            ctx,
            "oxpilot.artifact_comment",
            Some(("pull_request", &pr_ref)),
            json!({
                "session_id": ctx.session_id,
                "pr_ref": pr_ref,
                "comment_id": comment_id,
            }),
        )
        .await;

    let msg = format!("comment posted on {pr_ref}");
    Ok(ToolOutput {
        content: msg.clone(),
        output_summary: msg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_membership_string_shape() {
        // `owner/name#number` is the ONLY join key (BR8.1/Q10-A).
        let mut scope = BTreeSet::new();
        scope.insert("acme/app#42".to_string());
        assert!(scope.contains("acme/app#42"));
        assert!(!scope.contains("acme/app#43"));
        assert!(!scope.contains("acme/other#42"));
    }
}
