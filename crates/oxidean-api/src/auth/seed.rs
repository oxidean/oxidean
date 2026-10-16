//! Env-seeded initial admin (AUTH-06, D-04 auto-verify, D-13–D-16).

use chrono::Utc;
use uuid::Uuid;

use crate::auth::hash_password_str;
use oxidean_db::Database;

const SEEDED_ADMIN_USERNAME: &str = "system-administrator";

/// Create a single `sys-admin` when both `OXIDEAN_ADMIN_EMAIL` and
/// `OXIDEAN_ADMIN_PASSWORD` are set and `count_users() == 0`.
///
/// Username is always `system-administrator` with `must_change_credentials=true`.
/// `OXIDEAN_ALLOW_SIGNUP` (`true`/`1`, default false) is written to
/// `instance_auth_settings`. Seeded admin is auto-verified (D-04).
pub async fn maybe_seed_admin(db: &Database) -> Result<(), String> {
    let email = match std::env::var("OXIDEAN_ADMIN_EMAIL") {
        Ok(v) if !v.trim().is_empty() => v.trim().to_ascii_lowercase(),
        _ => return Ok(()),
    };
    let password = match std::env::var("OXIDEAN_ADMIN_PASSWORD") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(()),
    };

    let count = db.count_users().await?;
    if count > 0 {
        return Ok(());
    }

    let allow_signup = std::env::var("OXIDEAN_ALLOW_SIGNUP")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let password_hash = hash_password_str(&password).map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    db.create_user(
        &id,
        &email,
        SEEDED_ADMIN_USERNAME,
        Some(&password_hash),
        SEEDED_ADMIN_USERNAME,
        "",
        None,
        oxidean_core::Role::SysAdmin,
    )
    .await?;

    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&id, &now).await?;
    db.set_must_change_credentials(&id, true).await?;

    let settings = db.get_auth_settings().await?;
    db.update_auth_settings(
        &settings.provider_mode,
        &settings.email_provider,
        settings.from_address.as_deref(),
        settings.oidc_issuer.as_deref(),
        settings.oidc_client_id.as_deref(),
        settings.workos_client_id.as_deref(),
        allow_signup,
        &settings.default_visibility,
        settings.require_two_factor_local,
        settings.require_two_factor_external,
    )
    .await?;

    tracing::info!(
        username = %SEEDED_ADMIN_USERNAME,
        email = %email,
        allow_signup,
        "seeded initial sys-admin user from OXIDEAN_ADMIN_* (AUTH-06)"
    );
    Ok(())
}
