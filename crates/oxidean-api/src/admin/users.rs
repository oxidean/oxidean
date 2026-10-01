//! `admin.users.*` — list, role, sessions, ban/unban, hard delete.

use oxidean_core::{
    AdminSessionPublic, AdminUserAccessOrg, AdminUserAccessRepo, AdminUserActivityItem,
    AdminUserPublic, AdminUsersDeleteRequest, AdminUsersDeleteResponse,
    AdminUsersGetAccessResponse, AdminUsersGetActivityRequest, AdminUsersGetActivityResponse,
    AdminUsersListRequest, AdminUsersListResponse, AdminUsersListSessionsResponse,
    AdminUsersUpdateRoleRequest, AdminUsersUserIdRequest, AppError, CollaboratorPermission,
    OrgRole, Role,
};
use oxidean_db::UserRow;

use crate::admin::{db_err, require_admin_user};
use crate::jobs::delete_under_repos_dir;
use crate::rpc::RpcCtx;

const DEFAULT_LIST_LIMIT: i64 = 50;
const MAX_LIST_LIMIT: i64 = 100;

fn to_admin_public(row: &UserRow) -> AdminUserPublic {
    AdminUserPublic {
        id: row.id.clone(),
        email: row.email.clone(),
        username: row.username.clone(),
        display_name: row.display_name.clone(),
        role: row.role,
        email_verified: row.email_verified_at.is_some(),
        banned_at: row.banned_at.clone(),
        created_at: row.created_at.clone(),
    }
}

fn session_err(e: crate::auth::session::AuthError) -> AppError {
    match e {
        crate::auth::session::AuthError::NotConfigured => AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        ),
        crate::auth::session::AuthError::Store(msg) => {
            tracing::error!("session store error: {msg}");
            AppError::new("auth.session_failed", "session operation failed")
        }
    }
}

async fn load_target(ctx: &RpcCtx, user_id: &str) -> Result<UserRow, AppError> {
    let id = user_id.trim();
    if id.is_empty() {
        return Err(AppError::new("rpc.bad_input", "user_id is required"));
    }
    ctx.db
        .find_user_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("admin.user_not_found", "user not found"))
}

/// Wipe `{repos_dir}/{owner_slug}` when present (path-guarded).
async fn wipe_owner_dir(repos_dir: &std::path::Path, owner_slug: &str) -> Result<(), AppError> {
    let target = repos_dir.join(owner_slug);
    if !tokio::fs::try_exists(&target).await.unwrap_or(false) {
        return Ok(());
    }
    delete_under_repos_dir(repos_dir, &target).await.map_err(|e| {
        tracing::error!(error = %e, owner = %owner_slug, "wipe owner repos dir failed");
        AppError::new(
            "admin.delete_repos_failed",
            "Failed to delete repository files for this account.",
        )
    })
}

/// `admin.users.list` — paginated searchable user list.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<AdminUsersListResponse, AppError> {
    let _admin = require_admin_user(ctx).await?;
    let req: AdminUsersListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.users.list input: {e}"),
        )
    })?;
    let limit = req
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let offset = req.offset.unwrap_or(0).max(0);
    let query = req
        .query
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let (rows, total) = ctx
        .db
        .list_users_page(query, limit, offset)
        .await
        .map_err(db_err)?;
    Ok(AdminUsersListResponse {
        users: rows.iter().map(to_admin_public).collect(),
        total,
    })
}

/// `admin.users.updateRole` — `user` ↔ `sys-admin` only.
pub async fn update_role(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<AdminUserPublic, AppError> {
    let admin = require_admin_user(ctx).await?;
    let req: AdminUsersUpdateRoleRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.users.updateRole input: {e}"),
        )
    })?;

    if !matches!(req.role, Role::User | Role::SysAdmin) {
        return Err(AppError::new(
            "admin.invalid_role",
            "Only user and sys-admin roles can be assigned.",
        ));
    }

    let target = load_target(ctx, &req.user_id).await?;
    if target.role == req.role {
        return Ok(to_admin_public(&target));
    }

    // Self-demotion refused.
    if target.id == admin.id && req.role != Role::SysAdmin {
        return Err(AppError::new(
            "admin.cannot_demote_self",
            "You cannot demote your own sys-admin role.",
        ));
    }

    // Last sys-admin demotion refused.
    if target.role.is_sys_admin() && req.role != Role::SysAdmin {
        let admins = ctx.db.count_sys_admins().await.map_err(db_err)?;
        if admins <= 1 {
            return Err(AppError::new(
                "admin.last_sys_admin",
                "Cannot demote the last system administrator.",
            ));
        }
    }

    let updated = ctx
        .db
        .set_user_role(&target.id, req.role.as_str())
        .await
        .map_err(db_err)?;
    crate::audit::record(
        ctx,
        Some((&admin.id, &admin.username)),
        "admin.user_role_change",
        Some(("user", &target.id)),
        Some(
            serde_json::json!({ "username": target.username, "role": req.role.as_str() })
                .to_string(),
        ),
    )
    .await;
    Ok(to_admin_public(&updated))
}

/// `admin.users.revokeSessions` — force logout all devices.
pub async fn revoke_sessions(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let admin = require_admin_user(ctx).await?;
    let req: AdminUsersUserIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.users.revokeSessions input: {e}"),
        )
    })?;
    let target = load_target(ctx, &req.user_id).await?;
    let n = ctx
        .sessions
        .revoke_all(&ctx.db, &target.id)
        .await
        .map_err(session_err)?;
    crate::audit::record(
        ctx,
        Some((&admin.id, &admin.username)),
        "admin.sessions_revoked",
        Some(("user", &target.id)),
        Some(serde_json::json!({ "revoked": n }).to_string()),
    )
    .await;
    Ok(serde_json::json!({ "ok": true, "revoked": n }))
}

/// `admin.users.listSessions` — session rows for one user (client details; no tokens).
pub async fn list_sessions(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<AdminUsersListSessionsResponse, AppError> {
    let _admin = require_admin_user(ctx).await?;
    let req: AdminUsersUserIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.users.listSessions input: {e}"),
        )
    })?;
    let target = load_target(ctx, &req.user_id).await?;
    let rows = ctx
        .db
        .list_sessions_for_user(&target.id)
        .await
        .map_err(db_err)?;
    Ok(AdminUsersListSessionsResponse {
        sessions: rows
            .iter()
            .map(|s| AdminSessionPublic {
                id: s.id.clone(),
                created_at: s.created_at.clone(),
                last_seen_at: s.last_seen_at.clone(),
                expires_at: s.expires_at.clone(),
                remember_me: s.remember_me,
                ip_address: s.ip_address.clone(),
                user_agent: s.user_agent.clone(),
            })
            .collect(),
    })
}

/// `admin.users.getActivity` — merged feed of audit events + repo activity by actor.
pub async fn get_activity(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<AdminUsersGetActivityResponse, AppError> {
    let _admin = require_admin_user(ctx).await?;
    let req: AdminUsersGetActivityRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.users.getActivity input: {e}"),
        )
    })?;
    let target = load_target(ctx, &req.user_id).await?;
    let limit = req.limit.unwrap_or(100).clamp(1, 200);
    let source = req.source.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let event_type = req
        .event_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(s) = source {
        if !matches!(s, "audit" | "repository") {
            return Err(AppError::new(
                "rpc.bad_input",
                "source must be \"audit\" or \"repository\"",
            ));
        }
    }

    let mut items: Vec<AdminUserActivityItem> = Vec::new();
    let want_audit = source.map(|s| s == "audit").unwrap_or(true);
    let want_repo = source.map(|s| s == "repository").unwrap_or(true);

    let mut event_types: Vec<String> = if want_audit {
        ctx.db
            .list_audit_event_types_for_actor(&target.id)
            .await
            .map_err(db_err)?
    } else {
        Vec::new()
    };

    if want_audit {
        let rows = ctx
            .db
            .list_audit_events_for_actor(&target.id, event_type, limit)
            .await
            .map_err(db_err)?;
        items.extend(rows.iter().map(|e| AdminUserActivityItem {
            id: e.id.clone(),
            source: "audit".into(),
            event_type: e.event_type.clone(),
            created_at: e.created_at.clone(),
            target_type: e.target_type.clone(),
            target_id: e.target_id.clone(),
            detail: e.detail.clone(),
            ip_address: e.ip_address.clone(),
            user_agent: e.user_agent.clone(),
            repo_owner: None,
            repo_name: None,
            ref_name: None,
            commits_count: None,
            commit_message: None,
            pr_number: None,
        }));
    }

    if want_repo {
        let rows = ctx
            .db
            .list_repo_activity_by_actor(&target.id, limit)
            .await
            .map_err(db_err)?;
        for r in rows {
            if let Some(et) = event_type {
                if r.push_type != et {
                    continue;
                }
                if !event_types.iter().any(|t| t == &r.push_type) {
                    event_types.push(r.push_type.clone());
                }
            } else if !event_types.iter().any(|t| t == &r.push_type) {
                event_types.push(r.push_type.clone());
            }
            items.push(AdminUserActivityItem {
                id: r.id.clone(),
                source: "repository".into(),
                event_type: r.push_type.clone(),
                created_at: r.created_at.clone(),
                target_type: None,
                target_id: None,
                detail: None,
                ip_address: None,
                user_agent: None,
                repo_owner: r.repo_owner.clone(),
                repo_name: r.repo_name.clone(),
                ref_name: Some(r.ref_name.clone()),
                commits_count: Some(r.commits_count),
                commit_message: r.commit_message.clone(),
                pr_number: r.pr_number,
            });
        }
    }

    items.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    items.truncate(limit as usize);
    event_types.sort();

    Ok(AdminUsersGetActivityResponse { items, event_types })
}

/// `admin.users.ban` — set banned_at + revoke sessions (PATs gated at resolve).
pub async fn ban(ctx: &RpcCtx, input: serde_json::Value) -> Result<AdminUserPublic, AppError> {
    let admin = require_admin_user(ctx).await?;
    let req: AdminUsersUserIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.users.ban input: {e}"),
        )
    })?;
    let target = load_target(ctx, &req.user_id).await?;

    if target.id == admin.id {
        return Err(AppError::new(
            "admin.cannot_ban_self",
            "You cannot ban your own account.",
        ));
    }
    if target.role.is_sys_admin() {
        return Err(AppError::new(
            "admin.cannot_ban_sys_admin",
            "Demote a system administrator before banning them.",
        ));
    }
    if target.banned_at.is_some() {
        return Ok(to_admin_public(&target));
    }

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let updated = ctx
        .db
        .set_user_banned_at(&target.id, &now)
        .await
        .map_err(db_err)?;
    let _ = ctx
        .sessions
        .revoke_all(&ctx.db, &target.id)
        .await
        .map_err(session_err)?;
    crate::audit::record(
        ctx,
        Some((&admin.id, &admin.username)),
        "admin.user_ban",
        Some(("user", &target.id)),
        Some(serde_json::json!({ "username": target.username }).to_string()),
    )
    .await;
    Ok(to_admin_public(&updated))
}

/// `admin.users.unban` — clear banned_at (PATs become usable again).
pub async fn unban(ctx: &RpcCtx, input: serde_json::Value) -> Result<AdminUserPublic, AppError> {
    let admin = require_admin_user(ctx).await?;
    let req: AdminUsersUserIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.users.unban input: {e}"),
        )
    })?;
    let target = load_target(ctx, &req.user_id).await?;
    if target.banned_at.is_none() {
        return Ok(to_admin_public(&target));
    }
    let updated = ctx
        .db
        .clear_user_banned_at(&target.id)
        .await
        .map_err(db_err)?;
    crate::audit::record(
        ctx,
        Some((&admin.id, &admin.username)),
        "admin.user_unban",
        Some(("user", &target.id)),
        Some(serde_json::json!({ "username": target.username }).to_string()),
    )
    .await;
    Ok(to_admin_public(&updated))
}

/// `admin.users.delete` — hard delete user + personal repos + sole-owner orgs.
pub async fn delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<AdminUsersDeleteResponse, AppError> {
    let admin = require_admin_user(ctx).await?;
    let req: AdminUsersDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.users.delete input: {e}"),
        )
    })?;
    let target = load_target(ctx, &req.user_id).await?;

    if target.id == admin.id {
        return Err(AppError::new(
            "admin.cannot_delete_self",
            "You cannot delete your own account.",
        ));
    }
    if req.confirmation.trim() != target.username {
        return Err(AppError::new(
            "admin.delete_confirm",
            "Type the username to confirm deleting this account.",
        ));
    }
    if target.role.is_sys_admin() {
        let admins = ctx.db.count_sys_admins().await.map_err(db_err)?;
        if admins <= 1 {
            return Err(AppError::new(
                "admin.last_sys_admin",
                "Cannot delete the last system administrator.",
            ));
        }
    }

    // Collect wipe targets while rows still exist, then delete DB first.
    // Disk wipe is best-effort after success so a failed wipe cannot leave
    // ghost repos in the UI (orphan owner dirs are recoverable).
    let personal_repos = ctx
        .db
        .list_repositories_by_owner(&target.id)
        .await
        .map_err(db_err)?;
    let mut deleted_repos = personal_repos.len() as i64;
    let mut wipe_slugs: Vec<String> = Vec::new();
    wipe_slugs.push(target.username.clone());

    let memberships = ctx
        .db
        .list_orgs_for_user(&target.id)
        .await
        .map_err(db_err)?;
    let mut deleted_orgs: i64 = 0;
    let mut sole_owner_orgs: Vec<(String, String)> = Vec::new(); // (id, slug)
    let mut shared_orgs: Vec<String> = Vec::new(); // slugs with other members
    for org in memberships {
        // OrgMineRow.role is org role (`owner` | `admin` | `member` | `read`).
        if org.role != "owner" {
            continue;
        }
        let owners = ctx.db.count_org_owners(&org.id).await.map_err(db_err)?;
        if owners != 1 {
            continue;
        }
        let org_repos = ctx
            .db
            .list_repositories_by_owner(&org.id)
            .await
            .map_err(db_err)?;
        deleted_repos += org_repos.len() as i64;
        sole_owner_orgs.push((org.id.clone(), org.slug.clone()));
        let members = ctx.db.list_org_members(&org.id).await.map_err(db_err)?;
        if members.len() > 1 {
            shared_orgs.push(org.slug.clone());
        }
        wipe_slugs.push(org.slug);
    }

    // Shared orgs would be deleted along with the account — require an
    // explicit opt-in so other members' data is never removed silently.
    if !shared_orgs.is_empty() && req.delete_orgs != Some(true) {
        return Err(AppError::new(
            "admin.delete_orgs_confirm",
            format!(
                "Deleting this account also deletes organization(s) that still have other members: {}. Confirm again with delete_orgs to proceed.",
                shared_orgs.join(", ")
            ),
        ));
    }

    let _ = ctx
        .db
        .hard_delete_repositories_by_owner(&target.id, "user")
        .await
        .map_err(db_err)?;

    for (org_id, _) in &sole_owner_orgs {
        let _ = ctx
            .db
            .hard_delete_repositories_by_owner(org_id, "org")
            .await
            .map_err(db_err)?;
        ctx.db.delete_organization(org_id).await.map_err(db_err)?;
        deleted_orgs += 1;
    }

    // Drop remaining sessions then the user (FK cascades).
    let _ = ctx
        .sessions
        .revoke_all(&ctx.db, &target.id)
        .await
        .map_err(session_err)?;
    // Audit before delete — actor_id survives via ON DELETE SET NULL + username snapshot.
    crate::audit::record(
        ctx,
        Some((&admin.id, &admin.username)),
        "admin.user_delete",
        Some(("user", &target.id)),
        Some(
            serde_json::json!({
                "username": target.username,
                "deleted_repos": deleted_repos,
                "deleted_orgs": deleted_orgs,
            })
            .to_string(),
        ),
    )
    .await;
    ctx.db.delete_user(&target.id).await.map_err(db_err)?;

    for slug in wipe_slugs {
        if let Err(e) = wipe_owner_dir(&ctx.repos_dir, &slug).await {
            tracing::error!(
                error = %e.message,
                owner = %slug,
                "post-delete wipe failed; orphan repo dir may remain"
            );
        }
    }

    Ok(AdminUsersDeleteResponse {
        ok: true,
        deleted_repos,
        deleted_orgs,
    })
}

/// `admin.users.getAccess` — read-only org memberships + repo collaborator grants.
pub async fn get_access(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<AdminUsersGetAccessResponse, AppError> {
    let _admin = require_admin_user(ctx).await?;
    let req: AdminUsersUserIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.users.getAccess input: {e}"),
        )
    })?;
    let target = load_target(ctx, &req.user_id).await?;

    let org_rows = ctx
        .db
        .list_orgs_for_user(&target.id)
        .await
        .map_err(db_err)?;
    let mut orgs = Vec::with_capacity(org_rows.len());
    for row in org_rows {
        let role = OrgRole::parse(&row.role).map_err(|e| {
            tracing::error!(error = %e, "invalid org role in access summary");
            AppError::new("admin.internal", "admin operation failed")
        })?;
        orgs.push(AdminUserAccessOrg {
            slug: row.slug,
            display_name: row.display_name,
            role,
        });
    }

    let grant_rows = ctx
        .db
        .list_repo_collaborator_grants_for_user(&target.id)
        .await
        .map_err(db_err)?;
    let mut repos = Vec::with_capacity(grant_rows.len());
    for row in grant_rows {
        let permission = CollaboratorPermission::parse(&row.permission).map_err(|e| {
            tracing::error!(error = %e, "invalid collab permission in access summary");
            AppError::new("admin.internal", "admin operation failed")
        })?;
        repos.push(AdminUserAccessRepo {
            owner: row.owner_slug,
            name: row.name,
            permission,
        });
    }

    Ok(AdminUsersGetAccessResponse { orgs, repos })
}
