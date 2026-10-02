//! SSH public key RPC (`sshKey.add` / `list` / `revoke`) — GIT-04 / D-SSH-05.

use oxidean_core::{AddSshKeyRequest, AppError, SshKeyListItem};
use serde::Deserialize;
use ssh_key::{Algorithm, HashAlg, PublicKey};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::rpc::RpcCtx;

const MAX_KEYS_PER_USER: usize = 25;

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
            "sshKey.fingerprint_taken",
            "an SSH key with this fingerprint is already registered",
        )
    } else {
        tracing::error!("ssh key db error: {e}");
        AppError::new("sshKey.internal", "SSH key operation failed")
    }
}

fn require_session_user_id(ctx: &RpcCtx) -> Result<&str, AppError> {
    ctx.session
        .as_ref()
        .map(|s| s.user_id.as_str())
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

fn row_to_list_item(row: &oxidean_db::SshKeyRow) -> SshKeyListItem {
    SshKeyListItem {
        id: row.id.clone(),
        title: row.title.clone(),
        fingerprint: row.fingerprint.clone(),
        key_type: row.key_type.clone(),
        can_authenticate: row.can_authenticate,
        can_sign: row.can_sign,
        public_key: Some(row.public_key.clone()),
        last_used_at: row.last_used_at.clone(),
        last_used_ip: row.last_used_ip.clone(),
        created_at: row.created_at.clone(),
    }
}

/// Normalize OpenSSH line and validate algorithm / RSA size (D-SSH-05).
/// Shared with `deploy_keys` — error codes are `sshKey.*` by design.
pub(crate) fn parse_accepted_public_key(line: &str) -> Result<(PublicKey, String, String), AppError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(AppError::new(
            "sshKey.invalid_key",
            "public_key must be a non-empty OpenSSH public key line",
        ));
    }
    let key = PublicKey::from_openssh(trimmed).map_err(|e| {
        AppError::new(
            "sshKey.invalid_key",
            format!("invalid OpenSSH public key: {e}"),
        )
    })?;

    let key_type = match key.algorithm() {
        Algorithm::Ed25519 => "ssh-ed25519".to_string(),
        Algorithm::Rsa { hash: _ } => {
            // ssh-key 0.7: RSA key length via encoded size / key data.
            let bits = rsa_bits(&key).ok_or_else(|| {
                AppError::new(
                    "sshKey.invalid_key",
                    "could not determine RSA key size",
                )
            })?;
            if bits < 2048 {
                return Err(AppError::new(
                    "sshKey.invalid_key",
                    "RSA keys must be at least 2048 bits",
                ));
            }
            "ssh-rsa".to_string()
        }
        other => {
            return Err(AppError::new(
                "sshKey.invalid_key",
                format!("unsupported key type {other}; accept ssh-ed25519 or RSA ≥2048"),
            ));
        }
    };

    let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
    // Persist the trimmed original line (comment preserved).
    Ok((key, key_type, fingerprint))
}

fn rsa_bits(key: &PublicKey) -> Option<usize> {
    // Encode without comment and estimate from SSH wire blob when possible.
    // Prefer reading modulus length from the decoded key if available.
    match key.key_data() {
        ssh_key::public::KeyData::Rsa(rsa) => {
            // n is the modulus as MPInt
            let n = rsa.n().as_bytes();
            // strip leading zero if present (sign bit)
            let len = if n.first() == Some(&0) {
                n.len().saturating_sub(1)
            } else {
                n.len()
            };
            Some(len * 8)
        }
        _ => None,
    }
}

/// Register an SSH public key for the verified session user (D-SSH-05).
pub async fn add(ctx: &RpcCtx, input: serde_json::Value) -> Result<SshKeyListItem, AppError> {
    let user = require_verified(ctx).await?;
    let req: AddSshKeyRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid sshKey.add input: {e}"),
        )
    })?;

    if req.title.trim().is_empty() {
        return Err(AppError::new(
            "sshKey.title_required",
            "A title (note) is required for SSH keys",
        ));
    }

    if !req.can_authenticate && !req.can_sign {
        return Err(AppError::new(
            "sshKey.usage_required",
            "SSH key must allow authentication and/or commit signing",
        ));
    }

    let (_key, key_type, fingerprint) = parse_accepted_public_key(&req.public_key)?;
    let public_key_line = req.public_key.trim().to_string();

    let existing = ctx
        .db
        .list_ssh_keys_for_user(&user.id)
        .await
        .map_err(db_err)?;
    if existing.len() >= MAX_KEYS_PER_USER {
        return Err(AppError::new(
            "sshKey.limit_exceeded",
            format!("maximum of {MAX_KEYS_PER_USER} SSH keys per user"),
        ));
    }

    if ctx
        .db
        .find_ssh_key_by_fingerprint(&fingerprint)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "sshKey.fingerprint_taken",
            "an SSH key with this fingerprint is already registered",
        ));
    }
    // Deploy keys are a distinct credential class (GIT-23): a fingerprint
    // attached as a repo deploy key must not also resolve to an account —
    // the account path wins at SSH auth and would silently widen the scope
    // a repo admin set to read-only.
    if ctx
        .db
        .find_deploy_key_by_fingerprint(&fingerprint)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "sshKey.fingerprint_taken",
            "an SSH key with this fingerprint is already registered",
        ));
    }

    let id = Uuid::new_v4().to_string();
    ctx.db
        .create_ssh_key(
            &id,
            &user.id,
            req.title.trim(),
            &public_key_line,
            &fingerprint,
            &key_type,
            req.can_authenticate,
            req.can_sign,
        )
        .await
        .map_err(db_err)?;

    let row = ctx
        .db
        .list_ssh_keys_for_user(&user.id)
        .await
        .map_err(db_err)?
        .into_iter()
        .find(|k| k.id == id)
        .ok_or_else(|| AppError::new("sshKey.internal", "created SSH key missing from list"))?;

    Ok(row_to_list_item(&row))
}

/// List SSH keys for the signed-in user (session required; verify not required).
pub async fn list(ctx: &RpcCtx) -> Result<Vec<SshKeyListItem>, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let rows = ctx
        .db
        .list_ssh_keys_for_user(user_id)
        .await
        .map_err(db_err)?;
    Ok(rows.iter().map(row_to_list_item).collect())
}

#[derive(Debug, Deserialize)]
struct RevokeSshKeyRequest {
    id: String,
}

/// Hard-delete an SSH key owned by the signed-in user.
pub async fn revoke(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let req: RevokeSshKeyRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid sshKey.revoke input: {e}"),
        )
    })?;
    let id = req.id.trim();
    if id.is_empty() {
        return Err(AppError::new("sshKey.not_found", "SSH key not found"));
    }

    let owned = ctx
        .db
        .list_ssh_keys_for_user(user_id)
        .await
        .map_err(db_err)?
        .into_iter()
        .any(|k| k.id == id);
    if !owned {
        return Err(AppError::new("sshKey.not_found", "SSH key not found"));
    }

    ctx.db.revoke_ssh_key(id).await.map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}
