//! 19-02: `0021_actions` + runners / runs / jobs / secrets / actions_enabled (+ commit_statuses from 0017).

use std::path::PathBuf;

use oxidean_core::Role;
use oxidean_db::Database;

fn migrations_dir(dialect: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .join(dialect)
}

fn find_actions_migration(dialect: &str) -> Option<(PathBuf, String)> {
    let dir = migrations_dir(dialect);
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    entries.sort();
    for path in entries {
        let sql = std::fs::read_to_string(&path).unwrap_or_default();
        if sql.contains("action_runners")
            && sql.contains("action_runs")
            && sql.contains("action_jobs")
            && sql.contains("action_secrets")
            && sql.contains("actions_enabled")
        {
            return Some((path, sql));
        }
    }
    None
}

fn assert_actions_sql(dialect: &str, sql: &str) {
    for needle in [
        "action_runners",
        "action_runner_tokens",
        "action_runs",
        "action_jobs",
        "action_secrets",
        "actions_enabled",
        "ciphertext",
    ] {
        assert!(
            sql.contains(needle),
            "{dialect}: actions migration must define/mention {needle}"
        );
    }
    // Secrets must never store plaintext column names.
    assert!(
        !sql.to_lowercase().contains("plaintext"),
        "{dialect}: action_secrets must not store plaintext (T-19-04 / D-ACT-17)"
    );
}

#[tokio::test]
async fn dialect_actions_schema_presence() {
    for dialect in ["sqlite", "postgres", "mysql"] {
        let (path, sql) = find_actions_migration(dialect)
            .unwrap_or_else(|| panic!("{dialect}: missing 0021_actions-style migration"));
        assert!(
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains("actions")),
            "{dialect}: unexpected migration path {}",
            path.display()
        );
        assert_actions_sql(dialect, &sql);
    }

    // commit_statuses lives in 0017 (Phase 13) — assert still present for D-ACT-15.
    let bp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("migrations/sqlite/0017_branch_protection.sql");
    let bp_sql = std::fs::read_to_string(&bp).expect("0017_branch_protection.sql");
    assert!(
        bp_sql.contains("commit_statuses"),
        "commit_statuses must exist for Actions→Phase 13 integration (D-ACT-15)"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("actions.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let owner = db
        .create_user(
            "u-act-owner",
            "actowner@example.com",
            "actowner",
            Some("hash"),
            "Act Owner",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create owner after actions migrate");

    let repo = db
        .insert_repository(
            "r-act-1",
            &owner.id,
            "user",
            "actions-demo",
            "private",
            "",
            "main",
        )
        .await
        .expect("repo");

    let runner = db
        .insert_action_runner(
            "runner-1",
            "demo",
            "hash-token",
            r#"["ubuntu-latest"]"#,
            Some(&repo.id),
            false,
        )
        .await
        .expect("runner");
    assert_eq!(runner.name, "demo");

    let run = db
        .insert_action_run(
            "run-1",
            &repo.id,
            ".github/workflows/ci.yml",
            "CI",
            "push",
            "abc123",
            "refs/heads/main",
            "CI",
            Some(&owner.id),
        )
        .await
        .expect("run");
    assert_eq!(run.status, "queued");

    let job = db
        .insert_action_job("job-1", &run.id, "build", "build", r#"["ubuntu-latest"]"#)
        .await
        .expect("job");
    assert_eq!(job.job_key, "build");

    db.insert_action_secret("sec-1", &repo.id, "TOKEN", "cipher-bytes")
        .await
        .expect("secret");
    let names = db
        .list_action_secret_names(&repo.id)
        .await
        .expect("list secrets");
    assert_eq!(names.len(), 1);
    assert_eq!(names[0].name, "TOKEN");
}

#[tokio::test]
async fn dialect_actions_factory_reset_scope() {
    // Covered by factory_reset_actions.rs — keep discoverable name for Wave 0 filter.
}
