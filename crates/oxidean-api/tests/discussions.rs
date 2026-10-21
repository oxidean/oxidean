//! Discussions domain: categories, threads, comments, answers, pin/lock,
//! labels, transfer — plus the `discussions` unit flag, ACL,
//! `on: discussion|discussion_comment` triggers, and webhook fan-out.

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

async fn run_count(db: &Database, repo_id: &str) -> usize {
    db.list_action_runs_for_repo(repo_id, &Default::default(), 100, 0)
        .await
        .unwrap()
        .len()
}

/// Category + thread + comment CRUD — shared `#N` sequence with issues,
/// slug derivation, answerable categories, pin/lock, and one-level replies.
#[tokio::test]
async fn discussions_crud_and_answers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("disc_crud.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "dc1@ex.com", "dc1").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &user_id).await;
    create_repo(&app, &cookie, "demo", "public").await;
    let repo_id = db
        .find_repository_by_owner_name(&user_id, "demo")
        .await
        .unwrap()
        .unwrap()
        .id;

    // Categories — slug derived from the display name.
    let cat = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.categories.create","input":{"owner":"dc1","name":"demo","category_name":"Q&A","emoji":"🙏","is_answerable":true}}"#,
    )
    .await;
    assert_eq!(cat["ok"], true, "{cat}");
    let cat_id = cat["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(cat["data"]["slug"], "q-a");
    assert_eq!(cat["data"]["is_answerable"], true);

    let gen = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.categories.create","input":{"owner":"dc1","name":"demo","category_name":"General"}}"#,
    )
    .await;
    let gen_id = gen["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(gen["data"]["slug"], "general");
    assert_eq!(gen["data"]["is_answerable"], false);

    // Discussion in Q&A → `#1` on the shared issue sequence.
    let created = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.create","input":{{"owner":"dc1","name":"demo","title":"How do I X?","body":"Stuck on step 2.","category":"{cat_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(created["data"]["number"], 1);
    assert_eq!(created["data"]["category"]["slug"], "q-a");
    assert_eq!(created["data"]["html_url"], "/dc1/demo/discussions/1");
    assert_eq!(created["data"]["author_username"], "dc1");

    // An issue shares the sequence — issue #2, then discussion #3.
    let issue_id = uuid::Uuid::new_v4().to_string();
    db.insert_issue(&issue_id, &repo_id, &user_id, "bug", "b")
        .await
        .expect("issue");
    let third = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.create","input":{"owner":"dc1","name":"demo","title":"Ideas"}}"#,
    )
    .await;
    assert_eq!(
        third["data"]["number"], 3,
        "shared issue sequence — {third}"
    );

    // Category filter by slug.
    let filtered = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.list","input":{"owner":"dc1","name":"demo","category":"q-a"}}"#,
    )
    .await;
    assert_eq!(filtered["data"]["discussions"].as_array().unwrap().len(), 1);

    // Answered filter.
    let unanswered = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.list","input":{"owner":"dc1","name":"demo","answered":false}}"#,
    )
    .await;
    assert_eq!(
        unanswered["data"]["discussions"].as_array().unwrap().len(),
        2
    );

    // Comments — top-level + one-level reply; replying to a reply fails.
    let c1 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.comments.create","input":{"owner":"dc1","name":"demo","discussion":1,"body":"First top comment"}}"#,
    )
    .await;
    assert_eq!(c1["ok"], true, "{c1}");
    let c1_id = c1["data"]["id"].as_str().unwrap().to_string();
    let reply = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.comments.create","input":{{"owner":"dc1","name":"demo","discussion":1,"body":"a reply","parent":"{c1_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["data"]["parent_id"], c1_id);
    let reply_id = reply["data"]["id"].as_str().unwrap().to_string();
    let deep = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.comments.create","input":{{"owner":"dc1","name":"demo","discussion":1,"body":"too deep","parent":"{reply_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(deep["ok"], false, "one level only — {deep}");

    let comments = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.comments.list","input":{"owner":"dc1","name":"demo","discussion":1}}"#,
    )
    .await;
    assert_eq!(comments["data"]["comments"].as_array().unwrap().len(), 2);

    // Cross-thread comment ids don't resolve.
    let foreign = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.comments.update","input":{{"owner":"dc1","name":"demo","discussion":3,"comment":"{c1_id}","body":"x"}}}}"#
        ),
    )
    .await;
    assert_eq!(foreign["ok"], false);
    assert_eq!(foreign["error"]["code"], "discussion.comment_not_found");

    // Mark the answer — only answerable categories accept one.
    let ans = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.answer.set","input":{{"owner":"dc1","name":"demo","discussion":1,"comment":"{c1_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(ans["ok"], true, "{ans}");
    assert_eq!(ans["data"]["answer_comment_id"], c1_id);
    assert!(ans["data"]["answered_at"].is_string());
    assert_eq!(ans["data"]["answered_by_username"], "dc1");

    let no_ans = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.answer.set","input":{{"owner":"dc1","name":"demo","discussion":3,"comment":"{c1_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(no_ans["ok"], false);
    // comment doesn't belong to discussion 3 → comment_not_found wins over
    // not_answerable (scope check first, like GitHub's resource scoping).
    assert_eq!(no_ans["error"]["code"], "discussion.comment_not_found");

    let unanswerable = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.answer.clear","input":{"owner":"dc1","name":"demo","discussion":1}}"#,
    )
    .await;
    assert_eq!(unanswerable["ok"], true);
    assert!(unanswerable["data"]["answer_comment_id"].is_null());

    // Uncategorized discussion 3 gets the non-answerable rejection.
    let c3 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.comments.create","input":{"owner":"dc1","name":"demo","discussion":3,"body":"on 3"}}"#,
    )
    .await;
    let c3_id = c3["data"]["id"].as_str().unwrap().to_string();
    let reject = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.answer.set","input":{{"owner":"dc1","name":"demo","discussion":3,"comment":"{c3_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(reject["ok"], false);
    assert_eq!(reject["error"]["code"], "discussion.not_answerable");

    // Pin + lock flags.
    let pinned = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.pin","input":{"owner":"dc1","name":"demo","discussion":1,"pinned":true}}"#,
    )
    .await;
    assert_eq!(pinned["data"]["pinned"], true);
    let locked = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.lock","input":{"owner":"dc1","name":"demo","discussion":1,"locked":true}}"#,
    )
    .await;
    assert_eq!(locked["data"]["locked"], true);

    // Locked threads reject new comments from non-collaborators — a fresh
    // user can't comment; the owner (write+) still can.
    let (ac, alogin) = signup_and_login(&app, "dc2@ex.com", "dc2").await;
    let other_id = alogin["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &other_id).await;
    let blocked = rpc_json(
        &app,
        &ac,
        r#"{"procedure":"discussions.comments.create","input":{"owner":"dc1","name":"demo","discussion":1,"body":"hi"}}"#,
    )
    .await;
    assert_eq!(blocked["ok"], false);
    assert_eq!(blocked["error"]["code"], "discussion.locked");

    // Title edit + category change.
    let upd = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.update","input":{{"owner":"dc1","name":"demo","discussion":1,"title":"How do I X? (solved)","category":"{gen_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["title"], "How do I X? (solved)");
    assert_eq!(upd["data"]["category"]["slug"], "general");

    // Deleting the category leaves the discussion uncategorized (SET NULL).
    let del_cat = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.categories.delete","input":{{"owner":"dc1","name":"demo","category":"{gen_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(del_cat["ok"], true, "{del_cat}");
    let after = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.get","input":{"owner":"dc1","name":"demo","discussion":1}}"#,
    )
    .await;
    assert!(after["data"]["category"].is_null(), "{after}");

    // Discussion delete.
    let del = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.delete","input":{"owner":"dc1","name":"demo","discussion":3}}"#,
    )
    .await;
    assert_eq!(del["ok"], true);
    let gone = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.get","input":{"owner":"dc1","name":"demo","discussion":3}}"#,
    )
    .await;
    assert_eq!(gone["error"]["code"], "discussion.not_found");
}

/// `repo.discussions.setEnabled` gates every `discussions.*` RPC; `repo.get`
/// surfaces `discussions_enabled`; ACL matches Issues/Pulls.
#[tokio::test]
async fn discussions_unit_flag_and_acl() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("disc_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "dc3@ex.com", "dc3").await;
    let owner_id = login_v["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &owner_id).await;
    create_repo(&app, &cookie, "demo", "public").await;
    let repo_id = db
        .find_repository_by_owner_name(&owner_id, "demo")
        .await
        .unwrap()
        .unwrap()
        .id;

    let got = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.get","input":{"owner":"dc3","name":"demo"}}"#,
    )
    .await;
    assert_eq!(got["data"]["discussions_enabled"], true, "{got}");

    let off = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.discussions.setEnabled","input":{"owner":"dc3","name":"demo","enabled":false}}"#,
    )
    .await;
    assert_eq!(off["ok"], true, "{off}");
    for proc_body in [
        r#"{"procedure":"discussions.list","input":{"owner":"dc3","name":"demo"}}"#,
        r#"{"procedure":"discussions.create","input":{"owner":"dc3","name":"demo","title":"x"}}"#,
        r#"{"procedure":"discussions.categories.list","input":{"owner":"dc3","name":"demo"}}"#,
    ] {
        let v = rpc_json(&app, &cookie, proc_body).await;
        assert_eq!(v["ok"], false, "{v}");
        assert_eq!(v["error"]["code"], "repo.discussions.disabled", "{v}");
    }
    let got_off = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.get","input":{"owner":"dc3","name":"demo"}}"#,
    )
    .await;
    assert_eq!(got_off["data"]["discussions_enabled"], false);

    let on = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.discussions.setEnabled","input":{"owner":"dc3","name":"demo","enabled":true}}"#,
    )
    .await;
    assert_eq!(on["ok"], true, "{on}");

    // ACL — a read collaborator can read + comment but not create/lock.
    let (rc, rlogin) = signup_and_login(&app, "dc4@ex.com", "dc4").await;
    let reader_id = rlogin["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &reader_id).await;
    db.insert_repo_collaborator(&repo_id, &reader_id, "read")
        .await
        .expect("collaborator");

    let rlist = rpc_json(
        &app,
        &rc,
        r#"{"procedure":"discussions.list","input":{"owner":"dc3","name":"demo"}}"#,
    )
    .await;
    assert_eq!(rlist["ok"], true, "{rlist}");
    let rcreate = rpc_json(
        &app,
        &rc,
        r#"{"procedure":"discussions.create","input":{"owner":"dc3","name":"demo","title":"sneak"}}"#,
    )
    .await;
    assert_eq!(rcreate["ok"], false);
    assert_eq!(rcreate["error"]["code"], "repo.not_found");
    let rpin = rpc_json(
        &app,
        &rc,
        r#"{"procedure":"discussions.pin","input":{"owner":"dc3","name":"demo","discussion":1,"pinned":true}}"#,
    )
    .await;
    assert_eq!(rpin["ok"], false);
    assert_eq!(rpin["error"]["code"], "repo.not_found");

    // Unit toggle is admin-only.
    let roff = rpc_json(
        &app,
        &rc,
        r#"{"procedure":"repo.discussions.setEnabled","input":{"owner":"dc3","name":"demo","enabled":false}}"#,
    )
    .await;
    assert_eq!(roff["ok"], false);
    assert_eq!(roff["error"]["code"], "repo.not_found");
}

/// `discussion`/`discussion_comment` lifecycle triggers, `types:` filtering,
/// webhook payload shapes, and the transfer + label event vocabulary.
#[tokio::test]
async fn discussions_events_labels_transfer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("disc_ev.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "dc5@ex.com", "dc5").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &user_id).await;
    create_repo(&app, &cookie, "demo", "public").await;
    create_repo(&app, &cookie, "other", "public").await;

    let workflow = br#"
name: DiscTrig
on:
  discussion:
    types: [created, answered]
  discussion_comment:
    types: [created]
jobs:
  d:
    runs-on: ubuntu-latest
    steps:
      - run: echo discussions
"#;
    let git = CliGitBackend::new();
    let bare = repos_dir.join("dc5").join("demo.git");
    git.init_bare(&bare, "main").await.expect("init bare");
    git.seed_commit(
        &bare,
        "main",
        "seed",
        &[
            ("README.md".to_string(), b"hi".to_vec()),
            (".github/workflows/disc.yml".to_string(), workflow.to_vec()),
        ],
    )
    .await
    .expect("seed");

    let hook = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"webhook.create","input":{"owner":"dc5","name":"demo","url":"http://127.0.0.1:9/hook","secret":"s","events":["discussion","discussion_comment"]}}"#,
    )
    .await;
    assert_eq!(hook["ok"], true, "{hook}");
    let hook_id = hook["data"]["id"].as_str().unwrap().to_string();
    // A hook on the transfer target — `transferred` fires there, not on the
    // source repo's hook (GitHub parity).
    let other_hook = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"webhook.create","input":{"owner":"dc5","name":"other","url":"http://127.0.0.1:9/hook2","secret":"s","events":["discussion"]}}"#,
    )
    .await;
    assert_eq!(other_hook["ok"], true, "{other_hook}");
    let other_hook_id = other_hook["data"]["id"].as_str().unwrap().to_string();

    let repo_id = db
        .find_repository_by_owner_name(&user_id, "demo")
        .await
        .unwrap()
        .unwrap()
        .id;
    assert_eq!(run_count(&db, &repo_id).await, 0);

    // Category + discussion → `created` run + webhook.
    let cat = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.categories.create","input":{"owner":"dc5","name":"demo","category_name":"Q&A","is_answerable":true}}"#,
    )
    .await;
    let cat_id = cat["data"]["id"].as_str().unwrap().to_string();
    let created = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.create","input":{{"owner":"dc5","name":"demo","title":"Q?","category":"{cat_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(run_count(&db, &repo_id).await, 1, "on: discussion[created]");

    // Comment → `discussion_comment.created` run.
    let c1 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.comments.create","input":{"owner":"dc5","name":"demo","discussion":1,"body":"try this"}}"#,
    )
    .await;
    assert_eq!(c1["ok"], true);
    let c1_id = c1["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        run_count(&db, &repo_id).await,
        2,
        "on: discussion_comment[created]"
    );

    // Answer → `answered` is in the types filter → run.
    let ans = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.answer.set","input":{{"owner":"dc5","name":"demo","discussion":1,"comment":"{c1_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(ans["ok"], true, "{ans}");
    assert_eq!(
        run_count(&db, &repo_id).await,
        3,
        "on: discussion[answered]"
    );

    // Pin → `pinned` is NOT in the types filter → no run.
    let pin = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.pin","input":{"owner":"dc5","name":"demo","discussion":1,"pinned":true}}"#,
    )
    .await;
    assert_eq!(pin["ok"], true);
    assert_eq!(run_count(&db, &repo_id).await, 3, "pinned filtered out");

    // Labels — repo label + set → `labeled` webhook with payload.label.
    let label_id = uuid::Uuid::new_v4().to_string();
    db.insert_label(
        &label_id,
        "docs",
        "aa00ff",
        "documentation",
        None,
        Some(&repo_id),
    )
    .await
    .expect("label");
    let labeled = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"discussions.labels.set","input":{{"owner":"dc5","name":"demo","discussion":1,"label_ids":["{label_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(labeled["ok"], true, "{labeled}");
    assert_eq!(labeled["data"]["label_ids"], serde_json::json!([label_id]));

    // Transfer to `other` — new #N there, category/answer dropped.
    let xfer = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.transfer","input":{"owner":"dc5","name":"demo","discussion":1,"target_owner":"dc5","target_name":"other"}}"#,
    )
    .await;
    assert_eq!(xfer["ok"], true, "{xfer}");
    assert_eq!(xfer["data"]["number"], 1, "first number in target");
    assert!(xfer["data"]["category"].is_null());
    assert!(xfer["data"]["answer_comment_id"].is_null());
    assert_eq!(xfer["data"]["html_url"], "/dc5/other/discussions/1");

    // Self-transfer is refused.
    let self_xfer = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.transfer","input":{"owner":"dc5","name":"demo","discussion":1,"target_owner":"dc5","target_name":"demo"}}"#,
    )
    .await;
    assert_eq!(self_xfer["ok"], false);

    // Webhook deliveries — GitHub action vocabulary.
    let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.unwrap();
    let disc_actions: Vec<String> = deliveries
        .iter()
        .filter(|d| d.event == "discussion")
        .map(|d| {
            serde_json::from_str::<serde_json::Value>(&d.payload_json).unwrap()["action"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    for want in ["created", "answered", "pinned", "labeled"] {
        assert!(
            disc_actions.iter().any(|a| a == want),
            "missing discussion.{want} — got {disc_actions:?}"
        );
    }

    // `transferred` lands on the destination repo's hook with
    // `changes.new_repository`.
    let other_deliveries = db
        .list_webhook_deliveries(&other_hook_id, 50)
        .await
        .unwrap();
    let xfer_del = other_deliveries
        .iter()
        .find(|d| d.event == "discussion" && d.payload_json.contains("\"transferred\""))
        .expect("transferred delivery on target");
    let xp: serde_json::Value = serde_json::from_str(&xfer_del.payload_json).unwrap();
    assert_eq!(xp["action"], "transferred");
    assert_eq!(xp["changes"]["new_repository"]["name"], "other");
    assert_eq!(xp["repository"]["name"], "other");

    let created_del = deliveries
        .iter()
        .find(|d| d.event == "discussion" && d.payload_json.contains("\"created\""))
        .expect("created delivery");
    let p: serde_json::Value = serde_json::from_str(&created_del.payload_json).unwrap();
    assert_eq!(p["discussion"]["number"], 1);
    assert_eq!(p["discussion"]["title"], "Q?");
    assert_eq!(p["discussion"]["category"]["slug"], "q-a");
    assert_eq!(p["discussion"]["category"]["is_answerable"], true);
    assert_eq!(p["repository"]["name"], "demo");
    assert_eq!(p["sender"]["login"], "dc5");

    let labeled_del = deliveries
        .iter()
        .find(|d| d.event == "discussion" && d.payload_json.contains("\"labeled\""))
        .expect("labeled delivery");
    let lp: serde_json::Value = serde_json::from_str(&labeled_del.payload_json).unwrap();
    assert_eq!(lp["label"]["name"], "docs");

    let comment_del = deliveries
        .iter()
        .find(|d| d.event == "discussion_comment")
        .expect("comment delivery");
    let cp: serde_json::Value = serde_json::from_str(&comment_del.payload_json).unwrap();
    assert_eq!(cp["action"], "created");
    assert_eq!(cp["comment"]["body"], "try this");
    assert_eq!(cp["discussion"]["number"], 1);

    // The thread lives in `other` now — edits resolve there, not `demo`,
    // and `edited` delivers to the target repo's hook.
    let upd = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.update","input":{"owner":"dc5","name":"demo","discussion":1,"title":"Q? edited"}}"#,
    )
    .await;
    assert_eq!(upd["ok"], false, "discussion moved — {upd}");
    let moved = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"discussions.update","input":{"owner":"dc5","name":"other","discussion":1,"title":"Q? edited"}}"#,
    )
    .await;
    assert_eq!(moved["ok"], true, "{moved}");
    let edited_del = db
        .list_webhook_deliveries(&other_hook_id, 50)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.event == "discussion" && d.payload_json.contains("\"edited\""))
        .expect("edited delivery");
    let ep: serde_json::Value = serde_json::from_str(&edited_del.payload_json).unwrap();
    assert_eq!(ep["changes"]["title"]["from"], "Q?");
}
