//! `repo.teams.*` handlers (COL-05) — team access lists on org-owned repos.
//! Admin-gated via Capability ACL (same rule as `repo.collaborators.*`).

use oxidean_api_core::audit;
use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::notify;
use oxidean_api_core::repo::resolve_repo_for_admin;
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{
    AppError, CollaboratorPermission, RepoGetRequest, RepoTeamPublic, RepoTeamsAddRequest,
    RepoTeamsListResponse, RepoTeamsRemoveRequest, RepoTeamsUpdateRequest,
};
use oxidean_db::RepoTeamListRow;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e == "team repo grant not found" {
        AppError::new(
            "org.team_repo_not_found",
            "team has no grant on that repository",
        )
    } else {
        tracing::error!("repo teams db error: {e}");
        AppError::new("repo.internal", "repository operation failed")
    }
}

fn team_public(row: &RepoTeamListRow) -> Result<RepoTeamPublic, AppError> {
    Ok(RepoTeamPublic {
        team_id: row.team_id.clone(),
        slug: row.slug.clone(),
        name: row.name.clone(),
        permission: CollaboratorPermission::parse(&row.permission).map_err(|e| {
            tracing::error!(error = %e, "invalid team permission in row");
            AppError::new("repo.internal", "repository operation failed")
        })?,
        created_at: row.created_at.clone(),
    })
}

/// `repo.teams.list` — repo Admin; user-owned repos return an empty list.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoTeamsListResponse, AppError> {
    let req: RepoGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.teams.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    if !accessible.row.owner_type.eq_ignore_ascii_case("org") {
        return Ok(RepoTeamsListResponse { teams: vec![] });
    }
    let rows = ctx
        .db
        .list_repo_teams(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let mut teams = Vec::with_capacity(rows.len());
    for row in rows {
        teams.push(team_public(&row)?);
    }
    Ok(RepoTeamsListResponse { teams })
}

/// Resolve the owning org + named team for mutate ops — org-owned repos only.
async fn org_and_team(
    ctx: &RpcCtx,
    accessible: &oxidean_api_core::repo::AccessibleRepo,
    team_slug: &str,
) -> Result<(String, oxidean_db::OrgTeamRow), AppError> {
    if !accessible.row.owner_type.eq_ignore_ascii_case("org") {
        return Err(AppError::new(
            "repo.teams.org_only",
            "team access lists are only available on organization-owned repositories",
        ));
    }
    let org_id = accessible.row.owner_id.clone();
    let slug = team_slug.trim();
    if slug.is_empty() {
        return Err(AppError::new("rpc.bad_input", "team_slug is required"));
    }
    // Team must live in the org that owns this repo — cross-org grants are
    // structurally impossible, not just denied.
    let team = ctx
        .db
        .find_org_team_by_slug(&org_id, slug)
        .await
        .map_err(|e| {
            if e == "database not configured" {
                AppError::new(
                    "db.not_configured",
                    "no database configured for this instance",
                )
            } else {
                tracing::error!("repo teams db error: {e}");
                AppError::new("repo.internal", "repository operation failed")
            }
        })?
        .ok_or_else(|| AppError::new("org.team_not_found", "team not found"))?;
    Ok((org_id, team))
}

/// `repo.teams.add` — repo Admin; upsert grant (GitHub PUT parity).
pub async fn add(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoTeamPublic, AppError> {
    let req: RepoTeamsAddRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.teams.add input: {e}"),
        )
    })?;
    let caller = require_verified(ctx).await?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let (_org_id, team) = org_and_team(ctx, &accessible, &req.team_slug).await?;

    let grant = ctx
        .db
        .upsert_team_repo(&team.id, &accessible.row.id, req.permission.as_str())
        .await
        .map_err(db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "repo.team_add",
        Some(("repository", &accessible.row.id)),
        Some(format!(
            "granted team {} {} on {}/{}",
            team.slug,
            req.permission.as_str(),
            req.owner,
            req.name
        )),
    )
    .await;

    Ok(RepoTeamPublic {
        team_id: team.id,
        slug: team.slug,
        name: team.name,
        permission: req.permission,
        created_at: grant.created_at,
    })
}

/// `repo.teams.update` — repo Admin; grant must exist.
pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoTeamPublic, AppError> {
    let req: RepoTeamsUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.teams.update input: {e}"),
        )
    })?;
    let caller = require_verified(ctx).await?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let (_org_id, team) = org_and_team(ctx, &accessible, &req.team_slug).await?;

    let existing = ctx
        .db
        .find_team_repo(&team.id, &accessible.row.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            AppError::new(
                "org.team_repo_not_found",
                "team has no grant on that repository",
            )
        })?;

    ctx.db
        .update_team_repo_permission(&team.id, &accessible.row.id, req.permission.as_str())
        .await
        .map_err(db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "repo.team_update",
        Some(("repository", &accessible.row.id)),
        Some(format!(
            "set team {} to {} on {}/{}",
            team.slug,
            req.permission.as_str(),
            req.owner,
            req.name
        )),
    )
    .await;

    // Downgrade may revoke derived access for team members.
    let rank = |p: &str| match p {
        "admin" => 3,
        "write" => 2,
        _ => 1,
    };
    if rank(req.permission.as_str()) < rank(&existing.permission) {
        notify::sweep_repo_access(&ctx.db, &accessible.row.id).await;
    }

    Ok(RepoTeamPublic {
        team_id: team.id,
        slug: team.slug,
        name: team.name,
        permission: req.permission,
        created_at: existing.created_at,
    })
}

/// `repo.teams.remove` — repo Admin; grant must exist.
pub async fn remove(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let req: RepoTeamsRemoveRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.teams.remove input: {e}"),
        )
    })?;
    let caller = require_verified(ctx).await?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let (_org_id, team) = org_and_team(ctx, &accessible, &req.team_slug).await?;

    ctx.db
        .delete_team_repo(&team.id, &accessible.row.id)
        .await
        .map_err(db_err)?;

    audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "repo.team_remove",
        Some(("repository", &accessible.row.id)),
        Some(format!(
            "removed team {} from {}/{}",
            team.slug, req.owner, req.name
        )),
    )
    .await;

    notify::sweep_repo_access(&ctx.db, &accessible.row.id).await;

    Ok(serde_json::json!({ "ok": true }))
}
