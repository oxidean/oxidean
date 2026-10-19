//! Actions UI surface — deployment review (`reviewDeployment` + recorded
//! history + `can_review`/`waiting_reason`), artifact list/delete/download,
//! job-log download, repo-level deployments, and dispatch-input metadata.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::actions::dispatch::enqueue_run;
use oxidean_api::actions::parse_workflow_yaml;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, GitBackend};
use tower::ServiceExt;

fn ensure_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "p12-ui-test-key");
    });
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

async fn signup(app: &axum::Router, db: &Database, email: &str, username: &str) -> String {
    let r = rpc_json(
        app,
        "",
        &format!(
            r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], true, "signup — {r}");
    let user_id = r["data"]["id"].as_str().expect("user id");
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

struct Fx {
    db: Database,
    app: axum::Router,
    repos_dir: std::path::PathBuf,
    artifacts_dir: std::path::PathBuf,
    log_dir: std::path::PathBuf,
    owner_cookie: String,
    owner_id: String,
    _dir: tempfile::TempDir,
}

async fn fixture() -> Fx {
    ensure_env();
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    let artifacts_dir = dir.path().join("artifacts");
    let log_dir = dir.path().join("logs");
    std::fs::create_dir_all(&artifacts_dir).unwrap();
    std::fs::create_dir_all(&log_dir).unwrap();
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_actions_enabled(true)
    .with_repos_dir(repos_dir.clone())
    .with_actions_artifacts_dir(artifacts_dir.clone())
    .with_actions_log_dir(log_dir.clone());
    let app = router_with_state(state, build_cors("development", None).unwrap());
    let owner_cookie = signup(&app, &db, "uiowner@example.com", "uiowner").await;
    let owner = db
        .find_user_by_username("uiowner")
        .await
        .unwrap()
        .expect("owner row");
    db.insert_repository("r-ui", &owner.id, "user", "uidemo", "private", "", "main")
        .await
        .unwrap();
    Fx {
        db,
        app,
        repos_dir,
        artifacts_dir,
        log_dir,
        owner_cookie,
        owner_id: owner.id,
        _dir: dir,
    }
}

async fn enqueue(db: &Database, actor_id: &str, yaml: &[u8]) -> (String, Vec<String>) {
    let doc = parse_workflow_yaml(yaml).expect("parse");
    enqueue_run(
        db,
        "r-ui",
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "deadbeef",
        "refs/heads/main",
        Some(actor_id),
        None,
        None,
        false,
        None,
    )
    .await
    .expect("enqueue")
}

async fn held_deploy_job(fx: &Fx, env_protection: &str, actor_id: &str) -> (String, String) {
    fx.db
        .upsert_action_environment("env-ui", "r-ui", "production", env_protection)
        .await
        .unwrap();
    let (run_id, job_ids) = enqueue(
        &fx.db,
        actor_id,
        br#"
name: D
on: [push]
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: production
    steps:
      - run: echo deploy
"#,
    )
    .await;
    let job = fx
        .db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert!(job.approval_hold, "env-held leg is approval_hold");
    (run_id, job_ids[0].clone())
}

#[tokio::test]
async fn review_deployment_approves_and_records_history() {
    let fx = fixture().await;
    // Reviewer is a second repo-level collaborator — grant Write via a direct
    // membership row isn't needed: the reviewer gate is `required_reviewers`
    // membership, capability Write+ still applies, so make the reviewer a
    // second signed-up user with Write (repo owner capability isn't
    // user-scoped; collaborator insert gives Write).
    let reviewer_cookie = signup(&fx.app, &fx.db, "rev@example.com", "uirev").await;
    let reviewer = fx.db.find_user_by_username("uirev").await.unwrap().unwrap();
    fx.db
        .insert_repo_collaborator("r-ui", &reviewer.id, "write")
        .await
        .unwrap();
    let (_run_id, job_id) = held_deploy_job(
        &fx,
        &format!(r#"{{"required_reviewers":["{}"]}}"#, reviewer.id),
        &fx.owner_id,
    )
    .await;

    // Owner (Admin capability) may not self-approve *before* reviewer gate?
    // Admin overrides reviewer membership — but the owner triggered the run
    // and prevent_self_review is unset, so owner may approve too.
    let res = rpc_json(
        &fx.app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"repo.actions.reviewDeployment","input":{{"owner":"uiowner","name":"uidemo","job_id":"{job_id}","state":"approved","comment":"ship it"}}}}"#
        ),
    )
    .await;
    assert_eq!(res["ok"], true, "reviewer approve — {res}");
    let job = &res["data"]["job"];
    assert_eq!(job["waiting_reason"], serde_json::Value::Null);
    assert_eq!(job["status"], "queued");
    assert_eq!(job["environment"], "production");
    let reviews = job["reviews"].as_array().expect("reviews");
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0]["reviewer"], "uirev");
    assert_eq!(reviews[0]["state"], "approved");
    assert_eq!(reviews[0]["comment"], "ship it");

    // getRun exposes the recorded review + released state.
    let res = rpc_json(
        &fx.app,
        &fx.owner_cookie,
        &format!(
            r#"{{"procedure":"repo.actions.getRun","input":{{"owner":"uiowner","name":"uidemo","run_id":"{}"}}}}"#,
            job["run_id"].as_str().unwrap()
        ),
    )
    .await;
    let jobs = res["data"]["jobs"].as_array().unwrap();
    assert_eq!(jobs[0]["reviews"][0]["state"], "approved");
}

#[tokio::test]
async fn review_deployment_membership_and_self_review_gates() {
    let fx = fixture().await;
    let outsider_cookie = signup(&fx.app, &fx.db, "out@example.com", "uiout").await;
    let outsider = fx.db.find_user_by_username("uiout").await.unwrap().unwrap();
    fx.db
        .insert_repo_collaborator("r-ui", &outsider.id, "write")
        .await
        .unwrap();
    let reviewer_cookie = signup(&fx.app, &fx.db, "gate@example.com", "uigate").await;
    let reviewer = fx
        .db
        .find_user_by_username("uigate")
        .await
        .unwrap()
        .unwrap();
    fx.db
        .insert_repo_collaborator("r-ui", &reviewer.id, "write")
        .await
        .unwrap();

    // prevent_self_review + required_reviewers — the reviewer triggered the
    // run, so they may not self-review; the outsider isn't a reviewer at all.
    let (_run_id, job_id) = held_deploy_job(
        &fx,
        &format!(
            r#"{{"required_reviewers":["{}"],"prevent_self_review":true}}"#,
            reviewer.id
        ),
        &reviewer.id,
    )
    .await;

    let res = rpc_json(
        &fx.app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"repo.actions.reviewDeployment","input":{{"owner":"uiowner","name":"uidemo","job_id":"{job_id}","state":"approved"}}}}"#
        ),
    )
    .await;
    assert_eq!(res["ok"], false);
    assert_eq!(
        res["error"]["code"], "repo.actions.self_review_forbidden",
        "triggering actor blocked — {res}"
    );

    let res = rpc_json(
        &fx.app,
        &outsider_cookie,
        &format!(
            r#"{{"procedure":"repo.actions.reviewDeployment","input":{{"owner":"uiowner","name":"uidemo","job_id":"{job_id}","state":"approved"}}}}"#
        ),
    )
    .await;
    assert_eq!(res["ok"], false);
    assert_eq!(
        res["error"]["code"], "repo.actions.not_a_reviewer",
        "non-reviewer blocked — {res}"
    );

    // can_review is exposed per-caller: owner (Admin, non-triggering actor)
    // sees true; the blocked reviewer sees false.
    let job_row = fx.db.find_action_job_by_id(&job_id).await.unwrap().unwrap();
    let res = rpc_json(
        &fx.app,
        &fx.owner_cookie,
        &format!(
            r#"{{"procedure":"repo.actions.getRun","input":{{"owner":"uiowner","name":"uidemo","run_id":"{}"}}}}"#,
            job_row.run_id
        ),
    )
    .await;
    let jobs = res["data"]["jobs"].as_array().unwrap();
    assert_eq!(jobs[0]["waiting_reason"], "environment_review");
    assert_eq!(jobs[0]["can_review"], true, "admin override can review");

    let res = rpc_json(
        &fx.app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"repo.actions.getRun","input":{{"owner":"uiowner","name":"uidemo","run_id":"{}"}}}}"#,
            job_row.run_id
        ),
    )
    .await;
    let jobs = res["data"]["jobs"].as_array().unwrap();
    assert_eq!(
        jobs[0]["can_review"], false,
        "triggering actor cannot self-review"
    );

    // Admin override releases it.
    let res = rpc_json(
        &fx.app,
        &fx.owner_cookie,
        &format!(
            r#"{{"procedure":"repo.actions.reviewDeployment","input":{{"owner":"uiowner","name":"uidemo","job_id":"{job_id}","state":"rejected","comment":"not today"}}}}"#
        ),
    )
    .await;
    assert_eq!(res["ok"], true, "admin reject — {res}");
    let job = &res["data"]["job"];
    assert_eq!(job["status"], "failure");
    assert_eq!(job["reviews"][0]["state"], "rejected");
    let job_row = fx.db.find_action_job_by_id(&job_id).await.unwrap().unwrap();
    assert_eq!(job_row.status, "failure");
    assert!(!job_row.approval_hold);
}

#[tokio::test]
async fn artifacts_list_download_delete_roundtrip() {
    let fx = fixture().await;
    let (run_id, _job_ids) = enqueue(
        &fx.db,
        &fx.owner_id,
        br#"
name: A
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo build
"#,
    )
    .await;

    // Seed an artifact row + payload the way the upload endpoint does.
    let rel = format!("{run_id}/art-1.tgz");
    let dest = fx.artifacts_dir.join(&rel);
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::write(&dest, b"fake-tarball").unwrap();
    fx.db
        .upsert_action_artifact(&oxidean_db::actions::ActionArtifactRow {
            id: "art-1".into(),
            repository_id: "r-ui".into(),
            run_id: run_id.clone(),
            job_id: None,
            name: "dist".into(),
            storage_path: rel,
            size_bytes: 12,
            digest_sha256: "ab".repeat(32),
            expires_at: None,
            created_at: String::new(),
        })
        .await
        .unwrap();

    let res = rpc_json(
        &fx.app,
        &fx.owner_cookie,
        &format!(
            r#"{{"procedure":"repo.actions.listArtifacts","input":{{"owner":"uiowner","name":"uidemo","run_id":"{run_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(res["ok"], true, "listArtifacts — {res}");
    let artifacts = res["data"]["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["name"], "dist");
    assert_eq!(artifacts[0]["size_bytes"], 12);
    assert_eq!(artifacts[0]["digest_sha256"], "ab".repeat(32));

    // Browser download route: session cookie → gzip body + disposition.
    let res = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/repos/uiowner/uidemo/actions/artifacts/art-1")
                .header("cookie", &fx.owner_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cd = res
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(cd.contains("dist.tar.gz"), "disposition — {cd}");
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"fake-tarball");

    // Anonymous on a private repo → 404.
    let res = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/repos/uiowner/uidemo/actions/artifacts/art-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Delete removes row + payload file.
    let res = rpc_json(
        &fx.app,
        &fx.owner_cookie,
        r#"{"procedure":"repo.actions.deleteArtifact","input":{"owner":"uiowner","name":"uidemo","artifact_id":"art-1"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "deleteArtifact — {res}");
    assert_eq!(res["data"]["deleted"], true);
    assert!(!dest.exists(), "payload file removed");
    let res = rpc_json(
        &fx.app,
        &fx.owner_cookie,
        &format!(
            r#"{{"procedure":"repo.actions.listArtifacts","input":{{"owner":"uiowner","name":"uidemo","run_id":"{run_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(res["data"]["artifacts"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn job_log_download_route() {
    let fx = fixture().await;
    let (run_id, job_ids) = enqueue(
        &fx.db,
        &fx.owner_id,
        br#"
name: L
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo build
"#,
    )
    .await;
    oxidean_api::actions::append_job_log(
        &fx.log_dir,
        &run_id,
        &job_ids[0],
        b"hello from the job\n",
    )
    .await
    .unwrap();

    let res = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/repos/uiowner/uidemo/actions/jobs/{}/logs",
                    job_ids[0]
                ))
                .header("cookie", &fx.owner_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/plain"), "content-type — {ct}");
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello from the job\n");
}

#[tokio::test]
async fn repo_deployments_listing_and_env_filter() {
    let fx = fixture().await;
    fx.db
        .upsert_action_environment("e-ui", "r-ui", "production", "{}")
        .await
        .unwrap();
    let (run_id, job_ids) = enqueue(
        &fx.db,
        &fx.owner_id,
        br#"
name: E
on: [push]
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: production
    steps:
      - run: echo deploy
"#,
    )
    .await;

    // Repo-level listing (no run_id) + env filter.
    let res = rpc_json(
        &fx.app,
        &fx.owner_cookie,
        r#"{"procedure":"repo.actions.listDeployments","input":{"owner":"uiowner","name":"uidemo"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "repo deployments — {res}");
    let deps = res["data"]["deployments"].as_array().unwrap();
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0]["environment"], "production");
    assert_eq!(deps[0]["run_id"], run_id);
    assert_eq!(deps[0]["job_id"], job_ids[0]);

    let res = rpc_json(
        &fx.app,
        &fx.owner_cookie,
        r#"{"procedure":"repo.actions.listDeployments","input":{"owner":"uiowner","name":"uidemo","environment":"staging"}}"#,
    )
    .await;
    assert_eq!(res["data"]["deployments"].as_array().unwrap().len(), 0);

    let res = rpc_json(
        &fx.app,
        &fx.owner_cookie,
        &format!(
            r#"{{"procedure":"repo.actions.listDeployments","input":{{"owner":"uiowner","name":"uidemo","run_id":"{run_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(res["data"]["deployments"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn list_workflows_exposes_typed_dispatch_inputs() {
    let fx = fixture().await;
    let bare = fx.repos_dir.join("uiowner").join("uidemo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let git = CliGitBackend::new();
    git.init_bare(&bare, "main").await.unwrap();
    git.seed_commit(
        &bare,
        "main",
        "workflows",
        &[(
            ".github/workflows/deploy.yml".into(),
            br#"
name: Deploy
on:
  workflow_dispatch:
    inputs:
      environment:
        type: environment
        required: true
        description: Target env
      dry_run:
        type: boolean
        default: false
      retries:
        type: number
        default: 3
      channel:
        type: choice
        options: [stable, beta]
        default: stable
      reason:
        type: string
jobs:
  go:
    runs-on: ubuntu-latest
    steps:
      - run: echo go
"#
            .to_vec(),
        )],
    )
    .await
    .unwrap();

    let res = rpc_json(
        &fx.app,
        &fx.owner_cookie,
        r#"{"procedure":"repo.actions.listWorkflows","input":{"owner":"uiowner","name":"uidemo"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "listWorkflows — {res}");
    let wf = &res["data"]["workflows"][0];
    assert_eq!(wf["supports_dispatch"], true);
    let inputs = wf["inputs"].as_array().expect("inputs");
    assert_eq!(inputs.len(), 5);
    let by_name = |n: &str| inputs.iter().find(|i| i["name"] == n).unwrap();
    let env = by_name("environment");
    assert_eq!(env["type"], "environment");
    assert_eq!(env["required"], true);
    assert_eq!(env["description"], "Target env");
    let dry = by_name("dry_run");
    assert_eq!(dry["type"], "boolean");
    assert_eq!(dry["default"], false);
    assert_eq!(by_name("retries")["default"], 3);
    let ch = by_name("channel");
    assert_eq!(ch["type"], "choice");
    assert_eq!(ch["options"], serde_json::json!(["stable", "beta"]));
    assert_eq!(by_name("reason")["type"], "string");
}

/// Live log tail over SSE — `GET …/logs/stream` pushes a snapshot frame, a
/// frame per append, and a `done` frame when the job goes terminal. The wake
/// channel on log append makes the stream real-time rather than polled.
#[tokio::test]
async fn job_log_stream_sse_pushes_appends() {
    use futures_util::StreamExt;

    let fx = fixture().await;
    let (run_id, job_ids) = enqueue(
        &fx.db,
        &fx.owner_id,
        br#"
name: T
on: [push]
jobs:
  tail:
    runs-on: ubuntu-latest
    steps:
      - run: echo tail
"#,
    )
    .await;
    let job_id = &job_ids[0];
    // Seed one line so the connect-time snapshot is deterministic.
    oxidean_api::actions::logs::append_job_log(&fx.log_dir, &run_id, job_id, b"line one\n")
        .await
        .unwrap();
    let url = format!("/api/repos/uiowner/uidemo/actions/jobs/{job_id}/logs/stream");

    // Anonymous on a private repo → 404, same as the raw download.
    let res = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Owner session → 200 text/event-stream.
    let res = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&url)
                .header("cookie", &fx.owner_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "text/event-stream"
    );

    // Collect the body until a marker appears (bounded by a deadline).
    async fn read_until<S, E>(stream: &mut S, buf: &mut String, needle: &str)
    where
        S: futures_util::Stream<Item = Result<axum::body::Bytes, E>> + Unpin,
        E: std::fmt::Debug,
    {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        while !buf.contains(needle) {
            let chunk = tokio::time::timeout_at(deadline, stream.next())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {needle:?} — got {buf}"))
                .expect("stream ended early")
                .expect("body chunk");
            buf.push_str(&String::from_utf8_lossy(&chunk));
        }
    }
    let mut stream = res.into_body().into_data_stream();
    let mut buf = String::new();
    // Snapshot frame: full content at offset 0, still live.
    read_until(&mut stream, &mut buf, "\"offset\":0").await;
    assert!(
        buf.contains("line one"),
        "snapshot carries the seed — {buf}"
    );
    assert!(buf.contains("\"done\":false"), "job is live — {buf}");

    // An append wakes the stream — pushed bytes arrive without a client poll.
    oxidean_api::actions::logs::append_job_log(&fx.log_dir, &run_id, job_id, b"line two\n")
        .await
        .unwrap();
    read_until(&mut stream, &mut buf, "line two").await;
    let frames_at_append = buf.matches("\"offset\":").count();
    assert_eq!(
        frames_at_append, 2,
        "snapshot + append frames so far — {buf}"
    );

    // A rerun wipes the attempt log — the shrunk file triggers an offset:0
    // replacement snapshot so the client rebuilds instead of appending.
    let log_file = oxidean_api::actions::logs::job_log_path(&fx.log_dir, &run_id, job_id).unwrap();
    tokio::fs::write(&log_file, b"fresh attempt\n")
        .await
        .unwrap();
    read_until(&mut stream, &mut buf, "fresh attempt").await;
    let tail = &buf[buf.rfind("\"offset\":").unwrap()..];
    assert!(
        tail.starts_with("\"offset\":0,") || tail.starts_with("\"offset\":0}"),
        "wipe resends a replacement snapshot — {tail}"
    );

    // Terminal status ends the stream with a done frame.
    fx.db
        .update_action_job_status(job_id, "success")
        .await
        .unwrap();
    read_until(&mut stream, &mut buf, "\"done\":true").await;
}
