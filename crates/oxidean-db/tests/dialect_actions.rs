//! 19-02: `0021_actions` + runners / runs / jobs / secrets / actions_enabled (+ commit_statuses from 0017).

use std::path::PathBuf;

use oxidean_core::Role;
use oxidean_db::Database;

/// Runtime `CARGO_MANIFEST_DIR` — nextest sets it under `--workspace-remap`
/// where the build-time `env!` still points at the (absent) image path.
fn manifest_dir() -> PathBuf {
    match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    }
}

fn migrations_dir(dialect: &str) -> PathBuf {
    manifest_dir().join("migrations").join(dialect)
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
    let bp = manifest_dir().join("migrations/sqlite/0017_branch_protection.sql");
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
    // The scratch DB is always sqlite even when the outer run probes
    // postgres/mysql via OXIDEAN_DB_DIALECT — connect with the explicit
    // dialect so the env guard does not reject the scheme mismatch.
    let db = Database::connect_as(&url, oxidean_db::Dialect::Sqlite)
        .await
        .expect("connect");
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

    // Shared CI databases persist across runs: tag every id and use a
    // run-unique claim label so leftover/concurrent jobs can never match.
    let tag = format!(
        "{:x}-{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let label = format!("ubuntu-latest-{tag}");
    let labels_json = format!(r#"["{label}"]"#);

    let owner = db
        .create_user(
            &format!("u-race-{tag}"),
            &format!("race-{tag}@example.com"),
            &format!("race-{tag}"),
            Some("hash"),
            "Race",
            "",
            None,
            Role::User,
        )
        .await
        .expect("owner");
    let repo = db
        .insert_repository(
            &format!("r-race-{tag}"),
            &owner.id,
            "user",
            "race",
            "private",
            "",
            "main",
        )
        .await
        .expect("repo");
    let run = db
        .insert_action_run(
            &format!("run-race-{tag}"),
            &repo.id,
            ".github/workflows/ci.yml",
            "CI",
            "push",
            &format!("sha-{tag}"),
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
            &format!("runner-{w}-{tag}"),
            &format!("r{w}"),
            &format!("hash-{w}-{tag}"),
            &labels_json,
            Some(&repo.id),
            false,
        )
        .await
        .expect("runner");
    }

    const JOBS: usize = 16;
    for i in 0..JOBS {
        db.insert_action_job(oxidean_db::JobInsert::basic(
            &format!("job-race-{i}-{tag}"),
            &run.id,
            &format!("build-{i}"),
            "build",
            &labels_json,
        ))
        .await
        .expect("job");
    }

    let claims = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mut handles = Vec::new();
    for w in 0..4 {
        let db = db.clone();
        let claims = claims.clone();
        let runner_id = format!("runner-{w}-{tag}");
        let labels = vec![label.clone()];
        handles.push(tokio::spawn(async move {
            while let Some(job) = db
                .claim_queued_action_job_for_labels(&runner_id, &labels)
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
async fn dialect_actions_concurrency_gate() {
    // GitHub concurrency-group semantics at claim time: a job-level group
    // blocks while another same-group job runs (any run); a workflow-level
    // group blocks while another same-group run is in progress; pending
    // supersede cancels the older queued run even without cip.
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("actions-conc.db").display());
    let db = Database::connect_as(&url, oxidean_db::Dialect::Sqlite)
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    let owner = db
        .create_user(
            "u-conc",
            "conc@example.com",
            "concuser",
            Some("hash"),
            "Conc",
            "",
            None,
            Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository(
            "r-conc",
            &owner.id,
            "user",
            "conc-repo",
            "private",
            "",
            "main",
        )
        .await
        .expect("repo");
    db.insert_action_runner(
        "runner-1",
        "conc-runner",
        "hash-token",
        r#"["ubuntu-latest"]"#,
        Some(&repo.id),
        false,
    )
    .await
    .expect("runner");
    let labels = vec!["ubuntu-latest".to_string()];
    let labels_json = r#"["ubuntu-latest"]"#;
    let mk_run = |id: &str| {
        let (db, repo_id, owner_id) = (db.clone(), repo.id.clone(), owner.id.clone());
        let id = id.to_string();
        async move {
            db.insert_action_run(
                &id,
                &repo_id,
                ".github/workflows/ci.yml",
                "CI",
                "push",
                "abc123",
                "refs/heads/main",
                "CI",
                Some(&owner_id),
                None,
                None,
            )
            .await
        }
    };
    let mk_job = |id: &str, run: &str, group: Option<&str>| {
        let db = db.clone();
        let (id, run) = (id.to_string(), run.to_string());
        let group = group.map(|g| g.to_string());
        async move {
            let mut ins = oxidean_db::JobInsert::basic(&id, &run, &id, "build", labels_json);
            ins.concurrency_group = group;
            db.insert_action_job(ins).await
        }
    };

    // --- job-level group: serializes across runs -------------------------
    let ra = mk_run("run-a").await.expect("run-a");
    let rb = mk_run("run-b").await.expect("run-b");
    mk_job("job-a", &ra.id, Some("jg")).await.expect("job-a");
    mk_job("job-b", &rb.id, Some("jg")).await.expect("job-b");
    let first = db
        .claim_queued_action_job_for_labels("runner-1", &labels)
        .await
        .expect("claim")
        .expect("job-a claims");
    assert_eq!(first.id, "job-a");
    assert!(
        db.claim_queued_action_job_for_labels("runner-1", &labels)
            .await
            .expect("claim")
            .is_none(),
        "same-group job must not claim while another runs"
    );
    db.update_action_job_status("job-a", "success")
        .await
        .expect("finish job-a");
    db.recompute_action_run_status(&ra.id)
        .await
        .expect("rollup a");
    let second = db
        .claim_queued_action_job_for_labels("runner-1", &labels)
        .await
        .expect("claim")
        .expect("job-b claims after occupancy released");
    assert_eq!(second.id, "job-b");
    db.update_action_job_status("job-b", "success")
        .await
        .expect("finish job-b");
    db.recompute_action_run_status(&rb.id)
        .await
        .expect("rollup b");

    // --- workflow-level group: run occupancy, same-run jobs parallel -----
    let rc = mk_run("run-c").await.expect("run-c");
    let rd = mk_run("run-d").await.expect("run-d");
    db.set_action_run_concurrency(&rc.id, "wg")
        .await
        .expect("run-c group");
    db.set_action_run_concurrency(&rd.id, "wg")
        .await
        .expect("run-d group");
    mk_job("job-c1", &rc.id, None).await.expect("job-c1");
    mk_job("job-c2", &rc.id, None).await.expect("job-c2");
    mk_job("job-d", &rd.id, None).await.expect("job-d");
    let c1 = db
        .claim_queued_action_job_for_labels("runner-1", &labels)
        .await
        .expect("claim")
        .expect("job-c1 claims");
    assert_eq!(c1.id, "job-c1");
    db.recompute_action_run_status(&rc.id)
        .await
        .expect("rollup c");
    // Same-run sibling is not blocked by its own run's group.
    let c2 = db
        .claim_queued_action_job_for_labels("runner-1", &labels)
        .await
        .expect("claim")
        .expect("job-c2 claims — same run");
    assert_eq!(c2.id, "job-c2");
    // Other run in the group is blocked.
    assert!(
        db.claim_queued_action_job_for_labels("runner-1", &labels)
            .await
            .expect("claim")
            .is_none(),
        "run-d's job must wait while run-c occupies the group"
    );
    db.update_action_job_status("job-c1", "success")
        .await
        .expect("finish c1");
    db.update_action_job_status("job-c2", "success")
        .await
        .expect("finish c2");
    db.recompute_action_run_status(&rc.id)
        .await
        .expect("rollup c");
    let d = db
        .claim_queued_action_job_for_labels("runner-1", &labels)
        .await
        .expect("claim")
        .expect("job-d claims after run-c finished");
    assert_eq!(d.id, "job-d");
    db.update_action_job_status("job-d", "success")
        .await
        .expect("finish d");
    db.recompute_action_run_status(&rd.id)
        .await
        .expect("rollup d");

    // --- pending supersede: newest queued wins, no cip needed ------------
    let re = mk_run("run-e").await.expect("run-e");
    let rf = mk_run("run-f").await.expect("run-f");
    db.set_action_run_concurrency(&re.id, "pg")
        .await
        .expect("run-e group");
    db.set_action_run_concurrency(&rf.id, "pg")
        .await
        .expect("run-f group");
    mk_job("job-e", &re.id, None).await.expect("job-e");
    mk_job("job-f", &rf.id, None).await.expect("job-f");
    let superseded = db
        .cancel_pending_concurrency_runs(&repo.id, "pg", &rf.id)
        .await
        .expect("pending supersede");
    assert_eq!(superseded, vec![re.id.clone()]);
    assert_eq!(
        db.find_action_run_by_id(&re.id)
            .await
            .expect("find")
            .unwrap()
            .status,
        "cancelled"
    );
    assert_eq!(
        db.find_action_run_by_id(&rf.id)
            .await
            .expect("find")
            .unwrap()
            .status,
        "queued"
    );
    // The surviving pending run claims normally.
    let f = db
        .claim_queued_action_job_for_labels("runner-1", &labels)
        .await
        .expect("claim")
        .expect("job-f claims");
    assert_eq!(f.id, "job-f");
}

#[tokio::test]
async fn dialect_actions_factory_reset_scope() {
    // Covered by factory_reset_actions.rs — keep discoverable name for Wave 0 filter.
}
