//! Per-repository email invites (`repo.invites.*`).

use chrono::Utc;
use oxidean_core::{
    AppError, CollaboratorPermission, RepoGetRequest, RepoInvitePublic,
    RepoInvitesCreateItemResult, RepoInvitesCreateLinkRequest, RepoInvitesCreateLinkResponse,
    RepoInvitesCreateRequest, RepoInvitesCreateResponse, RepoInvitesListResponse,
    RepoInvitesRevokeRequest,
};
use oxidean_db::RepoInviteRow;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::auth::local::normalize_email;
use crate::auth::verify_reset::public_origin;
use crate::email::OutboundEmail;
use crate::repo::acl::AccessibleRepo;
use crate::repo::collaborators::resolve_repo_for_admin;
use crate::rpc::RpcCtx;

const TOKEN_BYTES: usize = 32;
const TTL_SECS: i64 = 7 * 24 * 60 * 60; // 7 days (email-bound invites)
const MIN_ISSUE_INTERVAL_SECS: i64 = 60;
const MAX_ISSUES_PER_HOUR: i64 = 20;
const MAX_BULK: usize = 50;
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
    AppError::new("auth.rate_limited", "too many emails; try again later")
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
        max_uses: row.max_uses,
        use_count: row.use_count,
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

/// Rejects when the caller's hourly issue budget is exhausted; otherwise
/// returns how many invites they may still create this hour.
async fn enforce_create_rate_limit(ctx: &RpcCtx, caller_id: &str) -> Result<i64, AppError> {
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
    Ok(MAX_ISSUES_PER_HOUR - count)
}

/// One recipient of a bulk `repo.invites.create` — never aborts the batch.
async fn create_one_email_invite(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    caller_id: &str,
    permission: CollaboratorPermission,
    email: &str,
) -> RepoInvitesCreateItemResult {
    let fail = |msg: &str| RepoInvitesCreateItemResult {
        email: email.to_string(),
        ok: false,
        error: Some(msg.to_string()),
        invite: None,
        invite_url: None,
    };

    let email = match normalize_email(email) {
        Ok(e) => e,
        Err(_) => return fail("invalid email address"),
    };

    match ctx.db.find_user_by_email(&email).await {
        Ok(Some(existing)) => {
            let already = ctx
                .db
                .find_repo_collaborator(&accessible.row.id, &existing.id)
                .await
                .map(|c| c.is_some())
                .unwrap_or(false)
                // Personal owner already has admin.
                || (accessible.row.owner_type.eq_ignore_ascii_case("user")
                    && accessible.row.owner_id == existing.id);
            if already {
                return fail("user is already a collaborator on this repository");
            }
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = %e, "repo invite user lookup failed");
            return fail("invite creation failed");
        }
    }

    if let Ok(Some(pending)) = ctx
        .db
        .find_pending_repo_invite_by_repo_email(&accessible.row.id, &email)
        .await
    {
        if let Ok(created) = parse_created_at(&pending.created_at) {
            if Utc::now().signed_duration_since(created).num_seconds() < MIN_ISSUE_INTERVAL_SECS {
                return fail("invite was just sent; try again later");
            }
        }
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let _ = ctx.db.revoke_repo_invite(&pending.id, &now).await;
    }

    let magic = generate_magic();
    let token_hash = sha256_hex(magic.as_bytes());
    let expires_at = (Utc::now() + chrono::Duration::seconds(TTL_SECS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let id = Uuid::new_v4().to_string();

    let row = match ctx
        .db
        .insert_repo_invite(
            &id,
            &accessible.row.id,
            Some(&email),
            permission.as_str(),
            &token_hash,
            Some(&expires_at),
            caller_id,
            Some(1),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "repo invite insert failed");
            return fail("invite creation failed");
        }
    };

    let msg = build_invite_email(
        &email,
        &accessible.owner_username,
        &accessible.row.name,
        permission,
        &magic,
    );
    if let Err(e) = ctx.email.send(msg).await {
        tracing::error!(error = %e, "repo invite email send failed");
    }

    RepoInvitesCreateItemResult {
        email,
        ok: true,
        error: None,
        invite: invite_public(&row).ok(),
        invite_url: Some(invite_url(&magic)),
    }
}

/// `repo.invites.create` — Repo Admin; bulk email invites with per-recipient results.
/// Hash-at-rest; each `invite_url` is returned exactly once.
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

    if req.emails.is_empty() {
        return Err(AppError::new("rpc.bad_input", "emails must not be empty"));
    }
    if req.emails.len() > MAX_BULK {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("at most {MAX_BULK} emails per request"),
        ));
    }
    let remaining = enforce_create_rate_limit(ctx, &caller.id).await?;

    let mut results = Vec::with_capacity(req.emails.len());
    let mut issued = 0i64;
    for raw in &req.emails {
        let email = raw.trim().to_string();
        if email.is_empty() {
            continue;
        }
        if issued >= remaining {
            results.push(RepoInvitesCreateItemResult {
                email,
                ok: false,
                error: Some("hourly invite limit reached".into()),
                invite: None,
                invite_url: None,
            });
            continue;
        }
        let item =
            create_one_email_invite(ctx, &accessible, &caller.id, req.permission, &email).await;
        if item.ok {
            issued += 1;
        }
        results.push(item);
    }

    crate::audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "repo.invites_create",
        Some(("repo", &accessible.row.id)),
        Some(
            serde_json::json!({
                "repo": format!("{}/{}", accessible.owner_username, accessible.row.name),
                "permission": req.permission.as_str(),
                "requested": req.emails.len(),
                "created": issued,
            })
            .to_string(),
        ),
    )
    .await;

    Ok(RepoInvitesCreateResponse { results })
}

/// `repo.invites.createLink` — Repo Admin; shareable link (no bound email).
pub async fn create_link(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoInvitesCreateLinkResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: RepoInvitesCreateLinkRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.invites.createLink input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;

    let expires_at = crate::admin::invites::normalize_expiry(req.expires_at.as_deref())?;
    let max_uses = crate::admin::invites::normalize_max_uses(req.max_uses)?;
    enforce_create_rate_limit(ctx, &caller.id).await?;

    let magic = generate_magic();
    let token_hash = sha256_hex(magic.as_bytes());
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_repo_invite(
            &id,
            &accessible.row.id,
            None,
            req.permission.as_str(),
            &token_hash,
            expires_at.as_deref(),
            &caller.id,
            max_uses,
        )
        .await
        .map_err(db_err)?;

    crate::audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "repo.invite_link_create",
        Some(("repo", &accessible.row.id)),
        Some(
            serde_json::json!({
                "repo": format!("{}/{}", accessible.owner_username, accessible.row.name),
                "permission": req.permission.as_str(),
                "max_uses": max_uses,
            })
            .to_string(),
        ),
    )
    .await;

    Ok(RepoInvitesCreateLinkResponse {
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
    let caller = require_verified(ctx).await?;
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
    crate::audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "repo.invite_revoke",
        Some(("repo", &accessible.row.id)),
        Some(serde_json::json!({ "invite_id": invite.id }).to_string()),
    )
    .await;
    Ok(serde_json::json!({ "ok": true }))
}
