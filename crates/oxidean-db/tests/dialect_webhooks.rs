//! Phase 18: `0019_webhooks` tri-dialect parity + round-trip.

use oxidean_core::Role;
use oxidean_db::Database;

#[tokio::test]
async fn dialect_webhooks_migrate_schema_presence() {
    let migration_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0019_webhooks.sql"
    );
    let sql = std::fs::read_to_string(migration_path).expect("0019_webhooks.sql");
    assert!(sql.contains("webhooks"));
    assert!(sql.contains("webhook_deliveries"));
    assert!(sql.contains("webhook_delivery_attempts"));

    for dialect in ["sqlite", "postgres", "mysql"] {
        let p = format!(
            "{}/migrations/{}/0019_webhooks.sql",
            env!("CARGO_MANIFEST_DIR"),
            dialect
        );
        assert!(std::path::Path::new(&p).exists(), "missing {p}");
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("webhooks.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let user = db
        .create_user(
            "u-hook",
            "hook@example.com",
            "hookuser",
            Some("hash"),
            "Hook",
            "",
            None,
            Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository("r-hook-1", &user.id, "user", "demo", "public", "", "main")
        .await
        .expect("repo");
    let hook = db
        .insert_webhook(
            "hook-1",
            &repo.id,
            "https://example.com/hook",
            "sekrit",
            true,
            r#"["issues","push"]"#,
            "demo",
            &user.id,
        )
        .await
        .expect("insert webhook");
    assert_eq!(hook.url, "https://example.com/hook");
    assert!(hook.active);

    let delivery = db
        .insert_webhook_delivery(
            "del-1",
            &hook.id,
            "guid-1",
            "issues",
            "opened",
            r#"{"action":"opened"}"#,
        )
        .await
        .expect("delivery");
    assert_eq!(delivery.status, "pending");

    let attempt = db
        .insert_webhook_delivery_attempt(
            "att-1",
            &delivery.id,
            1,
            Some(200),
            None,
            Some(12),
            Some("ok"),
        )
        .await
        .expect("attempt");
    assert_eq!(attempt.http_status, Some(200));
}
