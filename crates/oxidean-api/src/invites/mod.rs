//! Unified invite accept (`invites.accept`) + invite preview (`invites.get`).
//! Token lookup order: instance, then organization, then repository invites.

use chrono::Utc;
use oxidean_core::{
    is_reserved_username, validate_username, AppError, CollaboratorPermission, InvitesAcceptRequest,
    InvitesAcceptResponse, InvitesGetRequest, InvitesGetResponse, OrgMemberPublic, OrgPublic,
    OrgRole, Role,
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

/// `Some(reason)` when the invite is not acceptable: revoked / expired / seats gone.
fn not_acceptable_reason(
    expires_at: Option<&str>,
    revoked_at: Option<&str>,
    max_uses: Option<i64>,
    use_count: i64,
) -> Option<&'static str> {
    if revoked_at.is_some() {
        return Some("revoked");
    }
    if let Some(raw) = expires_at {
        match parse_expires(raw) {
            Ok(dt) if dt > Utc::now() => {}
            Ok(_) => return Some("expired"),
            Err(_) => return Some("expired"),
        }
    }
    if let Some(max) = max_uses {
        if use_count >= max {
            return Some("exhausted");
        }
    }
    None
}

fn seats_remaining(max_uses: Option<i64>, use_count: i64) -> Option<i64> {
    max_uses.map(|m| (m - use_count).max(0))
}

/// Resolve or provision the invitee.
///
/// `invite_email` is `Some` for email-bound invites (must match), `None` for
/// shareable links — then any signed-in user may accept, and anonymous accepts
/// must supply `request_email` so an account can be provisioned.
pub async fn resolve_or_provision_invitee(
    ctx: &mut RpcCtx,
    invite_email: Option<&str>,
    request_email: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<UserRow, AppError> {
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
        // Email-bound invites still enforce the recipient match; link invites
        // accept any signed-in user.
        if let Some(bound) = invite_email {
            if session_user.email.eq_ignore_ascii_case(&normalize_email(bound)?) {
                return Ok(session_user);
            }
            return Err(AppError::new(
                "invite.email_mismatch",
                "signed-in email does not match this invitation",
            ));
        }
        return Ok(session_user);
    }

    // Anonymous accept — effective email is the bound invite email or the
    // caller-supplied account email for shareable links.
    let email = match invite_email {
        Some(bound) => normalize_email(bound)?,
        None => normalize_email(request_email.unwrap_or("")).map_err(|_| {
            AppError::new(
                "rpc.bad_input",
                "email is required to accept this invitation",
            )
        })?,
    };

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
        .create(
            &ctx.db,
            &created.id,
            false,
            ctx.client.ip_address.as_deref(),
            ctx.client.user_agent.as_deref(),
        )
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
    email: Option<&str>,
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

    if not_acceptable_reason(
        row.expires_at.as_deref(),
        row.revoked_at.as_deref(),
        row.max_uses,
        row.use_count,
    )
    .is_some()
    {
        return Err(invalid_invite());
    }

    let user = resolve_or_provision_invitee(
        ctx,
        row.email.as_deref(),
        email,
        username,
        password,
    )
    .await?;

    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .consume_instance_invite(&row.id, &now)
        .await
        .map_err(|e| {
            if e.contains("not found") {
                invalid_invite()
            } else {
                db_err(e)
            }
        })?;

    crate::audit::record(
        ctx,
        Some((&user.id, &user.username)),
        "invite.accept",
        Some(("invite", &row.id)),
        Some(serde_json::json!({ "kind": "instance" }).to_string()),
    )
    .await;

    Ok(Some(InvitesAcceptResponse::Instance))
}

async fn accept_org(
    ctx: &mut RpcCtx,
    token: &str,
    email: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<Option<InvitesAcceptResponse>, AppError> {
    let row = match ctx
        .db
        .find_org_invite_by_token_hash(&sha256_hex(token.as_bytes()))
        .await
        .map_err(org_db_err)?
    {
        Some(r) => r,
        None => return Ok(None),
    };

    if not_acceptable_reason(
        row.expires_at.as_deref(),
        row.revoked_at.as_deref(),
        row.max_uses,
        row.use_count,
    )
    .is_some()
    {
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

    let user = resolve_or_provision_invitee(
        ctx,
        row.email.as_deref(),
        email,
        username,
        password,
    )
    .await?;

    if ctx
        .db
        .find_org_member(&org.id, &user.id)
        .await
        .map_err(org_db_err)?
        .is_some()
    {
        // Already a member — do not consume a seat on shareable links.
        return Err(AppError::new(
            "org.already_member",
            "user is already a member of this organization",
        ));
    }

    // Consume a seat before granting membership so a race cannot over-grant.
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .consume_org_invite(&row.id, &now)
        .await
        .map_err(|e| {
            if e == "org invite not found" {
                invalid_invite()
            } else {
                org_db_err(e)
            }
        })?;

    let member_row = ctx
        .db
        .insert_org_member(&org.id, &user.id, invite_role.as_str())
        .await
        .map_err(org_db_err)?;

    crate::audit::record(
        ctx,
        Some((&user.id, &user.username)),
        "invite.accept",
        Some(("invite", &row.id)),
        Some(serde_json::json!({ "kind": "org", "org": org.slug }).to_string()),
    )
    .await;

    let org_public: OrgPublic = to_public(&org)?;
    Ok(Some(InvitesAcceptResponse::Org {
        org: org_public,
        member: OrgMemberPublic {
            user_id: member_row.user_id,
            username: user.username,
            role: invite_role,
            created_at: member_row.created_at,
        },
    }))
}

async fn accept_repo(
    ctx: &mut RpcCtx,
    token: &str,
    email: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<InvitesAcceptResponse, AppError> {
    let row = ctx
        .db
        .find_repo_invite_by_token_hash(&sha256_hex(token.as_bytes()))
        .await
        .map_err(db_err)?
        .ok_or_else(invalid_invite)?;

    if not_acceptable_reason(
        row.expires_at.as_deref(),
        row.revoked_at.as_deref(),
        row.max_uses,
        row.use_count,
    )
    .is_some()
    {
        return Err(invalid_invite());
    }

    let permission = CollaboratorPermission::parse(&row.permission).map_err(|_| {
        AppError::new("repo.internal", "repository operation failed")
    })?;

    let repo = ctx
        .db
        .find_repository_by_id(&row.repository_id)
        .await
        .map_err(db_err)?
        .ok_or_else(invalid_invite)?;
    if repo.deleted_at.is_some() {
        return Err(invalid_invite());
    }

    let owner_slug = if repo.owner_type.eq_ignore_ascii_case("org") {
        ctx.db
            .find_organization_by_id(&repo.owner_id)
            .await
            .map_err(db_err)?
            .map(|o| o.slug)
            .ok_or_else(invalid_invite)?
    } else {
        ctx.db
            .find_user_by_id(&repo.owner_id)
            .await
            .map_err(db_err)?
            .map(|u| u.username)
            .ok_or_else(invalid_invite)?
    };

    let user = resolve_or_provision_invitee(
        ctx,
        row.email.as_deref(),
        email,
        username,
        password,
    )
    .await?;

    if let Some(existing) = ctx
        .db
        .find_repo_collaborator(&repo.id, &user.id)
        .await
        .map_err(db_err)?
    {
        // Already has a grant — do not burn a link seat; keep existing permission.
        let existing_perm = CollaboratorPermission::parse(&existing.permission).map_err(|_| {
            AppError::new("repo.internal", "repository operation failed")
        })?;
        return Ok(InvitesAcceptResponse::Repo {
            owner: owner_slug,
            name: repo.name,
            permission: existing_perm,
        });
    }

    // Personal owner already has full access — no seat consumed.
    if repo.owner_type.eq_ignore_ascii_case("user") && repo.owner_id == user.id {
        return Ok(InvitesAcceptResponse::Repo {
            owner: owner_slug,
            name: repo.name,
            permission: CollaboratorPermission::Admin,
        });
    }

    // Consume a seat before granting access so a race cannot over-grant.
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .consume_repo_invite(&row.id, &now)
        .await
        .map_err(|e| {
            if e == "repo invite not found" {
                invalid_invite()
            } else {
                db_err(e)
            }
        })?;

    ctx.db
        .insert_repo_collaborator(&repo.id, &user.id, permission.as_str())
        .await
        .map_err(db_err)?;

    crate::audit::record(
        ctx,
        Some((&user.id, &user.username)),
        "invite.accept",
        Some(("invite", &row.id)),
        Some(serde_json::json!({ "kind": "repo", "repo": format!("{owner_slug}/{}", repo.name) }).to_string()),
    )
    .await;

    Ok(InvitesAcceptResponse::Repo {
        owner: owner_slug,
        name: repo.name,
        permission,
    })
}

/// `invites.accept` — try instance_invites, then organization_invites, then repository_invites.
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

    let email = req.email.as_deref();
    let username = req.username.as_deref();
    let password = req.password.as_deref();

    if let Some(resp) = accept_instance(ctx, token, email, username, password).await? {
        return Ok(resp);
    }
    if let Some(resp) = accept_org(ctx, token, email, username, password).await? {
        return Ok(resp);
    }
    accept_repo(ctx, token, email, username, password).await
}

/// `invites.get` — anonymous-safe preview: scope, bound email, expiry, seats.
/// Unknown tokens return `invite.invalid` (same message as accept).
pub async fn get(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<InvitesGetResponse, AppError> {
    let req: InvitesGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid invites.get input: {e}"),
        )
    })?;
    let token = req.token.trim();
    if token.is_empty() || token.len() != TOKEN_HEX_LEN {
        return Err(invalid_invite());
    }
    let token_hash = sha256_hex(token.as_bytes());

    if let Some(row) = ctx
        .db
        .find_instance_invite_by_token_hash(&token_hash)
        .await
        .map_err(db_err)?
    {
        let reason = not_acceptable_reason(
            row.expires_at.as_deref(),
            row.revoked_at.as_deref(),
            row.max_uses,
            row.use_count,
        );
        return Ok(InvitesGetResponse {
            kind: "instance".into(),
            email: row.email,
            expires_at: row.expires_at,
            seats_remaining: seats_remaining(row.max_uses, row.use_count),
            org_slug: None,
            org_display_name: None,
            repo_owner: None,
            repo_name: None,
            grant: None,
            acceptable: reason.is_none(),
            reason: reason.map(|s| s.to_string()),
        });
    }

    if let Some(row) = ctx
        .db
        .find_org_invite_by_token_hash(&token_hash)
        .await
        .map_err(org_db_err)?
    {
        let org = ctx
            .db
            .find_organization_by_id(&row.org_id)
            .await
            .map_err(org_db_err)?
            .ok_or_else(invalid_invite)?;
        let reason = not_acceptable_reason(
            row.expires_at.as_deref(),
            row.revoked_at.as_deref(),
            row.max_uses,
            row.use_count,
        );
        return Ok(InvitesGetResponse {
            kind: "org".into(),
            email: row.email,
            expires_at: row.expires_at,
            seats_remaining: seats_remaining(row.max_uses, row.use_count),
            org_slug: Some(org.slug),
            org_display_name: Some(org.display_name),
            repo_owner: None,
            repo_name: None,
            grant: Some(row.role),
            acceptable: reason.is_none(),
            reason: reason.map(|s| s.to_string()),
        });
    }

    let row = ctx
        .db
        .find_repo_invite_by_token_hash(&token_hash)
        .await
        .map_err(db_err)?
        .ok_or_else(invalid_invite)?;

    let repo = ctx
        .db
        .find_repository_by_id(&row.repository_id)
        .await
        .map_err(db_err)?
        .ok_or_else(invalid_invite)?;
    if repo.deleted_at.is_some() {
        return Err(invalid_invite());
    }
    let owner_slug = if repo.owner_type.eq_ignore_ascii_case("org") {
        ctx.db
            .find_organization_by_id(&repo.owner_id)
            .await
            .map_err(db_err)?
            .map(|o| o.slug)
            .ok_or_else(invalid_invite)?
    } else {
        ctx.db
            .find_user_by_id(&repo.owner_id)
            .await
            .map_err(db_err)?
            .map(|u| u.username)
            .ok_or_else(invalid_invite)?
    };
    let reason = not_acceptable_reason(
        row.expires_at.as_deref(),
        row.revoked_at.as_deref(),
        row.max_uses,
        row.use_count,
    );
    Ok(InvitesGetResponse {
        kind: "repo".into(),
        email: row.email,
        expires_at: row.expires_at,
        seats_remaining: seats_remaining(row.max_uses, row.use_count),
        org_slug: None,
        org_display_name: None,
        repo_owner: Some(owner_slug),
        repo_name: Some(repo.name),
        grant: Some(row.permission),
        acceptable: reason.is_none(),
        reason: reason.map(|s| s.to_string()),
    })
}
