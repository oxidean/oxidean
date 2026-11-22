//! Org teams CRUD + membership + per-repo grants (`org.teams.*` — COL-05).
//!
//! Teams are org-scoped groups whose repo grants feed the shared
//! highest-wins Capability ACL. Membership requires org membership; team
//! `maintainer` can manage the team's members/repo grants without org Admin.

use oxidean_core::{
    AppError, OrgRole, OrgSlugRequest, OrgTeamMemberPublic, OrgTeamPublic, OrgTeamRepoPublic,
    OrgTeamSlugRequest, OrgTeamsCreateRequest, OrgTeamsListResponse, OrgTeamsMembersAddRequest,
    OrgTeamsMembersListResponse, OrgTeamsMembersRemoveRequest, OrgTeamsMembersSetRoleRequest,
    OrgTeamsReposAddRequest, OrgTeamsReposListResponse, OrgTeamsReposRemoveRequest,
    OrgTeamsReposUpdateRequest, OrgTeamsUpdateRequest, TeamRole,
};
use oxidean_db::{OrgTeamListRow, OrgTeamRow, OrganizationRow, UserRow};

use crate::members::require_admin_plus;
use oxidean_api_core::audit;
use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::notify;
use oxidean_api_core::org::{db_err, load_org_by_slug, require_org_role};
use oxidean_api_core::rpc::RpcCtx;

/// DB-error mapping for the teams domain — stable reason codes per spec.
fn team_db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new(
            "org.team_slug_taken",
            "a team with that slug already exists",
        )
    } else if e == "org team not found" {
        AppError::new("org.team_not_found", "team not found")
    } else if e == "team member not found" {
        AppError::new("org.team_member_not_found", "team member not found")
    } else if e == "team repo grant not found" {
        AppError::new(
            "org.team_repo_not_found",
            "team has no grant on that repository",
        )
    } else {
        tracing::error!("org teams db error: {e}");
        AppError::new("org.internal", "organization operation failed")
    }
}

/// Team slug: `^[a-z0-9][a-z0-9-]{0,98}[a-z0-9]$|^[a-z0-9]$` (spec §5).
fn valid_team_slug(slug: &str) -> bool {
    let b = slug.as_bytes();
    let ok_char = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-';
    match b.len() {
        1 => b[0].is_ascii_lowercase() || b[0].is_ascii_digit(),
        2..=100 => {
            (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
                && (b[b.len() - 1].is_ascii_lowercase() || b[b.len() - 1].is_ascii_digit())
                && b.iter().all(|c| ok_char(*c))
        }
        _ => false,
    }
}

/// Derive a team slug from a display name (GitHub parity): lowercase, runs of
/// non `[a-z0-9]` become a single `-`, edges trimmed, capped at 100 chars.
fn derive_team_slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = false;
    for c in name.trim().chars() {
        let lc = c.to_ascii_lowercase();
        if lc.is_ascii_lowercase() || lc.is_ascii_digit() {
            out.push(lc);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
        if out.len() >= 100 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    out
}

fn team_public(row: &OrgTeamListRow) -> OrgTeamPublic {
    OrgTeamPublic {
        id: row.id.clone(),
        org_id: row.org_id.clone(),
        slug: row.slug.clone(),
        name: row.name.clone(),
        description: row.description.clone(),
        member_count: row.member_count,
        repo_count: row.repo_count,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

/// `org.teams.get`/`update`/`delete`/`members.*`/`repos.*` resolve a team
/// inside its org by slug — identical `org.team_not_found` for misses.
async fn load_team(
    ctx: &RpcCtx,
    org: &OrganizationRow,
    team_slug: &str,
) -> Result<OrgTeamRow, AppError> {
    let slug = team_slug.trim();
    if slug.is_empty() {
        return Err(AppError::new("rpc.bad_input", "team_slug is required"));
    }
    ctx.db
        .find_org_team_by_slug(&org.id, slug)
        .await
        .map_err(team_db_err)?
        .ok_or_else(|| AppError::new("org.team_not_found", "team not found"))
}

/// Mutating team RPCs: org Owner/Admin OR the team's own `maintainer`.
/// Maintainers manage membership + grants but cannot create/delete teams.
async fn require_team_admin(
    ctx: &RpcCtx,
    org: &OrganizationRow,
    team_id: &str,
    caller: &UserRow,
) -> Result<(), AppError> {
    // Org role gates first: Owner/Admin allowed, Member falls through to the
    // maintainer check. A `require_org_role` *error* (non-member, or 2FA
    // lockout) propagates — maintainers must not bypass the org lockout (#130).
    match require_org_role(ctx, &org.id, &caller.id).await {
        Ok(role) if matches!(role, OrgRole::Owner | OrgRole::Admin) => return Ok(()),
        Ok(_) => {}
        Err(e) => return Err(e),
    }
    let member = ctx
        .db
        .find_team_member(team_id, &caller.id)
        .await
        .map_err(team_db_err)?;
    if member.is_some_and(|m| m.role == TeamRole::Maintainer.as_str()) {
        return Ok(());
    }
    Err(AppError::new(
        "org.forbidden",
        "organization admin or team maintainer access required",
    ))
}

/// `org.teams.list` — any org member; teams with member/repo counts.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgTeamsListResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgSlugRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.list input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_org_role(ctx, &org.id, &caller.id).await?;

    let rows = ctx.db.list_org_teams(&org.id).await.map_err(team_db_err)?;
    let teams = rows.iter().map(team_public).collect();
    Ok(OrgTeamsListResponse { teams })
}

/// `org.teams.get` — any org member; single team with counts.
pub async fn get(ctx: &RpcCtx, input: serde_json::Value) -> Result<OrgTeamPublic, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamSlugRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid org.teams.get input: {e}")))?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_org_role(ctx, &org.id, &caller.id).await?;
    let team = load_team(ctx, &org, &req.team_slug).await?;

    let rows = ctx.db.list_org_teams(&org.id).await.map_err(team_db_err)?;
    let row = rows
        .iter()
        .find(|r| r.id == team.id)
        .ok_or_else(|| AppError::new("org.team_not_found", "team not found"))?;
    Ok(team_public(row))
}

/// `org.teams.create` — Admin+; `team_slug` optional (derived from `name`).
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<OrgTeamPublic, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamsCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.create input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;

    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::new(
            "rpc.bad_input",
            "team name must be 1-100 characters",
        ));
    }
    let description = req.description.unwrap_or_default().trim().to_string();
    if description.chars().count() > 512 {
        return Err(AppError::new(
            "rpc.bad_input",
            "team description must be at most 512 characters",
        ));
    }
    // Explicit slugs normalize lowercase — `OPS-TEAM` means `ops-team`
    // (GitHub parity) and hits the case-insensitive uniqueness check.
    let slug = match req
        .team_slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) => s.to_lowercase(),
        None => derive_team_slug(name),
    };
    if !valid_team_slug(&slug) {
        return Err(AppError::new(
            "rpc.bad_input",
            "team slug must match ^[a-z0-9][a-z0-9-]{0,98}[a-z0-9]$ or ^[a-z0-9]$",
        ));
    }
    if ctx
        .db
        .find_org_team_by_slug(&org.id, &slug)
        .await
        .map_err(team_db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "org.team_slug_taken",
            "a team with that slug already exists",
        ));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_org_team(&id, &org.id, &slug, name, &description)
        .await
        .map_err(team_db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "org.team_create",
        Some(("team", &row.id)),
        Some(format!("created team {slug} in org {}", org.slug)),
    )
    .await;

    Ok(OrgTeamPublic {
        id: row.id,
        org_id: row.org_id,
        slug: row.slug,
        name: row.name,
        description: row.description,
        member_count: 0,
        repo_count: 0,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// `org.teams.update` — Admin+ or maintainer; rename/redesc/reslug.
pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<OrgTeamPublic, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamsUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.update input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let team = load_team(ctx, &org, &req.team_slug).await?;
    require_team_admin(ctx, &org, &team.id, &caller).await?;

    let slug = match req
        .new_slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) => s.to_lowercase(),
        None => team.slug.clone(),
    };
    if !valid_team_slug(&slug) {
        return Err(AppError::new(
            "rpc.bad_input",
            "team slug must match ^[a-z0-9][a-z0-9-]{0,98}[a-z0-9]$ or ^[a-z0-9]$",
        ));
    }
    let name = match req.name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => {
            if n.chars().count() > 100 {
                return Err(AppError::new(
                    "rpc.bad_input",
                    "team name must be 1-100 characters",
                ));
            }
            n.to_string()
        }
        None => team.name.clone(),
    };
    let description = match req.description.as_deref() {
        Some(d) => {
            if d.trim().chars().count() > 512 {
                return Err(AppError::new(
                    "rpc.bad_input",
                    "team description must be at most 512 characters",
                ));
            }
            d.trim().to_string()
        }
        None => team.description.clone(),
    };
    if slug != team.slug {
        if let Some(existing) = ctx
            .db
            .find_org_team_by_slug(&org.id, &slug)
            .await
            .map_err(team_db_err)?
        {
            if existing.id != team.id {
                return Err(AppError::new(
                    "org.team_slug_taken",
                    "a team with that slug already exists",
                ));
            }
        }
    }

    let row = ctx
        .db
        .update_org_team(&team.id, &slug, &name, &description)
        .await
        .map_err(team_db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "org.team_update",
        Some(("team", &row.id)),
        Some(format!("updated team {slug} in org {}", org.slug)),
    )
    .await;

    let rows = ctx.db.list_org_teams(&org.id).await.map_err(team_db_err)?;
    let listed = rows
        .iter()
        .find(|r| r.id == row.id)
        .ok_or_else(|| AppError::new("org.team_not_found", "team not found"))?;
    Ok(team_public(listed))
}

/// `org.teams.delete` — Admin+; cascades members + repo grants.
pub async fn delete(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamSlugRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.delete input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let team = load_team(ctx, &org, &req.team_slug).await?;

    ctx.db
        .delete_org_team(&team.id)
        .await
        .map_err(team_db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "org.team_delete",
        Some(("team", &team.id)),
        Some(format!("deleted team {} in org {}", team.slug, org.slug)),
    )
    .await;

    // The team may have been the sole access source on org repos — re-check
    // every watcher across the org's repos.
    notify::sweep_org_access(&ctx.db, &org.id).await;

    Ok(serde_json::json!({ "ok": true }))
}

/// `org.teams.members.list` — any org member; username + team role.
pub async fn members_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgTeamsMembersListResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamSlugRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.members.list input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_org_role(ctx, &org.id, &caller.id).await?;
    let team = load_team(ctx, &org, &req.team_slug).await?;

    let rows = ctx
        .db
        .list_team_members(&team.id)
        .await
        .map_err(team_db_err)?;
    let mut members = Vec::with_capacity(rows.len());
    for row in rows {
        members.push(OrgTeamMemberPublic {
            user_id: row.user_id,
            username: row.username,
            role: TeamRole::parse(&row.role).map_err(|e| {
                tracing::error!(error = %e, "invalid team role in row");
                AppError::new("org.internal", "organization operation failed")
            })?,
            created_at: row.created_at,
        });
    }
    Ok(OrgTeamsMembersListResponse { members })
}

/// `org.teams.members.add` — Admin+/maintainer; target must be an org member.
pub async fn members_add(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgTeamMemberPublic, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamsMembersAddRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.members.add input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let team = load_team(ctx, &org, &req.team_slug).await?;
    require_team_admin(ctx, &org, &team.id, &caller).await?;

    let role = req.role.unwrap_or(TeamRole::Member);
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

    // Team grants are member-scoped: only org members may join teams.
    if ctx
        .db
        .find_org_member(&org.id, &target.id)
        .await
        .map_err(db_err)?
        .is_none()
    {
        return Err(AppError::new(
            "org.member_not_found",
            "user is not a member of this organization",
        ));
    }
    if ctx
        .db
        .find_team_member(&team.id, &target.id)
        .await
        .map_err(team_db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "org.team_member_exists",
            "user is already a member of this team",
        ));
    }

    let row = ctx
        .db
        .insert_team_member(&team.id, &target.id, role.as_str())
        .await
        .map_err(team_db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "org.team_member_add",
        Some(("team", &team.id)),
        Some(format!("added {username} to team {}", team.slug)),
    )
    .await;

    Ok(OrgTeamMemberPublic {
        user_id: row.user_id,
        username: target.username,
        role,
        created_at: row.created_at,
    })
}

/// `org.teams.members.remove` — Admin+/maintainer.
pub async fn members_remove(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamsMembersRemoveRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.members.remove input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let team = load_team(ctx, &org, &req.team_slug).await?;
    require_team_admin(ctx, &org, &team.id, &caller).await?;

    ctx.db
        .delete_team_member(&team.id, &req.user_id)
        .await
        .map_err(team_db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "org.team_member_remove",
        Some(("team", &team.id)),
        Some(format!("removed {} from team {}", req.user_id, team.slug)),
    )
    .await;

    // Team grants may have been the user's only path to org repos.
    notify::sweep_user_access_on_owner_repos(&ctx.db, &req.user_id, &org.id).await;

    Ok(serde_json::json!({ "ok": true }))
}

/// `org.teams.members.set_role` — Admin+/maintainer; member↔maintainer.
pub async fn members_set_role(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgTeamMemberPublic, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamsMembersSetRoleRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.members.set_role input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let team = load_team(ctx, &org, &req.team_slug).await?;
    require_team_admin(ctx, &org, &team.id, &caller).await?;

    let row = ctx
        .db
        .update_team_member_role(&team.id, &req.user_id, req.role.as_str())
        .await
        .map_err(team_db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "org.team_member_set_role",
        Some(("team", &team.id)),
        Some(format!(
            "set {} team role to {} in {}",
            req.user_id,
            req.role.as_str(),
            team.slug
        )),
    )
    .await;

    let user = ctx
        .db
        .find_user_by_id(&row.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.user_not_found", "user not found"))?;

    Ok(OrgTeamMemberPublic {
        user_id: row.user_id,
        username: user.username,
        role: req.role,
        created_at: row.created_at,
    })
}

/// `org.teams.repos.list` — any org member; grants with repo names.
pub async fn repos_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgTeamsReposListResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamSlugRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.repos.list input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_org_role(ctx, &org.id, &caller.id).await?;
    let team = load_team(ctx, &org, &req.team_slug).await?;

    let rows = ctx
        .db
        .list_team_repos(&team.id)
        .await
        .map_err(team_db_err)?;
    let mut repos = Vec::with_capacity(rows.len());
    for row in rows {
        repos.push(OrgTeamRepoPublic {
            team_id: row.team_id,
            repo_id: row.repo_id,
            repo_name: row.repo_name,
            permission: oxidean_core::CollaboratorPermission::parse(&row.permission).map_err(
                |e| {
                    tracing::error!(error = %e, "invalid team repo permission in row");
                    AppError::new("org.internal", "organization operation failed")
                },
            )?,
            created_at: row.created_at,
        });
    }
    Ok(OrgTeamsReposListResponse { repos })
}

/// Resolve `repo` (name) inside the org — org-owned repos only.
async fn load_org_repo(
    ctx: &RpcCtx,
    org: &OrganizationRow,
    repo_name: &str,
) -> Result<oxidean_db::RepositoryRow, AppError> {
    let name = repo_name.trim();
    if name.is_empty() {
        return Err(AppError::new("rpc.bad_input", "repo is required"));
    }
    let repo = ctx
        .db
        .find_repository_by_owner_name(&org.id, name)
        .await
        .map_err(db_err)?
        .filter(|r| r.deleted_at.is_none())
        .ok_or_else(|| AppError::new("repo.not_found", "Repository not found"))?;
    Ok(repo)
}

/// `org.teams.repos.add` — Admin+/maintainer; upsert grant (PUT parity).
pub async fn repos_add(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgTeamRepoPublic, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamsReposAddRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.repos.add input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let team = load_team(ctx, &org, &req.team_slug).await?;
    require_team_admin(ctx, &org, &team.id, &caller).await?;
    let repo = load_org_repo(ctx, &org, &req.repo).await?;

    let row = ctx
        .db
        .upsert_team_repo(&team.id, &repo.id, req.permission.as_str())
        .await
        .map_err(team_db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "org.team_repo_add",
        Some(("team", &team.id)),
        Some(format!(
            "granted team {} {} on {}/{}",
            team.slug,
            req.permission.as_str(),
            org.slug,
            repo.name
        )),
    )
    .await;

    Ok(OrgTeamRepoPublic {
        team_id: row.team_id,
        repo_id: row.repo_id,
        repo_name: repo.name,
        permission: req.permission,
        created_at: row.created_at,
    })
}

/// `org.teams.repos.update` — Admin+/maintainer; grant must exist.
pub async fn repos_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgTeamRepoPublic, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamsReposUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.repos.update input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let team = load_team(ctx, &org, &req.team_slug).await?;
    require_team_admin(ctx, &org, &team.id, &caller).await?;
    let repo = load_org_repo(ctx, &org, &req.repo).await?;

    let existing = ctx
        .db
        .find_team_repo(&team.id, &repo.id)
        .await
        .map_err(team_db_err)?
        .ok_or_else(|| {
            AppError::new(
                "org.team_repo_not_found",
                "team has no grant on that repository",
            )
        })?;

    let row = ctx
        .db
        .update_team_repo_permission(&team.id, &repo.id, req.permission.as_str())
        .await
        .map_err(team_db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "org.team_repo_update",
        Some(("team", &team.id)),
        Some(format!(
            "set team {} to {} on {}/{}",
            team.slug,
            req.permission.as_str(),
            org.slug,
            repo.name
        )),
    )
    .await;

    // Downgrades may revoke derived access for members.
    let rank = |p: &str| match p {
        "admin" => 3,
        "write" => 2,
        _ => 1,
    };
    if rank(req.permission.as_str()) < rank(&existing.permission) {
        notify::sweep_repo_access(&ctx.db, &repo.id).await;
    }

    Ok(OrgTeamRepoPublic {
        team_id: row.team_id,
        repo_id: row.repo_id,
        repo_name: repo.name,
        permission: req.permission,
        created_at: row.created_at,
    })
}

/// `org.teams.repos.remove` — Admin+/maintainer; grant must exist.
pub async fn repos_remove(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgTeamsReposRemoveRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.teams.repos.remove input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let team = load_team(ctx, &org, &req.team_slug).await?;
    require_team_admin(ctx, &org, &team.id, &caller).await?;
    let repo = load_org_repo(ctx, &org, &req.repo).await?;

    ctx.db
        .delete_team_repo(&team.id, &repo.id)
        .await
        .map_err(team_db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "org.team_repo_remove",
        Some(("team", &team.id)),
        Some(format!(
            "removed team {} from {}/{}",
            team.slug, org.slug, repo.name
        )),
    )
    .await;

    notify::sweep_repo_access(&ctx.db, &repo.id).await;

    Ok(serde_json::json!({ "ok": true }))
}
