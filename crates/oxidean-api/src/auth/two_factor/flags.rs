//! Enforcement-policy computation (#130): which factor state a user is in and
//! whether a login channel must enroll/verify before a session is minted.

use oxidean_core::AppError;
use oxidean_db::{Database, UserRow};

/// How the account authenticated in this login attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginChannel {
    /// Local email/username + password (`auth.login`).
    Local,
    /// OIDC/WorkOS SSO callback.
    External,
}

/// What the login flow must do next.
#[derive(Debug, Clone)]
pub struct LoginGate {
    /// Confirmed TOTP or ≥1 passkey exists.
    pub has_factor: bool,
    /// Policy forces enrollment before a session may be issued.
    pub enroll_required: bool,
    /// Methods a `Verify` challenge accepts (`totp`/`passkey`/`email`/`recovery`)
    /// or methods an `Enroll` challenge offers (`totp`/`passkey`).
    pub methods: Vec<String>,
}

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("two-factor db error: {e}");
        AppError::new("auth.internal", "authentication failed")
    }
}

/// Factor methods currently usable by this user (verify challenges only).
pub async fn verify_methods(db: &Database, user_id: &str) -> Result<Vec<String>, AppError> {
    let mut methods = Vec::new();
    if let Some(t) = db.totp_find(user_id).await.map_err(db_err)? {
        if t.is_confirmed() {
            methods.push("totp".to_string());
        }
    }
    if !db
        .passkey_list_for_user(user_id)
        .await
        .map_err(db_err)?
        .is_empty()
    {
        methods.push("passkey".to_string());
    }
    if db.recovery_count_remaining(user_id).await.map_err(db_err)? > 0 {
        methods.push("recovery".to_string());
    }
    // Email OTP is always offered — availability is reported honestly by
    // `email_send` when the instance cannot send mail.
    methods.push("email".to_string());
    Ok(methods)
}

/// Whether org or instance policy requires this user to hold a factor.
///
/// `channel` decides which instance flag applies; org `require_two_factor`
/// applies regardless of channel.
async fn enroll_required(
    db: &Database,
    channel: LoginChannel,
    user_id: &str,
) -> Result<bool, AppError> {
    if db
        .user_in_org_requiring_two_factor(user_id)
        .await
        .map_err(db_err)?
    {
        return Ok(true);
    }
    let settings = db.get_auth_settings().await.map_err(db_err)?;
    Ok(match channel {
        LoginChannel::Local => settings.require_two_factor_local,
        LoginChannel::External => settings.require_two_factor_external,
    })
}

/// Compute the post-primary-auth gate for a login (`auth.login` / SSO callback).
pub async fn login_gate(
    db: &Database,
    user: &UserRow,
    channel: LoginChannel,
) -> Result<LoginGate, AppError> {
    let has_factor = db
        .user_has_confirmed_factor(&user.id)
        .await
        .map_err(db_err)?;
    if has_factor {
        return Ok(LoginGate {
            has_factor: true,
            enroll_required: false,
            methods: verify_methods(db, &user.id).await?,
        });
    }
    Ok(LoginGate {
        has_factor: false,
        enroll_required: enroll_required(db, channel, &user.id).await?,
        methods: vec!["totp".into(), "passkey".into()],
    })
}

/// `(enabled, enroll_required)` flags for `UserPublic` on session views.
///
/// Channel is unknown for a live session, so identity-mode is approximated by
/// credential shape: password users follow the local policy, passwordless
/// (SSO-only) users follow the external policy. Fail-open to `false` — this
/// field is advisory chrome, the login gate is the enforcement.
pub async fn for_user(db: &Database, row: &UserRow) -> (bool, bool) {
    let enabled = db.user_has_confirmed_factor(&row.id).await.unwrap_or(false);
    if enabled {
        return (true, false);
    }
    let channel = if row.password_hash.is_some() {
        LoginChannel::Local
    } else {
        LoginChannel::External
    };
    let required = enroll_required(db, channel, &row.id).await.unwrap_or(false);
    (false, required)
}
