//! Org email invites (`org.invites.*` — ORG-01 / D-ORG-03 / A2 / T-10-11 / T-10-12).

use chrono::Utc;
use oxidean_core::{
    AppError, OrgInvitePublic, OrgInvitesAcceptRequest, OrgInvitesAcceptResponse,
    OrgInvitesCreateItemResult, OrgInvitesCreateLinkRequest, OrgInvitesCreateLinkResponse,
    OrgInvitesCreateRequest, OrgInvitesCreateResponse, OrgInvitesListResponse,
    OrgInvitesRevokeRequest, OrgRole, OrgSlugRequest,
};
use oxidean_db::{OrgInviteRow, OrganizationRow, UserRow};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::auth::local::normalize_email;
use crate::auth::verify_reset::public_origin;
use crate::email::OutboundEmail;
use crate::org::{db_err, load_org_by_slug, require_org_role};
use crate::rpc::RpcCtx;

const TOKEN_BYTES: usize = 32;
const TTL_SECS: i64 = 7 * 24 * 60 * 60; // 7 days (email-bound invites)
const MIN_ISSUE_INTERVAL_SECS: i64 = 60;
const MAX_ISSUES_PER_HOUR: i64 = 20;
const MAX_BULK: usize = 50;
const INVITE_SUBJECT: &str = "You've been invited to an organization on Oxidean";

fn is_admin_plus(role: OrgRole) -> bool {
    matches!(role, OrgRole::Owner | OrgRole::Admin)
}

async fn require_admin_plus(
    ctx: &RpcCtx,
    org: &OrganizationRow,
    caller: &UserRow,
) -> Result<OrgRole, AppError> {
    let role = require_org_role(ctx, &org.id, &caller.id).await?;
    if !is_admin_plus(role) {
        return Err(AppError::new(
            "org.forbidden",
            "organization admin access required",
        ));
    }
    Ok(role)
}

fn ensure_can_grant_role(caller_role: OrgRole, new_role: OrgRole) -> Result<(), AppError> {
    if new_role == OrgRole::Owner && caller_role != OrgRole::Owner {
        return Err(AppError::new(
            "org.forbidden",
            "only an organization owner can grant the owner role",
        ));
    }
    Ok(())
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

fn invalid_invite() -> AppError {
    AppError::new(
        "org.invalid_invite",
        "invalid or expired invitation",
    )
}

fn parse_created_at(raw: &str) -> Result<chrono::DateTime<Utc>, AppError> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
                .map_err(|_| AppError::new("org.internal", "organization operation failed"))
        })
}

fn invite_public(row: &OrgInviteRow) -> Result<OrgInvitePublic, AppError> {
    let role = OrgRole::parse(&row.role).map_err(|e| {
        tracing::error!(error = %e, "invalid invite role");
        AppError::new("org.internal", "organization operation failed")
    })?;
    Ok(OrgInvitePublic {
        id: row.id.clone(),
        email: row.email.clone(),
        role,
        expires_at: row.expires_at.clone(),
        invited_by: row.invited_by.clone(),
        created_at: row.created_at.clone(),
        max_uses: row.max_uses,
        use_count: row.use_count,
    })
}

fn invite_url(magic: &str) -> String {
    let origin = public_origin();
    format!("{origin}/invites/{magic}")
}

fn build_invite_email(to: &str, org_slug: &str, org_name: &str, magic: &str) -> OutboundEmail {
    let link = invite_url(magic);
    let text = format!(
        "You've been invited to join {org_name} ({org_slug}) on Oxidean.\n\n\
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

/// Rejects when the caller's hourly issue budget is exhausted; otherwise
/// returns how many invites they may still create this hour.
async fn enforce_create_rate_limit(ctx: &RpcCtx, caller_id: &str) -> Result<i64, AppError> {
    let since = (Utc::now() - chrono::Duration::hours(1))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let count = ctx
        .db
        .count_org_invites_created_by_since(caller_id, &since)
        .await
        .map_err(db_err)?;
    if count >= MAX_ISSUES_PER_HOUR {
        return Err(rate_limited());
    }
    Ok(MAX_ISSUES_PER_HOUR - count)
}

/// One recipient of a bulk `org.invites.create` — never aborts the batch.
async fn create_one_email_invite(
    ctx: &RpcCtx,
    org: &OrganizationRow,
    caller_id: &str,
    role: OrgRole,
    email: &str,
) -> OrgInvitesCreateItemResult {
    let fail = |msg: &str| OrgInvitesCreateItemResult {
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

    // Soft success when invitee email already belongs to a member (anti-noise for admins).
    match ctx.db.find_user_by_email(&email).await {
        Ok(Some(existing)) => {
            if ctx
                .db
                .find_org_member(&org.id, &existing.id)
                .await
                .map(|m| m.is_some())
                .unwrap_or(false)
            {
                return fail("user is already a member of this organization");
            }
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = %e, "org invite user lookup failed");
            return fail("invite creation failed");
        }
    }

    if let Ok(Some(pending)) = ctx
        .db
        .find_pending_org_invite_by_org_email(&org.id, &email)
        .await
    {
        if let Ok(created) = parse_created_at(&pending.created_at) {
            if Utc::now().signed_duration_since(created).num_seconds() < MIN_ISSUE_INTERVAL_SECS {
                return fail("invite was just sent; try again later");
            }
        }
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let _ = ctx.db.revoke_org_invite(&pending.id, &now).await;
    }

    let magic = generate_magic();
    let token_hash = sha256_hex(magic.as_bytes());
    let expires_at = (Utc::now() + chrono::Duration::seconds(TTL_SECS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let id = Uuid::new_v4().to_string();

    let row = match ctx
        .db
        .insert_org_invite(
            &id,
            &org.id,
            Some(&email),
            role.as_str(),
            &token_hash,
            Some(&expires_at),
            caller_id,
            Some(1),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "org invite insert failed");
            return fail("invite creation failed");
        }
    };

    let msg = build_invite_email(&email, &org.slug, &org.display_name, &magic);
    if let Err(e) = ctx.email.send(msg).await {
        tracing::error!(error = %e, "org invite email send failed");
    }

    OrgInvitesCreateItemResult {
        email,
        ok: true,
        error: None,
        invite: invite_public(&row).ok(),
        invite_url: Some(invite_url(&magic)),
    }
}

/// `org.invites.create` — Admin+; bulk email invites with per-recipient results.
/// Hash-at-rest; each `invite_url` is returned exactly once.
pub async fn create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgInvitesCreateResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgInvitesCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.invites.create input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let caller_role = require_admin_plus(ctx, &org, &caller).await?;
    ensure_can_grant_role(caller_role, req.role)?;

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
            results.push(OrgInvitesCreateItemResult {
                email,
                ok: false,
                error: Some("hourly invite limit reached".into()),
                invite: None,
                invite_url: None,
            });
            continue;
        }
        let item = create_one_email_invite(ctx, &org, &caller.id, req.role, &email).await;
        if item.ok {
            issued += 1;
        }
        results.push(item);
    }

    crate::audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "org.invites_create",
        Some(("org", &org.id)),
        Some(
            serde_json::json!({
                "org": org.slug,
                "role": req.role.as_str(),
                "requested": req.emails.len(),
                "created": issued,
            })
            .to_string(),
        ),
    )
    .await;

    Ok(OrgInvitesCreateResponse { results })
}

/// `org.invites.createLink` — Admin+; shareable link (no bound email).
pub async fn create_link(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgInvitesCreateLinkResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgInvitesCreateLinkRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.invites.createLink input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let caller_role = require_admin_plus(ctx, &org, &caller).await?;
    ensure_can_grant_role(caller_role, req.role)?;

    let expires_at = crate::admin::invites::normalize_expiry(req.expires_at.as_deref())?;
    let max_uses = crate::admin::invites::normalize_max_uses(req.max_uses)?;
    enforce_create_rate_limit(ctx, &caller.id).await?;

    let magic = generate_magic();
    let token_hash = sha256_hex(magic.as_bytes());
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_org_invite(
            &id,
            &org.id,
            None,
            req.role.as_str(),
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
        "org.invite_link_create",
        Some(("org", &org.id)),
        Some(serde_json::json!({ "org": org.slug, "role": req.role.as_str(), "max_uses": max_uses }).to_string()),
    )
    .await;

    Ok(OrgInvitesCreateLinkResponse {
        invite: invite_public(&row)?,
        invite_url: invite_url(&magic),
    })
}

/// `org.invites.list` — Admin+; pending only; no tokens.
pub async fn list(ctx: &RpcCtx, input: serde_json::Value) -> Result<OrgInvitesListResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgSlugRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.invites.list input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;

    let rows = ctx
        .db
        .list_pending_org_invites(&org.id)
        .await
        .map_err(db_err)?;
    let mut invites = Vec::with_capacity(rows.len());
    for row in rows {
        invites.push(invite_public(&row)?);
    }
    Ok(OrgInvitesListResponse { invites })
}

/// `org.invites.revoke` — Admin+.
pub async fn revoke(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgInvitesRevokeRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.invites.revoke input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;

    let invite = ctx
        .db
        .find_org_invite_by_id(&req.invite_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.invite_not_found", "invite not found"))?;
    if invite.org_id != org.id {
        return Err(AppError::new("org.invite_not_found", "invite not found"));
    }
    if invite.accepted_at.is_some() || invite.revoked_at.is_some() {
        return Err(AppError::new("org.invite_not_found", "invite not found"));
    }

    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .revoke_org_invite(&invite.id, &now)
        .await
        .map_err(|e| {
            if e == "org invite not found" {
                AppError::new("org.invite_not_found", "invite not found")
            } else {
                db_err(e)
            }
        })?;
    crate::audit::record(
        ctx,
        Some((&caller.id, &caller.username)),
        "org.invite_revoke",
        Some(("org", &org.id)),
        Some(serde_json::json!({ "invite_id": invite.id }).to_string()),
    )
    .await;
    Ok(serde_json::json!({ "ok": true }))
}

fn map_unified_invite_err(e: AppError) -> AppError {
    match e.code.as_str() {
        "invite.invalid" => invalid_invite(),
        "invite.email_mismatch" => AppError::new(
            "org.invite_email_mismatch",
            "signed-in email does not match this invitation",
        ),
        "invite.login_required" => AppError::new(
            "org.invite_login_required",
            "an account with this email already exists; sign in to accept the invitation",
        ),
        _ => e,
    }
}

/// `org.invites.accept` — thin wrapper over unified `invites.accept` (org kind only).
pub async fn accept(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<OrgInvitesAcceptResponse, AppError> {
    let req: OrgInvitesAcceptRequest = serde_json::from_value(input.clone()).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.invites.accept input: {e}"),
        )
    })?;
    // Gate on kind before delegating: the unified accept would otherwise
    // consume a same-hashed instance/repo invite before this wrapper could
    // reject it.
    let token = req.token.trim();
    let is_org_invite = !token.is_empty()
        && token.len() == 64
        && ctx
            .db
            .find_org_invite_by_token_hash(&sha256_hex(token.as_bytes()))
            .await
            .map_err(db_err)?
            .is_some();
    if !is_org_invite {
        return Err(invalid_invite());
    }
    match crate::invites::accept(ctx, input)
        .await
        .map_err(map_unified_invite_err)?
    {
        oxidean_core::InvitesAcceptResponse::Org { org, member } => {
            Ok(OrgInvitesAcceptResponse { org, member })
        }
        oxidean_core::InvitesAcceptResponse::Instance
        | oxidean_core::InvitesAcceptResponse::Repo { .. } => Err(invalid_invite()),
    }
}
