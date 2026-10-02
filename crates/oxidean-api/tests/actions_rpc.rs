//! Phase 19 — Actions RPC / log round-trip foundation (ACT-03 / D-ACT-13).

use oxidean_api::actions::{append_job_log, read_job_log};
use oxidean_core::Role;
use oxidean_db::Database;

#[tokio::test]
async fn actions_rpc_list_runs() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!(
        "sqlite:{}",
        dir.path().join("rpc.db").display()
    ))
    .await
    .unwrap();
    db.migrate().await.unwrap();
    let owner = db
        .create_user(
            "u-rpc",
            "rpc@example.com",
            "rpcact",
            Some("h"),
            "R",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-rpc", &owner.id, "user", "rpc", "private", "", "main")
        .await
        .unwrap();
    db.insert_action_run(
        "run-rpc",
        &repo.id,
        ".github/workflows/ci.yml",
        "CI",
        "push",
        "abc",
        "refs/heads/main",
        "CI",
        Some(&owner.id),
    )
    .await
    .unwrap();
    let runs = db.list_action_runs_for_repo(&repo.id, &Default::default(), 100, 0).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, "run-rpc");
}

#[tokio::test]
async fn actions_rpc_run_detail() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!(
        "sqlite:{}",
        dir.path().join("rpc2.db").display()
    ))
    .await
    .unwrap();
    db.migrate().await.unwrap();
    let owner = db
        .create_user(
            "u-rpc2",
            "rpc2@example.com",
            "rpcact2",
            Some("h"),
            "R",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-rpc2", &owner.id, "user", "rpc2", "private", "", "main")
        .await
        .unwrap();
    db.insert_action_run(
        "run-2",
        &repo.id,
        ".github/workflows/ci.yml",
        "CI",
        "push",
        "abc",
        "refs/heads/main",
        "CI",
        None,
    )
    .await
    .unwrap();
    db.insert_action_job("job-2", "run-2", "build", "build", r#"["ubuntu-latest"]"#)
        .await
        .unwrap();
    let jobs = db.list_action_jobs_for_run("run-2").await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].job_key, "build");
}

#[tokio::test]
async fn actions_rpc_job_logs() {
    let dir = tempfile::tempdir().unwrap();
    let log_dir = dir.path().join("logs");
    append_job_log(&log_dir, "run-x", "job-y", b"hello from runner\n")
        .await
        .unwrap();
    let (bytes, size) = read_job_log(&log_dir, "run-x", "job-y", 0).await.unwrap();
    assert_eq!(bytes, b"hello from runner\n");
    assert_eq!(size, b"hello from runner\n".len() as u64);
    let (tail, _) = read_job_log(&log_dir, "run-x", "job-y", 6).await.unwrap();
    assert_eq!(tail, b"from runner\n");
}
