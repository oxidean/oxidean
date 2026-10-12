//! Phase 19 — Actions→commit status contexts for Phase 13 (D-ACT-15 / D-ACT-16).
//! Distinct from classic commit_status_rpc.rs (manual statuses).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::actions::{
    enqueue_run, job_status_to_commit_state, parse_workflow_yaml, publish_from_job_update,
    status_context,
};
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use tower::ServiceExt;

async fn seed_db() -> (Database, tempfile::TempDir, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("cs.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let owner = db
        .create_user(
            "u-cs",
            "cs@example.com",
            "csuser",
            Some("h"),
            "CS",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-cs", &owner.id, "user", "cs-demo", "public", "", "main")
        .await
        .unwrap();
    (db, dir, owner.id, repo.id)
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

/// Each workflow job publishes context `{workflow_name} / {job_id}` (D-ACT-15 for Phase 13).
#[tokio::test]
async fn commit_statuses_context_workflow_name_slash_job_id() {
    assert_eq!(status_context("CI", "build"), "CI / build");
    assert_eq!(job_status_to_commit_state("queued"), "pending");
    assert_eq!(job_status_to_commit_state("in_progress"), "pending");
    assert_eq!(job_status_to_commit_state("success"), "success");
    assert_eq!(job_status_to_commit_state("failure"), "failure");
    assert_eq!(job_status_to_commit_state("cancelled"), "error");

    let (db, _dir, uid, repo_id) = seed_db().await;
    let doc = parse_workflow_yaml(
        br#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
    )
    .unwrap();
    enqueue_run(
        &db,
        &repo_id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "abc1234deadbeef",
        "refs/heads/main",
        Some(&uid),
    )
    .await
    .unwrap();
    let statuses = db
        .list_commit_statuses(&repo_id, "abc1234deadbeef")
        .await
        .unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].context, "CI / build");
    assert_eq!(statuses[0].state, "pending");
}

/// Status transitions: queued / in_progress / success / failure / cancelled → classic states.
#[tokio::test]
async fn commit_statuses_lifecycle_states() {
    let (db, _dir, uid, repo_id) = seed_db().await;
    let doc = parse_workflow_yaml(
        br#"
name: Ship
on: [push]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: "true"
"#,
    )
    .unwrap();
    let (_run_id, job_ids) = enqueue_run(
        &db,
        &repo_id,
        ".github/workflows/ship.yml",
        &doc,
        "push",
        "feedface0123456",
        "refs/heads/main",
        Some(&uid),
    )
    .await
    .unwrap();
    let job_id = &job_ids[0];
    for (status, expect) in [("in_progress", "pending"), ("success", "success")] {
        db.update_action_job_status(job_id, status).await.unwrap();
        publish_from_job_update(&db, job_id, status, None)
            .await
            .unwrap();
        let statuses = db
            .list_commit_statuses(&repo_id, "feedface0123456")
            .await
            .unwrap();
        let st = statuses
            .iter()
            .find(|s| s.context == "Ship / test")
            .expect("context");
        assert_eq!(st.state, expect, "status={status}");
    }
}

/// Statuses are queryable via `repo.commitStatus.list` for Phase 13 (D-ACT-16).
#[tokio::test]
async fn commit_statuses_queryable_for_branch_protection() {
    let (db, dir, _uid, repo_id) = seed_db().await;
    let doc = parse_workflow_yaml(
        br#"
name: Gate
on: [push]
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - run: echo lint
"#,
    )
    .unwrap();
    enqueue_run(
        &db,
        &repo_id,
        ".github/workflows/gate.yml",
        &doc,
        "push",
        "bada55c0ffee123",
        "refs/heads/main",
        None,
    )
    .await
    .unwrap();

    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_repos_dir(dir.path().to_path_buf());
    let app = router_with_state(state, build_cors("development", None).unwrap());

    let signup = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"cs2@example.com","username":"cs2","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let signup_body = signup.into_body().collect().await.unwrap().to_bytes();
    let signup_v: serde_json::Value = serde_json::from_slice(&signup_body).unwrap();
    let user_id = signup_v["data"]["id"].as_str().unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now).await.unwrap();

    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"cs2@example.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let _ = login.into_body().collect().await;

    let list = app
        .oneshot(rpc_req_cookie(
            r#"{"procedure":"repo.commitStatus.list","input":{"owner":"csuser","name":"cs-demo","sha":"bada55c0ffee123"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let bytes = list.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    let statuses = v["data"]["statuses"].as_array().expect("statuses");
    assert!(
        statuses.iter().any(|s| s["context"] == "Gate / lint"),
        "{statuses:?}"
    );
}
