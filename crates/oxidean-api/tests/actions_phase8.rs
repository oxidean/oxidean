//! Phase 8 — stability hardening: heartbeat claims + stale-claim requeue +
//! `timeout-minutes` deadline kill, ordered/idempotent log ingest, bounded
//! logs + gzip at rest + retention, org/environment secret merge precedence
//! with decrypt audit, action-source policy loud-fail, and discovery errors
//! persisted as failed `__validation` runs.
// Env-lock guards are held across awaits on purpose — they serialize
// env-mutating tests within this binary.
#![allow(clippy::await_holding_lock)]

mod support;

use oxidean_api::actions::dispatch::{enqueue_run, fail_discovery_runs};
use oxidean_api::actions::logs::{
    append_job_log, append_job_log_bounded, compress_job_log, read_job_log,
};
use oxidean_api::actions::parse_workflow_yaml;
use oxidean_api::actions::secrets::{decrypted_secrets_for_job, encrypt_secret, SecretAudit};
use oxidean_api::actions::sweeps::{sweep_action_logs, sweep_stale_action_jobs};
use oxidean_core::Role;
use oxidean_db::Database;
use oxidean_git::CliGitBackend;

async fn mk_owner_repo(
    db: &Database,
    suffix: &str,
) -> (oxidean_db::UserRow, oxidean_db::RepositoryRow) {
    let owner = db
        .create_user(
            &format!("u-p8-{suffix}"),
            &format!("p8{suffix}@example.com"),
            &format!("p8{suffix}"),
            Some("h"),
            "P8",
            "",
            None,
            Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository(
            &format!("r-p8-{suffix}"),
            &owner.id,
            "user",
            "p8-demo",
            "private",
            "",
            "main",
        )
        .await
        .expect("repo");
    (owner, repo)
}

async fn enqueue(
    db: &Database,
    repo_id: &str,
    actor_id: &str,
    yaml: &[u8],
) -> (String, Vec<String>) {
    let doc = parse_workflow_yaml(yaml).expect("parse");
    enqueue_run(
        db,
        repo_id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "deadbeef",
        "refs/heads/main",
        Some(actor_id),
        None,
        None,
        false,
    )
    .await
    .expect("enqueue")
}

const CI: &[u8] = br#"
name: CI
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#;

async fn mk_runner(db: &Database, suffix: &str) -> String {
    let id = format!("runner-p8-{suffix}");
    db.insert_action_runner(
        &id,
        "r",
        &format!("th-{id}"),
        "[\"ubuntu-latest\"]",
        None,
        false,
    )
    .await
    .expect("runner");
    id
}

#[tokio::test]
async fn p8_claim_stamps_heartbeat_and_binds_runner() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "hb").await;
    let runner = mk_runner(&db, "hb").await;
    let (_run_id, job_ids) = enqueue(&db, &repo.id, &owner.id, CI).await;

    let job = db
        .claim_queued_action_job_for_labels(&runner, &["ubuntu-latest".to_string()])
        .await
        .unwrap()
        .expect("claim");
    assert_eq!(job.id, job_ids[0]);
    assert_eq!(job.status, "in_progress");
    assert_eq!(job.runner_id.as_deref(), Some(runner.as_str()));
    // Phase 8: claim stamps heartbeat_at — the stale sweeper's liveness epoch.
    assert!(job.heartbeat_at.is_some(), "claim must stamp heartbeat_at");
    assert_eq!(job.log_index, 0);
}

#[tokio::test]
async fn p8_log_index_cas_orders_batches() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "cas").await;
    let (_run_id, job_ids) = enqueue(&db, &repo.id, &owner.id, CI).await;
    let job = &job_ids[0];

    use oxidean_db::actions::LogIndexCas::*;
    assert_eq!(db.action_job_log_index(job).await.unwrap(), 0);
    assert_eq!(
        db.advance_action_job_log_index(job, 0).await.unwrap(),
        Accept
    );
    // Replayed batch (lost ack) is an idempotent duplicate — dropped.
    assert_eq!(
        db.advance_action_job_log_index(job, 0).await.unwrap(),
        Duplicate
    );
    // A future batch is a gap — rejected so the runner resyncs.
    assert_eq!(db.advance_action_job_log_index(job, 5).await.unwrap(), Gap);
    assert_eq!(db.action_job_log_index(job).await.unwrap(), 1);
    assert_eq!(
        db.advance_action_job_log_index(job, 1).await.unwrap(),
        Accept
    );
    assert_eq!(db.action_job_log_index(job).await.unwrap(), 2);
}

#[tokio::test]
async fn p8_stale_sweep_requeues_dead_runner() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "stale").await;
    let runner = mk_runner(&db, "stale").await;
    let (_run_id, job_ids) = enqueue(&db, &repo.id, &owner.id, CI).await;
    let job = db
        .claim_queued_action_job_for_labels(&runner, &["ubuntu-latest".to_string()])
        .await
        .unwrap()
        .expect("claim");
    assert_eq!(job.id, job_ids[0]);

    // Fresh heartbeat — not stale.
    let (timed_out, requeued) =
        sweep_stale_action_jobs(&db, &CliGitBackend::new(), dir.path(), 300, "test").await;
    assert_eq!((timed_out, requeued), (0, 0));

    // Runner went silent — backdate the heartbeat past the threshold.
    let old = (chrono::Utc::now() - chrono::Duration::seconds(3600)).to_rfc3339();
    db.touch_action_job_heartbeat(&job.id, &old).await.unwrap();
    let (timed_out, requeued) =
        sweep_stale_action_jobs(&db, &CliGitBackend::new(), dir.path(), 300, "test").await;
    assert_eq!((timed_out, requeued), (0, 1));

    let after = db.find_action_job_by_id(&job.id).await.unwrap().unwrap();
    assert_eq!(after.status, "queued");
    assert_eq!(after.runner_id, None);
    // `attempt` is 1-based at insert — the requeue bumped it once.
    assert_eq!(after.attempt, job.attempt + 1);
    assert_eq!(after.log_index, 0);

    // Requeue cleared the binding — the job is claimable again.
    let reclaimed = db
        .claim_queued_action_job_for_labels(&runner, &["ubuntu-latest".to_string()])
        .await
        .unwrap()
        .expect("reclaim");
    assert_eq!(reclaimed.id, job.id);
}

#[tokio::test]
async fn p8_deadline_lapse_fails_job_and_run() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("OXIDEAN_ACTIONS_LOG_DIR", dir.path().join("logs"));
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "dead").await;
    let runner = mk_runner(&db, "dead").await;
    let (run_id, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: CI
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    timeout-minutes: 0
    steps:
      - run: sleep 999
"#,
    )
    .await;
    let job = db
        .claim_queued_action_job_for_labels(&runner, &["ubuntu-latest".to_string()])
        .await
        .unwrap()
        .expect("claim");
    assert_eq!(job.id, job_ids[0]);
    assert!(job.timeout_at.is_some());

    // `timeout-minutes: 0` stores `now` at seconds precision; the fail-SQL
    // re-checks `timeout_at < at` with a seconds-precision bound, so the
    // deadline must be in a strictly earlier second to land this pass.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let (timed_out, requeued) =
        sweep_stale_action_jobs(&db, &CliGitBackend::new(), dir.path(), 300, "test").await;
    assert_eq!((timed_out, requeued), (1, 0));
    let after = db.find_action_job_by_id(&job.id).await.unwrap().unwrap();
    assert_eq!(after.status, "failure");
    let run = db.find_action_run_by_id(&run_id).await.unwrap().unwrap();
    assert_eq!(run.status, "failure");
}

#[tokio::test]
async fn p8_bounded_log_cap_writes_truncation_marker_once() {
    let dir = tempfile::tempdir().unwrap();
    let log_dir = dir.path().join("logs");
    let (run, job) = ("run-cap", "job-cap");

    assert!(
        append_job_log_bounded(&log_dir, run, job, b"chunk-one\n", 32)
            .await
            .unwrap()
    );
    // This chunk crosses the cap → truncation marker lands instead.
    assert!(
        !append_job_log_bounded(&log_dir, run, job, b"x".repeat(64).as_slice(), 32)
            .await
            .unwrap()
    );
    // Already at cap — dropped outright, no second marker.
    assert!(!append_job_log_bounded(&log_dir, run, job, b"more\n", 32)
        .await
        .unwrap());
    let (bytes, size) = read_job_log(&log_dir, run, job, 0).await.unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.starts_with("chunk-one\n"));
    assert_eq!(text.matches("truncated at 32 bytes").count(), 1, "{text}");
    assert!(!text.contains("more"));
    let _ = size;
}

#[tokio::test]
async fn p8_gzip_roundtrip_preserves_offsets_and_merges_late_appends() {
    let dir = tempfile::tempdir().unwrap();
    let log_dir = dir.path().join("logs");
    let (run, job) = ("run-gz", "job-gz");

    append_job_log(&log_dir, run, job, b"line-1\nline-2\n")
        .await
        .unwrap();
    let (b1, off1) = read_job_log(&log_dir, run, job, 0).await.unwrap();
    assert_eq!(b1, b"line-1\nline-2\n");

    assert!(compress_job_log(&log_dir, run, job).await.unwrap());
    // Compressed read with offset — the tail still works across the boundary.
    let (tail, total) = read_job_log(&log_dir, run, job, 7).await.unwrap();
    assert_eq!(tail, b"line-2\n");
    assert_eq!(total, off1);

    // Zombie append after compression → next compress merges it into the .gz.
    append_job_log(&log_dir, run, job, b"line-3\n")
        .await
        .unwrap();
    assert!(compress_job_log(&log_dir, run, job).await.unwrap());
    let (all, _) = read_job_log(&log_dir, run, job, 0).await.unwrap();
    assert_eq!(all, b"line-1\nline-2\nline-3\n");
}

#[tokio::test]
async fn p8_retention_sweep_compresses_terminal_and_deletes_aged() {
    let dir = tempfile::tempdir().unwrap();
    let log_dir = dir.path().join("logs");
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "ret").await;

    // Terminal run with a log.
    let (run_done, job_done) = enqueue(&db, &repo.id, &owner.id, CI).await;
    db.update_action_job_status(&job_done[0], "success")
        .await
        .unwrap();
    db.recompute_action_run_status(&run_done).await.unwrap();
    append_job_log(&log_dir, &run_done, &job_done[0], b"done\n")
        .await
        .unwrap();

    // Live run — untouched by either pass.
    let (run_live, job_live) = enqueue(&db, &repo.id, &owner.id, CI).await;
    append_job_log(&log_dir, &run_live, &job_live[0], b"live\n")
        .await
        .unwrap();

    // Compresses terminal-run logs, keeps live ones.
    let (compressed, deleted) = sweep_action_logs(&db, &log_dir, 90).await;
    assert_eq!((compressed, deleted), (1, 0));
    assert!(log_dir
        .join(&run_done)
        .join(format!("{}.log.gz", job_done[0]))
        .exists());
    assert!(log_dir
        .join(&run_live)
        .join(format!("{}.log", job_live[0]))
        .exists());

    // Age the terminal run's log past retention → the dir is deleted.
    let gz = log_dir
        .join(&run_done)
        .join(format!("{}.log.gz", job_done[0]));
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(100 * 86_400);
    std::fs::File::options()
        .write(true)
        .open(&gz)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let (_, deleted) = sweep_action_logs(&db, &log_dir, 90).await;
    assert_eq!(deleted, 1);
    assert!(!log_dir.join(&run_done).exists());
    // The live run's log survives.
    assert!(log_dir.join(&run_live).exists());
}

#[tokio::test]
async fn p8_org_env_secret_merge_precedence_and_audit() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "phase8-test-key");
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();

    let owner = db
        .create_user(
            "u-p8-sec",
            "p8sec@example.com",
            "p8sec",
            Some("h"),
            "P8",
            "",
            None,
            Role::User,
        )
        .await
        .expect("user");
    let org = db
        .insert_organization("org-p8-sec", "p8-org", "P8 Org", "read")
        .await
        .expect("org");
    let repo = db
        .insert_repository("r-p8-sec", &org.id, "org", "p8-demo", "private", "", "main")
        .await
        .expect("repo");

    // Same name at three scopes → narrowest wins (org < repo < environment).
    db.insert_action_secret(
        "s1",
        &repo.id,
        "TOKEN",
        &encrypt_secret("repo-val").unwrap(),
    )
    .await
    .unwrap();
    db.upsert_action_org_secret(
        "os1",
        &org.id,
        "TOKEN",
        &encrypt_secret("org-val").unwrap(),
        "all",
    )
    .await
    .unwrap();
    db.upsert_action_org_secret(
        "os2",
        &org.id,
        "ORG_ONLY",
        &encrypt_secret("org-only").unwrap(),
        "all",
    )
    .await
    .unwrap();
    let env = db
        .ensure_action_environment("env-p8-sec", &repo.id, "prod")
        .await
        .unwrap();
    db.upsert_action_environment_secret(
        "es1",
        &env.id,
        "TOKEN",
        &encrypt_secret("env-val").unwrap(),
    )
    .await
    .unwrap();

    let merged = decrypted_secrets_for_job(
        &db,
        &repo.id,
        Some("prod"),
        Some(SecretAudit {
            actor_username: "p8sec",
            actor_id: Some(&owner.id),
            repository_id: &repo.id,
            run_id: "run-1",
            job_id: "job-1",
        }),
    )
    .await
    .unwrap();
    assert_eq!(merged.get("TOKEN").map(String::as_str), Some("env-val"));
    assert_eq!(merged.get("ORG_ONLY").map(String::as_str), Some("org-only"));

    // Without an environment the repo secret beats the org secret —
    // GitHub: on a name collision the narrowest scope wins.
    let merged = decrypted_secrets_for_job(&db, &repo.id, None, None)
        .await
        .unwrap();
    assert_eq!(merged.get("TOKEN").map(String::as_str), Some("repo-val"));

    // Decrypt path was audited — names/counts land, never values.
    let events = db
        .list_audit_events_for_actor(&owner.id, None, 50)
        .await
        .unwrap();
    let decrypt = events
        .iter()
        .find(|e| e.event_type == "actions.secrets.decrypt")
        .expect("decrypt audit event");
    let detail = decrypt.detail.as_deref().unwrap_or("");
    assert!(detail.contains("\"environment_secrets\":1"), "{detail}");
    assert!(!detail.contains("env-val"), "values must never be audited");
}

#[tokio::test]
async fn p8_org_secret_selected_visibility_scopes_repos() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "phase8-test-key");
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let org = db
        .insert_organization("org-p8-vis", "p8-vis", "P8 Vis", "read")
        .await
        .unwrap();
    let in_scope = db
        .insert_repository("r-p8-in", &org.id, "org", "in", "private", "", "main")
        .await
        .unwrap();
    let out_scope = db
        .insert_repository("r-p8-out", &org.id, "org", "out", "private", "", "main")
        .await
        .unwrap();
    let public_repo = db
        .insert_repository("r-p8-pub", &org.id, "org", "pub", "public", "", "main")
        .await
        .unwrap();

    db.upsert_action_org_secret(
        "sv1",
        &org.id,
        "SEL",
        &encrypt_secret("sel").unwrap(),
        "selected",
    )
    .await
    .unwrap();
    db.set_action_org_secret_repos("sv1", std::slice::from_ref(&in_scope.id))
        .await
        .unwrap();
    db.upsert_action_org_secret(
        "sv2",
        &org.id,
        "PRIV",
        &encrypt_secret("priv").unwrap(),
        "private",
    )
    .await
    .unwrap();

    // `selected` — only the allowlisted repo sees it.
    let names = |out: Vec<oxidean_db::ActionSecretCipherRow>| {
        out.iter().map(|r| r.name.clone()).collect::<Vec<_>>()
    };
    let r = db
        .list_action_org_secret_ciphertexts_for_repo(&in_scope.id)
        .await
        .unwrap();
    assert_eq!(names(r), ["PRIV", "SEL"]);
    let r = db
        .list_action_org_secret_ciphertexts_for_repo(&out_scope.id)
        .await
        .unwrap();
    assert_eq!(names(r), ["PRIV"]);
    // `private` visibility excludes public repos.
    let r = db
        .list_action_org_secret_ciphertexts_for_repo(&public_repo.id)
        .await
        .unwrap();
    assert_eq!(names(r), Vec::<String>::new());
}

#[tokio::test]
async fn p8_policy_violation_fails_validation_loudly() {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock().unwrap();
    std::env::set_var("OXIDEAN_ACTIONS_ALLOWED_SOURCES", "trusted-org");
    let restore = || std::env::remove_var("OXIDEAN_ACTIONS_ALLOWED_SOURCES");

    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("OXIDEAN_ACTIONS_LOG_DIR", dir.path().join("logs"));
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "pol").await;
    let (run_id, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: CI
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: trusted-org/vetted@8f4b7f84864484a7bf31766abe9204da3cbe65b3
"#,
    )
    .await;
    restore();

    // Not in the allowlist → a visible failed `__validation` job, no `a` leg.
    assert_eq!(job_ids.len(), 1);
    let job = db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.job_key, "__validation");
    assert_eq!(job.status, "failure");
    assert!(job.name.contains("allowlist"), "{}", job.name);
    let run = db.find_action_run_by_id(&run_id).await.unwrap().unwrap();
    assert_eq!(run.status, "failure");
}

#[tokio::test]
async fn p8_discovery_errors_persist_as_failed_validation_runs() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("OXIDEAN_ACTIONS_LOG_DIR", dir.path().join("logs"));
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "disc").await;

    let errors = vec![
        oxidean_api::actions::DiscoveryFileError {
            path: ".github/workflows/broken.yml".into(),
            message: "parse exploded".into(),
        },
        oxidean_api::actions::DiscoveryFileError {
            path: ".github/workflows/broken2.yml".into(),
            message: "also broken".into(),
        },
    ];
    let n = fail_discovery_runs(
        &db,
        &repo.id,
        &errors,
        "push",
        "deadbeef",
        "refs/heads/main",
        Some(&owner.id),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(n, 2);

    let filter = oxidean_db::ActionRunFilter::default();
    let runs = db
        .list_action_runs_for_repo(&repo.id, &filter, 10, 0)
        .await
        .unwrap();
    assert_eq!(runs.len(), 2);
    for run in &runs {
        assert_eq!(run.status, "failure");
        let jobs = db.list_action_jobs_for_run(&run.id).await.unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].job_key, "__validation");
        assert_eq!(jobs[0].status, "failure");
        assert!(jobs[0].name.contains("invalid workflow file"));
    }
}
