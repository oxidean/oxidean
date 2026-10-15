//! Empty-instance bootstrap status + one-time setup wizard (AUTH-07)
//! + ENV forced credential confirm (AUTH-06).

use std::sync::{Mutex, MutexGuard};

use chrono::Utc;
use uuid::Uuid;

use oxidean_core::{
    validate_username, AppError, BootstrapSetupRequest, BootstrapStatus,
    ConfirmAdminCredentialsRequest, Role, UserPublic,
};
use oxidean_db::Database;

use crate::auth::local::{issue_session, user_to_public};
use crate::auth::password::{hash_password_str, PasswordError, MIN_PASSWORD_LEN};
use crate::rpc::{CookieChange, RpcCtx};

/// Default username assigned by ENV seed — must be changed on confirm (D-15/D-16).
pub const SEEDED_ADMIN_USERNAME: &str = "system-administrator";

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("bootstrap db error: {e}");
        AppError::new("auth.internal", "bootstrap operation failed")
    }
}

fn map_username_err(msg: String) -> AppError {
    if msg.contains("reserved") {
        AppError::new("auth.reserved_username", "username is reserved")
    } else {
        AppError::new("auth.invalid_username", msg)
    }
}

/// Serialize integration tests that mutate `OXIDEAN_ADMIN_*`.
pub fn lock_admin_env_for_tests() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// True when both admin ENV vars are non-empty (AUTH-06 seed path).
pub fn admin_env_configured() -> bool {
    let email_ok = std::env::var("OXIDEAN_ADMIN_EMAIL")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let password_ok = std::env::var("OXIDEAN_ADMIN_PASSWORD")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    email_ok && password_ok
}

/// `needs_setup` when the users table is empty and ENV seed is not configured.
///
/// Skipped / unconfigured databases are not empty-instance wizard targets — return
/// `false` so health/echo smoke paths and `Database::skipped()` test routers keep
/// working. Real empty SQLite/Postgres/MySQL pools still report `true` (D-11).
pub async fn needs_setup(db: &Database) -> Result<bool, AppError> {
    if admin_env_configured() {
        return Ok(false);
    }
    match db.count_users().await {
        Ok(count) => Ok(count == 0),
        Err(e) if e.contains("not configured") => Ok(false),
        Err(e) => Err(db_err(e)),
    }
}

/// Public bootstrap status for the SPA gate.
pub async fn bootstrap_status(ctx: &RpcCtx) -> Result<BootstrapStatus, AppError> {
    Ok(BootstrapStatus {
        needs_setup: needs_setup(&ctx.db).await?,
    })
}

/// Create the first `sys-admin` via the interactive wizard (AUTH-07).
/// Auto-verified; issues a session cookie. Rejects when setup is no longer available.
pub async fn bootstrap_setup(
    ctx: &mut RpcCtx,
    input: serde_json::Value,
) -> Result<UserPublic, AppError> {
    if !needs_setup(&ctx.db).await? {
        return Err(AppError::new(
            "auth.setup_unavailable",
            "Setup is no longer available for this instance.",
        ));
    }

    let req: BootstrapSetupRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid bootstrap_setup input: {e}"),
        )
    })?;

    let email = req.email.trim().to_ascii_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::new(
            "auth.invalid_email",
            "enter a valid email address",
        ));
    }

    let username = req.username.trim().to_string();
    // Do not bypass reserved usernames here: names like `admin` collide with
    // first-party UI routes (`/admin/auth`, `/admin/lfs`, …) so `/{owner}/{repo}`
    // never resolves for that account. ENV seed still uses direct DB create
    // with `system-administrator` (also reserved, but not a route prefix).
    validate_username(&username).map_err(map_username_err)?;

    let password_hash = hash_password_str(&req.password).map_err(|e| match e {
        PasswordError::TooShort => AppError::new(
            "auth.weak_password",
            format!("password must be at least {MIN_PASSWORD_LEN} characters"),
        ),
        PasswordError::Hash(_) => {
            tracing::error!("bootstrap password hash failed");
            AppError::new("auth.internal", "authentication failed")
        }
    })?;

    // Re-check emptiness under the create path (race with concurrent setup).
    if ctx.db.count_users().await.map_err(db_err)? > 0 {
        return Err(AppError::new(
            "auth.setup_unavailable",
            "Setup is no longer available for this instance.",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let display_name = username.clone();
    let row = ctx
        .db
        .create_user(
            &id,
            &email,
            &username,
            Some(&password_hash),
            &display_name,
            "",
            None,
            Role::SysAdmin,
        )
        .await
        .map_err(db_err)?;

    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .set_email_verified_at(&id, &now)
        .await
        .map_err(db_err)?;

    // Persist wizard auth stack + allow_signup; keep email_provider/from_address.
    let settings = ctx.db.get_auth_settings().await.map_err(db_err)?;
    let provider_mode = match req.provider_mode {
        oxidean_core::ProviderMode::Local => "local",
        oxidean_core::ProviderMode::Workos => "workos",
        oxidean_core::ProviderMode::Oidc => "oidc",
    };
    let oidc_issuer = req
        .oidc_issuer
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(settings.oidc_issuer.as_deref());
    let oidc_client_id = req
        .oidc_client_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(settings.oidc_client_id.as_deref());
    let workos_client_id = req
        .workos_client_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(settings.workos_client_id.as_deref());
    ctx.db
        .update_auth_settings(
            provider_mode,
            &settings.email_provider,
            settings.from_address.as_deref(),
            oidc_issuer,
            oidc_client_id,
            workos_client_id,
            req.allow_signup,
            &settings.default_visibility,
            settings.require_two_factor_local,
            settings.require_two_factor_external,
        )
        .await
        .map_err(db_err)?;

    let cookie = issue_session(ctx, &row.id, false).await?;
    ctx.set_cookie = Some(CookieChange::Set(cookie));

    tracing::info!(
        username = %username,
        email = %email,
        allow_signup = req.allow_signup,
        provider_mode = %provider_mode,
        "created initial sys-admin via /setup wizard (AUTH-07)"
    );

    // Reload so email_verified_at is reflected in UserPublic.
    let row = ctx
        .db
        .find_user_by_id(&id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.internal", "bootstrap user missing after create"))?;
    Ok(user_to_public(&ctx.db, &row).await)
}

/// Clear `must_change_credentials` after ENV seed forced-change (D-16/D-17).
/// Session required; rejects default username `system-administrator` (case-insensitive).
/// ENV email may be kept; `keep_password` skips password rewrite.
pub async fn confirm_admin_credentials(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<UserPublic, AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new(
            "auth.unauthenticated",
            "not authenticated",
        ));
    };

    let req: ConfirmAdminCredentialsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid confirm_admin_credentials input: {e}"),
        )
    })?;

    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;

    if !user.must_change_credentials {
        return Err(AppError::new(
            "auth.credentials_not_required",
            "credential confirmation is not required for this account",
        ));
    }

    let username = req.username.trim().to_string();
    if username.eq_ignore_ascii_case(SEEDED_ADMIN_USERNAME) {
        return Err(AppError::new(
            "auth.invalid_username",
            "must change the default system-administrator username",
        ));
    }
    validate_username(&username).map_err(map_username_err)?;

    if username != user.username {
        if let Some(other) = ctx
            .db
            .find_user_by_username(&username)
            .await
            .map_err(db_err)?
        {
            if other.id != user.id {
                return Err(AppError::new(
                    "auth.taken",
                    "email or username already taken",
                ));
            }
        } else if ctx
            .db
            .find_organization_by_slug(&username)
            .await
            .map_err(db_err)?
            .is_some()
        {
            return Err(AppError::new(
                "auth.taken",
                "email or username already taken",
            ));
        }
    }

    let email = match req.email.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(e) => e.to_ascii_lowercase(),
        None => user.email.clone(),
    };
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::new(
            "auth.invalid_email",
            "enter a valid email address",
        ));
    }
    if email != user.email {
        if let Some(other) = ctx.db.find_user_by_email(&email).await.map_err(db_err)? {
            if other.id != user.id {
                return Err(AppError::new(
                    "auth.taken",
                    "email or username already taken",
                ));
            }
        }
        ctx.db
            .update_user_email(&user.id, &email)
            .await
            .map_err(db_err)?;
    }

    if !req.keep_password {
        let password = req
            .password
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::new(
                    "auth.weak_password",
                    format!("password must be at least {MIN_PASSWORD_LEN} characters"),
                )
            })?;
        let password_hash = hash_password_str(password).map_err(|e| match e {
            PasswordError::TooShort => AppError::new(
                "auth.weak_password",
                format!("password must be at least {MIN_PASSWORD_LEN} characters"),
            ),
            PasswordError::Hash(_) => {
                tracing::error!("confirm_admin_credentials password hash failed");
                AppError::new("auth.internal", "authentication failed")
            }
        })?;
        ctx.db
            .set_password_hash(&user.id, &password_hash)
            .await
            .map_err(db_err)?;
    }

    let display_name = if user.display_name == user.username
        || user
            .display_name
            .eq_ignore_ascii_case(SEEDED_ADMIN_USERNAME)
    {
        username.clone()
    } else {
        user.display_name.clone()
    };

    if username != user.username {
        crate::git::rename_owner_repos_dir(&ctx.repos_dir, &user.username, &username).await?;
    }

    if let Err(e) = ctx
        .db
        .update_user_profile(
            &user.id,
            &display_name,
            &username,
            &user.bio,
            user.avatar_path.as_deref(),
        )
        .await
    {
        if username != user.username {
            let _ =
                crate::git::rename_owner_repos_dir(&ctx.repos_dir, &username, &user.username).await;
        }
        return Err(db_err(e));
    }

    let updated = ctx
        .db
        .clear_must_change_credentials(&user.id)
        .await
        .map_err(db_err)?;

    tracing::info!(
        user_id = %user.id,
        username = %username,
        "cleared must_change_credentials after ENV admin confirm (AUTH-06)"
    );

    Ok(user_to_public(&ctx.db, &updated).await)
}
