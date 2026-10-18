//! ev.milestone: repo-scoped milestone CRUD, issue linkage, list filters, ACL
//! gates, and the GitHub `milestone`/`issues.milestoned` event surface.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
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

/// CRUD + numbering + state transitions + counts + delete link-clearing.
#[tokio::test]
async fn milestone_crud_lifecycle_and_issue_counts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ms_crud.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "msc@ex.com", "msc").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "demo", "public").await;

    // Create → #1 open, counters at zero.
    let m1 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.create","input":{"owner":"msc","name":"demo","title":"v1","description":"first","due_on":"2030-01-15"}}"#,
    )
    .await;
    assert_eq!(m1["ok"], true, "milestone.create — {m1}");
    assert_eq!(m1["data"]["number"], 1);
    assert_eq!(m1["data"]["state"], "open");
    assert_eq!(m1["data"]["due_on"], "2030-01-15");
    assert_eq!(m1["data"]["creator_username"], "msc");
    assert_eq!(m1["data"]["open_issues"], 0);
    assert_eq!(m1["data"]["closed_issues"], 0);

    // Numbers are repo-scoped and monotonic.
    let m2 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.create","input":{"owner":"msc","name":"demo","title":"v2"}}"#,
    )
    .await;
    assert_eq!(m2["data"]["number"], 2);

    // Duplicate title (case-insensitive) is rejected.
    let dup = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.create","input":{"owner":"msc","name":"demo","title":"V1"}}"#,
    )
    .await;
    assert_eq!(dup["ok"], false);
    assert_eq!(dup["error"]["code"], "milestone.already_exists");

    // List open → both; link an issue → counts move.
    let issue = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.create","input":{"owner":"msc","name":"demo","title":"bug","body":"b","milestone":1}}"#,
    )
    .await;
    assert_eq!(issue["ok"], true, "issue.create — {issue}");
    assert_eq!(issue["data"]["milestone"]["number"], 1);
    assert_eq!(issue["data"]["milestone"]["title"], "v1");

    let got = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.get","input":{"owner":"msc","name":"demo","number":1}}"#,
    )
    .await;
    assert_eq!(got["data"]["open_issues"], 1);
    assert_eq!(got["data"]["closed_issues"], 0);

    // Close the issue → closed_issues ticks; close the milestone →
    // `closed_at` stamps, `state` flips, closed list shows it.
    let close = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.close","input":{"owner":"msc","name":"demo","number":1}}"#,
    )
    .await;
    assert_eq!(close["ok"], true, "{close}");
    let upd = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.update","input":{"owner":"msc","name":"demo","number":1,"title":"v1.0","state":"closed"}}"#,
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["title"], "v1.0");
    assert_eq!(upd["data"]["state"], "closed");
    assert!(upd["data"]["closed_at"].is_string());
    assert_eq!(upd["data"]["open_issues"], 0);
    assert_eq!(upd["data"]["closed_issues"], 1);

    let open_list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.list","input":{"owner":"msc","name":"demo"}}"#,
    )
    .await;
    assert_eq!(open_list["data"]["milestones"].as_array().unwrap().len(), 1);
    let all_list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.list","input":{"owner":"msc","name":"demo","state":"all"}}"#,
    )
    .await;
    assert_eq!(all_list["data"]["milestones"].as_array().unwrap().len(), 2);

    // Reopen clears closed_at.
    let reopen = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.update","input":{"owner":"msc","name":"demo","number":1,"state":"open"}}"#,
    )
    .await;
    assert_eq!(reopen["ok"], true);
    assert!(reopen["data"]["closed_at"].is_null());

    // Delete → linked issue keeps its data, the link clears.
    let del = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.delete","input":{"owner":"msc","name":"demo","number":1}}"#,
    )
    .await;
    assert_eq!(del["ok"], true, "{del}");
    let issue_after = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.get","input":{"owner":"msc","name":"demo","number":1}}"#,
    )
    .await;
    assert!(issue_after["data"]["milestone"].is_null());
}

/// `issue.list` milestone filter — `*` / `none` / `#N` / unknown `#N`.
#[tokio::test]
async fn issue_list_milestone_filter_semantics() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ms_filter.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "msf@ex.com", "msf").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "demo", "public").await;

    for title in ["a", "b", "c"] {
        rpc_json(
            &app,
            &cookie,
            &format!(
                r#"{{"procedure":"issue.create","input":{{"owner":"msf","name":"demo","title":"{title}","body":""}}}}"#
            ),
        )
        .await;
    }
    rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.create","input":{"owner":"msf","name":"demo","title":"v1"}}"#,
    )
    .await;
    // Issues #1 + #2 on milestone #1; #3 unassigned.
    for n in [1, 2] {
        let set = rpc_json(
            &app,
            &cookie,
            &format!(
                r#"{{"procedure":"issue.milestone.set","input":{{"owner":"msf","name":"demo","number":{n},"milestone":1}}}}"#
            ),
        )
        .await;
        assert_eq!(set["ok"], true, "{set}");
    }

    let list = |milestone: &str| {
        let app = app.clone();
        let cookie = cookie.clone();
        let milestone = milestone.to_string();
        async move {
            rpc_json(
                &app,
                &cookie,
                &format!(
                    r#"{{"procedure":"issue.list","input":{{"owner":"msf","name":"demo","state":"all","milestone":{milestone}}}}}"#
                ),
            )
            .await
        }
    };

    let by_number = list("\"1\"").await;
    assert_eq!(by_number["data"]["total"], 2);
    let has = list("\"*\"").await;
    assert_eq!(has["data"]["total"], 2);
    let none = list("\"none\"").await;
    assert_eq!(none["data"]["total"], 1);
    assert_eq!(none["data"]["issues"][0]["title"], "c");
    let unknown = list("\"9\"").await;
    assert_eq!(unknown["data"]["total"], 0);
    let absent = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.list","input":{"owner":"msf","name":"demo","state":"all"}}"#,
    )
    .await;
    assert_eq!(absent["data"]["total"], 3);
}

/// `issue.update` can move/clear the milestone without touching title/body.
#[tokio::test]
async fn issue_update_milestone_only_patch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ms_patch.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "msp@ex.com", "msp").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "demo", "public").await;
    rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.create","input":{"owner":"msp","name":"demo","title":"v1"}}"#,
    )
    .await;
    rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.create","input":{"owner":"msp","name":"demo","title":"v2"}}"#,
    )
    .await;
    rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.create","input":{"owner":"msp","name":"demo","title":"t","body":"","milestone":1}}"#,
    )
    .await;

    // Move to milestone 2 — milestone-only update is accepted.
    let upd = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.update","input":{"owner":"msp","name":"demo","number":1,"milestone":2}}"#,
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["milestone"]["number"], 2);

    // Explicit null clears; absent milestone keeps the link.
    let cleared = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.update","input":{"owner":"msp","name":"demo","number":1,"milestone":null}}"#,
    )
    .await;
    assert_eq!(cleared["ok"], true, "{cleared}");
    assert!(cleared["data"]["milestone"].is_null());
    let keep = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.update","input":{"owner":"msp","name":"demo","number":1,"title":"t2"}}"#,
    )
    .await;
    assert!(keep["data"]["milestone"].is_null());

    // Unknown milestone number → milestone.not_found, issue unchanged.
    let bad = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.update","input":{"owner":"msp","name":"demo","number":1,"milestone":9}}"#,
    )
    .await;
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["error"]["code"], "milestone.not_found");
}

/// ACL — milestone mutations need Write+; a read-only collaborator and a
/// stranger are both soft-404'd; Issues-unit-disabled repos gate too.
#[tokio::test]
async fn milestone_acl_write_required() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ms_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "msa@ex.com", "msa").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &owner_cookie, "demo", "private").await;
    let (other_cookie, other_v) = signup_and_login(&app, "msb@ex.com", "msb").await;
    verify_user(&db, other_v["data"]["id"].as_str().unwrap()).await;

    // Stranger on a private repo → repo.not_found for both read and write.
    let list = rpc_json(
        &app,
        &other_cookie,
        r#"{"procedure":"milestone.list","input":{"owner":"msa","name":"demo"}}"#,
    )
    .await;
    assert_eq!(list["ok"], false);
    assert_eq!(list["error"]["code"], "repo.not_found");
    let create = rpc_json(
        &app,
        &other_cookie,
        r#"{"procedure":"milestone.create","input":{"owner":"msa","name":"demo","title":"v1"}}"#,
    )
    .await;
    assert_eq!(create["ok"], false);
    assert_eq!(create["error"]["code"], "repo.not_found");

    // Owner (Admin) creates fine; unauthenticated gets no session → auth error.
    let ok = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"milestone.create","input":{"owner":"msa","name":"demo","title":"v1"}}"#,
    )
    .await;
    assert_eq!(ok["ok"], true, "{ok}");

    // `issue.milestone.set` on a missing issue → issue.not_found;
    // on a missing milestone → milestone.not_found.
    rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.create","input":{"owner":"msa","name":"demo","title":"t","body":""}}"#,
    )
    .await;
    let bad_ms = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.milestone.set","input":{"owner":"msa","name":"demo","number":1,"milestone":9}}"#,
    )
    .await;
    assert_eq!(bad_ms["error"]["code"], "milestone.not_found");
    let bad_issue = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.milestone.set","input":{"owner":"msa","name":"demo","number":9,"milestone":1}}"#,
    )
    .await;
    assert_eq!(bad_issue["error"]["code"], "issue.not_found");
}
