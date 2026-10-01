//! Per-repository email invites (`repo.invites.*`).

use chrono::Utc;
use oxidean_core::{
    AppError, CollaboratorPermission, RepoGetRequest, RepoInvitePublic, RepoInvitesCreateRequest,
    RepoInvitesCreateResponse, RepoInvitesListResponse, RepoInvitesRevokeRequest,
};
use oxidean_db::RepoInviteRow;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::auth::local::normalize_email;
use crate::auth::verify_reset::public_origin;
use crate::email::OutboundEmail;
use crate::repo::collaborators::resolve_repo_for_admin;
use crate::rpc::RpcCtx;

const TOKEN_BYTES: usize = 32;
const TTL_SECS: i64 = 7 * 24 * 60 * 60; // 7 days
const MIN_ISSUE_INTERVAL_SECS: i64 = 60;
const MAX_ISSUES_PER_HOUR: i64 = 5;
const INVITE_SUBJECT: &str = "You've been invited to a repository on Oxidean";

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("repo invite db error: {e}");
        AppError::new("repo.internal", "repository operation failed")
    }
}

fn sha256_hex(data: &[u8]) -> String {
    bytes_to_hex(&Sha256::digest(data))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn generate_magic() -> String {
    let mut token_bytes = [0u8; TOKEN_BYTES];
    rand::fill(&mut token_bytes);
    bytes_to_hex(&token_bytes)
}

fn rate_limited() -> AppError {
    AppError::new(
        "auth.rate_limited",
        "too many emails; try again later",
    )
}

fn invite_url(magic: &str) -> String {
    let origin = public_origin();
    format!("{origin}/invites/{magic}")
}

fn invite_public(row: &RepoInviteRow) -> Result<RepoInvitePublic, AppError> {
    let permission = CollaboratorPermission::parse(&row.permission).map_err(|e| {
        tracing::error!(error = %e, "invalid invite permission");
        AppError::new("repo.internal", "repository operation failed")
    })?;
    Ok(RepoInvitePublic {
        id: row.id.clone(),
        email: row.email.clone(),
        permission,
        expires_at: row.expires_at.clone(),
        invited_by: row.invited_by.clone(),
        created_at: row.created_at.clone(),
    })
}

fn build_invite_email(
    to: &str,
    owner: &str,
    name: &str,
    permission: CollaboratorPermission,
    magic: &str,
) -> OutboundEmail {
    let link = invite_url(magic);
    let text = format!(
        "You've been invited to {owner}/{name} on Oxidean with {perm} access.\n\n\
Accept this invitation:\n{link}\n\n\
This link expires in 7 days and can only be used once.\n\
If signup is closed on this instance, this invite still lets you create an account for the invited email.\n\
If you were not expecting this email, you can ignore it.\n",
        perm = permission.as_str(),
    );
    OutboundEmail {
        to: to.to_string(),
        subject: INVITE_SUBJECT.into(),
        text,
        html: None,
    }
}

fn parse_created_at(raw: &str) -> Result<chrono::DateTime<Utc>, AppError> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
                .map_err(|_| AppError::new("repo.internal", "repository operation failed"))
        })
}

async fn enforce_create_rate_limit(ctx: &RpcCtx, caller_id: &str) -> Result<(), AppError> {
    let since = (Utc::now() - chrono::Duration::hours(1))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let count = ctx
        .db
        .count_repo_invites_created_by_since(caller_id, &since)
        .await
        .map_err(db_err)?;
    if count >= MAX_ISSUES_PER_HOUR {
        return Err(rate_limited());
    }
    Ok(())
}

/// `repo.invites.create` — Repo Admin; hash-at-rest; returns invite_url once.
pub async fn create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoInvitesCreateResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: RepoInvitesCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.invites.create input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;

    let email = normalize_email(&req.email)?;
    enforce_create_rate_limit(ctx, &caller.id).await?;

    if let Some(existing) = ctx
        .db
        .find_user_by_email(&email)
        .await
        .map_err(db_err)?
    {
        if ctx
            .db
            .find_repo_collaborator(&accessible.row.id, &existing.id)
            .await
            .map_err(db_err)?
            .is_some()
        {
            return Err(AppError::new(
                "repo.already_collaborator",
                "user is already a collaborator on this repository",
            ));
        }
        // Personal owner already has admin — soft reject.
        if accessible.row.owner_type.eq_ignore_ascii_case("user")
            && accessible.row.owner_id == existing.id
        {
            return Err(AppError::new(
                "repo.already_collaborator",
                "user is already a collaborator on this repository",
            ));
        }
    }

    if let Some(pending) = ctx
        .db
        .find_pending_repo_invite_by_repo_email(&accessible.row.id, &email)
        .await
        .map_err(db_err)?
    {
        let created = parse_created_at(&pending.created_at)?;
        let age = Utc::now().signed_duration_since(created);
        if age.num_seconds() < MIN_ISSUE_INTERVAL_SECS {
            return Err(rate_limited());
        }
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let _ = ctx.db.revoke_repo_invite(&pending.id, &now).await;
    }

    let magic = generate_magic();
    let token_hash = sha256_hex(magic.as_bytes());
    let expires_at = (Utc::now() + chrono::Duration::seconds(TTL_SECS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let id = Uuid::new_v4().to_string();

    let row = ctx
        .db
        .insert_repo_invite(
            &id,
            &accessible.row.id,
            &email,
            req.permission.as_str(),
            &token_hash,
            &expires_at,
            &caller.id,
        )
        .await
        .map_err(db_err)?;

    let msg = build_invite_email(
        &email,
        &accessible.owner_username,
        &accessible.row.name,
        req.permission,
        &magic,
    );
    if let Err(e) = ctx.email.send(msg).await {
        tracing::error!(error = %e, "repo invite email send failed");
    }

    Ok(RepoInvitesCreateResponse {
        invite: invite_public(&row)?,
        invite_url: invite_url(&magic),
    })
}

/// `repo.invites.list` — Repo Admin; pending only; no tokens.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoInvitesListResponse, AppError> {
    let _caller = require_verified(ctx).await?;
    let req: RepoGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.invites.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let rows = ctx
        .db
        .list_pending_repo_invites(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let mut invites = Vec::with_capacity(rows.len());
    for row in rows {
        invites.push(invite_public(&row)?);
    }
    Ok(RepoInvitesListResponse { invites })
}

/// `repo.invites.revoke` — Repo Admin.
pub async fn revoke(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let _caller = require_verified(ctx).await?;
    let req: RepoInvitesRevokeRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.invites.revoke input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;

    let invite = ctx
        .db
        .find_repo_invite_by_id(&req.invite_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("repo.invite_not_found", "invite not found"))?;
    if invite.repository_id != accessible.row.id {
        return Err(AppError::new("repo.invite_not_found", "invite not found"));
    }
    if invite.accepted_at.is_some() || invite.revoked_at.is_some() {
        return Err(AppError::new("repo.invite_not_found", "invite not found"));
    }

    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .revoke_repo_invite(&invite.id, &now)
        .await
        .map_err(|e| {
            if e == "repo invite not found" {
                AppError::new("repo.invite_not_found", "invite not found")
            } else {
                db_err(e)
            }
        })?;
    Ok(serde_json::json!({ "ok": true }))
}
