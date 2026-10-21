//! `pull.*` RPC handlers — get/list/update/close/reopen.
//! `pull.create` + synchronize/enrichment helpers live in
//! `oxidean_api_core::pull` (issue #162).

use oxidean_core::{
    AppError, PullListRequest, PullListResponse, PullPublic, PullRefRequest, UpdatePullRequest,
};

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::git::bare_repo_path;
use oxidean_api_core::notify;
use oxidean_api_core::pull::acl;
use oxidean_api_core::pull::refs;
use oxidean_api_core::pull::{
    db_err, emit_pull_event, load_pull_in_repo, notify_actions_for_pull, resolve_ref_sha,
    to_public, to_public_many, validate_body, validate_ref_name, validate_title,
};
use oxidean_api_core::rpc::RpcCtx;

/// `pull.get` — Read+.
pub async fn get(ctx: &RpcCtx, input: serde_json::Value) -> Result<PullPublic, AppError> {
    let req: PullRefRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pull.get input: {e}")))?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    to_public(ctx, &row).await
}

/// `pull.list` — Read+; default state `open` (D-PR-26).
pub async fn list(ctx: &RpcCtx, input: serde_json::Value) -> Result<PullListResponse, AppError> {
    let req: PullListRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pull.list input: {e}")))?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let state = req.state.as_deref().unwrap_or("open");
    let state_filter = match state {
        "all" => None,
        "closed" => Some("closed"), // note: merged listed separately via state=merged or all
        "merged" => Some("merged"),
        "open" => Some("open"),
        other => {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("invalid pull state filter: {other}"),
            ))
        }
    };
    let offset = req.offset.unwrap_or(0);
    let limit = req.limit.unwrap_or(25);

    let author_id = resolve_username_filter(ctx, req.author.as_deref()).await?;
    if req
        .author
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty())
        && author_id.is_none()
    {
        return Ok(PullListResponse {
            pulls: vec![],
            total: 0,
            offset,
            limit,
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
        return Ok(PullListResponse {
            pulls: vec![],
            total: 0,
            offset,
            limit,
        });
    }
    let label_filter = req
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let review_state = req
        .review_state
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
    let needs_post_filter = label_filter.is_some()
        || assignee_id.is_some()
        || review_state.is_some()
        || state == "closed"
        || (state == "merged" && author_id.is_some());

    let (rows, total) = if needs_post_filter || author_id.is_some() || q.is_some() {
        // Author/q via DB when possible; closed/label/assignee/review post-filtered (D-PR-26).
        let fetch_limit = if needs_post_filter { 500 } else { limit };
        let fetch_offset = if needs_post_filter { 0 } else { offset };
        let search_state = if matches!(state, "closed" | "merged" | "all") {
            "all"
        } else {
            state
        };
        let (all, _) = ctx
            .db
            .search_pulls_for_repo(
                &accessible.row.id,
                oxidean_db::PullSearchFilters {
                    state: search_state,
                    author_id: author_id.as_deref(),
                    q: q.as_deref(),
                    offset: fetch_offset,
                    limit: fetch_limit,
                },
            )
            .await
            .map_err(db_err)?;
        let mut filtered = all;
        if state == "closed" {
            filtered.retain(|r| r.state == "closed" || r.state == "merged");
        }
        if state == "merged" {
            filtered.retain(|r| r.state == "merged");
        }
        if let Some(ref lid) = label_filter {
            let ids: Vec<String> = filtered.iter().map(|r| r.id.clone()).collect();
            let matching: std::collections::HashSet<String> = ctx
                .db
                .pull_ids_with_label(&ids, lid)
                .await
                .map_err(db_err)?
                .into_iter()
                .collect();
            filtered.retain(|r| matching.contains(&r.id));
        }
        if let Some(ref aid) = assignee_id {
            let ids: Vec<String> = filtered.iter().map(|r| r.id.clone()).collect();
            let matching: std::collections::HashSet<String> = ctx
                .db
                .list_pull_assignees_for_pulls(&ids)
                .await
                .map_err(db_err)?
                .into_iter()
                .filter(|(_, a)| &a.user_id == aid)
                .map(|(pull_id, _)| pull_id)
                .collect();
            filtered.retain(|r| matching.contains(&r.id));
        }
        if let Some(ref rs) = review_state {
            let ids: Vec<String> = filtered.iter().map(|r| r.id.clone()).collect();
            let reviews = ctx.db.list_reviews_for_pulls(&ids).await.map_err(db_err)?;
            let mut by_pull: std::collections::HashMap<&str, Vec<&oxidean_db::PullReviewRow>> =
                std::collections::HashMap::new();
            for r in &reviews {
                by_pull.entry(r.pull_id.as_str()).or_default().push(r);
            }
            filtered.retain(|row| pull_review_state_matches(by_pull.get(row.id.as_str()), rs));
        }
        let total = filtered.len() as i64;
        let page: Vec<_> = if needs_post_filter {
            filtered
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect()
        } else {
            filtered
        };
        (page, total)
    } else {
        ctx.db
            .list_pulls_for_repo(&accessible.row.id, state_filter, offset, limit)
            .await
            .map_err(db_err)?
    };

    let pulls = to_public_many(ctx, &rows).await?;
    Ok(PullListResponse {
        pulls,
        total,
        offset,
        limit,
    })
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

/// Aggregate a pull's review state from its (already-fetched) reviews and
/// compare against the wanted filter value. Latest review per author wins;
/// `changes_requested` dominates `approved`; no reviews → `review_required`.
fn pull_review_state_matches(
    reviews: Option<&Vec<&oxidean_db::PullReviewRow>>,
    want: &str,
) -> bool {
    let mut latest: std::collections::BTreeMap<&str, &oxidean_db::PullReviewRow> =
        std::collections::BTreeMap::new();
    for r in reviews.into_iter().flatten() {
        match latest.get(r.author_id.as_str()) {
            Some(prev) if prev.submitted_at >= r.submitted_at => {}
            _ => {
                latest.insert(r.author_id.as_str(), r);
            }
        }
    }
    let has_changes = latest.values().any(|r| r.state == "changes_requested");
    let has_approved = latest.values().any(|r| r.state == "approved");
    let actual = if has_changes {
        "changes_requested"
    } else if has_approved {
        "approved"
    } else {
        "review_required"
    };
    actual == want
}

/// `pull.close` — Author or Write+; open → closed (PR-06 / D-PR-29).
pub async fn close(ctx: &RpcCtx, input: serde_json::Value) -> Result<PullPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: PullRefRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pull.close input: {e}")))?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    if !acl::can_edit_pull(&user.id, &row, accessible.capability) {
        return Err(oxidean_api_core::repo::not_found());
    }
    if row.state != "open" {
        return Err(AppError::new(
            "pull.invalid_state",
            "only open pull requests can be closed",
        ));
    }
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .set_pull_state(&row.id, "closed", Some(&now), Some(&user.id))
        .await
        .map_err(db_err)?;
    let updated = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let subject = notify::subject_for_pull(&updated);
    let recipients = notify::pull_participant_ids(&ctx.db, &updated.id, &updated.author_id).await;
    notify::fanout_activity(&ctx.db, &user.id, recipients, "pr_closed", &subject).await;
    emit_pull_event(
        ctx,
        &accessible,
        &updated,
        "closed",
        &user.username,
        &user.id,
        false,
    )
    .await;
    // Merge queue: closing a queued PR removes it from the train.
    oxidean_api_core::merge_queue::after_pull_closed(
        &oxidean_api_core::merge_queue::MqDeps::from_ctx(ctx),
        &accessible.row.id,
        &updated,
        &user.username,
        &user.id,
    )
    .await;
    to_public(ctx, &updated).await
}

/// `pull.reopen` — Author or Write+; closed → open (not merged) (PR-06 / D-PR-29).
pub async fn reopen(ctx: &RpcCtx, input: serde_json::Value) -> Result<PullPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: PullRefRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pull.reopen input: {e}")))?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    if !acl::can_edit_pull(&user.id, &row, accessible.capability) {
        return Err(oxidean_api_core::repo::not_found());
    }
    if row.state != "closed" {
        return Err(AppError::new(
            "pull.invalid_state",
            "only closed (unmerged) pull requests can be reopened",
        ));
    }
    ctx.db
        .set_pull_state(&row.id, "open", None, None)
        .await
        .map_err(db_err)?;
    let updated = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    // API-06: self-heal the pull head ref for PRs opened before the namespace
    // existed (or after a ref write was lost).
    refs::sync_head_ref(
        ctx.git.as_ref(),
        &ctx.repos_dir,
        &ctx.db,
        &accessible.owner_username,
        &accessible.row.name,
        &updated,
    )
    .await;
    let subject = notify::subject_for_pull(&updated);
    let recipients = notify::pull_participant_ids(&ctx.db, &updated.id, &updated.author_id).await;
    notify::fanout_activity(&ctx.db, &user.id, recipients, "pr_reopened", &subject).await;
    emit_pull_event(
        ctx,
        &accessible,
        &updated,
        "reopened",
        &user.username,
        &user.id,
        false,
    )
    .await;
    let (head_owner, head_name) = if updated.head_repo_id == updated.repo_id {
        (
            accessible.owner_username.clone(),
            accessible.row.name.clone(),
        )
    } else if let Ok(Some(head_repo)) = ctx.db.find_repository_by_id(&updated.head_repo_id).await {
        let owner = match head_repo.owner_type.as_str() {
            "org" => ctx
                .db
                .find_organization_by_id(&head_repo.owner_id)
                .await
                .ok()
                .flatten()
                .map(|o| o.slug)
                .unwrap_or_default(),
            _ => ctx
                .db
                .find_user_by_id(&head_repo.owner_id)
                .await
                .ok()
                .flatten()
                .map(|u| u.username)
                .unwrap_or_default(),
        };
        (owner, head_repo.name)
    } else {
        (
            accessible.owner_username.clone(),
            accessible.row.name.clone(),
        )
    };
    notify_actions_for_pull(
        &ctx.db,
        ctx.git.as_ref(),
        &ctx.repos_dir,
        &updated,
        &accessible.owner_username,
        &accessible.row.name,
        &head_owner,
        &head_name,
        Some(&user.id),
        oxidean_api_core::actions::events::rpc_caller_is_job_token(ctx),
        oxidean_api_core::actions::PullRequestAction::Reopened,
        false,
    )
    .await;
    to_public(ctx, &updated).await
}

/// `pull.update` — Author or Write+; title/body/base_ref/draft (D-PR-04 partial / D-PR-29).
pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<PullPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: UpdatePullRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pull.update input: {e}")))?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    if !acl::can_edit_pull(&user.id, &row, accessible.capability) {
        return Err(oxidean_api_core::repo::not_found());
    }
    if row.state == "merged" {
        return Err(AppError::new(
            "pull.invalid_state",
            "merged pull requests cannot be updated",
        ));
    }
    let title = match req.title.as_deref() {
        Some(t) => validate_title(t)?.to_string(),
        None => row.title.clone(),
    };
    let body = match req.body.as_deref() {
        Some(b) => validate_body(Some(b))?,
        None => row.body.clone(),
    };
    let draft = req.draft.unwrap_or(row.draft);
    let (base_ref, base_sha) = if let Some(br) = req.base_ref.as_deref() {
        let base_ref = validate_ref_name(br, "base_ref")?;
        let base_sha = resolve_ref_sha(
            ctx,
            &accessible.owner_username,
            &accessible.row.name,
            &base_ref,
        )
        .await?;
        (base_ref, base_sha)
    } else {
        (row.base_ref.clone(), row.base_sha.clone())
    };

    let new_head_sha = if row.head_repo_id == row.repo_id {
        resolve_ref_sha(
            ctx,
            &accessible.owner_username,
            &accessible.row.name,
            &row.head_ref,
        )
        .await
        .unwrap_or_else(|_| row.head_sha.clone())
    } else {
        row.head_sha.clone()
    };
    let head_changed = new_head_sha != row.head_sha;
    let base_changed = base_ref != row.base_ref || base_sha != row.base_sha;

    ctx.db
        .update_pull_fields(&row.id, &title, &body, draft, &base_ref, &base_sha)
        .await
        .map_err(db_err)?;
    if head_changed {
        ctx.db
            .update_pull_head_sha(&row.id, &new_head_sha)
            .await
            .map_err(db_err)?;
        // API-06: refresh refs/pull/{N}/head (same-repo refresh — object present).
        if let Ok(bare) = bare_repo_path(
            &ctx.repos_dir,
            &accessible.owner_username,
            &accessible.row.name,
        ) {
            if let Err(e) = ctx
                .git
                .update_ref(&bare, &refs::head_ref_name(row.number), &new_head_sha)
                .await
            {
                tracing::warn!(error = %e, pull = %row.id, "pull refs: head ref write failed");
            }
        }
    }
    if head_changed || base_changed {
        ctx.db
            .mark_pull_line_comments_outdated(&row.id)
            .await
            .map_err(db_err)?;
    }
    let updated = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    // Merge queue: a head move or base retarget removes the PR from its
    // train — the queued commit/branch is stale.
    if head_changed || base_ref != row.base_ref {
        let reason = if head_changed {
            "head_changed"
        } else {
            "base_changed"
        };
        let deps = oxidean_api_core::merge_queue::MqDeps::from_ctx(ctx);
        if head_changed {
            oxidean_api_core::merge_queue::after_pull_head_moved(
                &deps,
                &accessible.row.id,
                &updated,
                &user.username,
                &user.id,
            )
            .await;
        } else {
            oxidean_api_core::merge_queue::dequeue_entry_for_pull(
                &deps,
                &accessible.row.id,
                &updated,
                reason,
                &user.username,
                &user.id,
            )
            .await;
        }
    }
    let action = if head_changed {
        "synchronize"
    } else {
        "edited"
    };
    emit_pull_event(
        ctx,
        &accessible,
        &updated,
        action,
        &user.username,
        &user.id,
        false,
    )
    .await;
    to_public(ctx, &updated).await
}
