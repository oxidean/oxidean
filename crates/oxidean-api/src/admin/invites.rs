//! `admin.invites.*` — instance email invites (bypass closed signup) and
//! shareable invite links (optional expiry, seat cap).

use chrono::Utc;
use oxidean_core::{
    AdminInvitesCreateItemResult, AdminInvitesCreateLinkRequest, AdminInvitesCreateLinkResponse,
    AdminInvitesCreateRequest, AdminInvitesCreateResponse, AdminInvitesListResponse,
    AdminInvitesRevokeRequest, AppError, InstanceInvitePublic,
};
use oxidean_db::InstanceInviteRow;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::admin::{db_err, require_admin_user};
use crate::auth::local::normalize_email;
use crate::auth::verify_reset::public_origin;
use crate::email::OutboundEmail;
use crate::rpc::RpcCtx;

const TOKEN_BYTES: usize = 32;
const TTL_SECS: i64 = 7 * 24 * 60 * 60; // 7 days (email-bound invites)
const MIN_ISSUE_INTERVAL_SECS: i64 = 60;
const MAX_ISSUES_PER_HOUR: i64 = 20;
const MAX_BULK: usize = 50;
const INVITE_SUBJECT: &str = "You've been invited to Oxidean";

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

fn invite_public(row: &InstanceInviteRow) -> InstanceInvitePublic {
    InstanceInvitePublic {
        id: row.id.clone(),
        email: row.email.clone(),
        expires_at: row.expires_at.clone(),
        invited_by: row.invited_by.clone(),
        created_at: row.created_at.clone(),
        max_uses: row.max_uses,
        use_count: row.use_count,
    }
}

pub(crate) fn invite_url(magic: &str) -> String {
    let origin = public_origin();
    format!("{origin}/invites/{magic}")
}

fn build_invite_email(to: &str, magic: &str) -> OutboundEmail {
    let link = invite_url(magic);
    let text = format!(
        "You've been invited to join this Oxidean instance.\n\n\
Accept this invitation:\n{link}\n\n\
This link expires in 7 days and can only be used once.\n\
If signup is closed on this instance, this invite still lets you create an account for the invited email.\n\
If you were not expecting this email, you can ignore it.\n"
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
                .map_err(|_| AppError::new("admin.internal", "admin operation failed"))
        })
}

/// Validate + normalize a caller-supplied expiry to UTC RFC-3339 seconds.
/// `None` → `None` (never expires). Past timestamps are rejected.
pub(crate) fn normalize_expiry(raw: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let dt = chrono::DateTime::parse_from_rfc3339(raw)
        .map(|d| d.with_timezone(&Utc))
        .or_else(|_| {
            // Accept `YYYY-MM-DD` (date-only UI input) as end-of-day UTC.
            chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .map(|d| {
                    d.and_hms_opt(23, 59, 59)
                        .unwrap_or_else(|| d.and_hms_opt(0, 0, 0).unwrap_or_default())
                        .and_utc()
                })
        })
        .map_err(|_| {
            AppError::new(
                "rpc.bad_input",
                "expires_at must be an ISO-8601 date or timestamp",
            )
        })?;
    if dt <= Utc::now() {
        return Err(AppError::new(
            "rpc.bad_input",
            "expires_at must be in the future",
        ));
    }
    Ok(Some(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)))
}

/// Validate a seat cap: positive when present.
pub(crate) fn normalize_max_uses(raw: Option<i64>) -> Result<Option<i64>, AppError> {
    match raw {
        Some(n) if n < 1 => Err(AppError::new(
            "rpc.bad_input",
            "max_uses must be a positive integer",
        )),
        other => Ok(other),
    }
}

/// Rejects when the caller's hourly issue budget is exhausted; otherwise
/// returns how many invites they may still create this hour.
async fn enforce_create_rate_limit(ctx: &RpcCtx, caller_id: &str) -> Result<i64, AppError> {
    let since = (Utc::now() - chrono::Duration::hours(1))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let count = ctx
        .db
        .count_instance_invites_created_by_since(caller_id, &since)
        .await
        .map_err(db_err)?;
    if count >= MAX_ISSUES_PER_HOUR {
        return Err(rate_limited());
    }
    Ok(MAX_ISSUES_PER_HOUR - count)
}

/// One recipient of a bulk `admin.invites.create` — never aborts the batch.
async fn create_one_email_invite(
    ctx: &RpcCtx,
    caller_id: &str,
    email: &str,
) -> AdminInvitesCreateItemResult {
    let fail = |msg: &str| AdminInvitesCreateItemResult {
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
    if ctx
        .db
        .find_user_by_email(&email)
        .await
        .map(|u| u.is_some())
        .unwrap_or(false)
    {
        return fail("an account with this email already exists");
    }

    if let Ok(Some(pending)) = ctx.db.find_pending_instance_invite_by_email(&email).await {
        if let Ok(created) = parse_created_at(&pending.created_at) {
            if Utc::now().signed_duration_since(created).num_seconds() < MIN_ISSUE_INTERVAL_SECS {
                return fail("invite was just sent; try again later");
            }
        }
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let _ = ctx.db.revoke_instance_invite(&pending.id, &now).await;
    }

    let magic = generate_magic();
    let token_hash = sha256_hex(magic.as_bytes());
    let expires_at = (Utc::now() + chrono::Duration::seconds(TTL_SECS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let id = Uuid::new_v4().to_string();

    let row = match ctx
        .db
        .insert_instance_invite(&id, Some(&email), &token_hash, Some(&expires_at), caller_id, Some(1))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "instance invite insert failed");
            return fail("invite creation failed");
        }
    };

    let url = invite_url(&magic);
    let msg = build_invite_email(&email, &magic);
    if let Err(e) = ctx.email.send(msg).await {
        tracing::error!(error = %e, "instance invite email send failed");
    }

    AdminInvitesCreateItemResult {
        email,
        ok: true,
        error: None,
        invite: Some(invite_public(&row)),
        invite_url: Some(url),
    }
}

/// `admin.invites.create` — bulk email invites; per-recipient results.
/// Hash-at-rest; each `invite_url` is returned exactly once.
pub async fn create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<AdminInvitesCreateResponse, AppError> {
    let caller = require_admin_user(ctx).await?;
    let req: AdminInvitesCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.invites.create input: {e}"),
        )
    })?;

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
            results.push(AdminInvitesCreateItemResult {
                email,
                ok: false,
                error: Some("hourly invite limit reached".into()),
                invite: None,
                invite_url: None,
            });
            continue;
        }
        let item = create_one_email_invite(ctx, &caller.id, &email).await;
        if item.ok {
            issued += 1;
        }
        results.push(item);
    }

    crate::audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "admin.invites_create",
        None,
        Some(
            serde_json::json!({ "requested": req.emails.len(), "created": issued }).to_string(),
        ),
    )
    .await;

    Ok(AdminInvitesCreateResponse { results })
}

/// `admin.invites.createLink` — shareable invite link (no bound email).
pub async fn create_link(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<AdminInvitesCreateLinkResponse, AppError> {
    let caller = require_admin_user(ctx).await?;
    let req: AdminInvitesCreateLinkRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.invites.createLink input: {e}"),
        )
    })?;

    let expires_at = normalize_expiry(req.expires_at.as_deref())?;
    let max_uses = normalize_max_uses(req.max_uses)?;
    enforce_create_rate_limit(ctx, &caller.id).await?;

    let magic = generate_magic();
    let token_hash = sha256_hex(magic.as_bytes());
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_instance_invite(
            &id,
            None,
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
        "admin.invite_link_create",
        Some(("invite", &id)),
        Some(serde_json::json!({ "max_uses": max_uses }).to_string()),
    )
    .await;

    Ok(AdminInvitesCreateLinkResponse {
        invite: invite_public(&row),
        invite_url: invite_url(&magic),
    })
}

/// `admin.invites.list` — pending only; no tokens.
pub async fn list(ctx: &RpcCtx) -> Result<AdminInvitesListResponse, AppError> {
    let _caller = require_admin_user(ctx).await?;
    let rows = ctx
        .db
        .list_pending_instance_invites()
        .await
        .map_err(db_err)?;
    Ok(AdminInvitesListResponse {
        invites: rows.iter().map(invite_public).collect(),
    })
}

/// `admin.invites.revoke`.
pub async fn revoke(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let caller = require_admin_user(ctx).await?;
    let req: AdminInvitesRevokeRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid admin.invites.revoke input: {e}"),
        )
    })?;

    let invite = ctx
        .db
        .find_instance_invite_by_id(&req.invite_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("admin.invite_not_found", "invite not found"))?;
    if invite.revoked_at.is_some() || invite.accepted_at.is_some() {
        return Err(AppError::new("admin.invite_not_found", "invite not found"));
    }

    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .revoke_instance_invite(&invite.id, &now)
        .await
        .map_err(|e| {
            if e == "instance invite not found" || e.contains("not found") {
                AppError::new("admin.invite_not_found", "invite not found")
            } else {
                db_err(e)
            }
        })?;

    crate::audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "admin.invite_revoke",
        Some(("invite", &invite.id)),
        None,
    )
    .await;

    Ok(serde_json::json!({ "ok": true }))
}
