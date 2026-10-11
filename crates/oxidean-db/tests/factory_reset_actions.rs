//! Phase 19: factory_reset_instance must wipe Actions domain (D-ACT-19).

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
async fn factory_reset_actions_wipes_runners_runs_jobs_secrets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("fr-actions.db");
    let url = format!("sqlite:{}", db_path.display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let owner = db
        .create_user(
            "u-fr-act",
            "fract@example.com",
            "fract",
            Some("hash"),
            "FR Act",
            "",
            None,
            Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository(
            "r-fr-act", &owner.id, "user", "fr-act", "private", "", "main",
        )
        .await
        .expect("repo");

    db.insert_action_runner(
        "runner-fr",
        "fr-runner",
        "tokhash",
        r#"["ubuntu-latest"]"#,
        Some(&repo.id),
        false,
    )
    .await
    .expect("runner");
    let run = db
        .insert_action_run(
            "run-fr",
            &repo.id,
            ".github/workflows/ci.yml",
            "CI",
            "push",
            "deadbeef",
            "refs/heads/main",
            "CI",
            Some(&owner.id),
        )
        .await
        .expect("run");
    db.insert_action_job("job-fr", &run.id, "build", "build", r#"["ubuntu-latest"]"#)
        .await
        .expect("job");
    db.insert_action_secret("sec-fr", &repo.id, "SECRET", "cipher")
        .await
        .expect("secret");

    // Instance-scoped runner (no repo) + registration token row via raw SQL.
    db.insert_action_runner(
        "runner-inst",
        "inst",
        "tok2",
        r#"["self-hosted"]"#,
        None,
        true,
    )
    .await
    .expect("instance runner");
    db.insert_action_runner_token("tok-fr", "reghash", "instance", None, true)
        .await
        .expect("token");

    assert!(count_table(&db_path, "action_runners").await >= 2);
    assert_eq!(count_table(&db_path, "action_runs").await, 1);
    assert_eq!(count_table(&db_path, "action_jobs").await, 1);
    assert_eq!(count_table(&db_path, "action_secrets").await, 1);
    assert_eq!(count_table(&db_path, "action_runner_tokens").await, 1);

    db.factory_reset_instance().await.expect("factory reset");

    assert_eq!(count_table(&db_path, "action_runners").await, 0);
    assert_eq!(count_table(&db_path, "action_runs").await, 0);
    assert_eq!(count_table(&db_path, "action_jobs").await, 0);
    assert_eq!(count_table(&db_path, "action_secrets").await, 0);
    assert_eq!(count_table(&db_path, "action_runner_tokens").await, 0);
}
