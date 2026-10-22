//! Email verify + password-reset token issue/consume — magic+OTP, resend, rate limits.

use chrono::Utc;
use oxidean_core::AppError;
use oxidean_db::{Database, UserRow};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::email::OutboundEmail;
use crate::rpc::RpcCtx;

pub const PURPOSE_VERIFY: &str = "verify";
pub const PURPOSE_RESET: &str = "reset";
/// Pre-session login OTP issued inside a pending 2FA challenge (#130).
pub const PURPOSE_LOGIN_2FA: &str = "login_2fa";
pub const TOKEN_BYTES: usize = 32;
pub const OTP_DIGITS: u32 = 100_000_000; // 8-digit numeric
pub const TTL_SECS: i64 = 30 * 60;
pub const MIN_ISSUE_INTERVAL_SECS: i64 = 60;
pub const MAX_ISSUES_PER_HOUR: i32 = 5;
pub const MAX_REDEEM_ATTEMPTS: i32 = 10;
pub const VERIFY_SUBJECT: &str = "Verify your Oxidean email";
pub const RESET_SUBJECT: &str = "Reset your Oxidean password";

/// Env allowlist for `auth.dev.privileged_ping` (D-10 / Open Q2 RESOLVED).
pub fn privileged_ping_env_allowed(env_name: &str) -> bool {
    matches!(env_name, "development" | "dev" | "test" | "compose") || cfg!(test)
}

pub fn db_err(e: String) -> AppError {
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

pub fn sha256_hex(data: &[u8]) -> String {
    bytes_to_hex(&Sha256::digest(data))
}

pub fn bytes_to_hex(bytes: &[u8]) -> String {
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
pub fn generate_magic() -> String {
    let mut token_bytes = [0u8; TOKEN_BYTES];
    rand::fill(&mut token_bytes);
    bytes_to_hex(&token_bytes)
}

/// 8-digit numeric OTP via rejection sampling (uniform over 00000000..=99999999).
pub fn generate_otp() -> String {
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

pub fn parse_created_at(raw: &str) -> Result<chrono::DateTime<Utc>, AppError> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
                .map_err(|_| AppError::new("auth.internal", "authentication failed"))
        })
}

pub fn rate_limited() -> AppError {
    AppError::new("auth.rate_limited", "too many emails; try again later")
}

pub fn invalid_token() -> AppError {
    AppError::new("auth.invalid_token", "invalid or expired verification code")
}

pub fn invalid_reset_token() -> AppError {
    AppError::new("auth.invalid_token", "invalid or expired reset code")
}

pub fn sso_only() -> AppError {
    AppError::new(
        "auth.sso_only",
        "This account signs in with SSO. Reset your password with your identity provider.",
    )
}

/// Identical anti-enumeration success payload (D-28).
pub fn reset_request_ok() -> serde_json::Value {
    serde_json::json!({ "ok": true })
}
/// Enforce soft rate limits and compute the next `issue_count` (D-19).
pub fn next_issue_count(
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

pub fn build_verify_email(to: &str, username: &str, magic: &str, otp: &str) -> OutboundEmail {
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

pub fn build_reset_email(to: &str, username: &str, magic: &str, otp: &str) -> OutboundEmail {
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
pub async fn issue_token_inner(
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

/// Issue an 8-digit sign-in OTP bound to `user_id` + primary email for an open
/// 2FA login challenge (#130). Rate-limited like verify/reset issuance.
pub async fn issue_2fa_token(
    db: &oxidean_db::Database,
    user_id: &str,
    target_email: &str,
) -> Result<IssuedVerifySecrets, AppError> {
    issue_token_inner(db, user_id, PURPOSE_LOGIN_2FA, target_email, true).await
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

pub async fn issue_verify_for_target_inner(
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
/// Clear `users.email_verified_at` for a user (D-05).
///
/// Call this when an email-change path lands: after updating the address, clear
/// verification and issue a new verify email via the existing verify channel.
/// Internal/db helper only this phase — not registered as a public RPC (T-05-14).
pub async fn clear_email_verification(db: &Database, user_id: &str) -> Result<UserRow, AppError> {
    db.clear_email_verified_at(user_id).await.map_err(db_err)
}
pub fn session_err(e: crate::auth::session::AuthError) -> AppError {
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

/// Issued secrets returned only to callers that need plaintext (tests / email senders).
#[derive(Debug, Clone)]
pub struct IssuedVerifySecrets {
    pub magic: String,
    pub otp: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::gate;
    use crate::auth::session::{ResolvedSession, SessionService};
    use crate::email::LogSink;
    use crate::rpc::RpcCtx;
    use chrono::Duration;
    use std::path::PathBuf;
    use std::sync::{Arc, RwLock};

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().expect("tempdir");
        let url = format!(
            "sqlite:{}",
            dir.path().join("verify_reset_clear.db").display()
        );
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
            actions_artifacts_dir: PathBuf::from("/tmp/oxidean-test-actions-artifacts"),
            actions_cache_dir: PathBuf::from("/tmp/oxidean-test-actions-cache"),
            pages_dir: PathBuf::from("/tmp/oxidean-test-pages"),
            git: Arc::new(oxidean_git::CliGitBackend::new()),
            env_name: "development".into(),
            session: Some(ResolvedSession {
                session_id: "sess-test".into(),
                user_id: user_id.to_string(),
                remember_me: false,
                expires_at: Utc::now() + Duration::hours(1),
            }),
            pat: None,
            rpc_dispatch: crate::rpc::core_dispatch(),
            client: crate::rpc::ClientMeta::default(),
            set_cookie: None,
            two_factor: crate::auth::two_factor::TwoFactorCtx::default(),
            login_begin_limiter: Arc::new(std::sync::Mutex::new(
                crate::auth::login_rate::LoginBeginLimiter::new(),
            )),
            lookup_limiter: Arc::new(std::sync::Mutex::new(
                crate::user::rate_limit::LookupLimiter::new(),
            )),
            search_timeout_ms: 8000,
            search_max_matches: 100,
            search_max_files: 50,
            vcs: None,
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
