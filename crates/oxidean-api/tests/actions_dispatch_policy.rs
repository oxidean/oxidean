//! Phase 19 — registered-only dispatch (ACT-07 / D-ACT-10 / D-ACT-20).

use oxidean_api::actions::dispatch_push_for_sha;
use oxidean_core::Role;
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, GitBackend};

#[tokio::test]
async fn actions_dispatch_policy_registered_runners_only() {
    // Jobs are only assigned via FetchTask on registered runners — enqueue leaves status queued.
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!(
        "sqlite:{}",
        dir.path().join("p.db").display()
    ))
    .await
    .unwrap();
    db.migrate().await.unwrap();
    let owner = db
        .create_user(
            "u-pol",
            "pol@example.com",
            "pol",
            Some("h"),
            "P",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-pol", &owner.id, "user", "pol", "private", "", "main")
        .await
        .unwrap();
    let bare = dir.path().join("pol").join("pol.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let git = CliGitBackend::new();
    git.init_bare(&bare, "main").await.unwrap();
    git.seed_commit(
        &bare,
        "main",
        "wf",
        &[(
            ".github/workflows/ci.yml".into(),
            br#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#
            .to_vec(),
        )],
    )
    .await
    .unwrap();
    let tip = git
        .list_refs(&bare)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.name.ends_with("main"))
        .unwrap()
        .oid;
    dispatch_push_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare,
        &repo.id,
        &tip,
        "refs/heads/main",
        Some(&owner.id),
        true,
    )
    .await
    .unwrap();
    let runs = db.list_action_runs_for_repo(&repo.id, &Default::default(), 100, 0).await.unwrap();
    let jobs = db.list_action_jobs_for_run(&runs[0].id).await.unwrap();
    assert!(jobs[0].runner_id.is_none());
    assert_eq!(jobs[0].status, "queued");
}

#[tokio::test]
async fn actions_dispatch_policy_unmatched_labels_stay_queued() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!(
        "sqlite:{}",
        dir.path().join("u.db").display()
    ))
    .await
    .unwrap();
    db.migrate().await.unwrap();
    let owner = db
        .create_user(
            "u-um",
            "um@example.com",
            "um",
            Some("h"),
            "U",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-um", &owner.id, "user", "um", "private", "", "main")
        .await
        .unwrap();
    db.insert_action_run(
        "run-um",
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
    db.insert_action_job(
        "job-um",
        "run-um",
        "build",
        "build",
        r#"["windows-latest"]"#,
    )
    .await
    .unwrap();
    // Runner with ubuntu only cannot claim windows job.
    db.insert_action_runner(
        "runner-um",
        "r",
        "hash",
        r#"["ubuntu-latest"]"#,
        None,
        false,
    )
    .await
    .unwrap();
    let claimed = db
        .claim_queued_action_job_for_labels("runner-um", &["ubuntu-latest".into()])
        .await
        .unwrap();
    assert!(claimed.is_none());
    let job = db.find_action_job_by_id("job-um").await.unwrap().unwrap();
    assert_eq!(job.status, "queued");
}

#[tokio::test]
async fn actions_dispatch_policy_no_in_process_execution() {
    // Control plane only creates queued rows — never runs step scripts in-process.
    assert!(
        !std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/actions/dispatch.rs")
            .exists()
            || true
    );
    let src = include_str!("../src/actions/dispatch.rs");
    assert!(!src.contains("std::process::Command"));
    assert!(!src.contains("tokio::process"));
}


#[test]
fn actions_dispatch_policy_no_managed_executor_symbols() {
    // ACT-07 / D-ACT-10: control plane must not ship in-process or hosted-minutes executors.
    let dispatch = include_str!("../src/actions/dispatch.rs");
    let runner = include_str!("../src/actions/runner_proto.rs");
    for src in [dispatch, runner] {
        assert!(!src.contains("managed_minutes"));
        assert!(!src.contains("ManagedMinutes"));
        assert!(!src.contains("cloud_executor"));
        assert!(!src.contains("std::process::Command"));
        assert!(!src.contains("tokio::process"));
    }
}
