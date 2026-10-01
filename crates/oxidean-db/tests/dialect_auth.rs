//! DATABASE_URL-gated auth migrate + user/session round-trip.
//! CI sets DATABASE_URL per dialect leg; local runs skip when unset.

use oxidean_db::Database;

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => Some(url),
        _ => None,
    }
}

#[tokio::test]
async fn migrate_auth_and_user_round_trip() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;

    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let settings = db.get_auth_settings().await.expect("auth settings seed");
    assert_eq!(settings.provider_mode, "local");
    assert_eq!(settings.email_provider, "log");

    let user_id = "00000000-0000-4000-8000-000000000001";
    let user = db
        .create_user(
            user_id,
            "roundtrip@example.com",
            "roundtrip-user",
            Some("$argon2id$test"),
            "Round Trip",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("insert user");
    assert_eq!(user.email, "roundtrip@example.com");
    assert_eq!(user.password_hash.as_deref(), Some("$argon2id$test"));
    assert_eq!(user.role, oxidean_core::Role::User);

    let token_hash = "abc0123456789abcdef0123456789abcdef0123456789abcdef0123456789ab";
    db.create_session(
        "00000000-0000-4000-8000-0000000000aa",
        user_id,
        token_hash,
        "2099-01-01T00:00:00Z",
        false,
        None,
        None,
    )
    .await
    .expect("create session");

    let session = db
        .find_session_by_token_hash(token_hash)
        .await
        .expect("find session")
        .expect("session present");
    assert_eq!(session.user_id, user_id);

    let deleted = db
        .delete_sessions_for_user(user_id)
        .await
        .expect("delete sessions for user");
    assert!(deleted >= 1);

    let gone = db
        .find_session_by_token_hash(token_hash)
        .await
        .expect("find after delete");
    assert!(gone.is_none());
}

#[tokio::test]
async fn migrate_email_token_and_verified_helpers() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;

    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let user_id = "00000000-0000-4000-8000-000000000002";
    db.create_user(
        user_id,
        "verify-helpers@example.com",
        "verify-helpers",
        Some("$argon2id$test"),
        "Verify Helpers",
        "",
        None,
        oxidean_core::Role::User,
    )
    .await
    .expect("insert user");

    let token_hash = "def0123456789abcdef0123456789abcdef0123456789abcdef0123456789ab";
    let otp_hash = "fed0123456789abcdef0123456789abcdef0123456789abcdef0123456789ab";
    let token = db
        .upsert_email_token(
            "00000000-0000-4000-8000-0000000000bb",
            user_id,
            "verify",
            token_hash,
            otp_hash,
            "2099-06-01T12:00:00Z",
            1,
        )
        .await
        .expect("upsert email token");
    assert_eq!(token.purpose, "verify");
    assert_eq!(token.user_id, user_id);
    assert_eq!(token.attempt_count, 0);
    assert_eq!(token.issue_count, 1);

    let by_token = db
        .find_email_token_by_token_hash(token_hash)
        .await
        .expect("find by token_hash")
        .expect("token row present");
    assert_eq!(by_token.id, token.id);
    assert_eq!(by_token.otp_hash, otp_hash);

    let by_otp = db
        .find_email_token_by_otp_hash(otp_hash)
        .await
        .expect("find by otp_hash")
        .expect("otp row present");
    assert_eq!(by_otp.token_hash, token_hash);

    let verified = db
        .set_email_verified_at(user_id, "2099-06-01T12:30:00Z")
        .await
        .expect("set email_verified_at");
    assert!(verified.email_verified_at.is_some());

    let cleared = db
        .clear_email_verified_at(user_id)
        .await
        .expect("clear email_verified_at");
    assert!(cleared.email_verified_at.is_none());
}

/// Wave 0 / 06-01: `0006_bootstrap_flags` columns round-trip after migrate.
#[tokio::test]
async fn migrate_0006_bootstrap_flags_columns() {
    let migration_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0006_bootstrap_flags.sql"
    );
    let sql = std::fs::read_to_string(migration_path).unwrap_or_default();
    assert!(
        !sql.is_empty(),
        "0006_bootstrap_flags.sql must exist (allow_signup + must_change_credentials)"
    );
    assert!(
        sql.contains("allow_signup"),
        "0006 must add instance_auth_settings.allow_signup"
    );
    assert!(
        sql.contains("must_change_credentials"),
        "0006 must add users.must_change_credentials"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("bootstrap_flags.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let settings = db.get_auth_settings().await.expect("settings");
    assert!(
        !settings.allow_signup,
        "after migrate, allow_signup default false must be readable"
    );

    let updated = db
        .update_auth_settings(
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
        .expect("set allow_signup");
    assert!(updated.allow_signup);

    let user = db
        .create_user(
            "u-bootstrap-flags",
            "flags@example.com",
            "flaguser",
            Some("hash"),
            "Flag User",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("insert user");
    assert!(
        !user.must_change_credentials,
        "must_change_credentials defaults false"
    );

    let flagged = db
        .set_must_change_credentials(&user.id, true)
        .await
        .expect("set must_change");
    assert!(flagged.must_change_credentials);

    let cleared = db
        .clear_must_change_credentials(&user.id)
        .await
        .expect("clear must_change");
    assert!(!cleared.must_change_credentials);

    let renamed = db
        .update_user_email(&user.id, "flags-renamed@example.com")
        .await
        .expect("update email");
    assert_eq!(renamed.email, "flags-renamed@example.com");

    // Primary mirror stays in sync on rename.
    let primary = db
        .list_user_emails(&user.id)
        .await
        .expect("list emails")
        .into_iter()
        .find(|e| e.is_primary)
        .expect("primary row");
    assert_eq!(primary.email, "flags-renamed@example.com");
}

#[tokio::test]
async fn user_emails_backfill_uniqueness_and_primary_sync() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("user_emails.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let a = db
        .create_user(
            "u-emails-a",
            "alice@example.com",
            "alice-emails",
            Some("hash"),
            "Alice",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("user a");
    let emails = db.list_user_emails(&a.id).await.expect("list a");
    assert_eq!(emails.len(), 1);
    assert!(emails[0].is_primary);
    assert_eq!(emails[0].email, "alice@example.com");

    let now = "2026-01-01T00:00:00Z";
    db.set_email_verified_at(&a.id, now)
        .await
        .expect("verify a");
    let by_email = db
        .find_user_by_email("alice@example.com")
        .await
        .expect("find")
        .expect("present");
    assert_eq!(by_email.id, a.id);

    let secondary = db
        .create_user_email("sec-a", &a.id, "alice-work@example.com", false, Some(now))
        .await
        .expect("secondary");
    let found = db
        .find_user_by_email("alice-work@example.com")
        .await
        .expect("find secondary")
        .expect("present");
    assert_eq!(found.id, a.id);

    let b = db
        .create_user(
            "u-emails-b",
            "bob@example.com",
            "bob-emails",
            Some("hash"),
            "Bob",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("user b");
    let taken = db
        .create_user_email("sec-b-taken", &b.id, "alice@example.com", false, None)
        .await;
    assert!(taken.is_err(), "email must be unique across users");

    let promoted = db
        .set_user_email_primary(&secondary.id)
        .await
        .expect("set primary");
    assert!(promoted.is_primary);
    assert_eq!(promoted.email, "alice-work@example.com");
    let mirrored = db.find_user_by_id(&a.id).await.expect("find").expect("user");
    assert_eq!(mirrored.email, "alice-work@example.com");
    assert!(mirrored.email_verified_at.is_some());
}
