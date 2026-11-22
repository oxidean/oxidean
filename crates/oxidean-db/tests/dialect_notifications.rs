//! 17-01: `0018_notifications` migration parity across dialects.

use oxidean_core::Role;
use oxidean_db::Database;

#[tokio::test]
async fn dialect_notifications_migration_module_present() {
    let sqlite = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0018_notifications.sql"
    );
    let postgres = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/postgres/0018_notifications.sql"
    );
    let mysql = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/mysql/0018_notifications.sql"
    );
    for path in [sqlite, postgres, mysql] {
        let sql = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            !sql.is_empty(),
            "notifications migration must exist at {path}"
        );
        assert!(
            sql.contains("notifications"),
            "{path} must define notifications table"
        );
        assert!(
            sql.contains("recipient_id"),
            "{path} must include recipient_id"
        );
        assert!(
            sql.contains("read_at"),
            "{path} must include read_at (null = unread)"
        );
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("notif.db").display());
    let db = Database::connect_as(&url, oxidean_db::Dialect::Sqlite)
        .await
        .expect("connect");
    db.migrate().await.expect("migrate 0018_notifications");
}

/// DEBT-06: `0044_notification_subject_kinds` — widened subject_kind CHECK,
/// `subject_ref`, and `action_runs.completion_notified` across dialects.
#[tokio::test]
async fn dialect_notification_subject_kinds_migration_present() {
    let sqlite = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0044_notification_subject_kinds.sql"
    );
    let postgres = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/postgres/0044_notification_subject_kinds.sql"
    );
    let mysql = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/mysql/0044_notification_subject_kinds.sql"
    );
    for path in [sqlite, postgres, mysql] {
        let sql = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            !sql.is_empty(),
            "subject-kinds migration must exist at {path}"
        );
        assert!(sql.contains("subject_ref"), "{path} must add subject_ref");
        for kind in ["release", "workflow_run", "push"] {
            assert!(
                sql.contains(kind),
                "{path} must widen subject_kind to include {kind}"
            );
        }
        assert!(
            sql.contains("completion_notified"),
            "{path} must add action_runs.completion_notified"
        );
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("kinds.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate()
        .await
        .expect("migrate 0044_notification_subject_kinds");

    // New subject kinds + subject_ref must actually persist post-migration.
    let user = db
        .create_user(
            "u-kinds",
            "k@ex.com",
            "kuser",
            Some("hash"),
            "K",
            "",
            None,
            Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository("r-kinds", &user.id, "user", "krepo", "public", "", "main")
        .await
        .expect("repo");
    let n = db
        .insert_notification(
            "n-rel",
            &user.id,
            &user.id,
            "release_published",
            "release",
            &repo.id,
            0,
            "v1",
            Some("v1.0.0"),
        )
        .await
        .expect("insert release notification");
    assert_eq!(n.subject_ref.as_deref(), Some("v1.0.0"));
    let n = db
        .insert_notification(
            "n-wf",
            &user.id,
            &user.id,
            "workflow_run_success",
            "workflow_run",
            &repo.id,
            0,
            "run",
            Some("run-1"),
        )
        .await
        .expect("insert workflow_run notification");
    assert_eq!(n.subject_ref.as_deref(), Some("run-1"));
    db.insert_notification(
        "n-push",
        &user.id,
        &user.id,
        "push",
        "push",
        &repo.id,
        0,
        "push",
        Some("refs/heads/main"),
    )
    .await
    .expect("insert push notification");

    // Rejected kinds still rejected by the rebuilt CHECK.
    let bad = db
        .insert_notification(
            "n-bogus", &user.id, &user.id, "x", "bogus", &repo.id, 0, "t", None,
        )
        .await;
    assert!(bad.is_err(), "unknown subject_kind must violate CHECK");
}

/// NOT-01: `list_primary_verified_emails_for_users` — batch address
/// resolution for notification fan-out. Unconditional sqlite coverage;
/// the same matrix runs per-dialect in `dialect_auth.rs` when
/// `DATABASE_URL` is set.
#[tokio::test]
async fn list_primary_verified_emails_for_users_sqlite() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("notif-mail.db").display());
    let db = Database::connect_as(&url, oxidean_db::Dialect::Sqlite)
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");

    assert!(db
        .list_primary_verified_emails_for_users(&[])
        .await
        .expect("empty input")
        .is_empty());

    let now = "2026-01-01T00:00:00Z";
    let a = db
        .create_user(
            "u-pv-a",
            "a@example.com",
            "pv-alice",
            Some("h"),
            "A",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("user a");
    let b = db
        .create_user(
            "u-pv-b",
            "b@example.com",
            "pv-bob",
            Some("h"),
            "B",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("user b");
    let c = db
        .create_user(
            "u-pv-c",
            "c@example.com",
            "pv-carol",
            Some("h"),
            "C",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("user c");

    // a + c: verified primary (create_user mirrors it unverified first).
    db.set_user_email_verified_at(&format!("{}-primary-email", a.id), Some(now))
        .await
        .expect("verify a");
    db.set_user_email_verified_at(&format!("{}-primary-email", c.id), Some(now))
        .await
        .expect("verify c");
    // c also gets a verified secondary — must not be returned.
    db.create_user_email(
        "u-pv-c-sec",
        &c.id,
        "carol-work@example.com",
        false,
        Some(now),
    )
    .await
    .expect("secondary");

    let out = db
        .list_primary_verified_emails_for_users(&[
            a.id.clone(),
            b.id.clone(),
            c.id.clone(),
            "u-missing".to_string(),
        ])
        .await
        .expect("batch lookup");
    let map: std::collections::HashMap<String, String> = out.into_iter().collect();
    assert_eq!(
        map.get(&a.id).map(String::as_str),
        Some("a@example.com"),
        "verified primary resolves"
    );
    assert!(
        !map.contains_key(&b.id),
        "unverified primary must be skipped"
    );
    assert_eq!(
        map.get(&c.id).map(String::as_str),
        Some("c@example.com"),
        "primary wins over a verified secondary"
    );
    assert_eq!(map.len(), 2);
}
