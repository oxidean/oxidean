//! ORG-01 / D-ORG-03: `user.lookup` live username autocomplete with anti-enumeration.

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

async fn rpc_json(app: &axum::Router, body: &str, cookie: &str) -> (StatusCode, serde_json::Value) {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, v)
}

/// Prefix shorter than 2 chars returns an empty list (anti-enumeration).
#[tokio::test]
async fn user_lookup_short_prefix_returns_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("lookup_short.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "caller@ex.com", "caller1").await;
    verify_user(&db, login_v["data"]["id"].as_str().expect("id")).await;
    let (_, _) = signup_and_login(&app, "alice@ex.com", "alice").await;

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"a"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "user.lookup short prefix — {v}");
    assert_eq!(v["ok"], true, "{v}");
    let users = v["data"]["users"].as_array().expect("users array");
    assert!(users.is_empty(), "prefix < 2 must be empty — {v}");
}

/// Matching prefix returns ≤10 hits with username/display/avatar only — never email (T-10-03).
#[tokio::test]
async fn user_lookup_prefix_returns_public_fields_without_email() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("lookup_prefix.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "looker@ex.com", "looker1").await;
    verify_user(&db, login_v["data"]["id"].as_str().expect("id")).await;

    for i in 0..12 {
        let uname = format!("alpha{i:02}");
        let email = format!("{uname}@ex.com");
        let (_, _) = signup_and_login(&app, &email, &uname).await;
    }

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"al"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "user.lookup prefix — {v}");
    assert_eq!(v["ok"], true, "{v}");
    let users = v["data"]["users"].as_array().expect("users array");
    assert!(!users.is_empty(), "prefix al must match alpha* — {v}");
    assert!(
        users.len() <= 10,
        "must cap at 10 — got {} — {v}",
        users.len()
    );

    for hit in users {
        assert!(hit.get("username").and_then(|x| x.as_str()).is_some());
        assert!(hit.get("display_name").and_then(|x| x.as_str()).is_some());
        // avatar_url may be null/absent; email must never appear
        assert!(
            hit.get("email").is_none(),
            "lookup hit must not include email — {hit}"
        );
        let s = hit.to_string();
        assert!(
            !s.contains('@'),
            "lookup payload must not leak email-shaped values — {hit}"
        );
    }
}

/// Email-shaped queries return empty (no @ search) — T-10-03 / D-ORG-03.
#[tokio::test]
async fn user_lookup_email_shaped_prefix_returns_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("lookup_email.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "caller2@ex.com", "caller2").await;
    verify_user(&db, login_v["data"]["id"].as_str().expect("id")).await;
    let (_, _) = signup_and_login(&app, "secret@ex.com", "secretuser").await;

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"secret@ex.com"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "user.lookup email-shaped — {v}");
    assert_eq!(v["ok"], true, "{v}");
    let users = v["data"]["users"].as_array().expect("users array");
    assert!(
        users.is_empty(),
        "email-shaped prefix must not search — {v}"
    );
}

/// Empty / whitespace-only prefix returns empty (anti-enumeration).
#[tokio::test]
async fn user_lookup_empty_prefix_returns_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("lookup_empty.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "empty@ex.com", "empty1").await;
    verify_user(&db, login_v["data"]["id"].as_str().expect("id")).await;
    let (_, _) = signup_and_login(&app, "zeta@ex.com", "zeta").await;

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"  "}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["ok"], true, "{v}");
    let users = v["data"]["users"].as_array().expect("users");
    assert!(users.is_empty(), "whitespace prefix must be empty — {v}");
}

/// Prefix match is case-insensitive on username.
#[tokio::test]
async fn user_lookup_prefix_is_case_insensitive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("lookup_case.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "case@ex.com", "casecaller").await;
    verify_user(&db, login_v["data"]["id"].as_str().expect("id")).await;
    let (_, _) = signup_and_login(&app, "mix@ex.com", "CamelCase").await;

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"cam"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["ok"], true, "{v}");
    let users = v["data"]["users"].as_array().expect("users");
    assert_eq!(users.len(), 1, "case-insensitive match — {v}");
    assert_eq!(users[0]["username"], "CamelCase");
    assert!(users[0].get("email").is_none());
}

/// Org context excludes existing members; unauthorized context returns empty.
#[tokio::test]
async fn user_lookup_org_context_excludes_members() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("lookup_org_ctx.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "octx@ex.com", "octxowner").await;
    verify_user(&db, owner_v["data"]["id"].as_str().expect("id")).await;
    let (_, create_org) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"lookup-org","display_name":"Lookup"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(create_org["ok"], true, "{create_org}");

    let (_, mem_v) = signup_and_login(&app, "mem@ex.com", "octxmember").await;
    verify_user(&db, mem_v["data"]["id"].as_str().expect("id")).await;
    let (_, add_v) = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"lookup-org","username":"octxmember","role":"member"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(add_v["ok"], true, "{add_v}");

    let (_, cand_v) = signup_and_login(&app, "cand@ex.com", "octxcand").await;
    verify_user(&db, cand_v["data"]["id"].as_str().expect("id")).await;

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"octx","context":{"kind":"org","slug":"lookup-org"}}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let users = v["data"]["users"].as_array().expect("users");
    let names: Vec<&str> = users
        .iter()
        .filter_map(|u| u["username"].as_str())
        .collect();
    assert!(names.contains(&"octxcand"), "{v}");
    assert!(!names.contains(&"octxmember"), "members excluded — {v}");
    assert!(!names.contains(&"octxowner"), "owner is member — {v}");
    assert!(users.iter().all(|u| u.get("email").is_none()));

    // Unauthorized (non-admin member) → empty.
    let login_body = r#"{"procedure":"auth.login","input":{"identifier":"mem@ex.com","password":"password1","remember_me":false}}"#;
    let login = app.clone().oneshot(rpc_req(login_body)).await.unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let mem_cookie = session_cookie_from_response(&login);
    let _ = login.into_body().collect().await;

    let (status, empty_v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"octx","context":{"kind":"org","slug":"lookup-org"}}}"#,
        &mem_cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{empty_v}");
    assert!(
        empty_v["data"]["users"]
            .as_array()
            .expect("users")
            .is_empty(),
        "non-admin org context must be empty — {empty_v}"
    );
}

/// Repo context excludes collaborators + owner; ranks org members first on org repos.
#[tokio::test]
async fn user_lookup_repo_context_excludes_and_ranks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("lookup_repo_ctx.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "rctx@ex.com", "rctxowner").await;
    verify_user(&db, owner_v["data"]["id"].as_str().expect("id")).await;
    let (_, create_org) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"rctx-org","display_name":"Rctx"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(create_org["ok"], true, "{create_org}");
    let (_, create_repo) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"owner":"rctx-org","name":"app","visibility":"private"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(create_repo["ok"], true, "{create_repo}");

    let (_, mem_v) = signup_and_login(&app, "rmem@ex.com", "rctxmem").await;
    verify_user(&db, mem_v["data"]["id"].as_str().expect("id")).await;
    let (_, add_mem) = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"rctx-org","username":"rctxmem","role":"member"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(add_mem["ok"], true, "{add_mem}");

    let (_, col_v) = signup_and_login(&app, "rcol@ex.com", "rctxcol").await;
    verify_user(&db, col_v["data"]["id"].as_str().expect("id")).await;
    let (_, add_col) = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"rctx-org","name":"app","username":"rctxcol","permission":"write"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(add_col["ok"], true, "{add_col}");

    let (_, out_v) = signup_and_login(&app, "rout@ex.com", "rctxout").await;
    verify_user(&db, out_v["data"]["id"].as_str().expect("id")).await;

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"rctx","context":{"kind":"repo","owner":"rctx-org","name":"app"}}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let users = v["data"]["users"].as_array().expect("users");
    let names: Vec<&str> = users
        .iter()
        .filter_map(|u| u["username"].as_str())
        .collect();
    assert!(
        names.contains(&"rctxmem"),
        "org member ranked/included — {v}"
    );
    assert!(names.contains(&"rctxout"), "global fill — {v}");
    assert!(!names.contains(&"rctxcol"), "collaborator excluded — {v}");
    // Org member should appear before outsider.
    let mem_idx = names.iter().position(|n| *n == "rctxmem").unwrap();
    let out_idx = names.iter().position(|n| *n == "rctxout").unwrap();
    assert!(mem_idx < out_idx, "org members ranked first — {v}");
    assert!(users.iter().all(|u| u.get("email").is_none()));

    // Unauthorized outsider → empty.
    let login_body = r#"{"procedure":"auth.login","input":{"identifier":"rout@ex.com","password":"password1","remember_me":false}}"#;
    let login = app.clone().oneshot(rpc_req(login_body)).await.unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let out_cookie = session_cookie_from_response(&login);
    let _ = login.into_body().collect().await;

    let (status, empty_v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"rctx","context":{"kind":"repo","owner":"rctx-org","name":"app"}}}"#,
        &out_cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{empty_v}");
    assert!(
        empty_v["data"]["users"]
            .as_array()
            .expect("users")
            .is_empty(),
        "unauthorized repo context empty — {empty_v}"
    );
}
