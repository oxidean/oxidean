//! Phase 12: factory_reset_instance wipes pull domain via repository CASCADE.

use oxidean_core::Role;
use oxidean_db::Database;
use sqlx::sqlite::SqlitePoolOptions;

async fn count_table(db_path: &std::path::Path, table: &str) -> i64 {
    let url = format!("sqlite:{}", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("reconnect for count");
    let sql = format!("SELECT COUNT(*) FROM {table}");
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(&*sql))
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("count {table}: {e}"))
}

#[tokio::test]
async fn factory_reset_pulls_wipes_pull_domain() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("factory_reset_pulls.db");
    let url = format!("sqlite:{}", db_path.display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let author = db
        .create_user(
            "u-fr-pull",
            "frpull@example.com",
            "frpull",
            Some("hash"),
            "FR Pull",
            "",
            None,
            Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository(
            "repo-fr-pull",
            &author.id,
            "user",
            "core",
            "public",
            "",
            "main",
        )
        .await
        .expect("repo");
    let _ = db
        .insert_pull(
            "pull-fr-1",
            &repo.id,
            1,
            "Title",
            "body",
            &author.id,
            "main",
            "aaa",
            &repo.id,
            "feature",
            "bbb",
            false,
        )
        .await
        .expect("pull");
    let _ = db
        .insert_pull_comment(
            "pc-fr-1",
            "pull-fr-1",
            &author.id,
            "hi",
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("comment");
    let _ = db
        .insert_pull_review("prv-fr-1", "pull-fr-1", &author.id, "commented", "ok", None)
        .await
        .expect("review");

    db.factory_reset_instance()
        .await
        .expect("factory_reset_instance");

    for table in [
        "pull_requests",
        "pull_comments",
        "pull_reviews",
        "pull_review_requests",
        "pull_labels",
        "pull_assignees",
    ] {
        let n = count_table(&db_path, table).await;
        assert_eq!(n, 0, "{table} must be empty after factory_reset");
    }
}
