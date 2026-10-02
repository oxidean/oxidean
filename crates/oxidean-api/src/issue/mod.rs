//! Issue RPC handlers — lifecycle + comments + history (ISS-01..02 / D-ISS-01..04 / D-ISS-09 / D-ISS-12 / D-ISS-20).

pub(crate) mod acl;

use oxidean_core::{
    AddIssueLinkRequest, AppError, AssigneeCandidatesRequest, AssigneeCandidatesResponse,
    CommentHistoryResponse, CommentRevisionPublic, CreateIssueCommentRequest, CreateIssueRequest,
    DeleteIssueCommentResponse, DeleteIssueRequest, DeleteIssueResponse, IssueAssigneePublic,
    IssueCommentPublic, IssueCommentRefRequest, IssueCommentsListResponse, IssueHistoryResponse,
    IssueLinkKind, IssueLinkPublic, IssueLinksListResponse, IssueListRequest, IssueListResponse,
    IssuePublic, IssueRefRequest, IssueRevisionPublic, IssueState, ReactionGroupPublic,
    ReactionTarget, RemoveIssueLinkRequest, RemoveIssueLinkResponse, SetIssueAssigneesRequest,
    SetIssueLabelsRequest, ToggleReactionRequest, ToggleReactionResponse, UpdateIssueCommentRequest,
    UpdateIssueRequest,
};
use oxidean_db::{IssueCommentRow, IssueRow};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::notify;
use crate::webhook::dispatch;
use crate::repo::not_found;
use crate::rpc::RpcCtx;

/// Title soft cap (~1k chars).
const TITLE_MAX_CHARS: usize = 1_024;
/// Body soft cap (~64k chars).
const BODY_MAX_CHARS: usize = 65_536;

fn db_err(e: String) -> AppError {
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

fn issue_not_found() -> AppError {
    AppError::new("issue.not_found", "Issue not found")
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

fn viewer_id(ctx: &RpcCtx) -> Option<&str> {
    ctx.session.as_ref().map(|s| s.user_id.as_str())
}

fn reaction_groups_to_public(
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

struct IssueEnrichment {
    usernames: std::collections::HashMap<String, String>,
    labels: std::collections::HashMap<String, Vec<oxidean_db::LabelRow>>,
    assignees: std::collections::HashMap<String, Vec<oxidean_db::issue_labels::IssueAssigneeRow>>,
    reactions: std::collections::HashMap<String, Vec<oxidean_db::issues::ReactionGroupRow>>,
}

async fn load_enrichment(ctx: &RpcCtx, rows: &[IssueRow]) -> Result<IssueEnrichment, AppError> {
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
        });
    }
    let (users, label_pairs, assignee_pairs, reaction_pairs) = tokio::try_join!(
        ctx.db.find_users_by_ids(&author_ids),
        ctx.db.list_labels_for_issues(&issue_ids),
        ctx.db.list_assignees_for_issues(&issue_ids),
        ctx.db
            .list_issue_reaction_groups_for_issues(&issue_ids, viewer_id(ctx)),
    )
    .map_err(db_err)?;
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
        usernames: users.into_iter().map(|u| (u.id, u.username)).collect(),
        labels,
        assignees,
        reactions,
    })
}

fn issue_to_public(row: &IssueRow, enrichment: &IssueEnrichment) -> Result<IssuePublic, AppError> {
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
        comment_count: row.comment_count,
    })
}

async fn to_public(ctx: &RpcCtx, row: &IssueRow) -> Result<IssuePublic, AppError> {
    let rows = std::slice::from_ref(row);
    let enrichment = load_enrichment(ctx, rows).await?;
    issue_to_public(row, &enrichment)
}

async fn to_public_many(ctx: &RpcCtx, rows: &[IssueRow]) -> Result<Vec<IssuePublic>, AppError> {
    let enrichment = load_enrichment(ctx, rows).await?;
    rows.iter()
        .map(|row| issue_to_public(row, &enrichment))
        .collect()
}

async fn load_issue_in_repo(
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

/// `issue.create` — Write+; allocates per-repo `#N` (D-ISS-01 / D-ISS-20).
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<IssuePublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: CreateIssueRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.create input: {e}"),
        )
    })?;
    let title = validate_title(&req.title)?.to_string();
    let body = validate_body(req.body.as_deref())?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;

    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_issue(&id, &accessible.row.id, &user.id, &title, &body)
        .await
        .map_err(db_err)?;
    // Notify on create: issue_opened to repo owner (when not the author) + mentions (D-01 / D-03).
    let subject = notify::subject_for_issue(&row);
    let mut opened_recipients: Vec<String> = Vec::new();
    if accessible.row.owner_type == "user" && accessible.row.owner_id != user.id {
        opened_recipients.push(accessible.row.owner_id.clone());
    }
    notify::fanout_activity(ctx, &user.id, opened_recipients, "issue_opened", &subject).await;
    let mentions = notify::resolve_mention_user_ids(ctx, &body).await;
    notify::fanout_suppress_ignored(ctx, &user.id, mentions, "issue_mention", &subject).await;
    let payload = dispatch::issues_payload(
        "opened",
        row.number,
        &row.title,
        &row.body,
        &row.state,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        &user.username,
        &user.id,
    );
    dispatch::emit(&ctx.db, &accessible.row.id, "issues", "opened", payload, &ctx.env_name).await;
    to_public(ctx, &row).await
}

/// `issue.get` — Read+; soft `repo.not_found` for unauthorized private (D-ISS-20).
pub async fn get(ctx: &RpcCtx, input: serde_json::Value) -> Result<IssuePublic, AppError> {
    let req: IssueRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid issue.get input: {e}"))
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    to_public(ctx, &row).await
}

/// `issue.list` — Read+; default state filter `open` (D-ISS-16..18).
pub async fn list(ctx: &RpcCtx, input: serde_json::Value) -> Result<IssueListResponse, AppError> {
    let req: IssueListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid issue.list input: {e}"))
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let state = req.state.as_deref().unwrap_or("open");
    let offset = req.offset.unwrap_or(0);
    let limit = req.limit.unwrap_or(25);

    // Resolve author/assignee usernames → ids; unknown → empty page (not an error).
    let author_id = resolve_username_filter(ctx, req.author.as_deref()).await?;
    if req.author.as_deref().map(str::trim).is_some_and(|s| !s.is_empty()) && author_id.is_none() {
        return Ok(IssueListResponse {
            issues: vec![],
            total: 0,
        });
    }
    let assignee_id = resolve_username_filter(ctx, req.assignee.as_deref()).await?;
    if req
        .assignee
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty())
        && assignee_id.is_none()
    {
        return Ok(IssueListResponse {
            issues: vec![],
            total: 0,
        });
    }
    let label_id = req
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let q = req
        .q
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let filters = oxidean_db::IssueListFilters {
        state,
        author_id: author_id.as_deref(),
        label_id: label_id.as_deref(),
        assignee_id: assignee_id.as_deref(),
        q: q.as_deref(),
        offset,
        limit,
    };
    let (rows, total) = ctx
        .db
        .list_issues_for_repo(&accessible.row.id, filters)
        .await
        .map_err(db_err)?;
    let issues = to_public_many(ctx, &rows).await?;
    Ok(IssueListResponse { issues, total })
}

async fn resolve_username_filter(
    ctx: &RpcCtx,
    raw: Option<&str>,
) -> Result<Option<String>, AppError> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let username = raw.strip_prefix('@').unwrap_or(raw);
    let user = ctx
        .db
        .find_user_by_username(username)
        .await
        .map_err(db_err)?;
    Ok(user.map(|u| u.id))
}

/// `issue.update` — Author or Write+; appends full revision on title/body change (D-ISS-03 / D-ISS-04).
pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<IssuePublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: UpdateIssueRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.update input: {e}"),
        )
    })?;
    if req.title.is_none() && req.body.is_none() {
        return Err(AppError::new(
            "rpc.bad_input",
            "title or body is required",
        ));
    }
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    if !acl::can_edit_issue(&user.id, &row, accessible.capability) {
        return Err(not_found());
    }

    let new_title = match &req.title {
        Some(t) => validate_title(t)?.to_string(),
        None => row.title.clone(),
    };
    let new_body = match &req.body {
        Some(b) => validate_body(Some(b.as_str()))?,
        None => row.body.clone(),
    };

    if new_title == row.title && new_body == row.body {
        return to_public(ctx, &row).await;
    }

    let rev_id = Uuid::new_v4().to_string();
    ctx.db
        .insert_issue_revision(&rev_id, &row.id, &user.id, &row.title, &row.body)
        .await
        .map_err(db_err)?;
    let updated = ctx
        .db
        .update_issue_content(&row.id, &new_title, &new_body)
        .await
        .map_err(db_err)?;
    let payload = dispatch::issues_payload(
        "edited",
        updated.number,
        &updated.title,
        &updated.body,
        &updated.state,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        &user.username,
        &user.id,
    );
    dispatch::emit(&ctx.db, &accessible.row.id, "issues", "edited", payload, &ctx.env_name).await;
    to_public(ctx, &updated).await
}

/// `issue.close` — Write+; open → closed (D-ISS-02 / D-ISS-20).
pub async fn close(ctx: &RpcCtx, input: serde_json::Value) -> Result<IssuePublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: IssueRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid issue.close input: {e}"))
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let row = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    if row.state == IssueState::Closed.as_str() {
        return to_public(ctx, &row).await;
    }
    let updated = ctx
        .db
        .close_issue(&row.id, &user.id)
        .await
        .map_err(db_err)?;
    let subject = notify::subject_for_issue(&updated);
    let recipients = notify::issue_participant_ids(ctx, &updated.id, &updated.author_id).await;
    notify::fanout_activity(ctx, &user.id, recipients, "issue_closed", &subject).await;
    let payload = dispatch::issues_payload(
        "closed",
        updated.number,
        &updated.title,
        &updated.body,
        &updated.state,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        &user.username,
        &user.id,
    );
    dispatch::emit(&ctx.db, &accessible.row.id, "issues", "closed", payload, &ctx.env_name).await;
    to_public(ctx, &updated).await
}

/// `issue.reopen` — Write+; closed → open (D-ISS-02 / D-ISS-20).
pub async fn reopen(ctx: &RpcCtx, input: serde_json::Value) -> Result<IssuePublic, AppError> {
    let _user = require_verified(ctx).await?;
    let req: IssueRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.reopen input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let row = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    if row.state == IssueState::Open.as_str() {
        return to_public(ctx, &row).await;
    }
    let updated = ctx.db.reopen_issue(&row.id).await.map_err(db_err)?;
    let subject = notify::subject_for_issue(&updated);
    let recipients = notify::issue_participant_ids(ctx, &updated.id, &updated.author_id).await;
    notify::fanout_activity(ctx, &_user.id, recipients, "issue_reopened", &subject).await;
    let payload = dispatch::issues_payload(
        "reopened",
        updated.number,
        &updated.title,
        &updated.body,
        &updated.state,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        &_user.username,
        &_user.id,
    );
    dispatch::emit(&ctx.db, &accessible.row.id, "issues", "reopened", payload, &ctx.env_name).await;
    to_public(ctx, &updated).await
}

/// `issue.history` — Read+; prior title/body revisions oldest-first (D-ISS-04).
pub async fn history(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<IssueHistoryResponse, AppError> {
    let req: IssueRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.history input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    let revs = ctx
        .db
        .list_issue_revisions(&row.id)
        .await
        .map_err(db_err)?;
    let editor_ids: Vec<String> = {
        let mut v: Vec<String> = revs.iter().map(|r| r.editor_id.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    let editors: std::collections::HashMap<String, String> = ctx
        .db
        .find_users_by_ids(&editor_ids)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|u| (u.id, u.username))
        .collect();
    let mut revisions = Vec::with_capacity(revs.len());
    for rev in &revs {
        let editor_username = editors.get(&rev.editor_id).cloned().unwrap_or_default();
        revisions.push(IssueRevisionPublic {
            id: rev.id.clone(),
            issue_id: rev.issue_id.clone(),
            editor_id: rev.editor_id.clone(),
            editor_username,
            title: rev.title.clone(),
            body: rev.body.clone(),
            created_at: rev.created_at.clone(),
        });
    }
    Ok(IssueHistoryResponse { revisions })
}

/// `issue.delete` — Admin + confirmNumber; does not reclaim `#N` (D-ISS-02 / D-ISS-20).
pub async fn delete(ctx: &RpcCtx, input: serde_json::Value) -> Result<DeleteIssueResponse, AppError> {
    let _user = require_verified(ctx).await?;
    let req: DeleteIssueRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.delete input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_admin(ctx, &req.owner, &req.name).await?;
    let row = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    if req.confirm_number != row.number {
        return Err(AppError::new(
            "issue.confirm_mismatch",
            "Type the issue number exactly to confirm deletion.",
        ));
    }
    // Cascades comments/reactions/links/revisions via FK ON DELETE CASCADE.
    ctx.db.delete_issue(&row.id).await.map_err(db_err)?;
    Ok(DeleteIssueResponse {
        number: row.number,
    })
}

fn comment_not_found() -> AppError {
    AppError::new("issue.comment_not_found", "Comment not found")
}

struct CommentEnrichment {
    usernames: std::collections::HashMap<String, String>,
    reactions: std::collections::HashMap<String, Vec<oxidean_db::issues::ReactionGroupRow>>,
}

async fn load_comment_enrichment(
    ctx: &RpcCtx,
    rows: &[IssueCommentRow],
) -> Result<CommentEnrichment, AppError> {
    use std::collections::HashMap;
    let comment_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let author_ids: Vec<String> = {
        let mut v: Vec<String> = rows.iter().map(|r| r.author_id.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    if comment_ids.is_empty() {
        return Ok(CommentEnrichment {
            usernames: HashMap::new(),
            reactions: HashMap::new(),
        });
    }
    let (users, reaction_pairs) = tokio::try_join!(
        ctx.db.find_users_by_ids(&author_ids),
        ctx.db
            .list_comment_reaction_groups_for_comments(&comment_ids, viewer_id(ctx)),
    )
    .map_err(db_err)?;
    let mut reactions: HashMap<String, Vec<oxidean_db::issues::ReactionGroupRow>> = HashMap::new();
    for (comment_id, g) in reaction_pairs {
        reactions.entry(comment_id).or_default().push(g);
    }
    Ok(CommentEnrichment {
        usernames: users.into_iter().map(|u| (u.id, u.username)).collect(),
        reactions,
    })
}

fn comment_row_to_public(
    row: &IssueCommentRow,
    enrichment: &CommentEnrichment,
) -> IssueCommentPublic {
    let reactions = reaction_groups_to_public(
        enrichment
            .reactions
            .get(&row.id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .to_vec(),
    );
    IssueCommentPublic {
        id: row.id.clone(),
        issue_id: row.issue_id.clone(),
        author_id: row.author_id.clone(),
        author_username: enrichment
            .usernames
            .get(&row.author_id)
            .cloned()
            .unwrap_or_default(),
        body: row.body.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
        reactions,
    }
}

async fn comment_to_public(
    ctx: &RpcCtx,
    row: &IssueCommentRow,
) -> Result<IssueCommentPublic, AppError> {
    let rows = std::slice::from_ref(row);
    let enrichment = load_comment_enrichment(ctx, rows).await?;
    Ok(comment_row_to_public(row, &enrichment))
}

async fn load_comment_in_issue(
    ctx: &RpcCtx,
    issue_id: &str,
    comment_id: &str,
) -> Result<IssueCommentRow, AppError> {
    let row = ctx
        .db
        .find_issue_comment_by_id(comment_id)
        .await
        .map_err(db_err)?
        .ok_or_else(comment_not_found)?;
    if row.issue_id != issue_id {
        return Err(comment_not_found());
    }
    Ok(row)
}

/// `issue.comments.list` — Read+; oldest-first (ISS-02 / D-ISS-20).
pub async fn comments_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<IssueCommentsListResponse, AppError> {
    let req: IssueRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.comments.list input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let issue = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    let rows = ctx
        .db
        .list_issue_comments(&issue.id)
        .await
        .map_err(db_err)?;
    let enrichment = load_comment_enrichment(ctx, &rows).await?;
    let comments = rows
        .iter()
        .map(|row| comment_row_to_public(row, &enrichment))
        .collect();
    Ok(IssueCommentsListResponse { comments })
}

/// `issue.comments.create` — Write+ (ISS-02 / D-ISS-20).
pub async fn comments_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<IssueCommentPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: CreateIssueCommentRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.comments.create input: {e}"),
        )
    })?;
    let body = validate_body(Some(req.body.as_str()))?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let issue = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_issue_comment(&id, &issue.id, &user.id, &body)
        .await
        .map_err(db_err)?;
    let subject = notify::subject_for_issue(&issue);
    let participants = notify::issue_participant_ids(ctx, &issue.id, &issue.author_id).await;
    let mentions = notify::resolve_mention_user_ids(ctx, &body).await;
    notify::fanout_activity(ctx, &user.id, participants.clone(), "issue_comment", &subject).await;
    let participant_set: std::collections::HashSet<_> = participants.into_iter().collect();
    let mention_only: Vec<_> = mentions
        .into_iter()
        .filter(|m| !participant_set.contains(m))
        .collect();
    notify::fanout_suppress_ignored(ctx, &user.id, mention_only, "issue_mention", &subject).await;
    comment_to_public(ctx, &row).await
}

/// `issue.comments.update` — author only; appends full revision (D-ISS-09 / D-ISS-12).
pub async fn comments_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<IssueCommentPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: UpdateIssueCommentRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.comments.update input: {e}"),
        )
    })?;
    let new_body = validate_body(Some(req.body.as_str()))?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let issue = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    let row = load_comment_in_issue(ctx, &issue.id, &req.comment_id).await?;
    if !acl::can_edit_comment(&user.id, &row.author_id) {
        return Err(not_found());
    }
    if new_body == row.body {
        return comment_to_public(ctx, &row).await;
    }
    let rev_id = Uuid::new_v4().to_string();
    ctx.db
        .insert_comment_revision(&rev_id, &row.id, &user.id, &row.body)
        .await
        .map_err(db_err)?;
    let updated = ctx
        .db
        .update_issue_comment_body(&row.id, &new_body)
        .await
        .map_err(db_err)?;
    comment_to_public(ctx, &updated).await
}

/// `issue.comments.delete` — author or Write+ moderation (D-ISS-09 / T-11-10).
pub async fn comments_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DeleteIssueCommentResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: IssueCommentRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.comments.delete input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let issue = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    let row = load_comment_in_issue(ctx, &issue.id, &req.comment_id).await?;
    if !acl::can_delete_comment(&user.id, &row.author_id, accessible.capability) {
        return Err(not_found());
    }
    ctx.db
        .delete_issue_comment(&row.id)
        .await
        .map_err(db_err)?;
    Ok(DeleteIssueCommentResponse { ok: true })
}

/// `issue.comments.history` — Read+; prior body revisions oldest-first (D-ISS-12).
pub async fn comments_history(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CommentHistoryResponse, AppError> {
    let req: IssueCommentRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.comments.history input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let issue = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    let row = load_comment_in_issue(ctx, &issue.id, &req.comment_id).await?;
    let revs = ctx
        .db
        .list_comment_revisions(&row.id)
        .await
        .map_err(db_err)?;
    let mut revisions = Vec::with_capacity(revs.len());
    for rev in &revs {
        let editor_username = match ctx.db.find_user_by_id(&rev.editor_id).await {
            Ok(Some(u)) => u.username,
            Ok(None) => String::new(),
            Err(e) => return Err(db_err(e)),
        };
        revisions.push(CommentRevisionPublic {
            id: rev.id.clone(),
            comment_id: rev.comment_id.clone(),
            editor_id: rev.editor_id.clone(),
            editor_username,
            body: rev.body.clone(),
            created_at: rev.created_at.clone(),
        });
    }
    Ok(CommentHistoryResponse { revisions })
}

/// `issue.labels.set` — Write+; replace label ids from effective repo set (D-ISS-07 / D-ISS-20).
pub async fn labels_set(ctx: &RpcCtx, input: serde_json::Value) -> Result<IssuePublic, AppError> {
    let _user = require_verified(ctx).await?;
    let req: SetIssueLabelsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.labels.set input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let issue = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;

    let allowed = crate::label::effective_label_id_set(
        ctx,
        &accessible.row.id,
        &accessible.row.owner_type,
        &accessible.row.owner_id,
    )
    .await?;

    let mut seen = std::collections::HashSet::new();
    let mut unique_ids = Vec::new();
    for id in &req.label_ids {
        let id = id.trim();
        if id.is_empty() {
            return Err(AppError::new("rpc.bad_input", "label id must not be empty"));
        }
        if !allowed.contains(id) {
            return Err(AppError::new(
                "rpc.bad_input",
                "label is not in the effective set for this repository",
            ));
        }
        if seen.insert(id.to_string()) {
            unique_ids.push(id.to_string());
        }
    }

    ctx.db
        .set_issue_labels(&issue.id, &unique_ids)
        .await
        .map_err(db_err)?;

    let refreshed = ctx
        .db
        .find_issue_by_id(&issue.id)
        .await
        .map_err(db_err)?
        .ok_or_else(issue_not_found)?;
    to_public(ctx, &refreshed).await
}

/// `issue.assignees.set` — Write+; each user_id must have Read+ (D-ISS-06 / D-ISS-08 / T-11-12).
pub async fn assignees_set(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<IssuePublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: SetIssueAssigneesRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.assignees.set input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let issue = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;

    let owner_ref = crate::repo::owner_ref_for_repo(&ctx.db, &accessible.row)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "owner_ref_for_repo failed");
            AppError::new("issue.internal", "issue operation failed")
        })?
        .ok_or_else(not_found)?;

    let before: std::collections::HashSet<String> = ctx
        .db
        .list_issue_assignees(&issue.id)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|a| a.user_id)
        .collect();

    let mut seen = std::collections::HashSet::new();
    let mut unique_ids = Vec::new();
    for id in &req.user_ids {
        let id = id.trim();
        if id.is_empty() {
            return Err(AppError::new("rpc.bad_input", "user id must not be empty"));
        }
        if !seen.insert(id.to_string()) {
            continue;
        }
        let Some(_u) = ctx.db.find_user_by_id(id).await.map_err(db_err)? else {
            return Err(AppError::new(
                "rpc.bad_input",
                "assignee user was not found",
            ));
        };
        let cap = crate::repo::effective_capability(
            &ctx.db,
            Some(id),
            &accessible.row,
            &owner_ref,
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "effective_capability for assignee failed");
            AppError::new("issue.internal", "issue operation failed")
        })?;
        if !crate::repo::meets(cap, crate::repo::Capability::Read) {
            return Err(AppError::new(
                "rpc.bad_input",
                "assignee must have Read access on this repository",
            ));
        }
        unique_ids.push(id.to_string());
    }

    ctx.db
        .set_issue_assignees(&issue.id, &unique_ids)
        .await
        .map_err(db_err)?;

    let after: std::collections::HashSet<String> = unique_ids.iter().cloned().collect();
    let newly_assigned: Vec<_> = after.difference(&before).cloned().collect();
    let newly_unassigned: Vec<_> = before.difference(&after).cloned().collect();
    let subject = notify::subject_for_issue(&issue);
    notify::fanout_suppress_ignored(ctx, &user.id, newly_assigned, "issue_assigned", &subject).await;
    notify::fanout_suppress_ignored(ctx, &user.id, newly_unassigned, "issue_unassigned", &subject).await;

    let refreshed = ctx
        .db
        .find_issue_by_id(&issue.id)
        .await
        .map_err(db_err)?
        .ok_or_else(issue_not_found)?;
    to_public(ctx, &refreshed).await
}

/// `issue.assigneeCandidates` — Write+; profiles with effective Read+ (D-ISS-08).
pub async fn assignee_candidates(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<AssigneeCandidatesResponse, AppError> {
    let _user = require_verified(ctx).await?;
    let req: AssigneeCandidatesRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.assigneeCandidates input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let owner_ref = crate::repo::owner_ref_for_repo(&ctx.db, &accessible.row)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "owner_ref_for_repo failed");
            AppError::new("issue.internal", "issue operation failed")
        })?
        .ok_or_else(not_found)?;

    let mut candidate_ids = std::collections::HashSet::new();
    match &owner_ref {
        crate::repo::OwnerRef::User { id, .. } => {
            candidate_ids.insert(id.clone());
        }
        crate::repo::OwnerRef::Org { id, .. } => {
            let members = ctx.db.list_org_members(id).await.map_err(db_err)?;
            for m in members {
                candidate_ids.insert(m.user_id);
            }
        }
    }
    let collabs = ctx
        .db
        .list_repo_collaborators(&accessible.row.id)
        .await
        .map_err(db_err)?;
    for c in collabs {
        candidate_ids.insert(c.user_id);
    }

    let prefix = req
        .prefix
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| p.to_ascii_lowercase());

    if let Some(ref p) = prefix {
        if p.chars().count() >= 2 && !p.contains('@') {
            let hits = ctx
                .db
                .list_users_by_username_prefix(p, 20)
                .await
                .map_err(db_err)?;
            for hit in hits {
                if let Some(u) = ctx
                    .db
                    .find_user_by_username(&hit.username)
                    .await
                    .map_err(db_err)?
                {
                    candidate_ids.insert(u.id);
                }
            }
        }
    }

    let mut users = Vec::new();
    for user_id in candidate_ids {
        let cap = crate::repo::effective_capability(
            &ctx.db,
            Some(&user_id),
            &accessible.row,
            &owner_ref,
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "effective_capability for candidate failed");
            AppError::new("issue.internal", "issue operation failed")
        })?;
        if !crate::repo::meets(cap, crate::repo::Capability::Read) {
            continue;
        }
        let Some(u) = ctx.db.find_user_by_id(&user_id).await.map_err(db_err)? else {
            continue;
        };
        if let Some(ref p) = prefix {
            if !u.username.to_ascii_lowercase().starts_with(p.as_str()) {
                continue;
            }
        }
        users.push(IssueAssigneePublic {
            user_id: u.id,
            username: u.username,
            display_name: u.display_name,
        });
    }
    users.sort_by(|a, b| a.username.to_ascii_lowercase().cmp(&b.username.to_ascii_lowercase()));
    Ok(AssigneeCandidatesResponse { users })
}

/// `issue.reactions.toggle` — Write+; the eight contents on issue|comment (D-ISS-11 / D-ISS-20).
pub async fn reactions_toggle(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ToggleReactionResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: ToggleReactionRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.reactions.toggle input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let issue = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    let content = req.content.as_str();

    let reacted = match req.target {
        ReactionTarget::Issue => ctx
            .db
            .toggle_issue_reaction(&issue.id, &user.id, content)
            .await
            .map_err(db_err)?,
        ReactionTarget::Comment => {
            let comment_id = req
                .comment_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    AppError::new(
                        "rpc.bad_input",
                        "commentId is required when target is comment",
                    )
                })?;
            let comment = load_comment_in_issue(ctx, &issue.id, comment_id).await?;
            ctx.db
                .toggle_comment_reaction(&comment.id, &user.id, content)
                .await
                .map_err(db_err)?
        }
    };

    let reaction_rows = match req.target {
        ReactionTarget::Issue => ctx
            .db
            .list_issue_reaction_groups(&issue.id, Some(&user.id))
            .await
            .map_err(db_err)?,
        ReactionTarget::Comment => {
            let comment_id = req.comment_id.as_deref().unwrap_or("").trim();
            ctx.db
                .list_comment_reaction_groups(comment_id, Some(&user.id))
                .await
                .map_err(db_err)?
        }
    };

    Ok(ToggleReactionResponse {
        reactions: reaction_groups_to_public(reaction_rows),
        reacted,
    })
}

fn link_row_to_public(row: oxidean_db::IssueLinkRow) -> Result<IssueLinkPublic, AppError> {
    let kind = IssueLinkKind::parse(&row.kind).map_err(|e| {
        tracing::error!(error = %e, "invalid issue link kind in db");
        AppError::new("issue.internal", "issue operation failed")
    })?;
    Ok(IssueLinkPublic {
        id: row.id,
        kind,
        target_repo_id: row.target_repo_id,
        target_number: row.target_number,
        target_opaque_id: row.target_opaque_id,
        title: row.title,
        created_at: row.created_at,
    })
}

/// `issue.links.list` — Read+ on accessible issues (D-ISS-13 / D-ISS-20).
pub async fn links_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<IssueLinksListResponse, AppError> {
    let _user = require_verified(ctx).await?;
    let req: IssueRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.links.list input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let issue = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    let rows = ctx
        .db
        .list_issue_links(&issue.id)
        .await
        .map_err(db_err)?;
    let mut links = Vec::with_capacity(rows.len());
    for row in rows {
        links.push(link_row_to_public(row)?);
    }
    Ok(IssueLinksListResponse { links })
}

/// `issue.links.add` — Write+; stub-capable rows (D-ISS-13 / D-ISS-14 / D-ISS-20).
pub async fn links_add(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<IssueLinkPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: AddIssueLinkRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.links.add input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let issue = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;

    let target_number = req.target_number.ok_or_else(|| {
        AppError::new("rpc.bad_input", "targetNumber is required")
    })?;
    if target_number < 1 {
        return Err(AppError::new(
            "rpc.bad_input",
            "targetNumber must be a positive integer",
        ));
    }

    let title = match req.title.as_deref() {
        None => None,
        Some(t) => {
            let trimmed = t.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed.chars().count() > TITLE_MAX_CHARS {
                return Err(AppError::new(
                    "rpc.bad_input",
                    format!("title exceeds {TITLE_MAX_CHARS} characters"),
                ));
            } else {
                Some(trimmed.to_string())
            }
        }
    };

    let target_repo_id = match req.kind {
        IssueLinkKind::Issue => Some(
            req.target_repo_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(accessible.row.id.as_str())
                .to_string(),
        ),
        IssueLinkKind::PrStub | IssueLinkKind::Pr => req
            .target_repo_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
    };

    // Optional resolve for issue links in the same repo — soft fill opaque id when found.
    let target_opaque_id = if req.kind == IssueLinkKind::Issue {
        if let Some(ref repo_id) = target_repo_id {
            if repo_id == &accessible.row.id {
                match ctx
                    .db
                    .find_issue_by_repo_number(repo_id, target_number)
                    .await
                    .map_err(db_err)?
                {
                    Some(target) => Some(target.id),
                    None => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_issue_link(
            &id,
            &issue.id,
            req.kind.as_str(),
            target_repo_id.as_deref(),
            Some(target_number),
            target_opaque_id.as_deref(),
            title.as_deref(),
            &user.id,
        )
        .await
        .map_err(db_err)?;
    link_row_to_public(row)
}

/// `issue.links.remove` — Write+; delete stub by opaque id (D-ISS-14 / D-ISS-20).
pub async fn links_remove(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RemoveIssueLinkResponse, AppError> {
    let _user = require_verified(ctx).await?;
    let req: RemoveIssueLinkRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid issue.links.remove input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let issue = load_issue_in_repo(ctx, &accessible.row.id, req.number).await?;
    let link_id = req.link_id.trim();
    if link_id.is_empty() {
        return Err(AppError::new("rpc.bad_input", "linkId is required"));
    }
    let deleted = ctx
        .db
        .delete_issue_link(&issue.id, link_id)
        .await
        .map_err(db_err)?;
    if !deleted {
        return Err(AppError::new("issue.link_not_found", "Link not found"));
    }
    Ok(RemoveIssueLinkResponse { ok: true })
}
