//! Per-repo deploy key RPC (`repo.deployKey.list` / `create` / `delete`) — GIT-23.
//!
//! Deploy keys are transport-only credentials for Git over SSH. They authorize
//! `git-upload-pack` (read) and — when `can_write` — `git-receive-pack` for the
//! one repository they are attached to. They never resolve to an account
//! identity: no session, no RPC, no web access.
//!
//! Fingerprint rules (deliberate):
//! - The same public key may be attached to **multiple** repos (one row each);
//!   `UNIQUE(repo_id, fingerprint)` blocks attaching it twice to one repo.
//! - A fingerprint already registered as an **account** key is rejected, and
//!   `sshKey.add` rejects fingerprints already attached as deploy keys — a key
//!   is either an account credential or a deploy credential, never both, so
//!   read-only scope cannot be silently widened by the account-key path.

use oxidean_core::{
    AppError, DeployKeyCreateRequest, DeployKeyDeleteRequest, DeployKeyListResponse,
    DeployKeyPublic, RepoGetRequest,
};
use uuid::Uuid;

use crate::repo::resolve_repo_for_admin;
use crate::rpc::RpcCtx;
use crate::ssh_keys::parse_accepted_public_key;

const MAX_KEYS_PER_REPO: usize = 50;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE")
        || e.contains("unique")
        || e.contains("Duplicate")
        || e.contains("constraint")
    {
        AppError::new(
            "deployKey.fingerprint_taken",
            "this key is already attached to the repository",
        )
    } else {
        tracing::error!("deploy key db error: {e}");
        AppError::new("repo.internal", "repository operation failed")
    }
}

fn row_to_public(row: &oxidean_db::DeployKeyRow) -> DeployKeyPublic {
    DeployKeyPublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        title: row.title.clone(),
        fingerprint: row.fingerprint.clone(),
        key_type: row.key_type.clone(),
        can_write: row.can_write,
        public_key: Some(row.public_key.clone()),
        last_used_at: row.last_used_at.clone(),
        last_used_ip: row.last_used_ip.clone(),
        created_by: row.created_by.clone(),
        created_at: row.created_at.clone(),
    }
}

/// `repo.deployKey.list` — Admin.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DeployKeyListResponse, AppError> {
    let req: RepoGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.deployKey.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let rows = ctx
        .db
        .list_deploy_keys_for_repo(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(DeployKeyListResponse {
        keys: rows.iter().map(row_to_public).collect(),
    })
}

/// `repo.deployKey.create` — Admin.
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<DeployKeyPublic, AppError> {
    let req: DeployKeyCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.deployKey.create input: {e}"),
        )
    })?;

    if req.title.trim().is_empty() {
        return Err(AppError::new(
            "deployKey.title_required",
            "A title is required for deploy keys",
        ));
    }
    // Reuse the account-key validator (sshKey.invalid_key on bad input).
    let (_key, key_type, fingerprint) = parse_accepted_public_key(&req.public_key)?;
    let public_key_line = req.public_key.trim().to_string();

    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;

    let existing = ctx
        .db
        .list_deploy_keys_for_repo(&accessible.row.id)
        .await
        .map_err(db_err)?;
    if existing.len() >= MAX_KEYS_PER_REPO {
        return Err(AppError::new(
            "deployKey.limit_exceeded",
            format!("maximum of {MAX_KEYS_PER_REPO} deploy keys per repository"),
        ));
    }
    if existing.iter().any(|k| k.fingerprint == fingerprint) {
        return Err(AppError::new(
            "deployKey.fingerprint_taken",
            "this key is already attached to the repository",
        ));
    }
    // A fingerprint that resolves to an account key must not double as a
    // deploy key — the account path would silently widen read-only scope.
    if ctx
        .db
        .find_ssh_key_by_fingerprint(&fingerprint)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "deployKey.fingerprint_taken",
            "this key is already registered as an account SSH key",
        ));
    }

    let creator = ctx
        .session
        .as_ref()
        .map(|s| s.user_id.clone())
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;

    let id = Uuid::new_v4().to_string();
    ctx.db
        .create_deploy_key(
            &id,
            &accessible.row.id,
            req.title.trim(),
            &public_key_line,
            &fingerprint,
            &key_type,
            req.can_write,
            &creator,
        )
        .await
        .map_err(db_err)?;

    let row = ctx
        .db
        .list_deploy_keys_for_repo(&accessible.row.id)
        .await
        .map_err(db_err)?
        .into_iter()
        .find(|k| k.id == id)
        .ok_or_else(|| AppError::new("repo.internal", "created deploy key missing from list"))?;

    Ok(row_to_public(&row))
}

/// `repo.deployKey.delete` — Admin. Hard-delete; unknown id → `deployKey.not_found`.
pub async fn delete(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let req: DeployKeyDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.deployKey.delete input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let id = req.id.trim();
    if id.is_empty() {
        return Err(AppError::new("deployKey.not_found", "Deploy key not found"));
    }
    let removed = ctx
        .db
        .revoke_deploy_key(&accessible.row.id, id)
        .await
        .map_err(db_err)?;
    if !removed {
        return Err(AppError::new("deployKey.not_found", "Deploy key not found"));
    }
    Ok(serde_json::json!({ "ok": true }))
}
