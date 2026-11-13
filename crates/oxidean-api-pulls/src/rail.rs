//! PR-rail read/write procedures (WI-0001 pr-new-ui parity):
//! `pull.milestone.set`, `pull.development.list` (closing-keyword issues),
//! `pull.projects.list` (project-board cards linked to the pull), and
//! `pull.deployments.list` (deployments on the pull head).

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::pull::{acl, db_err, load_pull_in_repo, to_public};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{
    AppError, PullDeploymentsListResponse, PullDevelopmentItem, PullDevelopmentListResponse,
    PullProjectLink, PullProjectsListResponse, PullPublic, PullRefRequest, SetPullMilestoneRequest,
};

fn milestone_not_found() -> AppError {
    AppError::new("milestone.not_found", "Milestone not found")
}

async fn load_milestone_in_repo(
    ctx: &RpcCtx,
    repo_id: &str,
    number: i64,
) -> Result<oxidean_db::MilestoneRow, AppError> {
    if number < 1 {
        return Err(milestone_not_found());
    }
    ctx.db
        .find_milestone_by_repo_number(repo_id, number)
        .await
        .map_err(db_err)?
        .ok_or_else(milestone_not_found)
}

/// `pull.milestone.set` — Write+; `milestone` is the milestone `#N`,
/// `null` clears. Mirrors `issue.milestone.set` without the
/// milestoned/demilestoned event fanout (pull milestone events are not
/// modeled — same call as `pull.labels.set`).
pub async fn milestone_set(ctx: &RpcCtx, input: serde_json::Value) -> Result<PullPublic, AppError> {
    require_verified(ctx).await?;
    let req: SetPullMilestoneRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.milestone.set input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let milestone = match req.milestone {
        Some(number) => Some(load_milestone_in_repo(ctx, &accessible.row.id, number).await?),
        None => None,
    };
    ctx.db
        .set_pull_milestone(&pull.id, milestone.as_ref().map(|m| m.id.as_str()))
        .await
        .map_err(db_err)?;
    let refreshed = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    to_public(ctx, &refreshed).await
}

/// Closing-keyword refs in `text` — `(relation, issue_number)` pairs in
/// first-seen order. GitHub's Development section scans the PR title and
/// body for `closes|fixes|resolves #N`.
fn closing_refs(text: &str) -> Vec<(String, i64)> {
    let lower = text.to_lowercase();
    let keywords = ["closes #", "fixes #", "resolves #"];
    let mut hits: Vec<(usize, usize, String, i64)> = Vec::new();
    for kw in keywords {
        let relation = kw.trim_end_matches(" #").to_string();
        let mut start = 0;
        while let Some(pos) = lower[start..].find(kw) {
            let abs = start + pos;
            let after = abs + kw.len();
            let digits: String = lower[after..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = digits.parse::<i64>() {
                if n > 0 {
                    hits.push((abs, after + digits.len(), relation.clone(), n));
                }
            }
            start = after + digits.len().max(1);
        }
    }
    hits.sort_by_key(|h| h.0);
    let mut out: Vec<(String, i64)> = Vec::new();
    for (_, _, rel, n) in hits {
        if !out.iter().any(|(_, m)| *m == n) {
            out.push((rel, n));
        }
    }
    out
}

/// `pull.development.list` — Read+; issues the pull's title/body closes.
/// GitHub's "Development" rail section.
pub async fn development_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullDevelopmentListResponse, AppError> {
    let req: PullRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.development.list input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;

    let mut refs = closing_refs(&pull.title);
    refs.extend(closing_refs(&pull.body));
    let mut seen = std::collections::HashSet::new();
    refs.retain(|(_, n)| seen.insert(*n));

    let mut items = Vec::new();
    for (relation, number) in refs {
        let Some(issue) = ctx
            .db
            .find_issue_by_repo_number(&accessible.row.id, number)
            .await
            .map_err(db_err)?
        else {
            continue;
        };
        items.push(PullDevelopmentItem {
            relation,
            issue: oxidean_api_core::issue::to_public(ctx, &issue).await?,
        });
    }
    Ok(PullDevelopmentListResponse { items })
}

/// `pull.projects.list` — Read+; project boards containing a card linked to
/// this pull. GitHub's "Projects" rail section.
pub async fn projects_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullProjectsListResponse, AppError> {
    let req: PullRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.projects.list input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let links = ctx
        .db
        .list_project_cards_for_content(&accessible.row.id, "pull_request", &pull.id)
        .await
        .map_err(db_err)?;
    Ok(PullProjectsListResponse {
        links: links
            .into_iter()
            .map(|l| PullProjectLink {
                card_id: l.card_id,
                column_name: l.column_name,
                project_number: l.project_number,
                project_title: l.project_title,
                project_state: l.project_state,
                project_url: format!(
                    "/{}/{}/projects/{}",
                    accessible.owner_username, accessible.row.name, l.project_number
                ),
            })
            .collect(),
    })
}

/// `pull.deployments.list` — Read+; deployments recorded against the pull
/// head (sha match or head-ref match). GitHub's "Deployments" rail section.
pub async fn deployments_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullDeploymentsListResponse, AppError> {
    let req: PullRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.deployments.list input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let rows = ctx
        .db
        .list_action_deployments_for_pull(
            &accessible.row.id,
            &pull.head_sha,
            &pull.head_ref,
            pull.number,
            30,
        )
        .await
        .map_err(db_err)?;
    let mut creator_ids: Vec<String> = rows.iter().filter_map(|r| r.creator_id.clone()).collect();
    creator_ids.sort();
    creator_ids.dedup();
    let names: std::collections::HashMap<String, String> = if creator_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        ctx.db
            .find_users_by_ids(&creator_ids)
            .await
            .map_err(db_err)?
            .into_iter()
            .map(|u| (u.id, u.username))
            .collect()
    };
    Ok(PullDeploymentsListResponse {
        deployments: rows
            .iter()
            .map(|r| {
                oxidean_api_core::actions::deployment_public(
                    r,
                    r.creator_id.as_ref().and_then(|id| names.get(id).cloned()),
                )
            })
            .collect(),
    })
}
