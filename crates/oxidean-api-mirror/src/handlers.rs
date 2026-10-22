//! `repo.mirror.*` RPC handlers — extracted from `core::mirror::rpc` (#162).
//! `upsert`/`sync_now` stay in core: `oxidean-api-repo`'s lifecycle calls them
//! as library functions on repo settings changes.

use oxidean_api_core::actions::secrets::encrypt_secret;
use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::mirror::rpc::db_err;
use oxidean_api_core::mirror::{encrypt_mirror_secret, generate_webhook_secret};
use oxidean_api_core::repo::resolve_repo_for_admin;
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{
    AppError, RepoMirrorDeleteRequest, RepoMirrorDeleteResponse, RepoMirrorFetchHostKeyRequest,
    RepoMirrorFetchHostKeyResponse, RepoMirrorGenerateSshKeyRequest,
    RepoMirrorGenerateSshKeyResponse, RepoMirrorGetRequest, RepoMirrorGetResponse,
    RepoMirrorRotateWebhookSecretRequest, RepoMirrorRotateWebhookSecretResponse,
};
use oxidean_git::ssh_host_from_remote_url;

pub async fn get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoMirrorGetResponse, AppError> {
    let req: RepoMirrorGetRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_request", e.to_string()))?;
    let _user = require_verified(ctx).await?;
    let repo = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let mirror = ctx
        .db
        .get_mirror_by_repo(&repo.row.id)
        .await
        .map_err(db_err)?;
    let mirror = match mirror {
        Some(m) => Some(
            oxidean_api_core::mirror::rpc::to_public(ctx, &req.owner, &req.name, &m, None).await?,
        ),
        None => None,
    };
    Ok(RepoMirrorGetResponse { mirror })
}

pub async fn delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoMirrorDeleteResponse, AppError> {
    let req: RepoMirrorDeleteRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_request", e.to_string()))?;
    let _user = require_verified(ctx).await?;
    let repo = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let deleted = ctx
        .db
        .delete_mirror_by_repo(&repo.row.id)
        .await
        .map_err(db_err)?;
    Ok(RepoMirrorDeleteResponse { deleted })
}

pub async fn generate_ssh_key(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoMirrorGenerateSshKeyResponse, AppError> {
    let req: RepoMirrorGenerateSshKeyRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_request", e.to_string()))?;
    let _user = require_verified(ctx).await?;
    let repo = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let existing = ctx
        .db
        .get_mirror_by_repo(&repo.row.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            AppError::new(
                "repo.mirror.not_found",
                "Configure the mirror remote URL first, then generate an SSH key",
            )
        })?;

    let tmp = tempfile::tempdir().map_err(|e| AppError::new("repo.mirror.ssh", e.to_string()))?;
    let key_path = tmp.path().join("id_ed25519");
    let key_s = key_path.to_str().unwrap_or("");
    let out = tokio::process::Command::new("ssh-keygen")
        .args([
            "-t",
            "ed25519",
            "-N",
            "",
            "-C",
            "oxidean-mirror",
            "-f",
            key_s,
        ])
        .output()
        .await
        .map_err(|e| AppError::new("repo.mirror.ssh", e.to_string()))?;
    if !out.status.success() {
        return Err(AppError::new(
            "repo.mirror.ssh",
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    let private = tokio::fs::read_to_string(&key_path)
        .await
        .map_err(|e| AppError::new("repo.mirror.ssh", e.to_string()))?;
    let public = tokio::fs::read_to_string(format!("{key_s}.pub"))
        .await
        .map_err(|e| AppError::new("repo.mirror.ssh", e.to_string()))?
        .trim()
        .to_string();
    let secret_ct =
        encrypt_mirror_secret(&private).map_err(|e| AppError::new("repo.mirror.secret", e))?;

    let row = ctx
        .db
        .upsert_mirror(
            &existing.id,
            &repo.row.id,
            &existing.remote_url,
            "ssh_key",
            &existing.username,
            &secret_ct,
            &public,
            &existing.known_hosts,
            "",
            existing.poll_interval_secs,
            existing.enabled,
            if existing.sync_mode == "exact" {
                "exact"
            } else {
                "merge"
            },
            false,
        )
        .await
        .map_err(db_err)?;

    Ok(RepoMirrorGenerateSshKeyResponse {
        ssh_public_key: public,
        mirror: oxidean_api_core::mirror::rpc::to_public(ctx, &req.owner, &req.name, &row, None)
            .await?,
    })
}

pub async fn rotate_webhook_secret(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoMirrorRotateWebhookSecretResponse, AppError> {
    let req: RepoMirrorRotateWebhookSecretRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_request", e.to_string()))?;
    let _user = require_verified(ctx).await?;
    let repo = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let existing = ctx
        .db
        .get_mirror_by_repo(&repo.row.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("repo.mirror.not_found", "No mirror configured"))?;
    let plain = generate_webhook_secret();
    let ct = encrypt_secret(&plain).map_err(|e| AppError::new("repo.mirror.secret", e))?;
    ctx.db
        .set_mirror_webhook_secret(&existing.id, &ct)
        .await
        .map_err(db_err)?;
    let row = ctx
        .db
        .get_mirror_by_repo(&repo.row.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("repo.mirror.not_found", "No mirror configured"))?;
    Ok(RepoMirrorRotateWebhookSecretResponse {
        webhook_secret: plain.clone(),
        mirror: oxidean_api_core::mirror::rpc::to_public(
            ctx,
            &req.owner,
            &req.name,
            &row,
            Some(plain),
        )
        .await?,
    })
}

pub async fn fetch_host_key(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoMirrorFetchHostKeyResponse, AppError> {
    let req: RepoMirrorFetchHostKeyRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_request", e.to_string()))?;
    let _user = require_verified(ctx).await?;
    let _repo = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let host = ssh_host_from_remote_url(&req.remote_url)
        .map_err(|e| AppError::new("repo.mirror.invalid_url", e.to_string()))?;

    // Validate host is a simple hostname / IP (no flags injection into ssh-keyscan).
    if host.is_empty()
        || host.starts_with('-')
        || host.contains(|c: char| {
            c.is_whitespace() || matches!(c, ';' | '|' | '&' | '`' | '$' | '(' | ')' | '\0')
        })
    {
        return Err(AppError::new(
            "repo.mirror.invalid_url",
            "invalid SSH host for keyscan",
        ));
    }

    let out = tokio::process::Command::new("ssh-keyscan")
        .args(["-T", "5", &host])
        .output()
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("No such file") || msg.contains("not found") {
                AppError::new(
                    "repo.mirror.ssh_unavailable",
                    "OpenSSH client is not installed on this Oxidean API host (install openssh-client / ensure `ssh-keyscan` is on PATH)",
                )
            } else {
                AppError::new("repo.mirror.ssh", msg)
            }
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let lower = stderr.to_ascii_lowercase();
        if lower.contains("cannot run ssh") || lower.contains("no such file") {
            return Err(AppError::new(
                "repo.mirror.ssh_unavailable",
                "OpenSSH client is not installed on this Oxidean API host (install openssh-client / ensure `ssh-keyscan` is on PATH)",
            ));
        }
        return Err(AppError::new(
            "repo.mirror.ssh",
            format!("ssh-keyscan failed: {}", stderr.trim()),
        ));
    }
    // ssh-keyscan writes comments/warnings to stderr; keys are on stdout.
    let known_hosts = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n");
    if known_hosts.is_empty() {
        return Err(AppError::new(
            "repo.mirror.ssh",
            format!("ssh-keyscan returned no host keys for {host}"),
        ));
    }
    Ok(RepoMirrorFetchHostKeyResponse {
        host,
        known_hosts: format!("{known_hosts}\n"),
    })
}
