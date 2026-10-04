//! Email verify + password-reset token issue/consume — magic+OTP, resend, rate limits.

use chrono::Utc;
use oxidean_core::{AppError, UserPublic};
use oxidean_db::{Database, UserRow};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::gate;
use crate::auth::local::{normalize_email, user_to_public};
use crate::auth::password::{hash_password_str, PasswordError, MIN_PASSWORD_LEN};
use crate::email::OutboundEmail;
use crate::rpc::{CookieChange, RpcCtx};

const PURPOSE_VERIFY: &str = "verify";
const PURPOSE_RESET: &str = "reset";
const TOKEN_BYTES: usize = 32;
const OTP_DIGITS: u32 = 100_000_000; // 8-digit numeric
const TTL_SECS: i64 = 30 * 60;
const MIN_ISSUE_INTERVAL_SECS: i64 = 60;
const MAX_ISSUES_PER_HOUR: i32 = 5;
const MAX_REDEEM_ATTEMPTS: i32 = 10;
const VERIFY_SUBJECT: &str = "Verify your Oxidean email";
const RESET_SUBJECT: &str = "Reset your Oxidean password";

/// Env allowlist for `auth.dev.privileged_ping` (D-10 / Open Q2 RESOLVED).
pub fn privileged_ping_env_allowed(env_name: &str) -> bool {
    matches!(
        env_name,
        "development" | "dev" | "test" | "compose"
    ) || cfg!(test)
}

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("verify/reset db error: {e}");
        AppError::new("auth.internal", "authentication failed")
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

/// Absolute origin for magic links — `OXIDEAN_PUBLIC_ORIGIN`, with Railway
/// gateway fallback when the configured host is stale (PR Environments).
pub fn public_origin() -> String {
    crate::public_origin::resolve_public_origin()
}

/// CSPRNG 32-byte magic → lowercase hex (64 chars).
fn generate_magic() -> String {
    let mut token_bytes = [0u8; TOKEN_BYTES];
    rand::fill(&mut token_bytes);
    bytes_to_hex(&token_bytes)
}

/// 8-digit numeric OTP via rejection sampling (uniform over 00000000..=99999999).
fn generate_otp() -> String {
    let limit = (u32::MAX / OTP_DIGITS) * OTP_DIGITS;
    loop {
        let mut buf = [0u8; 4];
        rand::fill(&mut buf);
        let n = u32::from_le_bytes(buf);
        if n < limit {
            return format!("{:08}", n % OTP_DIGITS);
        }
    }
}

fn parse_created_at(raw: &str) -> Result<chrono::DateTime<Utc>, AppError> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
                .map_err(|_| {
                    AppError::new("auth.internal", "authentication failed")
                })
        })
}

fn rate_limited() -> AppError {
    AppError::new(
        "auth.rate_limited",
        "too many emails; try again later",
    )
}

fn invalid_token() -> AppError {
    AppError::new(
        "auth.invalid_token",
        "invalid or expired verification code",
    )
}

fn invalid_reset_token() -> AppError {
    AppError::new(
        "auth.invalid_token",
        "invalid or expired reset code",
    )
}

fn sso_only() -> AppError {
    AppError::new(
        "auth.sso_only",
        "This account signs in with SSO. Reset your password with your identity provider.",
    )
}

/// Identical anti-enumeration success payload (D-28).
fn reset_request_ok() -> serde_json::Value {
    serde_json::json!({ "ok": true })
}

/// Issued secrets returned only to callers that need plaintext (tests / email senders).
#[derive(Debug, Clone)]
pub struct IssuedVerifySecrets {
    pub magic: String,
    pub otp: String,
}

/// Enforce soft rate limits and compute the next `issue_count` (D-19).
fn next_issue_count(
    existing: Option<&oxidean_db::email_tokens::EmailTokenRow>,
) -> Result<i32, AppError> {
    let Some(row) = existing else {
        return Ok(1);
    };
    let created = parse_created_at(&row.created_at)?;
    let age = Utc::now().signed_duration_since(created);
    if age.num_seconds() < MIN_ISSUE_INTERVAL_SECS {
        return Err(rate_limited());
    }
    if age.num_seconds() >= 3600 {
        return Ok(1);
    }
    if row.issue_count >= MAX_ISSUES_PER_HOUR {
        return Err(rate_limited());
    }
    Ok(row.issue_count + 1)
}

fn build_verify_email(to: &str, username: &str, magic: &str, otp: &str) -> OutboundEmail {
    let origin = public_origin();
    let link = format!("{origin}/verify?token={magic}");
    let text = format!(
        "Hi {username},\n\n\
Verify your Oxidean email with this link:\n{link}\n\n\
Or enter this 8-digit code:\n{otp}\n\n\
This link and code expire in 30 minutes.\n\
If you did not create an Oxidean account, you can ignore this email.\n"
    );
    OutboundEmail {
        to: to.to_string(),
        subject: VERIFY_SUBJECT.into(),
        text,
        html: None,
    }
}

fn build_reset_email(to: &str, username: &str, magic: &str, otp: &str) -> OutboundEmail {
    let origin = public_origin();
    let link = format!("{origin}/reset-password?token={magic}");
    let text = format!(
        "Hi {username},\n\n\
Reset your Oxidean password with this link:\n{link}\n\n\
Or enter this 8-digit code:\n{otp}\n\n\
This link and code expire in 30 minutes.\n\
If you did not request a password reset, you can ignore this email.\n"
    );
    OutboundEmail {
        to: to.to_string(),
        subject: RESET_SUBJECT.into(),
        text,
        html: None,
    }
}

/// Issue (or replace) a verify token row for `user_id`. Returns plaintext magic + OTP.
///
/// Applies soft rate limits when `enforce_rate_limit` is true (RPC paths).
/// Library/test helpers may pass `false` to seed a known OTP without waiting.
async fn issue_token_inner(
    db: &oxidean_db::Database,
    user_id: &str,
    purpose: &str,
    target_email: &str,
    enforce_rate_limit: bool,
) -> Result<IssuedVerifySecrets, AppError> {
    let existing = db
        .find_email_token_by_user_purpose_target(user_id, purpose, target_email)
        .await
        .map_err(db_err)?;
    let issue_count = if enforce_rate_limit {
        next_issue_count(existing.as_ref())?
    } else {
        existing
            .as_ref()
            .map(|r| r.issue_count.saturating_add(1).max(1))
            .unwrap_or(1)
    };

    let magic = generate_magic();
    let otp = generate_otp();
    let token_hash = sha256_hex(magic.as_bytes());
    let otp_hash = sha256_hex(otp.as_bytes());
    let expires_at = (Utc::now() + chrono::Duration::seconds(TTL_SECS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let id = Uuid::new_v4().to_string();
    db.upsert_email_token_for_target(
        &id,
        user_id,
        purpose,
        target_email,
        &token_hash,
        &otp_hash,
        &expires_at,
        issue_count,
    )
    .await
    .map_err(db_err)?;
    Ok(IssuedVerifySecrets { magic, otp })
}

/// Issue (or replace) a verify token row — no rate limit (tests / internal seed).
///
/// Uses the user's primary email as `target_email` so the row replaces the
/// signup auto-issued token (same unique key).
pub async fn issue_verify(
    db: &oxidean_db::Database,
    user_id: &str,
) -> Result<IssuedVerifySecrets, AppError> {
    issue_verify_inner(db, user_id, false).await
}

/// Issue reset token without rate limit (tests / planted SSO redeem cases).
pub async fn issue_reset(
    db: &oxidean_db::Database,
    user_id: &str,
) -> Result<IssuedVerifySecrets, AppError> {
    issue_token_inner(db, user_id, PURPOSE_RESET, "", false).await
}

pub async fn issue_verify_inner(
    db: &oxidean_db::Database,
    user_id: &str,
    enforce_rate_limit: bool,
) -> Result<IssuedVerifySecrets, AppError> {
    let user = db
        .find_user_by_id(user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "user not found"))?;
    let target = user.email.trim().to_ascii_lowercase();
    issue_verify_for_target_inner(db, user_id, &target, enforce_rate_limit).await
}

async fn issue_verify_for_target_inner(
    db: &oxidean_db::Database,
    user_id: &str,
    target_email: &str,
    enforce_rate_limit: bool,
) -> Result<IssuedVerifySecrets, AppError> {
    issue_token_inner(
        db,
        user_id,
        PURPOSE_VERIFY,
        target_email,
        enforce_rate_limit,
    )
    .await
}

/// Issue (or replace) a verify token for a specific address — no rate limit (tests).
pub async fn issue_verify_for_target(
    db: &oxidean_db::Database,
    user_id: &str,
    target_email: &str,
) -> Result<IssuedVerifySecrets, AppError> {
    let target = target_email.trim().to_ascii_lowercase();
    issue_verify_for_target_inner(db, user_id, &target, false).await
}

/// Issue + send verify email with rate limits (request/resend/signup auto-send).
pub async fn issue_and_send_verify(
    ctx: &RpcCtx,
    user_id: &str,
    email: &str,
    username: &str,
) -> Result<(), AppError> {
    issue_and_send_verify_for_target(ctx, user_id, email, username).await
}

/// Issue + send verify for a specific address (primary or secondary).
pub async fn issue_and_send_verify_for_target(
    ctx: &RpcCtx,
    user_id: &str,
    email: &str,
    username: &str,
) -> Result<(), AppError> {
    let target = email.trim().to_ascii_lowercase();
    let secrets = issue_verify_for_target_inner(&ctx.db, user_id, &target, true).await?;
    let msg = build_verify_email(&target, username, &secrets.magic, &secrets.otp);
    if let Err(e) = ctx.email.send(msg).await {
        tracing::error!(error = %e, "verify email send failed");
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct RequestPasswordResetRequest {
    pub email: String,
}

/// Anonymous anti-enumeration reset request (D-24…D-28).
///
/// Always returns the same success shape. Sends mail only for local-password users.
/// Soft rate limits reuse verify policy; when limited, still return success (D-28)
/// and skip send so the response cannot enumerate accounts.
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

    let user = ctx
        .db
        .find_user_by_email(&email)
        .await
        .map_err(db_err)?;

    if let Some(user) = user {
        if user.password_hash.is_some() {
            match issue_token_inner(&ctx.db, &user.id, PURPOSE_RESET, "", true).await {
                Ok(secrets) => {
                    let msg =
                        build_reset_email(&user.email, &user.username, &secrets.magic, &secrets.otp);
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
        return Err(AppError::new(
            "auth.unauthenticated",
            "not authenticated",
        ));
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

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
}

/// Consume verify magic token or OTP while signed in as the target user (D-21).
pub async fn verify(ctx: &RpcCtx, input: serde_json::Value) -> Result<UserPublic, AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new(
            "auth.unauthenticated",
            "not authenticated",
        ));
    };

    let req: VerifyRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid verify input: {e}"))
    })?;

    let code = req
        .code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
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

    ctx.db
        .delete_email_token(&row.id)
        .await
        .map_err(db_err)?;

    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    Ok(user_to_public(&user))
}

/// On OTP/magic miss, increment attempts on the session user's primary verify
/// token (or legacy empty-target row) and invalidate at the cap.
async fn bump_verify_attempt_on_miss(ctx: &RpcCtx, user_id: &str) -> Result<(), AppError> {
    let user = ctx
        .db
        .find_user_by_id(user_id)
        .await
        .map_err(db_err)?;
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

/// Clear `users.email_verified_at` for a user (D-05).
///
/// Call this when an email-change path lands: after updating the address, clear
/// verification and issue a new verify email via the existing verify channel.
/// Internal/db helper only this phase — not registered as a public RPC (T-05-14).
pub async fn clear_email_verification(db: &Database, user_id: &str) -> Result<UserRow, AppError> {
    db.clear_email_verified_at(user_id).await.map_err(db_err)
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    pub password: String,
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

/// Logged-out password reset redeem (D-26, D-27): set hash, revoke others, sign in.
pub async fn reset_password(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<UserPublic, AppError> {
    let req: ResetPasswordRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid reset_password input: {e}"))
    })?;

    let code = req
        .code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
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
    ctx.db
        .delete_email_token(&row.id)
        .await
        .map_err(db_err)?;

    // D-27: revoke all existing sessions, then mint a fresh session on this device.
    ctx.sessions
        .revoke_all(&ctx.db, &user.id)
        .await
        .map_err(session_err)?;
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
    Ok(user_to_public(&user))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::session::{ResolvedSession, SessionService};
    use crate::email::LogSink;
    use crate::rpc::RpcCtx;
    use chrono::Duration;
    use std::path::PathBuf;
    use std::sync::{Arc, RwLock};

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().expect("tempdir");
        let url = format!("sqlite:{}", dir.path().join("verify_reset_clear.db").display());
        let db = Database::connect(&url).await.expect("connect");
        db.migrate().await.expect("migrate");
        std::mem::forget(dir);
        db
    }

    fn rpc_ctx(db: Database, user_id: &str) -> RpcCtx {
        let email: Arc<dyn crate::email::EmailSender> = Arc::new(LogSink);
        RpcCtx {
            db,
            email: email.clone(),
            email_slot: Arc::new(RwLock::new(email)),
            sessions: SessionService::new("development"),
            uploads_dir: PathBuf::from("/tmp/oxidean-test-uploads"),
            repos_dir: PathBuf::from("/tmp/oxidean-test-repos"),
            lfs_dir: PathBuf::from("/tmp/oxidean-test-lfs"),
            release_assets_dir: PathBuf::from("/tmp/oxidean-test-release-assets"),
            template_packs_dir: PathBuf::from("/tmp/oxidean-test-template-packs"),
            actions_log_dir: PathBuf::from("/tmp/oxidean-test-actions-logs"),
            git: Arc::new(oxidean_git::CliGitBackend::new()),
            env_name: "development".into(),
            session: Some(ResolvedSession {
                session_id: "sess-test".into(),
                user_id: user_id.to_string(),
                remember_me: false,
                expires_at: Utc::now() + Duration::hours(1),
            }),
            pat: None,
            client: crate::rpc::ClientMeta::default(),
            set_cookie: None,
            lookup_limiter: Arc::new(std::sync::Mutex::new(
                crate::user::rate_limit::LookupLimiter::new(),
            )),
            search_timeout_ms: 8000,
            search_max_matches: 100,
            search_max_files: 50,
        }
    }

    /// D-05: clearing verification makes require_verified fail (T-05-14 helper).
    #[tokio::test]
    async fn clear_email_verification_clears_verified_flag() {
        let db = test_db().await;
        let id = Uuid::new_v4().to_string();
        db.create_user(
            &id,
            "clear-me@ex.com",
            "clearmetest",
            Some("$argon2id$test"),
            "Clear Me",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("create_user");

        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        db.set_email_verified_at(&id, &now)
            .await
            .expect("set verified");

        let ctx = rpc_ctx(db.clone(), &id);
        gate::require_verified(&ctx)
            .await
            .expect("require_verified should pass while verified");

        let cleared = clear_email_verification(&db, &id)
            .await
            .expect("clear_email_verification");
        assert!(
            cleared.email_verified_at.is_none(),
            "clear must null email_verified_at"
        );

        let err = gate::require_verified(&ctx)
            .await
            .expect_err("require_verified must fail after clear");
        assert_eq!(err.code, "auth.email_unverified");
    }
}
