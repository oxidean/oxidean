//! Checks domain: the public Checks RPC surface — suites, runs,
//! annotations, `on: check_run`/`on: check_suite` triggers, webhook
//! fan-out, the Actions-job → check-run projection, branch-protection
//! context folding, and GITHUB_TOKEN non-recursion.

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

/// Seed `{repos_dir}/{owner}/{name}.git` with a root commit on `main`.
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

/// Suite/run CRUD, annotations, filters, validation, ACL + isolation.
#[tokio::test]
async fn check_crud_annotations_and_acl() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("chk_crud.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "chk1@ex.com", "chk1").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "demo", "public").await;
    create_repo(&app, &cookie, "other", "public").await;
    let (_git, _bare, tip) = seed_repo_git(
        &repos_dir,
        "chk1",
        "demo",
        &[("README.md", b"hi".as_slice())],
    )
    .await;

    // check.suites.create — `api` app label, requested event fires.
    let suite = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.suites.create","input":{{"owner":"chk1","name":"demo","head_sha":"{tip}"}}}}"#
        ),
    )
    .await;
    assert_eq!(suite["ok"], true, "check.suites.create — {suite}");
    assert_eq!(suite["data"]["app"], "api");
    assert_eq!(suite["data"]["status"], "queued");
    assert!(suite["data"]["run_id"].is_null());
    let suite_id = suite["data"]["id"].as_str().unwrap().to_string();

    // check.runs.create — auto-suite when none supplied? GitHub creates the
    // run under the app's suite for the commit; our explicit suite still
    // lists it once the run lands under the shared `api` suite.
    let run = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.create","input":{{"owner":"chk1","name":"demo","check_name":"ci/test","head_sha":"{tip}","external_id":"ext-1","details_url":"https://ci.example/1","output":{{"title":"t","summary":"s","annotations":[{{"path":"src/lib.rs","start_line":3,"end_line":4,"annotation_level":"warning","message":"watch it","title":"lint"}}]}}}}}}"#
        ),
    )
    .await;
    assert_eq!(run["ok"], true, "check.runs.create — {run}");
    assert_eq!(run["data"]["name"], "ci/test");
    assert_eq!(run["data"]["status"], "queued");
    assert_eq!(run["data"]["app"], "api");
    assert_eq!(run["data"]["output"]["annotations_count"], 1);
    let run_id = run["data"]["id"].as_str().unwrap().to_string();
    // Auto-suite shares the (repo, sha) `api` suite — same row as the
    // explicit create above.
    assert_eq!(run["data"]["check_suite_id"], suite_id);

    // annotations.list — the stored row round-trips.
    let ann = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.annotations.list","input":{{"owner":"chk1","name":"demo","check_run_id":"{run_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(ann["ok"], true, "{ann}");
    let anns = ann["data"]["annotations"].as_array().unwrap();
    assert_eq!(anns.len(), 1);
    assert_eq!(anns[0]["path"], "src/lib.rs");
    assert_eq!(anns[0]["annotation_level"], "warning");
    assert_eq!(anns[0]["start_line"], 3);

    // Validation — completed requires conclusion; bad conclusion rejected;
    // conclusion without completed rejected.
    let bad = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.update","input":{{"owner":"chk1","name":"demo","id":"{run_id}","status":"completed"}}}}"#
        ),
    )
    .await;
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["error"]["code"], "rpc.bad_input");
    let bad2 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.update","input":{{"owner":"chk1","name":"demo","id":"{run_id}","conclusion":"bogus"}}}}"#
        ),
    )
    .await;
    assert_eq!(bad2["ok"], false);
    assert_eq!(bad2["error"]["code"], "rpc.bad_input");

    // update → in_progress + output fields, then completed+success.
    let upd = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.update","input":{{"owner":"chk1","name":"demo","id":"{run_id}","status":"in_progress","started_at":"2026-02-01T00:00:00Z"}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["status"], "in_progress");
    assert_eq!(upd["data"]["started_at"], "2026-02-01T00:00:00Z");
    let done = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.update","input":{{"owner":"chk1","name":"demo","id":"{run_id}","status":"completed","conclusion":"success","completed_at":"2026-02-01T00:05:00Z","output":{{"summary":"all green"}}}}}}"#
        ),
    )
    .await;
    assert_eq!(done["ok"], true, "{done}");
    assert_eq!(done["data"]["conclusion"], "success");
    assert_eq!(done["data"]["output"]["summary"], "all green");

    // Suite rolled up completed+success.
    let suite2 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.suites.get","input":{{"owner":"chk1","name":"demo","id":"{suite_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(suite2["data"]["status"], "completed");
    assert_eq!(suite2["data"]["conclusion"], "success");
    assert_eq!(suite2["data"]["check_runs_count"], 1);

    // listForRef — ref resolves, `check_name`/`conclusion` filter.
    let lfr = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"check.runs.listForRef","input":{"owner":"chk1","name":"demo","ref":"main","check_name":"ci/test","conclusion":"success"}}"#,
    )
    .await;
    assert_eq!(lfr["ok"], true, "{lfr}");
    assert_eq!(lfr["data"]["check_runs"].as_array().unwrap().len(), 1);
    let lfr_none = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"check.runs.listForRef","input":{"owner":"chk1","name":"demo","ref":"main","check_name":"other"}}"#,
    )
    .await;
    assert_eq!(lfr_none["data"]["check_runs"].as_array().unwrap().len(), 0);

    // listForSuite — `status` filter.
    let lfs = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.listForSuite","input":{{"owner":"chk1","name":"demo","suite_id":"{suite_id}","status":"completed"}}}}"#
        ),
    )
    .await;
    assert_eq!(lfs["data"]["check_runs"].as_array().unwrap().len(), 1);

    // rerequest — emits the event; state unchanged.
    let rr = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.rerequest","input":{{"owner":"chk1","name":"demo","id":"{run_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(rr["ok"], true, "{rr}");
    assert_eq!(rr["data"]["status"], "completed");

    // Isolation — cross-repo id → not_found; unknown ref → ref_not_found.
    let cross = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.get","input":{{"owner":"chk1","name":"other","id":"{run_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(cross["ok"], false);
    assert_eq!(cross["error"]["code"], "check.run_not_found");
    let badref = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"check.runs.listForRef","input":{"owner":"chk1","name":"demo","ref":"refs/heads/nowhere"}}"#,
    )
    .await;
    assert_eq!(badref["ok"], false);
    assert_eq!(badref["error"]["code"], "check.ref_not_found");

    // Outsider on a public repo: read ok, write denied.
    let (cookie_b, login_b) = signup_and_login(&app, "out4@ex.com", "out4").await;
    verify_user(&db, login_b["data"]["id"].as_str().unwrap()).await;
    let read = rpc_json(
        &app,
        &cookie_b,
        &format!(
            r#"{{"procedure":"check.runs.get","input":{{"owner":"chk1","name":"demo","id":"{run_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(read["ok"], true, "{read}");
    let write = rpc_json(
        &app,
        &cookie_b,
        &format!(
            r#"{{"procedure":"check.runs.update","input":{{"owner":"chk1","name":"demo","id":"{run_id}","status":"completed","conclusion":"failure"}}}}"#
        ),
    )
    .await;
    assert_eq!(write["ok"], false);
    assert_eq!(write["error"]["code"], "repo.not_found");
}

/// `on: check_run` / `on: check_suite` triggers + webhook deliveries.
#[tokio::test]
async fn check_events_trigger_runs_and_webhooks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("chk_ev.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "chk2@ex.com", "chk2").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "demo", "public").await;

    let workflow = br#"
name: CheckTrig
on:
  check_run: {}
  check_suite: {}
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo checks
"#;
    let (_git, _bare, tip) = seed_repo_git(
        &repos_dir,
        "chk2",
        "demo",
        &[
            ("README.md", b"hi".as_slice()),
            (".github/workflows/checks.yml", workflow.as_slice()),
        ],
    )
    .await;

    let hook = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"webhook.create","input":{"owner":"chk2","name":"demo","url":"http://127.0.0.1:9/hook","secret":"s","events":["check_run","check_suite"]}}"#,
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

    // check.suites.create → `check_suite.requested` webhook + run.
    let suite = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.suites.create","input":{{"owner":"chk2","name":"demo","head_sha":"{tip}"}}}}"#
        ),
    )
    .await;
    assert_eq!(suite["ok"], true, "{suite}");
    assert_eq!(run_count(&db, &repo_id).await, 1, "on: check_suite run");

    // check.runs.create → `check_run.created` webhook + run.
    let run = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.create","input":{{"owner":"chk2","name":"demo","check_name":"lint","head_sha":"{tip}","status":"in_progress"}}}}"#
        ),
    )
    .await;
    assert_eq!(run["ok"], true, "{run}");
    let run_id = run["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(run_count(&db, &repo_id).await, 2, "on: check_run run");

    // Completed transition → `check_run.completed` + `check_suite.completed`
    // (suite rollup) — two more runs.
    let done = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.update","input":{{"owner":"chk2","name":"demo","id":"{run_id}","status":"completed","conclusion":"failure"}}}}"#
        ),
    )
    .await;
    assert_eq!(done["ok"], true, "{done}");
    assert_eq!(
        run_count(&db, &repo_id).await,
        4,
        "completed → check_run + check_suite runs"
    );

    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    assert!(runs.iter().any(|r| r.event == "check_run"));
    assert!(runs.iter().any(|r| r.event == "check_suite"));

    // Deliveries — check_run created + completed, suite requested +
    // completed.
    let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.unwrap();
    let events: Vec<&str> = deliveries.iter().map(|d| d.event.as_str()).collect();
    assert!(events.contains(&"check_run"), "deliveries: {events:?}");
    assert!(events.contains(&"check_suite"), "deliveries: {events:?}");

    // Payload shape — `created` action + GitHub check_run fields. The
    // triggered workflow's own job also projects a `created` run ("d"),
    // so filter to the API-created name.
    let cr_delivery = deliveries
        .iter()
        .find(|d| {
            d.event == "check_run"
                && d.payload_json.contains("\"action\":\"created\"")
                && d.payload_json.contains("\"name\":\"lint\"")
        })
        .expect("check_run created delivery");
    let payload: serde_json::Value =
        serde_json::from_str(&cr_delivery.payload_json).expect("payload json");
    assert_eq!(payload["action"], "created");
    assert_eq!(payload["check_run"]["name"], "lint");
    assert_eq!(payload["check_run"]["status"], "in_progress");
    assert_eq!(payload["check_run"]["app"]["slug"], "api");
    assert_eq!(payload["repository"]["name"], "demo");
    assert_eq!(payload["sender"]["login"], "chk2");

    let cs_delivery = deliveries
        .iter()
        .find(|d| {
            d.event == "check_suite"
                && d.payload_json.contains("\"action\":\"requested\"")
                && d.payload_json.contains("\"slug\":\"api\"")
        })
        .expect("check_suite requested delivery");
    let spayload: serde_json::Value =
        serde_json::from_str(&cs_delivery.payload_json).expect("payload json");
    assert_eq!(spayload["action"], "requested");
    assert_eq!(spayload["check_suite"]["app"]["slug"], "api");
}

/// Actions jobs project check runs/suites — created at emit, completed at
/// the terminal transition, suite rolled up, job annotations folded in.
#[tokio::test]
async fn check_actions_job_projection() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("chk_proj.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;

    let owner = db
        .create_user(
            "u-chk-p",
            "chkp@example.com",
            "chkp",
            Some("h"),
            "P",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository("r-chk-p", &owner.id, "user", "demo", "private", "", "main")
        .await
        .expect("repo");

    let doc = oxidean_api::actions::parse_workflow_yaml(
        br#"
name: S
on: [push]
jobs:
  work:
    runs-on: ubuntu-latest
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
    assert_eq!(job_ids.len(), 1);

    // enqueue emitted workflow_job(queued) → suite + check run projected.
    let suite = db
        .find_check_suite_for_run(&run_id)
        .await
        .unwrap()
        .expect("suite projected");
    assert_eq!(suite.app, "github-actions");
    assert_eq!(suite.run_id.as_deref(), Some(run_id.as_str()));
    assert_eq!(suite.head_sha, "deadbeef");
    assert_eq!(suite.status, "queued");

    let check = db
        .find_check_run_by_job(&job_ids[0])
        .await
        .unwrap()
        .expect("check run projected");
    assert_eq!(check.check_suite_id, suite.id);
    assert_eq!(check.name, "work");
    assert_eq!(check.status, "queued");
    assert_eq!(check.run_id.as_deref(), Some(run_id.as_str()));

    // Terminal transition → check completed + suite rollup.
    db.update_action_job_status(&job_ids[0], "success")
        .await
        .unwrap();
    oxidean_api::webhook::dispatch::notify_workflow_job(&db, &job_ids[0], "completed").await;

    let check2 = db.find_check_run(&check.id).await.unwrap().unwrap();
    assert_eq!(check2.status, "completed");
    assert_eq!(check2.conclusion.as_deref(), Some("success"));
    let suite2 = db.find_check_suite(&suite.id).await.unwrap().unwrap();
    assert_eq!(suite2.status, "completed");
    assert_eq!(suite2.conclusion.as_deref(), Some("success"));

    // Job annotations surface through check.annotations.list — a stored
    // `action_annotations` row (log-extracted `::error::`) folds in.
    db.insert_action_annotation(&oxidean_db::actions::ActionAnnotationRow {
        id: "ann-1".into(),
        run_id: run_id.clone(),
        job_id: job_ids[0].clone(),
        annotation_level: "error".into(),
        message: "boom".into(),
        path: Some("src/main.rs".into()),
        start_line: Some(10),
        end_line: Some(10),
        start_column: None,
        end_column: None,
        title: Some("compile".into()),
        created_at: "2026-01-01T00:00:00Z".into(),
    })
    .await
    .unwrap();
    let count = oxidean_api::check::annotation_count(&db, &check2).await;
    assert_eq!(count, 1);
}

/// A check run satisfies a branch-protection `required_status_contexts`
/// name — merge unblocks without a commit status.
#[tokio::test]
async fn check_run_satisfies_required_context() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("chk_bp.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login) = signup_and_login(&app, "bpchk@ex.com", "bpchk").await;
    verify_user(&db, login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let br = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"bpchk","name":"core","branch":"feature","start":"main"}}"#,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    let rule = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"bpchk","name":"core","pattern":"main","require_reviews":false,"required_status_contexts":["ci/test"],"enforce_admins":true}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "{rule}");

    let pr = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.create","input":{"owner":"bpchk","name":"core","title":"CI","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();
    let head = pr["data"]["head_sha"].as_str().unwrap().to_string();

    let merge = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"bpchk","name":"core","number":{n},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(merge["ok"], false, "{merge}");
    assert_eq!(merge["error"]["code"], "pull.merge_blocked");

    // A completed+success check run named `ci/test` on the head sha
    // satisfies the required context — no commit status needed.
    let run = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.create","input":{{"owner":"bpchk","name":"core","check_name":"ci/test","head_sha":"{head}","status":"completed","conclusion":"success"}}}}"#
        ),
    )
    .await;
    assert_eq!(run["ok"], true, "{run}");

    let merge2 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"bpchk","name":"core","number":{n},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(merge2["ok"], true, "merge after check run — {merge2}");
}

/// GitHub's GITHUB_TOKEN non-recursion rule: a job-token caller can create
/// check runs, but the `on: check_run` dispatch it causes is suppressed.
#[tokio::test]
async fn check_job_token_does_not_recurse() {
    std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "chk-jt-test-key");
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("chk_jt.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let owner = db
        .create_user(
            "u-chk-jt",
            "chkjt@example.com",
            "chkjt",
            Some("h"),
            "JT",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository("r-chk-jt", &owner.id, "user", "demo", "private", "", "main")
        .await
        .expect("repo");
    db.set_email_verified_at(&owner.id, "2026-01-01T00:00:00Z")
        .await
        .unwrap();

    let (_git, _bare, tip_sha) = seed_repo_git(
        &repos_dir,
        "chkjt",
        "demo",
        &[
            ("README.md", b"hi".as_slice()),
            (
                ".github/workflows/checks.yml",
                br#"
name: CheckTrig
on:
  check_run: {}
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo checks
"#
                .as_slice(),
            ),
        ],
    )
    .await;

    // `checks: write` job → plaintext token for the RPC call.
    let doc = oxidean_api::actions::parse_workflow_yaml(
        br#"
name: S
on: [push]
jobs:
  work:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      checks: write
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
    let token = minted.plaintext.expect("checks:write injects plaintext");

    // check.runs.create with the job token — the row lands…
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(format!(
                    r#"{{"procedure":"check.runs.create","input":{{"owner":"chkjt","name":"demo","check_name":"tok","head_sha":"{tip_sha}"}}}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["ok"], true, "check.runs.create via job token — {v}");

    // …but the `on: check_run` workflow did not enqueue (only the push run
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
