//! Wiki domain: git-backed pages (`{name}.wiki.git`), the unit flag,
//! `gollum` lifecycle/webhook emission for both RPC writes and direct
//! wiki pushes, and ACL.

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

/// Page CRUD — the `.wiki.git` bare initializes lazily, pages are real
/// `*.md` files, revisions come from `git log`, deletes remove the file.
#[tokio::test]
async fn wiki_pages_crud_and_history() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wiki_crud.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "w1@ex.com", "w1").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "demo", "public").await;

    // Unborn wiki → empty page list, get 404s.
    let list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.list","input":{"owner":"w1","name":"demo"}}"#,
    )
    .await;
    assert_eq!(list["ok"], true, "{list}");
    assert_eq!(list["data"]["pages"].as_array().unwrap().len(), 0);
    let missing = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.get","input":{"owner":"w1","name":"demo","page":"Home"}}"#,
    )
    .await;
    assert_eq!(missing["ok"], false, "{missing}");
    assert_eq!(missing["error"]["code"], "wiki.not_found");

    // Bad names are refused before they touch git.
    for bad in ["..", "a/b", ".hidden", "", "   "] {
        let bad_req = rpc_json(
            &app,
            &cookie,
            &format!(
                r#"{{"procedure":"wiki.pages.create","input":{{"owner":"w1","name":"demo","title":"{bad}","content":"x"}}}}"#
            ),
        )
        .await;
        assert_eq!(
            bad_req["error"]["code"], "wiki.bad_page",
            "title {bad:?} — {bad_req}"
        );
    }

    // create — title with spaces becomes `My-Page`, `.wiki.git` materializes.
    let created = rpc_json(
        &app,
        &cookie,
        r##"{"procedure":"wiki.pages.create","input":{"owner":"w1","name":"demo","title":"My Page","content":"# Hello"}}"##,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(created["data"]["page"]["page_name"], "My-Page");
    let wiki_bare = repos_dir.join("w1").join("demo.wiki.git");
    assert!(wiki_bare.is_dir(), "wiki bare materialized");

    let git = CliGitBackend::new();
    let head = git
        .rev_parse(&wiki_bare, "refs/heads/master")
        .await
        .expect("wiki master resolves");
    assert_eq!(created["data"]["page"]["sha"], head);
    // The file is real git content — readable without the RPC layer.
    let blob = git
        .cat_blob(&wiki_bare, "master", "My-Page.md")
        .await
        .expect("cat blob");
    assert_eq!(String::from_utf8_lossy(&blob), "# Hello");

    // Duplicate create refuses — edits go through update.
    let dup = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.create","input":{"owner":"w1","name":"demo","title":"My Page","content":"x"}}"#,
    )
    .await;
    assert_eq!(dup["error"]["code"], "wiki.page_exists", "{dup}");

    // get — raw content + tip sha.
    let page = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.get","input":{"owner":"w1","name":"demo","page":"My-Page"}}"#,
    )
    .await;
    assert_eq!(page["data"]["page"]["content"], "# Hello");

    // list — sorted, reserved flag off for ordinary pages.
    let list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.list","input":{"owner":"w1","name":"demo"}}"#,
    )
    .await;
    let pages = list["data"]["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0]["page_name"], "My-Page");
    assert_eq!(pages[0]["reserved"], false);

    // _Sidebar is a real page flagged reserved (GitHub layout file).
    let side = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.create","input":{"owner":"w1","name":"demo","title":"_Sidebar","content":"nav"}}"#,
    )
    .await;
    assert_eq!(side["ok"], true, "{side}");
    assert_eq!(side["data"]["page"]["reserved"], true);

    // update — content round-trips, history grows.
    let updated = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.update","input":{"owner":"w1","name":"demo","page":"My-Page","content":"v2","message":"tweak"}}"#,
    )
    .await;
    assert_eq!(updated["ok"], true, "{updated}");
    let revs = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.revisions.list","input":{"owner":"w1","name":"demo","page":"My-Page"}}"#,
    )
    .await;
    let revisions = revs["data"]["revisions"].as_array().unwrap();
    assert_eq!(revisions.len(), 2, "{revs}");
    assert_eq!(revisions[0]["message"], "tweak");
    assert_eq!(revisions[0]["author_username"], "w1");

    // delete — file gone from the tree; no gollum (GitHub: create/edit only).
    let deleted = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.delete","input":{"owner":"w1","name":"demo","page":"My-Page"}}"#,
    )
    .await;
    assert_eq!(deleted["ok"], true, "{deleted}");
    assert!(git
        .ls_tree_entry(&wiki_bare, "master", "My-Page.md")
        .await
        .unwrap()
        .is_none());
    // History is git — the delete commit is a revision too.
    let revs = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.revisions.list","input":{"owner":"w1","name":"demo","page":"My-Page"}}"#,
    )
    .await;
    assert_eq!(
        revs["data"]["revisions"].as_array().unwrap().len(),
        3,
        "{revs}"
    );

    // Updating a missing page 404s.
    let gone = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.update","input":{"owner":"w1","name":"demo","page":"My-Page","content":"x"}}"#,
    )
    .await;
    assert_eq!(gone["error"]["code"], "wiki.not_found", "{gone}");
}

/// The `repo.wiki.*` unit flag gates every `wiki.*` RPC; writes need repo
/// Write; `.wiki` is refused as a repo name (transport shadowing).
#[tokio::test]
async fn wiki_unit_flag_acl_and_name_guard() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wiki_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "w2@ex.com", "w2").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "demo", "private").await;

    // `.wiki`-suffixed repo names are rejected — they'd shadow the wiki
    // bare at `{owner}/x.wiki.git`.
    let clash = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"x.wiki","visibility":"public","description":""}}"#,
    )
    .await;
    assert_eq!(clash["ok"], false, "{clash}");

    // Unit flag defaults on.
    let enabled = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.wiki.getEnabled","input":{"owner":"w2","name":"demo"}}"#,
    )
    .await;
    assert_eq!(enabled["data"]["enabled"], true, "{enabled}");

    // A second user with no repo access is denied — private repo → not_found
    // on both read and write (no existence leak).
    let (cookie2, login_v2) = signup_and_login(&app, "w3@ex.com", "w3").await;
    verify_user(&db, login_v2["data"]["id"].as_str().unwrap()).await;
    let denied = rpc_json(
        &app,
        &cookie2,
        r#"{"procedure":"wiki.pages.list","input":{"owner":"w2","name":"demo"}}"#,
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found");
    let denied_create = rpc_json(
        &app,
        &cookie2,
        r#"{"procedure":"wiki.pages.create","input":{"owner":"w2","name":"demo","title":"P","content":"x"}}"#,
    )
    .await;
    assert_eq!(denied_create["error"]["code"], "repo.not_found");

    // Read collaborator can list but not write — wiki edits need repo Write.
    let repo_id = db
        .find_repository_by_owner_name(login_v["data"]["id"].as_str().unwrap(), "demo")
        .await
        .unwrap()
        .unwrap()
        .id;
    db.insert_repo_collaborator(&repo_id, login_v2["data"]["id"].as_str().unwrap(), "read")
        .await
        .unwrap();
    let ro_list = rpc_json(
        &app,
        &cookie2,
        r#"{"procedure":"wiki.pages.list","input":{"owner":"w2","name":"demo"}}"#,
    )
    .await;
    assert_eq!(ro_list["ok"], true, "{ro_list}");
    let ro_create = rpc_json(
        &app,
        &cookie2,
        r#"{"procedure":"wiki.pages.create","input":{"owner":"w2","name":"demo","title":"P","content":"x"}}"#,
    )
    .await;
    assert_eq!(ro_create["error"]["code"], "repo.not_found", "{ro_create}");

    // Disable → every wiki.* RPC rejects with the unit error.
    let off = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.wiki.setEnabled","input":{"owner":"w2","name":"demo","enabled":false}}"#,
    )
    .await;
    assert_eq!(off["data"]["enabled"], false, "{off}");
    for proc_name in [
        "wiki.pages.list",
        "wiki.pages.get",
        "wiki.pages.create",
        "wiki.pages.update",
        "wiki.pages.delete",
        "wiki.revisions.list",
    ] {
        let input = if proc_name == "wiki.pages.list" {
            r#"{"owner":"w2","name":"demo"}"#.to_string()
        } else if proc_name == "wiki.pages.create" {
            r#"{"owner":"w2","name":"demo","title":"P","content":"x"}"#.to_string()
        } else if proc_name == "wiki.pages.update" {
            r#"{"owner":"w2","name":"demo","page":"P","content":"x"}"#.to_string()
        } else {
            r#"{"owner":"w2","name":"demo","page":"P"}"#.to_string()
        };
        let v = rpc_json(
            &app,
            &cookie,
            &format!(r#"{{"procedure":"{proc_name}","input":{input}}}"#),
        )
        .await;
        assert_eq!(
            v["error"]["code"], "repo.wiki.disabled",
            "{proc_name} — {v}"
        );
    }

    // Re-enable → works again.
    let on = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.wiki.setEnabled","input":{"owner":"w2","name":"demo","enabled":true}}"#,
    )
    .await;
    assert_eq!(on["data"]["enabled"], true);
    let again = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.list","input":{"owner":"w2","name":"demo"}}"#,
    )
    .await;
    assert_eq!(again["ok"], true, "{again}");
}

/// `wiki.pages.create`/`update` fire `gollum` — webhook delivery with the
/// GitHub `pages[]` shape and an `on: gollum` Actions run. `push` never
/// fires for wiki writes.
#[tokio::test]
async fn wiki_gollum_events_webhook_and_trigger() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wiki_ev.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "w4@ex.com", "w4").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "demo", "public").await;
    seed_repo_git(
        &repos_dir,
        "w4",
        "demo",
        &[
            ("README.md", b"hi".as_slice()),
            (
                ".github/workflows/gollum.yml",
                br#"
name: G
on:
  gollum:
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo wiki
"#
                .as_slice(),
            ),
            (
                ".github/workflows/onpush.yml",
                br#"
name: P
on:
  push:
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo push
"#
                .as_slice(),
            ),
        ],
    )
    .await;

    let hook = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"webhook.create","input":{"owner":"w4","name":"demo","url":"http://127.0.0.1:9/hook","secret":"s","events":["gollum","push"]}}"#,
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

    let created = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.create","input":{"owner":"w4","name":"demo","title":"Home","content":"home"}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");

    // `on: gollum` enqueued; `on: push` did not.
    let runs = db
        .list_action_runs_for_repo(&repo_id, &Default::default(), 100, 0)
        .await
        .unwrap();
    assert_eq!(
        runs.len(),
        1,
        "runs: {:?}",
        runs.iter().map(|r| &r.event).collect::<Vec<_>>()
    );
    assert_eq!(runs[0].event, "gollum");

    // Webhook delivery — GitHub `pages[]` shape, no top-level `action`.
    let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.unwrap();
    let gollum = deliveries
        .iter()
        .find(|d| d.event == "gollum")
        .expect("gollum delivery");
    let payload: serde_json::Value =
        serde_json::from_str(&gollum.payload_json).expect("payload json");
    assert!(
        payload.get("action").is_none(),
        "gollum has no top-level action"
    );
    let pages = payload["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0]["page_name"], "Home");
    assert_eq!(pages[0]["title"], "Home");
    assert_eq!(pages[0]["action"], "created");
    assert_eq!(pages[0]["html_url"], "/w4/demo/wiki/Home");
    assert_eq!(payload["repository"]["name"], "demo");
    assert_eq!(payload["sender"]["login"], "w4");
    assert!(
        !deliveries.iter().any(|d| d.event == "push"),
        "wiki writes never emit push"
    );

    // Update → edited action.
    let updated = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"wiki.pages.update","input":{"owner":"w4","name":"demo","page":"Home","content":"v2"}}"#,
    )
    .await;
    assert_eq!(updated["ok"], true, "{updated}");
    let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.unwrap();
    let edited = deliveries
        .iter()
        .filter(|d| d.event == "gollum")
        .map(|d| serde_json::from_str::<serde_json::Value>(&d.payload_json).unwrap())
        .find(|p| p["pages"][0]["action"] == "edited")
        .expect("edited gollum delivery");
    assert_eq!(edited["pages"][0]["page_name"], "Home");
}

/// The transport fan-out (`notify_wiki_push`) — a direct wiki push emits
/// one `gollum` event batching every changed page, never `push`, and still
/// drives `on: gollum` Actions runs.
#[tokio::test]
async fn wiki_push_fanout_emits_gollum() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wiki_push.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;

    let owner = db
        .create_user(
            "u-w5",
            "w5@example.com",
            "w5",
            Some("h"),
            "W5",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository("r-w5", &owner.id, "user", "demo", "public", "", "main")
        .await
        .expect("repo");

    let git = CliGitBackend::new();
    let main_bare = repos_dir.join("w5").join("demo.git");
    let wiki_bare = repos_dir.join("w5").join("demo.wiki.git");
    git.init_bare(&main_bare, "main").await.expect("init main");
    git.seed_commit(
        &main_bare,
        "main",
        "seed",
        &[
            ("README.md".to_string(), b"hi".to_vec()),
            (
                ".github/workflows/gollum.yml".to_string(),
                br#"
name: G
on:
  gollum:
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo wiki
"#
                .to_vec(),
            ),
            (
                ".github/workflows/onpush.yml".to_string(),
                br#"
name: P
on:
  push:
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo push
"#
                .to_vec(),
            ),
        ],
    )
    .await
    .expect("seed main");
    git.init_bare(&wiki_bare, "master")
        .await
        .expect("init wiki");

    // Two wiki commits — c1 creates Home, c2 edits Home and creates About.
    let c1 = git
        .commit_files(
            &wiki_bare,
            "master",
            "master",
            "add home",
            &[FileChange::Upsert {
                path: "Home.md".into(),
                content: b"home".to_vec(),
            }],
            "W5",
            "w5@example.com",
            None,
            "write",
        )
        .await
        .expect("c1");
    let c2 = git
        .commit_files(
            &wiki_bare,
            "master",
            "master",
            "more pages",
            &[
                FileChange::Upsert {
                    path: "Home.md".into(),
                    content: b"home v2".to_vec(),
                },
                FileChange::Upsert {
                    path: "About.md".into(),
                    content: b"about".to_vec(),
                },
            ],
            "W5",
            "w5@example.com",
            None,
            "write",
        )
        .await
        .expect("c2");

    // Webhook rows fan out — subscribe a hook to gollum + push.
    let hook_id = "wh-w5";
    db.insert_webhook(
        hook_id,
        &repo.id,
        "http://127.0.0.1:9/h",
        "s",
        true,
        &serde_json::to_string(&["gollum", "push"]).unwrap(),
        "hook",
        &owner.id,
    )
    .await
    .expect("hook");

    // A push spanning both commits (zero-old → c2 tip covers About +
    // Home; c1's creation shows via the same range).
    let updates = vec![(
        "0".repeat(40),
        c2.sha.clone(),
        "refs/heads/master".to_string(),
    )];
    oxidean_api::webhook::dispatch::notify_wiki_push(
        &db,
        &git,
        &main_bare,
        &wiki_bare,
        &repo.id,
        "w5",
        "demo",
        "main",
        "w5",
        &owner.id,
        &updates,
        "development",
        false,
        true,
    )
    .await;

    let deliveries = db.list_webhook_deliveries(hook_id, 50).await.unwrap();
    let gollum = deliveries
        .iter()
        .find(|d| d.event == "gollum")
        .expect("gollum delivery");
    let payload: serde_json::Value =
        serde_json::from_str(&gollum.payload_json).expect("payload json");
    let pages = payload["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2, "{payload}");
    let home = pages
        .iter()
        .find(|p| p["page_name"] == "Home")
        .expect("home page");
    // Created + edited inside one push → `created` wins.
    assert_eq!(home["action"], "created");
    assert!(pages
        .iter()
        .any(|p| p["page_name"] == "About" && p["action"] == "created"));
    assert!(!deliveries.iter().any(|d| d.event == "push"));

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
    assert_eq!(runs[0].event, "gollum");

    // A second push carrying only a Home edit → edited action.
    let c3 = git
        .commit_files(
            &wiki_bare,
            "master",
            "master",
            "edit home",
            &[FileChange::Upsert {
                path: "Home.md".into(),
                content: b"home v3".to_vec(),
            }],
            "W5",
            "w5@example.com",
            None,
            "write",
        )
        .await
        .expect("c3");
    let updates2 = vec![(
        c2.sha.clone(),
        c3.sha.clone(),
        "refs/heads/master".to_string(),
    )];
    oxidean_api::webhook::dispatch::notify_wiki_push(
        &db,
        &git,
        &main_bare,
        &wiki_bare,
        &repo.id,
        "w5",
        "demo",
        "main",
        "w5",
        &owner.id,
        &updates2,
        "development",
        false,
        true,
    )
    .await;
    let deliveries = db.list_webhook_deliveries(hook_id, 50).await.unwrap();
    let edited = deliveries
        .iter()
        .filter(|d| d.event == "gollum")
        .map(|d| serde_json::from_str::<serde_json::Value>(&d.payload_json).unwrap())
        .find(|p| p["pages"][0]["action"] == "edited")
        .expect("edited gollum delivery");
    assert_eq!(edited["pages"][0]["page_name"], "Home");
    let _ = c1;
}
