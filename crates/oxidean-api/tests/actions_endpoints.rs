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

async fn test_app(
    db: Database,
    repos_dir: std::path::PathBuf,
    actions_log_dir: std::path::PathBuf,
    actions_artifacts_dir: std::path::PathBuf,
) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir)
        .with_actions_log_dir(actions_log_dir)
        .with_actions_artifacts_dir(actions_artifacts_dir);
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
    log_dir: std::path::PathBuf,
    artifacts_dir: std::path::PathBuf,
    cookie: String,
    owner_username: String,
    owner_id: String,
    _dir: tempfile::TempDir,
}

async fn fixture(visibility: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    let log_dir = dir.path().join("alogs");
    let artifacts_dir = dir.path().join("artifacts");
    let app = test_app(
        db.clone(),
        repos_dir.clone(),
        log_dir.clone(),
        artifacts_dir.clone(),
    )
    .await;
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
        log_dir,
        artifacts_dir,
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
            None,
            None,
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
                None,
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
            None,
            None,
        )
        .await
        .unwrap();
    fx.db
        .insert_action_job(oxidean_db::JobInsert::basic(
            "job-1",
            "run-j",
            "build",
            "build",
            r#"["ubuntu-latest"]"#,
        ))
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

/// `repo.actions.dispatchRepository` — GitHub's repository_dispatch surface:
/// `event_type` is the activity type, `client_payload` lands verbatim in
/// `github.event.client_payload`, and `types:` filters apply.
#[tokio::test]
async fn dispatch_repository_enqueues_run() {
    let fx = fixture("public").await;
    let bare = fx.repos_dir.join(&fx.owner_username).join("actdemo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let git = CliGitBackend::new();
    git.init_bare(&bare, "main").await.unwrap();
    git.seed_commit(
        &bare,
        "main",
        "workflows",
        &[(
            ".github/workflows/deploy-hook.yml".into(),
            br#"
name: Deploy Hook
on:
  repository_dispatch:
    types: [deploy]
jobs:
  go:
    runs-on: ubuntu-latest
    steps:
      - run: echo deployed
"#
            .to_vec(),
        )],
    )
    .await
    .unwrap();

    // Anonymous dispatch must fail (Write+).
    let anon = rpc_json(
        &fx.app,
        "",
        r#"{"procedure":"repo.actions.dispatchRepository","input":{"owner":"actowner","name":"actdemo","event_type":"deploy"}}"#,
    )
    .await;
    assert_eq!(anon["ok"], false, "anonymous dispatch — {anon}");

    // event_type mismatch — `types: [deploy]` filters `build` out.
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.dispatchRepository","input":{"owner":"actowner","name":"actdemo","event_type":"build"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "dispatch — {res}");
    assert!(fx
        .db
        .list_action_runs_for_repo("r-act", &Default::default(), 10, 0)
        .await
        .unwrap()
        .is_empty());

    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.dispatchRepository","input":{"owner":"actowner","name":"actdemo","event_type":"deploy","client_payload":{"env":"prod","unit":42}}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "dispatch — {res}");

    let mut run = None;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let runs = fx
            .db
            .list_action_runs_for_repo("r-act", &Default::default(), 10, 0)
            .await
            .unwrap();
        if let Some(r) = runs.into_iter().find(|r| r.event == "repository_dispatch") {
            run = Some(r);
            break;
        }
    }
    let run = run.expect("repository_dispatch run enqueued");
    let payload: serde_json::Value =
        serde_json::from_str(run.event_payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(payload["action"], "deploy");
    assert_eq!(payload["client_payload"]["env"], "prod");
    assert_eq!(payload["client_payload"]["unit"], 42);
    assert_eq!(payload["repository"]["name"], "actdemo");

    // event_type validation.
    let bad = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.dispatchRepository","input":{"owner":"actowner","name":"actdemo","event_type":"  "}}"#,
    )
    .await;
    assert_eq!(bad["ok"], false, "blank event_type — {bad}");
    assert_eq!(bad["error"]["code"], "rpc.bad_input");
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
            None,
            None,
        )
        .await
        .unwrap();
    fx.db
        .insert_action_job(oxidean_db::JobInsert::basic(
            "job-m",
            "run-m",
            "build",
            "build",
            r#"["ubuntu-latest"]"#,
        ))
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
    // The interrupted job's commit status flips pending → error so required
    // checks don't hang on a cancelled run.
    let statuses = fx
        .db
        .list_commit_statuses("r-act", "deadbeef")
        .await
        .unwrap();
    let st = statuses
        .iter()
        .find(|s| s.context == "CI / build")
        .expect("cancel publishes a commit status for the job");
    assert_eq!(st.state, "error");

    // Seed the dead attempt's leftovers — log files (plain + compressed),
    // outputs/summary, and the append cursor — so the rerun's wipe and
    // state reset are observable rather than vacuous.
    let log_path = fx.log_dir.join("run-m").join("job-m.log");
    std::fs::create_dir_all(log_path.parent().unwrap()).unwrap();
    std::fs::write(&log_path, "attempt-1 output\n").unwrap();
    std::fs::write(log_path.with_extension("log.gz"), b"gzip-bytes").unwrap();
    fx.db
        .set_action_job_outputs("job-m", r#"{"out":"v"}"#)
        .await
        .unwrap();
    fx.db
        .set_action_job_summary("job-m", "# old")
        .await
        .unwrap();
    fx.db
        .touch_action_job_heartbeat(
            "job-m",
            &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        )
        .await
        .unwrap();
    assert!(
        fx.db
            .set_action_job_log_index_if("job-m", 0, 7)
            .await
            .unwrap(),
        "log-index seed"
    );

    // Rerun requeues run + jobs and replaces the prior attempt's state.
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
    assert_eq!(job.log_index, 0, "append cursor resets");
    assert_eq!(job.outputs_json, "{}", "outputs cleared");
    assert_eq!(job.summary_md, None, "summary cleared");
    assert_eq!(job.heartbeat_at, None, "heartbeat cleared");
    assert!(!log_path.exists(), "prior attempt log deleted");
    assert!(
        !log_path.with_extension("log.gz").exists(),
        "compressed-at-rest log deleted"
    );
    assert!(
        !job.debug_logging,
        "plain rerun leaves debug_logging cleared"
    );

    // "Enable debug logging" stamps the flag on the new attempt; a plain
    // rerun afterwards clears it (per-attempt, not sticky).
    fx.db.cancel_action_run("run-m").await.unwrap();
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.rerunRun","input":{"owner":"actowner","name":"actdemo","run_id":"run-m","debug":true}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "debug rerun — {res}");
    let job = fx.db.find_action_job_by_id("job-m").await.unwrap().unwrap();
    assert_eq!(job.status, "queued");
    assert!(job.debug_logging, "debug rerun stamps debug_logging");

    fx.db.cancel_action_run("run-m").await.unwrap();
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.rerunRun","input":{"owner":"actowner","name":"actdemo","run_id":"run-m"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "plain rerun — {res}");
    let job = fx.db.find_action_job_by_id("job-m").await.unwrap().unwrap();
    assert!(
        !job.debug_logging,
        "plain rerun clears the attempt's debug flag"
    );

    // An unfinished run cannot be rerun.
    fx.db
        .insert_action_run(
            "run-open",
            "r-act",
            ".github/workflows/ci.yml",
            "CI",
            "push",
            "deadbeef",
            "main",
            "CI",
            Some(&fx.owner_id),
            None,
            None,
        )
        .await
        .unwrap();
    let open = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.rerunRun","input":{"owner":"actowner","name":"actdemo","run_id":"run-open"}}"#,
    )
    .await;
    assert_eq!(open["ok"], false, "unfinished rerun — {open}");
    assert_eq!(open["error"]["code"], "repo.actions.run_not_finished");

    // A run id from another repo is rejected.
    let other = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.rerunRun","input":{"owner":"actowner","name":"actdemo","run_id":"run-nonexistent"}}"#,
    )
    .await;
    assert_eq!(other["ok"], false, "wrong run — {other}");

    // deleteRun — in-flight runs must cancel first; a finished run deletes
    // its jobs (cascade), the run's log dir, and owned artifact payloads.
    fx.db
        .upsert_action_artifact(&oxidean_db::ActionArtifactRow {
            id: "art-1".into(),
            repository_id: "r-act".into(),
            run_id: "run-m".into(),
            job_id: Some("job-m".into()),
            name: "dist".into(),
            storage_path: "run-m/dist.zip".into(),
            size_bytes: 4,
            digest_sha256: "deadbeef".into(),
            expires_at: None,
            created_at: String::new(),
        })
        .await
        .unwrap();
    let payload = fx.artifacts_dir.join("run-m").join("dist.zip");
    std::fs::create_dir_all(payload.parent().unwrap()).unwrap();
    std::fs::write(&payload, b"zip!").unwrap();
    std::fs::write(&log_path, "attempt-3 output\n").unwrap();

    let in_flight = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.deleteRun","input":{"owner":"actowner","name":"actdemo","run_id":"run-m"}}"#,
    )
    .await;
    assert_eq!(in_flight["ok"], false, "in-flight delete — {in_flight}");
    assert_eq!(in_flight["error"]["code"], "repo.actions.run_not_finished");

    fx.db.cancel_action_run("run-m").await.unwrap();
    let del = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.deleteRun","input":{"owner":"actowner","name":"actdemo","run_id":"run-m"}}"#,
    )
    .await;
    assert_eq!(del["ok"], true, "delete — {del}");
    assert!(
        fx.db
            .find_action_run_by_id("run-m")
            .await
            .unwrap()
            .is_none(),
        "run row deleted"
    );
    assert!(
        fx.db
            .find_action_job_by_id("job-m")
            .await
            .unwrap()
            .is_none(),
        "job cascaded"
    );
    assert!(
        fx.db
            .find_action_artifact_by_id("art-1")
            .await
            .unwrap()
            .is_none(),
        "artifact row cascaded"
    );
    assert!(!log_path.exists(), "run log dir removed");
    assert!(!payload.exists(), "artifact payload removed");
}

#[tokio::test]
async fn workflow_enable_disable() {
    let fx = fixture("public").await;
    seed_workflows(&fx).await;

    let list = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.listWorkflows","input":{"owner":"actowner","name":"actdemo"}}"#,
    )
    .await;
    assert_eq!(list["ok"], true, "list — {list}");
    assert_eq!(list["data"]["workflows"][0]["disabled"], false);

    // Disable — dispatchWorkflow then refuses, listWorkflows marks it.
    let off = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.setWorkflowEnabled","input":{"owner":"actowner","name":"actdemo","path":".github/workflows/ci.yml","enabled":false}}"#,
    )
    .await;
    assert_eq!(off["ok"], true, "disable — {off}");

    let list = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.listWorkflows","input":{"owner":"actowner","name":"actdemo"}}"#,
    )
    .await;
    assert_eq!(list["data"]["workflows"][0]["disabled"], true, "marked");

    let disp = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.dispatchWorkflow","input":{"owner":"actowner","name":"actdemo","workflow_id":".github/workflows/ci.yml","git_ref":"main"}}"#,
    )
    .await;
    assert_eq!(disp["ok"], false, "disabled dispatch — {disp}");
    assert_eq!(disp["error"]["code"], "repo.actions.workflow_disabled");

    // A non-workflow path is rejected before touching the store.
    let bad = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.setWorkflowEnabled","input":{"owner":"actowner","name":"actdemo","path":"src/main.rs","enabled":false}}"#,
    )
    .await;
    assert_eq!(bad["ok"], false, "bad path — {bad}");

    // Re-enable restores dispatch.
    let on = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.setWorkflowEnabled","input":{"owner":"actowner","name":"actdemo","path":".github/workflows/ci.yml","enabled":true}}"#,
    )
    .await;
    assert_eq!(on["ok"], true, "enable — {on}");
    let disp = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.dispatchWorkflow","input":{"owner":"actowner","name":"actdemo","workflow_id":".github/workflows/ci.yml","git_ref":"main"}}"#,
    )
    .await;
    assert_eq!(disp["ok"], true, "re-enabled dispatch — {disp}");
}

#[tokio::test]
async fn download_run_logs_zip() {
    let fx = fixture("public").await;
    fx.db
        .insert_action_run(
            "run-z",
            "r-act",
            ".github/workflows/ci.yml",
            "CI",
            "push",
            "deadbeef",
            "main",
            "CI",
            Some(&fx.owner_id),
            None,
            None,
        )
        .await
        .unwrap();
    fx.db
        .insert_action_job(oxidean_db::JobInsert::basic(
            "job-z",
            "run-z",
            "build",
            "build",
            r#"["ubuntu-latest"]"#,
        ))
        .await
        .unwrap();
    let log_dir = fx.log_dir.join("run-z");
    std::fs::create_dir_all(&log_dir).unwrap();
    std::fs::write(log_dir.join("job-z.log"), "build output\n").unwrap();

    // A second job whose log carries `##[step]` markers splits into GitHub's
    // per-step archive layout.
    fx.db
        .insert_action_job(oxidean_db::JobInsert::basic(
            "job-z2",
            "run-z",
            "test",
            "test",
            r#"["ubuntu-latest"]"#,
        ))
        .await
        .unwrap();
    std::fs::write(
        log_dir.join("job-z2.log"),
        "Job leg — test / 2 step(s) [host]\n##[step]checkout\ncheckout out\n##[step-done]checkout\n##[step]run tests\ntest out\n##[step-done]run tests\njob concluded success\n",
    )
    .unwrap();

    // Anonymous on a public repo reads the archive.
    let res = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/repos/actowner/actdemo/actions/runs/run-z/logs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("application/zip")
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..2], b"PK", "zip magic");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    {
        let mut entry = zip.by_name("build.txt").expect("job entry in archive");
        let mut text = String::new();
        std::io::Read::read_to_string(&mut entry, &mut text).unwrap();
        assert_eq!(text, "build output\n");
    }

    // Per-step layout — `{job}/{seq}_{step}.txt` with the runner preamble in
    // `0_setup.txt` and trailing lines in `N_teardown.txt`.
    let read_entry = |zip: &mut zip::ZipArchive<std::io::Cursor<axum::body::Bytes>>, name: &str| {
        let mut e = zip.by_name(name).unwrap_or_else(|_| panic!("entry {name}"));
        let mut s = String::new();
        std::io::Read::read_to_string(&mut e, &mut s).unwrap();
        s
    };
    assert_eq!(
        read_entry(&mut zip, "test/0_setup.txt"),
        "Job leg — test / 2 step(s) [host]\n"
    );
    assert_eq!(
        read_entry(&mut zip, "test/1_checkout.txt"),
        "checkout out\n"
    );
    assert_eq!(read_entry(&mut zip, "test/2_run_tests.txt"), "test out\n");
    assert_eq!(
        read_entry(&mut zip, "test/3_teardown.txt"),
        "job concluded success\n"
    );

    // Unknown run and private-repo anonymous reads 404.
    let missing = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/repos/actowner/actdemo/actions/runs/nope/logs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let pfx = fixture("private").await;
    let denied = pfx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/repos/actowner/actdemo/actions/runs/run-z/logs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn workflow_badge_svg() {
    let fx = fixture("public").await;
    fx.db
        .insert_action_run(
            "run-b",
            "r-act",
            ".github/workflows/ci.yml",
            "CI",
            "push",
            "deadbeef",
            "main",
            "CI",
            Some(&fx.owner_id),
            None,
            None,
        )
        .await
        .unwrap();
    fx.db
        .insert_action_job(oxidean_db::JobInsert::basic(
            "job-b",
            "run-b",
            "build",
            "build",
            r#"["ubuntu-latest"]"#,
        ))
        .await
        .unwrap();
    fx.db
        .update_action_job_status("job-b", "success")
        .await
        .unwrap();
    fx.db.recompute_action_run_status("run-b").await.unwrap();

    async fn badge(app: &axum::Router, uri: &str) -> (StatusCode, String) {
        let res = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let body = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec())
            .unwrap();
        (status, body)
    }

    // Anonymous read on a public repo — passing badge.
    let (status, body) = badge(
        &fx.app,
        "/actowner/actdemo/actions/workflows/ci.yml/badge.svg",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "badge — {body}");
    assert!(body.contains("image") || body.starts_with("<svg"), "{body}");
    assert!(body.contains("passing"), "{body}");
    assert!(body.contains("CI"), "{body}");

    // Same shape under /api/repos for direct API consumers.
    let (status, body) = badge(
        &fx.app,
        "/api/repos/actowner/actdemo/actions/workflows/ci.yml/badge.svg",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "api badge — {body}");
    assert!(body.contains("passing"), "{body}");

    // `?branch=`/`?event=` filters — matching branch passes, mismatching
    // branch renders "no status".
    let (status, body) = badge(
        &fx.app,
        "/actowner/actdemo/actions/workflows/ci.yml/badge.svg?branch=main",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("passing"), "{body}");
    let (status, body) = badge(
        &fx.app,
        "/actowner/actdemo/actions/workflows/ci.yml/badge.svg?branch=release",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("no status"), "{body}");

    // Unknown workflow file → no status; unknown repo → 404.
    let (status, body) = badge(
        &fx.app,
        "/actowner/actdemo/actions/workflows/nope.yml/badge.svg",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("no status"), "{body}");
    let (status, _) = badge(
        &fx.app,
        "/actowner/nothere/actions/workflows/ci.yml/badge.svg",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Private repos don't leak badge status to anonymous callers.
    let fx2 = fixture("private").await;
    let (status, _) = badge(
        &fx2.app,
        "/actowner/actdemo/actions/workflows/ci.yml/badge.svg",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// GitHub caps an environment's required reviewers at 6; the upsert RPC
/// enforces the cap and dedupes tokens before counting.
#[tokio::test]
async fn environment_reviewer_cap() {
    let fx = fixture("public").await;
    for i in 0..7 {
        fx.db
            .create_user(
                &format!("u-cap-{i}"),
                &format!("cap{i}@example.com"),
                &format!("capuser{i}"),
                Some("h"),
                "C",
                "",
                None,
                oxidean_core::Role::User,
            )
            .await
            .unwrap();
    }
    let names = |n: usize| {
        (0..n)
            .map(|i| format!("\"capuser{i}\""))
            .collect::<Vec<_>>()
            .join(",")
    };
    // Seven distinct reviewers → rejected.
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        &format!(
            r#"{{"procedure":"repo.actions.environments.upsert","input":{{"owner":"actowner","name":"actdemo","environment":"prod","protection":{{"required_reviewers":[{}]}}}}}}"#,
            names(7)
        ),
    )
    .await;
    assert_eq!(res["ok"], false, "7 reviewers — {res}");
    // Six distinct reviewers → accepted; a duplicate token doesn't count twice.
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        &format!(
            r#"{{"procedure":"repo.actions.environments.upsert","input":{{"owner":"actowner","name":"actdemo","environment":"prod","protection":{{"required_reviewers":[{},"capuser0"]}}}}}}"#,
            names(6)
        ),
    )
    .await;
    assert_eq!(res["ok"], true, "6 reviewers + dup — {res}");
    assert_eq!(
        res["data"]["environment"]["protection"]["required_reviewers"]
            .as_array()
            .unwrap()
            .len(),
        6,
        "duplicate token deduped"
    );
}

#[tokio::test]
async fn job_annotations_lists_and_enforces_ownership() {
    let fx = fixture("private").await;
    fx.db
        .insert_action_run(
            "run-ann",
            "r-act",
            ".github/workflows/ci.yml",
            "CI",
            "push",
            "deadbeef",
            "main",
            "CI",
            Some(&fx.owner_id),
            None,
            None,
        )
        .await
        .unwrap();
    fx.db
        .insert_action_job(oxidean_db::JobInsert::basic(
            "job-ann",
            "run-ann",
            "build",
            "build",
            r#"["ubuntu-latest"]"#,
        ))
        .await
        .unwrap();
    // A second run/job in the same repo to prove job scoping.
    fx.db
        .insert_action_run(
            "run-ann-2",
            "r-act",
            ".github/workflows/ci.yml",
            "CI",
            "push",
            "cafef00d",
            "main",
            "CI",
            None,
            None,
            None,
        )
        .await
        .unwrap();
    fx.db
        .insert_action_job(oxidean_db::JobInsert::basic(
            "job-ann-2",
            "run-ann-2",
            "build",
            "build",
            r#"["ubuntu-latest"]"#,
        ))
        .await
        .unwrap();
    fx.db
        .insert_action_annotation(&oxidean_db::actions::ActionAnnotationRow {
            id: "ann-1".into(),
            run_id: "run-ann".into(),
            job_id: "job-ann".into(),
            annotation_level: "error".into(),
            message: "boom".into(),
            path: Some("src/main.rs".into()),
            start_line: Some(3),
            end_line: Some(4),
            start_column: Some(1),
            end_column: Some(9),
            title: Some("Lint".into()),
            created_at: String::new(),
        })
        .await
        .unwrap();
    fx.db
        .insert_action_annotation(&oxidean_db::actions::ActionAnnotationRow {
            id: "ann-2".into(),
            run_id: "run-ann".into(),
            job_id: "job-ann".into(),
            annotation_level: "notice".into(),
            message: "fyi".into(),
            path: None,
            start_line: None,
            end_line: None,
            start_column: None,
            end_column: None,
            title: None,
            created_at: String::new(),
        })
        .await
        .unwrap();
    fx.db
        .insert_action_annotation(&oxidean_db::actions::ActionAnnotationRow {
            id: "ann-3".into(),
            run_id: "run-ann-2".into(),
            job_id: "job-ann-2".into(),
            annotation_level: "warning".into(),
            message: "other run".into(),
            path: None,
            start_line: None,
            end_line: None,
            start_column: None,
            end_column: None,
            title: None,
            created_at: String::new(),
        })
        .await
        .unwrap();

    // Anonymous on a private repo is denied.
    let res = rpc_json(
        &fx.app,
        "",
        r#"{"procedure":"repo.actions.jobAnnotations","input":{"owner":"actowner","name":"actdemo","run_id":"run-ann","job_id":"job-ann"}}"#,
    )
    .await;
    assert_eq!(res["ok"], false, "anonymous private — {res}");

    // Job-scoped list returns only that job's annotations with full fields.
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.jobAnnotations","input":{"owner":"actowner","name":"actdemo","run_id":"run-ann","job_id":"job-ann"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "job-scoped list — {res}");
    let anns = res["data"]["annotations"].as_array().unwrap();
    assert_eq!(anns.len(), 2, "job annotations — {anns:?}");
    let err = anns
        .iter()
        .find(|a| a["annotation_level"] == "error")
        .unwrap();
    assert_eq!(err["message"], "boom");
    assert_eq!(err["path"], "src/main.rs");
    assert_eq!(err["start_line"], 3);
    assert_eq!(err["end_line"], 4);
    assert_eq!(err["start_column"], 1);
    assert_eq!(err["end_column"], 9);
    assert_eq!(err["title"], "Lint");

    // Run-level list spans jobs on the run but not other runs.
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.jobAnnotations","input":{"owner":"actowner","name":"actdemo","run_id":"run-ann"}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "run-scoped list — {res}");
    let anns = res["data"]["annotations"].as_array().unwrap();
    assert_eq!(anns.len(), 2, "run annotations stay run-scoped — {anns:?}");

    // Cross-run job/run mismatches are not-found, not leaks.
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.jobAnnotations","input":{"owner":"actowner","name":"actdemo","run_id":"run-ann","job_id":"job-ann-2"}}"#,
    )
    .await;
    assert_eq!(res["ok"], false, "job from another run — {res}");
    let res = rpc_json(
        &fx.app,
        &fx.cookie,
        r#"{"procedure":"repo.actions.jobAnnotations","input":{"owner":"actowner","name":"actdemo","run_id":"no-such-run"}}"#,
    )
    .await;
    assert_eq!(res["ok"], false, "unknown run — {res}");
}
