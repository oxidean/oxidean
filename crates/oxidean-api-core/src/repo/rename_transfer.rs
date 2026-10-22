//! Repository rename (+ transfer in 15-04) with bare-disk move and redirects (GIT-16/17).

use chrono::{Duration, Utc};
use oxidean_core::AppError;
use oxidean_db::Database;
use uuid::Uuid;

use super::acl::{lookup_repo_row_or_redirect, meets, AccessibleRepo, Capability};
use crate::rpc::RpcCtx;

/// Default redirect retention (D-REL-08 / RESEARCH).
pub const DEFAULT_REPO_REDIRECT_RETENTION_DAYS: u64 = 90;

pub fn redirect_retention_days() -> u64 {
    std::env::var("OXIDEAN_REPO_REDIRECT_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_REPO_REDIRECT_RETENTION_DAYS)
}

pub fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new(
            "repo.name_taken",
            "A repository with this name already exists. Choose a different name.",
        )
    } else {
        tracing::error!("repo rename/transfer db error: {e}");
        AppError::new("repo.internal", "repository operation failed")
    }
}

/// Resolve for ACL honoring redirects. Live path always wins.
pub async fn resolve_repo_or_redirect(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
    need: Capability,
) -> Result<AccessibleRepo, AppError> {
    use super::acl::effective_capability;
    use super::acl::not_found;

    let pair = match lookup_repo_row_or_redirect(&ctx.db, owner, name).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "lookup_repo_row_or_redirect failed");
            return Err(AppError::new(
                "repo.internal",
                "repository operation failed",
            ));
        }
    };
    let Some((row, owner_ref)) = pair else {
        return Err(not_found());
    };

    let caller_id = ctx.session.as_ref().map(|s| s.user_id.as_str());
    let capability = match effective_capability(&ctx.db, caller_id, &row, &owner_ref).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "effective_capability failed");
            return Err(AppError::new(
                "repo.internal",
                "repository operation failed",
            ));
        }
    };
    if !meets(capability, need) {
        return Err(not_found());
    }
    Ok(AccessibleRepo {
        row,
        owner_username: owner_ref.slug().to_string(),
        capability,
    })
}

pub async fn insert_rename_redirect(
    ctx: &RpcCtx,
    old_owner_slug: &str,
    old_name: &str,
    repo_id: &str,
) -> Result<(), AppError> {
    let days = redirect_retention_days() as i64;
    let expires =
        (Utc::now() + Duration::days(days)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let id = Uuid::new_v4().to_string();
    let _ = ctx
        .db
        .delete_repository_redirect(old_owner_slug, old_name)
        .await;
    ctx.db
        .insert_repository_redirect(&id, old_owner_slug, old_name, repo_id, &expires)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// Delete redirect when a live repo occupies the slug/name (supersede).
pub async fn supersede_redirect_on_create(
    db: &Database,
    owner_slug: &str,
    name: &str,
) -> Result<(), String> {
    db.delete_repository_redirect(owner_slug, name).await
}
