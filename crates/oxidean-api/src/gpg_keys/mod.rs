//! GPG public key RPC (`gpgKey.add` / `list` / `revoke`).

use std::process::Stdio;

use oxidean_core::{AddGpgKeyRequest, AppError, GpgKeyListItem, RevokeGpgKeyRequest};
use tokio::io::AsyncWriteExt;
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
            "gpgKey.fingerprint_taken",
            "a GPG key with this fingerprint is already registered",
        )
    } else {
        tracing::error!("gpg key db error: {e}");
        AppError::new("gpgKey.internal", "GPG key operation failed")
    }
}

fn require_session_user_id(ctx: &RpcCtx) -> Result<&str, AppError> {
    ctx.session
        .as_ref()
        .map(|s| s.user_id.as_str())
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

fn parse_uid_emails_json(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn row_to_list_item(row: &oxidean_db::GpgKeyRow) -> GpgKeyListItem {
    GpgKeyListItem {
        id: row.id.clone(),
        title: row.title.clone(),
        fingerprint: row.fingerprint.clone(),
        key_id: row.key_id.clone(),
        uid_emails: parse_uid_emails_json(&row.uid_emails),
        armored_public_key: Some(row.armored_public_key.clone()),
        created_at: row.created_at.clone(),
    }
}

#[derive(Debug, Default)]
struct ParsedGpgKey {
    fingerprint: String,
    key_id: String,
    uid_emails: Vec<String>,
}

/// Parse an armored public key via `gpg --show-keys --with-colons`.
async fn parse_armored_public_key(armor: &str) -> Result<ParsedGpgKey, AppError> {
    let trimmed = armor.trim();
    if trimmed.is_empty() {
        return Err(AppError::new(
            "gpgKey.invalid_key",
            "armored_public_key must be a non-empty OpenPGP public key",
        ));
    }
    if !trimmed.contains("BEGIN PGP PUBLIC KEY BLOCK") {
        return Err(AppError::new(
            "gpgKey.invalid_key",
            "expected an ASCII-armored PGP public key block",
        ));
    }

    let mut child = tokio::process::Command::new("gpg")
        .args([
            "--batch",
            "--status-fd",
            "2",
            "--with-colons",
            "--show-keys",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            AppError::new(
                "gpgKey.gpg_unavailable",
                format!("gpg is required to register GPG keys ({e})"),
            )
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(trimmed.as_bytes())
            .await
            .map_err(|e| AppError::new("gpgKey.invalid_key", format!("failed to feed gpg: {e}")))?;
        drop(stdin);
    }

    let output = child.wait_with_output().await.map_err(|e| {
        AppError::new(
            "gpgKey.gpg_unavailable",
            format!("gpg show-keys failed: {e}"),
        )
    })?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::new(
            "gpgKey.invalid_key",
            format!("invalid OpenPGP public key ({})", err.lines().next().unwrap_or("gpg error")),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut parsed = ParsedGpgKey::default();
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.is_empty() {
            continue;
        }
        match fields[0] {
            "pub" | "sub" => {
                if fields.len() > 4 && parsed.key_id.is_empty() && fields[0] == "pub" {
                    parsed.key_id = fields[4].to_uppercase();
                }
            }
            "fpr" => {
                if fields.len() > 9 && parsed.fingerprint.is_empty() {
                    parsed.fingerprint = fields[9].to_uppercase();
                }
            }
            "uid" if fields.len() > 9 => {
                let uid = fields[9];
                if let Some(email) = extract_email_from_uid(uid) {
                    if !parsed
                        .uid_emails
                        .iter()
                        .any(|e| e.eq_ignore_ascii_case(&email))
                    {
                        parsed.uid_emails.push(email);
                    }
                }
            }
            _ => {}
        }
    }

    if parsed.fingerprint.is_empty() || parsed.key_id.is_empty() {
        return Err(AppError::new(
            "gpgKey.invalid_key",
            "could not parse fingerprint/key id from OpenPGP public key",
        ));
    }

    // Prefer last 16 hex of fingerprint as display key id when short id missing length.
    if parsed.key_id.len() < 8 {
        let fp = &parsed.fingerprint;
        parsed.key_id = fp[fp.len().saturating_sub(16)..].to_string();
    }

    Ok(parsed)
}

fn extract_email_from_uid(uid: &str) -> Option<String> {
    let start = uid.find('<')?;
    let end = uid[start + 1..].find('>')? + start + 1;
    let email = uid[start + 1..end].trim();
    if email.contains('@') {
        Some(email.to_ascii_lowercase())
    } else {
        None
    }
}

/// Register a GPG public key for the verified session user.
pub async fn add(ctx: &RpcCtx, input: serde_json::Value) -> Result<GpgKeyListItem, AppError> {
    let user = require_verified(ctx).await?;
    let req: AddGpgKeyRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid gpgKey.add input: {e}")))?;

    if req.title.trim().is_empty() {
        return Err(AppError::new(
            "gpgKey.title_required",
            "A title (note) is required for GPG keys",
        ));
    }

    let parsed = parse_armored_public_key(&req.armored_public_key).await?;
    let armor = req.armored_public_key.trim().to_string();
    let uid_json = serde_json::to_string(&parsed.uid_emails).unwrap_or_else(|_| "[]".into());

    let existing = ctx
        .db
        .list_gpg_keys_for_user(&user.id)
        .await
        .map_err(db_err)?;
    if existing.len() >= MAX_KEYS_PER_USER {
        return Err(AppError::new(
            "gpgKey.limit_exceeded",
            format!("maximum of {MAX_KEYS_PER_USER} GPG keys per user"),
        ));
    }

    if ctx
        .db
        .find_gpg_key_by_fingerprint(&parsed.fingerprint)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "gpgKey.fingerprint_taken",
            "a GPG key with this fingerprint is already registered",
        ));
    }

    let id = Uuid::new_v4().to_string();
    ctx.db
        .create_gpg_key(
            &id,
            &user.id,
            req.title.trim(),
            &armor,
            &parsed.fingerprint,
            &parsed.key_id,
            &uid_json,
        )
        .await
        .map_err(db_err)?;

    let row = ctx
        .db
        .list_gpg_keys_for_user(&user.id)
        .await
        .map_err(db_err)?
        .into_iter()
        .find(|k| k.id == id)
        .ok_or_else(|| AppError::new("gpgKey.internal", "created GPG key missing from list"))?;

    Ok(row_to_list_item(&row))
}

/// List GPG keys for the signed-in user.
pub async fn list(ctx: &RpcCtx) -> Result<Vec<GpgKeyListItem>, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let rows = ctx
        .db
        .list_gpg_keys_for_user(user_id)
        .await
        .map_err(db_err)?;
    Ok(rows.iter().map(row_to_list_item).collect())
}

/// Hard-delete a GPG key owned by the signed-in user.
pub async fn revoke(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let req: RevokeGpgKeyRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid gpgKey.revoke input: {e}")))?;
    let id = req.id.trim();
    if id.is_empty() {
        return Err(AppError::new("gpgKey.not_found", "GPG key not found"));
    }

    let owned = ctx
        .db
        .list_gpg_keys_for_user(user_id)
        .await
        .map_err(db_err)?
        .into_iter()
        .any(|k| k.id == id);
    if !owned {
        return Err(AppError::new("gpgKey.not_found", "GPG key not found"));
    }

    ctx.db.revoke_gpg_key(id).await.map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}
