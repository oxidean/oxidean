//! GIT-20: repository archive mode — read-only enforcement + browse/clone stays open.
//!
//! `repo.setArchived` (Admin) flips `repositories.archived`; write paths then fail
//! with `repo.archived` while `repo.get` and git-upload-pack remain available.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::ssh::pack::{authorize_pack, AuthzDecision, PackCommand};
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

async fn create_repo(app: &axum::Router, cookie: &str, name: &str, visibility: &str) {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":""}}}}"#
    );
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(&body, cookie))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

async fn set_archived(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    name: &str,
    archived: bool,
) -> serde_json::Value {
    let body = format!(
        r#"{{"procedure":"repo.setArchived","input":{{"owner":"{owner}","name":"{name}","archived":{archived}}}}}"#
    );
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(&body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn basic_header(user: &str, password: &str) -> String {
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

/// Full read-only sweep: archive → push/issues/pulls/releases/branches denied,
/// browse + fetch stay open; unarchive restores writes.
#[tokio::test]
async fn archived_repo_blocks_writes_keeps_reads() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("archived_mode.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "arc@ex.com", "arcowner").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &cookie, "hello", "public").await;

    // Seed an issue while writable so comment writes can be tested post-archive.
    let issue = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.create","input":{"owner":"arcowner","name":"hello","title":"First","body":"body"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = issue.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "pre-archive issue.create — {v}");

    // Classic PAT for the git transport checks.
    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"cli","scopes":["repo"]}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v["data"]["token"].as_str().expect("token").to_string();

    // Archive — Admin path must succeed and echo the flag.
    let v = set_archived(&app, &cookie, "arcowner", "hello", true).await;
    assert_eq!(v["ok"], true, "repo.setArchived — {v}");
    assert_eq!(v["data"]["archived"], true, "RepoPublic.archived — {v}");

    // Browse stays open (anonymous read of public repo).
    let get = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"repo.get","input":{"owner":"arcowner","name":"hello"}}"#,
        ))
        .await
        .unwrap();
    let bytes = get.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "repo.get while archived — {v}");
    assert_eq!(v["data"]["archived"], true);

    // Fetch/clone stays open — anonymous upload-pack on a public repo.
    let fetch = Request::builder()
        .method("GET")
        .uri("/arcowner/hello.git/info/refs?service=git-upload-pack")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(fetch).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "archived repo must still serve upload-pack (clone/fetch)"
    );

    // Push over Smart HTTP is denied with a clear 403.
    let push_refs = Request::builder()
        .method("GET")
        .uri("/arcowner/hello.git/info/refs?service=git-receive-pack")
        .header(header::AUTHORIZATION, basic_header("arcowner", &token))
        .body(Body::empty())
        .unwrap();
    let push_res = app.clone().oneshot(push_refs).await.unwrap();
    assert_eq!(
        push_res.status(),
        StatusCode::FORBIDDEN,
        "receive-pack must be denied on archived repo"
    );
    let body = push_res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("archived"),
        "push denial should mention archive — {text}"
    );

    // SSH receive-pack authorization denies; upload-pack still allowed.
    let push = authorize_pack(
        &db,
        &repos,
        user_id,
        &PackCommand::ReceivePack {
            owner: "arcowner".into(),
            name: "hello".into(),
        },
    )
    .await;
    match push {
        AuthzDecision::Deny { message } => {
            assert!(
                message.contains("archived"),
                "ssh push deny should mention archive — {message}"
            );
        }
        AuthzDecision::Allow { .. } => panic!("ssh receive-pack must be denied on archived repo"),
    }
    let fetch = authorize_pack(
        &db,
        &repos,
        user_id,
        &PackCommand::UploadPack {
            owner: "arcowner".into(),
            name: "hello".into(),
        },
    )
    .await;
    assert!(
        matches!(fetch, AuthzDecision::Allow { is_push: false, .. }),
        "ssh upload-pack (fetch) must stay open on archived repo"
    );

    // Content writes all fail with repo.archived.
    for (proc, input) in [
        (
            "issue.create",
            r#"{"owner":"arcowner","name":"hello","title":"N","body":"b"}"#,
        ),
        (
            "issue.comments.create",
            r#"{"owner":"arcowner","name":"hello","number":1,"body":"hi"}"#,
        ),
        (
            "pull.create",
            r#"{"owner":"arcowner","name":"hello","title":"P","base_ref":"main","head_ref":"topic"}"#,
        ),
        (
            "release.create",
            r#"{"owner":"arcowner","name":"hello","tag_name":"v1"}"#,
        ),
        (
            "repo.branchCreate",
            r#"{"owner":"arcowner","name":"hello","branch":"topic"}"#,
        ),
        (
            "repo.commitStatus.create",
            r#"{"owner":"arcowner","name":"hello","sha":"0123456789abcdef","context":"ci/test","state":"success","description":"","target_url":null}"#,
        ),
    ] {
        let body = format!(r#"{{"procedure":"{proc}","input":{input}}}"#);
        let res = app
            .clone()
            .oneshot(rpc_req_with_cookie(&body, &cookie))
            .await
            .unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["ok"], false, "{proc} must fail on archived repo — {v}");
        assert_eq!(
            v["error"]["code"], "repo.archived",
            "{proc} must surface repo.archived — {v}"
        );
    }

    // Unarchive restores writes.
    let v = set_archived(&app, &cookie, "arcowner", "hello", false).await;
    assert_eq!(v["ok"], true, "repo.setArchived(false) — {v}");
    assert_eq!(v["data"]["archived"], false);

    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.create","input":{"owner":"arcowner","name":"hello","title":"After","body":"b"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "issue.create after unarchive — {v}");
}

/// Only Admin may toggle the flag — non-admin gets the soft not_found.
#[tokio::test]
async fn set_archived_requires_admin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("archived_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, login_v) = signup_and_login(&app, "own@ex.com", "own1").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &owner_cookie, "hello", "public").await;

    let (other_cookie, other_v) = signup_and_login(&app, "other@ex.com", "other1").await;
    let other_id = other_v["data"]["id"].as_str().expect("id");
    verify_user(&db, other_id).await;

    // Authenticated non-member → soft not_found (same as unauthorized private).
    let v = set_archived(&app, &other_cookie, "own1", "hello", true).await;
    assert_eq!(v["ok"], false, "non-admin setArchived — {v}");
    assert_eq!(v["error"]["code"], "repo.not_found");

    // Anonymous → unauthenticated.
    let res = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"repo.setArchived","input":{"owner":"own1","name":"hello","archived":true}}"#,
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false, "anon setArchived — {v}");
    assert_eq!(v["error"]["code"], "auth.unauthenticated");

    // Owner (Admin capability) succeeds.
    let v = set_archived(&app, &owner_cookie, "own1", "hello", true).await;
    assert_eq!(v["ok"], true, "admin setArchived — {v}");
    assert_eq!(v["data"]["archived"], true);
}
