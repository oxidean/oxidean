//! Phase 5 — Gitea `runner.v1` compat surface (stock act_runner protocol).
//!
//! Exercises the ConnectRPC unary endpoints at
//! `/api/actions/runner.v1.RunnerService/*` over both `application/proto` and
//! `application/json`, including the `x-runner-uuid`/`x-runner-token` auth
//! pair act_runner sends.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::actions::dispatch::enqueue_run;
use oxidean_api::actions::gitea_proto::{pb, ping_pb};
use oxidean_api::actions::mint_registration_token;
use oxidean_api::actions::parse_workflow_yaml;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use prost::Message;
use tower::ServiceExt;

const SVC: &str = "/api/actions/runner.v1.RunnerService";

async fn app_db() -> (axum::Router, Database, tempfile::TempDir) {
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
    .with_actions_log_dir(log_dir);
    let app = router_with_state(state, build_cors("development", None).unwrap());
    (app, db, dir)
}

async fn post(app: &axum::Router, proc: &str, ct: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
    post_headers(app, proc, ct, &[], body).await
}

async fn post_headers(
    app: &axum::Router,
    proc: &str,
    ct: &str,
    headers: &[(&str, String)],
    body: Vec<u8>,
) -> (StatusCode, Vec<u8>) {
    let mut b = Request::builder()
        .method("POST")
        .uri(format!("{SVC}/{proc}"))
        .header("content-type", ct);
    for (k, v) in headers {
        b = b.header(*k, v.as_str());
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, bytes.to_vec())
}

fn runner_headers(uuid: &str, token: &str) -> [(&'static str, String); 2] {
    [
        ("x-runner-uuid", uuid.to_string()),
        ("x-runner-token", token.to_string()),
    ]
}

/// Register through the JSON codec — returns (uuid, plaintext ort_ token).
async fn json_register(app: &axum::Router, db: &Database, name: &str) -> (String, String) {
    let reg = mint_registration_token(db).await.unwrap();
    let (status, bytes) = post(
        app,
        "Register",
        "application/json",
        serde_json::json!({
            "name": name,
            "token": reg,
            "labels": ["ubuntu-latest"],
            "version": "0.2.12",
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Register rejected");
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (
        v["runner"]["uuid"].as_str().unwrap().to_string(),
        v["runner"]["token"].as_str().unwrap().to_string(),
    )
}

async fn seed_repo_job(db: &Database, yaml: &[u8], key: &str) -> String {
    let owner = db
        .create_user(
            &format!("u-{key}"),
            &format!("{key}@example.com"),
            key,
            Some("h"),
            "U",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository(
            &format!("r-{key}"),
            &owner.id,
            "user",
            key,
            "private",
            "",
            "main",
        )
        .await
        .unwrap();
    let doc = parse_workflow_yaml(yaml).unwrap();
    let (run_id, _) = enqueue_run(
        db,
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
    )
    .await
    .unwrap();
    run_id
}

#[tokio::test]
async fn gitea_proto_ping_echoes() {
    let (app, _db, _dir) = app_db().await;
    // act_runner pings at register/daemon startup — proto wire.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/ping.v1.PingService/Ping")
                .header("content-type", "application/proto")
                .body(Body::from(
                    ping_pb::PingRequest {
                        data: "act_runner 0.2.12".into(),
                    }
                    .encode_to_vec(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let resp = ping_pb::PingResponse::decode(&bytes[..]).unwrap();
    assert_eq!(resp.data, "act_runner 0.2.12");

    // JSON wire too.
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/ping.v1.PingService/Ping")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"data":"hello"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["data"], "hello");
}

#[tokio::test]
async fn gitea_proto_json_round_trip() {
    let (app, db, dir) = app_db().await;
    let run_id = seed_repo_job(
        &db,
        br#"
name: CI
on: [push]
env:
  TOP: level
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        "gt1",
    )
    .await;
    let (uuid, token) = json_register(&app, &db, "gt-runner").await;

    // Declare with the act_runner header pair.
    let (status, bytes) = post_headers(
        &app,
        "Declare",
        "application/json",
        &runner_headers(&uuid, &token),
        serde_json::json!({"version": "0.2.12", "labels": ["ubuntu-latest"]})
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["runner"]["uuid"], uuid);

    // FetchTask — first call must return a task (version starts stale).
    let (status, bytes) = post_headers(
        &app,
        "FetchTask",
        "application/json",
        &runner_headers(&uuid, &token),
        serde_json::json!({"tasksVersion": -1})
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let task = v["task"].as_object().expect("task returned");
    let task_id: i64 = task["id"].as_str().unwrap().parse().unwrap();
    assert!(task_id > 0);

    // workflowPayload is a complete single-job workflow YAML.
    use base64::Engine;
    let payload_b64 = task["workflowPayload"].as_str().unwrap();
    let payload = base64::engine::general_purpose::STANDARD
        .decode(payload_b64)
        .unwrap();
    let wf: serde_yaml::Value = serde_yaml::from_slice(&payload).unwrap();
    let jobs = wf["jobs"].as_mapping().unwrap();
    assert_eq!(jobs.len(), 1, "act_runner requires exactly one job");
    let job = &jobs[serde_yaml::Value::String("build".into())];
    assert_eq!(job["steps"][0]["run"].as_str().unwrap(), "echo hi");
    assert_eq!(wf["env"]["TOP"].as_str().unwrap(), "level");

    // GitHub-shaped context struct.
    let ctx = &task["context"];
    assert_eq!(ctx["event_name"], "push");
    assert_eq!(ctx["sha"], "deadbeef");
    assert_eq!(ctx["ref"], "refs/heads/main");
    assert_eq!(ctx["ref_name"], "main");
    assert_eq!(ctx["job"], "build");
    assert_eq!(ctx["repository"], "gt1/gt1");
    assert_eq!(ctx["run_id"], run_id);

    // Watermark: a second fetch at the returned version yields no task and
    // must not claim another job.
    let version = v["tasksVersion"].as_i64().unwrap();
    let (status, bytes) = post_headers(
        &app,
        "FetchTask",
        "application/json",
        &runner_headers(&uuid, &token),
        serde_json::json!({"tasksVersion": version})
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v["task"].is_null(), "matching watermark → no task");

    // UpdateLog via the header pair — ack_index advances by row count.
    let (status, bytes) = post_headers(
        &app,
        "UpdateLog",
        "application/json",
        &runner_headers(&uuid, &token),
        serde_json::json!({
            "taskId": task_id,
            "index": 0,
            "rows": [{"content": "line one"}, {"content": "line two"}],
            "noMore": false
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ackIndex"], 2);

    // UpdateTask success with outputs.
    let (status, bytes) = post_headers(
        &app,
        "UpdateTask",
        "application/json",
        &runner_headers(&uuid, &token),
        serde_json::json!({
            "state": {"id": task_id, "result": 1},
            "outputs": {"artifact_path": "out.bin"}
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["state"]["result"], 1);
    assert_eq!(v["sentOutputs"], serde_json::json!(["artifact_path"]));

    let job = db
        .list_action_jobs_for_run(&run_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(job.status, "success");
    let outputs: serde_json::Value = serde_json::from_str(&job.outputs_json).unwrap();
    assert_eq!(outputs["artifact_path"], "out.bin");

    // Log landed on disk.
    let log_path = dir
        .path()
        .join("actions-logs")
        .join(&run_id)
        .join(format!("{}.log", job.id));
    let log = std::fs::read_to_string(log_path).unwrap();
    assert!(log.contains("line one\nline two\n"));
}

#[tokio::test]
async fn gitea_proto_binary_round_trip() {
    let (app, db, _dir) = app_db().await;
    let run_id = seed_repo_job(
        &db,
        br#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo proto
"#,
        "gt2",
    )
    .await;

    // Register over application/proto.
    let reg = mint_registration_token(&db).await.unwrap();
    let req = pb::RegisterRequest {
        name: "proto-runner".into(),
        token: reg,
        version: "0.2.12".into(),
        labels: vec!["ubuntu-latest".into()],
        ephemeral: false,
        capabilities: vec![],
        ..Default::default()
    };
    let (status, bytes) = post(&app, "Register", "application/proto", req.encode_to_vec()).await;
    assert_eq!(status, StatusCode::OK);
    let resp = pb::RegisterResponse::decode(&bytes[..]).unwrap();
    let runner = resp.runner.unwrap();
    assert!(runner.token.starts_with("ort_"));
    assert_eq!(runner.uuid.len(), 36);

    let (status, bytes) = post_headers(
        &app,
        "FetchTask",
        "application/proto",
        &runner_headers(&runner.uuid, &runner.token),
        pb::FetchTaskRequest { tasks_version: -1 }.encode_to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp = pb::FetchTaskResponse::decode(&bytes[..]).unwrap();
    let task = resp.task.expect("task returned");
    assert!(task.id > 0);
    let wf: serde_yaml::Value = serde_yaml::from_slice(&task.workflow_payload.unwrap()).unwrap();
    assert_eq!(
        wf["jobs"]["build"]["steps"][0]["run"].as_str().unwrap(),
        "echo proto"
    );
    let ctx = task.context.unwrap();
    assert_eq!(
        ctx.fields["event_name"].kind,
        Some(prost_types::value::Kind::StringValue("push".into()))
    );

    // UpdateLog + UpdateTask over proto.
    let (status, bytes) = post_headers(
        &app,
        "UpdateLog",
        "application/proto",
        &runner_headers(&runner.uuid, &runner.token),
        pb::UpdateLogRequest {
            task_id: task.id,
            index: 0,
            rows: vec![pb::LogRow {
                time: None,
                content: "proto log".into(),
            }],
            no_more: false,
        }
        .encode_to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        pb::UpdateLogResponse::decode(&bytes[..]).unwrap().ack_index,
        1
    );

    let (status, bytes) = post_headers(
        &app,
        "UpdateTask",
        "application/proto",
        &runner_headers(&runner.uuid, &runner.token),
        pb::UpdateTaskRequest {
            state: Some(pb::TaskState {
                id: task.id,
                result: pb::Result::Success as i32,
                ..Default::default()
            }),
            outputs: [("k".to_string(), "v".to_string())].into_iter().collect(),
        }
        .encode_to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp = pb::UpdateTaskResponse::decode(&bytes[..]).unwrap();
    assert_eq!(resp.state.unwrap().result, pb::Result::Success as i32);

    let job = db
        .list_action_jobs_for_run(&run_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(job.status, "success");
}

#[tokio::test]
async fn gitea_proto_auth_and_ownership() {
    let (app, db, _dir) = app_db().await;
    let _run_id = seed_repo_job(
        &db,
        br#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        "gt3",
    )
    .await;
    let (uuid, token) = json_register(&app, &db, "owner-runner").await;
    let (uuid2, token2) = json_register(&app, &db, "other-runner").await;

    // Missing auth → unauthenticated.
    let (status, _) = post(&app, "FetchTask", "application/json", b"{}".to_vec()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Wrong token for a real uuid → unauthenticated.
    let (status, _) = post_headers(
        &app,
        "FetchTask",
        "application/json",
        &runner_headers(&uuid, "ort_wrong"),
        b"{}".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Claim the job.
    let (status, bytes) = post_headers(
        &app,
        "FetchTask",
        "application/json",
        &runner_headers(&uuid, &token),
        serde_json::json!({"tasksVersion": -1})
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let task_id: i64 = v["task"]["id"].as_str().unwrap().parse().unwrap();

    // Second runner cannot update the claimed task.
    let (status, _) = post_headers(
        &app,
        "UpdateTask",
        "application/json",
        &runner_headers(&uuid2, &token2),
        serde_json::json!({"state": {"id": task_id, "result": 1}})
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = post_headers(
        &app,
        "UpdateLog",
        "application/json",
        &runner_headers(&uuid2, &token2),
        serde_json::json!({"taskId": task_id, "index": 0, "rows": []})
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Unknown task id → not_found.
    let (status, _) = post_headers(
        &app,
        "UpdateTask",
        "application/json",
        &runner_headers(&uuid, &token),
        serde_json::json!({"state": {"id": 999999, "result": 1}})
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn gitea_proto_cancel_reflects_to_runner() {
    let (app, db, _dir) = app_db().await;
    let run_id = seed_repo_job(
        &db,
        br#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        "gt4",
    )
    .await;
    let (uuid, token) = json_register(&app, &db, "cancel-runner").await;
    let (status, bytes) = post_headers(
        &app,
        "FetchTask",
        "application/json",
        &runner_headers(&uuid, &token),
        serde_json::json!({"tasksVersion": -1})
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let task_id: i64 = v["task"]["id"].as_str().unwrap().parse().unwrap();

    // Server-side cancel wins the race — the runner's next UpdateTask poll
    // (result unspecified = still running) must read back CANCELLED.
    let job_id = db
        .list_action_jobs_for_run(&run_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .id;
    db.update_action_job_status(&job_id, "cancelled")
        .await
        .unwrap();

    let (status, bytes) = post_headers(
        &app,
        "UpdateTask",
        "application/json",
        &runner_headers(&uuid, &token),
        serde_json::json!({"state": {"id": task_id, "result": 0}})
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["state"]["result"], 3, "RESULT_CANCELLED echoes back");
}

#[tokio::test]
async fn gitea_proto_needs_and_matrix_narrowing() {
    let (app, db, _dir) = app_db().await;
    let _run_id = seed_repo_job(
        &db,
        br#"
name: CI
on: [push]
jobs:
  prep:
    runs-on: ubuntu-latest
    outputs:
      ver: ${{ steps.s.outputs.ver }}
    steps:
      - id: s
        run: echo "ver=1.2" >> $GITHUB_OUTPUT
  build:
    needs: prep
    strategy:
      matrix:
        os: [ubuntu-latest]
        shard: [1, 2]
    runs-on: ${{ matrix.os }}
    steps:
      - run: echo "shard ${{ matrix.shard }} needs ${{ needs.prep.outputs.ver }}"
"#,
        "gt5",
    )
    .await;
    let (uuid, token) = json_register(&app, &db, "nx-runner").await;
    let headers = runner_headers(&uuid, &token);
    let fetch = || async {
        let (status, bytes) = post_headers(
            &app,
            "FetchTask",
            "application/json",
            &headers,
            serde_json::json!({"tasksVersion": -1})
                .to_string()
                .into_bytes(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["task"].clone()
    };

    // First claim: prep. Complete it with an output.
    let t = fetch().await;
    let tid: i64 = t["id"].as_str().unwrap().parse().unwrap();
    let wf: serde_yaml::Value = serde_yaml::from_slice(
        &base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            t["workflowPayload"].as_str().unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(wf["jobs"].get("prep").is_some());
    let (status, _) = post_headers(
        &app,
        "UpdateTask",
        "application/json",
        &headers,
        serde_json::json!({"state": {"id": tid, "result": 1}, "outputs": {"ver": "1.2"}})
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Scheduler flips build legs to queued — poll fetch (version bumped by
    // the enqueue/requeue watermark). Claim the first build leg.
    let t = fetch().await;
    let wf: serde_yaml::Value = serde_yaml::from_slice(
        &base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            t["workflowPayload"].as_str().unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let jobs = wf["jobs"].as_mapping().unwrap();
    assert_eq!(jobs.len(), 1);
    let build = &jobs[serde_yaml::Value::String("build".into())];
    // strategy narrowed to exactly one shard — act_runner re-expands it.
    let matrix = &build["strategy"]["matrix"];
    let shards = matrix["shard"].as_sequence().unwrap();
    assert_eq!(shards.len(), 1);
    assert_eq!(matrix["os"].as_sequence().unwrap().len(), 1);

    // needs context carries prep's evaluated outputs + success result.
    let need = &t["needs"]["prep"];
    assert_eq!(need["result"], 1);
    assert_eq!(need["outputs"]["ver"], "1.2");
}
