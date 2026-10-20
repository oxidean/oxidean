//! Phase 19 — runner protocol tracer (ACT-06 / D-ACT-07 / D-ACT-09 / D-ACT-18).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::actions::dispatch::{dispatch_push_for_sha, enqueue_run};
use oxidean_api::actions::mint_registration_token;
use oxidean_api::actions::parse_workflow_yaml;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use oxidean_git::CliGitBackend;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

async fn app_db() -> (axum::Router, Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("r.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_actions_enabled(true);
    let app = router_with_state(state, build_cors("development", None).unwrap());
    (app, db, dir)
}

#[tokio::test]
async fn actions_runner_protocol_register() {
    let (app, db, _dir) = app_db().await;
    let reg = mint_registration_token(&db).await.unwrap();
    let body = serde_json::json!({
        "name": "runner-1",
        "labels": ["ubuntu-latest"],
        "token": reg,
    });
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v["runner_id"].as_str().unwrap().len() > 8);
    assert!(v["runner_token"].as_str().unwrap().starts_with("ort_"));
}

#[tokio::test]
async fn actions_runner_protocol_declare_labels() {
    let (app, db, _dir) = app_db().await;
    let reg = mint_registration_token(&db).await.unwrap();
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "r1",
                        "labels": ["ubuntu-latest"],
                        "token": reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let reg_body = reg_res.into_body().collect().await.unwrap().to_bytes();
    let reg_v: serde_json::Value = serde_json::from_slice(&reg_body).unwrap();
    let runner_token = reg_v["runner_token"].as_str().unwrap();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/declare")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {runner_token}"))
                .body(Body::from(
                    serde_json::json!({
                        "labels": ["ubuntu-latest:docker://node:20", "self-hosted"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn actions_runner_protocol_fetch_task_label_match() {
    let (app, db, dir) = app_db().await;
    let owner = db
        .create_user(
            "u-ft",
            "ft@example.com",
            "ft",
            Some("h"),
            "F",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-ft", &owner.id, "user", "ft", "private", "", "main")
        .await
        .unwrap();
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
        &repo.id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "deadbeef",
        "refs/heads/main",
        Some(&owner.id),
        None,
        None,
        false,
        None,
    )
    .await
    .unwrap();

    let reg = mint_registration_token(&db).await.unwrap();
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "r1",
                        "labels": ["ubuntu-latest"],
                        "token": reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let reg_body = reg_res.into_body().collect().await.unwrap().to_bytes();
    let reg_v: serde_json::Value = serde_json::from_slice(&reg_body).unwrap();
    let runner_token = reg_v["runner_token"].as_str().unwrap();

    let fetch = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/fetch_task")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {runner_token}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fetch.status(), StatusCode::OK);
    let bytes = fetch.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v["job_id"].as_str().is_some());
    assert_eq!(v["job_key"], "build");
    // 0053: steps come from the durable payload captured at enqueue — the
    // fixture's repos_dir points nowhere real, so a git re-read would yield [].
    assert_eq!(v["steps"][0]["run"], "echo hi");
    assert_eq!(v["run_attempt"], 1);
    let _ = dir;
    let _ = dispatch_push_for_sha;
    let _ = CliGitBackend::new();
}

/// A requeued run increments `attempt` — `fetch_task` reports it as
/// `run_attempt` (GITHUB_RUN_ATTEMPT parity).
#[tokio::test]
async fn actions_runner_protocol_rerun_bumps_run_attempt() {
    let (app, db, _dir) = app_db().await;
    let owner = db
        .create_user(
            "u-ra",
            "ra@example.com",
            "ra",
            Some("h"),
            "R",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-ra", &owner.id, "user", "ra", "private", "", "main")
        .await
        .unwrap();
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
    let (run_id, _) = enqueue_run(
        &db,
        &repo.id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "deadbeef",
        "refs/heads/main",
        Some(&owner.id),
        None,
        None,
        false,
        None,
    )
    .await
    .unwrap();

    let reg = mint_registration_token(&db).await.unwrap();
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "r1",
                        "labels": ["ubuntu-latest"],
                        "token": reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let reg_body = reg_res.into_body().collect().await.unwrap().to_bytes();
    let runner_token = serde_json::from_slice::<serde_json::Value>(&reg_body).unwrap()
        ["runner_token"]
        .as_str()
        .unwrap()
        .to_string();

    let fetch = |app: axum::Router, tok: &str| {
        let tok = tok.to_string();
        async move {
            let res = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/actions/fetch_task")
                        .header("content-type", "application/json")
                        .header("authorization", format!("Bearer {tok}"))
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
        }
    };

    let first = fetch(app.clone(), &runner_token).await;
    assert_eq!(first["run_attempt"], 1);
    let job_id = first["job_id"].as_str().unwrap();
    db.update_action_job_status(job_id, "failure")
        .await
        .unwrap();

    db.requeue_action_run(&run_id, false, None, false)
        .await
        .unwrap();

    let second = fetch(app.clone(), &runner_token).await;
    assert_eq!(second["run_attempt"], 2);
    assert_eq!(second["job_id"], job_id, "requeue resets the same job row");
}

#[tokio::test]
async fn actions_runner_protocol_ignores_session_cookie() {
    let (app, db, _dir) = app_db().await;
    // Signup to get a session cookie
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(
                    r#"{"procedure":"auth.signup","input":{"email":"ck@ex.com","username":"ckuser","password":"password1"}}"#,
                ))
                .unwrap(),
        )
        .await;
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(
                    r#"{"procedure":"auth.login","input":{"identifier":"ck@ex.com","password":"password1","remember_me":false}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
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

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/fetch_task")
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let _ = db;
}

#[tokio::test]
async fn actions_runner_protocol_env_bootstrap_token() {
    let (app, _db, _dir) = app_db().await;
    std::env::set_var(
        "OXIDEAN_RUNNER_REGISTRATION_TOKEN",
        "bootstrap-secret-token",
    );
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "ephemeral",
                        "labels": ["ubuntu-latest"],
                        "token": "bootstrap-secret-token"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    std::env::remove_var("OXIDEAN_RUNNER_REGISTRATION_TOKEN");
    assert_eq!(res.status(), StatusCode::OK);
}

/// ACT-06: claimed runner can append logs and transition job state via protocol.
#[tokio::test]
async fn actions_runner_protocol_update_task_and_log() {
    let dir = tempfile::tempdir().unwrap();
    let log_dir = dir.path().join("actions-logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("r.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_actions_enabled(true)
    .with_actions_log_dir(log_dir.clone());
    let app = router_with_state(state, build_cors("development", None).unwrap());

    let owner = db
        .create_user(
            "u-upd",
            "upd@example.com",
            "upd",
            Some("h"),
            "U",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-upd", &owner.id, "user", "upd", "private", "", "main")
        .await
        .unwrap();
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
        &repo.id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "cafebabe",
        "refs/heads/main",
        Some(&owner.id),
        None,
        None,
        false,
        None,
    )
    .await
    .unwrap();

    let reg = mint_registration_token(&db).await.unwrap();
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "r-upd",
                        "labels": ["ubuntu-latest"],
                        "token": reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let reg_v: serde_json::Value =
        serde_json::from_slice(&reg_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let runner_token = reg_v["runner_token"].as_str().unwrap();

    let fetch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/fetch_task")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {runner_token}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fetch.status(), StatusCode::OK);
    let fetch_v: serde_json::Value =
        serde_json::from_slice(&fetch.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let job_id = fetch_v["job_id"].as_str().expect("claimed job_id");
    let run_id = fetch_v["run_id"].as_str().expect("claimed run_id");
    // Runner needs repo coordinates to clone for checkout steps.
    assert_eq!(fetch_v["repository_owner"].as_str().unwrap(), "upd");
    assert_eq!(fetch_v["repository_name"].as_str().unwrap(), "upd");

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/update_task")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {runner_token}"))
                .body(Body::from(
                    serde_json::json!({ "job_id": job_id, "state": "in_progress" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::OK);
    let job = db.find_action_job_by_id(job_id).await.unwrap().unwrap();
    assert_eq!(job.status, "in_progress");
    assert!(job.started_at.is_some(), "in_progress sets started_at");
    let run = db.find_action_run_by_id(run_id).await.unwrap().unwrap();
    assert_eq!(run.status, "in_progress", "run rolls up to in_progress");

    let log_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/update_log")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {runner_token}"))
                .body(Body::from(
                    serde_json::json!({
                        "run_id": run_id,
                        "job_id": job_id,
                        "chunk": "hello from runner\n"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(log_res.status(), StatusCode::OK);

    // `##[ann]{json}` markers are stripped at ingest and persisted as
    // structured annotation rows; the visible `##[error]` line survives.
    let ann_chunk = "##[ann]{\"level\":\"error\",\"message\":\"boom\",\"path\":\"src/main.rs\",\"line\":3,\"col\":1,\"title\":\"Lint\"}\n##[error]boom\n";
    let ann_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/update_log")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {runner_token}"))
                .body(Body::from(
                    serde_json::json!({
                        "run_id": run_id,
                        "job_id": job_id,
                        "chunk": ann_chunk
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ann_res.status(), StatusCode::OK);
    let anns = db.list_action_annotations_for_job(job_id).await.unwrap();
    assert_eq!(anns.len(), 1, "annotation row persisted — {anns:?}");
    assert_eq!(anns[0].annotation_level, "error");
    assert_eq!(anns[0].message, "boom");
    assert_eq!(anns[0].path.as_deref(), Some("src/main.rs"));
    assert_eq!(anns[0].start_line, Some(3));
    assert_eq!(anns[0].start_column, Some(1));
    assert_eq!(anns[0].title.as_deref(), Some("Lint"));

    let upd = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/update_task")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {runner_token}"))
                .body(Body::from(
                    serde_json::json!({
                        "job_id": job_id,
                        "state": "success"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upd.status(), StatusCode::OK);

    let job = db.find_action_job_by_id(job_id).await.unwrap().unwrap();
    assert_eq!(job.status, "success");
    assert!(
        job.finished_at.is_some(),
        "terminal status sets finished_at"
    );

    let run = db.find_action_run_by_id(run_id).await.unwrap().unwrap();
    assert_eq!(run.status, "success", "run rolls up to success");
    assert!(run.finished_at.is_some(), "terminal run sets finished_at");

    let runner = db
        .find_action_runner_by_token_hash(&oxidean_api::actions::tokens::hash_token(runner_token))
        .await
        .unwrap()
        .unwrap();
    assert!(
        runner.last_online.is_some(),
        "protocol calls bump runner last_online"
    );

    let log_path = log_dir.join(run_id).join(format!("{job_id}.log"));
    let log_bytes = std::fs::read(&log_path).expect("log file written by update_log");
    let log_text = String::from_utf8_lossy(&log_bytes);
    assert!(
        log_text.contains("hello from runner"),
        "update_log chunk must persist under ACTIONS_LOG_DIR"
    );
    assert!(
        log_text.contains("##[error]boom"),
        "visible annotation line persists"
    );
    assert!(
        !log_text.contains("##[ann]"),
        "annotation side-channel markers are stripped at ingest"
    );

    let statuses = db.list_commit_statuses(&repo.id, "cafebabe").await.unwrap();
    let st = statuses
        .iter()
        .find(|s| s.context.contains("build"))
        .expect("commit status published for job");
    assert_eq!(st.state, "success");
}

/// Fork-PR hardening: a `pull_request` run whose head commit comes from a pull
/// opened against this repo from another repository (`head_repo_id` recorded
/// at dispatch differs from the run's `repository_id`) must receive no repo
/// secrets, and its OXIDEAN_TOKEN must be clamped to `read` even when the
/// workflow asks for `contents: write`. Same-repo PRs keep both.
#[tokio::test]
async fn actions_runner_protocol_pull_request_provenance_gate() {
    use oxidean_api::actions::secrets::encrypt_secret;
    use oxidean_git::GitBackend;

    std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "integration-test-key");
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("fk.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let repos_dir = dir.path().join("repos");
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_actions_enabled(true)
    .with_repos_dir(repos_dir.clone());
    let app = router_with_state(state, build_cors("development", None).unwrap());

    let owner = db
        .create_user(
            "u-fk",
            "fk@example.com",
            "fkown",
            Some("h"),
            "F",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let base = db
        .insert_repository(
            "r-fkbase", &owner.id, "user", "fkbase", "private", "", "main",
        )
        .await
        .unwrap();
    let fork = db
        .insert_repository(
            "r-fkfork", &owner.id, "user", "fkfork", "private", "", "main",
        )
        .await
        .unwrap();
    let ct = encrypt_secret("base-secret").unwrap();
    db.insert_action_secret("sec-fk", &base.id, "API_TOKEN", &ct)
        .await
        .unwrap();

    // The workflow at head asks for write; fork runs must be clamped to read.
    let yaml = br#"
name: CI
on: [pull_request]
permissions:
  contents: write
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#;
    let bare = repos_dir.join("fkown").join("fkbase.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let git = CliGitBackend::new();
    git.init_bare(&bare, "main").await.unwrap();
    git.seed_commit(
        &bare,
        "main",
        "wf",
        &[(".github/workflows/ci.yml".into(), yaml.to_vec())],
    )
    .await
    .unwrap();
    let tip = git
        .list_refs(&bare)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.name.ends_with("main"))
        .map(|r| r.oid)
        .unwrap();
    let doc = parse_workflow_yaml(yaml).unwrap();

    let reg = mint_registration_token(&db).await.unwrap();
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"name":"r-fk","labels":["ubuntu-latest"],"token":reg})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let reg_v: serde_json::Value =
        serde_json::from_slice(&reg_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let runner_token = reg_v["runner_token"].as_str().unwrap().to_string();

    let fetch_once = |app: &axum::Router| {
        let token = runner_token.clone();
        let app = app.clone();
        async move {
            let res = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/actions/fetch_task")
                        .header("content-type", "application/json")
                        .header("authorization", format!("Bearer {token}"))
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            serde_json::from_slice::<serde_json::Value>(
                &res.into_body().collect().await.unwrap().to_bytes(),
            )
            .unwrap()
        }
    };

    // Case 1 — fork head: no secrets, write clamped to read.
    db.insert_pull(
        "pr-fork",
        &base.id,
        1,
        "fork pr",
        "",
        &owner.id,
        "main",
        "0".repeat(40).as_str(),
        &fork.id,
        "feature",
        &tip,
        false,
    )
    .await
    .unwrap();
    enqueue_run(
        &db,
        &base.id,
        ".github/workflows/ci.yml",
        &doc,
        "pull_request",
        &tip,
        "refs/heads/feature",
        Some(&owner.id),
        Some(&fork.id),
        None,
        false,
        None,
    )
    .await
    .unwrap();
    let v = fetch_once(&app).await;
    assert_eq!(v["event"], "pull_request");
    assert_eq!(v["pull_number"], 1);
    assert!(
        v["secrets"]
            .as_object()
            .map(|m| m.is_empty())
            .unwrap_or(true),
        "fork PR run must receive no secrets, got: {}",
        v["secrets"]
    );
    assert_eq!(
        v["job_token_permission"], "read",
        "fork PR write grant must clamp to read"
    );

    // Case 2 — same-repo head: secrets + write preserved.
    db.insert_pull(
        "pr-same",
        &base.id,
        2,
        "same pr",
        "",
        &owner.id,
        "main",
        "0".repeat(40).as_str(),
        &base.id,
        "feature2",
        &tip,
        false,
    )
    .await
    .unwrap();
    enqueue_run(
        &db,
        &base.id,
        ".github/workflows/ci.yml",
        &doc,
        "pull_request",
        &tip,
        "refs/heads/feature2",
        Some(&owner.id),
        Some(&base.id),
        None,
        false,
        None,
    )
    .await
    .unwrap();
    let v = fetch_once(&app).await;
    assert_eq!(v["event"], "pull_request");
    assert_eq!(v["pull_number"], 2);
    assert_eq!(v["secrets"]["API_TOKEN"], "base-secret");
    assert_eq!(v["job_token_permission"], "write");

    // Case 3 — pull_request run with no recorded provenance: fail closed.
    db.insert_action_run(
        "run-orphan",
        &base.id,
        ".github/workflows/ci.yml",
        "CI",
        "pull_request",
        "9".repeat(40).as_str(),
        "refs/heads/feature9",
        "CI",
        Some(&owner.id),
        None,
        None,
    )
    .await
    .unwrap();
    // 0053: a hand-inserted job needs a durable payload row or the claim
    // path fails it loudly instead of handing it out.
    db.insert_action_payload(
        "sha-orphan",
        &base.id,
        ".github/workflows/ci.yml",
        "9".repeat(40).as_str(),
        r#"{"version":1,"steps":[{"run":"echo hi"}],"contents_permission":"write"}"#,
    )
    .await
    .unwrap();
    let mut orphan = oxidean_db::JobInsert::basic(
        "job-orphan",
        "run-orphan",
        "build",
        "build",
        r#"["ubuntu-latest"]"#,
    );
    orphan.payload_sha = Some("sha-orphan".into());
    db.insert_action_job(orphan).await.unwrap();
    let v = fetch_once(&app).await;
    assert_eq!(v["job_id"], "job-orphan");
    assert!(
        v["secrets"]
            .as_object()
            .map(|m| m.is_empty())
            .unwrap_or(true),
        "unprovenanced pull_request run must receive no secrets, got: {}",
        v["secrets"]
    );
}

/// Phase 9 — `fetch_task` long-poll: an empty queue holds the request up to
/// `wait_ms`, an enqueue wakes it mid-hold, and `wait_ms=0` returns
/// immediately (old cadence).
#[tokio::test]
async fn actions_runner_protocol_fetch_task_long_poll() {
    let (app, db, _dir) = app_db().await;
    let reg = mint_registration_token(&db).await.unwrap();
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "r-lp",
                        "labels": ["ubuntu-latest"],
                        "token": reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let reg_v: serde_json::Value =
        serde_json::from_slice(&reg_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let runner_token = reg_v["runner_token"].as_str().unwrap().to_string();

    let fetch = |app: axum::Router, tok: String, wait_ms: u64| {
        tokio::spawn(async move {
            let res = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/actions/fetch_task")
                        .header("content-type", "application/json")
                        .header("authorization", format!("Bearer {tok}"))
                        .body(Body::from(
                            serde_json::json!({"wait_ms": wait_ms}).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
        })
    };

    // wait_ms=0 → immediate empty response (legacy short-poll shape).
    let t0 = std::time::Instant::now();
    let v = fetch(app.clone(), runner_token.clone(), 0).await.unwrap();
    assert!(v["job_id"].is_null(), "empty queue returns no job: {v}");
    assert!(
        t0.elapsed() < std::time::Duration::from_secs(2),
        "wait_ms=0 must not hold"
    );

    // Bounded hold — nothing enqueued, so a 300ms wait releases empty on its own.
    let t0 = std::time::Instant::now();
    let v = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        fetch(app.clone(), runner_token.clone(), 300),
    )
    .await
    .expect("long-poll must release on its own deadline")
    .unwrap();
    assert!(v["job_id"].is_null(), "held-then-empty response: {v}");
    assert!(
        t0.elapsed() >= std::time::Duration::from_millis(300),
        "hold released before wait_ms elapsed"
    );

    // Wake-on-enqueue — a 10s hold must cut short when a job lands.
    let pending = fetch(app.clone(), runner_token.clone(), 10_000);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        !pending.is_finished(),
        "long-poll must hold while the queue is empty"
    );

    let owner = db
        .create_user(
            "u-lp",
            "lp@example.com",
            "lp",
            Some("h"),
            "L",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-lp", &owner.id, "user", "lp", "private", "", "main")
        .await
        .unwrap();
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
    let t0 = std::time::Instant::now();
    enqueue_run(
        &db,
        &repo.id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "feedface",
        "refs/heads/main",
        Some(&owner.id),
        None,
        None,
        false,
        None,
    )
    .await
    .unwrap();
    let v = tokio::time::timeout(std::time::Duration::from_secs(5), pending)
        .await
        .expect("enqueue must wake the long-poll")
        .unwrap();
    assert!(v["job_id"].as_str().is_some(), "woken claim: {v}");
    assert!(
        t0.elapsed() < std::time::Duration::from_secs(3),
        "wake took {:?} — the 2s fallback tick shouldn't be the mechanism",
        t0.elapsed()
    );
}

/// Cross-run / cross-repository artifact downloads — the `download-artifact`
/// `github-token`/`repository`/`run-id` inputs (GitHub's v4 cross-run path).
/// Runner tokens keep same-run reads; job tokens and user PATs authorize on
/// token scope plus repository ACL.
async fn artifact_fixture() -> (
    axum::Router,
    Database,
    tempfile::TempDir,
    std::path::PathBuf,
    oxidean_db::UserRow,
    oxidean_db::RepositoryRow,
    String,
    Vec<String>,
) {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("x.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let artifacts_dir = dir.path().join("art");
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_actions_enabled(true)
    .with_actions_artifacts_dir(artifacts_dir.clone());
    let app = router_with_state(state, build_cors("development", None).unwrap());

    let owner = db
        .create_user(
            "u-art",
            "art@example.com",
            "artown",
            Some("h"),
            "A",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-art", &owner.id, "user", "artdemo", "private", "", "main")
        .await
        .unwrap();
    let doc = parse_workflow_yaml(
        br#"
name: A
on: [push]
permissions:
  contents: read
  actions: read
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
    )
    .unwrap();
    let (run_id, job_ids) = enqueue_run(
        &db,
        &repo.id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "deadbeef",
        "refs/heads/main",
        Some(&owner.id),
        None,
        None,
        false,
        None,
    )
    .await
    .unwrap();

    let rel = format!("{run_id}/art-x.tgz");
    let dest = artifacts_dir.join(&rel);
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::write(&dest, b"payload-a").unwrap();
    db.upsert_action_artifact(&oxidean_db::actions::ActionArtifactRow {
        id: "art-x".into(),
        repository_id: repo.id.clone(),
        run_id: run_id.clone(),
        job_id: Some(job_ids[0].clone()),
        name: "dist".into(),
        storage_path: rel,
        size_bytes: 9,
        digest_sha256: "ab".repeat(32),
        expires_at: None,
        state: "committed".into(),
        mime_type: "application/gzip".into(),
        created_at: String::new(),
    })
    .await
    .unwrap();
    (app, db, dir, artifacts_dir, owner, repo, run_id, job_ids)
}

async fn artifact_get(app: &axum::Router, uri: String, token: Option<&str>) -> StatusCode {
    let mut b = Request::builder().method("GET").uri(uri);
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    app.clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

async fn artifact_list_status(
    app: &axum::Router,
    run_id: &str,
    repository: Option<&str>,
    token: Option<&str>,
) -> StatusCode {
    let mut body = serde_json::json!({"run_id": run_id});
    if let Some(r) = repository {
        body["repository"] = serde_json::Value::String(r.to_string());
    }
    let mut b = Request::builder()
        .method("POST")
        .uri("/api/actions/artifacts/list")
        .header("content-type", "application/json");
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    app.clone()
        .oneshot(b.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
        .status()
}

/// Register a runner through the real endpoint and return its bearer token.
async fn register_runner(app: &axum::Router, db: &Database, name: &str) -> (String, String) {
    let reg = mint_registration_token(db).await.unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": name,
                        "labels": ["ubuntu-latest"],
                        "token": reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    (
        v["runner_id"].as_str().unwrap().to_string(),
        v["runner_token"].as_str().unwrap().to_string(),
    )
}

fn make_pat(
    user_id: &str,
    kind: &str,
    scopes: Option<&str>,
    perms: Option<&str>,
    repo_access: Option<&str>,
    repos: &[String],
) -> (String, String) {
    let prefix = if kind == "fine_grained" {
        "oxidean_fg_"
    } else {
        "oxidean_pat_"
    };
    let token = format!("{}{}", prefix, uuid::Uuid::new_v4().simple());
    let hash = oxidean_api::auth::session::sha256_hex(token.as_bytes());
    (
        token,
        serde_json::json!({
            "user_id": user_id,
            "kind": kind,
            "hash": hash,
            "scopes": scopes,
            "perms": perms,
            "repo_access": repo_access,
            "repos": repos,
        })
        .to_string(),
    )
}

async fn insert_pat(db: &Database, spec: &str) {
    let v: serde_json::Value = serde_json::from_str(spec).unwrap();
    let repos: Vec<String> = v["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap().to_string())
        .collect();
    db.create_pat(
        &uuid::Uuid::new_v4().to_string(),
        v["user_id"].as_str().unwrap(),
        v["kind"].as_str().unwrap(),
        "artifact-test",
        "oxidean_tok_test",
        v["hash"].as_str().unwrap(),
        v["scopes"].as_str(),
        None,
        v["perms"].as_str(),
        v["repo_access"].as_str(),
        None,
        &repos,
    )
    .await
    .expect("create_pat");
}

#[tokio::test]
async fn artifacts_runner_and_unauthenticated_reads() {
    let (app, db, _dir, _art_dir, _owner, _repo, run_id, job_ids) = artifact_fixture().await;
    let uri = format!("/api/actions/artifacts/{run_id}/dist");

    // No credential / garbage credential → 401.
    assert_eq!(
        artifact_get(&app, uri.clone(), None).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        artifact_get(&app, uri.clone(), Some("oxidean_pat_deadbeef")).await,
        StatusCode::UNAUTHORIZED
    );

    // An unscoped runner that owns a job in the run reads it (GitHub's
    // runtime-token path); a runner owning nothing is denied.
    let (runner_id, runner_token) = register_runner(&app, &db, "claiming").await;
    assert_eq!(
        artifact_get(&app, uri.clone(), Some(&runner_token)).await,
        StatusCode::FORBIDDEN,
        "unscoped runner with no claimed job"
    );
    let claimed = db
        .claim_queued_action_job_for_labels(&runner_id, &["ubuntu-latest".to_string()])
        .await
        .unwrap();
    assert_eq!(claimed.unwrap().id, job_ids[0]);
    assert_eq!(
        artifact_get(&app, uri.clone(), Some(&runner_token)).await,
        StatusCode::OK,
        "runner owning a job in the run"
    );

    // A runner registered to the run's repository reads without a claim.
    db.insert_action_runner(
        "rnr-scoped",
        "scoped",
        &oxidean_api::actions::tokens::hash_token("repo-scoped-secret"),
        "[]",
        Some("r-art"),
        None,
        None,
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        artifact_get(&app, uri.clone(), Some("repo-scoped-secret")).await,
        StatusCode::OK
    );

    // A runner scoped to a *different* repository is denied.
    db.insert_repository(
        "r-other",
        "u-art",
        "user",
        "otherdemo",
        "private",
        "",
        "main",
    )
    .await
    .unwrap();
    db.insert_action_runner(
        "rnr-foreign",
        "foreign",
        &oxidean_api::actions::tokens::hash_token("foreign-secret"),
        "[]",
        Some("r-other"),
        None,
        None,
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        artifact_get(&app, uri.clone(), Some("foreign-secret")).await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn artifacts_token_matrix_and_repository_param() {
    let (app, db, _dir, _art_dir, owner, repo, run_id, job_ids) = artifact_fixture().await;
    let uri = format!("/api/actions/artifacts/{run_id}/dist");

    // Job token minted against the run's job: repo-pinned + `actions: read`
    // granted by the workflow's permissions map → reads succeed.
    let run = db.find_action_run_by_id(&run_id).await.unwrap().unwrap();
    let owner_ref = oxidean_api::repo::owner_ref_for_repo(&db, &repo)
        .await
        .unwrap()
        .unwrap();
    let job = db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    let payload_row = db
        .find_action_payload(job.payload_sha.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&payload_row.payload_json).unwrap();
    let requested = oxidean_api::actions::job_tokens::permissions_from_payload(&payload);
    let minted = oxidean_api::actions::job_tokens::mint_job_token(
        &db,
        &run,
        &repo,
        &owner_ref,
        &job_ids[0],
        &requested,
    )
    .await
    .unwrap()
    .expect("contents+actions grants mint a token");
    let job_token = minted.plaintext.expect("plaintext token");
    assert_eq!(
        artifact_get(&app, uri.clone(), Some(&job_token)).await,
        StatusCode::OK,
        "repo-pinned job token with actions:read"
    );
    assert_eq!(
        artifact_list_status(&app, &run_id, None, Some(&job_token)).await,
        StatusCode::OK
    );

    // The same job token cannot read another repository's run — the pin is
    // the token's repository, not the caller's visibility.
    let repo_b = db
        .insert_repository("r-art-b", &owner.id, "user", "artb", "public", "", "main")
        .await
        .unwrap();
    db.insert_action_run(
        "run-b",
        &repo_b.id,
        ".github/workflows/ci.yml",
        "CI",
        "push",
        "cafebabe",
        "main",
        "CI",
        Some(&owner.id),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        artifact_get(
            &app,
            "/api/actions/artifacts/run-b/dist".into(),
            Some(&job_token)
        )
        .await,
        StatusCode::FORBIDDEN,
        "job token is pinned to its own repository"
    );

    // Classic PATs: `repo` scope + repository visibility passes; a non-repo
    // scope fails even though the owner can see the repository.
    let (pat_repo, spec) = make_pat(&owner.id, "classic", Some(r#"["repo"]"#), None, None, &[]);
    insert_pat(&db, &spec).await;
    assert_eq!(
        artifact_get(&app, uri.clone(), Some(&pat_repo)).await,
        StatusCode::OK,
        "classic repo PAT from the owner"
    );

    let (pat_wf, spec) = make_pat(
        &owner.id,
        "classic",
        Some(r#"["workflow"]"#),
        None,
        None,
        &[],
    );
    insert_pat(&db, &spec).await;
    assert_eq!(
        artifact_get(&app, uri.clone(), Some(&pat_wf)).await,
        StatusCode::FORBIDDEN,
        "workflow scope is not a repo-family scope"
    );

    // A repo-scoped PAT from a user who cannot see the private repo gets 404 —
    // never a revealing 403.
    let outsider = db
        .create_user(
            "u-out",
            "out@example.com",
            "outp",
            Some("h"),
            "O",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let (pat_out, spec) = make_pat(
        &outsider.id,
        "classic",
        Some(r#"["repo"]"#),
        None,
        None,
        &[],
    );
    insert_pat(&db, &spec).await;
    assert_eq!(
        artifact_get(&app, uri.clone(), Some(&pat_out)).await,
        StatusCode::NOT_FOUND,
        "private repo invisible to outsiders"
    );

    // Fine-grained PATs: `actions: read` over a selected repository passes;
    // missing grant or missing repository coverage fails.
    let (fg_ok, spec) = make_pat(
        &owner.id,
        "fine_grained",
        None,
        Some(r#"{"actions":"read"}"#),
        Some("selected"),
        &["r-art".to_string()],
    );
    insert_pat(&db, &spec).await;
    assert_eq!(
        artifact_get(&app, uri.clone(), Some(&fg_ok)).await,
        StatusCode::OK,
        "FG actions:read over the selected repo"
    );

    let (fg_nocover, spec) = make_pat(
        &owner.id,
        "fine_grained",
        None,
        Some(r#"{"actions":"read"}"#),
        Some("selected"),
        &[],
    );
    insert_pat(&db, &spec).await;
    assert_eq!(
        artifact_get(&app, uri.clone(), Some(&fg_nocover)).await,
        StatusCode::FORBIDDEN,
        "actions:read but repo not covered"
    );

    let (fg_noperm, spec) = make_pat(
        &owner.id,
        "fine_grained",
        None,
        Some(r#"{"contents":"read"}"#),
        Some("all"),
        &[],
    );
    insert_pat(&db, &spec).await;
    assert_eq!(
        artifact_get(&app, uri.clone(), Some(&fg_noperm)).await,
        StatusCode::FORBIDDEN,
        "contents:read without actions grant"
    );

    // `repository` must resolve to the run's own repository.
    assert_eq!(
        artifact_get(
            &app,
            format!("{uri}?repository=artown/artdemo"),
            Some(&pat_repo)
        )
        .await,
        StatusCode::OK,
        "matching repository param"
    );
    assert_eq!(
        artifact_get(
            &app,
            format!("{uri}?repository=artown/artb"),
            Some(&pat_repo)
        )
        .await,
        StatusCode::NOT_FOUND,
        "different repo → 404 like GitHub resolving the run inside it"
    );
    assert_eq!(
        artifact_get(&app, format!("{uri}?repository=noslash"), Some(&pat_repo)).await,
        StatusCode::NOT_FOUND,
        "malformed repository param"
    );
    assert_eq!(
        artifact_list_status(&app, &run_id, Some("artown/artdemo"), Some(&pat_repo)).await,
        StatusCode::OK
    );
    assert_eq!(
        artifact_list_status(&app, &run_id, Some("artown/artb"), Some(&pat_repo)).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        artifact_list_status(&app, &run_id, None, None).await,
        StatusCode::UNAUTHORIZED
    );
}

/// Repo-scoped registration tokens bind the runner to the repo — its claims
/// isolate to that repo's queue while instance runners keep the global one.
#[tokio::test]
async fn actions_runner_repo_scope_claim_isolation() {
    let (app, db, _dir) = app_db().await;
    let owner = db
        .create_user(
            "u-scope",
            "scope@example.com",
            "scope",
            Some("h"),
            "S",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo_a = db
        .insert_repository("r-sca", &owner.id, "user", "sca", "private", "", "main")
        .await
        .unwrap();
    let repo_b = db
        .insert_repository("r-scb", &owner.id, "user", "scb", "private", "", "main")
        .await
        .unwrap();
    let doc = parse_workflow_yaml(
        br#"name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
    )
    .unwrap();
    // One queued job per repo.
    for repo in [&repo_a, &repo_b] {
        enqueue_run(
            &db,
            &repo.id,
            ".github/workflows/ci.yml",
            &doc,
            "push",
            "deadbeef",
            "refs/heads/main",
            Some(&owner.id),
            None,
            None,
            false,
            None,
        )
        .await
        .unwrap();
    }

    // Repo-scoped token (as `POST …/actions/runners/registration-token`
    // mints) → the registered runner binds to repo A.
    let reg =
        oxidean_api::actions::mint_registration_token_for_scope(&db, "repo", Some(&repo_a.id))
            .await
            .unwrap();
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "r-scoped",
                        "labels": ["ubuntu-latest"],
                        "token": reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg_res.status(), StatusCode::OK);
    let reg_body = reg_res.into_body().collect().await.unwrap().to_bytes();
    let reg_v: serde_json::Value = serde_json::from_slice(&reg_body).unwrap();
    let scoped_runner_id = reg_v["runner_id"].as_str().unwrap().to_string();
    let scoped_token = reg_v["runner_token"].as_str().unwrap().to_string();
    let row = db
        .find_action_runner_by_id(&scoped_runner_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.repository_id.as_deref(), Some(repo_a.id.as_str()));

    // The scoped runner claims repo A's job.
    let fetch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/fetch_task")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {scoped_token}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_slice(&fetch.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let claimed_job = v["job_id"].as_str().unwrap().to_string();
    let claimed = db
        .find_action_job_by_id(&claimed_job)
        .await
        .unwrap()
        .unwrap();
    let run_a = db
        .find_action_run_by_id(&claimed.run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run_a.repository_id, repo_a.id);

    // A second repo-A runner finds nothing — only repo B's job remains and
    // cross-repo claims are refused.
    let reg2 =
        oxidean_api::actions::mint_registration_token_for_scope(&db, "repo", Some(&repo_a.id))
            .await
            .unwrap();
    let reg_res2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "r-scoped-2",
                        "labels": ["ubuntu-latest"],
                        "token": reg2
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let tok2 = serde_json::from_slice::<serde_json::Value>(
        &reg_res2.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap()["runner_token"]
        .as_str()
        .unwrap()
        .to_string();
    let fetch2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/fetch_task")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {tok2}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    let v2: serde_json::Value =
        serde_json::from_slice(&fetch2.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(
        v2["job_id"].is_null(),
        "scoped runner claimed cross-repo job"
    );

    // An instance-scoped runner still claims globally — picks up repo B.
    let reg3 = mint_registration_token(&db).await.unwrap();
    let reg_res3 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "r-global",
                        "labels": ["ubuntu-latest"],
                        "token": reg3
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let tok3 = serde_json::from_slice::<serde_json::Value>(
        &reg_res3.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap()["runner_token"]
        .as_str()
        .unwrap()
        .to_string();
    let fetch3 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/fetch_task")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {tok3}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    let v3: serde_json::Value =
        serde_json::from_slice(&fetch3.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(v3["job_id"].as_str().is_some());
}

/// REST `/actions/runners` surface — registration-token mint binds the runner
/// to the repo; list/get reshape to GitHub's runner object; delete 204.
#[tokio::test]
async fn actions_runner_repo_rest_surface() {
    let (app, db, _dir) = app_db().await;
    let owner = db
        .create_user(
            "u-rrr",
            "rrr@example.com",
            "rrr",
            Some("h"),
            "R",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    db.insert_repository("r-rrr", &owner.id, "user", "rrr", "private", "", "main")
        .await
        .unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at("u-rrr", &now).await.unwrap();
    let pat = "oxidean_pat_runner_rest".to_string();
    db.create_pat(
        "pat-rrr",
        "u-rrr",
        "classic",
        "test",
        "oxidean_pat_",
        &hex::encode(Sha256::digest(pat.as_bytes())),
        Some(r#"["repo","workflow"]"#),
        None,
        None,
        None,
        None,
        &[],
    )
    .await
    .unwrap();
    let base = "/api/v1/repos/rrr/rrr/actions/runners";

    // Mint a repo-scoped registration token via the REST route.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{base}/registration-token"))
                .header("authorization", format!("Bearer {pat}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let reg = serde_json::from_slice::<serde_json::Value>(&body).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    // Register — the runner binds to the repo.
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "rest-runner",
                        "labels": ["self-hosted", "linux"],
                        "token": reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let reg_v: serde_json::Value =
        serde_json::from_slice(&reg_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let runner_id = reg_v["runner_id"].as_str().unwrap().to_string();

    // List — GitHub `{total_count, runners}` shape.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(base)
                .header("authorization", format!("Bearer {pat}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let list: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(list["total_count"], 1);
    assert_eq!(list["runners"][0]["name"], "rest-runner");
    assert_eq!(list["runners"][0]["status"], "offline");
    assert_eq!(list["runners"][0]["busy"], false);
    assert_eq!(list["runners"][0]["labels"][0]["name"], "self-hosted");

    // Get one, then delete — 204 → gone.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("{base}/{runner_id}"))
                .header("authorization", format!("Bearer {pat}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // ---- remove-token (`config.sh remove` credential): a repo remove-token
    // authorizes DELETE on repo-bound runners without a user session; it
    // cannot register a runner and cannot touch a shared instance runner.
    let reg2 = oxidean_api::actions::mint_registration_token_for_scope(&db, "repo", Some("r-rrr"))
        .await
        .unwrap();
    let reg_res2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "rest-runner-2",
                        "labels": ["self-hosted"],
                        "token": reg2
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let runner_id2 = serde_json::from_slice::<serde_json::Value>(
        &reg_res2.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap()["runner_id"]
        .as_str()
        .unwrap()
        .to_string();
    // Shared instance runner — the repo remove-token must not reach it.
    let shared_reg = mint_registration_token(&db).await.unwrap();
    let shared_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "rest-shared",
                        "labels": ["self-hosted"],
                        "token": shared_reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let shared_id = serde_json::from_slice::<serde_json::Value>(
        &shared_res.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap()["runner_id"]
        .as_str()
        .unwrap()
        .to_string();

    async fn mint_remove(app: &axum::Router, pat: &str, base: &str) -> String {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("{base}/remove-token"))
                    .header("authorization", format!("Bearer {pat}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        serde_json::from_slice::<serde_json::Value>(
            &res.into_body().collect().await.unwrap().to_bytes(),
        )
        .unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string()
    }
    async fn rest_delete(
        app: &axum::Router,
        base: &str,
        id: &str,
        auth: Option<String>,
    ) -> StatusCode {
        let mut b = Request::builder()
            .method("DELETE")
            .uri(format!("{base}/{id}"));
        if let Some(a) = auth {
            b = b.header("authorization", a);
        }
        app.clone()
            .oneshot(b.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    let rm_bad_target = mint_remove(&app, &pat, base).await;
    assert_eq!(
        rest_delete(
            &app,
            base,
            &shared_id,
            Some(format!("Bearer {rm_bad_target}"))
        )
        .await,
        StatusCode::NOT_FOUND,
        "remove-token cannot delete a shared instance runner"
    );
    let rm_reg = mint_remove(&app, &pat, base).await;
    // A remove token is not a registration token.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "rm-as-reg",
                        "labels": ["self-hosted"],
                        "token": rm_reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let rm_ok = mint_remove(&app, &pat, base).await;
    assert_eq!(
        rest_delete(&app, base, &runner_id2, Some(format!("Bearer {rm_ok}"))).await,
        StatusCode::NO_CONTENT,
        "remove-token deletes a repo-bound runner"
    );
    // One-shot: the same token no longer authenticates.
    assert_eq!(
        rest_delete(&app, base, &runner_id2, Some(format!("Bearer {rm_ok}"))).await,
        StatusCode::UNAUTHORIZED
    );
    assert!(db
        .find_action_runner_by_id(&runner_id2)
        .await
        .unwrap()
        .is_none());

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("{base}/{runner_id}"))
                .header("authorization", format!("Bearer {pat}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("{base}/{runner_id}"))
                .header("authorization", format!("Bearer {pat}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

/// Org-scoped runners (GitHub `/orgs/{org}/actions/runners`): an
/// org-registration-token binds the runner to `owner_type='org'`; its claims
/// reach every repository owned by that org and nothing else. Management
/// routes sit behind org Admin+.
#[tokio::test]
async fn actions_org_runners_scope_and_isolation() {
    let (app, db, _dir) = app_db().await;
    let admin = db
        .create_user(
            "u-ora",
            "ora@example.com",
            "ora",
            Some("h"),
            "O",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let member = db
        .create_user(
            "u-orm",
            "orm@example.com",
            "orm",
            Some("h"),
            "M",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let other = db
        .create_user(
            "u-orx",
            "orx@example.com",
            "orx",
            Some("h"),
            "X",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let org = db
        .insert_organization("o-rnr", "rnr-org", "Runner Org", "read")
        .await
        .unwrap();
    db.insert_org_member(&org.id, &admin.id, "owner")
        .await
        .unwrap();
    db.insert_org_member(&org.id, &member.id, "member")
        .await
        .unwrap();
    let org_repo = db
        .insert_repository("r-orga", &org.id, "org", "orga", "private", "", "main")
        .await
        .unwrap();
    let user_repo = db
        .insert_repository("r-userx", &other.id, "user", "userx", "private", "", "main")
        .await
        .unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for uid in ["u-ora", "u-orm"] {
        db.set_email_verified_at(uid, &now).await.unwrap();
    }
    let admin_pat = format!("oxidean_pat_{}", uuid::Uuid::new_v4().simple());
    db.create_pat(
        "pat-ora",
        "u-ora",
        "classic",
        "test",
        "oxidean_pat_",
        &hex::encode(Sha256::digest(admin_pat.as_bytes())),
        Some(r#"["repo","workflow"]"#),
        None,
        None,
        None,
        None,
        &[],
    )
    .await
    .unwrap();
    let member_pat = format!("oxidean_pat_{}", uuid::Uuid::new_v4().simple());
    db.create_pat(
        "pat-orm",
        "u-orm",
        "classic",
        "test",
        "oxidean_pat_",
        &hex::encode(Sha256::digest(member_pat.as_bytes())),
        Some(r#"["repo","workflow"]"#),
        None,
        None,
        None,
        None,
        &[],
    )
    .await
    .unwrap();

    let base = "/api/v1/orgs/rnr-org/actions/runners";

    // Non-admin member is denied the runner surface (403 — org.forbidden).
    for (method, uri) in [
        ("GET", base.to_string()),
        ("POST", format!("{base}/registration-token")),
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("authorization", format!("Bearer {member_pat}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "{method} non-admin");
    }

    // Mint an org registration token, then register through the runner
    // protocol — the row binds owner_type='org' + owner_id=org.id.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{base}/registration-token"))
                .header("authorization", format!("Bearer {admin_pat}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let reg = serde_json::from_slice::<serde_json::Value>(
        &res.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "org-runner",
                        "labels": ["ubuntu-latest"],
                        "token": reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg_res.status(), StatusCode::OK);
    let reg_v: serde_json::Value =
        serde_json::from_slice(&reg_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let runner_id = reg_v["runner_id"].as_str().unwrap().to_string();
    let runner_token = reg_v["runner_token"].as_str().unwrap().to_string();
    let row = db
        .find_action_runner_by_id(&runner_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.owner_type.as_deref(), Some("org"));
    assert_eq!(row.owner_id.as_deref(), Some("o-rnr"));
    assert!(row.repository_id.is_none());

    // Queue a job in the user repo (older) and the org repo — the org runner
    // must skip the user repo's job and claim the org repo's.
    let doc = parse_workflow_yaml(
        b"name: CI\non: [push]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n",
    )
    .unwrap();
    for repo in [&user_repo, &org_repo] {
        enqueue_run(
            &db,
            &repo.id,
            ".github/workflows/ci.yml",
            &doc,
            "push",
            "deadbeef",
            "refs/heads/main",
            Some(&other.id),
            None,
            None,
            false,
            None,
        )
        .await
        .unwrap();
    }
    let fetch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/fetch_task")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {runner_token}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fetch.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&fetch.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["repository_name"].as_str().unwrap(), "orga");
    assert_eq!(v["repository_owner"].as_str().unwrap(), "rnr-org");
    // Nothing else in the org's queue — a second fetch comes back empty.
    let fetch2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/fetch_task")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {runner_token}"))
                .body(Body::from(r#"{"wait_ms":0}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let v2: serde_json::Value =
        serde_json::from_slice(&fetch2.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(v2["job_id"].is_null(), "org runner crossed into user repo");

    // Admin list/get see the runner; the org's repo also sees it (GitHub
    // surfaces org runners on repo runner lists).
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(base)
                .header("authorization", format!("Bearer {admin_pat}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let list: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(list["total_count"], 1);
    assert_eq!(list["runners"][0]["name"], "org-runner");
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("{base}/{runner_id}"))
                .header("authorization", format!("Bearer {admin_pat}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repos/rnr-org/orga/actions/runners")
                .header("authorization", format!("Bearer {admin_pat}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let repo_list: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(
        repo_list["runners"]
            .as_array()
            .map(|a| a.iter().any(|r| r["id"] == runner_id))
            .unwrap_or(false),
        "org runner visible on the org repo's runner list: {repo_list}"
    );

    // An org remove-token authorizes runner self-removal on the org route —
    // but cannot reach a repo-bound runner.
    let repo_reg =
        oxidean_api::actions::mint_registration_token_for_scope(&db, "repo", Some(&org_repo.id))
            .await
            .unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "repo-bound",
                        "labels": ["self-hosted"],
                        "token": repo_reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let repo_runner_id = serde_json::from_slice::<serde_json::Value>(
        &res.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap()["runner_id"]
        .as_str()
        .unwrap()
        .to_string();

    let mint_org_remove = |pat: &str| {
        let app = app.clone();
        let pat = pat.to_string();
        async move {
            let res = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("{base}/remove-token"))
                        .header("authorization", format!("Bearer {pat}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::CREATED);
            serde_json::from_slice::<serde_json::Value>(
                &res.into_body().collect().await.unwrap().to_bytes(),
            )
            .unwrap()["token"]
                .as_str()
                .unwrap()
                .to_string()
        }
    };
    let rm_wrong = mint_org_remove(&admin_pat).await;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("{base}/{repo_runner_id}"))
                .header("authorization", format!("Bearer {rm_wrong}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "org remove-token cannot delete a repo-bound runner"
    );
    // Admin-session DELETE cannot remove the repo-bound runner through the
    // org route either — each scope only manages its own runners.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("{base}/{repo_runner_id}"))
                .header("authorization", format!("Bearer {admin_pat}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let rm_ok = mint_org_remove(&admin_pat).await;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("{base}/{runner_id}"))
                .header("authorization", format!("Bearer {rm_ok}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "org remove-token deletes an org-bound runner"
    );
    assert!(db
        .find_action_runner_by_id(&runner_id)
        .await
        .unwrap()
        .is_none());
}
