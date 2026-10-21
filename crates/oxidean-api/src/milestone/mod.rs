//! Milestone RPC handlers — repo-scoped `#N` milestones plus the
//! issue-link surface. Mutations fire `milestone` lifecycle events
//! (GitHub activity types `created`/`closed`/`opened`/`edited`/`deleted`)
//! and `issues.milestoned`/`demilestoned` on link changes, plus matching
//! webhook emits.

use std::collections::HashMap;

use oxidean_core::{
    AppError, CreateMilestoneRequest, DeleteMilestoneResponse, IssuePublic, MilestoneListRequest,
    MilestoneListResponse, MilestonePublic, MilestoneRefRequest, SetIssueMilestoneRequest,
    UpdateMilestoneRequest,
};
use oxidean_db::MilestoneRow;
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::issue::acl;
use crate::rpc::RpcCtx;
use crate::webhook::dispatch;

/// Title soft cap (matches the issue title cap).
const TITLE_MAX_CHARS: usize = 1_024;
/// Description soft cap (matches the issue body cap).
const DESCRIPTION_MAX_CHARS: usize = 65_536;

fn db_err(e: String) -> AppError {
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

fn milestone_not_found() -> AppError {
    AppError::new("milestone.not_found", "Milestone not found")
}

fn validate_title(title: &str) -> Result<&str, AppError> {
    let t = title.trim();
    if t.is_empty() {
        return Err(AppError::new("rpc.bad_input", "title is required"));
    }
    if t.chars().count() > TITLE_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("title exceeds {TITLE_MAX_CHARS} characters"),
        ));
    }
    Ok(t)
}

fn validate_description(description: Option<&str>) -> Result<String, AppError> {
    let stored = description.unwrap_or("").to_string();
    if stored.chars().count() > DESCRIPTION_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("description exceeds {DESCRIPTION_MAX_CHARS} characters"),
        ));
    }
    Ok(stored)
}

/// `YYYY-MM-DD` due dates; full ISO-8601 timestamps reduce to their date
/// part (GitHub REST accepts both shapes).
fn validate_due_on(due_on: &str) -> Result<String, AppError> {
    let t = due_on.trim();
    let date = t.get(..10).unwrap_or(t);
    if chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
        return Err(AppError::new(
            "rpc.bad_input",
            "due_on must be a YYYY-MM-DD date",
        ));
    }
    Ok(date.to_string())
}

pub(crate) fn milestone_to_public(
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

/// Batch-resolve `creator_id` → username for a milestone page.
async fn creator_usernames(ctx: &RpcCtx, rows: &[MilestoneRow]) -> HashMap<String, String> {
    let mut ids: Vec<String> = rows.iter().filter_map(|r| r.creator_id.clone()).collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return HashMap::new();
    }
    ctx.db
        .find_users_by_ids(&ids)
        .await
        .map(|users| users.into_iter().map(|u| (u.id, u.username)).collect())
        .unwrap_or_default()
}

async fn load_milestone_in_repo(
    ctx: &RpcCtx,
    repo_id: &str,
    number: i64,
) -> Result<MilestoneRow, AppError> {
    if number < 1 {
        return Err(milestone_not_found());
    }
    ctx.db
        .find_milestone_by_repo_number(repo_id, number)
        .await
        .map_err(db_err)?
        .ok_or_else(milestone_not_found)
}

/// `milestone.list` — Read+; `state` filters `open` (default) / `closed` / `all`.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<MilestoneListResponse, AppError> {
    let req: MilestoneListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid milestone.list input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let state = match req.state.as_deref().unwrap_or("open").trim() {
        "open" => "open",
        "closed" => "closed",
        "all" => "all",
        other => {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("invalid milestone state filter: {other}"),
            ))
        }
    };
    let rows = ctx
        .db
        .list_milestones_for_repo(&accessible.row.id, state)
        .await
        .map_err(db_err)?;
    let usernames = creator_usernames(ctx, &rows).await;
    let milestones = rows
        .iter()
        .map(|r| {
            milestone_to_public(
                r,
                r.creator_id
                    .as_ref()
                    .and_then(|id| usernames.get(id).cloned()),
            )
        })
        .collect();
    Ok(MilestoneListResponse { milestones })
}

/// `milestone.get` — Read+; resolves `#N`.
pub async fn get(ctx: &RpcCtx, input: serde_json::Value) -> Result<MilestonePublic, AppError> {
    let req: MilestoneRefRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid milestone.get input: {e}")))?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_milestone_in_repo(ctx, &accessible.row.id, req.number).await?;
    let usernames = creator_usernames(ctx, std::slice::from_ref(&row)).await;
    Ok(milestone_to_public(
        &row,
        row.creator_id
            .as_ref()
            .and_then(|id| usernames.get(id).cloned()),
    ))
}

/// `milestone.create` — Write+; allocates per-repo `#N`; fires `created`.
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<MilestonePublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: CreateMilestoneRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid milestone.create input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let title = validate_title(&req.title)?.to_string();
    let description = validate_description(req.description.as_deref())?;
    let due_on = req.due_on.as_deref().map(validate_due_on).transpose()?;

    if ctx
        .db
        .find_milestone_by_repo_title(&accessible.row.id, &title)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "milestone.already_exists",
            "a milestone with this title already exists",
        ));
    }

    let number = ctx
        .db
        .allocate_next_milestone_number(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_milestone(
            &id,
            &accessible.row.id,
            number,
            &title,
            &description,
            due_on.as_deref(),
            Some(&user.id),
        )
        .await
        .map_err(db_err)?;

    let payload = milestone_webhook_payload("created", &row, &accessible, &user);
    dispatch::emit(
        &ctx.db,
        &accessible.row.id,
        "milestone",
        "created",
        payload,
        &ctx.env_name,
    )
    .await;
    notify_actions_milestone(ctx, &accessible, "created", &row, &user).await;
    Ok(milestone_to_public(&row, Some(user.username.clone())))
}

/// `milestone.update` — Write+; field edits fire `edited`, state
/// transitions fire `closed`/`opened` (GitHub milestone activity types).
pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<MilestonePublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: UpdateMilestoneRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid milestone.update input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let before = load_milestone_in_repo(ctx, &accessible.row.id, req.number).await?;

    let title = req.title.as_deref().map(validate_title).transpose()?;
    let description = req
        .description
        .as_deref()
        .map(|d| validate_description(Some(d)))
        .transpose()?;
    let due_on = req
        .due_on
        .as_ref()
        .map(|d| d.as_deref().map(validate_due_on).transpose())
        .transpose()?;
    let state = match req.state.as_deref().map(str::trim) {
        None => None,
        Some("open") => Some("open"),
        Some("closed") => Some("closed"),
        Some(other) => {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("invalid milestone state: {other}"),
            ))
        }
    };

    if let Some(t) = title {
        if !t.eq_ignore_ascii_case(&before.title)
            && ctx
                .db
                .find_milestone_by_repo_title(&accessible.row.id, t)
                .await
                .map_err(db_err)?
                .is_some()
        {
            return Err(AppError::new(
                "milestone.already_exists",
                "a milestone with this title already exists",
            ));
        }
    }

    let mut row = ctx
        .db
        .update_milestone(
            &before.id,
            title,
            description.as_deref(),
            due_on.as_ref().map(|d| d.as_deref()),
        )
        .await
        .map_err(db_err)?
        .ok_or_else(milestone_not_found)?;

    let edited = title.is_some() || description.is_some() || req.due_on.as_ref().is_some();
    let mut actions: Vec<&str> = Vec::new();
    if edited {
        actions.push("edited");
    }
    if let Some(s) = state {
        if s != before.state {
            row = ctx
                .db
                .set_milestone_state(&row.id, s)
                .await
                .map_err(db_err)?
                .ok_or_else(milestone_not_found)?;
            actions.push(if s == "closed" { "closed" } else { "opened" });
        }
    }

    for action in actions {
        let payload = milestone_webhook_payload(action, &row, &accessible, &user);
        dispatch::emit(
            &ctx.db,
            &accessible.row.id,
            "milestone",
            action,
            payload,
            &ctx.env_name,
        )
        .await;
        notify_actions_milestone(ctx, &accessible, action, &row, &user).await;
    }
    Ok(milestone_to_public(&row, Some(user.username.clone())))
}

/// `milestone.delete` — Write+; linked issues keep their data, the link
/// clears (`issues.milestone_id` → `ON DELETE SET NULL`); fires `deleted`.
pub async fn delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DeleteMilestoneResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: MilestoneRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid milestone.delete input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let row = load_milestone_in_repo(ctx, &accessible.row.id, req.number).await?;

    let payload = milestone_webhook_payload("deleted", &row, &accessible, &user);
    dispatch::emit(
        &ctx.db,
        &accessible.row.id,
        "milestone",
        "deleted",
        payload,
        &ctx.env_name,
    )
    .await;
    notify_actions_milestone(ctx, &accessible, "deleted", &row, &user).await;

    ctx.db.delete_milestone(&row.id).await.map_err(db_err)?;
    Ok(DeleteMilestoneResponse { ok: true })
}

/// `issue.milestone.set` — Write+; `milestone` is the milestone `#N`,
/// `null` clears. Fires `issues.milestoned`/`demilestoned` only on an
/// actual change (GitHub semantics).
pub async fn set_issue_milestone(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<IssuePublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: SetIssueMilestoneRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.milestone.set input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let issue = crate::issue::load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    let milestone = match req.milestone {
        Some(number) => Some(load_milestone_in_repo(ctx, &accessible.row.id, number).await?),
        None => None,
    };
    crate::issue::apply_milestone(ctx, &accessible, &issue, milestone.as_ref(), &user).await?;
    let refreshed = crate::issue::load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    crate::issue::to_public(ctx, &refreshed).await
}

/// GitHub-shaped `milestone` webhook/lifecycle payload.
fn milestone_webhook_payload(
    action: &str,
    row: &MilestoneRow,
    accessible: &crate::repo::AccessibleRepo,
    user: &oxidean_db::UserRow,
) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "milestone": {
            "number": row.number,
            "title": row.title,
            "description": row.description,
            "state": row.state,
            "due_on": row.due_on,
            "open_issues": row.open_issues,
            "closed_issues": row.closed_issues,
            "closed_at": row.closed_at,
            "created_at": row.created_at,
            "updated_at": row.updated_at,
            "html_url": format!("/{}/{}/milestone/{}", accessible.owner_username, accessible.row.name, row.number),
        },
        "repository": {
            "id": accessible.row.id,
            "name": accessible.row.name,
            "full_name": format!("{}/{}", accessible.owner_username, accessible.row.name),
            "owner": { "login": accessible.owner_username },
        },
        "sender": {
            "login": user.username,
            "id": user.id,
        }
    })
}

/// `on: milestone` lifecycle dispatch — soft-fails like every lifecycle fan-out.
async fn notify_actions_milestone(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    action: &str,
    row: &MilestoneRow,
    user: &oxidean_db::UserRow,
) {
    let Ok(bare) = crate::git::bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    ) else {
        return;
    };
    let repository = crate::actions::payload::repository_embed(
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.default_branch,
    );
    let sender = crate::actions::payload::user_embed(&user.username);
    let payload = crate::actions::payload::milestone_payload(action, row, &repository, &sender);
    crate::actions::events::notify_lifecycle_actions(
        &ctx.db,
        ctx.git.as_ref(),
        &crate::actions::events::LifecycleEvent {
            bare_repo: bare,
            repository_id: accessible.row.id.clone(),
            default_branch: accessible.row.default_branch.clone(),
            event_name: "milestone",
            action: action.to_string(),
            payload,
            actor_id: Some(user.id.clone()),
            job_token_actor: crate::actions::events::rpc_caller_is_job_token(ctx),
            actions_enabled_instance: crate::actions::env_actions_enabled(),
        },
    )
    .await;
}
