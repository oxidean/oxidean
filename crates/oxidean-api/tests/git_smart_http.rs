//! GIT-02: Git Smart HTTP auth / ACL / status codes (08-04 tracer + 08-06 expansion stubs).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
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

fn basic_header(user: &str, password: &str) -> String {
    // Manual base64 so tests don't need the base64 crate.
    let raw = format!("{user}:{password}");
    format!("Basic {}", encode_b64(raw.as_bytes()))
}

fn encode_b64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let mut n = (chunk[0] as u32) << 16;
        if chunk.len() > 1 {
            n |= (chunk[1] as u32) << 8;
        }
        if chunk.len() > 2 {
            n |= chunk[2] as u32;
        }
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

async fn create_public_repo(app: &axum::Router, db: &Database, cookie: &str, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"hello","visibility":"public","description":""}}"#,
            cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let _ = create.into_body().collect().await;
}

fn info_refs_uri(owner: &str, repo: &str) -> String {
    format!("/{owner}/{repo}.git/info/refs?service=git-upload-pack")
}

/// Public repo: anonymous `info/refs?service=git-upload-pack` allowed.
#[tokio::test]
async fn git_smart_public_anon_upload_pack_info_refs_ok() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("smart_anon.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "owner@ex.com", "owner1").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    create_public_repo(&app, &db, &cookie, user_id).await;

    let req = Request::builder()
        .method("GET")
        .uri(info_refs_uri("owner1", "hello"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "public anon upload-pack info/refs must succeed"
    );
    let ct = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("git-upload-pack"),
        "expected git-upload-pack content-type, got {ct}"
    );
}

/// Private repo: anonymous → 401 + WWW-Authenticate (D-21).
#[tokio::test]
async fn git_smart_private_anon_401_www_authenticate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("smart_priv.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "priv@ex.com", "privown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let _ = create.into_body().collect().await;

    let req = Request::builder()
        .method("GET")
        .uri(info_refs_uri("privown", "secret"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "private anon must 401 (D-21)"
    );
    let www = res
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        www.contains("Basic") && www.contains("Oxidean Git"),
        "WWW-Authenticate — {www}"
    );
}

/// Basic auth with account password (not PAT) → 401 + PAT hint (D-11).
#[tokio::test]
async fn git_smart_basic_account_password_rejected_401() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("smart_pw.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "pw@ex.com", "pwuser").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    create_public_repo(&app, &db, &cookie, user_id).await;

    let req = Request::builder()
        .method("GET")
        .uri(info_refs_uri("pwuser", "hello"))
        .header(header::AUTHORIZATION, basic_header("pwuser", "password1"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let www = res
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        www.contains("Basic") && www.contains("Oxidean Git"),
        "WWW-Authenticate — {www}"
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    assert!(
        body.to_lowercase().contains("personal access token"),
        "body must hint PAT — {body}"
    );
}

/// Session cookie alone is treated as anonymous (D-12) — never authenticates Smart HTTP.
#[tokio::test]
async fn git_smart_session_cookie_ignored_as_anon() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("smart_cookie.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "ck@ex.com", "ckuser").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    create_public_repo(&app, &db, &cookie, user_id).await;

    // Cookie present but no Basic — public still OK (anon path); cookie must not be required.
    let req = Request::builder()
        .method("GET")
        .uri(info_refs_uri("ckuser", "hello"))
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "cookie alone on public = anon success"
    );

    // Private repo + session cookie only (no Basic) must still 401 — cookie must not elevate.
    let create_priv = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_priv.status(), StatusCode::OK);
    let _ = create_priv.into_body().collect().await;

    let req_priv = Request::builder()
        .method("GET")
        .uri(info_refs_uri("ckuser", "secret"))
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let res_priv = app.clone().oneshot(req_priv).await.unwrap();
    assert_eq!(
        res_priv.status(),
        StatusCode::UNAUTHORIZED,
        "private + session cookie alone must 401 (D-12)"
    );
    let www_priv = res_priv
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        www_priv.contains("Basic") && www_priv.contains("Oxidean Git"),
        "WWW-Authenticate — {www_priv}"
    );

    // Mint PAT then prove cookie alone does not substitute for Basic on a path that needs auth:
    // use password rejection path — cookie + wrong secret still PAT-hint (not session elevate).
    let req2 = Request::builder()
        .method("GET")
        .uri(info_refs_uri("ckuser", "hello"))
        .header(header::COOKIE, &cookie)
        .header(header::AUTHORIZATION, basic_header("ckuser", "password1"))
        .body(Body::empty())
        .unwrap();
    let res2 = app.oneshot(req2).await.unwrap();
    assert_eq!(
        res2.status(),
        StatusCode::UNAUTHORIZED,
        "session cookie must not make account password succeed"
    );
}

/// Fine-grained contents:read cannot receive-pack → 403 (D-23).
#[tokio::test]
async fn git_smart_insufficient_scope_403() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("smart_scope.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "scope@ex.com", "scopeown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    create_public_repo(&app, &db, &cookie, user_id).await;

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createFineGrained","input":{"name":"read-only","repo_access":"all","contents":"read"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v["data"]["token"].as_str().expect("token");

    let req = Request::builder()
        .method("GET")
        .uri("/scopeown/hello.git/info/refs?service=git-receive-pack")
        .header(header::AUTHORIZATION, basic_header("scopeown", token))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "contents:read must not receive-pack — got {}",
        res.status()
    );
}

/// Failed-auth over limit → 429 + Retry-After (D-26).
#[tokio::test]
async fn git_smart_failed_auth_rate_limit_429_retry_after() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("smart_rl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "rl@ex.com", "rluser").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    create_public_repo(&app, &db, &cookie, user_id).await;

    // 20 failed Basic attempts from the same IP → next is 429.
    // Use username alias `git` so only the IP bucket fills (user limit is 10).
    for i in 0..20 {
        let req = Request::builder()
            .method("GET")
            .uri(info_refs_uri("rluser", "hello"))
            .header(header::AUTHORIZATION, basic_header("git", "password1"))
            .header("x-forwarded-for", "198.51.100.9")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "attempt {i} should still be 401"
        );
    }

    let limited = Request::builder()
        .method("GET")
        .uri(info_refs_uri("rluser", "hello"))
        .header(header::AUTHORIZATION, basic_header("git", "password1"))
        .header("x-forwarded-for", "198.51.100.9")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(limited).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "21st failure from same IP must 429"
    );
    let retry = res
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        !retry.is_empty() && retry.parse::<u64>().unwrap_or(0) > 0,
        "Retry-After seconds required — got {retry:?}"
    );
}

/// Unverified owner push (receive-pack) denied (D-24); fetch still allowed (Open Q2).
#[tokio::test]
async fn git_smart_unverified_push_denied() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("smart_unv.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "unv@ex.com", "unvown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    create_public_repo(&app, &db, &cookie, user_id).await;

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"pre-unverify","scopes":["repo"]}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v["data"]["token"].as_str().expect("token").to_string();

    db.clear_email_verified_at(user_id)
        .await
        .expect("clear verify");

    // Fetch still OK for unverified + valid PAT.
    let fetch = Request::builder()
        .method("GET")
        .uri(info_refs_uri("unvown", "hello"))
        .header(header::AUTHORIZATION, basic_header("unvown", &token))
        .body(Body::empty())
        .unwrap();
    let fetch_res = app.clone().oneshot(fetch).await.unwrap();
    assert_eq!(
        fetch_res.status(),
        StatusCode::OK,
        "unverified may still fetch with PAT"
    );

    let push = Request::builder()
        .method("GET")
        .uri("/unvown/hello.git/info/refs?service=git-receive-pack")
        .header(header::AUTHORIZATION, basic_header("unvown", &token))
        .body(Body::empty())
        .unwrap();
    let push_res = app.oneshot(push).await.unwrap();
    assert_ne!(
        push_res.status(),
        StatusCode::OK,
        "unverified must not receive-pack"
    );
    let push_bytes = push_res.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&push_bytes);
    assert!(
        body.contains("auth.email_unverified") || body.contains("email"),
        "must surface email_unverified semantics — {body}"
    );
}

/// Valid classic PAT authenticates public upload-pack info/refs (tracer fetch path).
#[tokio::test]
async fn git_smart_pat_push_fetch_happy_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("smart_pat.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "patgit@ex.com", "patgit").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    create_public_repo(&app, &db, &cookie, user_id).await;

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"cli","scopes":["repo"]}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v["data"]["token"].as_str().expect("token");
    let pat_id = v["data"]["item"]["id"]
        .as_str()
        .expect("pat id")
        .to_string();

    let req = Request::builder()
        .method("GET")
        .uri(info_refs_uri("patgit", "hello"))
        .header(header::AUTHORIZATION, basic_header("patgit", token))
        .header("x-forwarded-for", "203.0.113.50")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "valid PAT must allow fetch info/refs"
    );
    let ct = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("git-upload-pack"),
        "expected upload-pack advertisement, got {ct}"
    );

    // D-09: successful auth updates last_used_at / last_used_ip.
    let list = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.list","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_bytes = list.into_body().collect().await.unwrap().to_bytes();
    let list_v: serde_json::Value = serde_json::from_slice(&list_bytes).unwrap();
    let item = list_v["data"]
        .as_array()
        .expect("list")
        .iter()
        .find(|p| p["id"] == pat_id)
        .expect("pat in list");
    assert!(
        item["last_used_at"].as_str().is_some(),
        "last_used_at must be set after successful auth — {item}"
    );
    assert_eq!(
        item["last_used_ip"].as_str(),
        Some("203.0.113.50"),
        "last_used_ip from X-Forwarded-For — {item}"
    );

    // Classic repo scope can advertise receive-pack (push) when verified (D-20).
    let push_refs = Request::builder()
        .method("GET")
        .uri("/patgit/hello.git/info/refs?service=git-receive-pack")
        .header(header::AUTHORIZATION, basic_header("patgit", token))
        .body(Body::empty())
        .unwrap();
    let push_res = app.oneshot(push_refs).await.unwrap();
    assert_eq!(
        push_res.status(),
        StatusCode::OK,
        "classic repo PAT must allow receive-pack info/refs"
    );
    let push_ct = push_res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        push_ct.contains("git-receive-pack"),
        "expected receive-pack advertisement, got {push_ct}"
    );
}

/// ORG-04: classic PAT push as collaborator (not owner) with repo scope + Write.
#[tokio::test]
async fn git_smart_collaborator_classic_pat_push() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("smart_collab_push.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "cowrite@ex.com", "cowrite").await;
    let owner_id = owner_v["data"]["id"].as_str().unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify owner");
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"team","visibility":"private","description":""}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let _ = create.into_body().collect().await;

    let (collab_cookie, collab_v) = signup_and_login(&app, "cpush@ex.com", "cpush1").await;
    let collab_id = collab_v["data"]["id"].as_str().unwrap();
    db.set_email_verified_at(collab_id, &now)
        .await
        .expect("verify collab");

    let add = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"cowrite","name":"team","username":"cpush1","permission":"write"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(add.status(), StatusCode::OK);
    let add_bytes = add.into_body().collect().await.unwrap().to_bytes();
    let add_v: serde_json::Value = serde_json::from_slice(&add_bytes).unwrap();
    assert_eq!(add_v["ok"], true, "grant write collab — {add_v}");

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"collab-cli","scopes":["repo"]}}"#,
            &collab_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let pat_bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    let token = pat_v["data"]["token"].as_str().expect("token");

    let push_refs = Request::builder()
        .method("GET")
        .uri("/cowrite/team.git/info/refs?service=git-receive-pack")
        .header(header::AUTHORIZATION, basic_header("cpush1", token))
        .body(Body::empty())
        .unwrap();
    let push_res = app.oneshot(push_refs).await.unwrap();
    assert_eq!(
        push_res.status(),
        StatusCode::OK,
        "write collaborator classic PAT must allow receive-pack (ORG-04)"
    );
    let push_ct = push_res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        push_ct.contains("git-receive-pack"),
        "expected receive-pack advertisement, got {push_ct}"
    );
}

/// ORG-04: Read collaborator classic PAT cannot receive-pack.
#[tokio::test]
async fn git_smart_collaborator_read_cannot_push() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("smart_collab_read.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "coread@ex.com", "coread").await;
    let owner_id = owner_v["data"]["id"].as_str().unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify owner");
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"locked","visibility":"private","description":""}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let _ = create.into_body().collect().await;

    let (collab_cookie, collab_v) = signup_and_login(&app, "creadp@ex.com", "creadp1").await;
    let collab_id = collab_v["data"]["id"].as_str().unwrap();
    db.set_email_verified_at(collab_id, &now)
        .await
        .expect("verify collab");

    let add = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"coread","name":"locked","username":"creadp1","permission":"read"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(add.status(), StatusCode::OK);
    let _ = add.into_body().collect().await;

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"read-cli","scopes":["repo"]}}"#,
            &collab_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let pat_bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    let token = pat_v["data"]["token"].as_str().expect("token");

    // Read collaborator may fetch private repo.
    let fetch = Request::builder()
        .method("GET")
        .uri(info_refs_uri("coread", "locked"))
        .header(header::AUTHORIZATION, basic_header("creadp1", token))
        .body(Body::empty())
        .unwrap();
    let fetch_res = app.clone().oneshot(fetch).await.unwrap();
    assert_eq!(
        fetch_res.status(),
        StatusCode::OK,
        "read collaborator may upload-pack private repo"
    );

    let push = Request::builder()
        .method("GET")
        .uri("/coread/locked.git/info/refs?service=git-receive-pack")
        .header(header::AUTHORIZATION, basic_header("creadp1", token))
        .body(Body::empty())
        .unwrap();
    let push_res = app.oneshot(push).await.unwrap();
    assert_ne!(
        push_res.status(),
        StatusCode::OK,
        "read collaborator must not receive-pack"
    );
}

/// ORG-04 / D-21 / T-10-01: private non-grantee Smart HTTP → 401.
#[tokio::test]
async fn git_smart_private_non_grantee_401() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("smart_nongrantee.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "ngown@ex.com", "ngown").await;
    let owner_id = owner_v["data"]["id"].as_str().unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify owner");
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","description":""}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let _ = create.into_body().collect().await;

    let (stranger_cookie, stranger_v) = signup_and_login(&app, "ngstr@ex.com", "ngstr1").await;
    let stranger_id = stranger_v["data"]["id"].as_str().unwrap();
    db.set_email_verified_at(stranger_id, &now)
        .await
        .expect("verify stranger");

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"stranger-cli","scopes":["repo"]}}"#,
            &stranger_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let pat_bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    let token = pat_v["data"]["token"].as_str().expect("token");

    let req = Request::builder()
        .method("GET")
        .uri(info_refs_uri("ngown", "secret"))
        .header(header::AUTHORIZATION, basic_header("ngstr1", token))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "private non-grantee must 401 (D-21 / T-10-01)"
    );
    let www = res
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        www.contains("Basic") && www.contains("Oxidean Git"),
        "WWW-Authenticate — {www}"
    );
}

/// A4 / ORG-04: FG All covers org Owner/Admin repos (not personal-owner_id equality).
#[tokio::test]
async fn git_smart_fg_all_org_owner_receive_pack() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("smart_fg_all_org.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "fgorg@ex.com", "fgorgown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");

    let org = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"fg-all-org"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(org.status(), StatusCode::OK);
    let _ = org.into_body().collect().await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"svc","visibility":"private","owner":"fg-all-org"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    assert_eq!(create_v["ok"], true, "org repo create — {create_v}");

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createFineGrained","input":{"name":"org-all","repo_access":"all","contents":"write"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let pat_bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    let token = pat_v["data"]["token"].as_str().expect("token");

    let push = Request::builder()
        .method("GET")
        .uri("/fg-all-org/svc.git/info/refs?service=git-receive-pack")
        .header(header::AUTHORIZATION, basic_header("fgorgown", token))
        .body(Body::empty())
        .unwrap();
    let push_res = app.oneshot(push).await.unwrap();
    assert_eq!(
        push_res.status(),
        StatusCode::OK,
        "FG All + org Owner must allow receive-pack (A4) — got {}",
        push_res.status()
    );
}

/// Collaborator FG Selected + contents write can receive-pack (PAT ∩ ACL).
#[tokio::test]
async fn git_smart_collaborator_fg_selected_push() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("smart_fg_collab_sel.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "fgsown@ex.com", "fgsown").await;
    let owner_id = owner_v["data"]["id"].as_str().unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify owner");
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"sel","visibility":"private","description":""}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    let repo_id = create_v["data"]["id"]
        .as_str()
        .expect("repo id")
        .to_string();

    let (collab_cookie, collab_v) = signup_and_login(&app, "fgssel@ex.com", "fgssel1").await;
    let collab_id = collab_v["data"]["id"].as_str().unwrap();
    db.set_email_verified_at(collab_id, &now)
        .await
        .expect("verify collab");

    let add = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"fgsown","name":"sel","username":"fgssel1","permission":"write"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(add.status(), StatusCode::OK);
    let _ = add.into_body().collect().await;

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            &format!(
                r#"{{"procedure":"pat.createFineGrained","input":{{"name":"sel-push","repo_access":"selected","contents":"write","repository_ids":["{repo_id}"]}}}}"#
            ),
            &collab_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        create_pat.status(),
        StatusCode::OK,
        "mint Selected FG for collaborator repo"
    );
    let pat_bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    assert_eq!(pat_v["ok"], true, "mint Selected — {pat_v}");
    let token = pat_v["data"]["token"].as_str().expect("token");

    let push = Request::builder()
        .method("GET")
        .uri("/fgsown/sel.git/info/refs?service=git-receive-pack")
        .header(header::AUTHORIZATION, basic_header("fgssel1", token))
        .body(Body::empty())
        .unwrap();
    let push_res = app.oneshot(push).await.unwrap();
    assert_eq!(
        push_res.status(),
        StatusCode::OK,
        "collaborator FG Selected write must receive-pack"
    );
}
