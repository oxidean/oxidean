//! Issue #23 repo about migration parity (homepage, watches, topics, fork_count).

use oxidean_core::Role;
use oxidean_db::Database;

#[tokio::test]
async fn dialect_repo_about_schema_presence() {
    let migration_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0023_repo_about.sql"
    );
    let sql = std::fs::read_to_string(migration_path).unwrap_or_default();
    assert!(!sql.is_empty(), "0023_repo_about.sql must exist");
    assert!(sql.contains("homepage"), "must add repositories.homepage");
    assert!(
        sql.contains("watch_count"),
        "must add repositories.watch_count"
    );
    assert!(
        sql.contains("fork_count"),
        "must add repositories.fork_count"
    );
    assert!(
        sql.contains("repository_watches"),
        "must define repository_watches"
    );
    assert!(sql.contains("topics"), "must define topics");
    assert!(
        sql.contains("repository_topics"),
        "must define repository_topics"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("about.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let _owner = db
        .create_user(
            "u-about-owner",
            "about@example.com",
            "aboutown",
            Some("hash"),
            "About Owner",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create owner after about migrate");

    let repo = db
        .insert_repository(
            "r-about-1",
            "u-about-owner",
            "user",
            "hello",
            "public",
            "",
            "main",
        )
        .await
        .expect("insert repo");
    assert_eq!(repo.id, "r-about-1");

    let homepage = db.get_repo_homepage(&repo.id).await.expect("homepage");
    assert_eq!(homepage, "");

    let fork_count = db.get_repo_fork_count(&repo.id).await.expect("fork_count");
    assert_eq!(fork_count, 0);

    let watch_count = db
        .get_repo_watch_count(&repo.id)
        .await
        .expect("watch_count");
    assert_eq!(watch_count, 0);

    let topics = db.list_repo_topics(&repo.id).await.expect("topics");
    assert!(topics.is_empty());
}
