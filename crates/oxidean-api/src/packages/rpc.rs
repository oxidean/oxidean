//! Session RPC: packages.list / deleteVersion / Admin usage+quota (PKG-05, D-PKG-09).

use std::collections::BTreeMap;

use oxidean_core::{
    AppError, PackagePublic, PackageUsageByFormat, PackageUsageRow, PackageVersionPublic,
    PackagesAdminSetQuotaRequest, PackagesAdminSetQuotaResponse, PackagesAdminUsageRequest,
    PackagesAdminUsageResponse, PackagesDeleteVersionRequest, PackagesDeleteVersionResponse,
    PackagesListRequest, PackagesListResponse,
};
use oxidean_db::PackageRow;

use crate::auth::gate::require_verified;
use crate::packages::acl::{self, PackageAction};
use crate::packages::quota::{self, owner_quota_default_from_env};
use crate::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    tracing::error!("packages rpc db error: {e}");
    AppError::new("packages.internal", "package operation failed")
}

async fn require_sys_admin(ctx: &RpcCtx) -> Result<(), AppError> {
    let user = require_verified(ctx).await?;
    if !user.role.is_sys_admin() {
        return Err(AppError::new(
            "admin.forbidden",
            "You need system admin access to manage package quotas.",
        ));
    }
    Ok(())
}

async fn resolve_owner_login(ctx: &RpcCtx, login: &str) -> Result<(String, String), AppError> {
    if let Some(u) = ctx.db.find_user_by_username(login).await.map_err(db_err)? {
        return Ok(("user".into(), u.id));
    }
    if let Some(o) = ctx
        .db
        .find_organization_by_slug(login)
        .await
        .map_err(db_err)?
    {
        return Ok(("org".into(), o.id));
    }
    Err(AppError::new("packages.not_found", "owner not found"))
}

async fn to_public(ctx: &RpcCtx, row: PackageRow) -> Result<Option<PackagePublic>, AppError> {
    let linked = match &row.repository_id {
        Some(id) => ctx.db.find_repository_by_id(id).await.map_err(db_err)?,
        None => None,
    };
    let uid = ctx.session.as_ref().map(|s| s.user_id.as_str());
    let have = acl::owner_capability(&ctx.db, uid, &row, linked.as_ref())
        .await
        .map_err(db_err)?;
    let visibility = acl::effective_visibility(&row, linked.as_ref());
    if !acl::authorize(&visibility, have, true, PackageAction::Pull) {
        if uid.is_none() {
            return Ok(None);
        }
        if !acl::authorize(&visibility, have, true, PackageAction::Pull) {
            return Ok(None);
        }
    }
    let versions = ctx
        .db
        .list_package_versions(&row.id)
        .await
        .map_err(db_err)?;
    Ok(Some(PackagePublic {
        id: row.id,
        owner_type: row.owner_type,
        owner_id: row.owner_id,
        name: row.name,
        format: row.format,
        visibility: row.visibility,
        repository_id: row.repository_id,
        versions: versions
            .into_iter()
            .map(|v| PackageVersionPublic {
                version: v.version,
                digest: v.digest,
                created_at: v.created_at,
            })
            .collect(),
    }))
}

pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PackagesListResponse, AppError> {
    // Anonymous OK — `to_public` enforces Pull ACL (public packages / linked-repo Read).
    let req: PackagesListRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid packages.list input: {e}")))?;

    let rows = if let Some(repo_id) = req.repository_id.as_deref() {
        ctx.db
            .list_packages_by_repository(repo_id)
            .await
            .map_err(db_err)?
    } else if let Some(owner) = req.owner.as_deref() {
        let (ot, oid) = resolve_owner_login(ctx, owner).await?;
        ctx.db
            .list_packages_by_owner(&ot, &oid)
            .await
            .map_err(db_err)?
    } else {
        return Err(AppError::new(
            "rpc.bad_input",
            "packages.list requires owner or repository_id",
        ));
    };

    let mut packages = Vec::new();
    for row in rows {
        if let Some(p) = to_public(ctx, row).await? {
            packages.push(p);
        }
    }
    Ok(PackagesListResponse { packages })
}

pub async fn delete_version(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PackagesDeleteVersionResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: PackagesDeleteVersionRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid packages.deleteVersion input: {e}"),
        )
    })?;

    let pkg = ctx
        .db
        .find_package_by_id(&req.package_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("packages.not_found", "package not found"))?;

    let expected = format!("{}@{}", pkg.name, req.version);
    if req.confirm != expected {
        return Err(AppError::new(
            "packages.confirm_mismatch",
            format!("confirm must equal {expected}"),
        ));
    }

    let linked = match &pkg.repository_id {
        Some(id) => ctx.db.find_repository_by_id(id).await.map_err(db_err)?,
        None => None,
    };
    let have = acl::owner_capability(&ctx.db, Some(&user.id), &pkg, linked.as_ref())
        .await
        .map_err(db_err)?;
    let visibility = acl::effective_visibility(&pkg, linked.as_ref());
    if !acl::authorize(&visibility, have, true, PackageAction::Delete) {
        return Err(AppError::new(
            "packages.forbidden",
            "Admin permission required to delete package versions",
        ));
    }

    let ver = ctx
        .db
        .find_package_version(&pkg.id, &req.version)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("packages.not_found", "version not found"))?;

    let digests = ctx
        .db
        .list_package_version_blob_digests(&ver.id)
        .await
        .map_err(db_err)?;
    ctx.db
        .delete_package_version(&ver.id)
        .await
        .map_err(db_err)?;
    for d in digests {
        let _ = ctx.db.adjust_package_blob_refcount(&d, -1).await;
    }
    // Deleting the last version removes the package entirely.
    let remaining = ctx
        .db
        .list_package_versions(&pkg.id)
        .await
        .map_err(db_err)?;
    if remaining.is_empty() {
        ctx.db.delete_package(&pkg.id).await.map_err(db_err)?;
    }
    Ok(PackagesDeleteVersionResponse { ok: true })
}

pub async fn admin_usage(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PackagesAdminUsageResponse, AppError> {
    require_sys_admin(ctx).await?;
    let req: PackagesAdminUsageRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid packages.adminUsage input: {e}"),
        )
    })?;
    let (owner_type, owner_id) = resolve_owner_login(ctx, &req.owner).await?;
    let used = quota::owner_usage_bytes(&ctx.db, &owner_type, &owner_id)
        .await
        .map_err(db_err)?;
    let quota_bytes = quota::effective_owner_quota(&ctx.db, &owner_type, &owner_id)
        .await
        .map_err(db_err)?;
    let rows = ctx
        .db
        .list_package_usage_for_owner(&owner_type, &owner_id)
        .await
        .map_err(db_err)?;
    let mut by_fmt: BTreeMap<String, u64> = BTreeMap::new();
    let mut packages = Vec::new();
    for r in rows {
        let b = r.bytes.max(0) as u64;
        *by_fmt.entry(r.format.clone()).or_default() += b;
        packages.push(PackageUsageRow {
            package_id: r.package_id,
            name: r.name,
            format: r.format,
            bytes: b,
        });
    }
    Ok(PackagesAdminUsageResponse {
        owner_type,
        owner_id,
        used_bytes: used,
        quota_bytes,
        default_quota_bytes: owner_quota_default_from_env(),
        by_format: by_fmt
            .into_iter()
            .map(|(format, bytes)| PackageUsageByFormat { format, bytes })
            .collect(),
        packages,
    })
}

pub async fn admin_set_quota(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PackagesAdminSetQuotaResponse, AppError> {
    require_sys_admin(ctx).await?;
    let req: PackagesAdminSetQuotaRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid packages.adminSetQuota input: {e}"),
        )
    })?;
    let (owner_type, owner_id) = resolve_owner_login(ctx, &req.owner).await?;
    ctx.db
        .upsert_package_quota_override(&owner_type, &owner_id, req.max_bytes as i64)
        .await
        .map_err(db_err)?;
    Ok(PackagesAdminSetQuotaResponse {
        ok: true,
        max_bytes: req.max_bytes,
    })
}
