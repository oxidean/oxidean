//! `user.lookup` — live username prefix autocomplete (ORG-01 / D-ORG-03 / T-10-03).
//!
//! Optional `context` excludes already-granted users and ranks in-context hits first.

use std::collections::HashSet;

use oxidean_core::{
    AppError, OrgRole, Role, UserLookupContext, UserLookupHit, UserLookupRequest,
    UserLookupResponse,
};
use oxidean_db::users::UserLookupRow;

use crate::auth::gate::require_verified;
use crate::org::{load_org_by_slug, require_org_role};
use crate::repo::resolve_repo_for_admin;
use crate::rpc::RpcCtx;

const LOOKUP_LIMIT: i64 = 10;
const MIN_PREFIX_LEN: usize = 2;
/// Fetch a wider window so we can exclude/rank without under-filling.
const FETCH_LIMIT: i64 = 40;

fn hit_from_row(r: UserLookupRow) -> UserLookupHit {
    UserLookupHit {
        username: r.username,
        display_name: r.display_name,
        avatar_url: r.avatar_path,
    }
}

fn empty() -> UserLookupResponse {
    UserLookupResponse { users: vec![] }
}

fn merge_ranked(
    preferred: Vec<UserLookupHit>,
    rest: Vec<UserLookupHit>,
    exclude: &HashSet<String>,
) -> Vec<UserLookupHit> {
    let mut seen: HashSet<String> = exclude.clone();
    let mut out = Vec::with_capacity(LOOKUP_LIMIT as usize);
    for hit in preferred.into_iter().chain(rest) {
        let key = hit.username.to_ascii_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);
        out.push(hit);
        if out.len() >= LOOKUP_LIMIT as usize {
            break;
        }
    }
    out
}

async fn lookup_global(
    ctx: &RpcCtx,
    prefix: &str,
    exclude: &HashSet<String>,
) -> Result<Vec<UserLookupHit>, AppError> {
    let rows = ctx
        .db
        .list_users_by_username_prefix(prefix, FETCH_LIMIT)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "user.lookup db failed");
            AppError::new("user.lookup_failed", "Could not look up users.")
        })?;
    Ok(merge_ranked(
        Vec::new(),
        rows.into_iter().map(hit_from_row).collect(),
        exclude,
    ))
}

async fn with_org_context(
    ctx: &RpcCtx,
    prefix: &str,
    slug: &str,
) -> Result<UserLookupResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let org = match load_org_by_slug(ctx, slug).await {
        Ok(o) => o,
        Err(_) => return Ok(empty()),
    };
    let role = match require_org_role(ctx, &org.id, &caller.id).await {
        Ok(r) => r,
        Err(_) => return Ok(empty()),
    };
    if !matches!(role, OrgRole::Owner | OrgRole::Admin) {
        return Ok(empty());
    }

    let members = ctx
        .db
        .list_org_members(&org.id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "user.lookup org members failed");
            AppError::new("user.lookup_failed", "Could not look up users.")
        })?;
    let exclude: HashSet<String> = members
        .into_iter()
        .map(|m| m.username.to_ascii_lowercase())
        .collect();

    let users = lookup_global(ctx, prefix, &exclude).await?;
    Ok(UserLookupResponse { users })
}

async fn with_repo_context(
    ctx: &RpcCtx,
    prefix: &str,
    owner: &str,
    name: &str,
) -> Result<UserLookupResponse, AppError> {
    let accessible = match resolve_repo_for_admin(ctx, owner, name).await {
        Ok(a) => a,
        Err(_) => return Ok(empty()),
    };

    let collabs = ctx
        .db
        .list_repo_collaborators(&accessible.row.id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "user.lookup collabs failed");
            AppError::new("user.lookup_failed", "Could not look up users.")
        })?;

    let mut exclude: HashSet<String> = collabs
        .into_iter()
        .map(|c| c.username.to_ascii_lowercase())
        .collect();
    exclude.insert(accessible.owner_username.to_ascii_lowercase());

    // Prefer org members (non-collaborators) when the repo is org-owned.
    let preferred = if accessible.row.owner_type.eq_ignore_ascii_case("org") {
        let members = ctx
            .db
            .list_org_members(&accessible.row.owner_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "user.lookup org members for repo failed");
                AppError::new("user.lookup_failed", "Could not look up users.")
            })?;
        let prefix_lower = prefix.to_ascii_lowercase();
        members
            .into_iter()
            .filter(|m| {
                m.username
                    .to_ascii_lowercase()
                    .starts_with(&prefix_lower)
                    && !exclude.contains(&m.username.to_ascii_lowercase())
            })
            .take(LOOKUP_LIMIT as usize)
            .map(|m| UserLookupHit {
                username: m.username,
                display_name: String::new(),
                avatar_url: None,
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    // Enrich preferred hits with display/avatar when possible.
    let mut preferred_enriched = Vec::with_capacity(preferred.len());
    for hit in preferred {
        if let Ok(Some(user)) = ctx.db.find_user_by_username(&hit.username).await {
            if user.banned_at.is_some() {
                continue;
            }
            preferred_enriched.push(UserLookupHit {
                username: user.username,
                display_name: user.display_name,
                avatar_url: user.avatar_path,
            });
        }
    }

    let global_rows = ctx
        .db
        .list_users_by_username_prefix(prefix, FETCH_LIMIT)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "user.lookup db failed");
            AppError::new("user.lookup_failed", "Could not look up users.")
        })?;
    let rest: Vec<UserLookupHit> = global_rows.into_iter().map(hit_from_row).collect();
    let users = merge_ranked(preferred_enriched, rest, &exclude);
    Ok(UserLookupResponse { users })
}

async fn with_instance_context(
    ctx: &RpcCtx,
    prefix: &str,
) -> Result<UserLookupResponse, AppError> {
    let caller = require_verified(ctx).await?;
    if caller.role != Role::SysAdmin {
        return Ok(empty());
    }
    let users = lookup_global(ctx, prefix, &HashSet::new()).await?;
    Ok(UserLookupResponse { users })
}

/// `user.lookup` — verified session, rate-limited, username prefix only (never email).
pub async fn lookup(ctx: &RpcCtx, input: serde_json::Value) -> Result<UserLookupResponse, AppError> {
    let _caller = require_verified(ctx).await?;

    let session = ctx.session.as_ref().ok_or_else(|| {
        AppError::new("auth.unauthenticated", "Sign in to look up users.")
    })?;

    {
        let mut lim = ctx
            .lookup_limiter
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if lim.check_and_record(&session.session_id).is_err() {
            return Err(AppError::new(
                "user.rate_limited",
                "Too many lookup requests. Try again shortly.",
            ));
        }
    }

    let req: UserLookupRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid user.lookup input: {e}"))
    })?;

    let prefix = req.prefix.trim();

    // Anti-enumeration: short prefix / email-shaped → empty (no DB search).
    if prefix.chars().count() < MIN_PREFIX_LEN || prefix.contains('@') {
        return Ok(empty());
    }

    match req.context {
        None => {
            let users = lookup_global(ctx, prefix, &HashSet::new()).await?;
            Ok(UserLookupResponse { users })
        }
        Some(UserLookupContext::Instance) => with_instance_context(ctx, prefix).await,
        Some(UserLookupContext::Org { slug }) => with_org_context(ctx, prefix, &slug).await,
        Some(UserLookupContext::Repo { owner, name }) => {
            with_repo_context(ctx, prefix, &owner, &name).await
        }
    }
}
