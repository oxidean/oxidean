//! Phase 6 — trigger filters, pull_request_target, fork approval, schedule
//! sweeper, lifecycle events, workflow_dispatch inputs.

mod support;

use std::sync::Arc;

use oxidean_api::actions::dispatch::notify_push_actions;
use oxidean_api::actions::events::{
    dispatch_pull_request_for_sha, LifecycleEvent, PullRequestAction, PullRequestEvent,
};
use oxidean_api::actions::notify_lifecycle_actions;
use oxidean_core::Role;
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, FileChange, GitBackend};

async fn seed(bare: &std::path::Path, files: &[(String, Vec<u8>)]) -> String {
    let git = CliGitBackend::new();
    if !bare.exists() {
        git.init_bare(bare, "main").await.expect("init");
    }
    git.seed_commit(bare, "main", "c", files)
        .await
        .expect("seed");
    git.list_refs(bare)
        .await
        .expect("refs")
        .into_iter()
        .find(|r| r.name.ends_with("main"))
        .map(|r| r.oid)
        .expect("tip")
}

/// Follow-on commit on `branch` parented on `base` — `seed_commit` writes an
/// orphan root each call (non-FF on a second same-branch seed), so history
/// chains go through the forge commit path instead.
async fn commit_on(
    git: &dyn GitBackend,
    bare: &std::path::Path,
    branch: &str,
    base: &str,
    msg: &str,
    files: &[(&str, &[u8])],
) -> String {
    let changes: Vec<FileChange> = files
        .iter()
        .map(|(p, c)| FileChange::Upsert {
            path: (*p).to_string(),
            content: c.to_vec(),
        })
        .collect();
    git.commit_files(
        bare,
        branch,
        base,
        msg,
        &changes,
        "Test",
        "t@example.com",
        None,
        "admin",
    )
    .await
    .expect("commit")
    .sha
}

async fn mk_user_repo(
    db: &Database,
    suffix: &str,
    bare: &std::path::Path,
    workflow: &[u8],
) -> (String, String, String) {
    let (owner_id, repo_id, _, tip) = mk_user_repo_files(
        db,
        suffix,
        bare,
        &[(".github/workflows/ci.yml".into(), workflow.to_vec())],
    )
    .await;
    (owner_id, repo_id, tip)
}

/// Same fixture but seeds several files into the root commit — reusable-
/// workflow tests need caller + callee workflows side by side. Returns
/// `(owner_id, repo_id, owner_slug, tip)`; the slug is the `<owner>/` dir
/// level `expand_call`'s local `uses:` resolution builds under the repos dir.
async fn mk_user_repo_files(
    db: &Database,
    suffix: &str,
    bare: &std::path::Path,
    files: &[(String, Vec<u8>)],
) -> (String, String, String, String) {
    let owner = db
        .create_user(
            &format!("u-p6-{suffix}"),
            &format!("p6{suffix}@example.com"),
            &format!("p6{suffix}"),
            Some("hash"),
            "P6",
            "",
            None,
            Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository(
            &format!("r-p6-{suffix}"),
            &owner.id,
            "user",
            "p6-demo",
            "private",
            "",
            "main",
        )
        .await
        .expect("repo");
    let tip = seed(bare, files).await;
    (owner.id, repo.id, owner.username, tip)
}

async fn run_count(db: &Database, repo_id: &str) -> usize {
    db.list_action_runs_for_repo(repo_id, &Default::default(), 100, 0)
        .await
        .unwrap()
        .len()
}

#[tokio::test]
async fn p6_push_branch_and_tag_filters() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-f").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let yaml = br#"
name: F
on:
  push:
    branches: [main]
    tags: ['v*']
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#;
    let (uid, repo_id, tip) = mk_user_repo(&db, "f", &bare, yaml).await;
    let git: Arc<dyn GitBackend> = Arc::new(CliGitBackend::new());
    let repos_dir = bare.parent().unwrap().parent().unwrap().to_path_buf();

    // Same commit pushed to a non-matching branch — no run.
    let feat_tip = commit_on(git.as_ref(), &bare, "feature", "main", "c2", &[("x", b"1")]).await;
    notify_push_actions(
        &db,
        git.clone(),
        &repos_dir,
        &repo_id,
        "p6-f",
        "p6-demo",
        Some(&uid),
        &[("".into(), feat_tip.clone(), "refs/heads/feature".into())],
        true,
    )
    .await;
    assert_eq!(
        run_count(&db, &repo_id).await,
        0,
        "feature branch filtered out"
    );

    // Tag matching `v*` — enqueues despite the branches list (tag updates
    // aren't subject to `branches`).
    let new_tip = commit_on(git.as_ref(), &bare, "main", "main", "c3", &[("y", b"2")]).await;
    notify_push_actions(
        &db,
        git.clone(),
        &repos_dir,
        &repo_id,
        "p6-f",
        "p6-demo",
        Some(&uid),
        &[
            ("0".repeat(40), new_tip.clone(), "refs/tags/v1.0".into()),
            (tip.clone(), new_tip, "refs/heads/main".into()),
        ],
        true,
    )
    .await;
    assert_eq!(
        run_count(&db, &repo_id).await,
        2,
        "tag v1.0 + main push each enqueue one run (multi-ref)"
    );
    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    assert!(runs.iter().any(|r| r.head_ref == "refs/tags/v1.0"));
    assert!(runs.iter().any(|r| r.head_ref == "refs/heads/main"));
    let _ = tip;
}

#[tokio::test]
async fn p6_push_paths_filters() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-p").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let yaml = br#"
name: P
on:
  push:
    paths: ['src/**']
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#;
    let (uid, repo_id, tip) = mk_user_repo(&db, "p", &bare, yaml).await;
    let git: Arc<dyn GitBackend> = Arc::new(CliGitBackend::new());
    let repos_dir = bare.parent().unwrap().parent().unwrap().to_path_buf();

    // Commit touching only docs/ — no run.
    let docs_tip = commit_on(
        git.as_ref(),
        &bare,
        "main",
        "main",
        "docs",
        &[("docs/readme.md", b"d")],
    )
    .await;
    notify_push_actions(
        &db,
        git.clone(),
        &repos_dir,
        &repo_id,
        "p6-p",
        "p6-demo",
        Some(&uid),
        &[(tip, docs_tip.clone(), "refs/heads/main".into())],
        true,
    )
    .await;
    assert_eq!(run_count(&db, &repo_id).await, 0, "docs-only push filtered");

    // Commit touching src/ — runs.
    let src_tip = commit_on(
        git.as_ref(),
        &bare,
        "main",
        "main",
        "src",
        &[("src/lib.rs", b"s")],
    )
    .await;
    notify_push_actions(
        &db,
        git.clone(),
        &repos_dir,
        &repo_id,
        "p6-p",
        "p6-demo",
        Some(&uid),
        &[(docs_tip, src_tip.clone(), "refs/heads/main".into())],
        true,
    )
    .await;
    assert_eq!(run_count(&db, &repo_id).await, 1);

    // Fabricated unreachable `before` (force-push edge) — commit listing
    // fails, paths constraints read as unconstrained: the run still fires.
    let phantom = "f".repeat(40);
    notify_push_actions(
        &db,
        git,
        &repos_dir,
        &repo_id,
        "p6-p",
        "p6-demo",
        Some(&uid),
        &[(phantom, src_tip, "refs/heads/main".into())],
        true,
    )
    .await;
    assert_eq!(
        run_count(&db, &repo_id).await,
        2,
        "unlistable history must fail open, not drop paths-filtered runs"
    );
}

#[tokio::test]
async fn p6_pull_request_types_and_target_optin() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-t").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let yaml = br#"
name: T
on:
  pull_request:
    types: [opened]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: echo t
"#;
    let (uid, repo_id, tip) = mk_user_repo(&db, "t", &bare, yaml).await;
    let git = CliGitBackend::new();

    // `synchronize` filtered by types.
    let n = dispatch_pull_request_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare,
        &repo_id,
        PullRequestAction::Synchronize,
        &tip,
        "refs/heads/feature",
        Some(&uid),
        Some(&repo_id),
        true,
    )
    .await
    .unwrap();
    assert_eq!(n, 0);
    let n = dispatch_pull_request_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare,
        &repo_id,
        PullRequestAction::Opened,
        &tip,
        "refs/heads/feature",
        Some(&uid),
        Some(&repo_id),
        true,
    )
    .await
    .unwrap();
    assert_eq!(n, 1);

    // pull_request_target — opt-in required. A repo with a target workflow
    // but no opt-in gets zero target runs.
    let bare2 = dir.path().join("p6-t2").join("p6-demo.git");
    std::fs::create_dir_all(bare2.parent().unwrap()).unwrap();
    let t_yaml = br#"
name: TT
on: [pull_request_target]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: echo tt
"#;
    let (uid2, repo2_id, tip2) = mk_user_repo(&db, "t2", &bare2, t_yaml).await;
    let mk_event = |action| PullRequestEvent {
        action,
        repository_id: repo2_id.clone(),
        bare_repo: bare2.clone(),
        base_bare: bare2.clone(),
        head_sha: tip2.clone(),
        head_ref: "feature".into(),
        base_ref: "main".into(),
        base_sha: tip2.clone(),
        pull_number: 1,
        pull_title: "t".into(),
        pull_author_id: Some(uid2.clone()),
        head_owner: "p6t2".into(),
        head_name: "p6-demo".into(),
        base_owner: "p6t2".into(),
        base_name: "p6-demo".into(),
        base_default_branch: "main".into(),
        actor_id: Some(uid2.clone()),
        head_repo_id: Some(repo2_id.clone()),
        actions_enabled_instance: true,
    };
    let n = oxidean_api::actions::dispatch_pull_request(
        &db,
        Arc::new(git.clone()),
        &mk_event(PullRequestAction::Opened),
    )
    .await
    .unwrap();
    assert_eq!(n, 0, "pull_request_target must not fire without opt-in");

    db.set_repo_actions_pull_request_target(&repo2_id, true)
        .await
        .unwrap();
    let n = oxidean_api::actions::dispatch_pull_request(
        &db,
        Arc::new(git.clone()),
        &mk_event(PullRequestAction::Opened),
    )
    .await
    .unwrap();
    assert_eq!(n, 1);
    let runs = db
        .list_action_runs_for_repo(&repo2_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    let run = &runs[0];
    assert_eq!(run.event, "pull_request_target");
    assert_eq!(run.head_ref, "refs/heads/main", "trusted run uses base ref");
    assert_eq!(run.head_sha, tip2);
}

#[tokio::test]
async fn p6_fork_approval_hold_and_release() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-a").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let yaml = br#"
name: A
on: [pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: echo a
"#;
    let (uid, repo_id, tip) = mk_user_repo(&db, "a", &bare, yaml).await;
    let git = CliGitBackend::new();
    db.set_repo_actions_fork_pr_approval(&repo_id, "always")
        .await
        .unwrap();

    // Fork PR — head_repo_id differs → held for approval.
    let n = dispatch_pull_request_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare,
        &repo_id,
        PullRequestAction::Opened,
        &tip,
        "refs/heads/feature",
        Some(&uid),
        Some("other-repo-id"),
        true,
    )
    .await
    .unwrap();
    assert_eq!(n, 1);
    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    // `waiting_approval` is derived from the flag — the stored run keeps its
    // natural status and every live job sits `waiting` + approval_hold.
    assert!(runs[0].approval_hold, "run flagged held");
    let jobs = db.list_action_jobs_for_run(&runs[0].id).await.unwrap();
    assert!(jobs
        .iter()
        .all(|j| j.approval_hold && j.status == "waiting"));

    // Held jobs are invisible to claim even for a matching runner.
    assert!(db
        .claim_queued_action_job_for_labels("r-held", &["ubuntu-latest".to_string()])
        .await
        .unwrap()
        .is_none());

    // Approval clears the flags — the scheduler then promotes `waiting` legs.
    db.approve_action_run(&runs[0].id).await.unwrap();
    let run = db
        .find_action_run_by_id(&runs[0].id)
        .await
        .unwrap()
        .unwrap();
    assert!(!run.approval_hold);
    let jobs = db.list_action_jobs_for_run(&runs[0].id).await.unwrap();
    assert!(jobs
        .iter()
        .all(|j| !j.approval_hold && j.status == "waiting"));

    // Same-repo PR under `always` still auto-runs (fork-only gate).
    let n = dispatch_pull_request_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare,
        &repo_id,
        PullRequestAction::Synchronize,
        &tip,
        "refs/heads/feature",
        Some(&uid),
        Some(&repo_id),
        true,
    )
    .await
    .unwrap();
    assert_eq!(n, 1);
    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    let sync_run = runs.iter().find(|r| !r.approval_hold).unwrap();
    assert_eq!(sync_run.event, "pull_request");
}

#[tokio::test]
async fn p6_schedule_sweep_dedupes_per_minute() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    // The sweeper resolves the repo's owner slug itself — `p6s` is the
    // username `mk_user_repo` creates, and the bare layout must match.
    let bare = dir.path().join("p6s").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let yaml = br#"
name: S
on:
  schedule:
    - cron: '* * * * *'
jobs:
  tick:
    runs-on: ubuntu-latest
    steps:
      - run: echo tick
"#;
    let (_uid, repo_id, _tip) = mk_user_repo(&db, "s", &bare, yaml).await;
    let git: Arc<dyn GitBackend> = Arc::new(CliGitBackend::new());
    let repos_dir = bare.parent().unwrap().parent().unwrap().to_path_buf();

    let n = oxidean_api::actions::cron::sweep_scheduled_workflows(&db, git.clone(), &repos_dir)
        .await
        .unwrap();
    assert_eq!(n, 1, "first sweep fires the every-minute cron");
    let n = oxidean_api::actions::cron::sweep_scheduled_workflows(&db, git.clone(), &repos_dir)
        .await
        .unwrap();
    assert_eq!(n, 0, "same-minute second sweep deduped");

    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].event, "schedule");
    assert_eq!(runs[0].head_ref, "refs/heads/main");
    // `github.event.schedule` carries the cron entry that matched this tick.
    let payload: serde_json::Value =
        serde_json::from_str(runs[0].event_payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(payload["schedule"].as_str().unwrap(), "* * * * *");
}

#[tokio::test]
async fn p6_lifecycle_release_and_issues() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-l").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let yaml = br#"
name: L
on:
  release:
    types: [published]
  issues: {}
jobs:
  l:
    runs-on: ubuntu-latest
    steps:
      - run: echo l
"#;
    let (uid, repo_id, _tip) = mk_user_repo(&db, "l", &bare, yaml).await;
    let git: Arc<dyn GitBackend> = Arc::new(CliGitBackend::new());
    let repos_dir = bare.parent().unwrap().parent().unwrap().to_path_buf();

    let ev = |event_name: &'static str, action: &str| LifecycleEvent {
        bare_repo: bare.clone(),
        repository_id: repo_id.clone(),
        default_branch: "main".into(),
        event_name,
        action: action.to_string(),
        payload: serde_json::json!({"action": action}),
        actor_id: Some(uid.clone()),
        actions_enabled_instance: true,
    };

    // release.types=[published] — `created` doesn't fire, `published` does.
    notify_lifecycle_actions(&db, git.as_ref(), &ev("release", "created")).await;
    assert_eq!(run_count(&db, &repo_id).await, 0);
    notify_lifecycle_actions(&db, git.as_ref(), &ev("release", "published")).await;
    assert_eq!(run_count(&db, &repo_id).await, 1);

    // issues with empty types — every action fires.
    notify_lifecycle_actions(&db, git.as_ref(), &ev("issues", "opened")).await;
    assert_eq!(run_count(&db, &repo_id).await, 2);
    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    assert!(runs.iter().any(|r| r.event == "release"));
    assert!(runs.iter().any(|r| r.event == "issues"));
    let _ = repos_dir;
}

/// The generic lifecycle map fires for every wired event name and honours
/// `types:` filtering on each — the mutation-site wiring layers on top.
#[tokio::test]
async fn p6_lifecycle_generic_events() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-g").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let yaml = br#"
name: L2
on:
  issue_comment:
    types: [created]
  create: {}
  delete: {}
  watch: {}
  fork: {}
  pull_request_review:
    types: [dismissed]
  pull_request_review_comment: {}
  registry_package: {}
  label:
    types: [created]
  public: {}
  status: {}
  repository_dispatch:
    types: [deploy]
  branch_protection_rule:
    types: [created]
jobs:
  l:
    runs-on: ubuntu-latest
    steps:
      - run: echo l
"#;
    let (uid, repo_id, _tip) = mk_user_repo(&db, "g", &bare, yaml).await;
    let git: Arc<dyn GitBackend> = Arc::new(CliGitBackend::new());

    let ev = |event_name: &'static str, action: &str| LifecycleEvent {
        bare_repo: bare.clone(),
        repository_id: repo_id.clone(),
        default_branch: "main".into(),
        event_name,
        action: action.to_string(),
        payload: serde_json::json!({"action": action}),
        actor_id: Some(uid.clone()),
        actions_enabled_instance: true,
    };

    // types:[created] — edited is filtered out, created fires.
    notify_lifecycle_actions(&db, git.as_ref(), &ev("issue_comment", "edited")).await;
    assert_eq!(run_count(&db, &repo_id).await, 0);
    notify_lifecycle_actions(&db, git.as_ref(), &ev("issue_comment", "created")).await;
    assert_eq!(run_count(&db, &repo_id).await, 1);

    // Empty types — every action fires.
    for (name, action) in [
        ("create", "created"),
        ("delete", "deleted"),
        ("watch", "started"),
        ("fork", "created"),
        ("pull_request_review_comment", "created"),
        ("registry_package", "published"),
    ] {
        notify_lifecycle_actions(&db, git.as_ref(), &ev(name, action)).await;
    }
    assert_eq!(run_count(&db, &repo_id).await, 7);

    // pull_request_review types:[dismissed] — submitted filtered, dismissed fires.
    notify_lifecycle_actions(&db, git.as_ref(), &ev("pull_request_review", "submitted")).await;
    assert_eq!(run_count(&db, &repo_id).await, 7);
    notify_lifecycle_actions(&db, git.as_ref(), &ev("pull_request_review", "dismissed")).await;
    assert_eq!(run_count(&db, &repo_id).await, 8);

    // label types:[created] — edited filtered, created fires.
    notify_lifecycle_actions(&db, git.as_ref(), &ev("label", "edited")).await;
    assert_eq!(run_count(&db, &repo_id).await, 8);
    notify_lifecycle_actions(&db, git.as_ref(), &ev("label", "created")).await;
    assert_eq!(run_count(&db, &repo_id).await, 9);

    // GitHub defines no activity types for these — bare `on:` fires on any.
    for name in ["public", "status"] {
        notify_lifecycle_actions(&db, git.as_ref(), &ev(name, "")).await;
    }
    assert_eq!(run_count(&db, &repo_id).await, 11);

    // repository_dispatch — types: match the caller-chosen event_type.
    notify_lifecycle_actions(&db, git.as_ref(), &ev("repository_dispatch", "build")).await;
    assert_eq!(run_count(&db, &repo_id).await, 11);
    notify_lifecycle_actions(&db, git.as_ref(), &ev("repository_dispatch", "deploy")).await;
    assert_eq!(run_count(&db, &repo_id).await, 12);

    // branch_protection_rule types:[created] — edited filtered.
    notify_lifecycle_actions(&db, git.as_ref(), &ev("branch_protection_rule", "edited")).await;
    assert_eq!(run_count(&db, &repo_id).await, 12);
    notify_lifecycle_actions(&db, git.as_ref(), &ev("branch_protection_rule", "created")).await;
    assert_eq!(run_count(&db, &repo_id).await, 13);

    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    for name in [
        "issue_comment",
        "create",
        "delete",
        "watch",
        "fork",
        "pull_request_review",
        "pull_request_review_comment",
        "registry_package",
        "label",
        "public",
        "status",
        "repository_dispatch",
        "branch_protection_rule",
    ] {
        assert!(
            runs.iter().any(|r| r.event == name),
            "missing run for {name}"
        );
    }
}

#[tokio::test]
async fn p6_event_payload_persisted_for_runner() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-e").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let yaml = br#"
name: E
on: [push]
jobs:
  e:
    runs-on: ubuntu-latest
    steps:
      - run: echo e
"#;
    let (uid, repo_id, tip) = mk_user_repo(&db, "e", &bare, yaml).await;
    let git = CliGitBackend::new();
    let n = oxidean_api::actions::dispatch_push_for_sha(
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
    assert_eq!(n, 1);
    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    let payload: serde_json::Value =
        serde_json::from_str(runs[0].event_payload_json.as_deref().unwrap_or(""))
            .expect("payload persisted");
    assert_eq!(payload["ref"], "refs/heads/main");
    assert_eq!(payload["after"], tip);
}

#[tokio::test]
async fn p6_skip_ci_marker_suppresses_push() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-s").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let yaml = br#"
name: S
on: [push]
jobs:
  s:
    runs-on: ubuntu-latest
    steps:
      - run: echo s
"#;
    let (uid, repo_id, _tip) = mk_user_repo(&db, "s", &bare, yaml).await;
    let git: Arc<dyn GitBackend> = Arc::new(CliGitBackend::new());
    let repos_dir = bare.parent().unwrap().parent().unwrap().to_path_buf();

    // Head commit carrying `[skip ci]` — GitHub suppresses push workflows.
    let skip_tip = commit_on(
        git.as_ref(),
        &bare,
        "main",
        "main",
        "docs: touch readme [skip ci]",
        &[("readme.md", b"x")],
    )
    .await;
    notify_push_actions(
        &db,
        git.clone(),
        &repos_dir,
        &repo_id,
        "p6-s",
        "p6-demo",
        Some(&uid),
        &[("0".repeat(40), skip_tip, "refs/heads/main".into())],
        true,
    )
    .await;
    assert_eq!(run_count(&db, &repo_id).await, 0, "[skip ci] must not run");

    // A normal follow-up commit still dispatches.
    let ok_tip = commit_on(
        git.as_ref(),
        &bare,
        "main",
        "main",
        "docs: touch readme again",
        &[("readme.md", b"y")],
    )
    .await;
    notify_push_actions(
        &db,
        git.clone(),
        &repos_dir,
        &repo_id,
        "p6-s",
        "p6-demo",
        Some(&uid),
        &[("".into(), ok_tip, "refs/heads/main".into())],
        true,
    )
    .await;
    assert_eq!(run_count(&db, &repo_id).await, 1);
}

#[tokio::test]
async fn p6_branch_delete_and_tag_flood() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-d").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let yaml = br#"
name: D
on: [push]
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo d
"#;
    let (uid, repo_id, tip) = mk_user_repo(&db, "d", &bare, yaml).await;
    let git: Arc<dyn GitBackend> = Arc::new(CliGitBackend::new());
    let repos_dir = bare.parent().unwrap().parent().unwrap().to_path_buf();

    // Branch deletion — `after` all-zeros never runs workflows.
    notify_push_actions(
        &db,
        git.clone(),
        &repos_dir,
        &repo_id,
        "p6-d",
        "p6-demo",
        Some(&uid),
        &[(tip.clone(), "0".repeat(40), "refs/heads/main".into())],
        true,
    )
    .await;
    assert_eq!(run_count(&db, &repo_id).await, 0, "deletion must not run");

    // More than three tags in one push — GitHub suppresses all tag events;
    // a branch update in the same push still runs.
    let zero = "0".repeat(40);
    let updates: Vec<(String, String, String)> = (0..4)
        .map(|i| (zero.clone(), tip.clone(), format!("refs/tags/v9.{i}")))
        .chain(std::iter::once((
            zero.clone(),
            tip.clone(),
            "refs/heads/main".to_string(),
        )))
        .collect();
    notify_push_actions(
        &db,
        git.clone(),
        &repos_dir,
        &repo_id,
        "p6-d",
        "p6-demo",
        Some(&uid),
        &updates,
        true,
    )
    .await;
    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    assert_eq!(runs.len(), 1, "tag flood suppressed, branch ran");
    assert_eq!(runs[0].head_ref, "refs/heads/main");
}

#[tokio::test]
async fn p6_paths_filters_skipped_on_tag_push() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-tp").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    // `paths` that no commit matches — a tag push still fires because
    // GitHub never evaluates path filters for tag pushes.
    let yaml = br#"
name: TP
on:
  push:
    paths: ['docs/**']
jobs:
  tp:
    runs-on: ubuntu-latest
    steps:
      - run: echo tp
"#;
    let (uid, repo_id, tip) = mk_user_repo(&db, "tp", &bare, yaml).await;
    let git: Arc<dyn GitBackend> = Arc::new(CliGitBackend::new());
    let repos_dir = bare.parent().unwrap().parent().unwrap().to_path_buf();

    // Same commit on a branch: touched only readme → no run (sanity).
    let c = commit_on(
        git.as_ref(),
        &bare,
        "main",
        "main",
        "c",
        &[("readme.md", b"1")],
    )
    .await;
    notify_push_actions(
        &db,
        git.clone(),
        &repos_dir,
        &repo_id,
        "p6-tp",
        "p6-demo",
        Some(&uid),
        &[(tip.clone(), c.clone(), "refs/heads/main".into())],
        true,
    )
    .await;
    assert_eq!(
        run_count(&db, &repo_id).await,
        0,
        "paths filtered branch push"
    );

    // Tag push of that same non-matching commit: paths don't apply to tags.
    notify_push_actions(
        &db,
        git.clone(),
        &repos_dir,
        &repo_id,
        "p6-tp",
        "p6-demo",
        Some(&uid),
        &[("0".repeat(40), c, "refs/tags/v1".into())],
        true,
    )
    .await;
    assert_eq!(run_count(&db, &repo_id).await, 1, "tag push ignores paths");
}

#[tokio::test]
async fn p6_skip_ci_marker_suppresses_pull_request() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-ps").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let yaml = br#"
name: PS
on:
  pull_request:
jobs:
  ps:
    runs-on: ubuntu-latest
    steps:
      - run: echo ps
"#;
    let (uid, repo_id, _tip) = mk_user_repo(&db, "ps", &bare, yaml).await;
    let git = CliGitBackend::new();

    // PR head commit with [ci skip] → no pull_request run.
    let head = commit_on(
        &git,
        &bare,
        "feature",
        "main",
        "feature work [ci skip]",
        &[("f", b"1")],
    )
    .await;
    let n = dispatch_pull_request_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare,
        &repo_id,
        PullRequestAction::Opened,
        &head,
        "refs/heads/feature",
        Some(&uid),
        Some(&repo_id),
        true,
    )
    .await
    .unwrap();
    assert_eq!(n, 0, "[ci skip] must suppress pull_request runs");

    // Clean head commits still dispatch.
    let head2 = commit_on(
        &git,
        &bare,
        "feature",
        "feature",
        "more work",
        &[("f", b"2")],
    )
    .await;
    let n = dispatch_pull_request_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare,
        &repo_id,
        PullRequestAction::Opened,
        &head2,
        "refs/heads/feature",
        Some(&uid),
        Some(&repo_id),
        true,
    )
    .await
    .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn p6_pull_request_test_merge_commit() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-tm").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let git = CliGitBackend::new();

    let yaml = br#"
name: CI
on: [pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: echo sha=${{ github.sha }} ref=${{ github.ref }}
"#;
    let (uid, repo_id, tip) = mk_user_repo(&db, "tm", &bare, yaml).await;
    // Head branch adds a file — merges cleanly into main.
    let head = commit_on(
        &git,
        &bare,
        "feature",
        "main",
        "add",
        &[("head.txt", b"head\n")],
    )
    .await;

    let event = PullRequestEvent {
        action: PullRequestAction::Opened,
        repository_id: repo_id.clone(),
        bare_repo: bare.clone(),
        base_bare: bare.clone(),
        head_sha: head.clone(),
        head_ref: "feature".into(),
        base_ref: "main".into(),
        base_sha: tip.clone(),
        pull_number: 7,
        pull_title: "t".into(),
        pull_author_id: Some(uid.clone()),
        head_owner: "u".into(),
        head_name: "p6-demo".into(),
        base_owner: "u".into(),
        base_name: "p6-demo".into(),
        base_default_branch: "main".into(),
        actor_id: Some(uid.clone()),
        head_repo_id: Some(repo_id.clone()),
        actions_enabled_instance: true,
    };
    let n = oxidean_api::actions::dispatch_pull_request(&db, Arc::new(git.clone()), &event)
        .await
        .unwrap();
    assert_eq!(n, 1);

    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    let run = &runs[0];
    assert_eq!(run.event, "pull_request");
    assert_eq!(run.head_ref, "refs/pull/7/merge");
    assert_ne!(
        run.head_sha, head,
        "run sha is the test-merge, not the head"
    );
    assert_eq!(run.head_sha.len(), 40);

    // Merge ref resolves in the base repo and has (base, head) parents.
    let bare_s = bare.to_str().unwrap();
    let resolved = std::process::Command::new("git")
        .args(["-C", bare_s, "rev-parse", "refs/pull/7/merge"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&resolved.stdout).trim(),
        run.head_sha
    );
    let parents = std::process::Command::new("git")
        .args(["-C", bare_s, "rev-parse", &format!("{}^@", run.head_sha)])
        .output()
        .unwrap();
    let mut ps: Vec<String> = String::from_utf8_lossy(&parents.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .collect();
    ps.sort();
    let mut want = vec![tip.clone(), head.clone()];
    want.sort();
    assert_eq!(ps, want);
    // Merged tree carries the head-only file.
    let tree = std::process::Command::new("git")
        .args(["-C", bare_s, "ls-tree", "--name-only", &run.head_sha])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&tree.stdout).contains("head.txt"));
    // Base branch unmoved.
    let base_tip = std::process::Command::new("git")
        .args(["-C", bare_s, "rev-parse", "refs/heads/main"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&base_tip.stdout).trim(), tip);

    // Payload keeps pull_request.head.sha = the real head.
    let payload: serde_json::Value =
        serde_json::from_str(run.event_payload_json.as_deref().unwrap_or("{}")).unwrap();
    assert_eq!(
        payload["pull_request"]["head"]["sha"].as_str().unwrap(),
        head
    );
}

#[tokio::test]
async fn p6_pull_request_merge_conflict_falls_back_to_head() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let bare = dir.path().join("p6-tmc").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let git = CliGitBackend::new();

    let yaml = br#"
name: CI
on: [pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#;
    let (uid, repo_id, tip) = mk_user_repo(&db, "tmc", &bare, yaml).await;
    // Both sides touch the same line → unmergeable.
    let head = commit_on(
        &git,
        &bare,
        "feature",
        "main",
        "conflict",
        &[(".github/workflows/ci.yml", b"name: CI\non: [pull_request]\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo head\n")],
    )
    .await;
    let _diverged = commit_on(
        &git,
        &bare,
        "main",
        "main",
        "diverge",
        &[(".github/workflows/ci.yml", b"name: CI\non: [pull_request]\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo base\n")],
    )
    .await;

    let event = PullRequestEvent {
        action: PullRequestAction::Opened,
        repository_id: repo_id.clone(),
        bare_repo: bare.clone(),
        base_bare: bare.clone(),
        head_sha: head.clone(),
        head_ref: "feature".into(),
        base_ref: "main".into(),
        base_sha: tip.clone(),
        pull_number: 9,
        pull_title: "t".into(),
        pull_author_id: Some(uid.clone()),
        head_owner: "u".into(),
        head_name: "p6-demo".into(),
        base_owner: "u".into(),
        base_name: "p6-demo".into(),
        base_default_branch: "main".into(),
        actor_id: Some(uid.clone()),
        head_repo_id: Some(repo_id.clone()),
        actions_enabled_instance: true,
    };
    let n = oxidean_api::actions::dispatch_pull_request(&db, Arc::new(git.clone()), &event)
        .await
        .unwrap();
    assert_eq!(n, 1);

    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    let run = &runs[0];
    assert_eq!(run.event, "pull_request");
    assert_eq!(run.head_sha, head, "unmergeable PR falls back to head sha");
    assert_eq!(run.head_ref, "refs/pull/9/head");
}

/// Reusable-workflow `GITHUB_TOKEN` is only ever *downgraded* by the call —
/// every callee scope clamps to the caller job's resolved permissions.
/// Caller declares `permissions: {contents: read}` (explicit map resets the
/// rest to none); the callee tries `contents: write` + `issues: write` on one
/// job and leaves the other undeclared. After expansion the declared leg must
/// land `read`/`none` and the undeclared leg inherits the repo default —
/// still clamped.
#[tokio::test]
async fn p6_reusable_callee_permissions_clamped_to_caller() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();

    let repos_dir = dir.path().join("repos");
    // Local `uses:` resolution reads the bare from OXIDEAN_REPOS_DIR — point
    // it at this test's repos root. No other test in this binary reads the
    // var, so the process-global set is safe here.
    std::env::set_var("OXIDEAN_REPOS_DIR", &repos_dir);
    // Owner username for suffix "pc" is `p6pc` (mk_user_repo_files' third
    // create_user arg) — the `<owner>/` level local `uses:` resolves under.
    let bare = repos_dir.join("p6pc").join("p6-demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();

    let caller = br#"
name: Caller
on: [push]
jobs:
  call:
    permissions:
      contents: read
    uses: ./.github/workflows/callee.yml
"#;
    let callee = br#"
name: Callee
on: workflow_call
jobs:
  declared:
    permissions:
      contents: write
      issues: write
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
  undeclared:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#;
    let (_uid, repo_id, _slug, tip) = mk_user_repo_files(
        &db,
        "pc",
        &bare,
        &[
            (".github/workflows/ci.yml".into(), caller.to_vec()),
            (".github/workflows/callee.yml".into(), callee.to_vec()),
        ],
    )
    .await;

    let doc = oxidean_api::actions::parse_workflow_yaml(caller).unwrap();
    let (run_id, _job_ids) = oxidean_api::actions::enqueue_run(
        &db,
        &repo_id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        &tip,
        "refs/heads/main",
        None,
        None,
        None,
        false,
    )
    .await
    .unwrap();

    let run = db
        .find_action_run_by_id(&run_id)
        .await
        .unwrap()
        .expect("run row");
    let jobs = db.list_action_jobs_for_run(&run_id).await.unwrap();
    let caller_job = jobs
        .iter()
        .find(|j| j.job_key == "call")
        .expect("caller leg");
    let stored = db
        .find_action_payload(caller_job.payload_sha.as_deref().unwrap())
        .await
        .unwrap()
        .expect("caller payload");
    let payload: serde_json::Value = serde_json::from_str(&stored.payload_json).unwrap();
    // Sanity: the caller leg froze its own resolved map — contents read,
    // every other scope reset to none by the explicit block.
    assert_eq!(payload["permissions"]["contents"], "read");
    assert_eq!(payload["permissions"]["issues"], "none");

    let outcome = oxidean_api::actions::reusable::expand_call(
        &db,
        &run,
        caller_job,
        &payload,
        &oxidean_actions_expr::Context::new(),
    )
    .await
    .unwrap();
    if let oxidean_api::actions::reusable::ExpandOutcome::Failed(msg) = &outcome {
        panic!("expansion failed: {msg}");
    }

    let jobs = db.list_action_jobs_for_run(&run_id).await.unwrap();
    let callee_jobs: Vec<_> = jobs
        .iter()
        .filter(|j| j.caller_job_id == caller_job.id)
        .collect();
    assert_eq!(callee_jobs.len(), 2);
    for cj in callee_jobs {
        let stored = db
            .find_action_payload(cj.payload_sha.as_deref().unwrap())
            .await
            .unwrap()
            .expect("callee payload");
        let p: serde_json::Value = serde_json::from_str(&stored.payload_json).unwrap();
        assert_eq!(
            p["permissions"]["contents"], "read",
            "{} must clamp contents write→read",
            cj.job_key
        );
        assert_eq!(p["contents_permission"], "read", "{}", cj.job_key);
        if cj.job_key == "declared" {
            // The caller never granted issues — an explicit map reset it to
            // none, so the callee's `issues: write` clamps to none.
            assert_eq!(p["permissions"]["issues"], "none", "{}", cj.job_key);
        } else {
            // Undeclared callee job resolved the repo `read` default
            // (contents+packages read) — packages still clamps to the
            // caller's none.
            assert_eq!(p["permissions"]["packages"], "none", "{}", cj.job_key);
        }
    }
}
