//! Organization RPC handlers (ORG-01 / ORG-02 / D-ORG-01 / D-ORG-02a / D-ORG-02b).

use oxidean_core::{AppError, MemberBasePermission, OrgPublic, OrgRole};
use oxidean_db::OrganizationRow;

use crate::rpc::RpcCtx;

pub fn db_err(e: String) -> AppError {
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

pub fn to_public(row: &OrganizationRow) -> Result<OrgPublic, AppError> {
    let member_base_permission =
        MemberBasePermission::parse(&row.member_base_permission).map_err(|e| {
            tracing::error!(error = %e, "invalid member_base_permission in org row");
            AppError::new("org.internal", "organization operation failed")
        })?;
    Ok(OrgPublic {
        id: row.id.clone(),
        slug: row.slug.clone(),
        display_name: row.display_name.clone(),
        member_base_permission,
        require_two_factor: row.require_two_factor,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    })
}

pub async fn load_org_by_slug(ctx: &RpcCtx, slug: &str) -> Result<OrganizationRow, AppError> {
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

pub async fn require_org_role(
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

    // Org 2FA membership policy (issue #130): when the org requires a second
    // factor, members without one lose org-scoped access until they enroll —
    // mirrors GitHub's "enable 2FA to continue" lockout. The user can still
    // reach `auth.two_factor.*` (not org-scoped) to enroll.
    let required = ctx
        .db
        .find_organization_by_id(org_id)
        .await
        .map_err(db_err)?
        .map(|o| o.require_two_factor)
        .unwrap_or(false);
    if required
        && !ctx
            .db
            .user_has_confirmed_factor(user_id)
            .await
            .map_err(db_err)?
    {
        return Err(AppError::new(
            "org.two_factor_required",
            "This organization requires two-factor authentication. Enable it in your account security settings to continue.",
        ));
    }

    OrgRole::parse(&role).map_err(|e| {
        tracing::error!(error = %e, "invalid org role in membership");
        AppError::new("org.internal", "organization operation failed")
    })
}
