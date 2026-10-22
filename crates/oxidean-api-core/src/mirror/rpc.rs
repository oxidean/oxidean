//! Mirror RPC handlers that are also library calls — `repo` lifecycle code
//! drives `upsert`/`sync_now` on repo settings changes (#162). The remaining
//! `repo.mirror.*` handlers live in `oxidean-api-mirror`.

use crate::actions::secrets::encrypt_secret;
use crate::auth::gate::require_verified;
use crate::mirror::engine::{encrypt_mirror_secret, generate_webhook_secret};
use crate::mirror::queue::enqueue_mirror_for_repo;
use crate::repo::resolve_repo_for_admin;
use crate::rpc::RpcCtx;
use oxidean_core::{
    AppError, RepoMirrorPublic, RepoMirrorRefResultPublic, RepoMirrorSyncNowRequest,
    RepoMirrorSyncNowResponse, RepoMirrorUpsertRequest,
};
use oxidean_db::RepositoryMirrorRow;
use oxidean_git::validate_remote_url;
use uuid::Uuid;

pub fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!(error = %e, "mirror db error");
        AppError::new("repo.mirror.internal", "mirror operation failed")
    }
}

fn mask_secret(has: bool) -> String {
    if has {
        "********".into()
    } else {
        String::new()
    }
}

fn webhook_url(_ctx: &RpcCtx, owner: &str, name: &str) -> String {
    let origin = std::env::var("OXIDEAN_PUBLIC_ORIGIN").unwrap_or_else(|_| {
        format!(
            "http://{}",
            std::env::var("OXIDEAN_HTTP_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into())
        )
    });
    let origin = origin.trim_end_matches('/');
    // Prefer path under /api for Traefik.
    format!("{origin}/api/repos/{owner}/{name}/mirror/hook")
}

pub async fn to_public(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
    row: &RepositoryMirrorRow,
    reveal_webhook: Option<String>,
) -> Result<RepoMirrorPublic, AppError> {
    let refs = ctx
        .db
        .list_mirror_ref_results(&row.id)
        .await
        .map_err(db_err)?;
    Ok(RepoMirrorPublic {
        id: row.id.clone(),
        repository_id: row.repository_id.clone(),
        remote_url: row.remote_url.clone(),
        auth_kind: row.auth_kind.clone(),
        username: row.username.clone(),
        has_secret: !row.secret_ciphertext.is_empty(),
        ssh_public_key: row.ssh_public_key.clone(),
        known_hosts: row.known_hosts.clone(),
        webhook_url: webhook_url(ctx, owner, name),
        webhook_secret_masked: mask_secret(!row.webhook_secret_ciphertext.is_empty()),
        poll_interval_secs: row.poll_interval_secs,
        enabled: row.enabled,
        sync_mode: if row.sync_mode == "exact" {
            "exact".into()
        } else {
            "merge".into()
        },
        last_synced_at: row.last_synced_at.clone(),
        last_status: row.last_status.clone(),
        last_error: row.last_error.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
        ref_results: refs
            .into_iter()
            .map(|r| RepoMirrorRefResultPublic {
                refname: r.refname,
                outcome: r.outcome,
                local_oid: r.local_oid,
                remote_oid: r.remote_oid,
                detail: r.detail,
                updated_at: r.updated_at,
            })
            .collect(),
        webhook_secret: reveal_webhook,
    })
}

pub async fn upsert(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoMirrorPublic, AppError> {
    let req: RepoMirrorUpsertRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_request", e.to_string()))?;
    let _user = require_verified(ctx).await?;
    let repo = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;

    let url = validate_remote_url(&req.remote_url)
        .map_err(|e| AppError::new("repo.mirror.invalid_url", e.to_string()))?
        .to_string();
    let auth_kind = match req.auth_kind.as_str() {
        "https_token" | "ssh_key" => req.auth_kind.as_str(),
        _ => {
            return Err(AppError::new(
                "repo.mirror.invalid_auth",
                "auth_kind must be https_token or ssh_key",
            ))
        }
    };

    let existing = ctx
        .db
        .get_mirror_by_repo(&repo.row.id)
        .await
        .map_err(db_err)?;

    let secret_ct = match req
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) => encrypt_mirror_secret(s).map_err(|e| AppError::new("repo.mirror.secret", e))?,
        None => String::new(), // upsert keeps existing when empty
    };
    if existing.is_none() && secret_ct.is_empty() {
        // HTTPS always needs a token on create. SSH may save URL + known_hosts first,
        // then generate/paste a deploy key (repo.mirror.generateSshKey).
        if auth_kind != "ssh_key" {
            return Err(AppError::new(
                "repo.mirror.secret_required",
                "A token is required when creating an HTTPS mirror",
            ));
        }
    }
    if auth_kind == "ssh_key" {
        let kh = req.known_hosts.as_deref().unwrap_or("").trim();
        let has_kh = !kh.is_empty()
            || existing
                .as_ref()
                .map(|e| !e.known_hosts.is_empty())
                .unwrap_or(false);
        if !has_kh {
            return Err(AppError::new(
                "repo.mirror.known_hosts_required",
                "SSH known_hosts is required for ssh_key remotes",
            ));
        }
    }

    let (webhook_ct, reveal_webhook) = if existing
        .as_ref()
        .map(|e| e.webhook_secret_ciphertext.is_empty())
        .unwrap_or(true)
    {
        let plain = generate_webhook_secret();
        let ct = encrypt_secret(&plain).map_err(|e| AppError::new("repo.mirror.secret", e))?;
        (ct, Some(plain))
    } else {
        (String::new(), None)
    };

    let id = existing
        .as_ref()
        .map(|e| e.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let poll = req.poll_interval_secs.unwrap_or(60).clamp(0, 86_400);
    let enabled = req.enabled.unwrap_or(true);
    let username = req.username.unwrap_or_default();
    let ssh_pub = req.ssh_public_key.unwrap_or_default();
    let known_hosts = req.known_hosts.unwrap_or_default();
    let sync_mode = match req
        .sync_mode
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some("exact") => "exact",
        Some("merge") => "merge",
        Some(_) => {
            return Err(AppError::new(
                "repo.mirror.invalid_sync_mode",
                "sync_mode must be merge or exact",
            ))
        }
        None => existing
            .as_ref()
            .map(|e| e.sync_mode.as_str())
            .filter(|s| *s == "exact" || *s == "merge")
            .unwrap_or("merge"),
    };
    let clear_ref_snapshot = existing
        .as_ref()
        .map(|e| e.sync_mode.as_str() != sync_mode)
        .unwrap_or(false);

    let row = ctx
        .db
        .upsert_mirror(
            &id,
            &repo.row.id,
            &url,
            auth_kind,
            &username,
            &secret_ct,
            &ssh_pub,
            &known_hosts,
            &webhook_ct,
            poll,
            enabled,
            sync_mode,
            clear_ref_snapshot,
        )
        .await
        .map_err(db_err)?;

    to_public(ctx, &req.owner, &req.name, &row, reveal_webhook).await
}

pub async fn sync_now(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoMirrorSyncNowResponse, AppError> {
    let req: RepoMirrorSyncNowRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_request", e.to_string()))?;
    let _user = require_verified(ctx).await?;
    let repo = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let mirror = ctx
        .db
        .get_mirror_by_repo(&repo.row.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("repo.mirror.not_found", "No mirror configured"))?;
    enqueue_mirror_for_repo(
        ctx.db.clone(),
        ctx.git.clone(),
        ctx.repos_dir.clone(),
        repo.row.id.clone(),
    )
    .await
    .map_err(|e| AppError::new("repo.mirror.enqueue", e))?;
    let public = to_public(ctx, &req.owner, &req.name, &mirror, None).await?;
    Ok(RepoMirrorSyncNowResponse {
        enqueued: true,
        mirror: public,
    })
}
