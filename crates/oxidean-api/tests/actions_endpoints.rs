//! Actions RPC endpoints — anonymous reads on public repos, list pagination,
//! workflow discovery/dispatch, and run rerun/cancel mutations.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, GitBackend};
use tower::ServiceExt;

async fn test_app(db: Database, repos_dir: std::path::PathBuf) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir);
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

fn rpc_req(body: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("cookie", cookie)
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app.clone().oneshot(rpc_req(body, cookie)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn signup_verified_owner(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
) -> String {
    let signup = rpc_json(
        app,
        "",
        &format!(
            r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
        ),
    )
    .await;
    assert_eq!(signup["ok"], true, "signup — {signup}");
    let user_id = signup["data"]["id"].as_str().expect("user id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(format!(
                    r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    login
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

struct Fixture {
    db: Database,
    app: axum::Router,
    repos_dir: std::path::PathBuf,
    cookie: String,
    owner_username: String,
    owner_id: String,
    _dir: tempfile::TempDir,
}

async fn fixture(visibility: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    let app = test_app(db.clone(), repos_dir.clone()).await;
    let cookie = signup_verified_owner(&app, &db, "owner@example.com", "actowner").await;
    let owner = db
        .find_user_by_username("actowner")
        .await
        .unwrap()
        .expect("owner row");
    // A public/private repo row; the bare git dir is created lazily by tests
    // that exercise workflow discovery.
    db.insert_repository(
        "r-act", &owner.id, "user", "actdemo", visibility, "", "main",
    )
    .await
    .unwrap();
    Fixture {
        db,
        app,
        repos_dir,
        cookie,
        owner_username: "actowner".into(),
        owner_id: owner.id,
        _dir: dir,
    }
}

async fn seed_workflows(fx: &Fixture) {
    let bare = fx.repos_dir.join(&fx.owner_username).join("actdemo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let git = CliGitBackend::new();
    git.init_bare(&bare, "main").await.unwrap();
    git.seed_commit(
        &bare,
        "main",
        "workflows",
        &[
            (
                ".github/workflows/ci.yml".into(),
                br#"
name: CI
on: [push, workflow_dispatch]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#
                .to_vec(),
            ),
            (
                ".github/workflows/push-only.yml".into(),
                br#"
name: Push Only
on:
  push:
    branches: [main]
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - run: echo lint
"#
                .to_vec(),
            ),
        ],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn list_runs_anonymous_on_public_repo() {
    let fx = fixture("public").await;
    fx.db
        .insert_action_run(
            "run-1",
            "r-act",
            ".github/workflows/ci.yml",
            "CI",
            "push",
            "deadbeef",
            "main",
            "CI",
            Some(&fx.owner_id),
        )
        .await
        .unwrap();

    // Anonymous (no cookie) must read runs on a public repo.
    let res = rpc_json(
        &fx.app,
        "",
        r#"{"procedure":"repo.actions.listRuns","input":{"owner":"actowner","name":"actdemo"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "anonymous listRuns — {res}");
    let data = &res["data"];
    assert_eq!(data["total_count"], 1);
    assert_eq!(data["page"], 1);
    assert_eq!(data["per_page"], 25);
    let run = &data["runs"][0];
    assert_eq!(run["id"], "run-1");
    assert_eq!(run["actor"], "actowner");
    assert!(run["created_at"].as_str().is_some());
    assert!(run["updated_at"].as_str().is_some());
}

#[tokio::test]
async fn list_runs_anonymous_on_private_repo_denied() {
    let fx = fixture("private").await;
    let res = rpc_json(
        &fx.app,
        "",
        r#"{"procedure":"repo.actions.listRuns","input":{"owner":"actowner","name":"actdemo"}}"#,
    )
    .await;
    assert_eq!(res["ok"], false, "anonymous private listRuns — {res}");
    // Owner still sees their (empty) run list.
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.listRuns","input":{"owner":"actowner","name":"actdemo"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "owner private listRuns — {res}");
    assert_eq!(res["data"]["total_count"], 0);
}

#[tokio::test]
async fn list_runs_paginates() {
    let fx = fixture("public").await;
    for i in 0..3 {
        fx.db
            .insert_action_run(
                &format!("run-{i}"),
                "r-act",
                ".github/workflows/ci.yml",
                "CI",
                "push",
                "deadbeef",
                "main",
                "CI",
                None,
            )
            .await
            .unwrap();
    }
    let page1 = rpc_json(
        &fx.app,
        "",
        r#"{"procedure":"repo.actions.listRuns","input":{"owner":"actowner","name":"actdemo","per_page":2,"page":1}}"#,
    )
    .await;
    assert_eq!(page1["ok"], true, "{page1}");
    assert_eq!(page1["data"]["runs"].as_array().unwrap().len(), 2);
    assert_eq!(page1["data"]["total_count"], 3);
    assert_eq!(page1["data"]["per_page"], 2);

    let page2 = rpc_json(
        &fx.app,
        "",
        r#"{"procedure":"repo.actions.listRuns","input":{"owner":"actowner","name":"actdemo","per_page":2,"page":2}}"#,
    )
    .await;
    assert_eq!(page2["ok"], true, "{page2}");
    assert_eq!(page2["data"]["runs"].as_array().unwrap().len(), 1);
    // Pages don't overlap.
    assert_ne!(
        page1["data"]["runs"][0]["id"],
        page2["data"]["runs"][0]["id"]
    );
}

#[tokio::test]
async fn get_run_anonymous_includes_jobs_and_timestamps() {
    let fx = fixture("public").await;
    fx.db
        .insert_action_run(
            "run-j",
            "r-act",
            ".github/workflows/ci.yml",
            "CI",
            "push",
            "deadbeef",
            "main",
            "CI",
            Some(&fx.owner_id),
        )
        .await
        .unwrap();
    fx.db
        .insert_action_job("job-1", "run-j", "build", "build", r#"["ubuntu-latest"]"#)
        .await
        .unwrap();

    let res = rpc_json(
        &fx.app,
        "",
        r#"{"procedure":"repo.actions.getRun","input":{"owner":"actowner","name":"actdemo","run_id":"run-j"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "anonymous getRun — {res}");
    assert_eq!(res["data"]["run"]["actor"], "actowner");
    let job = &res["data"]["jobs"][0];
    assert_eq!(job["id"], "job-1");
    assert_eq!(job["status"], "queued");
    // Queued jobs carry no timestamps yet — keys are omitted until a run starts.
    assert!(job.get("started_at").is_none() || job["started_at"].is_null());
}

#[tokio::test]
async fn list_workflows_discovers_dispatchable_anonymous() {
    let fx = fixture("public").await;
    seed_workflows(&fx).await;
    let res = rpc_json(
        &fx.app,
        "",
        r#"{"procedure":"repo.actions.listWorkflows","input":{"owner":"actowner","name":"actdemo"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "listWorkflows — {res}");
    let wfs = res["data"]["workflows"].as_array().unwrap();
    assert_eq!(wfs.len(), 2);
    let ci = wfs.iter().find(|w| w["name"] == "CI").unwrap();
    assert_eq!(ci["supports_dispatch"], true);
    let push_only = wfs.iter().find(|w| w["name"] == "Push Only").unwrap();
    assert_eq!(push_only["supports_dispatch"], false);
    assert_eq!(res["data"]["git_ref"], "main");
}

#[tokio::test]
async fn dispatch_workflow_enqueues_run() {
    let fx = fixture("public").await;
    seed_workflows(&fx).await;

    // Anonymous dispatch must fail.
    let anon = rpc_json(
        &fx.app,
        "",
        r#"{"procedure":"repo.actions.dispatchWorkflow","input":{"owner":"actowner","name":"actdemo","workflow_id":".github/workflows/ci.yml","git_ref":"main"}}"#,
    )
    .await;
    assert_eq!(anon["ok"], false, "anonymous dispatch — {anon}");

    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.dispatchWorkflow","input":{"owner":"actowner","name":"actdemo","workflow_id":".github/workflows/ci.yml","git_ref":"main"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "dispatch — {res}");
    let run_id = res["data"]["run_id"].as_str().expect("run_id");

    let run = fx.db.find_action_run_by_id(run_id).await.unwrap().unwrap();
    assert_eq!(run.event, "workflow_dispatch");
    assert_eq!(run.head_ref, "main");
    assert_eq!(run.triggered_by.as_deref(), Some(fx.owner_id.as_str()));

    // Dispatching a non-dispatchable workflow is rejected.
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.dispatchWorkflow","input":{"owner":"actowner","name":"actdemo","workflow_id":".github/workflows/push-only.yml","git_ref":"main"}}"#,
    )
    .await;
    assert_eq!(res["ok"], false, "push-only dispatch — {res}");
    assert_eq!(res["error"]["code"], "repo.actions.dispatch_unsupported");
}

#[tokio::test]
async fn rerun_and_cancel_run() {
    let fx = fixture("public").await;
    fx.db
        .insert_action_run(
            "run-m",
            "r-act",
            ".github/workflows/ci.yml",
            "CI",
            "push",
            "deadbeef",
            "main",
            "CI",
            Some(&fx.owner_id),
        )
        .await
        .unwrap();
    fx.db
        .insert_action_job("job-m", "run-m", "build", "build", r#"["ubuntu-latest"]"#)
        .await
        .unwrap();

    // Anonymous mutations are denied.
    let anon = rpc_json(
        &fx.app,
        "",
        r#"{"procedure":"repo.actions.cancelRun","input":{"owner":"actowner","name":"actdemo","run_id":"run-m"}}"#,
    )
    .await;
    assert_eq!(anon["ok"], false, "anonymous cancel — {anon}");

    // Cancel the queued run.
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.cancelRun","input":{"owner":"actowner","name":"actdemo","run_id":"run-m"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "cancel — {res}");
    assert_eq!(res["data"]["run"]["status"], "cancelled");
    let job = fx.db.find_action_job_by_id("job-m").await.unwrap().unwrap();
    assert_eq!(job.status, "cancelled");

    // Rerun requeues run + jobs.
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.rerunRun","input":{"owner":"actowner","name":"actdemo","run_id":"run-m"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "rerun — {res}");
    assert_eq!(res["data"]["run"]["status"], "queued");
    let job = fx.db.find_action_job_by_id("job-m").await.unwrap().unwrap();
    assert_eq!(job.status, "queued");

    // A run id from another repo is rejected.
    let other = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.rerunRun","input":{"owner":"actowner","name":"actdemo","run_id":"run-nonexistent"}}"#,
    )
    .await;
    assert_eq!(other["ok"], false, "wrong run — {other}");
}
