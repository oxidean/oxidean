//! DEBT-06 — watch levels (`all` | `participating` | `ignore`), notification
//! fan-out matrix, and user follow RPCs.

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

async fn rpc_json_anon(app: &axum::Router, body: &str) -> serde_json::Value {
    let res = app.clone().oneshot(rpc_req(body)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn create_repo(app: &axum::Router, cookie: &str, name: &str) {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"public","description":""}}}}"#
    );
    let v = rpc_json(app, cookie, &body).await;
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

async fn add_collaborator(app: &axum::Router, cookie: &str, owner: &str, name: &str, username: &str) {
    let body = format!(
        r#"{{"procedure":"repo.collaborators.add","input":{{"owner":"{owner}","name":"{name}","username":"{username}","permission":"write"}}}}"#
    );
    let v = rpc_json(app, cookie, &body).await;
    assert_eq!(v["ok"], true, "repo.collaborators.add — {v}");
}

async fn create_issue(app: &axum::Router, cookie: &str, owner: &str, name: &str, title: &str) {
    let body = format!(
        r#"{{"procedure":"issue.create","input":{{"owner":"{owner}","name":"{name}","title":"{title}","body":"root"}}}}"#
    );
    let v = rpc_json(app, cookie, &body).await;
    assert_eq!(v["ok"], true, "issue.create — {v}");
}

async fn comment_issue(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    name: &str,
    body_text: &str,
) -> serde_json::Value {
    let body = format!(
        r#"{{"procedure":"issue.comments.create","input":{{"owner":"{owner}","name":"{name}","number":1,"body":"{body_text}"}}}}"#
    );
    rpc_json(app, cookie, &body).await
}

async fn watch(app: &axum::Router, cookie: &str, owner: &str, name: &str, level: &str) -> serde_json::Value {
    let body = if level.is_empty() {
        format!(r#"{{"procedure":"repo.watch","input":{{"owner":"{owner}","name":"{name}"}}}}"#)
    } else {
        format!(
            r#"{{"procedure":"repo.watch","input":{{"owner":"{owner}","name":"{name}","level":"{level}"}}}}"#
        )
    };
    rpc_json(app, cookie, &body).await
}

async fn unread_count(app: &axum::Router, cookie: &str) -> i64 {
    let v = rpc_json(
        app,
        cookie,
        r#"{"procedure":"notification.unreadCount","input":{}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "notification.unreadCount — {v}");
    v["data"]["count"].as_i64().unwrap()
}

async fn unread_reasons(app: &axum::Router, cookie: &str) -> Vec<String> {
    let v = rpc_json(
        app,
        cookie,
        r#"{"procedure":"notification.list","input":{"filter":"unread"}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "notification.list — {v}");
    v["data"]["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["reason"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn watch_level_roundtrip_and_count_deltas() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("watch_levels.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "wlown@ex.com", "wlown").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &owner_cookie, "feed").await;

    let (watcher_cookie, watcher_v) = signup_and_login(&app, "wlwatch@ex.com", "wlwatch").await;
    verify_user(&db, watcher_v["data"]["id"].as_str().unwrap()).await;

    // Default watch → `all`, watch_count 1.
    let v = watch(&app, &watcher_cookie, "wlown", "feed", "").await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["viewer_watch_level"], "all");
    assert_eq!(v["data"]["viewer_is_watching"], true);
    assert_eq!(v["data"]["watch_count"], 1);

    // Same level upsert is idempotent — no double count.
    let v = watch(&app, &watcher_cookie, "wlown", "feed", "all").await;
    assert_eq!(v["data"]["watch_count"], 1, "{v}");

    // Downgrade to ignore → subscription row remains, active count drops.
    let v = watch(&app, &watcher_cookie, "wlown", "feed", "ignore").await;
    assert_eq!(v["data"]["viewer_watch_level"], "ignore");
    assert_eq!(v["data"]["viewer_is_watching"], false);
    assert_eq!(v["data"]["watch_count"], 0, "{v}");

    // Ignore → participating reactivates the count.
    let v = watch(&app, &watcher_cookie, "wlown", "feed", "participating").await;
    assert_eq!(v["data"]["viewer_watch_level"], "participating");
    assert_eq!(v["data"]["viewer_is_watching"], true);
    assert_eq!(v["data"]["watch_count"], 1, "{v}");

    // Invalid level → bad_input.
    let v = rpc_json(
        &app,
        &watcher_cookie,
        r#"{"procedure":"repo.watch","input":{"owner":"wlown","name":"feed","level":"loud"}}"#,
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");

    // Back to ignore, then unwatch — count must not go negative.
    let _ = watch(&app, &watcher_cookie, "wlown", "feed", "ignore").await;
    let v = rpc_json(
        &app,
        &watcher_cookie,
        r#"{"procedure":"repo.unwatch","input":{"owner":"wlown","name":"feed"}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["watch_count"], 0);
    assert!(v["data"]["viewer_watch_level"].is_null(), "{v}");

    // Ignore watchers do not appear on the public watchers list.
    let _ = watch(&app, &watcher_cookie, "wlown", "feed", "ignore").await;
    let v = rpc_json_anon(
        &app,
        r#"{"procedure":"repo.watchers.list","input":{"owner":"wlown","name":"feed"}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["total"], 0, "{v}");

    // listWatched still surfaces the ignore row for the settings matrix.
    let v = rpc_json(
        &app,
        &watcher_cookie,
        r#"{"procedure":"user.listWatched","input":{}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    let repos = v["data"]["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 1, "{v}");
    assert_eq!(repos[0]["name"], "feed");
    assert_eq!(repos[0]["viewer_watch_level"], "ignore");
}

#[tokio::test]
async fn fanout_honors_watch_levels() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("watch_fanout.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "foown@ex.com", "foown").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &owner_cookie, "loud").await;

    let mut cookies: Vec<(String, String)> = Vec::new();
    for (email, username) in [
        ("foall@ex.com", "foall"),
        ("fopart@ex.com", "fopart"),
        ("foign@ex.com", "foign"),
        ("fowriter@ex.com", "fowriter"),
    ] {
        let (cookie, v) = signup_and_login(&app, email, username).await;
        verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
        cookies.push((username.to_string(), cookie));
    }
    let cookie_of = |name: &str| -> &String {
        &cookies.iter().find(|(u, _)| u == name).unwrap().1
    };
    let all_cookie = cookie_of("foall").clone();
    let part_cookie = cookie_of("fopart").clone();
    let ign_cookie = cookie_of("foign").clone();
    let writer_cookie = cookie_of("fowriter").clone();

    // Watch at each level; writer gets collaborator rights to comment.
    let v = watch(&app, &all_cookie, "foown", "loud", "all").await;
    assert_eq!(v["ok"], true, "{v}");
    let v = watch(&app, &part_cookie, "foown", "loud", "participating").await;
    assert_eq!(v["ok"], true, "{v}");
    let v = watch(&app, &ign_cookie, "foown", "loud", "ignore").await;
    assert_eq!(v["ok"], true, "{v}");
    add_collaborator(&app, &owner_cookie, "foown", "loud", "fowriter").await;
    add_collaborator(&app, &owner_cookie, "foown", "loud", "fopart").await;

    // Issue open → `all` watcher gets issue_opened; participating/ignore do not.
    create_issue(&app, &writer_cookie, "foown", "loud", "First").await;
    assert!(
        unread_reasons(&app, &all_cookie).await.contains(&"issue_opened".to_string()),
        "all watcher should see issue_opened"
    );
    assert_eq!(unread_count(&app, &part_cookie).await, 0);
    assert_eq!(unread_count(&app, &ign_cookie).await, 0);

    // Mark the all-watcher's inbox read so later assertions count fresh rows.
    let _ = rpc_json(
        &app,
        &all_cookie,
        r#"{"procedure":"notification.markAllRead","input":{}}"#,
    )
    .await;

    // Comment from writer → all watcher notified; participating watcher (non-participant) not.
    let v = comment_issue(&app, &writer_cookie, "foown", "loud", "first ping").await;
    assert_eq!(v["ok"], true, "{v}");
    assert!(
        unread_reasons(&app, &all_cookie).await.contains(&"issue_comment".to_string()),
        "all watcher should see issue_comment"
    );
    assert_eq!(unread_count(&app, &part_cookie).await, 0);
    assert_eq!(unread_count(&app, &ign_cookie).await, 0);

    // Participating watcher joins the thread → next comment notifies them.
    let v = comment_issue(&app, &part_cookie, "foown", "loud", "i'm in").await;
    assert_eq!(v["ok"], true, "{v}");
    let _ = rpc_json(
        &app,
        &all_cookie,
        r#"{"procedure":"notification.markAllRead","input":{}}"#,
    )
    .await;
    let v = comment_issue(&app, &writer_cookie, "foown", "loud", "second ping").await;
    assert_eq!(v["ok"], true, "{v}");
    assert!(
        unread_reasons(&app, &part_cookie)
            .await
            .contains(&"issue_comment".to_string()),
        "participating watcher should see issue_comment after joining"
    );
    assert!(
        unread_reasons(&app, &all_cookie).await.contains(&"issue_comment".to_string()),
        "all watcher should see second issue_comment"
    );

    // Ignore suppresses even direct @-mentions.
    let v = comment_issue(&app, &writer_cookie, "foown", "loud", "hey @foign look").await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(unread_count(&app, &ign_cookie).await, 0, "ignore watcher must stay silent");

    // Non-watcher mention still delivers (legacy pass-through).
    let (_anon_cookie, _anon_v) = ("", ());
    let (nomatch_cookie, nomatch_v) = signup_and_login(&app, "fono@ex.com", "fono").await;
    verify_user(&db, nomatch_v["data"]["id"].as_str().unwrap()).await;
    let v = comment_issue(&app, &writer_cookie, "foown", "loud", "hey @fono look").await;
    assert_eq!(v["ok"], true, "{v}");
    assert!(
        unread_reasons(&app, &nomatch_cookie)
            .await
            .contains(&"issue_mention".to_string()),
        "non-watcher mention should still notify"
    );
}

#[tokio::test]
async fn follow_unfollow_roundtrip_and_lists() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("follows.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (a_cookie, a_v) = signup_and_login(&app, "fa@ex.com", "fa").await;
    verify_user(&db, a_v["data"]["id"].as_str().unwrap()).await;
    let (b_cookie, b_v) = signup_and_login(&app, "fb@ex.com", "fb").await;
    verify_user(&db, b_v["data"]["id"].as_str().unwrap()).await;

    // Self-follow rejected.
    let v = rpc_json(
        &app,
        &a_cookie,
        r#"{"procedure":"user.follow","input":{"username":"fa"}}"#,
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "user.self_follow", "{v}");

    // A follows B → target profile shows count + viewer flag.
    let v = rpc_json(
        &app,
        &a_cookie,
        r#"{"procedure":"user.follow","input":{"username":"fb"}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["username"], "fb");
    assert_eq!(v["data"]["follower_count"], 1);
    assert_eq!(v["data"]["following_count"], 0);
    assert_eq!(v["data"]["viewer_is_following"], true);

    // Idempotent second follow keeps count at 1.
    let v = rpc_json(
        &app,
        &a_cookie,
        r#"{"procedure":"user.follow","input":{"username":"fb"}}"#,
    )
    .await;
    assert_eq!(v["data"]["follower_count"], 1, "{v}");

    // Anonymous getPublicProfile shows counts; viewer flag stays false.
    let v = rpc_json_anon(
        &app,
        r#"{"procedure":"user.getPublicProfile","input":{"username":"fb"}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["follower_count"], 1);
    assert_eq!(v["data"]["viewer_is_following"], false);

    // Public follower list contains A; following list for A contains B.
    let v = rpc_json_anon(
        &app,
        r#"{"procedure":"user.followers.list","input":{"username":"fb"}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["total"], 1);
    assert_eq!(v["data"]["users"][0]["username"], "fa");

    let v = rpc_json_anon(
        &app,
        r#"{"procedure":"user.following.list","input":{"username":"fa"}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["total"], 1);
    assert_eq!(v["data"]["users"][0]["username"], "fb");

    // Search filter narrows the list.
    let v = rpc_json_anon(
        &app,
        r#"{"procedure":"user.followers.list","input":{"username":"fb","q":"nomatch"}}"#,
    )
    .await;
    assert_eq!(v["data"]["total"], 0, "{v}");

    // B does not auto-follow A (asymmetric edges).
    let v = rpc_json(
        &app,
        &b_cookie,
        r#"{"procedure":"user.getPublicProfile","input":{"username":"fa"}}"#,
    )
    .await;
    assert_eq!(v["data"]["viewer_is_following"], false, "{v}");

    // Unfollow is idempotent and clears the flag.
    let v = rpc_json(
        &app,
        &a_cookie,
        r#"{"procedure":"user.unfollow","input":{"username":"fb"}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["follower_count"], 0);
    assert_eq!(v["data"]["viewer_is_following"], false);
    let v = rpc_json(
        &app,
        &a_cookie,
        r#"{"procedure":"user.unfollow","input":{"username":"fb"}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");

    // Unknown target → user.not_found.
    let v = rpc_json(
        &app,
        &a_cookie,
        r#"{"procedure":"user.follow","input":{"username":"ghost"}}"#,
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "user.not_found", "{v}");
}
