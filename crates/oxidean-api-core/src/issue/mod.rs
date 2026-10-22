//! Issue domain shared layer — ACL, enrichment/serialization, and the
//! lifecycle notification helpers other core services lean on (pull comments,
//! merge queue, project, discussion). The `issue.*` RPC handlers live in
//! `oxidean-api-issues` (issue #162).

pub mod acl;

use oxidean_core::{AppError, IssueAssigneePublic, IssuePublic, IssueState, ReactionGroupPublic};
use oxidean_db::IssueRow;

use crate::rpc::RpcCtx;
use crate::webhook::dispatch;

pub fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.starts_with("invalid issue state filter:") {
        AppError::new("rpc.bad_input", e)
    } else {
        tracing::error!("issue db error: {e}");
        AppError::new("issue.internal", "issue operation failed")
    }
}

pub fn issue_not_found() -> AppError {
    AppError::new("issue.not_found", "Issue not found")
}

pub fn viewer_id(ctx: &RpcCtx) -> Option<&str> {
    ctx.session.as_ref().map(|s| s.user_id.as_str())
}

pub fn reaction_groups_to_public(
    rows: Vec<oxidean_db::issues::ReactionGroupRow>,
) -> Vec<ReactionGroupPublic> {
    rows.into_iter()
        .map(|r| ReactionGroupPublic {
            content: r.content,
            count: r.count,
            viewer_has_reacted: r.viewer_has_reacted,
        })
        .collect()
}

pub struct IssueEnrichment {
    usernames: std::collections::HashMap<String, String>,
    labels: std::collections::HashMap<String, Vec<oxidean_db::LabelRow>>,
    assignees: std::collections::HashMap<String, Vec<oxidean_db::issue_labels::IssueAssigneeRow>>,
    reactions: std::collections::HashMap<String, Vec<oxidean_db::issues::ReactionGroupRow>>,
    /// milestone id → row for linked issues (embedded `milestone` object).
    milestones: std::collections::HashMap<String, oxidean_db::MilestoneRow>,
}

pub async fn load_enrichment(ctx: &RpcCtx, rows: &[IssueRow]) -> Result<IssueEnrichment, AppError> {
    use std::collections::HashMap;
    let issue_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let author_ids: Vec<String> = {
        let mut v: Vec<String> = rows.iter().map(|r| r.author_id.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    if issue_ids.is_empty() {
        return Ok(IssueEnrichment {
            usernames: HashMap::new(),
            labels: HashMap::new(),
            assignees: HashMap::new(),
            reactions: HashMap::new(),
            milestones: HashMap::new(),
        });
    }
    let milestone_ids: Vec<String> = {
        let mut v: Vec<String> = rows.iter().filter_map(|r| r.milestone_id.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    let (users, label_pairs, assignee_pairs, reaction_pairs, milestone_rows) = tokio::try_join!(
        ctx.db.find_users_by_ids(&author_ids),
        ctx.db.list_labels_for_issues(&issue_ids),
        ctx.db.list_assignees_for_issues(&issue_ids),
        ctx.db
            .list_issue_reaction_groups_for_issues(&issue_ids, viewer_id(ctx)),
        ctx.db.list_milestones_by_ids(&milestone_ids),
    )
    .map_err(db_err)?;
    let mut usernames: HashMap<String, String> =
        users.into_iter().map(|u| (u.id, u.username)).collect();
    // Milestone creators are often the issue authors already fetched; pull
    // only the missing ids in one extra query.
    let extra_creator_ids: Vec<String> = milestone_rows
        .iter()
        .filter_map(|m| m.creator_id.clone())
        .filter(|id| !usernames.contains_key(id))
        .collect();
    if !extra_creator_ids.is_empty() {
        if let Ok(extra) = ctx.db.find_users_by_ids(&extra_creator_ids).await {
            usernames.extend(extra.into_iter().map(|u| (u.id, u.username)));
        }
    }
    let mut labels: HashMap<String, Vec<oxidean_db::LabelRow>> = HashMap::new();
    for (issue_id, label) in label_pairs {
        labels.entry(issue_id).or_default().push(label);
    }
    let mut assignees: HashMap<String, Vec<oxidean_db::issue_labels::IssueAssigneeRow>> =
        HashMap::new();
    for (issue_id, a) in assignee_pairs {
        assignees.entry(issue_id).or_default().push(a);
    }
    let mut reactions: HashMap<String, Vec<oxidean_db::issues::ReactionGroupRow>> = HashMap::new();
    for (issue_id, g) in reaction_pairs {
        reactions.entry(issue_id).or_default().push(g);
    }
    Ok(IssueEnrichment {
        usernames,
        labels,
        assignees,
        reactions,
        milestones: milestone_rows
            .into_iter()
            .map(|m| (m.id.clone(), m))
            .collect(),
    })
}

pub fn issue_to_public(
    row: &IssueRow,
    enrichment: &IssueEnrichment,
) -> Result<IssuePublic, AppError> {
    let state = IssueState::parse(&row.state).map_err(|e| {
        tracing::error!(error = %e, "invalid issue state in db");
        AppError::new("issue.internal", "issue operation failed")
    })?;
    let author_username = enrichment
        .usernames
        .get(&row.author_id)
        .cloned()
        .unwrap_or_default();
    let labels = crate::label::label_rows_to_public(
        enrichment
            .labels
            .get(&row.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    );
    let assignees = enrichment
        .assignees
        .get(&row.id)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|a| IssueAssigneePublic {
            user_id: a.user_id.clone(),
            username: a.username.clone(),
            display_name: a.display_name.clone(),
        })
        .collect();
    let reactions = reaction_groups_to_public(
        enrichment
            .reactions
            .get(&row.id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .to_vec(),
    );
    let milestone = row
        .milestone_id
        .as_ref()
        .and_then(|id| enrichment.milestones.get(id))
        .map(|m| {
            Box::new(crate::milestone::milestone_to_public(
                m,
                m.creator_id
                    .as_ref()
                    .and_then(|id| enrichment.usernames.get(id).cloned()),
            ))
        });
    Ok(IssuePublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        number: row.number,
        title: row.title.clone(),
        body: row.body.clone(),
        state,
        author_id: row.author_id.clone(),
        author_username,
        closed_at: row.closed_at.clone(),
        closed_by: row.closed_by.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
        labels,
        assignees,
        reactions,
        milestone,
        comment_count: row.comment_count,
    })
}

pub async fn to_public(ctx: &RpcCtx, row: &IssueRow) -> Result<IssuePublic, AppError> {
    let rows = std::slice::from_ref(row);
    let enrichment = load_enrichment(ctx, rows).await?;
    issue_to_public(row, &enrichment)
}

pub async fn to_public_many(ctx: &RpcCtx, rows: &[IssueRow]) -> Result<Vec<IssuePublic>, AppError> {
    let enrichment = load_enrichment(ctx, rows).await?;
    rows.iter()
        .map(|row| issue_to_public(row, &enrichment))
        .collect()
}

pub async fn load_issue_in_repo(
    ctx: &RpcCtx,
    repo_id: &str,
    number: i64,
) -> Result<IssueRow, AppError> {
    if number < 1 {
        return Err(issue_not_found());
    }
    ctx.db
        .find_issue_by_repo_number(repo_id, number)
        .await
        .map_err(db_err)?
        .ok_or_else(issue_not_found)
}

pub async fn notify_actions_issue(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    action: &str,
    row: &IssueRow,
    user: &oxidean_db::UserRow,
) {
    let Ok(bare) = crate::git::bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    ) else {
        return;
    };
    let author = match ctx.db.find_user_by_id(&row.author_id).await {
        Ok(Some(u)) => crate::actions::payload::user_embed(&u.username),
        _ => serde_json::json!({}),
    };
    let repository = crate::actions::payload::repository_embed(
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.default_branch,
    );
    let sender = crate::actions::payload::user_embed(&user.username);
    let payload = crate::actions::payload::issues_payload(
        action,
        row.number,
        &row.title,
        &row.state,
        &row.body,
        &author,
        &repository,
        &sender,
    );
    crate::actions::events::notify_lifecycle_actions(
        &ctx.db,
        ctx.git.as_ref(),
        &crate::actions::events::LifecycleEvent {
            bare_repo: bare,
            repository_id: accessible.row.id.clone(),
            default_branch: accessible.row.default_branch.clone(),
            event_name: "issues",
            action: action.to_string(),
            payload,
            actor_id: Some(user.id.clone()),
            job_token_actor: crate::actions::events::rpc_caller_is_job_token(ctx),
            actions_enabled_instance: crate::actions::env_actions_enabled(),
        },
    )
    .await;
}

/// Set/clear an issue's milestone — shared by `issue.milestone.set`,
/// `issue.create`, and `issue.update`. Emits `issues.milestoned`/
/// `demilestoned` (webhook + Actions lifecycle) only on an actual change,
/// matching GitHub.
pub async fn apply_milestone(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    issue: &IssueRow,
    milestone: Option<&oxidean_db::MilestoneRow>,
    user: &oxidean_db::UserRow,
) -> Result<(), AppError> {
    let new_id = milestone.map(|m| m.id.as_str());
    if issue.milestone_id.as_deref() == new_id {
        return Ok(());
    }
    ctx.db
        .set_issue_milestone(&issue.id, new_id)
        .await
        .map_err(db_err)?;
    // GitHub fires `milestoned`/`demilestoned` on the *issues* event with
    // the milestone object embedded — mirroring `notify_actions_label_delta`.
    let (action, embed) = match milestone {
        Some(m) => (
            "milestoned",
            serde_json::json!({
                "number": m.number,
                "title": m.title,
                "state": m.state,
                "due_on": m.due_on,
            }),
        ),
        None => {
            // Demilestoned embeds the *removed* milestone — resolve the
            // previous id so receivers can tell which milestone was dropped.
            let previous = match &issue.milestone_id {
                Some(id) => ctx.db.find_milestone_by_id(id).await.ok().flatten(),
                None => None,
            };
            (
                "demilestoned",
                previous
                    .map(|m| {
                        serde_json::json!({
                            "number": m.number,
                            "title": m.title,
                            "state": m.state,
                            "due_on": m.due_on,
                        })
                    })
                    .unwrap_or(serde_json::Value::Null),
            )
        }
    };
    let mut payload = dispatch::issues_payload(
        action,
        issue.number,
        &issue.title,
        &issue.body,
        &issue.state,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        &user.username,
        &user.id,
    );
    payload["milestone"] = embed.clone();
    dispatch::emit(
        &ctx.db,
        &accessible.row.id,
        "issues",
        action,
        payload,
        &ctx.env_name,
    )
    .await;
    notify_actions_milestone_delta(ctx, accessible, action, issue, embed, user).await;
    Ok(())
}

/// `on: issues` `milestoned`/`demilestoned` — same envelope as
/// `notify_actions_label_delta`, with `payload.milestone` naming the
/// added/removed milestone.
pub async fn notify_actions_milestone_delta(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    action: &str,
    row: &IssueRow,
    milestone: serde_json::Value,
    user: &oxidean_db::UserRow,
) {
    let Ok(bare) = crate::git::bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    ) else {
        return;
    };
    let author = match ctx.db.find_user_by_id(&row.author_id).await {
        Ok(Some(u)) => crate::actions::payload::user_embed(&u.username),
        _ => serde_json::json!({}),
    };
    let repository = crate::actions::payload::repository_embed(
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.default_branch,
    );
    let sender = crate::actions::payload::user_embed(&user.username);
    let mut payload = crate::actions::payload::issues_payload(
        action,
        row.number,
        &row.title,
        &row.state,
        &row.body,
        &author,
        &repository,
        &sender,
    );
    payload["milestone"] = milestone;
    crate::actions::events::notify_lifecycle_actions(
        &ctx.db,
        ctx.git.as_ref(),
        &crate::actions::events::LifecycleEvent {
            bare_repo: bare,
            repository_id: accessible.row.id.clone(),
            default_branch: accessible.row.default_branch.clone(),
            event_name: "issues",
            action: action.to_string(),
            payload,
            actor_id: Some(user.id.clone()),
            job_token_actor: crate::actions::events::rpc_caller_is_job_token(ctx),
            actions_enabled_instance: crate::actions::env_actions_enabled(),
        },
    )
    .await;
}
