//! `merge_queue.*` RPC handlers — extracted from `core::merge_queue` (#162).

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::git::bare_repo_path;
use oxidean_api_core::protection::effective_for_branch;
use oxidean_api_core::repo::{resolve_repo_for_read, Capability, RepoUnit};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::AppError;
use serde::Deserialize;

use oxidean_api_core::merge_queue::*;

/// `merge_queue.list` — Read; the train slice (default `queued`).
pub async fn list(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    #[derive(Deserialize)]
    struct Req {
        owner: String,
        name: String,
        #[serde(default)]
        base_ref: Option<String>,
        #[serde(default)]
        state: Option<String>,
    }
    let req: Req = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid merge_queue.list: {e}")))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    oxidean_api_core::repo::units::require_unit_enabled(ctx, RepoUnit::Pulls, &accessible.row.id)
        .await?;
    let state = req.state.unwrap_or_else(|| "queued".into());
    // `all` maps to the empty filter the DB layer ORs out.
    let state_filter = if state == "all" { "" } else { state.as_str() };
    let entries = ctx
        .db
        .list_merge_queue_entries(
            &accessible.row.id,
            req.base_ref.as_deref().unwrap_or(""),
            state_filter,
        )
        .await
        .map_err(db_err)?;
    let mut out = Vec::with_capacity(entries.len());
    for e in &entries {
        let pull = load_pull_for_entry(&ctx.db, e).await;
        let login = username_of(&ctx.db, &e.enqueued_by).await;
        out.push(entry_to_public(e, pull.as_ref(), login));
    }
    Ok(
        serde_json::to_value(oxidean_core::MergeQueueListResponse { entries: out })
            .unwrap_or_default(),
    )
}

/// `merge_queue.get` — Read; the PR's queue slot + the group it rides.
pub async fn get(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let req: oxidean_core::MergeQueueGetRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid merge_queue.get: {e}")))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    oxidean_api_core::repo::units::require_unit_enabled(ctx, RepoUnit::Pulls, &accessible.row.id)
        .await?;
    // Live slot first; absent that, the newest row in any state so the
    // caller can render *why* the PR left the train (`dequeue_reason`).
    let entry = match ctx
        .db
        .find_merge_queue_entry_for_pull_number(&accessible.row.id, req.number, "queued")
        .await
        .map_err(db_err)?
    {
        Some(e) => Some(e),
        None => ctx
            .db
            .find_latest_merge_queue_entry_for_pull_number(&accessible.row.id, req.number)
            .await
            .map_err(db_err)?,
    };
    let mut group_public = None;
    if let Some(gid) = entry.as_ref().and_then(|e| e.group_id.as_deref()) {
        if let Ok(Some(g)) = ctx.db.find_merge_group(gid).await {
            group_public = Some(group_to_public(&ctx.db, &g).await);
        }
    }
    let entry_public = match &entry {
        Some(e) => {
            let pull = load_pull_for_entry(&ctx.db, e).await;
            let login = username_of(&ctx.db, &e.enqueued_by).await;
            Some(entry_to_public(e, pull.as_ref(), login))
        }
        None => None,
    };
    // `required` is per-base-branch — resolve through the entry's train or,
    // when unqueued, the PR's target branch (the merge button reads this).
    let base_ref = entry.as_ref().map(|e| e.base_ref.clone()).or(
        match ctx
            .db
            .find_pull_by_repo_number(&accessible.row.id, req.number)
            .await
        {
            Ok(Some(pull)) => Some(pull.base_ref),
            _ => None,
        },
    );
    let required = match base_ref {
        Some(b) => effective_for_branch(&ctx.db, &accessible.row.id, &b)
            .await
            .map(|eff| eff.required_merge_queue)
            .unwrap_or(false),
        None => false,
    };
    Ok(serde_json::to_value(oxidean_core::MergeQueueGetResponse {
        entry: entry_public,
        group: group_public,
        required,
    })
    .unwrap_or_default())
}

/// `merge_queue.groups` — Read; recent groups for a train.
pub async fn groups(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let req: oxidean_core::MergeQueueGroupsRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid merge_queue.groups: {e}")))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    oxidean_api_core::repo::units::require_unit_enabled(ctx, RepoUnit::Pulls, &accessible.row.id)
        .await?;
    let rows = ctx
        .db
        .list_merge_groups(
            &accessible.row.id,
            req.base_ref.as_deref().unwrap_or(""),
            req.limit.unwrap_or(20).clamp(1, 100),
        )
        .await
        .map_err(db_err)?;
    let mut groups = Vec::with_capacity(rows.len());
    for g in &rows {
        groups.push(group_to_public(&ctx.db, g).await);
    }
    Ok(serde_json::to_value(oxidean_core::MergeQueueGroupsResponse { groups }).unwrap_or_default())
}

/// `merge_queue.enqueue` — Write; admits the PR to its base train.
/// GitHub gates entry on the same protections `pull.merge` evaluates
/// (minus the queue flag itself) plus a clean mergeable check against the
/// live base tip.
pub async fn enqueue(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let user = require_verified(ctx).await?;
    let req: oxidean_core::MergeQueueEnqueueRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid merge_queue.enqueue: {e}")))?;
    let accessible = resolve_capability(ctx, &req.owner, &req.name, Capability::Write).await?;
    oxidean_api_core::repo::units::require_unit_enabled(ctx, RepoUnit::Pulls, &accessible.row.id)
        .await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    if pull.state != "open" {
        return Err(conflict(
            "only open pull requests can enter the merge queue",
        ));
    }
    if pull.draft {
        return Err(conflict("draft pull requests cannot enter the merge queue"));
    }
    let eff = effective_for_branch(&ctx.db, &accessible.row.id, &pull.base_ref).await?;
    if !eff.required_merge_queue {
        return Err(conflict(
            "merge queue is not required for the pull request's base branch",
        ));
    }
    if ctx
        .db
        .find_queued_entry_for_pull(&pull.id)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(conflict("pull request is already queued"));
    }
    // Same gate `pull.merge` runs — reviews, threads, required contexts,
    // up-to-date-head. The queue decides the method (`None` skips the
    // repo's allowed-methods check).
    oxidean_api_core::pull::merge_ops::evaluate_merge_gate(ctx, &accessible, &eff, &pull, None)
        .await?;
    let bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    oxidean_api_core::pull::ensure_head_objects_in_base(
        &ctx.db,
        ctx.git.as_ref(),
        &ctx.repos_dir,
        &bare,
        &pull,
    )
    .await?;
    // GitHub's mergeable gate at enqueue: the PR must merge cleanly onto
    // the live base tip — mid-train conflicts surface at group formation.
    let base_sha = ctx
        .git
        .rev_parse(&bare, &format!("refs/heads/{}", pull.base_ref))
        .await
        .map_err(|_| invalid("base branch does not resolve"))?;
    ctx.git
        .test_merge_commit(
            &bare,
            &base_sha,
            &pull.head_sha,
            &format!("Merge test for #{}", pull.number),
        )
        .await
        .map_err(|e| {
            let msg = e.to_string().to_lowercase();
            if msg.contains("conflict") {
                conflict("pull request cannot be merged — conflicts must be resolved first")
            } else {
                invalid("mergeability check failed")
            }
        })?;

    let entry_id = uuid::Uuid::new_v4().to_string();
    let entry = ctx
        .db
        .insert_merge_queue_entry(
            &entry_id,
            &accessible.row.id,
            &pull.id,
            pull.number,
            &pull.base_ref,
            &user.id,
        )
        .await
        .map_err(|e| {
            // The partial unique index is the last word on races — a second
            // enqueue landing between our check and INSERT surfaces here.
            if e.contains("unique") || e.contains("UNIQUE") {
                conflict("pull request is already queued")
            } else {
                db_err(e)
            }
        })?;

    let deps = MqDeps::from_ctx(ctx);
    let group = form_group(
        &deps,
        &QueueScope::of(&accessible),
        &pull.base_ref,
        Some(&user),
    )
    .await?;
    let entry = ctx
        .db
        .find_merge_queue_entry(&entry.id)
        .await
        .map_err(db_err)?
        .unwrap_or(entry);
    emit_pull_queue_event(
        &deps,
        &QueueScope::of(&accessible),
        &pull,
        "enqueued",
        None,
        &user.username,
        &user.id,
    )
    .await;
    let login = username_of(&ctx.db, &entry.enqueued_by).await;
    let group_public = match group {
        Some(g) => Some(group_to_public(&ctx.db, &g).await),
        None => None,
    };
    Ok(
        serde_json::to_value(oxidean_core::MergeQueueEnqueueResponse {
            entry: entry_to_public(&entry, Some(&pull), login),
            group: group_public,
        })
        .unwrap_or_default(),
    )
}

/// `merge_queue.dequeue` — Write; GitHub's "remove from queue". The entry
/// leaves; the group it rode is destroyed and re-forms without it.
pub async fn dequeue(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let user = require_verified(ctx).await?;
    let req: oxidean_core::MergeQueueDequeueRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid merge_queue.dequeue: {e}")))?;
    let accessible = resolve_capability(ctx, &req.owner, &req.name, Capability::Write).await?;
    let entry = ctx
        .db
        .find_merge_queue_entry_for_pull_number(&accessible.row.id, req.number, "queued")
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("merge_queue.not_found", "pull request is not queued"))?;
    let deps = MqDeps::from_ctx(ctx);
    let scope = QueueScope::of(&accessible);
    let entry = remove_entry(&deps, &scope, &entry, "manual", &user.username, &user.id).await?;
    let pull = load_pull_for_entry(&ctx.db, &entry).await;
    let login = username_of(&ctx.db, &entry.enqueued_by).await;
    Ok(
        serde_json::to_value(oxidean_core::MergeQueueDequeueResponse {
            entry: entry_to_public(&entry, pull.as_ref(), login),
        })
        .unwrap_or_default(),
    )
}
