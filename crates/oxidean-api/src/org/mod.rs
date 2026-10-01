//! Organization RPC handlers (ORG-01 / ORG-02 / D-ORG-01 / D-ORG-02a / D-ORG-02b).

mod invites;
mod members;

pub use invites::{
    accept as invites_accept, create as invites_create, create_link as invites_create_link,
    list as invites_list, revoke as invites_revoke,
};
pub use members::{add as members_add, list as members_list, remove as members_remove, update_role as members_update_role};

use oxidean_core::{
    is_reserved_username, validate_username, AppError, CreateOrgRequest, MemberBasePermission,
    OrgListMineResponse, OrgMemberPublic, OrgMineEntry, OrgPublic, OrgRole, OrgSlugRequest,
};
use oxidean_db::{OrgMemberListRow, OrganizationRow};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::rpc::RpcCtx;

pub(crate) fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new(
            "org.slug_taken",
            "That slug is already used by a user or organization. Choose a different slug.",
        )
    } else if e == "org member not found" {
        AppError::new("org.member_not_found", "member not found")
    } else if e == "organization not found" {
        AppError::new("org.not_found", "organization not found")
    } else {
        tracing::error!("org db error: {e}");
        AppError::new("org.internal", "organization operation failed")
    }
}

fn map_slug_err(msg: String) -> AppError {
    if msg.contains("reserved") {
        AppError::new("auth.reserved_username", "username is reserved")
    } else {
        AppError::new("auth.invalid_username", msg)
    }
}

pub(crate) fn to_public(row: &OrganizationRow) -> Result<OrgPublic, AppError> {
    let member_base_permission = MemberBasePermission::parse(&row.member_base_permission)
        .map_err(|e| {
            tracing::error!(error = %e, "invalid member_base_permission in org row");
            AppError::new("org.internal", "organization operation failed")
        })?;
    Ok(OrgPublic {
        id: row.id.clone(),
        slug: row.slug.clone(),
        display_name: row.display_name.clone(),
        member_base_permission,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    })
}

pub(crate) fn member_public(row: &OrgMemberListRow) -> Result<OrgMemberPublic, AppError> {
    let role = OrgRole::parse(&row.role).map_err(|e| {
        tracing::error!(error = %e, "invalid org member role in row");
        AppError::new("org.internal", "organization operation failed")
    })?;
    Ok(OrgMemberPublic {
        user_id: row.user_id.clone(),
        username: row.username.clone(),
        role,
        created_at: row.created_at.clone(),
    })
}

pub(crate) async fn load_org_by_slug(
    ctx: &RpcCtx,
    slug: &str,
) -> Result<OrganizationRow, AppError> {
    let slug = slug.trim();
    if slug.is_empty() {
        return Err(AppError::new("rpc.bad_input", "slug is required"));
    }
    ctx.db
        .find_organization_by_slug(slug)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.not_found", "organization not found"))
}

pub(crate) async fn require_org_role(
    ctx: &RpcCtx,
    org_id: &str,
    user_id: &str,
) -> Result<OrgRole, AppError> {
    let role = ctx
        .db
        .find_org_member_role(org_id, user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.forbidden", "not a member of this organization"))?;
    OrgRole::parse(&role).map_err(|e| {
        tracing::error!(error = %e, "invalid org role in membership");
        AppError::new("org.internal", "organization operation failed")
    })
}

/// `org.create` — verified user creates an org and becomes Owner (ORG-01 / D-ORG-01 / D-ORG-02a).
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<OrgPublic, AppError> {
    let user = require_verified(ctx).await?;

    let req: CreateOrgRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid org.create input: {e}"))
    })?;

    validate_username(&req.slug).map_err(map_slug_err)?;
    let slug = req.slug.trim().to_string();

    if is_reserved_username(&slug) {
        return Err(AppError::new(
            "auth.reserved_username",
            "username is reserved",
        ));
    }

    let display_name = req
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(slug.as_str())
        .to_string();

    // D-ORG-01 / T-10-04: shared namespace — reject collisions with users or orgs.
    if crate::repo::login_slug_taken(&ctx.db, &slug)
        .await
        .map_err(db_err)?
    {
        return Err(AppError::new(
            "org.slug_taken",
            "That slug is already used by a user or organization. Choose a different slug.",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_organization(
            &id,
            &slug,
            &display_name,
            MemberBasePermission::None.as_str(),
        )
        .await
        .map_err(db_err)?;

    // Creator is sole Owner (ORG-01 / D-ORG-02a / T-10-05).
    ctx.db
        .insert_org_owner_membership(&row.id, &user.id)
        .await
        .map_err(db_err)?;

    to_public(&row)
}

/// `org.get` — public org profile by slug (anonymous OK; mirrors `user.getPublicProfile`).
pub async fn get(ctx: &RpcCtx, input: serde_json::Value) -> Result<OrgPublic, AppError> {
    let req: OrgSlugRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid org.get input: {e}")))?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    to_public(&org)
}

/// `org.listMine` — orgs the caller belongs to, with role (UI picker / overview).
pub async fn list_mine(ctx: &RpcCtx) -> Result<OrgListMineResponse, AppError> {
    let user = require_verified(ctx).await?;
    let rows = ctx.db.list_orgs_for_user(&user.id).await.map_err(db_err)?;
    let mut orgs = Vec::with_capacity(rows.len());
    for row in rows {
        let member_base_permission = MemberBasePermission::parse(&row.member_base_permission)
            .map_err(|e| {
                tracing::error!(error = %e, "invalid member_base_permission in org mine row");
                AppError::new("org.internal", "organization operation failed")
            })?;
        let role = OrgRole::parse(&row.role).map_err(|e| {
            tracing::error!(error = %e, "invalid org role in org mine row");
            AppError::new("org.internal", "organization operation failed")
        })?;
        orgs.push(OrgMineEntry {
            id: row.id,
            slug: row.slug,
            display_name: row.display_name,
            member_base_permission,
            role,
            created_at: row.created_at,
            updated_at: row.updated_at,
        });
    }
    Ok(OrgListMineResponse { orgs })
}

/// `org.updateSettings` — Admin+ (D-ORG-02b).
pub async fn update_settings(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgPublic, AppError> {
    use oxidean_core::OrgUpdateSettingsRequest;

    let caller = require_verified(ctx).await?;
    let req: OrgUpdateSettingsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.updateSettings input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let role = require_org_role(ctx, &org.id, &caller.id).await?;
    if !matches!(role, OrgRole::Owner | OrgRole::Admin) {
        return Err(AppError::new(
            "org.forbidden",
            "organization admin access required",
        ));
    }

    let base = req.member_base_permission.map(|p| p.as_str().to_string());
    let display = req
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    if base.is_none() && display.is_none() {
        return to_public(&org);
    }

    let row = ctx
        .db
        .update_organization_settings(&org.id, base.as_deref(), display.as_deref())
        .await
        .map_err(db_err)?;
    to_public(&row)
}