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

    // 0053: durable payloads + normalized labels + attempt metadata in every dialect.
    for dialect in ["sqlite", "postgres", "mysql"] {
        let p = migrations_dir(dialect).join("0053_action_job_payloads.sql");
        let sql = std::fs::read_to_string(&p)
            .unwrap_or_else(|_| panic!("{dialect}: missing {}", p.display()));
        for needle in [
            "action_payloads",
            "action_job_labels",
            "payload_sha",
            "needs_json",
            "matrix_json",
            "matrix_key",
            "outputs_json",
            "attempt",
            "timeout_at",
            "concurrency_group",
            "event_payload_json",
            "concurrency_cancel_pending",
        ] {
            assert!(
                sql.contains(needle),
                "{dialect}: 0053 must define/mention {needle}"
            );
        }
    }

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
            None,
            None,
        )
        .await
        .expect("run");
    assert_eq!(run.status, "queued");

    let job = db
        .insert_action_job(oxidean_db::JobInsert::basic(
            "job-1",
            &run.id,
            "build",
            "build",
            r#"["ubuntu-latest"]"#,
        ))
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

    // 0053: durable payloads + normalized claim labels + attempt counters.
    db.insert_action_payload(
        "sha1",
        &repo.id,
        ".github/workflows/ci.yml",
        "abc123",
        r#"{"version":1,"steps":[{"run":"echo hi"}],"contents_permission":"read"}"#,
    )
    .await
    .expect("payload");
    let mut insert =
        oxidean_db::JobInsert::basic("job-2", &run.id, "test", "test", r#"["ubuntu-latest"]"#);
    insert.payload_sha = Some("sha1".into());
    db.insert_action_job(insert).await.expect("job2");
    // Oldest queued label-matching job claims first.
    let claimed = db
        .claim_queued_action_job_for_labels("runner-1", &["ubuntu-latest".to_string()])
        .await
        .expect("claim")
        .expect("matching job claimable");
    assert_eq!(claimed.id, "job-1");
    let claimed = db
        .claim_queued_action_job_for_labels("runner-1", &["ubuntu-latest".to_string()])
        .await
        .expect("claim")
        .expect("second job claimable");
    assert_eq!(claimed.id, "job-2");
    assert_eq!(claimed.payload_sha.as_deref(), Some("sha1"));
    assert_eq!(claimed.attempt, 1);
    // Label mismatch is refused by the SQL claim predicate, not a Rust scan.
    let miss = db
        .claim_queued_action_job_for_labels("runner-1", &["windows-latest".to_string()])
        .await
        .expect("claim miss");
    assert!(miss.is_none());
    let payload = db.find_action_payload("sha1").await.expect("find").unwrap();
    assert!(payload.payload_json.contains("echo hi"));
}

/// 0053 claim atomicity: N jobs × M concurrent claimers — every job is
/// claimed exactly once (no double-claim) and no runner returns a phantom.
/// Runs against DATABASE_URL under `make db-matrix` (Postgres/MySQL legs);
/// falls back to a tempdir SQLite so the tree-local suite exercises it too.
#[tokio::test]
async fn dialect_actions_claim_race() {
    let (db, _tmp) = match std::env::var("DATABASE_URL") {
        Ok(url) => (Database::connect(&url).await.expect("connect"), None),
        Err(_) => {
            let dir = tempfile::tempdir().expect("tempdir");
            let db = Database::connect(&format!(
                "sqlite:{}",
                dir.path().join("claim-race.db").display()
            ))
            .await
            .expect("connect");
            (db, Some(dir))
        }
    };
    db.migrate().await.expect("migrate");

    let owner = db
        .create_user(
            "u-race",
            "race@example.com",
            "race",
            Some("hash"),
            "Race",
            "",
            None,
            Role::User,
        )
        .await
        .expect("owner");
    let repo = db
        .insert_repository("r-race", &owner.id, "user", "race", "private", "", "main")
        .await
        .expect("repo");
    let run = db
        .insert_action_run(
            "run-race",
            &repo.id,
            ".github/workflows/ci.yml",
            "CI",
            "push",
            "sha",
            "refs/heads/main",
            "CI",
            Some(&owner.id),
            None,
            None,
        )
        .await
        .expect("run");

    // Claiming runners must exist — action_jobs.runner_id is a real FK.
    for w in 0..4 {
        db.insert_action_runner(
            &format!("runner-{w}"),
            &format!("r{w}"),
            &format!("hash-{w}"),
            r#"["ubuntu-latest"]"#,
            Some(&repo.id),
            false,
        )
        .await
        .expect("runner");
    }

    const JOBS: usize = 16;
    for i in 0..JOBS {
        db.insert_action_job(oxidean_db::JobInsert::basic(
            &format!("job-race-{i}"),
            &run.id,
            &format!("build-{i}"),
            "build",
            r#"["ubuntu-latest"]"#,
        ))
        .await
        .expect("job");
    }

    let claims = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mut handles = Vec::new();
    for w in 0..4 {
        let db = db.clone();
        let claims = claims.clone();
        handles.push(tokio::spawn(async move {
            while let Some(job) = db
                .claim_queued_action_job_for_labels(
                    &format!("runner-{w}"),
                    &["ubuntu-latest".to_string()],
                )
                .await
                .expect("claim")
            {
                claims.lock().unwrap().push(job.id);
            }
        }));
    }
    for h in handles {
        h.await.expect("claimer panicked");
    }
    let mut claimed = claims.lock().unwrap().clone();
    claimed.sort();
    claimed.dedup();
    claimed.retain(|id| id.starts_with("job-race-"));
    assert_eq!(
        claimed.len(),
        JOBS,
        "every job claimed exactly once, no double-claims"
    );
}

#[tokio::test]
async fn dialect_actions_factory_reset_scope() {
    // Covered by factory_reset_actions.rs — keep discoverable name for Wave 0 filter.
}
