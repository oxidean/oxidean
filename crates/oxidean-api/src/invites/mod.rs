//! Unified invite accept (`invites.accept`) — instance then organization tokens.

use chrono::Utc;
use oxidean_core::{
    is_reserved_username, validate_username, AppError, InvitesAcceptRequest, InvitesAcceptResponse,
    OrgMemberPublic, OrgPublic, OrgRole, Role,
};
use oxidean_db::UserRow;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::local::normalize_email;
use crate::auth::password::{hash_password_str, PasswordError, MIN_PASSWORD_LEN};
use crate::org::{db_err as org_db_err, to_public};
use crate::rpc::{CookieChange, RpcCtx};

const TOKEN_HEX_LEN: usize = 64; // 32 bytes as hex

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

fn invalid_invite() -> AppError {
    AppError::new(
        "invite.invalid",
        "invalid or expired invitation",
    )
}

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("invite accept db error: {e}");
        AppError::new("invite.internal", "invitation operation failed")
    }
}

fn map_username_err(msg: String) -> AppError {
    if msg.contains("reserved") {
        AppError::new("auth.reserved_username", "username is reserved")
    } else {
        AppError::new("auth.invalid_username", msg)
    }
}

fn session_err(e: crate::auth::session::AuthError) -> AppError {
    match e {
        crate::auth::session::AuthError::NotConfigured => AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        ),
        crate::auth::session::AuthError::Store(msg) => {
            tracing::error!("session store error: {msg}");
            AppError::new("auth.session_failed", "session operation failed")
        }
    }
}

fn parse_expires(raw: &str) -> Result<chrono::DateTime<Utc>, AppError> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
        })
        .map_err(|_| invalid_invite())
}

/// Resolve or provision a user for an invite email (shared by instance + org accept).
pub async fn resolve_or_provision_invitee(
    ctx: &mut RpcCtx,
    invite_email: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<UserRow, AppError> {
    let email = normalize_email(invite_email)?;

    if let Some(session) = &ctx.session {
        let session_user = ctx
            .db
            .find_user_by_id(&session.user_id)
            .await
            .map_err(db_err)?
            .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
        if session_user.banned_at.is_some() {
            return Err(AppError::new(
                "auth.banned",
                "This account has been suspended.",
            ));
        }
        if session_user.email.eq_ignore_ascii_case(&email) {
            return Ok(session_user);
        }
        return Err(AppError::new(
            "invite.email_mismatch",
            "signed-in email does not match this invitation",
        ));
    }

    if ctx
        .db
        .find_user_by_email(&email)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "invite.login_required",
            "an account with this email already exists; sign in to accept the invitation",
        ));
    }

    let username = username
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            AppError::new(
                "rpc.bad_input",
                "username is required to accept this invitation",
            )
        })?;
    let password = password.filter(|s| !s.is_empty()).ok_or_else(|| {
        AppError::new(
            "rpc.bad_input",
            "password is required to accept this invitation",
        )
    })?;

    validate_username(username).map_err(map_username_err)?;
    if is_reserved_username(username) {
        return Err(AppError::new(
            "auth.reserved_username",
            "username is reserved",
        ));
    }
    let password_hash = hash_password_str(password).map_err(|e| match e {
        PasswordError::TooShort => AppError::new(
            "auth.weak_password",
            format!("password must be at least {MIN_PASSWORD_LEN} characters"),
        ),
        PasswordError::Hash(_) => {
            tracing::error!("password hash failed");
            AppError::new("auth.internal", "authentication failed")
        }
    })?;

    if crate::repo::login_slug_taken(&ctx.db, username)
        .await
        .map_err(db_err)?
    {
        return Err(AppError::new(
            "auth.taken",
            "email or username already taken",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let display_name = username.to_string();
    let created = ctx
        .db
        .create_user(
            &id,
            &email,
            username,
            Some(&password_hash),
            &display_name,
            "",
            None,
            Role::User,
        )
        .await
        .map_err(db_err)?;

    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .set_email_verified_at(&created.id, &now)
        .await
        .map_err(db_err)?;

    let (_tok, cookie) = ctx
        .sessions
        .create(&ctx.db, &created.id, false)
        .await
        .map_err(session_err)?;
    ctx.set_cookie = Some(CookieChange::Set(cookie));

    ctx.db
        .find_user_by_id(&created.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.internal", "authentication failed"))
}

async fn accept_instance(
    ctx: &mut RpcCtx,
    token: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<Option<InvitesAcceptResponse>, AppError> {
    let row = match ctx
        .db
        .find_instance_invite_by_token_hash(&sha256_hex(token.as_bytes()))
        .await
        .map_err(db_err)?
    {
        Some(r) => r,
        None => return Ok(None),
    };

    if row.accepted_at.is_some() || row.revoked_at.is_some() {
        return Err(invalid_invite());
    }
    let expires_at = parse_expires(&row.expires_at)?;
    if expires_at <= Utc::now() {
        return Err(invalid_invite());
    }

    let _user = resolve_or_provision_invitee(ctx, &row.email, username, password).await?;

    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .accept_instance_invite(&row.id, &now)
        .await
        .map_err(|e| {
            if e.contains("not found") {
                invalid_invite()
            } else {
                db_err(e)
            }
        })?;

    Ok(Some(InvitesAcceptResponse::Instance))
}

async fn accept_org(
    ctx: &mut RpcCtx,
    token: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<InvitesAcceptResponse, AppError> {
    let row = ctx
        .db
        .find_org_invite_by_token_hash(&sha256_hex(token.as_bytes()))
        .await
        .map_err(org_db_err)?
        .ok_or_else(invalid_invite)?;

    if row.accepted_at.is_some() || row.revoked_at.is_some() {
        return Err(invalid_invite());
    }
    let expires_at = parse_expires(&row.expires_at)?;
    if expires_at <= Utc::now() {
        return Err(invalid_invite());
    }

    let invite_role = OrgRole::parse(&row.role).map_err(|_| {
        AppError::new("org.internal", "organization operation failed")
    })?;

    let org = ctx
        .db
        .find_organization_by_id(&row.org_id)
        .await
        .map_err(org_db_err)?
        .ok_or_else(invalid_invite)?;

    let user = resolve_or_provision_invitee(ctx, &row.email, username, password).await?;

    if ctx
        .db
        .find_org_member(&org.id, &user.id)
        .await
        .map_err(org_db_err)?
        .is_some()
    {
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let _ = ctx.db.accept_org_invite(&row.id, &now).await;
        return Err(AppError::new(
            "org.already_member",
            "user is already a member of this organization",
        ));
    }

    let member_row = ctx
        .db
        .insert_org_member(&org.id, &user.id, invite_role.as_str())
        .await
        .map_err(org_db_err)?;

    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .accept_org_invite(&row.id, &now)
        .await
        .map_err(|e| {
            if e == "org invite not found" {
                invalid_invite()
            } else {
                org_db_err(e)
            }
        })?;

    let org_public: OrgPublic = to_public(&org)?;
    Ok(InvitesAcceptResponse::Org {
        org: org_public,
        member: OrgMemberPublic {
            user_id: member_row.user_id,
            username: user.username,
            role: invite_role,
            created_at: member_row.created_at,
        },
    })
}

/// `invites.accept` — try instance_invites first, then organization_invites.
pub async fn accept(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<InvitesAcceptResponse, AppError> {
    let req: InvitesAcceptRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid invites.accept input: {e}"),
        )
    })?;
    let token = req.token.trim();
    if token.is_empty() || token.len() != TOKEN_HEX_LEN {
        return Err(invalid_invite());
    }

    let username = req.username.as_deref();
    let password = req.password.as_deref();

    if let Some(resp) = accept_instance(ctx, token, username, password).await? {
        return Ok(resp);
    }
    accept_org(ctx, token, username, password).await
}
