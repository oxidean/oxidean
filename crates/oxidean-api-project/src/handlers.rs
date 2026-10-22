//! Project board RPC handlers — repo-scoped classic projects (GitHub
//! Projects API shape). `projects.*` mutations fire `project` /
//! `project_column` / `project_card` lifecycle events (GitHub activity
//! types) and matching webhook emits.
//!
//! Scope rules: columns and cards resolve only inside their parent
//! (board `#N` for the repo, column inside the board, card inside the
//! board) — cross-board ids return `project.*.not_found` rather than
//! touching another repo's rows.

use std::collections::HashMap;

use oxidean_core::{
    AppError, ProjectCardCreateRequest, ProjectCardMoveRequest, ProjectCardPublic,
    ProjectCardRefRequest, ProjectCardUpdateRequest, ProjectCardsListRequest,
    ProjectCardsListResponse, ProjectColumnCreateRequest, ProjectColumnMoveRequest,
    ProjectColumnPublic, ProjectColumnRefRequest, ProjectColumnUpdateRequest,
    ProjectColumnsListRequest, ProjectColumnsListResponse, ProjectCreateRequest,
    ProjectDeleteResponse, ProjectDeleteSimpleResponse, ProjectPublic, ProjectRefRequest,
    ProjectUpdateRequest, ProjectsListRequest, ProjectsListResponse,
};
use oxidean_db::{ProjectCardRow, ProjectColumnRow, ProjectRow};
use serde_json::json;
use uuid::Uuid;

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::repo::{
    ensure_not_archived, meets, not_found, require_unit_enabled, resolve_repo_for_read,
    AccessibleRepo, Capability, RepoUnit,
};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_api_core::webhook::dispatch;

/// Title soft cap (matches the issue title cap).
const TITLE_MAX_CHARS: usize = 1_024;
/// Body / note soft cap (matches the issue body cap).
const BODY_MAX_CHARS: usize = 65_536;
/// Column name cap (GitHub rejects absurdly long column names).
const COLUMN_NAME_MAX_CHARS: usize = 1_024;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("project db error: {e}");
        AppError::new("project.internal", "project operation failed")
    }
}

fn project_not_found() -> AppError {
    AppError::new("project.not_found", "Project not found")
}

fn column_not_found() -> AppError {
    AppError::new("project.column_not_found", "Project column not found")
}

fn card_not_found() -> AppError {
    AppError::new("project.card_not_found", "Project card not found")
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

fn validate_body(body: Option<&str>) -> Result<String, AppError> {
    let stored = body.unwrap_or("").to_string();
    if stored.chars().count() > BODY_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("body exceeds {BODY_MAX_CHARS} characters"),
        ));
    }
    Ok(stored)
}

fn validate_column_name(name: &str) -> Result<&str, AppError> {
    let n = name.trim();
    if n.is_empty() {
        return Err(AppError::new("rpc.bad_input", "column name is required"));
    }
    if n.chars().count() > COLUMN_NAME_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("column name exceeds {COLUMN_NAME_MAX_CHARS} characters"),
        ));
    }
    Ok(n)
}

/// `projects.*` read gate — repo Read+ plus the Projects unit flag.
async fn resolve_for_read(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    require_unit_enabled(ctx, RepoUnit::Projects, &accessible.row.id).await?;
    Ok(accessible)
}

/// `projects.*` write gate — repo Write+, unit flag, archived read-only.
async fn resolve_for_write(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    require_unit_enabled(ctx, RepoUnit::Projects, &accessible.row.id).await?;
    ensure_not_archived(&accessible)?;
    Ok(accessible)
}

async fn load_project_in_repo(
    ctx: &RpcCtx,
    repo_id: &str,
    number: i64,
) -> Result<ProjectRow, AppError> {
    if number < 1 {
        return Err(project_not_found());
    }
    ctx.db
        .find_project_by_repo_number(repo_id, number)
        .await
        .map_err(db_err)?
        .ok_or_else(project_not_found)
}

async fn load_column_in_project(
    ctx: &RpcCtx,
    project_id: &str,
    column_id: &str,
) -> Result<ProjectColumnRow, AppError> {
    ctx.db
        .find_column_in_project(project_id, column_id)
        .await
        .map_err(db_err)?
        .ok_or_else(column_not_found)
}

/// Card scoped through its column → project — cross-board ids don't resolve.
async fn load_card_in_project(
    ctx: &RpcCtx,
    project_id: &str,
    card_id: &str,
) -> Result<ProjectCardRow, AppError> {
    let Some(card) = ctx.db.find_card_by_id(card_id).await.map_err(db_err)? else {
        return Err(card_not_found());
    };
    ctx.db
        .find_column_in_project(project_id, &card.column_id)
        .await
        .map_err(db_err)?
        .ok_or_else(card_not_found)?;
    Ok(card)
}

fn board_html_url(accessible: &AccessibleRepo, number: i64) -> String {
    format!(
        "/{}/{}/projects/{number}",
        accessible.owner_username, accessible.row.name
    )
}

fn project_to_public(
    row: &ProjectRow,
    accessible: &AccessibleRepo,
    creator_username: Option<String>,
) -> ProjectPublic {
    ProjectPublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        number: row.number,
        title: row.title.clone(),
        body: row.body.clone(),
        state: row.state.clone(),
        creator_id: row.creator_id.clone(),
        creator_username,
        html_url: board_html_url(accessible, row.number),
        closed_at: row.closed_at.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

fn column_to_public(row: &ProjectColumnRow) -> ProjectColumnPublic {
    ProjectColumnPublic {
        id: row.id.clone(),
        project_id: row.project_id.clone(),
        name: row.name.clone(),
        position: row.position,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

/// `/{owner}/{repo}/issues/{n}` | `/{owner}/{repo}/pulls/{n}` — the linked
/// entity's web route (webhook `content_url` + card enrichment).
fn content_url(accessible: &AccessibleRepo, content_type: &str, number: i64) -> String {
    let unit = if content_type == "pull_request" {
        "pulls"
    } else {
        "issues"
    };
    format!(
        "/{}/{}/{unit}/{number}",
        accessible.owner_username, accessible.row.name
    )
}

/// Card → public shape. `content` enrichment (number/title/url) resolves
/// the linked row — missing rows leave the fields absent rather than
/// failing the whole list (the card still displays as a stale link).
async fn card_to_public(
    ctx: &RpcCtx,
    row: &ProjectCardRow,
    accessible: &AccessibleRepo,
    usernames: &HashMap<String, String>,
) -> ProjectCardPublic {
    let mut public = ProjectCardPublic {
        id: row.id.clone(),
        column_id: row.column_id.clone(),
        note: row.note.clone(),
        content_type: row.content_type.clone(),
        content_id: row.content_id.clone(),
        content_number: None,
        content_title: None,
        content_state: None,
        content_url: None,
        position: row.position,
        archived: row.archived,
        creator_id: row.creator_id.clone(),
        creator_username: row
            .creator_id
            .as_ref()
            .and_then(|id| usernames.get(id).cloned()),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    };
    if let (Some(kind), Some(id)) = (row.content_type.as_deref(), row.content_id.as_deref()) {
        match kind {
            "issue" => {
                if let Ok(Some(issue)) = ctx.db.find_issue_by_id(id).await {
                    public.content_number = Some(issue.number);
                    public.content_title = Some(issue.title.clone());
                    public.content_state = Some(issue.state.clone());
                    public.content_url = Some(content_url(accessible, "issue", issue.number));
                }
            }
            "pull_request" => {
                if let Ok(Some(pull)) = ctx.db.find_pull_by_id(id).await {
                    public.content_number = Some(pull.number);
                    public.content_title = Some(pull.title.clone());
                    public.content_state = Some(if pull.merged_at.is_some() {
                        "merged".to_string()
                    } else {
                        pull.state.clone()
                    });
                    public.content_url = Some(content_url(accessible, "pull_request", pull.number));
                }
            }
            _ => {}
        }
    }
    public
}

/// Batch-resolve `creator_id` → username across any row set.
async fn creator_usernames(
    ctx: &RpcCtx,
    creator_ids: impl Iterator<Item = Option<String>>,
) -> HashMap<String, String> {
    let mut ids: Vec<String> = creator_ids.flatten().collect();
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

/// Shared lifecycle fan-out — the webhook emit plus the
/// `on: <event>` Actions trigger (job-token suppressed, soft-fail).
async fn emit_and_notify(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    event_name: &'static str,
    action: &str,
    payload: serde_json::Value,
    user: &oxidean_db::UserRow,
) {
    dispatch::emit(
        &ctx.db,
        &accessible.row.id,
        event_name,
        action,
        payload.clone(),
        &ctx.env_name,
    )
    .await;
    let Ok(bare) = oxidean_api_core::git::bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    ) else {
        return;
    };
    oxidean_api_core::actions::events::notify_lifecycle_actions(
        &ctx.db,
        ctx.git.as_ref(),
        &oxidean_api_core::actions::events::LifecycleEvent {
            bare_repo: bare,
            repository_id: accessible.row.id.clone(),
            default_branch: accessible.row.default_branch.clone(),
            event_name,
            action: action.to_string(),
            payload,
            actor_id: Some(user.id.clone()),
            job_token_actor: oxidean_api_core::actions::events::rpc_caller_is_job_token(ctx),
            actions_enabled_instance: oxidean_api_core::actions::env_actions_enabled(),
        },
    )
    .await;
}

fn repository_and_sender(
    accessible: &AccessibleRepo,
    user: &oxidean_db::UserRow,
) -> (serde_json::Value, serde_json::Value) {
    (
        oxidean_api_core::actions::payload::repository_embed(
            &accessible.owner_username,
            &accessible.row.name,
            &accessible.row.default_branch,
        ),
        oxidean_api_core::actions::payload::user_embed(&user.username),
    )
}

// ---------------------------------------------------------------------------
// Boards
// ---------------------------------------------------------------------------

/// `projects.list` — Read+; `state` filters `open` (default) / `closed` / `all`.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectsListResponse, AppError> {
    let req: ProjectsListRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid projects.list input: {e}")))?;
    let accessible = resolve_for_read(ctx, &req.owner, &req.name).await?;
    let state = match req.state.as_deref().unwrap_or("open").trim() {
        "open" => "open",
        "closed" => "closed",
        "all" => "all",
        other => {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("invalid project state filter: {other}"),
            ))
        }
    };
    let rows = ctx
        .db
        .list_projects_for_repo(&accessible.row.id, state)
        .await
        .map_err(db_err)?;
    let usernames = creator_usernames(ctx, rows.iter().map(|r| r.creator_id.clone())).await;
    let projects = rows
        .iter()
        .map(|r| {
            project_to_public(
                r,
                &accessible,
                r.creator_id
                    .as_ref()
                    .and_then(|id| usernames.get(id).cloned()),
            )
        })
        .collect();
    Ok(ProjectsListResponse { projects })
}

/// `projects.get` — Read+; resolves `#N`.
pub async fn get(ctx: &RpcCtx, input: serde_json::Value) -> Result<ProjectPublic, AppError> {
    let req: ProjectRefRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid projects.get input: {e}")))?;
    let accessible = resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;
    let usernames = creator_usernames(ctx, std::iter::once(row.creator_id.clone())).await;
    Ok(project_to_public(
        &row,
        &accessible,
        row.creator_id
            .as_ref()
            .and_then(|id| usernames.get(id).cloned()),
    ))
}

/// `projects.create` — Write+; allocates per-repo `#N`; fires `created`.
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<ProjectPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.create input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let title = validate_title(&req.title)?.to_string();
    let body = validate_body(req.body.as_deref())?;

    let number = ctx
        .db
        .allocate_next_project_number(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_project(
            &id,
            &accessible.row.id,
            number,
            &title,
            &body,
            Some(&user.id),
        )
        .await
        .map_err(db_err)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let payload = oxidean_api_core::actions::payload::project_payload(
        "created",
        &row,
        &board_html_url(&accessible, row.number),
        None,
        &repository,
        &sender,
    );
    emit_and_notify(ctx, &accessible, "project", "created", payload, &user).await;
    Ok(project_to_public(
        &row,
        &accessible,
        Some(user.username.clone()),
    ))
}

/// `projects.update` — Write+; field edits fire `edited`, state
/// transitions fire `closed`/`reopened` (GitHub project activity types).
pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<ProjectPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.update input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let before = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;

    let title = req.title.as_deref().map(validate_title).transpose()?;
    let body = req
        .body
        .as_deref()
        .map(|b| validate_body(Some(b)))
        .transpose()?;
    let state = match req.state.as_deref().map(str::trim) {
        None => None,
        Some("open") => Some("open"),
        Some("closed") => Some("closed"),
        Some(other) => {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("invalid project state: {other}"),
            ))
        }
    };

    let mut row = ctx
        .db
        .update_project(&before.id, title, body.as_deref())
        .await
        .map_err(db_err)?
        .ok_or_else(project_not_found)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let html_url = board_html_url(&accessible, row.number);
    let mut actions: Vec<(&str, Option<serde_json::Value>)> = Vec::new();
    if title.is_some() || body.is_some() {
        let mut changes = json!({});
        if let Some(t) = title {
            if t != before.title {
                changes["name"] = json!({ "from": before.title });
            }
        }
        if let Some(b) = body.as_deref() {
            if b != before.body {
                changes["body"] = json!({ "from": before.body });
            }
        }
        if !changes.as_object().is_none_or(|m| m.is_empty()) {
            actions.push(("edited", Some(changes)));
        }
    }
    if let Some(s) = state {
        if s != before.state {
            row = ctx
                .db
                .set_project_state(&row.id, s)
                .await
                .map_err(db_err)?
                .ok_or_else(project_not_found)?;
            actions.push((if s == "closed" { "closed" } else { "reopened" }, None));
        }
    }

    for (action, changes) in actions {
        let payload = oxidean_api_core::actions::payload::project_payload(
            action,
            &row,
            &html_url,
            changes.as_ref(),
            &repository,
            &sender,
        );
        emit_and_notify(ctx, &accessible, "project", action, payload, &user).await;
    }
    Ok(project_to_public(
        &row,
        &accessible,
        Some(user.username.clone()),
    ))
}

/// `projects.delete` — Write+; columns/cards cascade; fires `deleted`.
pub async fn delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectDeleteResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.delete input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let row = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let payload = oxidean_api_core::actions::payload::project_payload(
        "deleted",
        &row,
        &board_html_url(&accessible, row.number),
        None,
        &repository,
        &sender,
    );
    emit_and_notify(ctx, &accessible, "project", "deleted", payload, &user).await;

    ctx.db.delete_project(&row.id).await.map_err(db_err)?;
    Ok(ProjectDeleteResponse { ok: true })
}

// ---------------------------------------------------------------------------
// Columns
// ---------------------------------------------------------------------------

/// `projects.columns.list` — Read+; position-ordered columns of `#N`.
pub async fn columns_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectColumnsListResponse, AppError> {
    let req: ProjectColumnsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.columns.list input: {e}"),
        )
    })?;
    let accessible = resolve_for_read(ctx, &req.owner, &req.name).await?;
    let project = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;
    let rows = ctx
        .db
        .list_columns_for_project(&project.id)
        .await
        .map_err(db_err)?;
    Ok(ProjectColumnsListResponse {
        columns: rows.iter().map(column_to_public).collect(),
    })
}

/// `projects.columns.create` — Write+; appends the column; fires `created`.
pub async fn columns_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectColumnPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectColumnCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.columns.create input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let project = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;
    let column_name = validate_column_name(&req.column_name)?.to_string();

    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_project_column(&id, &project.id, &column_name)
        .await
        .map_err(db_err)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let payload = oxidean_api_core::actions::payload::project_column_payload(
        "created",
        &row,
        &board_html_url(&accessible, project.number),
        None,
        &repository,
        &sender,
    );
    emit_and_notify(
        ctx,
        &accessible,
        "project_column",
        "created",
        payload,
        &user,
    )
    .await;
    Ok(column_to_public(&row))
}

/// `projects.columns.update` — Write+; rename fires `edited`.
pub async fn columns_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectColumnPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectColumnUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.columns.update input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let project = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;
    let before = load_column_in_project(ctx, &project.id, &req.column).await?;
    let column_name = validate_column_name(&req.column_name)?.to_string();

    let row = ctx
        .db
        .update_project_column(&before.id, &column_name)
        .await
        .map_err(db_err)?
        .ok_or_else(column_not_found)?;

    if column_name != before.name {
        let (repository, sender) = repository_and_sender(&accessible, &user);
        let changes = json!({ "name": { "from": before.name } });
        let payload = oxidean_api_core::actions::payload::project_column_payload(
            "edited",
            &row,
            &board_html_url(&accessible, project.number),
            Some(&changes),
            &repository,
            &sender,
        );
        emit_and_notify(ctx, &accessible, "project_column", "edited", payload, &user).await;
    }
    Ok(column_to_public(&row))
}

/// `projects.columns.move` — Write+; re-orders inside the board; fires `moved`.
pub async fn columns_move(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectColumnPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectColumnMoveRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.columns.move input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let project = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;
    let before = load_column_in_project(ctx, &project.id, &req.column).await?;

    let row = ctx
        .db
        .move_project_column(&before.id, req.position)
        .await
        .map_err(db_err)?
        .ok_or_else(column_not_found)?;

    if row.position != before.position {
        let (repository, sender) = repository_and_sender(&accessible, &user);
        let payload = oxidean_api_core::actions::payload::project_column_payload(
            "moved",
            &row,
            &board_html_url(&accessible, project.number),
            None,
            &repository,
            &sender,
        );
        emit_and_notify(ctx, &accessible, "project_column", "moved", payload, &user).await;
    }
    Ok(column_to_public(&row))
}

/// `projects.columns.delete` — Write+; cards cascade; fires `deleted`.
pub async fn columns_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectDeleteSimpleResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectColumnRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.columns.delete input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let project = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;
    let row = load_column_in_project(ctx, &project.id, &req.column).await?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let payload = oxidean_api_core::actions::payload::project_column_payload(
        "deleted",
        &row,
        &board_html_url(&accessible, project.number),
        None,
        &repository,
        &sender,
    );
    emit_and_notify(
        ctx,
        &accessible,
        "project_column",
        "deleted",
        payload,
        &user,
    )
    .await;

    ctx.db
        .delete_project_column(&row.id)
        .await
        .map_err(db_err)?;
    Ok(ProjectDeleteSimpleResponse { ok: true })
}

// ---------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------

/// `projects.cards.list` — Read+; `archived` includes archived cards.
pub async fn cards_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectCardsListResponse, AppError> {
    let req: ProjectCardsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.cards.list input: {e}"),
        )
    })?;
    let accessible = resolve_for_read(ctx, &req.owner, &req.name).await?;
    // Scope check: the column must belong to a project in this repo.
    let column = ctx
        .db
        .find_column_by_id(&req.column)
        .await
        .map_err(db_err)?
        .ok_or_else(column_not_found)?;
    let Some(project) = ctx
        .db
        .find_project_by_id(&column.project_id)
        .await
        .map_err(db_err)?
    else {
        return Err(project_not_found());
    };
    if project.repo_id != accessible.row.id {
        return Err(column_not_found());
    }
    let rows = ctx
        .db
        .list_cards_for_column(&req.column, req.archived.unwrap_or(false))
        .await
        .map_err(db_err)?;
    let usernames = creator_usernames(ctx, rows.iter().map(|r| r.creator_id.clone())).await;
    let mut cards = Vec::with_capacity(rows.len());
    for row in &rows {
        cards.push(card_to_public(ctx, row, &accessible, &usernames).await);
    }
    Ok(ProjectCardsListResponse { cards })
}

/// Validate a linked card — `content_type` `issue`/`pull_request` and the
/// row must exist **in this repo**.
async fn resolve_card_content(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    content_type: &str,
    content_id: &str,
) -> Result<(), AppError> {
    match content_type {
        "issue" => {
            let Some(issue) = ctx.db.find_issue_by_id(content_id).await.map_err(db_err)? else {
                return Err(AppError::new(
                    "project.content_not_found",
                    "linked issue not found",
                ));
            };
            if issue.repo_id != accessible.row.id {
                return Err(AppError::new(
                    "project.content_not_found",
                    "linked issue not found",
                ));
            }
        }
        "pull_request" => {
            let Some(pull) = ctx.db.find_pull_by_id(content_id).await.map_err(db_err)? else {
                return Err(AppError::new(
                    "project.content_not_found",
                    "linked pull request not found",
                ));
            };
            if pull.repo_id != accessible.row.id {
                return Err(AppError::new(
                    "project.content_not_found",
                    "linked pull request not found",
                ));
            }
        }
        other => {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("invalid content_type: {other}"),
            ))
        }
    }
    Ok(())
}

/// `projects.cards.create` — Write+; note cards or issue/pull links; fires `created`.
pub async fn cards_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectCardPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectCardCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.cards.create input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let column = ctx
        .db
        .find_column_by_id(&req.column)
        .await
        .map_err(db_err)?
        .ok_or_else(column_not_found)?;
    // Scope check: the column must belong to a project in this repo.
    let Some(project) = ctx
        .db
        .find_project_by_id(&column.project_id)
        .await
        .map_err(db_err)?
    else {
        return Err(project_not_found());
    };
    if project.repo_id != accessible.row.id {
        return Err(column_not_found());
    }

    let note = req.note.as_deref().map(str::trim);
    let has_link = req.content_type.is_some() || req.content_id.is_some();
    match (note, has_link) {
        (Some(n), false) if !n.is_empty() => {
            if n.chars().count() > BODY_MAX_CHARS {
                return Err(AppError::new(
                    "rpc.bad_input",
                    format!("note exceeds {BODY_MAX_CHARS} characters"),
                ));
            }
        }
        (None, true) => {}
        _ => {
            return Err(AppError::new(
                "rpc.bad_input",
                "exactly one of note or content_type+content_id is required",
            ))
        }
    }
    let (content_type, content_id) = if has_link {
        let ct = req.content_type.as_deref().unwrap_or_default();
        let ci = req.content_id.as_deref().unwrap_or_default();
        if ci.is_empty() {
            return Err(AppError::new("rpc.bad_input", "content_id is required"));
        }
        resolve_card_content(ctx, &accessible, ct, ci).await?;
        (Some(ct.to_string()), Some(ci.to_string()))
    } else {
        (None, None)
    };

    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_project_card(
            &id,
            &column.id,
            note,
            content_type.as_deref(),
            content_id.as_deref(),
            Some(&user.id),
        )
        .await
        .map_err(db_err)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let card_url = card_content_url(ctx, &accessible, &row).await;
    let payload = oxidean_api_core::actions::payload::project_card_payload(
        "created",
        &row,
        card_url.as_deref(),
        None,
        &repository,
        &sender,
    );
    emit_and_notify(ctx, &accessible, "project_card", "created", payload, &user).await;
    let usernames = creator_usernames(ctx, std::iter::once(row.creator_id.clone())).await;
    Ok(card_to_public(ctx, &row, &accessible, &usernames).await)
}

/// The linked entity's web route for webhook `content_url`.
async fn card_content_url(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    row: &ProjectCardRow,
) -> Option<String> {
    let (kind, id) = (row.content_type.as_deref()?, row.content_id.as_deref()?);
    match kind {
        "issue" => ctx
            .db
            .find_issue_by_id(id)
            .await
            .ok()
            .flatten()
            .map(|i| content_url(accessible, "issue", i.number)),
        "pull_request" => ctx
            .db
            .find_pull_by_id(id)
            .await
            .ok()
            .flatten()
            .map(|p| content_url(accessible, "pull_request", p.number)),
        _ => None,
    }
}

/// `projects.cards.update` — Write+; note edits and archive toggles fire `edited`.
pub async fn cards_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectCardPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectCardUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.cards.update input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let project = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;
    let before = load_card_in_project(ctx, &project.id, &req.card).await?;

    let note = req
        .note
        .as_deref()
        .map(|n| {
            let t = n.trim().to_string();
            if t.is_empty() {
                Err(AppError::new("rpc.bad_input", "note cannot be empty"))
            } else if t.chars().count() > BODY_MAX_CHARS {
                Err(AppError::new(
                    "rpc.bad_input",
                    format!("note exceeds {BODY_MAX_CHARS} characters"),
                ))
            } else {
                Ok(t)
            }
        })
        .transpose()?;
    if note.is_some() && before.note.is_none() {
        return Err(AppError::new(
            "rpc.bad_input",
            "linked cards have no editable note",
        ));
    }

    let row = ctx
        .db
        .update_project_card(&before.id, note.as_deref(), req.archived)
        .await
        .map_err(db_err)?
        .ok_or_else(card_not_found)?;

    let mut changes = json!({});
    if let Some(n) = note.as_deref() {
        if Some(n) != before.note.as_deref() {
            changes["note"] = json!({ "from": before.note });
        }
    }
    if let Some(a) = req.archived {
        if a != before.archived {
            changes["archived"] = json!({ "from": before.archived });
        }
    }
    if !changes.as_object().is_none_or(|m| m.is_empty()) {
        let (repository, sender) = repository_and_sender(&accessible, &user);
        let card_url = card_content_url(ctx, &accessible, &row).await;
        let payload = oxidean_api_core::actions::payload::project_card_payload(
            "edited",
            &row,
            card_url.as_deref(),
            Some(&changes),
            &repository,
            &sender,
        );
        emit_and_notify(ctx, &accessible, "project_card", "edited", payload, &user).await;
    }
    let usernames = creator_usernames(ctx, std::iter::once(row.creator_id.clone())).await;
    Ok(card_to_public(ctx, &row, &accessible, &usernames).await)
}

/// `projects.cards.move` — Write+; reorders or re-columns; fires `moved`.
pub async fn cards_move(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectCardPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectCardMoveRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.cards.move input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let project = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;
    let before = load_card_in_project(ctx, &project.id, &req.card).await?;

    let dest_column_id = req.column.as_deref().unwrap_or(&before.column_id);
    if dest_column_id != before.column_id {
        load_column_in_project(ctx, &project.id, dest_column_id).await?;
    }

    let row = ctx
        .db
        .move_project_card(&before.id, dest_column_id, req.position)
        .await
        .map_err(db_err)?
        .ok_or_else(card_not_found)?;

    if row.column_id != before.column_id || row.position != before.position {
        let (repository, sender) = repository_and_sender(&accessible, &user);
        let mut changes = json!({});
        if row.column_id != before.column_id {
            changes["column_id"] = json!({ "from": before.column_id });
        }
        let card_url = card_content_url(ctx, &accessible, &row).await;
        let changes_opt = if changes.as_object().is_none_or(|m| m.is_empty()) {
            None
        } else {
            Some(&changes)
        };
        let payload = oxidean_api_core::actions::payload::project_card_payload(
            "moved",
            &row,
            card_url.as_deref(),
            changes_opt,
            &repository,
            &sender,
        );
        emit_and_notify(ctx, &accessible, "project_card", "moved", payload, &user).await;
    }
    let usernames = creator_usernames(ctx, std::iter::once(row.creator_id.clone())).await;
    Ok(card_to_public(ctx, &row, &accessible, &usernames).await)
}

/// `projects.cards.convert` — Write+; turns a note card into a real issue
/// (GitHub "Convert note to issue"). The created issue fires `issues.opened`;
/// the card fires `project_card.converted` with `changes.note.from`.
pub async fn cards_convert(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectCardPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectCardRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.cards.convert input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let project = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;
    let card = load_card_in_project(ctx, &project.id, &req.card).await?;

    let Some(note) = card.note.clone() else {
        return Err(AppError::new(
            "rpc.bad_input",
            "only note cards can be converted",
        ));
    };
    // GitHub creates a real issue — first line is the title, the trimmed
    // rest the body.
    let mut lines = note.lines();
    let title = validate_title(lines.next().unwrap_or("Untitled"))?.to_string();
    let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    let issue_id = Uuid::new_v4().to_string();
    let issue = ctx
        .db
        .insert_issue(&issue_id, &accessible.row.id, &user.id, &title, &body)
        .await
        .map_err(db_err)?;

    let row = ctx
        .db
        .convert_project_card(&card.id, "issue", &issue.id)
        .await
        .map_err(db_err)?
        .ok_or_else(card_not_found)?;

    // `issues.opened` for the created issue (webhook + Actions trigger).
    let issue_payload = dispatch::issues_payload(
        "opened",
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
    dispatch::emit(
        &ctx.db,
        &accessible.row.id,
        "issues",
        "opened",
        issue_payload,
        &ctx.env_name,
    )
    .await;
    oxidean_api_core::issue::notify_actions_issue(ctx, &accessible, "opened", &issue, &user).await;

    // `project_card.converted` — `changes` carries the removed note
    // (GitHub's convert payload shape).
    let (repository, sender) = repository_and_sender(&accessible, &user);
    let changes = json!({
        "note": { "from": note },
    });
    let card_url = card_content_url(ctx, &accessible, &row).await;
    let payload = oxidean_api_core::actions::payload::project_card_payload(
        "converted",
        &row,
        card_url.as_deref(),
        Some(&changes),
        &repository,
        &sender,
    );
    emit_and_notify(
        ctx,
        &accessible,
        "project_card",
        "converted",
        payload,
        &user,
    )
    .await;

    let usernames = creator_usernames(ctx, std::iter::once(row.creator_id.clone())).await;
    Ok(card_to_public(ctx, &row, &accessible, &usernames).await)
}

/// `projects.cards.delete` — Write+; fires `deleted`.
pub async fn cards_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ProjectDeleteSimpleResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: ProjectCardRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid projects.cards.delete input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let project = load_project_in_repo(ctx, &accessible.row.id, req.project).await?;
    let row = load_card_in_project(ctx, &project.id, &req.card).await?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let card_url = card_content_url(ctx, &accessible, &row).await;
    let payload = oxidean_api_core::actions::payload::project_card_payload(
        "deleted",
        &row,
        card_url.as_deref(),
        None,
        &repository,
        &sender,
    );
    emit_and_notify(ctx, &accessible, "project_card", "deleted", payload, &user).await;

    ctx.db.delete_project_card(&row.id).await.map_err(db_err)?;
    Ok(ProjectDeleteSimpleResponse { ok: true })
}
