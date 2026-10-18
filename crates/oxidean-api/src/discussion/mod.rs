//! Discussion forum RPC handlers — repo-scoped categories, threads, and
//! threaded comments (GitHub Discussions shape). `discussions.*` mutations
//! fire `discussion` / `discussion_comment` lifecycle events (GitHub
//! activity types) and matching webhook emits.
//!
//! Scope rules: discussions resolve by the shared `#N` issue sequence
//! inside their repo; categories/comments resolve only inside their parent
//! (comment → discussion → repo). Cross-repo ids return
//! `discussion.*.not_found` rather than touching another repo's rows.

use std::collections::HashMap;

use oxidean_core::{
    AppError, DiscussionAnswerSetRequest, DiscussionCategoriesListRequest,
    DiscussionCategoriesListResponse, DiscussionCategoryCreateRequest, DiscussionCategoryPublic,
    DiscussionCategoryRefRequest, DiscussionCategoryUpdateRequest, DiscussionCommentCreateRequest,
    DiscussionCommentPublic, DiscussionCommentRefRequest, DiscussionCommentUpdateRequest,
    DiscussionCommentsListRequest, DiscussionCommentsListResponse, DiscussionCreateRequest,
    DiscussionDeleteResponse, DiscussionLabelsSetRequest, DiscussionLockRequest,
    DiscussionPinRequest, DiscussionPublic, DiscussionRefRequest, DiscussionTransferRequest,
    DiscussionUpdateRequest, DiscussionsListRequest, DiscussionsListResponse,
};
use oxidean_db::{DiscussionCategoryRow, DiscussionCommentRow, DiscussionRow};
use serde_json::json;
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::repo::{
    ensure_not_archived, meets, not_found, require_unit_enabled, resolve_repo_for_read,
    AccessibleRepo, Capability, RepoUnit,
};
use crate::rpc::RpcCtx;
use crate::webhook::dispatch;

/// Title soft cap (matches the issue title cap).
const TITLE_MAX_CHARS: usize = 1_024;
/// Body / comment soft cap (matches the issue body cap).
const BODY_MAX_CHARS: usize = 65_536;
/// Category name / emoji caps (GitHub category name is short).
const CATEGORY_NAME_MAX_CHARS: usize = 128;
const EMOJI_MAX_CHARS: usize = 64;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("discussion db error: {e}");
        AppError::new("discussion.internal", "discussion operation failed")
    }
}

fn discussion_not_found() -> AppError {
    AppError::new("discussion.not_found", "Discussion not found")
}

fn category_not_found() -> AppError {
    AppError::new(
        "discussion.category_not_found",
        "Discussion category not found",
    )
}

fn comment_not_found() -> AppError {
    AppError::new(
        "discussion.comment_not_found",
        "Discussion comment not found",
    )
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

/// GitHub's category slug: lowercase, alphanumeric runs joined by `-`
/// ("Q&A" → `q-a`, "Show and tell" → `show-and-tell`). Empty runs fall back
/// to `general` — a name must produce at least one slug character.
fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true; // suppress leading dash
    for ch in name.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "general".to_string()
    } else {
        out
    }
}

/// `discussions.*` read gate — repo Read+ plus the Discussions unit flag.
async fn resolve_for_read(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    require_unit_enabled(ctx, RepoUnit::Discussions, &accessible.row.id).await?;
    Ok(accessible)
}

/// `discussions.*` write gate — repo Write+, unit flag, archived read-only.
async fn resolve_for_write(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    require_unit_enabled(ctx, RepoUnit::Discussions, &accessible.row.id).await?;
    ensure_not_archived(&accessible)?;
    Ok(accessible)
}

async fn load_discussion_in_repo(
    ctx: &RpcCtx,
    repo_id: &str,
    number: i64,
) -> Result<DiscussionRow, AppError> {
    if number < 1 {
        return Err(discussion_not_found());
    }
    ctx.db
        .find_discussion_by_number(repo_id, number)
        .await
        .map_err(db_err)?
        .ok_or_else(discussion_not_found)
}

async fn load_category_in_repo(
    ctx: &RpcCtx,
    repo_id: &str,
    category_id: &str,
) -> Result<DiscussionCategoryRow, AppError> {
    let Some(cat) = ctx
        .db
        .find_discussion_category_by_id(category_id)
        .await
        .map_err(db_err)?
    else {
        return Err(category_not_found());
    };
    if cat.repo_id != repo_id {
        return Err(category_not_found());
    }
    Ok(cat)
}

/// Comment scoped through its discussion → repo — cross-thread ids don't
/// resolve.
async fn load_comment_in_discussion(
    ctx: &RpcCtx,
    discussion_id: &str,
    comment_id: &str,
) -> Result<DiscussionCommentRow, AppError> {
    let Some(comment) = ctx
        .db
        .find_discussion_comment_by_id(comment_id)
        .await
        .map_err(db_err)?
    else {
        return Err(comment_not_found());
    };
    if comment.discussion_id != discussion_id {
        return Err(comment_not_found());
    }
    Ok(comment)
}

fn discussion_html_url(accessible: &AccessibleRepo, number: i64) -> String {
    format!(
        "/{}/{}/discussions/{number}",
        accessible.owner_username, accessible.row.name
    )
}

fn comment_html_url(accessible: &AccessibleRepo, number: i64, comment_id: &str) -> String {
    format!(
        "{}#comment-{comment_id}",
        discussion_html_url(accessible, number)
    )
}

fn category_to_public(row: &DiscussionCategoryRow) -> DiscussionCategoryPublic {
    DiscussionCategoryPublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        name: row.name.clone(),
        slug: row.slug.clone(),
        emoji: row.emoji.clone(),
        description: row.description.clone(),
        is_answerable: row.is_answerable,
        position: row.position,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

/// Batch-resolve `author_id`/`answered_by` → username across any row set.
async fn usernames(
    ctx: &RpcCtx,
    ids: impl Iterator<Item = Option<String>>,
) -> HashMap<String, String> {
    let mut ids: Vec<String> = ids.flatten().collect();
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

async fn discussion_to_public(
    ctx: &RpcCtx,
    row: &DiscussionRow,
    accessible: &AccessibleRepo,
) -> Result<DiscussionPublic, AppError> {
    let category = match &row.category_id {
        Some(cid) => ctx
            .db
            .find_discussion_category_by_id(cid)
            .await
            .map_err(db_err)?
            .map(|c| category_to_public(&c)),
        None => None,
    };
    let label_ids = ctx
        .db
        .list_discussion_label_ids(&row.id)
        .await
        .map_err(db_err)?;
    let names = usernames(
        ctx,
        [row.author_id.clone(), row.answered_by.clone()].into_iter(),
    )
    .await;
    Ok(DiscussionPublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        number: row.number,
        title: row.title.clone(),
        body: row.body.clone(),
        category,
        author_id: row.author_id.clone(),
        author_username: row.author_id.as_ref().and_then(|id| names.get(id).cloned()),
        answer_comment_id: row.answer_comment_id.clone(),
        answered_at: row.answered_at.clone(),
        answered_by: row.answered_by.clone(),
        answered_by_username: row
            .answered_by
            .as_ref()
            .and_then(|id| names.get(id).cloned()),
        locked: row.locked,
        pinned: row.pinned,
        label_ids,
        html_url: discussion_html_url(accessible, row.number),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    })
}

async fn comment_to_public(
    ctx: &RpcCtx,
    row: &DiscussionCommentRow,
    discussion_number: i64,
    accessible: &AccessibleRepo,
) -> DiscussionCommentPublic {
    let names = usernames(ctx, std::iter::once(row.author_id.clone())).await;
    DiscussionCommentPublic {
        id: row.id.clone(),
        discussion_id: row.discussion_id.clone(),
        parent_id: row.parent_id.clone(),
        author_id: row.author_id.clone(),
        author_username: row.author_id.as_ref().and_then(|id| names.get(id).cloned()),
        body: row.body.clone(),
        html_url: comment_html_url(accessible, discussion_number, &row.id),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

/// The `discussion` object embedded in webhook/lifecycle payloads —
/// category, labels, comment count, author + answer urls resolved.
async fn embed_for(
    ctx: &RpcCtx,
    row: &DiscussionRow,
    accessible: &AccessibleRepo,
) -> serde_json::Value {
    let category = match &row.category_id {
        Some(cid) => ctx
            .db
            .find_discussion_category_by_id(cid)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let label_ids = ctx
        .db
        .list_discussion_label_ids(&row.id)
        .await
        .unwrap_or_default();
    let mut label_names = Vec::with_capacity(label_ids.len());
    for id in &label_ids {
        if let Ok(Some(label)) = ctx.db.find_label_by_id(id).await {
            label_names.push(label.name);
        }
    }
    let comments = ctx
        .db
        .list_discussion_comments(&row.id)
        .await
        .unwrap_or_default();
    let author_login = match &row.author_id {
        Some(id) => ctx
            .db
            .find_user_by_id(id)
            .await
            .ok()
            .flatten()
            .map(|u| u.username),
        None => None,
    };
    let answer_url = row
        .answer_comment_id
        .as_ref()
        .map(|cid| comment_html_url(accessible, row.number, cid));
    crate::actions::payload::discussion_embed(
        row,
        category.as_ref(),
        author_login.as_deref(),
        &label_names,
        comments.len() as i64,
        &discussion_html_url(accessible, row.number),
        answer_url.as_deref(),
    )
}

/// Shared lifecycle fan-out — the webhook emit plus the `on: discussion` /
/// `on: discussion_comment` Actions trigger (job-token suppressed, soft-fail).
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
    let Ok(bare) = crate::git::bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    ) else {
        return;
    };
    crate::actions::events::notify_lifecycle_actions(
        &ctx.db,
        ctx.git.as_ref(),
        &crate::actions::events::LifecycleEvent {
            bare_repo: bare,
            repository_id: accessible.row.id.clone(),
            default_branch: accessible.row.default_branch.clone(),
            event_name,
            action: action.to_string(),
            payload,
            actor_id: Some(user.id.clone()),
            job_token_actor: crate::actions::events::rpc_caller_is_job_token(ctx),
            actions_enabled_instance: crate::actions::env_actions_enabled(),
        },
    )
    .await;
}

fn repository_and_sender(
    accessible: &AccessibleRepo,
    user: &oxidean_db::UserRow,
) -> (serde_json::Value, serde_json::Value) {
    (
        crate::actions::payload::repository_embed(
            &accessible.owner_username,
            &accessible.row.name,
            &accessible.row.default_branch,
        ),
        crate::actions::payload::user_embed(&user.username),
    )
}

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

/// `discussions.categories.list` — Read+.
pub async fn categories_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionCategoriesListResponse, AppError> {
    let req: DiscussionCategoriesListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.categories.list input: {e}"),
        )
    })?;
    let accessible = resolve_for_read(ctx, &req.owner, &req.name).await?;
    let rows = ctx
        .db
        .list_discussion_categories(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(DiscussionCategoriesListResponse {
        categories: rows.iter().map(category_to_public).collect(),
    })
}

/// `discussions.categories.create` — Write+; slug derived from the display
/// name, deduped with a numeric suffix.
pub async fn categories_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionCategoryPublic, AppError> {
    let _user = require_verified(ctx).await?;
    let req: DiscussionCategoryCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.categories.create input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let name = req.category_name.trim();
    if name.is_empty() {
        return Err(AppError::new("rpc.bad_input", "category name is required"));
    }
    if name.chars().count() > CATEGORY_NAME_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("category name exceeds {CATEGORY_NAME_MAX_CHARS} characters"),
        ));
    }
    let emoji = req.emoji.unwrap_or_default();
    if emoji.chars().count() > EMOJI_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("emoji exceeds {EMOJI_MAX_CHARS} characters"),
        ));
    }
    let description = req.description.unwrap_or_default();
    let is_answerable = req.is_answerable.unwrap_or(false);

    let base = slugify(name);
    let mut slug = base.clone();
    for suffix in 2.. {
        match ctx
            .db
            .find_discussion_category_by_slug(&accessible.row.id, &slug)
            .await
            .map_err(db_err)?
        {
            None => break,
            Some(existing) if existing.name == name => {
                return Err(AppError::new(
                    "rpc.conflict",
                    "a category with this name already exists",
                ));
            }
            Some(_) => slug = format!("{base}-{suffix}"),
        }
    }

    let row = ctx
        .db
        .insert_discussion_category(
            &Uuid::new_v4().to_string(),
            &accessible.row.id,
            name,
            &slug,
            &emoji,
            &description,
            is_answerable,
        )
        .await
        .map_err(db_err)?;
    Ok(category_to_public(&row))
}

/// `discussions.categories.update` — Write+. GitHub keeps the slug stable
/// on rename; we match that.
pub async fn categories_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionCategoryPublic, AppError> {
    let _user = require_verified(ctx).await?;
    let req: DiscussionCategoryUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.categories.update input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let existing = load_category_in_repo(ctx, &accessible.row.id, &req.category).await?;
    let name = req
        .category_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty());
    if let Some(n) = name {
        if n.chars().count() > CATEGORY_NAME_MAX_CHARS {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("category name exceeds {CATEGORY_NAME_MAX_CHARS} characters"),
            ));
        }
        if let Some(other) = ctx
            .db
            .find_discussion_category_by_slug(&accessible.row.id, &slugify(n))
            .await
            .map_err(db_err)?
        {
            if other.id != existing.id && other.name == n {
                return Err(AppError::new(
                    "rpc.conflict",
                    "a category with this name already exists",
                ));
            }
        }
    }
    let emoji = req.emoji.as_deref();
    if let Some(e) = emoji {
        if e.chars().count() > EMOJI_MAX_CHARS {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("emoji exceeds {EMOJI_MAX_CHARS} characters"),
            ));
        }
    }
    let row = ctx
        .db
        .update_discussion_category(
            &existing.id,
            name,
            emoji,
            req.description.as_deref(),
            req.is_answerable,
        )
        .await
        .map_err(db_err)?
        .ok_or_else(category_not_found)?;
    Ok(category_to_public(&row))
}

/// `discussions.categories.delete` — Write+; discussions keep their rows
/// (`category_id` SET NULL — GitHub leaves them uncategorized).
pub async fn categories_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionDeleteResponse, AppError> {
    let _user = require_verified(ctx).await?;
    let req: DiscussionCategoryRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.categories.delete input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let existing = load_category_in_repo(ctx, &accessible.row.id, &req.category).await?;
    ctx.db
        .delete_discussion_category(&existing.id)
        .await
        .map_err(db_err)?;
    Ok(DiscussionDeleteResponse { ok: true })
}

// ---------------------------------------------------------------------------
// Discussions
// ---------------------------------------------------------------------------

/// `discussions.list` — Read+; optional `category` (id or slug) and
/// `answered` filters.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionsListResponse, AppError> {
    let req: DiscussionsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.list input: {e}"),
        )
    })?;
    let accessible = resolve_for_read(ctx, &req.owner, &req.name).await?;
    // `category` accepts an id or a slug — the public surface exposes both.
    let category_id = match req.category.as_deref().map(str::trim) {
        Some("") | None => None,
        Some(c) => {
            let by_id = ctx
                .db
                .find_discussion_category_by_id(c)
                .await
                .map_err(db_err)?
                .filter(|cat| cat.repo_id == accessible.row.id);
            let by_slug = match by_id {
                Some(_) => None,
                None => ctx
                    .db
                    .find_discussion_category_by_slug(&accessible.row.id, c)
                    .await
                    .map_err(db_err)?,
            };
            Some(by_id.or(by_slug).ok_or_else(category_not_found)?.id)
        }
    };
    let rows = ctx
        .db
        .list_discussions(&accessible.row.id, category_id.as_deref(), req.answered)
        .await
        .map_err(db_err)?;
    let mut discussions = Vec::with_capacity(rows.len());
    for row in &rows {
        discussions.push(discussion_to_public(ctx, row, &accessible).await?);
    }
    Ok(DiscussionsListResponse { discussions })
}

/// `discussions.get` — Read+; resolves the shared `#N`.
pub async fn get(ctx: &RpcCtx, input: serde_json::Value) -> Result<DiscussionPublic, AppError> {
    let req: DiscussionRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.get input: {e}"),
        )
    })?;
    let accessible = resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    discussion_to_public(ctx, &row, &accessible).await
}

/// `discussions.create` — Write+; allocates the shared issue-sequence `#N`;
/// fires `created`.
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<DiscussionPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.create input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let title = validate_title(&req.title)?;
    let body = validate_body(req.body.as_deref())?;
    let category_id = match req.category.as_deref() {
        Some(cid) => Some(
            load_category_in_repo(ctx, &accessible.row.id, cid)
                .await?
                .id,
        ),
        None => None,
    };
    let label_ids = validate_label_ids(
        ctx,
        &accessible,
        req.label_ids.as_deref().unwrap_or_default(),
    )
    .await?;

    let number = ctx
        .db
        .allocate_next_issue_number(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let row = ctx
        .db
        .insert_discussion(
            &Uuid::new_v4().to_string(),
            &accessible.row.id,
            number,
            title,
            &body,
            category_id.as_deref(),
            Some(&user.id),
        )
        .await
        .map_err(db_err)?;
    if !label_ids.is_empty() {
        ctx.db
            .set_discussion_labels(&row.id, &label_ids)
            .await
            .map_err(db_err)?;
    }

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let embed = embed_for(ctx, &row, &accessible).await;
    emit_and_notify(
        ctx,
        &accessible,
        "discussion",
        "created",
        crate::actions::payload::discussion_payload(
            "created",
            &embed,
            None,
            None,
            None,
            &repository,
            &sender,
        ),
        &user,
    )
    .await;

    discussion_to_public(ctx, &row, &accessible).await
}

/// `discussions.update` — Write+ (or the author for title/body). Title/body
/// edits fire `edited` with `changes.title.from`/`body.from`; a category
/// switch fires `category_changed` with `changes.category.from` (GitHub
/// keeps both events distinct — an update that touches both emits both).
pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<DiscussionPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.update input: {e}"),
        )
    })?;
    let accessible = resolve_for_read(ctx, &req.owner, &req.name).await?;
    require_unit_enabled(ctx, RepoUnit::Discussions, &accessible.row.id).await?;
    ensure_not_archived(&accessible)?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    // GitHub lets the author edit title/body; category changes need write.
    let is_author = row.author_id.as_deref() == Some(user.id.as_str());
    let content_edit = req.title.is_some() || req.body.is_some();
    if !meets(accessible.capability, Capability::Write) && !(is_author && !row.locked) {
        return Err(not_found());
    }
    if content_edit && row.locked && !meets(accessible.capability, Capability::Write) {
        return Err(AppError::new("discussion.locked", "Discussion is locked"));
    }
    if req.category.is_some() && !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }

    let title = req.title.as_deref().map(validate_title).transpose()?;
    let body = req
        .body
        .as_deref()
        .map(|b| validate_body(Some(b)))
        .transpose()?;
    let new_category = match &req.category {
        Some(Some(cid)) => Some(Some(
            load_category_in_repo(ctx, &accessible.row.id, cid).await?,
        )),
        Some(None) => Some(None),
        None => None,
    };

    let mut changes = json!({});
    let mut edited = false;
    if let Some(t) = title {
        if t != row.title {
            changes["title"] = json!({ "from": row.title });
            edited = true;
        }
    }
    if let Some(b) = &body {
        if *b != row.body {
            changes["body"] = json!({ "from": row.body });
            edited = true;
        }
    }
    let category_changed = match &new_category {
        Some(cat) => cat.as_ref().map(|c| c.id.as_str()) != row.category_id.as_deref(),
        None => false,
    };

    let updated = ctx
        .db
        .update_discussion(
            &row.id,
            title,
            body.as_deref(),
            new_category
                .as_ref()
                .map(|c| c.as_ref().map(|cat| cat.id.as_str())),
        )
        .await
        .map_err(db_err)?
        .ok_or_else(discussion_not_found)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let embed = embed_for(ctx, &updated, &accessible).await;
    if edited {
        emit_and_notify(
            ctx,
            &accessible,
            "discussion",
            "edited",
            crate::actions::payload::discussion_payload(
                "edited",
                &embed,
                Some(&changes),
                None,
                None,
                &repository,
                &sender,
            ),
            &user,
        )
        .await;
    }
    if category_changed {
        let from = match &row.category_id {
            Some(cid) => ctx
                .db
                .find_discussion_category_by_id(cid)
                .await
                .ok()
                .flatten()
                .map(|c| json!({ "id": c.id, "name": c.name, "slug": c.slug })),
            None => None,
        };
        emit_and_notify(
            ctx,
            &accessible,
            "discussion",
            "category_changed",
            crate::actions::payload::discussion_payload(
                "category_changed",
                &embed,
                Some(&json!({ "category": { "from": from } })),
                None,
                None,
                &repository,
                &sender,
            ),
            &user,
        )
        .await;
    }

    discussion_to_public(ctx, &updated, &accessible).await
}

/// `discussions.delete` — Write+ or author; fires `deleted`.
pub async fn delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionDeleteResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.delete input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    let embed = embed_for(ctx, &row, &accessible).await;
    ctx.db.delete_discussion(&row.id).await.map_err(db_err)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    emit_and_notify(
        ctx,
        &accessible,
        "discussion",
        "deleted",
        crate::actions::payload::discussion_payload(
            "deleted",
            &embed,
            None,
            None,
            None,
            &repository,
            &sender,
        ),
        &user,
    )
    .await;
    Ok(DiscussionDeleteResponse { ok: true })
}

/// `discussions.pin` — Write+; fires `pinned`/`unpinned`.
pub async fn pin(ctx: &RpcCtx, input: serde_json::Value) -> Result<DiscussionPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionPinRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.pin input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    if row.pinned == req.pinned {
        return discussion_to_public(ctx, &row, &accessible).await;
    }
    let updated = ctx
        .db
        .set_discussion_flags(&row.id, Some(req.pinned), None)
        .await
        .map_err(db_err)?
        .ok_or_else(discussion_not_found)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let action = if req.pinned { "pinned" } else { "unpinned" };
    let embed = embed_for(ctx, &updated, &accessible).await;
    emit_and_notify(
        ctx,
        &accessible,
        "discussion",
        action,
        crate::actions::payload::discussion_payload(
            action,
            &embed,
            None,
            None,
            None,
            &repository,
            &sender,
        ),
        &user,
    )
    .await;
    discussion_to_public(ctx, &updated, &accessible).await
}

/// `discussions.lock` — Write+; fires `locked`/`unlocked`. Locked threads
/// reject new comments (GitHub semantics — moderators can still post).
pub async fn lock(ctx: &RpcCtx, input: serde_json::Value) -> Result<DiscussionPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionLockRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.lock input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    if row.locked == req.locked {
        return discussion_to_public(ctx, &row, &accessible).await;
    }
    let updated = ctx
        .db
        .set_discussion_flags(&row.id, None, Some(req.locked))
        .await
        .map_err(db_err)?
        .ok_or_else(discussion_not_found)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let action = if req.locked { "locked" } else { "unlocked" };
    let embed = embed_for(ctx, &updated, &accessible).await;
    emit_and_notify(
        ctx,
        &accessible,
        "discussion",
        action,
        crate::actions::payload::discussion_payload(
            action,
            &embed,
            None,
            None,
            None,
            &repository,
            &sender,
        ),
        &user,
    )
    .await;
    discussion_to_public(ctx, &updated, &accessible).await
}

/// `discussions.transfer` — Write+ on the destination (and the source);
/// fires `transferred` on the destination repo. GitHub drops the category
/// and answer on transfer (categories are repo-scoped); we match.
pub async fn transfer(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionTransferRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.transfer input: {e}"),
        )
    })?;
    let source = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let target = resolve_for_write(ctx, &req.target_owner, &req.target_name).await?;
    if target.row.id == source.row.id {
        return Err(AppError::new(
            "rpc.bad_input",
            "cannot transfer a discussion to its own repository",
        ));
    }
    let row = load_discussion_in_repo(ctx, &source.row.id, req.discussion).await?;

    let new_number = ctx
        .db
        .allocate_next_issue_number(&target.row.id)
        .await
        .map_err(db_err)?;
    let updated = ctx
        .db
        .transfer_discussion(&row.id, &target.row.id, new_number)
        .await
        .map_err(db_err)?
        .ok_or_else(discussion_not_found)?;

    let (repository, sender) = repository_and_sender(&target, &user);
    let embed = embed_for(ctx, &updated, &target).await;
    emit_and_notify(
        ctx,
        &target,
        "discussion",
        "transferred",
        crate::actions::payload::discussion_payload(
            "transferred",
            &embed,
            Some(&json!({ "new_repository": repository })),
            None,
            None,
            &repository,
            &sender,
        ),
        &user,
    )
    .await;
    discussion_to_public(ctx, &updated, &target).await
}

/// Validate a caller-supplied label id set against the repo's effective
/// labels (same gate as `issue.labels.set`).
async fn validate_label_ids(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    ids: &[String],
) -> Result<Vec<String>, AppError> {
    let allowed = crate::label::effective_label_id_set(
        ctx,
        &accessible.row.id,
        &accessible.row.owner_type,
        &accessible.row.owner_id,
    )
    .await?;
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for id in ids {
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
            unique.push(id.to_string());
        }
    }
    Ok(unique)
}

/// `discussions.labels.set` — Write+; replaces the label set and fires one
/// `labeled`/`unlabeled` event per delta (GitHub semantics — `payload.label`
/// names the changed label).
pub async fn labels_set(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionLabelsSetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.labels.set input: {e}"),
        )
    })?;
    let accessible = resolve_for_write(ctx, &req.owner, &req.name).await?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    let unique_ids = validate_label_ids(ctx, &accessible, &req.label_ids).await?;

    let before: std::collections::HashSet<String> = ctx
        .db
        .list_discussion_label_ids(&row.id)
        .await
        .map_err(db_err)?
        .into_iter()
        .collect();
    ctx.db
        .set_discussion_labels(&row.id, &unique_ids)
        .await
        .map_err(db_err)?;

    let after: std::collections::HashSet<&str> = unique_ids.iter().map(String::as_str).collect();
    let mut deltas: Vec<(&str, oxidean_db::LabelRow)> = Vec::new();
    for id in &before {
        if !after.contains(id.as_str()) {
            if let Ok(Some(label)) = ctx.db.find_label_by_id(id).await {
                deltas.push(("unlabeled", label));
            }
        }
    }
    for id in &unique_ids {
        if !before.contains(id) {
            if let Ok(Some(label)) = ctx.db.find_label_by_id(id).await {
                deltas.push(("labeled", label));
            }
        }
    }

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let updated = ctx
        .db
        .find_discussion_by_id(&row.id)
        .await
        .map_err(db_err)?
        .ok_or_else(discussion_not_found)?;
    let embed = embed_for(ctx, &updated, &accessible).await;
    for (action, label) in deltas {
        let label_embed = json!({
            "id": label.id,
            "name": label.name,
            "color": label.color,
            "description": label.description,
        });
        emit_and_notify(
            ctx,
            &accessible,
            "discussion",
            action,
            crate::actions::payload::discussion_payload(
                action,
                &embed,
                None,
                Some(&label_embed),
                None,
                &repository,
                &sender,
            ),
            &user,
        )
        .await;
    }
    discussion_to_public(ctx, &updated, &accessible).await
}

/// `discussions.answer.set` — Write+ or the discussion author (GitHub lets
/// the author pick the answer); answerable categories only; fires `answered`.
pub async fn answer_set(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionAnswerSetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.answer.set input: {e}"),
        )
    })?;
    let accessible = resolve_for_read(ctx, &req.owner, &req.name).await?;
    require_unit_enabled(ctx, RepoUnit::Discussions, &accessible.row.id).await?;
    ensure_not_archived(&accessible)?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    let is_author = row.author_id.as_deref() == Some(user.id.as_str());
    if !meets(accessible.capability, Capability::Write) && !is_author {
        return Err(not_found());
    }
    let comment = load_comment_in_discussion(ctx, &row.id, &req.comment).await?;
    let category = match &row.category_id {
        Some(cid) => ctx
            .db
            .find_discussion_category_by_id(cid)
            .await
            .map_err(db_err)?,
        None => None,
    };
    match &category {
        Some(c) if c.is_answerable => {}
        _ => {
            return Err(AppError::new(
                "discussion.not_answerable",
                "this discussion's category does not accept answers",
            ))
        }
    }

    let updated = ctx
        .db
        .set_discussion_answer(&row.id, Some(&comment.id), Some(&user.id))
        .await
        .map_err(db_err)?
        .ok_or_else(discussion_not_found)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let embed = embed_for(ctx, &updated, &accessible).await;
    let answer_embed = json!({
        "id": comment.id,
        "html_url": comment_html_url(&accessible, row.number, &comment.id),
    });
    emit_and_notify(
        ctx,
        &accessible,
        "discussion",
        "answered",
        crate::actions::payload::discussion_payload(
            "answered",
            &embed,
            None,
            None,
            Some(&answer_embed),
            &repository,
            &sender,
        ),
        &user,
    )
    .await;
    discussion_to_public(ctx, &updated, &accessible).await
}

/// `discussions.answer.clear` — Write+ or author; fires `unanswered`.
pub async fn answer_clear(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.answer.clear input: {e}"),
        )
    })?;
    let accessible = resolve_for_read(ctx, &req.owner, &req.name).await?;
    require_unit_enabled(ctx, RepoUnit::Discussions, &accessible.row.id).await?;
    ensure_not_archived(&accessible)?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    let is_author = row.author_id.as_deref() == Some(user.id.as_str());
    if !meets(accessible.capability, Capability::Write) && !is_author {
        return Err(not_found());
    }
    if row.answer_comment_id.is_none() {
        return discussion_to_public(ctx, &row, &accessible).await;
    }
    let updated = ctx
        .db
        .set_discussion_answer(&row.id, None, None)
        .await
        .map_err(db_err)?
        .ok_or_else(discussion_not_found)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let embed = embed_for(ctx, &updated, &accessible).await;
    emit_and_notify(
        ctx,
        &accessible,
        "discussion",
        "unanswered",
        crate::actions::payload::discussion_payload(
            "unanswered",
            &embed,
            None,
            None,
            None,
            &repository,
            &sender,
        ),
        &user,
    )
    .await;
    discussion_to_public(ctx, &updated, &accessible).await
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

/// `discussions.comments.list` — Read+; chronological, `parent_id` threads
/// replies.
pub async fn comments_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionCommentsListResponse, AppError> {
    let req: DiscussionCommentsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.comments.list input: {e}"),
        )
    })?;
    let accessible = resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    let comments = ctx
        .db
        .list_discussion_comments(&row.id)
        .await
        .map_err(db_err)?;
    let mut out = Vec::with_capacity(comments.len());
    for c in &comments {
        out.push(comment_to_public(ctx, c, row.number, &accessible).await);
    }
    Ok(DiscussionCommentsListResponse { comments: out })
}

/// `discussions.comments.create` — Read+ collaborators (GitHub lets any
/// signed-in user comment on public discussions; we mirror issues — Write+
/// is NOT required, matching `issue.comments.create`). Replies thread one
/// level deep; locked threads reject non-moderators.
pub async fn comments_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionCommentPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionCommentCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.comments.create input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    require_unit_enabled(ctx, RepoUnit::Discussions, &accessible.row.id).await?;
    ensure_not_archived(&accessible)?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    let body = validate_body(Some(&req.body))?;
    if body.trim().is_empty() {
        return Err(AppError::new("rpc.bad_input", "body is required"));
    }
    if row.locked && !meets(accessible.capability, Capability::Write) {
        return Err(AppError::new("discussion.locked", "Discussion is locked"));
    }
    // One level of threading — `parent` must be a top-level comment on the
    // same discussion (GitHub rejects replying to a reply).
    let parent_id = match req.parent.as_deref() {
        Some(pid) => {
            let parent = load_comment_in_discussion(ctx, &row.id, pid).await?;
            if parent.parent_id.is_some() {
                return Err(AppError::new(
                    "rpc.bad_input",
                    "cannot reply to a reply — discussions thread one level deep",
                ));
            }
            Some(parent.id)
        }
        None => None,
    };
    let comment = ctx
        .db
        .insert_discussion_comment(
            &Uuid::new_v4().to_string(),
            &row.id,
            parent_id.as_deref(),
            Some(&user.id),
            &body,
        )
        .await
        .map_err(db_err)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let embed = embed_for(ctx, &row, &accessible).await;
    emit_and_notify(
        ctx,
        &accessible,
        "discussion_comment",
        "created",
        crate::actions::payload::discussion_comment_payload(
            "created",
            &embed,
            &comment,
            &comment_html_url(&accessible, row.number, &comment.id),
            Some(&user.username),
            None,
            &repository,
            &sender,
        ),
        &user,
    )
    .await;
    Ok(comment_to_public(ctx, &comment, row.number, &accessible).await)
}

/// `discussions.comments.update` — author or Write+; fires `edited` with
/// `changes.body.from`.
pub async fn comments_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionCommentPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionCommentUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.comments.update input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    require_unit_enabled(ctx, RepoUnit::Discussions, &accessible.row.id).await?;
    ensure_not_archived(&accessible)?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    let comment = load_comment_in_discussion(ctx, &row.id, &req.comment).await?;
    let is_author = comment.author_id.as_deref() == Some(user.id.as_str());
    if !meets(accessible.capability, Capability::Write) && !is_author {
        return Err(not_found());
    }
    let body = validate_body(Some(&req.body))?;
    let old_body = comment.body.clone();
    let updated = ctx
        .db
        .update_discussion_comment(&comment.id, &body)
        .await
        .map_err(db_err)?
        .ok_or_else(comment_not_found)?;

    if old_body != body {
        let (repository, sender) = repository_and_sender(&accessible, &user);
        let embed = embed_for(ctx, &row, &accessible).await;
        emit_and_notify(
            ctx,
            &accessible,
            "discussion_comment",
            "edited",
            crate::actions::payload::discussion_comment_payload(
                "edited",
                &embed,
                &updated,
                &comment_html_url(&accessible, row.number, &updated.id),
                Some(&user.username),
                Some(&json!({ "body": { "from": old_body } })),
                &repository,
                &sender,
            ),
            &user,
        )
        .await;
    }
    Ok(comment_to_public(ctx, &updated, row.number, &accessible).await)
}

/// `discussions.comments.delete` — author or Write+; fires `deleted`.
/// Deleting the marked answer clears `answer_comment_id` (DB layer) and
/// emits `unanswered` too (GitHub semantics).
pub async fn comments_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DiscussionDeleteResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: DiscussionCommentRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid discussions.comments.delete input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    require_unit_enabled(ctx, RepoUnit::Discussions, &accessible.row.id).await?;
    ensure_not_archived(&accessible)?;
    let row = load_discussion_in_repo(ctx, &accessible.row.id, req.discussion).await?;
    let comment = load_comment_in_discussion(ctx, &row.id, &req.comment).await?;
    let is_author = comment.author_id.as_deref() == Some(user.id.as_str());
    if !meets(accessible.capability, Capability::Write) && !is_author {
        return Err(not_found());
    }
    let was_answer = row.answer_comment_id.as_deref() == Some(comment.id.as_str());
    ctx.db
        .delete_discussion_comment(&comment.id)
        .await
        .map_err(db_err)?;

    let (repository, sender) = repository_and_sender(&accessible, &user);
    let embed = embed_for(
        ctx,
        &ctx.db
            .find_discussion_by_id(&row.id)
            .await
            .map_err(db_err)?
            .unwrap_or_else(|| row.clone()),
        &accessible,
    )
    .await;
    emit_and_notify(
        ctx,
        &accessible,
        "discussion_comment",
        "deleted",
        crate::actions::payload::discussion_comment_payload(
            "deleted",
            &embed,
            &comment,
            &comment_html_url(&accessible, row.number, &comment.id),
            Some(&user.username),
            None,
            &repository,
            &sender,
        ),
        &user,
    )
    .await;
    if was_answer {
        emit_and_notify(
            ctx,
            &accessible,
            "discussion",
            "unanswered",
            crate::actions::payload::discussion_payload(
                "unanswered",
                &embed,
                None,
                None,
                None,
                &repository,
                &sender,
            ),
            &user,
        )
        .await;
    }
    Ok(DiscussionDeleteResponse { ok: true })
}
