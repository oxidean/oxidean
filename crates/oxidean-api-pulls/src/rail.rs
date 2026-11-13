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

/// Upper bound on closing refs resolved per `pull.development.list` call.
const MAX_DEVELOPMENT_REFS: usize = 50;

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

use crate::closing_refs::closing_refs;

/// Resolve one closing ref to its target repo + issue row. Qualified
/// `owner/repo#N` refs honor the same read ACL as `pull.development.list`;
/// inaccessible or missing targets are skipped like absent issues.
async fn resolve_development_ref(
    ctx: &RpcCtx,
    base: &oxidean_api_core::repo::AccessibleRepo,
    base_owner: &str,
    relation: &'static str,
    repo: Option<&(String, String)>,
    number: i64,
) -> Result<Option<(String, String, String, oxidean_db::IssueRow)>, AppError> {
    let (repo_id, owner, name) = match repo {
        Some((owner_slug, repo_name)) => {
            let target =
                oxidean_api_core::repo::resolve_repo_for_read(ctx, owner_slug, repo_name).await;
            let target = match target {
                Ok(t) => t,
                Err(e) if e.code == "repo.not_found" => return Ok(None),
                Err(e) => return Err(e),
            };
            (target.row.id, target.owner_username, target.row.name)
        }
        None => (
            base.row.id.clone(),
            base_owner.to_string(),
            base.row.name.clone(),
        ),
    };
    let Some(issue) = ctx
        .db
        .find_issue_by_repo_number(&repo_id, number)
        .await
        .map_err(db_err)?
    else {
        return Ok(None);
    };
    Ok(Some((relation.to_string(), owner, name, issue)))
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
    refs.retain(|r| {
        seen.insert((
            r.repo
                .as_ref()
                .map(|(o, n)| (o.to_lowercase(), n.to_lowercase())),
            r.number,
        ))
    });
    // Bound the per-ref lookups — a body stuffed with refs must not turn one
    // rail view into hundreds of queries.
    refs.truncate(MAX_DEVELOPMENT_REFS);

    let mut relations = Vec::with_capacity(refs.len());
    let mut owners = Vec::with_capacity(refs.len());
    let mut names = Vec::with_capacity(refs.len());
    let mut rows = Vec::with_capacity(refs.len());
    for r in refs {
        let Some((relation, owner, name, issue)) = resolve_development_ref(
            ctx,
            &accessible,
            &accessible.owner_username,
            r.relation,
            r.repo.as_ref(),
            r.number,
        )
        .await?
        else {
            continue;
        };
        relations.push(relation);
        owners.push(owner);
        names.push(name);
        rows.push(issue);
    }
    let publics = oxidean_api_core::issue::to_public_many(ctx, &rows).await?;
    let items = relations
        .into_iter()
        .zip(owners)
        .zip(names)
        .zip(publics)
        .map(|(((relation, owner), name), issue)| PullDevelopmentItem {
            relation,
            repo_owner: owner,
            repo_name: name,
            issue,
        })
        .collect();
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

#[cfg(test)]
mod tests {
    use crate::closing_refs::closing_refs;

    fn refs(text: &str) -> Vec<(Option<crate::closing_refs::RepoSlug>, i64, &'static str)> {
        closing_refs(text)
            .into_iter()
            .map(|r| (r.repo, r.number, r.relation))
            .collect()
    }

    #[test]
    fn closing_refs_parses_keywords() {
        assert_eq!(
            refs("closes #12 and fixes #7"),
            vec![(None, 12, "closes"), (None, 7, "fixes")]
        );
        assert_eq!(refs("close #3"), vec![(None, 3, "close")]);
    }

    #[test]
    fn closing_refs_parses_qualified_refs() {
        assert_eq!(
            refs("closes owner/repo#9"),
            vec![(Some(("owner".into(), "repo".into())), 9, "closes")]
        );
    }
}
