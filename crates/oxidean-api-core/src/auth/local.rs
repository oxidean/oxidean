//! Local-auth shared helpers — `user_to_public`, `normalize_email`,
//! `map_username_err`, `db_err` stay in core (#162): `profile`, `bootstrap`,
//! `invites` and `emails` consume them across domain boundaries. The
//! `auth.{signup,login*,logout*,me,provider_config}` handlers live in
//! `oxidean-api-auth`.

use oxidean_core::{AppError, UserPublic};
use oxidean_db::{Database, UserRow};

use crate::auth::external::is_placeholder_username;

/// Convert a DB user row to the public RPC DTO (never includes password_hash).
///
/// `two_factor_*` flags reflect current enrollment + policy (#130): a session
/// user's required-state is computed from the org/instance policy plus whether
/// the account has a local password (SSO-only accounts follow the external
/// policy). Errors fail closed to `false` so a blip never claims "enrolled".
pub async fn user_to_public(db: &Database, row: &UserRow) -> UserPublic {
    let (two_factor_enabled, two_factor_enroll_required) =
        crate::auth::two_factor::flags::for_user(db, row).await;
    UserPublic {
        id: row.id.clone(),
        email: row.email.clone(),
        username: row.username.clone(),
        display_name: row.display_name.clone(),
        bio: row.bio.clone(),
        avatar_url: row.avatar_path.clone(),
        role: row.role,
        profile_incomplete: is_placeholder_username(&row.username),
        email_verified: row.email_verified_at.is_some(),
        must_change_credentials: row.must_change_credentials,
        default_branch: if row.default_branch.trim().is_empty() {
            "main".into()
        } else {
            row.default_branch.clone()
        },
        two_factor_enabled,
        two_factor_enroll_required,
    }
}

pub fn map_username_err(msg: String) -> AppError {
    if msg.contains("reserved") {
        AppError::new("auth.reserved_username", "username is reserved")
    } else {
        AppError::new("auth.invalid_username", msg)
    }
}

pub fn normalize_email(raw: &str) -> Result<String, AppError> {
    let email = raw.trim().to_ascii_lowercase();
    if email.is_empty() || !email.contains('@') || email.starts_with('@') || email.ends_with('@') {
        return Err(AppError::new("auth.invalid_email", "invalid email address"));
    }
    Ok(email)
}

pub fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new("auth.taken", "email or username already taken")
    } else {
        tracing::error!("auth db error: {e}");
        AppError::new("auth.internal", "authentication failed")
    }
}

pub fn session_err(e: crate::auth::session::AuthError) -> AppError {
    tracing::error!("session error: {e}");
    AppError::new("auth.internal", "authentication failed")
}

/// Mint a session + return the cookie (cookie applied by callers via
/// `CookieChange::Set`). Shared by `local`, `bootstrap`, `verify_reset` and
/// `two_factor` handlers across domain crates (#162).
pub async fn issue_session(
    ctx: &mut crate::rpc::RpcCtx,
    user_id: &str,
    remember_me: bool,
) -> Result<cookie::Cookie<'static>, AppError> {
    let (_token, cookie) = ctx
        .sessions
        .create(
            &ctx.db,
            user_id,
            remember_me,
            ctx.client.ip_address.as_deref(),
            ctx.client.user_agent.as_deref(),
        )
        .await
        .map_err(session_err)?;
    Ok(cookie)
}
