//! Release RPC handlers — tag-bound notes (GIT-14 / D-REL-01..03 / D-REL-12).

use oxidean_core::{
    AppError, CreateReleaseRequest, DeleteReleaseAssetRequest, DeleteReleaseAssetResponse,
    DeleteReleaseRequest, DeleteReleaseResponse, ReleaseAssetPublic, ReleaseGetRequest,
    ReleaseListRequest, ReleaseListResponse, ReleasePublic, UpdateReleaseRequest,
};
use oxidean_db::{ReleaseAssetRow, ReleaseRow};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::git::bare_repo_path;
use crate::notify;
use crate::repo::resolve_repo_for_admin;
use crate::repo::{
    ensure_not_archived, meets, not_found, resolve_repo_for_read, AccessibleRepo, Capability,
};
use crate::routes::release_assets::{delete_asset_with_file, remove_asset_file};
use crate::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new("db.not_configured", "no database configured for this instance")
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new("release.tag_taken", "A release already exists for this tag.")
    } else if e == "release not found" {
        AppError::new("release.not_found", "Release not found")
    } else {
        tracing::error!("release db error: {e}");
        AppError::new("release.internal", "release operation failed")
    }
}

fn release_not_found() -> AppError {
    AppError::new("release.not_found", "Release not found")
}

fn tag_missing() -> AppError {
    AppError::new("release.tag_missing", "Tag does not exist on this repository.")
}

fn asset_public(row: &ReleaseAssetRow) -> ReleaseAssetPublic {
    ReleaseAssetPublic {
        id: row.id.clone(),
        release_id: row.release_id.clone(),
        filename: row.filename.clone(),
        content_type: row.content_type.clone(),
        byte_size: row.byte_size,
        uploader_id: row.uploader_id.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

async fn to_public(ctx: &RpcCtx, row: &ReleaseRow) -> Result<ReleasePublic, AppError> {
    let author_username = match ctx.db.find_user_by_id(&row.author_id).await {
        Ok(Some(u)) => u.username,
        Ok(None) => String::new(),
        Err(e) => return Err(db_err(e)),
    };
    let assets = ctx
        .db
        .list_assets_for_release(&row.id)
        .await
        .map_err(db_err)?
        .iter()
        .map(asset_public)
        .collect();
    Ok(ReleasePublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        tag_name: row.tag_name.clone(),
        title: row.title.clone(),
        body: row.body.clone(),
        draft: row.draft,
        prerelease: row.prerelease,
        author_id: row.author_id.clone(),
        author_username,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
        assets,
    })
}

fn can_see_drafts(accessible: &AccessibleRepo) -> bool {
    meets(accessible.capability, Capability::Write)
}

async fn tag_exists(ctx: &RpcCtx, accessible: &AccessibleRepo, tag_name: &str) -> Result<bool, AppError> {
    let path = bare_repo_path(&ctx.repos_dir, &accessible.owner_username, &accessible.row.name)?;
    let refs = ctx.git.list_refs(&path).await.map_err(|e| {
        tracing::error!(error = %e, "list_refs for release tag check");
        AppError::new("release.internal", "release operation failed")
    })?;
    let want_full = format!("refs/tags/{tag_name}");
    Ok(refs.iter().any(|r| r.name == want_full || r.name == tag_name || r.name.strip_prefix("refs/tags/") == Some(tag_name)))
}

pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<ReleasePublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: CreateReleaseRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid release.create input: {e}"))
    })?;
    let tag_name = req.tag_name.trim();
    if tag_name.is_empty() {
        return Err(AppError::new("rpc.bad_input", "tag_name is required"));
    }
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    // GIT-20: releases are content — frozen while archived.
    ensure_not_archived(&accessible)?;
    if !tag_exists(ctx, &accessible, tag_name).await? {
        return Err(tag_missing());
    }
    let title = if req.title.trim().is_empty() { tag_name.to_string() } else { req.title.trim().to_string() };
    let id = Uuid::new_v4().to_string();
    let row = ctx.db.insert_release(&id, &accessible.row.id, tag_name, &title, &req.body, req.draft, req.prerelease, &user.id).await.map_err(db_err)?;
    // DEBT-06: publish fan-out through the watch matrix — `all` watchers are
    // notified; drafts stay silent until they publish.
    if !row.draft {
        let subject = notify::subject_for_release(&row.repo_id, &row.tag_name, &row.title);
        notify::fanout_activity(&ctx.db, &user.id, [], "release_published", &subject).await;
    }
    to_public(ctx, &row).await
}

pub async fn list(ctx: &RpcCtx, input: serde_json::Value) -> Result<ReleaseListResponse, AppError> {
    let req: ReleaseListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid release.list input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let include_drafts = can_see_drafts(&accessible);
    let rows = ctx.db.list_releases_for_repo(&accessible.row.id, include_drafts).await.map_err(db_err)?;
    let mut releases = Vec::with_capacity(rows.len());
    for row in rows {
        releases.push(to_public(ctx, &row).await?);
    }
    Ok(ReleaseListResponse { releases })
}

pub async fn get(ctx: &RpcCtx, input: serde_json::Value) -> Result<ReleasePublic, AppError> {
    let req: ReleaseGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid release.get input: {e}"))
    })?;
    let tag_name = req.tag_name.trim();
    if tag_name.is_empty() {
        return Err(AppError::new("rpc.bad_input", "tag_name is required"));
    }
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let row = ctx.db.find_release_by_repo_tag(&accessible.row.id, tag_name).await.map_err(db_err)?.ok_or_else(release_not_found)?;
    if row.draft && !can_see_drafts(&accessible) {
        return Err(release_not_found());
    }
    to_public(ctx, &row).await
}

pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<ReleasePublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: UpdateReleaseRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid release.update input: {e}"))
    })?;
    let tag_name = req.tag_name.trim();
    if tag_name.is_empty() {
        return Err(AppError::new("rpc.bad_input", "tag_name is required"));
    }
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    // GIT-20: releases are content — frozen while archived.
    ensure_not_archived(&accessible)?;
    let existing = ctx.db.find_release_by_repo_tag(&accessible.row.id, tag_name).await.map_err(db_err)?.ok_or_else(release_not_found)?;
    let is_author = existing.author_id == user.id;
    if !is_author && !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    let title = req.title.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(existing.title.as_str()).to_string();
    let body = req.body.clone().unwrap_or_else(|| existing.body.clone());
    let draft = req.draft.unwrap_or(existing.draft);
    let prerelease = req.prerelease.unwrap_or(existing.prerelease);
    let row = ctx.db.update_release(&existing.id, &title, &body, draft, prerelease).await.map_err(db_err)?;
    // DEBT-06: draft→non-draft is a publish; edits to a live release notify.
    // Draft edits stay invisible to watchers.
    let reason = if existing.draft && !row.draft {
        Some("release_published")
    } else if !existing.draft {
        Some("release_edited")
    } else {
        None
    };
    if let Some(reason) = reason {
        let subject = notify::subject_for_release(&row.repo_id, &row.tag_name, &row.title);
        notify::fanout_activity(&ctx.db, &user.id, [], reason, &subject).await;
    }
    to_public(ctx, &row).await
}

pub async fn delete(ctx: &RpcCtx, input: serde_json::Value) -> Result<DeleteReleaseResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: DeleteReleaseRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid release.delete input: {e}"))
    })?;
    let tag_name = req.tag_name.trim();
    if tag_name.is_empty() {
        return Err(AppError::new("rpc.bad_input", "tag_name is required"));
    }
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    // GIT-20: releases are content — frozen while archived.
    ensure_not_archived(&accessible)?;
    let existing = ctx.db.find_release_by_repo_tag(&accessible.row.id, tag_name).await.map_err(db_err)?.ok_or_else(release_not_found)?;
    let assets = ctx
        .db
        .list_assets_for_release(&existing.id)
        .await
        .map_err(db_err)?;
    for asset in &assets {
        remove_asset_file(&ctx.release_assets_dir, &asset.id).await;
    }
    ctx.db.delete_release(&existing.id).await.map_err(db_err)?;
    // DEBT-06: deleting a live release notifies watchers; drafts never existed.
    if !existing.draft {
        let subject =
            notify::subject_for_release(&accessible.row.id, &existing.tag_name, &existing.title);
        notify::fanout_activity(&ctx.db, &user.id, [], "release_deleted", &subject).await;
    }
    Ok(DeleteReleaseResponse { ok: true })
}

pub async fn delete_asset(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DeleteReleaseAssetResponse, AppError> {
    let _user = require_verified(ctx).await?;
    let req: DeleteReleaseAssetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid release.deleteAsset input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    // GIT-20: releases are content — frozen while archived.
    ensure_not_archived(&accessible)?;
    let asset = ctx
        .db
        .find_release_asset_by_id(req.asset_id.trim())
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("release.asset_not_found", "Asset not found"))?;
    let release = ctx
        .db
        .find_release_by_id(&asset.release_id)
        .await
        .map_err(db_err)?
        .ok_or_else(release_not_found)?;
    if release.repo_id != accessible.row.id {
        return Err(AppError::new("release.asset_not_found", "Asset not found"));
    }
    delete_asset_with_file(ctx, &ctx.release_assets_dir, &asset.id).await?;
    Ok(DeleteReleaseAssetResponse { ok: true })
}
