//! Label domain shared layer — effective-label-set computation and row
//! mapping used by `issue` enrichment and `discussion`. The `label.*` RPC
//! handlers live in `oxidean-api-issues` (issue #162).

use std::collections::HashSet;

use oxidean_core::{AppError, LabelPublic, LabelScope};
use oxidean_db::LabelRow;

use crate::rpc::RpcCtx;

pub fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new(
            "rpc.bad_input",
            "a label with this name already exists in this scope",
        )
    } else {
        tracing::error!("label db error: {e}");
        AppError::new("label.internal", "label operation failed")
    }
}

pub fn to_public(row: &LabelRow, hidden: bool) -> LabelPublic {
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
        let org_labels = ctx.db.list_labels_for_org(owner_id).await.map_err(db_err)?;
        for row in org_labels {
            let is_hidden = hidden_ids.remove(&row.id);
            if is_hidden && !include_hidden {
                continue;
            }
            out.push(to_public(&row, is_hidden));
        }
    }

    let repo_labels = ctx.db.list_labels_for_repo(repo_id).await.map_err(db_err)?;
    for row in repo_labels {
        out.push(to_public(&row, false));
    }

    out.sort_by_key(|a| a.name.to_ascii_lowercase());
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
