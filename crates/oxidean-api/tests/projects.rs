//! Projects domain: repo-scoped classic boards — `projects.*` CRUD,
//! columns/cards, card→issue conversion, the Projects unit flag, ACL,
//! `on: project|project_column|project_card` triggers, and webhook fan-out.

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

/// Board/column/card CRUD — numbering, position ordering, moves, scoping,
/// and issue-linked card enrichment.
#[tokio::test]
async fn projects_crud_and_cards() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("proj_crud.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "pj1@ex.com", "pj1").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &user_id).await;
    create_repo(&app, &cookie, "demo", "public").await;
    let repo_id = db
        .find_repository_by_owner_name(&user_id, "demo")
        .await
        .unwrap()
        .unwrap()
        .id;

    // Board create → `#1`, `open`.
    let created = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.create","input":{"owner":"pj1","name":"demo","title":"Roadmap","body":"Q3 plan"}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let board = &created["data"];
    assert_eq!(board["number"], 1);
    assert_eq!(board["title"], "Roadmap");
    assert_eq!(board["state"], "open");
    assert_eq!(board["html_url"], "/pj1/demo/projects/1");
    assert_eq!(board["creator_username"], "pj1");

    // Second board → `#2`.
    let second = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.create","input":{"owner":"pj1","name":"demo","title":"Bugs"}}"#,
    )
    .await;
    assert_eq!(second["data"]["number"], 2);

    // List filters open by default.
    let list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.list","input":{"owner":"pj1","name":"demo"}}"#,
    )
    .await;
    assert_eq!(list["data"]["projects"].as_array().unwrap().len(), 2);

    // Update title → `edited`; close → `closed` + `closed_at`.
    let upd = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.update","input":{"owner":"pj1","name":"demo","project":2,"title":"Bug triage","state":"closed"}}"#,
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["title"], "Bug triage");
    assert_eq!(upd["data"]["state"], "closed");
    assert!(upd["data"]["closed_at"].is_string());
    let open_list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.list","input":{"owner":"pj1","name":"demo"}}"#,
    )
    .await;
    assert_eq!(open_list["data"]["projects"].as_array().unwrap().len(), 1);
    let all_list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.list","input":{"owner":"pj1","name":"demo","state":"all"}}"#,
    )
    .await;
    assert_eq!(all_list["data"]["projects"].as_array().unwrap().len(), 2);

    // Columns — append order, rename, move.
    let col_a = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.columns.create","input":{"owner":"pj1","name":"demo","project":1,"column_name":"Todo"}}"#,
    )
    .await;
    assert_eq!(col_a["ok"], true, "{col_a}");
    let col_a_id = col_a["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(col_a["data"]["position"], 0);
    let col_b = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.columns.create","input":{"owner":"pj1","name":"demo","project":1,"column_name":"Doing"}}"#,
    )
    .await;
    let col_b_id = col_b["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(col_b["data"]["position"], 1);

    let cols = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.columns.list","input":{"owner":"pj1","name":"demo","project":1}}"#,
    )
    .await;
    let names: Vec<&str> = cols["data"]["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Todo", "Doing"]);

    let renamed = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.columns.update","input":{{"owner":"pj1","name":"demo","project":1,"column":"{col_a_id}","column_name":"Backlog"}}}}"#
        ),
    )
    .await;
    assert_eq!(renamed["data"]["name"], "Backlog");

    // Move col_b to position 0 → re-packs contiguously.
    let moved = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.columns.move","input":{{"owner":"pj1","name":"demo","project":1,"column":"{col_b_id}","position":0}}}}"#
        ),
    )
    .await;
    assert_eq!(moved["ok"], true, "{moved}");
    assert_eq!(moved["data"]["position"], 0);
    let cols2 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.columns.list","input":{"owner":"pj1","name":"demo","project":1}}"#,
    )
    .await;
    let names2: Vec<&str> = cols2["data"]["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names2, vec!["Doing", "Backlog"]);

    // Note card in "Doing".
    let note = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.create","input":{{"owner":"pj1","name":"demo","column":"{col_b_id}","note":"Triage flaky tests"}}}}"#
        ),
    )
    .await;
    assert_eq!(note["ok"], true, "{note}");
    assert_eq!(note["data"]["note"], "Triage flaky tests");
    assert_eq!(note["data"]["position"], 0);
    let note_id = note["data"]["id"].as_str().unwrap().to_string();

    // Linked card — a real issue.
    let issue_id = uuid::Uuid::new_v4().to_string();
    db.insert_issue(&issue_id, &repo_id, &user_id, "Fix the thing", "body")
        .await
        .expect("issue");
    let linked = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.create","input":{{"owner":"pj1","name":"demo","column":"{col_b_id}","content_type":"issue","content_id":"{issue_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(linked["ok"], true, "{linked}");
    assert_eq!(linked["data"]["content_type"], "issue");
    assert_eq!(linked["data"]["content_number"], 1);
    assert_eq!(linked["data"]["content_title"], "Fix the thing");
    assert_eq!(linked["data"]["content_state"], "open");
    assert_eq!(linked["data"]["content_url"], "/pj1/demo/issues/1");

    // Cross-repo content is refused.
    let bogus = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.create","input":{{"owner":"pj1","name":"demo","column":"{col_b_id}","content_type":"issue","content_id":"no-such-id"}}}}"#
        ),
    )
    .await;
    assert_eq!(bogus["ok"], false);
    assert_eq!(bogus["error"]["code"], "project.content_not_found");

    // Either note or content — not both/neither.
    for bad in [
        r#"{"procedure":"projects.cards.create","input":{"owner":"pj1","name":"demo","column":"COL","note":"n","content_type":"issue","content_id":"x"}}"#
            .replace("COL", &col_b_id),
        r#"{"procedure":"projects.cards.create","input":{"owner":"pj1","name":"demo","column":"COL"}}"#
            .replace("COL", &col_b_id),
    ] {
        let v = rpc_json(&app, &cookie, &bad).await;
        assert_eq!(v["ok"], false, "{v}");
        assert_eq!(v["error"]["code"], "rpc.bad_input");
    }

    // Card move to "Backlog".
    let moved_card = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.move","input":{{"owner":"pj1","name":"demo","project":1,"card":"{note_id}","column":"{col_a_id}","position":0}}}}"#
        ),
    )
    .await;
    assert_eq!(moved_card["ok"], true, "{moved_card}");
    assert_eq!(moved_card["data"]["column_id"], col_a_id);
    let backlog_cards = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.list","input":{{"owner":"pj1","name":"demo","column":"{col_a_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(backlog_cards["data"]["cards"].as_array().unwrap().len(), 1);

    // Archived cards hide by default.
    let archived = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.update","input":{{"owner":"pj1","name":"demo","project":1,"card":"{note_id}","archived":true}}}}"#
        ),
    )
    .await;
    assert_eq!(archived["data"]["archived"], true);
    let visible = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.list","input":{{"owner":"pj1","name":"demo","column":"{col_a_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(visible["data"]["cards"].as_array().unwrap().len(), 0);
    let with_archived = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.list","input":{{"owner":"pj1","name":"demo","column":"{col_a_id}","archived":true}}}}"#
        ),
    )
    .await;
    assert_eq!(with_archived["data"]["cards"].as_array().unwrap().len(), 1);

    // Cross-board scoping — col_b lives on board 1, not board 2.
    let scoped = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.columns.update","input":{{"owner":"pj1","name":"demo","project":2,"column":"{col_b_id}","column_name":"x"}}}}"#
        ),
    )
    .await;
    assert_eq!(scoped["ok"], false);
    assert_eq!(scoped["error"]["code"], "project.column_not_found");

    // Column delete cascades cards.
    let del_col = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.columns.delete","input":{{"owner":"pj1","name":"demo","project":1,"column":"{col_a_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(del_col["ok"], true, "{del_col}");
    let gone_cards = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.list","input":{{"owner":"pj1","name":"demo","column":"{col_a_id}","archived":true}}}}"#
        ),
    )
    .await;
    assert_eq!(gone_cards["ok"], false, "column is gone");
    assert_eq!(gone_cards["error"]["code"], "project.column_not_found");

    // Board delete.
    let del = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.delete","input":{"owner":"pj1","name":"demo","project":1}}"#,
    )
    .await;
    assert_eq!(del["ok"], true, "{del}");
    let missing = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.get","input":{"owner":"pj1","name":"demo","project":1}}"#,
    )
    .await;
    assert_eq!(missing["ok"], false);
    assert_eq!(missing["error"]["code"], "project.not_found");
}

/// `repo.projects.setEnabled` gates every `projects.*` RPC; `repo.get`
/// surfaces `projects_enabled`; read/write ACL matches Issues/Pulls.
#[tokio::test]
async fn projects_unit_flag_and_acl() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("proj_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "pj2@ex.com", "pj2").await;
    let owner_id = login_v["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &owner_id).await;
    create_repo(&app, &cookie, "demo", "public").await;
    let repo_id = db
        .find_repository_by_owner_name(&owner_id, "demo")
        .await
        .unwrap()
        .unwrap()
        .id;

    // Default on — repo.get exposes the flag.
    let got = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.get","input":{"owner":"pj2","name":"demo"}}"#,
    )
    .await;
    assert_eq!(got["data"]["projects_enabled"], true, "{got}");

    // Disable → all reads+writes return repo.projects.disabled.
    let off = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.projects.setEnabled","input":{"owner":"pj2","name":"demo","enabled":false}}"#,
    )
    .await;
    assert_eq!(off["ok"], true, "{off}");
    for proc_body in [
        r#"{"procedure":"projects.list","input":{"owner":"pj2","name":"demo"}}"#,
        r#"{"procedure":"projects.create","input":{"owner":"pj2","name":"demo","title":"x"}}"#,
    ] {
        let v = rpc_json(&app, &cookie, proc_body).await;
        assert_eq!(v["ok"], false, "{v}");
        assert_eq!(v["error"]["code"], "repo.projects.disabled", "{v}");
    }
    let got_off = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.get","input":{"owner":"pj2","name":"demo"}}"#,
    )
    .await;
    assert_eq!(got_off["data"]["projects_enabled"], false);

    // Re-enable — data flows again.
    let on = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.projects.setEnabled","input":{"owner":"pj2","name":"demo","enabled":true}}"#,
    )
    .await;
    assert_eq!(on["ok"], true, "{on}");
    let list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.list","input":{"owner":"pj2","name":"demo"}}"#,
    )
    .await;
    assert_eq!(list["ok"], true, "{list}");

    // ACL — a read collaborator can list but not create.
    let (rc, rlogin) = signup_and_login(&app, "pj3@ex.com", "pj3").await;
    let reader_id = rlogin["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &reader_id).await;
    db.insert_repo_collaborator(&repo_id, &reader_id, "read")
        .await
        .expect("collaborator");

    let rlist = rpc_json(
        &app,
        &rc,
        r#"{"procedure":"projects.list","input":{"owner":"pj2","name":"demo"}}"#,
    )
    .await;
    assert_eq!(rlist["ok"], true, "{rlist}");
    let rcreate = rpc_json(
        &app,
        &rc,
        r#"{"procedure":"projects.create","input":{"owner":"pj2","name":"demo","title":"sneak"}}"#,
    )
    .await;
    assert_eq!(rcreate["ok"], false);
    assert_eq!(rcreate["error"]["code"], "repo.not_found");

    // Unit toggle is admin-only.
    let roff = rpc_json(
        &app,
        &rc,
        r#"{"procedure":"repo.projects.setEnabled","input":{"owner":"pj2","name":"demo","enabled":false}}"#,
    )
    .await;
    assert_eq!(roff["ok"], false);
    assert_eq!(roff["error"]["code"], "repo.not_found");
}

/// `project`/`project_column`/`project_card` lifecycle triggers, webhook
/// payload shapes, and note→issue conversion (`issues.opened` +
/// `project_card.converted`).
#[tokio::test]
async fn projects_events_and_convert() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("proj_ev.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "pj4@ex.com", "pj4").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &user_id).await;
    create_repo(&app, &cookie, "demo", "public").await;

    let workflow = br#"
name: ProjTrig
on:
  project: {}
  project_column: {}
  project_card: {}
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo projects
"#;
    let (_git, _bare, _tip) = seed_repo_git(
        &repos_dir,
        "pj4",
        "demo",
        &[
            ("README.md", b"hi".as_slice()),
            (".github/workflows/projects.yml", workflow.as_slice()),
        ],
    )
    .await;

    let hook = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"webhook.create","input":{"owner":"pj4","name":"demo","url":"http://127.0.0.1:9/hook","secret":"s","events":["project","project_column","project_card","issues"]}}"#,
    )
    .await;
    assert_eq!(hook["ok"], true, "{hook}");
    let hook_id = hook["data"]["id"].as_str().unwrap().to_string();

    let repo_id = db
        .find_repository_by_owner_name(&user_id, "demo")
        .await
        .unwrap()
        .unwrap()
        .id;
    assert_eq!(run_count(&db, &repo_id).await, 0);

    // projects.create → `project.created` webhook + `on: project` run.
    let board = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.create","input":{"owner":"pj4","name":"demo","title":"Roadmap"}}"#,
    )
    .await;
    assert_eq!(board["ok"], true, "{board}");
    assert_eq!(run_count(&db, &repo_id).await, 1, "on: project run");

    // columns.create → `project_column.created` + run.
    let col = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.columns.create","input":{"owner":"pj4","name":"demo","project":1,"column_name":"Todo"}}"#,
    )
    .await;
    assert_eq!(col["ok"], true, "{col}");
    let col_id = col["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(run_count(&db, &repo_id).await, 2, "on: project_column run");

    // cards.create (note) → `project_card.created` + run.
    let card = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.create","input":{{"owner":"pj4","name":"demo","column":"{col_id}","note":"Convert me\n\nwith a body"}}}}"#
        ),
    )
    .await;
    assert_eq!(card["ok"], true, "{card}");
    let card_id = card["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(run_count(&db, &repo_id).await, 3, "on: project_card run");

    // cards.convert → a real issue (`issues.opened`) + `project_card.converted`.
    let conv = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.convert","input":{{"owner":"pj4","name":"demo","project":1,"card":"{card_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(conv["ok"], true, "{conv}");
    assert_eq!(conv["data"]["content_type"], "issue");
    assert_eq!(conv["data"]["content_title"], "Convert me");
    assert!(conv["data"]["note"].is_null());
    // `issues` isn't in the workflow's `on:` — only project_card fires.
    assert_eq!(run_count(&db, &repo_id).await, 4, "converted run");

    let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.unwrap();
    let events: Vec<&str> = deliveries.iter().map(|d| d.event.as_str()).collect();
    assert!(events.contains(&"project"), "deliveries: {events:?}");
    assert!(events.contains(&"project_column"), "deliveries: {events:?}");
    assert!(events.contains(&"project_card"), "deliveries: {events:?}");
    assert!(
        events.contains(&"issues"),
        "issues.opened on convert: {events:?}"
    );

    // Payload shapes — GitHub vocabulary.
    let proj = deliveries
        .iter()
        .find(|d| d.event == "project")
        .expect("project delivery");
    let p: serde_json::Value = serde_json::from_str(&proj.payload_json).unwrap();
    assert_eq!(p["action"], "created");
    assert_eq!(p["project"]["name"], "Roadmap");
    assert_eq!(p["project"]["number"], 1);
    assert_eq!(p["repository"]["name"], "demo");
    assert_eq!(p["sender"]["login"], "pj4");

    let col_del = deliveries
        .iter()
        .find(|d| d.event == "project_column")
        .expect("project_column delivery");
    let c: serde_json::Value = serde_json::from_str(&col_del.payload_json).unwrap();
    assert_eq!(c["action"], "created");
    assert_eq!(c["project_column"]["name"], "Todo");

    let converted = deliveries
        .iter()
        .find(|d| d.event == "project_card" && d.payload_json.contains("\"converted\""))
        .expect("converted delivery");
    let cv: serde_json::Value = serde_json::from_str(&converted.payload_json).unwrap();
    assert_eq!(cv["action"], "converted");
    assert_eq!(cv["project_card"]["content_url"], "/pj4/demo/issues/1");
    assert_eq!(cv["changes"]["note"]["from"], "Convert me\n\nwith a body");

    let issues = deliveries
        .iter()
        .find(|d| d.event == "issues")
        .expect("issues delivery");
    let i: serde_json::Value = serde_json::from_str(&issues.payload_json).unwrap();
    assert_eq!(i["action"], "opened");
    assert_eq!(i["issue"]["number"], 1);
    assert_eq!(i["issue"]["title"], "Convert me");
    assert_eq!(i["issue"]["body"], "with a body");

    // types: filter — an `on: project_card: [deleted]` workflow does not
    // fire for `converted`.
    let typed = br#"
name: Typed
on:
  project_card:
    types: [deleted]
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo deleted-only
"#;
    let git = CliGitBackend::new();
    let bare = repos_dir.join("pj4").join("demo.git");
    git.commit_files(
        &bare,
        "main",
        "main",
        "add typed workflow",
        &[FileChange::Upsert {
            path: ".github/workflows/typed.yml".to_string(),
            content: typed.to_vec(),
        }],
        "Test",
        "t@ex.com",
        None,
        "admin",
    )
    .await
    .expect("typed commit");

    let before = run_count(&db, &repo_id).await;
    let card2 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.create","input":{{"owner":"pj4","name":"demo","column":"{col_id}","note":"second"}}}}"#
        ),
    )
    .await;
    assert_eq!(card2["ok"], true);
    // project_card.created → the unfiltered workflow runs, typed doesn't.
    assert_eq!(run_count(&db, &repo_id).await, before + 1);
}

/// REST surface — repo-scoped project routes over `projects.*`:
/// board/column/card CRUD, moves, note→issue convert.
#[tokio::test]
async fn rest_projects_surface() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("proj_rest.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "pr@ex.com", "pr1").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "board", "public").await;

    // Board create — GitHub `name` → `title`.
    let (s, v) = rest_json(
        &app,
        "POST",
        "/api/v1/repos/pr1/board/projects",
        &cookie,
        Some(r#"{"name":"Roadmap","body":"Q3"}"#.into()),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["title"], "Roadmap", "{v}");
    assert_eq!(v["number"], 1, "{v}");

    let (s, v) = rest_json(
        &app,
        "GET",
        "/api/v1/repos/pr1/board/projects",
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v.as_array().expect("bare array").len(), 1, "{v}");

    let (s, v) = rest_json(
        &app,
        "GET",
        "/api/v1/repos/pr1/board/projects/1",
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["title"], "Roadmap", "{v}");

    // Columns.
    let (s, col) = rest_json(
        &app,
        "POST",
        "/api/v1/repos/pr1/board/projects/1/columns",
        &cookie,
        Some(r#"{"name":"Todo"}"#.into()),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{col}");
    let col_id = col["id"].as_str().unwrap().to_string();

    let (s, col2) = rest_json(
        &app,
        "POST",
        "/api/v1/repos/pr1/board/projects/1/columns",
        &cookie,
        Some(r#"{"name":"Done"}"#.into()),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{col2}");
    let col2_id = col2["id"].as_str().unwrap().to_string();

    let (s, v) = rest_json(
        &app,
        "PATCH",
        &format!("/api/v1/repos/pr1/board/projects/1/columns/{col_id}"),
        &cookie,
        Some(r#"{"name":"Backlog"}"#.into()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["name"], "Backlog", "{v}");

    // Column move — swap Done to position 0.
    let (s, v) = rest_json(
        &app,
        "POST",
        &format!("/api/v1/repos/pr1/board/projects/1/columns/{col2_id}/moves"),
        &cookie,
        Some(r#"{"position":0}"#.into()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["position"], 0, "{v}");

    // Cards — note card in Done.
    let (s, card) = rest_json(
        &app,
        "POST",
        &format!("/api/v1/repos/pr1/board/projects/1/columns/{col2_id}/cards"),
        &cookie,
        Some(r#"{"note":"Ship it\nwith tests"}"#.into()),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{card}");
    let card_id = card["id"].as_str().unwrap().to_string();

    let (s, v) = rest_json(
        &app,
        "GET",
        &format!("/api/v1/repos/pr1/board/projects/1/columns/{col2_id}/cards"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v.as_array().unwrap().len(), 1, "{v}");

    // Get-by-id via project-scoped path.
    let (s, v) = rest_json(
        &app,
        "GET",
        &format!("/api/v1/repos/pr1/board/projects/1/cards/{card_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        v["note"],
        "Ship it
with tests",
        "{v}"
    );

    // Move the card to Backlog.
    let (s, v) = rest_json(
        &app,
        "POST",
        &format!("/api/v1/repos/pr1/board/projects/1/cards/{card_id}/moves"),
        &cookie,
        Some(format!(r#"{{"column_id":"{col_id}","position":0}}"#)),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["column_id"], col_id, "{v}");

    // Convert the note card into a real issue.
    let (s, v) = rest_json(
        &app,
        "POST",
        &format!("/api/v1/repos/pr1/board/projects/1/cards/{card_id}/convert"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["content_type"], "issue", "{v}");
    let issue_number = v["content_number"].as_i64().expect("content_number");
    let issue = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"issue.get","input":{{"owner":"pr1","name":"board","number":{issue_number}}}}}"#
        ),
    )
    .await;
    assert_eq!(issue["ok"], true, "{issue}");
    assert_eq!(issue["data"]["title"], "Ship it", "{issue}");

    // Delete board → columns and cards cascade.
    let (s, _) = rest_json(
        &app,
        "DELETE",
        "/api/v1/repos/pr1/board/projects/1",
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = rest_json(
        &app,
        "GET",
        "/api/v1/repos/pr1/board/projects/1",
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

async fn rest_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    body: Option<String>,
) -> (StatusCode, serde_json::Value) {
    let req = match body {
        Some(j) => Request::builder()
            .method(method)
            .uri(uri)
            .header("cookie", cookie)
            .header("content-type", "application/json")
            .body(Body::from(j))
            .unwrap(),
        None => Request::builder()
            .method(method)
            .uri(uri)
            .header("cookie", cookie)
            .body(Body::empty())
            .unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}
