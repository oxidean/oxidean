//! Admin + repo template RPC handlers (issue #18).

use oxidean_core::{
    AdminTemplateDeleteRequest, AdminTemplateSetEnabledRequest, AdminTemplateUpdateRequest,
    AdminTemplatesListResponse, AppError, InstanceTemplatePackPublic, RepoTemplateEnabledResponse,
    RepoTemplateGetEnabledRequest, RepoTemplateSetEnabledRequest,
};
use oxidean_db::InstanceTemplatePackRow;
use uuid::Uuid;

use crate::auth::admin::require_admin;
use crate::auth::gate::require_verified;
use crate::repo::{resolve_repo_for_admin, resolve_repo_for_read};
use crate::rpc::RpcCtx;
use crate::templates::store;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("template db error: {e}");
        AppError::new("admin.internal", "template operation failed")
    }
}

fn pack_public(row: InstanceTemplatePackRow) -> InstanceTemplatePackPublic {
    InstanceTemplatePackPublic {
        id: row.id,
        slug: row.slug,
        label: row.label,
        group: row.group,
        description: row.description,
        default_gitignore: row.default_gitignore,
        enabled: row.enabled,
        byte_size: row.byte_size,
        content_digest: row.content_digest,
        uploaded_by_user_id: row.uploaded_by_user_id,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// `admin.templates.list`
pub async fn admin_list(ctx: &RpcCtx) -> Result<AdminTemplatesListResponse, AppError> {
    require_admin(ctx).await?;
    let packs = ctx
        .db
        .list_instance_template_packs(false)
        .await
        .map_err(db_err)?;
    Ok(AdminTemplatesListResponse {
        packs: packs.into_iter().map(pack_public).collect(),
    })
}

/// `admin.templates.update`
pub async fn admin_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<InstanceTemplatePackPublic, AppError> {
    require_admin(ctx).await?;
    let req: AdminTemplateUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.templates.update input: {e}"),
        )
    })?;
    ctx.db
        .update_instance_template_pack(
            &req.id,
            req.label.as_deref(),
            req.group.as_deref(),
            req.description.as_deref(),
            // Only touch default_gitignore when the field is present in JSON — treated as
            // Option via serde default None meaning "leave unchanged".
            None,
        )
        .await
        .map_err(db_err)?;
    // If client sent default_gitignore key as null/string via optional field:
    if req.default_gitignore.is_some() {
        ctx.db
            .update_instance_template_pack(
                &req.id,
                None,
                None,
                None,
                Some(req.default_gitignore.as_deref()),
            )
            .await
            .map_err(db_err)?;
    }
    let row = ctx
        .db
        .get_instance_template_pack(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("admin.template_not_found", "Template pack not found."))?;
    Ok(pack_public(row))
}

/// `admin.templates.setEnabled`
pub async fn admin_set_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<InstanceTemplatePackPublic, AppError> {
    require_admin(ctx).await?;
    let req: AdminTemplateSetEnabledRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.templates.setEnabled input: {e}"),
        )
    })?;
    ctx.db
        .set_instance_template_pack_enabled(&req.id, req.enabled)
        .await
        .map_err(db_err)?;
    let row = ctx
        .db
        .get_instance_template_pack(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("admin.template_not_found", "Template pack not found."))?;
    Ok(pack_public(row))
}

/// `admin.templates.delete`
pub async fn admin_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    require_admin(ctx).await?;
    let req: AdminTemplateDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.templates.delete input: {e}"),
        )
    })?;
    let row = ctx
        .db
        .get_instance_template_pack(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("admin.template_not_found", "Template pack not found."))?;
    let digest = row.content_digest.clone();
    ctx.db
        .delete_instance_template_pack(&req.id)
        .await
        .map_err(db_err)?;
    // Content-addressed store: only unlink the zip when no other pack shares the digest.
    let remaining = ctx
        .db
        .count_instance_template_packs_by_digest(&digest)
        .await
        .map_err(db_err)?;
    if remaining == 0 {
        let _ = store::delete_pack(&ctx.template_packs_dir, &digest);
    }
    Ok(serde_json::json!({ "ok": true }))
}

/// Persist an uploaded zip as an instance template pack (called from HTTP multipart).
pub async fn create_pack_from_bytes(
    ctx: &RpcCtx,
    slug: &str,
    label: &str,
    group: &str,
    description: &str,
    default_gitignore: Option<&str>,
    bytes: &[u8],
) -> Result<InstanceTemplatePackPublic, AppError> {
    require_admin(ctx).await?;
    let user = require_verified(ctx).await?;
    let slug = slug.trim();
    if slug.is_empty()
        || !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::new(
            "admin.invalid_template_slug",
            "Slug must be ascii alphanumeric with hyphen/underscore.",
        ));
    }
    if ctx
        .db
        .get_instance_template_pack_by_slug(slug)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "admin.template_slug_taken",
            "A template pack with this slug already exists.",
        ));
    }
    let max = store::max_pack_bytes_from_env();
    let (digest, size) = store::put_pack(&ctx.template_packs_dir, bytes, max).map_err(|e| {
        AppError::new(
            "admin.invalid_template_pack",
            format!("Could not store template pack: {e}"),
        )
    })?;
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_instance_template_pack(
            &id,
            slug,
            label.trim(),
            if group.trim().is_empty() {
                "Custom"
            } else {
                group.trim()
            },
            description,
            default_gitignore,
            true,
            size as i64,
            &digest,
            &user.id,
        )
        .await
        .map_err(db_err)?;
    Ok(pack_public(row))
}

/// `repo.templates.getEnabled`
pub async fn repo_get_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoTemplateEnabledResponse, AppError> {
    let req: RepoTemplateGetEnabledRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.templates.getEnabled input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let enabled = ctx
        .db
        .get_repo_is_template(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(RepoTemplateEnabledResponse { enabled })
}

/// `repo.templates.setEnabled`
pub async fn repo_set_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoTemplateEnabledResponse, AppError> {
    let _user = require_verified(ctx).await?;
    let req: RepoTemplateSetEnabledRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.templates.setEnabled input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    ctx.db
        .set_repo_is_template(&accessible.row.id, req.enabled)
        .await
        .map_err(db_err)?;
    Ok(RepoTemplateEnabledResponse {
        enabled: req.enabled,
    })
}
