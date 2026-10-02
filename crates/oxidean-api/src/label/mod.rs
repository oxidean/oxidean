//! Label definition RPC — org catalog + repo local/hide (ISS-03 / D-ISS-05 / D-ISS-07).

use std::collections::HashSet;

use oxidean_core::{
    AppError, CreateLabelRequest, DeleteLabelRequest, LabelPublic, LabelScope,
    LabelsListResponse, ListLabelsForRepoRequest, OrgRole, OrgSlugRequest, UpdateLabelRequest,
};
use oxidean_db::LabelRow;
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::org::{load_org_by_slug, require_org_role};
use crate::repo::not_found;
use crate::rpc::RpcCtx;

const NAME_MAX_CHARS: usize = 50;
const DESC_MAX_CHARS: usize = 200;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new("rpc.bad_input", "a label with this name already exists in this scope")
    } else {
        tracing::error!("label db error: {e}");
        AppError::new("label.internal", "label operation failed")
    }
}

fn validate_name(name: &str) -> Result<&str, AppError> {
    let t = name.trim();
    if t.is_empty() {
        return Err(AppError::new("rpc.bad_input", "name is required"));
    }
    if t.chars().count() > NAME_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("name exceeds {NAME_MAX_CHARS} characters"),
        ));
    }
    Ok(t)
}

fn validate_description(description: Option<&str>) -> Result<String, AppError> {
    let stored = description.unwrap_or("").to_string();
    if stored.chars().count() > DESC_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("description exceeds {DESC_MAX_CHARS} characters"),
        ));
    }
    Ok(stored)
}

/// Normalize `#RGB` / `#RRGGBB` / bare hex → lowercase 6-digit hex without `#`.
fn validate_color(color: &str) -> Result<String, AppError> {
    let raw = color.trim().trim_start_matches('#');
    let expanded = if raw.len() == 3 && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        raw.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        raw.to_string()
    };
    if expanded.len() != 6 || !expanded.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::new(
            "rpc.bad_input",
            "color must be a 3- or 6-digit hex value",
        ));
    }
    Ok(expanded.to_ascii_lowercase())
}

fn to_public(row: &LabelRow, hidden: bool) -> LabelPublic {
    let scope = if row.org_id.is_some() {
        LabelScope::Org
    } else {
        LabelScope::Repo
    };
    LabelPublic {
        id: row.id.clone(),
        name: row.name.clone(),
        color: row.color.clone(),
        description: row.description.clone(),
        scope,
        org_id: row.org_id.clone(),
        repo_id: row.repo_id.clone(),
        hidden,
    }
}

fn is_org_admin(role: OrgRole) -> bool {
    matches!(role, OrgRole::Owner | OrgRole::Admin)
}

async fn require_org_admin(ctx: &RpcCtx, slug: &str) -> Result<oxidean_db::OrganizationRow, AppError> {
    let caller = require_verified(ctx).await?;
    let org = load_org_by_slug(ctx, slug).await?;
    let role = require_org_role(ctx, &org.id, &caller.id).await?;
    if !is_org_admin(role) {
        return Err(AppError::new(
            "org.forbidden",
            "organization admin access required",
        ));
    }
    Ok(org)
}

/// Build effective (and optionally hidden) label set for a repo (D-ISS-05).
pub async fn effective_labels_for_repo(
    ctx: &RpcCtx,
    repo_id: &str,
    owner_type: &str,
    owner_id: &str,
    include_hidden: bool,
) -> Result<Vec<LabelPublic>, AppError> {
    let mut hidden_ids: HashSet<String> = ctx
        .db
        .list_hidden_label_ids(repo_id)
        .await
        .map_err(db_err)?
        .into_iter()
        .collect();

    let mut out = Vec::new();

    if owner_type == "org" {
        let org_labels = ctx
            .db
            .list_labels_for_org(owner_id)
            .await
            .map_err(db_err)?;
        for row in org_labels {
            let is_hidden = hidden_ids.remove(&row.id);
            if is_hidden && !include_hidden {
                continue;
            }
            out.push(to_public(&row, is_hidden));
        }
    }

    let repo_labels = ctx
        .db
        .list_labels_for_repo(repo_id)
        .await
        .map_err(db_err)?;
    for row in repo_labels {
        out.push(to_public(&row, false));
    }

    out.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    Ok(out)
}

/// Ids allowed for `issue.labels.set` (non-hidden effective set).
pub async fn effective_label_id_set(
    ctx: &RpcCtx,
    repo_id: &str,
    owner_type: &str,
    owner_id: &str,
) -> Result<HashSet<String>, AppError> {
    let labels = effective_labels_for_repo(ctx, repo_id, owner_type, owner_id, false).await?;
    Ok(labels.into_iter().map(|l| l.id).collect())
}

pub fn label_rows_to_public(rows: &[LabelRow]) -> Vec<LabelPublic> {
    rows.iter().map(|r| to_public(r, false)).collect()
}

/// `label.listForRepo` — Read+; effective set (− hidden) unless Admin `includeHidden`.
pub async fn list_for_repo(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<LabelsListResponse, AppError> {
    let req: ListLabelsForRepoRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid label.listForRepo input: {e}"),
        )
    })?;
    // Labels are shared repo metadata (issues AND pulls) — resolve via the
    // repo acl so they stay reachable when one unit is disabled (COL-13).
    let accessible = crate::repo::resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let want_hidden = req.include_hidden.unwrap_or(false);
    if want_hidden {
        let admin = crate::repo::resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
        let labels = effective_labels_for_repo(
            ctx,
            &admin.row.id,
            &admin.row.owner_type,
            &admin.row.owner_id,
            true,
        )
        .await?;
        return Ok(LabelsListResponse { labels });
    }
    let labels = effective_labels_for_repo(
        ctx,
        &accessible.row.id,
        &accessible.row.owner_type,
        &accessible.row.owner_id,
        false,
    )
    .await?;
    Ok(LabelsListResponse { labels })
}

/// `label.listForOrg` — org member can view catalog; used by org settings.
pub async fn list_for_org(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<LabelsListResponse, AppError> {
    let req: OrgSlugRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid label.listForOrg input: {e}"),
        )
    })?;
    let caller = require_verified(ctx).await?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_org_role(ctx, &org.id, &caller.id).await?;
    let rows = ctx
        .db
        .list_labels_for_org(&org.id)
        .await
        .map_err(db_err)?;
    Ok(LabelsListResponse {
        labels: rows.iter().map(|r| to_public(r, false)).collect(),
    })
}

/// `label.create` — Admin (org or repo) (D-ISS-07 / T-11-02).
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<LabelPublic, AppError> {
    let req: CreateLabelRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid label.create input: {e}"))
    })?;
    let name = validate_name(&req.name)?.to_string();
    let color = validate_color(&req.color)?;
    let description = validate_description(req.description.as_deref())?;
    let id = Uuid::new_v4().to_string();

    match req.scope {
        LabelScope::Org => {
            let org = require_org_admin(ctx, &req.owner).await?;
            let row = ctx
                .db
                .insert_label(&id, &name, &color, &description, Some(&org.id), None)
                .await
                .map_err(db_err)?;
            Ok(to_public(&row, false))
        }
        LabelScope::Repo => {
            let repo_name = req
                .repo
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::new("rpc.bad_input", "repo is required for repo scope"))?;
            let accessible =
                crate::repo::resolve_repo_for_admin(ctx, &req.owner, repo_name).await?;
            let row = ctx
                .db
                .insert_label(
                    &id,
                    &name,
                    &color,
                    &description,
                    None,
                    Some(&accessible.row.id),
                )
                .await
                .map_err(db_err)?;
            Ok(to_public(&row, false))
        }
    }
}

/// `label.update` — Admin; may toggle `hidden` for org labels on a repo.
pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<LabelPublic, AppError> {
    let req: UpdateLabelRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid label.update input: {e}"))
    })?;
    let row = ctx
        .db
        .find_label_by_id(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("label.not_found", "label not found"))?;

    // Hide/unhide path for org labels in a repo context.
    if let Some(hidden) = req.hidden {
        let repo_name = req
            .repo
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::new("rpc.bad_input", "repo is required to hide/unhide a label")
            })?;
        if row.org_id.is_none() {
            return Err(AppError::new(
                "rpc.bad_input",
                "only org labels can be hidden on a repository",
            ));
        }
        let accessible = crate::repo::resolve_repo_for_admin(ctx, &req.owner, repo_name).await?;
        if accessible.row.owner_type != "org"
            || accessible.row.owner_id != row.org_id.as_deref().unwrap_or("")
        {
            return Err(not_found());
        }
        ctx.db
            .set_repo_label_hidden(&accessible.row.id, &row.id, hidden)
            .await
            .map_err(db_err)?;
        // Field updates may also be present; continue if so.
        if req.name.is_none() && req.color.is_none() && req.description.is_none() {
            return Ok(to_public(&row, hidden));
        }
    }

    let name = if let Some(ref n) = req.name {
        validate_name(n)?.to_string()
    } else {
        row.name.clone()
    };
    let color = if let Some(ref c) = req.color {
        validate_color(c)?
    } else {
        row.color.clone()
    };
    let description = if let Some(ref d) = req.description {
        validate_description(Some(d))?
    } else {
        row.description.clone()
    };

    if let Some(org_id) = row.org_id.as_deref() {
        let org = require_org_admin(ctx, &req.owner).await?;
        if org.id != org_id {
            return Err(AppError::new("label.not_found", "label not found"));
        }
    } else {
        let repo_name = req
            .repo
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::new("rpc.bad_input", "repo is required to update a repo label")
            })?;
        let accessible = crate::repo::resolve_repo_for_admin(ctx, &req.owner, repo_name).await?;
        if row.repo_id.as_deref() != Some(accessible.row.id.as_str()) {
            return Err(not_found());
        }
    }

    let updated = ctx
        .db
        .update_label(&row.id, &name, &color, &description)
        .await
        .map_err(db_err)?;
    let hidden = req.hidden.unwrap_or(false);
    Ok(to_public(&updated, hidden))
}

/// `label.delete` — Admin (D-ISS-07).
pub async fn delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let req: DeleteLabelRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid label.delete input: {e}"))
    })?;
    let row = ctx
        .db
        .find_label_by_id(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("label.not_found", "label not found"))?;

    if let Some(org_id) = row.org_id.as_deref() {
        let org = require_org_admin(ctx, &req.owner).await?;
        if org.id != org_id {
            return Err(AppError::new("label.not_found", "label not found"));
        }
    } else {
        let repo_name = req
            .repo
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::new("rpc.bad_input", "repo is required to delete a repo label")
            })?;
        let accessible = crate::repo::resolve_repo_for_admin(ctx, &req.owner, repo_name).await?;
        if row.repo_id.as_deref() != Some(accessible.row.id.as_str()) {
            return Err(not_found());
        }
    }

    ctx.db.delete_label(&row.id).await.map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}
