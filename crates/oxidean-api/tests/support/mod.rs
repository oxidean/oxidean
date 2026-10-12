//! Shared helpers for oxidean-api integration tests.

#![allow(dead_code)]

use tokio::sync::{Mutex, MutexGuard};

use oxidean_api::auth::hash_password_str;
use oxidean_db::Database;
use uuid::Uuid;

/// Serialize tests that mutate `OXIDEAN_ADMIN_*` / `OXIDEAN_ALLOW_SIGNUP` (process-wide env).
/// Async mutex: the guard is `Send` and intentionally held across `.await`s.
pub async fn lock_admin_env() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::const_new(());
    LOCK.lock().await
}

/// Create a verified `sys-admin` when the users table is empty and open local signup
/// (`allow_signup=true`) so AUTH-01-style signup tests can run post-bootstrap.
/// Avoids toggling `OXIDEAN_ADMIN_*` (racy under parallel tests).
pub async fn unlock_signup(db: &Database) {
    let count = db.count_users().await.expect("count_users");
    if count == 0 {
        let id = Uuid::new_v4().to_string();
        let email = format!("sysadmin-{id}@example.com");
        let username = format!("sys-{}", &id[..8]);
        let password_hash = hash_password_str("test-sysadmin-pass").expect("hash");
        db.create_user(
            &id,
            &email,
            &username,
            Some(&password_hash),
            &username,
            "",
            None,
            oxidean_core::Role::SysAdmin,
        )
        .await
        .expect("create sys-admin");
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        db.set_email_verified_at(&id, &now)
            .await
            .expect("verify sys-admin");
    }

    let settings = db.get_auth_settings().await.expect("auth settings");
    if !settings.allow_signup {
        db.update_auth_settings(
            &settings.provider_mode,
            &settings.email_provider,
            settings.from_address.as_deref(),
            settings.oidc_issuer.as_deref(),
            settings.oidc_client_id.as_deref(),
            settings.workos_client_id.as_deref(),
            true,
            &settings.default_visibility,
        )
        .await
        .expect("open allow_signup for tests");
    }
}
