//! Merge queue — `required_merge_queue` protection, the `merge_queue.*`
//! RPC surface, and the group lifecycle (form → checks → land | destroy).
//!
//! Parity anchors:
//! - Direct `pull.merge` is rejected while a matching rule requires the
//!   queue (`pull.merge_queue_required`).
//! - Enqueue admits only protected, mergeable, open non-draft PRs; one live
//!   slot per pull.
//! - A formed group materializes a merge-commit chain on
//!   `refs/heads/gh-readonly-queue/{base}/group-{id}` — the exact sha
//!   `merge_group` runs check out.
//! - All-successful suites land the train: base fast-forwards, PRs merge on
//!   their chain links, entries flip `merged`.
//! - Any non-success destroys the group and dequeues members
//!   `failed_checks`; a base push destroys and re-forms on the new tip;
//!   head moves remove the entry `head_changed`.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::merge_queue::{after_suite_completed, MqDeps};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, FileChange, GitBackend};
use tower::ServiceExt;

struct T {
    app: axum::Router,
    db: Database,
    repos_dir: std::path::PathBuf,
    pages_dir: std::path::PathBuf,
    git: CliGitBackend,
    _dir: tempfile::TempDir,
}

async fn test_app() -> T {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let pages = dir.path().join("pages");
    let url = format!("sqlite:{}", dir.path().join("mq.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_repos_dir(repos.clone())
    .with_pages_dir(pages.clone());
    let cors = build_cors("development", None).expect("cors");
    T {
        app: router_with_state(state, cors),
        db,
        repos_dir: repos,
        pages_dir: pages,
        git: CliGitBackend::new(),
        _dir: dir,
    }
}

impl T {
    fn deps(&self) -> MqDeps {
        MqDeps {
            db: self.db.clone(),
            git: Arc::new(CliGitBackend::new()),
            repos_dir: self.repos_dir.clone(),
            pages_dir: self.pages_dir.clone(),
            env_name: "development".into(),
            job_token_actor: false,
            actions_enabled: false,
        }
    }

    fn bare(&self, owner: &str, name: &str) -> std::path::PathBuf {
        self.repos_dir.join(owner).join(format!("{name}.git"))
    }
}

fn rpc_req(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn rpc_req_cookie(body: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("cookie", cookie)
        .body(Body::from(body.to_owned()))
        .unwrap()
}

/// Error responses are 4xx with the `{ok:false,error}` envelope in the
/// body — callers assert `ok`/codes, not the HTTP status.
async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_cookie(body, cookie))
        .await
        .unwrap();
    serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn signup_and_login(app: &axum::Router, email: &str, username: &str) -> (String, String) {
    let signup = app
        .clone()
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
        )))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;
    let login = app
        .clone()
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
        )))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string();
    let v: serde_json::Value =
        serde_json::from_slice(&login.into_body().collect().await.unwrap().to_bytes()).unwrap();
    (cookie, v["data"]["id"].as_str().unwrap().to_string())
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

/// Signup + verify + repo, then a `feature` branch carrying a commit `main`
/// lacks — the PR head must differ from base for merge-queue chaining.
async fn seed(t: &T, email: &str, username: &str, name: &str) -> (String, String) {
    let (cookie, uid) = signup_and_login(&t.app, email, username).await;
    verify_user(&t.db, &uid).await;
    let create = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
        ),
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let br = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.branchCreate","input":{{"owner":"{username}","name":"{name}","branch":"feature","start":"main"}}}}"#
        ),
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    // A real head commit on feature — merge-queue chain links need it.
    t.git
        .commit_files(
            &t.bare(username, name),
            "feature",
            "feature",
            "feature change",
            &[FileChange::Upsert {
                path: "feature.txt".into(),
                content: b"feature\n".to_vec(),
            }],
            "T",
            "t@t",
            None,
            "admin",
        )
        .await
        .expect("seed feature commit");
    (cookie, uid)
}

async fn open_pr(t: &T, cookie: &str, username: &str, name: &str) -> i64 {
    let pr = rpc_json(
        &t.app,
        cookie,
        &format!(
            r#"{{"procedure":"pull.create","input":{{"owner":"{username}","name":"{name}","title":"Change","base_ref":"main","head_ref":"feature"}}}}"#
        ),
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    pr["data"]["number"].as_i64().unwrap()
}

async fn require_queue(t: &T, cookie: &str, username: &str, name: &str) {
    let rule = rpc_json(
        &t.app,
        cookie,
        &format!(
            r#"{{"procedure":"repo.branchProtection.create","input":{{"owner":"{username}","name":"{name}","pattern":"main","required_merge_queue":true}}}}"#
        ),
    )
    .await;
    assert_eq!(rule["ok"], true, "{rule}");
    assert_eq!(rule["data"]["required_merge_queue"], true, "{rule}");
}

async fn suite(
    db: &Database,
    repo_id: &str,
    head_sha: &str,
    status: &str,
    conclusion: Option<&str>,
) {
    db.insert_check_suite(&oxidean_db::checks::NewCheckSuite {
        id: &uuid::Uuid::new_v4().to_string(),
        repository_id: repo_id,
        head_sha,
        head_branch: None,
        app: "test",
        run_id: None,
        status,
        conclusion,
        url: None,
        creator_id: None,
    })
    .await
    .expect("insert check suite");
}

/// Protection → enqueue → group forms → success lands the train.
#[tokio::test]
async fn merge_queue_required_enqueues_and_lands() {
    let t = test_app().await;
    let (cookie, uid) = seed(&t, "q1@ex.com", "q1", "train").await;
    let n = open_pr(&t, &cookie, "q1", "train").await;
    require_queue(&t, &cookie, "q1", "train").await;

    // Direct merge is rejected — GitHub hides the merge button for the same reason.
    let merge = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"q1","name":"train","number":{n},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(merge["ok"], false, "{merge}");
    assert_eq!(
        merge["error"]["code"], "pull.merge_queue_required",
        "{merge}"
    );

    let enq = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"merge_queue.enqueue","input":{{"owner":"q1","name":"train","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(enq["ok"], true, "{enq}");
    assert_eq!(enq["data"]["entry"]["state"], "queued");
    assert_eq!(enq["data"]["entry"]["position"], 1);
    assert_eq!(enq["data"]["group"]["state"], "checking");

    // Duplicate enqueue — one live slot per pull.
    let dup = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"merge_queue.enqueue","input":{{"owner":"q1","name":"train","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(dup["ok"], false, "{dup}");

    // get → entry + group + required, and the queue ref is materialized.
    let get = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"merge_queue.get","input":{{"owner":"q1","name":"train","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(get["ok"], true, "{get}");
    assert_eq!(get["data"]["required"], true);
    let head_sha = get["data"]["group"]["head_sha"].as_str().unwrap();
    let head_ref = get["data"]["group"]["head_ref"].as_str().unwrap();
    assert!(
        head_ref.starts_with("refs/heads/gh-readonly-queue/main/"),
        "{head_ref}"
    );
    let resolved = t
        .git
        .rev_parse(&t.bare("q1", "train"), head_ref)
        .await
        .expect("queue ref resolves");
    assert_eq!(resolved, head_sha);

    // All-successful suites land the train: base fast-forwards, PR merges.
    let repo =
        t.db.find_repository_by_owner_name(&uid, "train")
            .await
            .unwrap()
            .unwrap();
    suite(&t.db, &repo.id, head_sha, "completed", Some("success")).await;
    after_suite_completed(&t.deps(), &repo.id, head_sha, "success").await;

    let base_sha = t
        .git
        .rev_parse(&t.bare("q1", "train"), "refs/heads/main")
        .await
        .unwrap();
    assert_eq!(base_sha, head_sha, "base must fast-forward to group head");

    let pull =
        t.db.find_pull_by_repo_number(&repo.id, n)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(pull.state, "merged");
    assert!(
        pull.merge_commit_sha
            .as_deref()
            .is_some_and(|s| !s.is_empty()),
        "chain link recorded as merge_commit_sha"
    );

    let entry =
        t.db.find_latest_merge_queue_entry_for_pull_number(&repo.id, n)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(entry.state, "merged");
}

/// A non-success suite conclusion destroys the group and dequeues its
/// members `failed_checks`.
#[tokio::test]
async fn merge_queue_failing_checks_destroy_group() {
    let t = test_app().await;
    let (cookie, uid) = seed(&t, "q2@ex.com", "q2", "failq").await;
    let n = open_pr(&t, &cookie, "q2", "failq").await;
    require_queue(&t, &cookie, "q2", "failq").await;
    let enq = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"merge_queue.enqueue","input":{{"owner":"q2","name":"failq","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(enq["ok"], true, "{enq}");
    let head_sha = enq["data"]["group"]["head_sha"]
        .as_str()
        .unwrap()
        .to_string();
    let repo =
        t.db.find_repository_by_owner_name(&uid, "failq")
            .await
            .unwrap()
            .unwrap();

    suite(&t.db, &repo.id, &head_sha, "completed", Some("failure")).await;
    after_suite_completed(&t.deps(), &repo.id, &head_sha, "failure").await;

    let entry =
        t.db.find_latest_merge_queue_entry_for_pull_number(&repo.id, n)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(entry.state, "dequeued");
    assert_eq!(entry.dequeue_reason, "failed_checks");
    let group =
        t.db.find_merge_group(enq["data"]["group"]["id"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(group.state, "destroyed");
    // Base untouched by the failed train.
    let pull =
        t.db.find_pull_by_repo_number(&repo.id, n)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(pull.state, "open");
}

/// The group waits for *every* suite on its head — an early success must
/// not land while siblings are still pending.
#[tokio::test]
async fn merge_queue_waits_for_all_suites() {
    let t = test_app().await;
    let (cookie, uid) = seed(&t, "q3@ex.com", "q3", "multi").await;
    let n = open_pr(&t, &cookie, "q3", "multi").await;
    require_queue(&t, &cookie, "q3", "multi").await;
    let enq = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"merge_queue.enqueue","input":{{"owner":"q3","name":"multi","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(enq["ok"], true, "{enq}");
    let head_sha = enq["data"]["group"]["head_sha"]
        .as_str()
        .unwrap()
        .to_string();
    let repo =
        t.db.find_repository_by_owner_name(&uid, "multi")
            .await
            .unwrap()
            .unwrap();

    // Suite A completes success; suite B still in_progress → group waits.
    suite(&t.db, &repo.id, &head_sha, "completed", Some("success")).await;
    suite(&t.db, &repo.id, &head_sha, "in_progress", None).await;
    after_suite_completed(&t.deps(), &repo.id, &head_sha, "success").await;
    let group =
        t.db.find_merge_group(enq["data"]["group"]["id"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        group.state, "checking",
        "pending sibling must hold the group"
    );

    // B completes success → lands.
    let suites =
        t.db.list_check_suites(&repo.id, Some(&head_sha), 50)
            .await
            .unwrap();
    for s in suites.iter().filter(|s| s.status != "completed") {
        t.db.update_check_suite_state(&s.id, "completed", Some("success"))
            .await
            .unwrap();
    }
    after_suite_completed(&t.deps(), &repo.id, &head_sha, "success").await;
    let group =
        t.db.find_merge_group(enq["data"]["group"]["id"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(group.state, "merged");
}

/// A push to the base branch invalidates the checking group; entries stay
/// queued and a fresh group forms on the new tip.
#[tokio::test]
async fn merge_queue_base_push_reforms_group() {
    let t = test_app().await;
    let (cookie, uid) = seed(&t, "q4@ex.com", "q4", "shift").await;
    let n = open_pr(&t, &cookie, "q4", "shift").await;
    require_queue(&t, &cookie, "q4", "shift").await;
    let enq = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"merge_queue.enqueue","input":{{"owner":"q4","name":"shift","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(enq["ok"], true, "{enq}");
    let old_head = enq["data"]["group"]["head_sha"]
        .as_str()
        .unwrap()
        .to_string();
    let repo =
        t.db.find_repository_by_owner_name(&uid, "shift")
            .await
            .unwrap()
            .unwrap();

    // External push moves main.
    t.git
        .commit_files(
            &t.bare("q4", "shift"),
            "main",
            "main",
            "base move",
            &[FileChange::Upsert {
                path: "base.txt".into(),
                content: b"moved\n".to_vec(),
            }],
            "T",
            "t@t",
            None,
            "admin",
        )
        .await
        .expect("base move");
    oxidean_api::merge_queue::after_base_pushed(&t.deps(), &repo.id, "main").await;

    let old_group =
        t.db.find_merge_group(enq["data"]["group"]["id"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(old_group.state, "destroyed");
    // Entry survives and a new group rides the new tip.
    let entry =
        t.db.find_queued_entry_for_pull(
            &t.db
                .find_pull_by_repo_number(&repo.id, n)
                .await
                .unwrap()
                .unwrap()
                .id,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entry.state, "queued");
    let new_group =
        t.db.find_checking_merge_group(&repo.id, "main")
            .await
            .unwrap()
            .expect("reformed group");
    assert_ne!(new_group.head_sha, old_head);
    let new_tip = t
        .git
        .rev_parse(&t.bare("q4", "shift"), "refs/heads/main")
        .await
        .unwrap();
    assert_eq!(new_group.base_sha, new_tip);
}

/// Head moves dequeue the PR `head_changed` and destroy its group; manual
/// dequeue marks `manual`.
#[tokio::test]
async fn merge_queue_head_move_and_manual_dequeue() {
    let t = test_app().await;
    let (cookie, uid) = seed(&t, "q5@ex.com", "q5", "moving").await;
    let n = open_pr(&t, &cookie, "q5", "moving").await;
    require_queue(&t, &cookie, "q5", "moving").await;
    let enq = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"merge_queue.enqueue","input":{{"owner":"q5","name":"moving","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(enq["ok"], true, "{enq}");
    let repo =
        t.db.find_repository_by_owner_name(&uid, "moving")
            .await
            .unwrap()
            .unwrap();
    let pull =
        t.db.find_pull_by_repo_number(&repo.id, n)
            .await
            .unwrap()
            .unwrap();

    // A new commit lands on the PR head — the queued sha is stale.
    t.git
        .commit_files(
            &t.bare("q5", "moving"),
            "feature",
            "feature",
            "new head",
            &[FileChange::Upsert {
                path: "more.txt".into(),
                content: b"more\n".to_vec(),
            }],
            "T",
            "t@t",
            None,
            "admin",
        )
        .await
        .expect("head move");
    oxidean_api::merge_queue::after_pull_head_moved(&t.deps(), &repo.id, &pull, "q5", &uid).await;

    let entry =
        t.db.find_latest_merge_queue_entry_for_pull_number(&repo.id, n)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(entry.state, "dequeued");
    assert_eq!(entry.dequeue_reason, "head_changed");
    let group =
        t.db.find_merge_group(enq["data"]["group"]["id"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(group.state, "destroyed");

    // Re-enqueue on the fresh head, then a manual dequeue.
    let re_enq = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"merge_queue.enqueue","input":{{"owner":"q5","name":"moving","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(re_enq["ok"], true, "{re_enq}");
    let deq = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"merge_queue.dequeue","input":{{"owner":"q5","name":"moving","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(deq["ok"], true, "{deq}");
    assert_eq!(deq["data"]["entry"]["state"], "dequeued");
    assert_eq!(deq["data"]["entry"]["dequeue_reason"], "manual");
}
