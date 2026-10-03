//! Org membership CRUD (`org.members.*` — ORG-01 / ORG-02 / D-ORG-02a / T-10-09 / T-10-10).

use oxidean_core::{
    AppError, OrgMemberPublic, OrgMembersAddRequest, OrgMembersListResponse,
    OrgMembersRemoveRequest, OrgMembersUpdateRoleRequest, OrgRole, OrgSlugRequest,
};
use oxidean_db::{OrganizationRow, UserRow};

use crate::auth::gate::require_verified;
use crate::notify;
use crate::org::{db_err, load_org_by_slug, member_public, require_org_role};
use crate::rpc::RpcCtx;

fn is_admin_plus(role: OrgRole) -> bool {
    matches!(role, OrgRole::Owner | OrgRole::Admin)
}

/// Caller must be Admin or Owner.
async fn require_admin_plus(
    ctx: &RpcCtx,
    org: &OrganizationRow,
    caller: &UserRow,
) -> Result<OrgRole, AppError> {
    let role = require_org_role(ctx, &org.id, &caller.id).await?;
    if !is_admin_plus(role) {
        return Err(AppError::new(
            "org.forbidden",
            "organization admin access required",
        ));
    }
    Ok(role)
}

/// Last-owner guard: demoting/removing the sole Owner → `org.last_owner`.
async fn guard_last_owner(
    ctx: &RpcCtx,
    org_id: &str,
    _target_user_id: &str,
    target_was_owner: bool,
) -> Result<(), AppError> {
    if !target_was_owner {
        return Ok(());
    }
    let owners = ctx.db.count_org_owners(org_id).await.map_err(db_err)?;
    if owners <= 1 {
        return Err(AppError::new(
            "org.last_owner",
            "cannot demote or remove the last organization owner",
        ));
    }
    Ok(())
}

/// Only Owners may grant the Owner role (T-10-09).
fn ensure_can_grant_role(caller_role: OrgRole, new_role: OrgRole) -> Result<(), AppError> {
    if new_role == OrgRole::Owner && caller_role != OrgRole::Owner {
        return Err(AppError::new(
            "org.forbidden",
            "only an organization owner can grant the owner role",
        ));
    }
    Ok(())
}

/// `org.members.list` — any org member; usernames + roles, no emails.
pub async fn list(ctx: &RpcCtx, input: serde_json::Value) -> Result<OrgMembersListResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgSlugRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid org.members.list input: {e}"))
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_org_role(ctx, &org.id, &caller.id).await?;

    let rows = ctx.db.list_org_members(&org.id).await.map_err(db_err)?;
    let mut members = Vec::with_capacity(rows.len());
    for row in rows {
        members.push(member_public(&row)?);
    }
    Ok(OrgMembersListResponse { members })
}

/// `org.members.add` — Admin+; Owner role grants Owner-only; works with allow_signup false.
pub async fn add(ctx: &RpcCtx, input: serde_json::Value) -> Result<OrgMemberPublic, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgMembersAddRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid org.members.add input: {e}"))
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let caller_role = require_admin_plus(ctx, &org, &caller).await?;
    ensure_can_grant_role(caller_role, req.role)?;

    let username = req.username.trim();
    if username.is_empty() {
        return Err(AppError::new("rpc.bad_input", "username is required"));
    }
    let target = ctx
        .db
        .find_user_by_username(username)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.user_not_found", "user not found"))?;

    if ctx
        .db
        .find_org_member(&org.id, &target.id)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "org.already_member",
            "user is already a member of this organization",
        ));
    }

    let row = ctx
        .db
        .insert_org_member(&org.id, &target.id, req.role.as_str())
        .await
        .map_err(db_err)?;

    Ok(OrgMemberPublic {
        user_id: row.user_id,
        username: target.username,
        role: req.role,
        created_at: row.created_at,
    })
}

/// `org.members.updateRole` — Admin+; Owner grants; last Owner protected.
pub async fn update_role(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgMemberPublic, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgMembersUpdateRoleRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.members.updateRole input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let caller_role = require_admin_plus(ctx, &org, &caller).await?;
    ensure_can_grant_role(caller_role, req.role)?;

    let existing = ctx
        .db
        .find_org_member(&org.id, &req.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.member_not_found", "member not found"))?;
    let was_owner = existing.role == OrgRole::Owner.as_str();

    if was_owner && req.role != OrgRole::Owner {
        guard_last_owner(ctx, &org.id, &req.user_id, true).await?;
    }

    // Admin cannot demote/change another Owner (T-10-09).
    if was_owner && caller_role != OrgRole::Owner {
        return Err(AppError::new(
            "org.forbidden",
            "only an organization owner can change an owner membership",
        ));
    }

    let row = ctx
        .db
        .update_org_member_role(&org.id, &req.user_id, req.role.as_str())
        .await
        .map_err(db_err)?;

    // DEBT-06: demotion to Member drops Owner/Admin grants — the user may no
    // longer read private org repos (member_base decides). Re-check each.
    if req.role == OrgRole::Member {
        notify::sweep_user_access_on_owner_repos(&ctx.db, &req.user_id, &org.id).await;
    }

    let user = ctx
        .db
        .find_user_by_id(&row.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.user_not_found", "user not found"))?;

    Ok(OrgMemberPublic {
        user_id: row.user_id,
        username: user.username,
        role: req.role,
        created_at: row.created_at,
    })
}

/// `org.members.remove` — Admin+; last Owner protected.
pub async fn remove(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgMembersRemoveRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.members.remove input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let caller_role = require_admin_plus(ctx, &org, &caller).await?;

    let existing = ctx
        .db
        .find_org_member(&org.id, &req.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.member_not_found", "member not found"))?;
    let was_owner = existing.role == OrgRole::Owner.as_str();

    if was_owner && caller_role != OrgRole::Owner {
        return Err(AppError::new(
            "org.forbidden",
            "only an organization owner can remove an owner",
        ));
    }
    guard_last_owner(ctx, &org.id, &req.user_id, was_owner).await?;

    ctx.db
        .remove_org_member(&org.id, &req.user_id)
        .await
        .map_err(db_err)?;

    // DEBT-06: membership removal drops org role grants — re-check read on
    // every org repo and auto-unwatch where access is gone.
    notify::sweep_user_access_on_owner_repos(&ctx.db, &req.user_id, &org.id).await;

    Ok(serde_json::json!({ "ok": true }))
}
