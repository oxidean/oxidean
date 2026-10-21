//! `auth.{verify,request_verify,resend_verify,request_password_reset,
//! reset_password,dev.privileged_ping}` handlers — extracted from
//! `core::auth::verify_reset` (#162). Token-issue machinery stays in core
//! (`emails`/`packages` consume it across domain boundaries).

use chrono::Utc;
use oxidean_api_core::auth::gate;
use oxidean_api_core::auth::local::{normalize_email, user_to_public};
use oxidean_api_core::auth::password::{hash_password_str, PasswordError, MIN_PASSWORD_LEN};
use oxidean_api_core::rpc::{CookieChange, RpcCtx};
use oxidean_core::{AppError, UserPublic};
use serde::Deserialize;

use oxidean_api_core::auth::verify_reset::*;

#[derive(Debug, Deserialize)]
pub struct RequestPasswordResetRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    pub password: String,
}

pub async fn request_password_reset(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let req: RequestPasswordResetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid request_password_reset input: {e}"),
        )
    })?;
    let email = normalize_email(&req.email)?;

    let user = ctx.db.find_user_by_email(&email).await.map_err(db_err)?;

    if let Some(user) = user {
        if user.password_hash.is_some() {
            match issue_token_inner(&ctx.db, &user.id, PURPOSE_RESET, "", true).await {
                Ok(secrets) => {
                    let msg = build_reset_email(
                        &user.email,
                        &user.username,
                        &secrets.magic,
                        &secrets.otp,
                    );
                    if let Err(e) = ctx.email.send(msg).await {
                        tracing::error!(error = %e, "reset email send failed");
                    }
                }
                Err(e) if e.code == "auth.rate_limited" => {
                    // Swallow into identical success (T-05-09 / D-28).
                    tracing::debug!("password reset rate limited; returning ok");
                }
                Err(e) => return Err(e),
            }
        }
        // SSO-only (password_hash null): no mail, same success.
    }
    // Unknown email: no mail, same success.

    Ok(reset_request_ok())
}

async fn require_session_user(ctx: &RpcCtx) -> Result<oxidean_db::UserRow, AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new("auth.unauthenticated", "not authenticated"));
    };
    ctx.db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

/// Signed-in: issue + send verify email (`auth.request_verify`).
pub async fn request_verify(ctx: &RpcCtx) -> Result<serde_json::Value, AppError> {
    let user = require_session_user(ctx).await?;
    issue_and_send_verify(ctx, &user.id, &user.email, &user.username).await?;
    Ok(serde_json::json!({ "ok": true }))
}

/// Signed-in: replace prior issuance + send (`auth.resend_verify`) — same as request (D-19).
pub async fn resend_verify(ctx: &RpcCtx) -> Result<serde_json::Value, AppError> {
    request_verify(ctx).await
}
/// Consume verify magic token or OTP while signed in as the target user (D-21).
pub async fn verify(ctx: &RpcCtx, input: serde_json::Value) -> Result<UserPublic, AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new("auth.unauthenticated", "not authenticated"));
    };

    let req: VerifyRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid verify input: {e}")))?;

    let code = req.code.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let token = req
        .token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if code.is_none() && token.is_none() {
        return Err(AppError::new("rpc.bad_input", "provide token or code"));
    }

    // Resolve by hash so secondary-address tokens (non-empty target_email) redeem.
    let row = if let Some(token) = token {
        ctx.db
            .find_email_token_by_token_hash(&sha256_hex(token.as_bytes()))
            .await
            .map_err(db_err)?
    } else if let Some(code) = code {
        if code.len() != 8 || !code.chars().all(|c| c.is_ascii_digit()) {
            None
        } else {
            ctx.db
                .find_email_token_by_otp_hash(&sha256_hex(code.as_bytes()))
                .await
                .map_err(db_err)?
        }
    } else {
        None
    };

    let Some(row) = row else {
        // Wrong OTP/magic: attribute attempts to the session user's primary
        // verify token (T-05-06). Fall back to legacy empty target.
        bump_verify_attempt_on_miss(ctx, &session.user_id).await?;
        return Err(invalid_token());
    };

    if row.user_id != session.user_id || row.purpose != PURPOSE_VERIFY {
        return Err(invalid_token());
    }

    let expires_at = chrono::DateTime::parse_from_rfc3339(&row.expires_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| invalid_token())?;
    if expires_at <= Utc::now() {
        let _ = ctx.db.delete_email_token(&row.id).await;
        return Err(invalid_token());
    }

    // Token/OTP already matched via hash lookup above.
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let target = row.target_email.trim().to_ascii_lowercase();
    if target.is_empty() {
        // Legacy primary verify token (empty target).
        ctx.db
            .set_email_verified_at(&session.user_id, &now)
            .await
            .map_err(db_err)?;
    } else if let Some(addr) = ctx
        .db
        .find_user_email_by_address(&target)
        .await
        .map_err(db_err)?
    {
        if addr.user_id != session.user_id {
            return Err(invalid_token());
        }
        ctx.db
            .set_user_email_verified_at(&addr.id, Some(&now))
            .await
            .map_err(db_err)?;
        if addr.is_primary {
            ctx.db
                .set_email_verified_at(&session.user_id, &now)
                .await
                .map_err(db_err)?;
        }
    } else {
        return Err(invalid_token());
    }

    ctx.db.delete_email_token(&row.id).await.map_err(db_err)?;

    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    Ok(user_to_public(&ctx.db, &user).await)
}

/// On OTP/magic miss, increment attempts on the session user's primary verify
/// token (or legacy empty-target row) and invalidate at the cap.
async fn bump_verify_attempt_on_miss(ctx: &RpcCtx, user_id: &str) -> Result<(), AppError> {
    let user = ctx.db.find_user_by_id(user_id).await.map_err(db_err)?;
    let Some(user) = user else {
        return Ok(());
    };
    let primary = user.email.trim().to_ascii_lowercase();
    let row = match ctx
        .db
        .find_email_token_by_user_purpose_target(user_id, PURPOSE_VERIFY, &primary)
        .await
        .map_err(db_err)?
    {
        Some(r) => Some(r),
        None => ctx
            .db
            .find_email_token_by_user_purpose_target(user_id, PURPOSE_VERIFY, "")
            .await
            .map_err(db_err)?,
    };
    let Some(row) = row else {
        return Ok(());
    };
    let attempts = ctx
        .db
        .increment_email_token_attempts(&row.id)
        .await
        .map_err(db_err)?;
    if attempts >= MAX_REDEEM_ATTEMPTS {
        let _ = ctx.db.delete_email_token(&row.id).await;
    }
    Ok(())
}

/// Dev/test privileged RPC used to prove `require_verified` (D-10).
pub async fn privileged_ping(ctx: &RpcCtx) -> Result<serde_json::Value, AppError> {
    let _user = gate::require_verified(ctx).await?;
    Ok(serde_json::json!({ "ok": true }))
}
/// Logged-out password reset redeem (D-26, D-27): set hash, revoke others, sign in.
pub async fn reset_password(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<UserPublic, AppError> {
    let req: ResetPasswordRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid reset_password input: {e}"),
        )
    })?;

    let code = req.code.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let token = req
        .token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if code.is_none() && token.is_none() {
        return Err(AppError::new("rpc.bad_input", "provide token or code"));
    }

    let password_hash = hash_password_str(&req.password).map_err(|e| match e {
        PasswordError::TooShort => AppError::new(
            "auth.weak_password",
            format!("password must be at least {MIN_PASSWORD_LEN} characters"),
        ),
        PasswordError::Hash(_) => {
            tracing::error!("password hash failed");
            AppError::new("auth.internal", "authentication failed")
        }
    })?;

    // Resolve by token_hash or otp_hash (logged-out; no session scope — D-27).
    let row = if let Some(token) = token {
        ctx.db
            .find_email_token_by_token_hash(&sha256_hex(token.as_bytes()))
            .await
            .map_err(db_err)?
    } else if let Some(code) = code {
        if code.len() != 8 || !code.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid_reset_token());
        }
        ctx.db
            .find_email_token_by_otp_hash(&sha256_hex(code.as_bytes()))
            .await
            .map_err(db_err)?
    } else {
        None
    };

    let Some(row) = row else {
        return Err(invalid_reset_token());
    };

    if row.purpose != PURPOSE_RESET {
        return Err(invalid_reset_token());
    }

    let expires_at = chrono::DateTime::parse_from_rfc3339(&row.expires_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| invalid_reset_token())?;
    if expires_at <= Utc::now() {
        let _ = ctx.db.delete_email_token(&row.id).await;
        return Err(invalid_reset_token());
    }

    // Re-check credential against row (defense in depth) + attempt cap on mismatch.
    let matches = if let Some(code) = code {
        sha256_hex(code.as_bytes()) == row.otp_hash
    } else if let Some(token) = token {
        sha256_hex(token.as_bytes()) == row.token_hash
    } else {
        false
    };

    if !matches {
        let attempts = ctx
            .db
            .increment_email_token_attempts(&row.id)
            .await
            .map_err(db_err)?;
        if attempts >= MAX_REDEEM_ATTEMPTS {
            let _ = ctx.db.delete_email_token(&row.id).await;
        }
        return Err(invalid_reset_token());
    }

    let user = ctx
        .db
        .find_user_by_id(&row.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(invalid_reset_token)?;

    if user.password_hash.is_none() {
        let _ = ctx.db.delete_email_token(&row.id).await;
        return Err(sso_only());
    }

    ctx.db
        .set_password_hash(&user.id, &password_hash)
        .await
        .map_err(db_err)?;
    ctx.db.delete_email_token(&row.id).await.map_err(db_err)?;

    // D-27: revoke all existing sessions, then mint a fresh session on this device.
    ctx.sessions
        .revoke_all(&ctx.db, &user.id)
        .await
        .map_err(session_err)?;

    // Two-factor gate (#130): a reset token proves inbox control, not the
    // enrolled factor — open a challenge instead of minting when the account
    // has a factor or a policy requires enrollment.
    let gate = oxidean_api_core::auth::two_factor::flags::login_gate(
        &ctx.db,
        &user,
        oxidean_api_core::auth::two_factor::LoginChannel::Local,
    )
    .await?;
    if gate.has_factor || gate.enroll_required {
        return Err(oxidean_api_core::auth::two_factor::open_challenge(
            ctx, &gate, &user.id, false, true,
        ));
    }

    let (_tok, cookie) = ctx
        .sessions
        .create(
            &ctx.db,
            &user.id,
            false,
            ctx.client.ip_address.as_deref(),
            ctx.client.user_agent.as_deref(),
        )
        .await
        .map_err(session_err)?;
    ctx.set_cookie = Some(CookieChange::Set(cookie));

    let user = ctx
        .db
        .find_user_by_id(&user.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.internal", "authentication failed"))?;
    Ok(user_to_public(&ctx.db, &user).await)
}
