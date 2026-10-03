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
    let db = Database::connect(&url).await.expect("connect");
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
        assert!(!sql.is_empty(), "subject-kinds migration must exist at {path}");
        assert!(
            sql.contains("subject_ref"),
            "{path} must add subject_ref"
        );
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
    db.migrate().await.expect("migrate 0044_notification_subject_kinds");

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
            "n-bogus",
            &user.id,
            &user.id,
            "x",
            "bogus",
            &repo.id,
            0,
            "t",
            None,
        )
        .await;
    assert!(bad.is_err(), "unknown subject_kind must violate CHECK");
}
