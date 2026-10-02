//! Phase 19 — push + pull_request triggers (ACT-02 / D-ACT-04 / D-ACT-05 / D-ACT-06).

use std::sync::Arc;

use oxidean_api::actions::{
    dispatch_pull_request_for_sha, dispatch_push_for_sha, PullRequestAction,
};
use oxidean_core::Role;
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, GitBackend};

async fn seed_repo_with_workflow(
    db: &Database,
    bare: &std::path::Path,
    workflow: &[u8],
) -> (String, String, String) {
    let owner = db
        .create_user(
            "u-act-trig",
            "acttrig@example.com",
            "acttrig",
            Some("hash"),
            "Act Trig",
            "",
            None,
            Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository(
            "r-act-trig",
            &owner.id,
            "user",
            "trig-demo",
            "private",
            "",
            "main",
        )
        .await
        .expect("repo");
    let git = CliGitBackend::new();
    git.init_bare(bare, "main").await.expect("init");
    git.seed_commit(
        bare,
        "main",
        "wf",
        &[(".github/workflows/ci.yml".into(), workflow.to_vec())],
    )
    .await
    .expect("seed");
    let tip = git
        .list_refs(bare)
        .await
        .expect("refs")
        .into_iter()
        .find(|r| r.name.ends_with("main"))
        .map(|r| r.oid)
        .expect("main tip");
    (owner.id, repo.id, tip)
}

#[tokio::test]
async fn actions_triggers_push_after_receive_pack() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let db = Database::connect(&format!("sqlite:{}", db_path.display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("acttrig").join("trig-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let yaml = br#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#;
    let (uid, repo_id, tip) = seed_repo_with_workflow(&db, &bare, yaml).await;
    let git = CliGitBackend::new();
    let n = dispatch_push_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare,
        &repo_id,
        &tip,
        "refs/heads/main",
        Some(&uid),
        true,
    )
    .await
    .expect("dispatch");
    assert_eq!(n, 1);
    let runs = db.list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].event, "push");
    assert_eq!(runs[0].status, "queued");
    let jobs = db.list_action_jobs_for_run(&runs[0].id).await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, "queued");
}

#[tokio::test]
async fn actions_triggers_pull_request_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!(
        "sqlite:{}",
        dir.path().join("pr.db").display()
    ))
    .await
    .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("acttrig").join("trig-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let yaml = br#"
name: PR
on: [pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: echo pr
"#;
    let (uid, repo_id, tip) = seed_repo_with_workflow(&db, &bare, yaml).await;
    let git = CliGitBackend::new();

    for action in [
        PullRequestAction::Opened,
        PullRequestAction::Synchronize,
        PullRequestAction::Reopened,
    ] {
        let n = dispatch_pull_request_for_sha(
            &db,
            &git as &dyn GitBackend,
            &bare,
            &repo_id,
            action,
            &tip,
            "refs/heads/feature",
            Some(&uid),
            true,
        )
        .await
        .expect("pr dispatch");
        assert_eq!(n, 1, "{action:?}");
    }

    let runs = db.list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0).await.unwrap();
    assert_eq!(runs.len(), 3);
    assert!(runs.iter().all(|r| r.event == "pull_request"));

    // push-only workflows must not enqueue on PR events
    let bare2 = dir.path().join("acttrig").join("push-only.git");
    std::fs::create_dir_all(bare2.parent().unwrap()).unwrap();
    let push_yaml = br#"
name: PushOnly
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#;
    let owner = db
        .create_user(
            "u-act-trig2",
            "acttrig2@example.com",
            "acttrig2",
            Some("hash"),
            "Act Trig2",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo2 = db
        .insert_repository(
            "r-act-trig2",
            &owner.id,
            "user",
            "push-only",
            "private",
            "",
            "main",
        )
        .await
        .unwrap();
    git.init_bare(&bare2, "main").await.unwrap();
    git.seed_commit(
        &bare2,
        "main",
        "wf",
        &[(".github/workflows/ci.yml".into(), push_yaml.to_vec())],
    )
    .await
    .unwrap();
    let tip2 = git
        .list_refs(&bare2)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.name.ends_with("main"))
        .map(|r| r.oid)
        .unwrap();
    let n = dispatch_pull_request_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare2,
        &repo2.id,
        PullRequestAction::Opened,
        &tip2,
        "refs/heads/feature",
        Some(&owner.id),
        true,
    )
    .await
    .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn actions_triggers_respect_instance_and_repo_gates() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!(
        "sqlite:{}",
        dir.path().join("g.db").display()
    ))
    .await
    .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("acttrig").join("trig-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let yaml = br#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#;
    let (uid, repo_id, tip) = seed_repo_with_workflow(&db, &bare, yaml).await;
    let git = CliGitBackend::new();

    let n = dispatch_push_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare,
        &repo_id,
        &tip,
        "refs/heads/main",
        Some(&uid),
        false,
    )
    .await
    .unwrap();
    assert_eq!(n, 0);

    db.set_repo_actions_enabled(&repo_id, false).await.unwrap();
    let n = dispatch_push_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare,
        &repo_id,
        &tip,
        "refs/heads/main",
        Some(&uid),
        true,
    )
    .await
    .unwrap();
    assert_eq!(n, 0);
    let _ = Arc::new(CliGitBackend::new());
}
