//! ev.deployment / ev.deployment_status: the public Deployments RPC surface —
//! create/list/get/delete, status flow, `auto_inactive`, `auto_merge`,
//! `required_contexts`, ACL + repo isolation — plus the `deployment` and
//! `deployment_status` lifecycle/webhook fan-out.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, FileChange, GitBackend};
use tower::ServiceExt;

async fn test_app(db: Database, repos_dir: std::path::PathBuf) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir);
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
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

fn rpc_req_with_cookie(body: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("cookie", cookie)
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn session_cookie_from_response(res: &axum::http::Response<Body>) -> String {
    let set_cookie = res
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap();
    set_cookie.split(';').next().unwrap().trim().to_string()
}

async fn signup_and_login(
    app: &axum::Router,
    email: &str,
    username: &str,
) -> (String, serde_json::Value) {
    let signup_body = format!(
        r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
    );
    let signup = app.clone().oneshot(rpc_req(&signup_body)).await.unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;

    let login_body = format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
    );
    let login = app.clone().oneshot(rpc_req(&login_body)).await.unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (cookie, v)
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn create_repo(app: &axum::Router, cookie: &str, name: &str, visibility: &str) {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":""}}}}"#
    );
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(&body, cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

/// Seed `{repos_dir}/{owner}/{name}.git` — the path `bare_repo_path` derives —
/// with a root commit on `main`. Returns the tip sha.
async fn seed_repo_git(
    repos_dir: &std::path::Path,
    owner: &str,
    name: &str,
    files: &[(&str, &[u8])],
) -> (CliGitBackend, std::path::PathBuf, String) {
    let bare = repos_dir.join(owner).join(format!("{name}.git"));
    let git = CliGitBackend::new();
    git.init_bare(&bare, "main").await.expect("init bare");
    let owned: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(p, c)| ((*p).to_string(), c.to_vec()))
        .collect();
    git.seed_commit(&bare, "main", "seed", &owned)
        .await
        .expect("seed commit");
    let tip = git.rev_parse(&bare, "refs/heads/main").await.expect("tip");
    (git, bare, tip)
}

async fn run_count(db: &Database, repo_id: &str) -> usize {
    db.list_action_runs_for_repo(repo_id, &Default::default(), 100, 0)
        .await
        .unwrap()
        .len()
}

/// create/list/get, status flow, non-transient supersede, delete gating.
#[tokio::test]
async fn deployment_crud_statuses_and_supersede() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("dep_crud.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "dep1@ex.com", "dep1").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "demo", "public").await;
    let (_git, _bare, _tip) = seed_repo_git(
        &repos_dir,
        "dep1",
        "demo",
        &[("README.md", b"hi".as_slice())],
    )
    .await;

    // Create on `main` — ref resolves to the seeded sha.
    let d1 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"deployment.create","input":{"owner":"dep1","name":"demo","ref":"main","auto_merge":false,"required_contexts":[],"environment":"staging","task":"deploy","description":"first"}}"#,
    )
    .await;
    assert_eq!(d1["ok"], true, "deployment.create — {d1}");
    assert_eq!(d1["data"]["environment"], "staging");
    assert_eq!(d1["data"]["task"], "deploy");
    assert_eq!(d1["data"]["ref"], "refs/heads/main");
    assert!(d1["data"]["run_id"].is_null());
    assert!(d1["data"]["job_id"].is_null());
    assert!(d1["data"]["state"].is_null());
    assert_eq!(d1["data"]["creator_username"], "dep1");
    let d1_id = d1["data"]["id"].as_str().unwrap().to_string();
    let d1_sha = d1["data"]["sha"].as_str().unwrap().to_string();

    // The environment row auto-created on first reference (job-path parity).
    let envs = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.actions.environments.list","input":{"owner":"dep1","name":"demo"}}"#,
    )
    .await;
    assert!(envs["data"]["environments"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["name"] == "staging"));

    // env filter + get.
    let list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"deployment.list","input":{"owner":"dep1","name":"demo","environment":"staging"}}"#,
    )
    .await;
    assert_eq!(list["data"]["deployments"].as_array().unwrap().len(), 1);
    let prod = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"deployment.list","input":{"owner":"dep1","name":"demo","environment":"production"}}"#,
    )
    .await;
    assert_eq!(prod["data"]["deployments"].as_array().unwrap().len(), 0);
    let got = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.get","input":{{"owner":"dep1","name":"demo","id":"{d1_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(got["data"]["sha"], d1_sha);

    // Status flow — in_progress lands, list is newest-first, `state` projects.
    let s1 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.status.create","input":{{"owner":"dep1","name":"demo","deployment_id":"{d1_id}","state":"in_progress","description":"deploying"}}}}"#
        ),
    )
    .await;
    assert_eq!(s1["ok"], true, "{s1}");
    assert_eq!(s1["data"]["state"], "in_progress");
    let st = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.status.list","input":{{"owner":"dep1","name":"demo","deployment_id":"{d1_id}"}}}}"#
        ),
    )
    .await;
    let statuses = st["data"]["statuses"].as_array().unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0]["state"], "in_progress");

    // A non-transient deployment supersedes the prior one (inactive status).
    let d2 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"deployment.create","input":{"owner":"dep1","name":"demo","ref":"main","auto_merge":false,"required_contexts":[],"environment":"staging"}}"#,
    )
    .await;
    assert_eq!(d2["ok"], true, "{d2}");
    let d2_id = d2["data"]["id"].as_str().unwrap().to_string();
    let got1 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.get","input":{{"owner":"dep1","name":"demo","id":"{d1_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(got1["data"]["state"], "inactive");

    // Delete gating — latest non-inactive blocks; inactive frees.
    let bad_state = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.status.create","input":{{"owner":"dep1","name":"demo","deployment_id":"{d2_id}","state":"bogus"}}}}"#
        ),
    )
    .await;
    assert_eq!(bad_state["ok"], false);
    assert_eq!(bad_state["error"]["code"], "rpc.bad_input");
    let _ = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.status.create","input":{{"owner":"dep1","name":"demo","deployment_id":"{d2_id}","state":"in_progress"}}}}"#
        ),
    )
    .await;
    let blocked = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.delete","input":{{"owner":"dep1","name":"demo","id":"{d2_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(blocked["ok"], false);
    assert_eq!(blocked["error"]["code"], "deployment.active");
    let _ = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.status.create","input":{{"owner":"dep1","name":"demo","deployment_id":"{d2_id}","state":"inactive"}}}}"#
        ),
    )
    .await;
    let del = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.delete","input":{{"owner":"dep1","name":"demo","id":"{d2_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(del["data"]["ok"], true);
}

/// `auto_inactive` on success, `auto_merge`, `required_contexts`.
#[tokio::test]
async fn deployment_auto_merge_contexts_and_auto_inactive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("dep_am.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "dep2@ex.com", "dep2").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "demo", "public").await;
    let (git, bare, _tip) = seed_repo_git(
        &repos_dir,
        "dep2",
        "demo",
        &[("README.md", b"hi".as_slice())],
    )
    .await;

    // Feature branch off the root; main advances one commit ahead.
    git.commit_files(
        &bare,
        "feature",
        "main",
        "feature work",
        &[FileChange::Upsert {
            path: "f.txt".into(),
            content: b"f".to_vec(),
        }],
        "Test",
        "t@example.com",
        None,
        "admin",
    )
    .await
    .expect("feature commit");
    git.commit_files(
        &bare,
        "main",
        "main",
        "main advances",
        &[FileChange::Upsert {
            path: "m.txt".into(),
            content: b"m".to_vec(),
        }],
        "Test",
        "t@example.com",
        None,
        "admin",
    )
    .await
    .expect("main commit");
    let feature_before = git.rev_parse(&bare, "refs/heads/feature").await.unwrap();

    // Failing context blocks create by default…
    let repo_id = db
        .find_repository_by_owner_name(login_v["data"]["id"].as_str().unwrap(), "demo")
        .await
        .unwrap()
        .unwrap()
        .id;
    db.upsert_commit_status(
        "cs1",
        &repo_id,
        &feature_before,
        "ci/test",
        "failure",
        "red",
        None,
        None,
    )
    .await
    .unwrap();
    let blocked = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"deployment.create","input":{"owner":"dep2","name":"demo","ref":"feature","environment":"staging"}}"#,
    )
    .await;
    assert_eq!(blocked["ok"], false);
    assert_eq!(blocked["error"]["code"], "deployment.required_contexts");

    // `required_contexts: []` skips verification; auto_merge merges main in.
    let d1 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"deployment.create","input":{"owner":"dep2","name":"demo","ref":"feature","environment":"staging","required_contexts":[]}}"#,
    )
    .await;
    assert_eq!(d1["ok"], true, "{d1}");
    let feature_after = git.rev_parse(&bare, "refs/heads/feature").await.unwrap();
    assert_ne!(
        feature_before, feature_after,
        "auto-merge must move the ref"
    );
    assert_eq!(d1["data"]["sha"], feature_after);
    let d1_id = d1["data"]["id"].as_str().unwrap().to_string();

    // success + default auto_inactive marks any other env deployment
    // inactive — but there is only one here, so create a sibling first.
    let d2 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"deployment.create","input":{"owner":"dep2","name":"demo","ref":"feature","environment":"staging","required_contexts":[],"auto_merge":false,"transient_environment":true}}"#,
    )
    .await;
    assert_eq!(d2["ok"], true, "{d2}");
    let d2_id = d2["data"]["id"].as_str().unwrap().to_string();
    // transient: the prior deployment was NOT auto-inactivated by the create.
    let got1 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.get","input":{{"owner":"dep2","name":"demo","id":"{d1_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(got1["data"]["state"], serde_json::Value::Null);

    let succ = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.status.create","input":{{"owner":"dep2","name":"demo","deployment_id":"{d1_id}","state":"success","log_url":"https://logs.example/x"}}}}"#
        ),
    )
    .await;
    assert_eq!(succ["ok"], true, "{succ}");
    // auto_inactive — the transient sibling gets an inactive status.
    let got2 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.get","input":{{"owner":"dep2","name":"demo","id":"{d2_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(got2["data"]["state"], "inactive");
}

/// ACL + repo isolation + ref resolution errors.
#[tokio::test]
async fn deployment_acl_isolation_and_ref_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("dep_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie_a, login_a) = signup_and_login(&app, "own3@ex.com", "own3").await;
    verify_user(&db, login_a["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie_a, "demo", "public").await;
    create_repo(&app, &cookie_a, "other", "public").await;
    let _ = seed_repo_git(
        &repos_dir,
        "own3",
        "demo",
        &[("README.md", b"hi".as_slice())],
    )
    .await;

    let d = rpc_json(
        &app,
        &cookie_a,
        r#"{"procedure":"deployment.create","input":{"owner":"own3","name":"demo","ref":"main","auto_merge":false,"required_contexts":[]}}"#,
    )
    .await;
    assert_eq!(d["ok"], true, "{d}");
    let dep_id = d["data"]["id"].as_str().unwrap().to_string();

    // Same id from a different repo → not_found (isolation).
    let cross = rpc_json(
        &app,
        &cookie_a,
        &format!(
            r#"{{"procedure":"deployment.get","input":{{"owner":"own3","name":"other","id":"{dep_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(cross["ok"], false);
    assert_eq!(cross["error"]["code"], "deployment.not_found");

    // Unknown ref → ref_not_found.
    let badref = rpc_json(
        &app,
        &cookie_a,
        r#"{"procedure":"deployment.create","input":{"owner":"own3","name":"demo","ref":"refs/heads/nowhere","auto_merge":false,"required_contexts":[]}}"#,
    )
    .await;
    assert_eq!(badref["ok"], false);
    assert_eq!(badref["error"]["code"], "deployment.ref_not_found");

    // Outsider on a public repo: read ok, write denied.
    let (cookie_b, login_b) = signup_and_login(&app, "out3@ex.com", "out3").await;
    verify_user(&db, login_b["data"]["id"].as_str().unwrap()).await;
    let read = rpc_json(
        &app,
        &cookie_b,
        &format!(
            r#"{{"procedure":"deployment.get","input":{{"owner":"own3","name":"demo","id":"{dep_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(read["ok"], true, "{read}");
    let write = rpc_json(
        &app,
        &cookie_b,
        r#"{"procedure":"deployment.create","input":{"owner":"own3","name":"demo","ref":"main","auto_merge":false,"required_contexts":[]}}"#,
    )
    .await;
    assert_eq!(write["ok"], false);
    assert_eq!(write["error"]["code"], "repo.not_found");
}

/// `on: deployment` / `on: deployment_status` triggers + webhook deliveries.
#[tokio::test]
async fn deployment_events_trigger_runs_and_webhooks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("dep_ev.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "dep4@ex.com", "dep4").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "demo", "public").await;

    let workflow = br#"
name: DeployTrig
on:
  deployment: {}
  deployment_status: {}
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo deploy
"#;
    let _ = seed_repo_git(
        &repos_dir,
        "dep4",
        "demo",
        &[
            ("README.md", b"hi".as_slice()),
            (".github/workflows/deploy.yml", workflow.as_slice()),
        ],
    )
    .await;

    // Webhook subscription for both events (deliveries land sync at emit).
    let hook = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"webhook.create","input":{"owner":"dep4","name":"demo","url":"http://127.0.0.1:9/hook","secret":"s","events":["deployment","deployment_status"]}}"#,
    )
    .await;
    assert_eq!(hook["ok"], true, "{hook}");
    let hook_id = hook["data"]["id"].as_str().unwrap().to_string();

    let repo_id = db
        .find_repository_by_owner_name(login_v["data"]["id"].as_str().unwrap(), "demo")
        .await
        .unwrap()
        .unwrap()
        .id;
    assert_eq!(run_count(&db, &repo_id).await, 0);

    // deployment.create → `deployment` webhook + run + initial status-less
    // (API deployments carry no backing job; first status is caller-made).
    let d = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"deployment.create","input":{"owner":"dep4","name":"demo","ref":"main","auto_merge":false,"required_contexts":[],"environment":"prod"}}"#,
    )
    .await;
    assert_eq!(d["ok"], true, "{d}");
    let dep_id = d["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(run_count(&db, &repo_id).await, 1, "on: deployment run");

    // status.create → `deployment_status` webhook + run.
    let s = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"deployment.status.create","input":{{"owner":"dep4","name":"demo","deployment_id":"{dep_id}","state":"success"}}}}"#
        ),
    )
    .await;
    assert_eq!(s["ok"], true, "{s}");
    assert_eq!(
        run_count(&db, &repo_id).await,
        2,
        "on: deployment_status run"
    );

    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    assert!(runs.iter().any(|r| r.event == "deployment"));
    assert!(runs.iter().any(|r| r.event == "deployment_status"));

    // Delivery rows land synchronously at emit (spawn is async after).
    let deliveries = db.list_webhook_deliveries(&hook_id, 20).await.unwrap();
    let events: Vec<&str> = deliveries.iter().map(|d| d.event.as_str()).collect();
    assert!(events.contains(&"deployment"), "deliveries: {events:?}");
    assert!(
        events.contains(&"deployment_status"),
        "deliveries: {events:?}"
    );

    // Payload shape — GitHub's `created` action + deployment/repository/sender.
    let dep_delivery = deliveries
        .iter()
        .find(|d| d.event == "deployment")
        .expect("deployment delivery");
    let payload: serde_json::Value =
        serde_json::from_str(&dep_delivery.payload_json).expect("payload json");
    assert_eq!(payload["action"], "created");
    assert_eq!(payload["deployment"]["id"], dep_id);
    assert_eq!(payload["deployment"]["environment"], "prod");
    assert_eq!(payload["repository"]["name"], "demo");
    assert_eq!(payload["sender"]["login"], "dep4");
}

/// GitHub's GITHUB_TOKEN non-recursion rule: a job-token caller can create
/// the deployment, but the `on: deployment` dispatch it causes is suppressed.
#[tokio::test]
async fn deployment_job_token_does_not_recurse() {
    std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "dep-jt-test-key");
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("dep_jt.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    // The job token resolves to the run's triggering user — create + verify
    // it directly so `require_verified` passes.
    let owner = db
        .create_user(
            "u-dep-jt",
            "depjt@example.com",
            "depjt",
            Some("h"),
            "JT",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository("r-dep-jt", &owner.id, "user", "demo", "private", "", "main")
        .await
        .expect("repo");
    db.set_email_verified_at(&owner.id, "2026-01-01T00:00:00Z")
        .await
        .unwrap();

    // `on: deployment` listener in the bare repo — would fire if dispatch ran.
    let _ = seed_repo_git(
        &repos_dir,
        "depjt",
        "demo",
        &[
            ("README.md", b"hi".as_slice()),
            (
                ".github/workflows/deploy.yml",
                br#"
name: DeployTrig
on:
  deployment: {}
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo deploy
"#
                .as_slice(),
            ),
        ],
    )
    .await;

    // Source job — mint a token scoped `deployments: write` so create passes
    // the DeploymentsWrite gate (`contents` grant is what makes the token
    // plaintext injectable at all).
    let doc = oxidean_api::actions::parse_workflow_yaml(
        br#"
name: S
on: [push]
jobs:
  work:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      deployments: write
    steps:
      - run: echo hi
"#,
    )
    .expect("parse");
    let (run_id, job_ids) = oxidean_api::actions::dispatch::enqueue_run(
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
    )
    .await
    .expect("enqueue");
    let run = db.find_action_run_by_id(&run_id).await.unwrap().unwrap();
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
    let job_payload: serde_json::Value = serde_json::from_str(&payload_row.payload_json).unwrap();
    let requested = oxidean_api::actions::job_tokens::permissions_from_payload(&job_payload);
    let owner_ref = oxidean_api::repo::owner_ref_for_repo(&db, &repo)
        .await
        .unwrap()
        .unwrap();
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
    .unwrap();
    let token = minted
        .plaintext
        .expect("deployments:write injects plaintext");

    // deployment.create with the job token — the row lands…
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    r#"{"procedure":"deployment.create","input":{"owner":"depjt","name":"demo","ref":"main","auto_merge":false,"required_contexts":[]}}"#.to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["ok"], true, "deployment.create via job token — {v}");

    // …but the `on: deployment` workflow did not enqueue (only the push run
    // that minted the token exists).
    let runs = db
        .list_action_runs_for_repo(&repo.id, &Default::default(), 100, 0)
        .await
        .unwrap();
    assert_eq!(
        runs.len(),
        1,
        "runs: {:?}",
        runs.iter().map(|r| &r.event).collect::<Vec<_>>()
    );
    assert_eq!(runs[0].event, "push");
}
