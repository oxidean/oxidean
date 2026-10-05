//! API-05: Atom feeds — XML validity, content type, entries, visibility gating.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;
use uuid::Uuid;

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

fn get_req(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn get_req_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
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
        // No template pickers → empty seed → no web-flow signing-key path (flaky
        // under parallel tests on a cold checkout).
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":""}}}}"#
    );
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(&body, cookie))
        .await
        .unwrap();
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

async fn feed_get(app: &axum::Router, uri: &str) -> (StatusCode, String, String) {
    let res = app.clone().oneshot(get_req(uri)).await.unwrap();
    let status = res.status();
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        content_type,
        String::from_utf8_lossy(&bytes).to_string(),
    )
}

async fn feed_get_with_cookie(
    app: &axum::Router,
    uri: &str,
    cookie: &str,
) -> (StatusCode, String, String) {
    let res = app
        .clone()
        .oneshot(get_req_with_cookie(uri, cookie))
        .await
        .unwrap();
    let status = res.status();
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        content_type,
        String::from_utf8_lossy(&bytes).to_string(),
    )
}

/// Minimal well-formedness check for generated feeds (no xml dep):
/// one root element, balanced non-empty tags, `&` only starts an entity.
fn assert_well_formed_xml(xml: &str) {
    let mut rest = xml.trim_start();
    if let Some(after) = rest.strip_prefix("<?xml") {
        let end = after.find("?>").expect("xml prolog must close");
        rest = &after[end + 2..];
    }
    let mut stack: Vec<String> = Vec::new();
    let mut roots = 0usize;
    let mut rest = rest.trim_start();
    while !rest.is_empty() {
        if let Some(tag) = rest.strip_prefix('<') {
            if let Some(decl) = tag.strip_prefix('!') {
                let end = decl.find('>').expect("<! must close");
                rest = &decl[end + 1..];
                continue;
            }
            if let Some(pi) = tag.strip_prefix('?') {
                let end = pi.find("?>").expect("<? must close");
                rest = &pi[end + 2..];
                continue;
            }
            let end = tag.find('>').expect("tag must close");
            let inner = tag[..end].trim();
            assert!(!inner.is_empty(), "empty <> tag");
            if let Some(name) = inner.strip_prefix('/') {
                let top = stack.pop().unwrap_or_else(|| panic!("stray </{name}>"));
                assert_eq!(top, name.trim(), "mismatched close tag");
            } else if !inner.ends_with('/') {
                let name = inner
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap();
                if stack.is_empty() {
                    roots += 1;
                }
                stack.push(name.to_string());
            }
            rest = tag[end + 1..].trim_start();
        } else {
            let next = rest.find('<').unwrap_or(rest.len());
            let text = &rest[..next];
            let mut t = text;
            while let Some(pos) = t.find('&') {
                let after = &t[pos..];
                assert!(
                    after.starts_with("&amp;")
                        || after.starts_with("&lt;")
                        || after.starts_with("&gt;")
                        || after.starts_with("&quot;")
                        || after.starts_with("&apos;")
                        || after.starts_with("&#"),
                    "bare '&' in text node: {xml}"
                );
                t = &t[pos + 1..];
            }
            rest = rest[next..].trim_start();
        }
    }
    assert!(stack.is_empty(), "unclosed elements: {stack:?}");
    assert_eq!(roots, 1, "exactly one root element expected");
}

#[tokio::test]
async fn repo_activity_feed_serves_atom_and_gates_private() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("feed-activity.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    std::fs::create_dir_all(&repos_dir).unwrap();
    let app = test_app(db.clone(), repos_dir).await;

    let (owner_cookie, owner_login) = signup_and_login(&app, "feedowner@ex.com", "feedown").await;
    let owner_id = owner_login["data"]["id"].as_str().unwrap();
    verify_user(&db, owner_id).await;
    create_repo(&app, &owner_cookie, "pubrepo", "public").await;
    create_repo(&app, &owner_cookie, "privrepo", "private").await;

    let repo = db
        .find_repository_by_owner_name(owner_id, "pubrepo")
        .await
        .expect("find")
        .expect("pubrepo");
    let event_id = Uuid::new_v4().to_string();
    db.insert_repo_activity(
        &event_id,
        &repo.id,
        owner_id,
        "push",
        "refs/heads/main",
        "0000000000000000000000000000000000000000",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        2,
        Some("feat: add <widget> & \"docs\""),
        None,
    )
    .await
    .expect("insert activity");

    // Anonymous read of a public repo feed: Atom XML, one entry, escaped text.
    let (status, ctype, body) = feed_get(&app, "/api/repos/feedown/pubrepo/activity.atom").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        ctype.starts_with("application/atom+xml"),
        "content-type: {ctype}"
    );
    assert_well_formed_xml(&body);
    assert!(
        body.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">"),
        "{body}"
    );
    assert!(
        body.contains("<link rel=\"self\" type=\"application/atom+xml\""),
        "{body}"
    );
    assert!(body.contains("<entry>"), "{body}");
    assert!(
        body.contains(&format!("<id>urn:uuid:{event_id}</id>")),
        "{body}"
    );
    assert!(
        body.contains("<title>feedown pushed 2 commits to main</title>"),
        "{body}"
    );
    assert!(
        body.contains("feat: add &lt;widget&gt; &amp; &quot;docs&quot;"),
        "content must be xml-escaped — {body}"
    );
    assert!(
        !body.contains("<widget>"),
        "unescaped markup leaked: {body}"
    );
    assert!(body.contains("/feedown/pubrepo/commit/"), "{body}");

    // Private repo feed: identical 404 for anonymous (anti-enumeration).
    let (status, _c, body) = feed_get(&app, "/api/repos/feedown/privrepo/activity.atom").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(body.contains("repo.not_found"), "{body}");

    // Owner (Read+) can subscribe to the private feed.
    let (status, ctype, body) = feed_get_with_cookie(
        &app,
        "/api/repos/feedown/privrepo/activity.atom",
        &owner_cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(ctype.starts_with("application/atom+xml"), "{ctype}");
    assert_well_formed_xml(&body);
    assert!(
        !body.contains("<entry>"),
        "private feed should be empty — {body}"
    );

    // Unknown repo → same 404 shape.
    let (status, _c, _b) = feed_get(&app, "/api/repos/feedown/nope/activity.atom").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn repo_releases_feed_lists_published_hides_drafts_anon() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("feed-releases.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    std::fs::create_dir_all(&repos_dir).unwrap();
    let app = test_app(db.clone(), repos_dir).await;

    let (owner_cookie, owner_login) = signup_and_login(&app, "relown@ex.com", "relown").await;
    let owner_id = owner_login["data"]["id"].as_str().unwrap();
    verify_user(&db, owner_id).await;
    create_repo(&app, &owner_cookie, "shipit", "public").await;

    let repo = db
        .find_repository_by_owner_name(owner_id, "shipit")
        .await
        .expect("find")
        .expect("shipit");
    db.insert_release(
        &Uuid::new_v4().to_string(),
        &repo.id,
        "v1.0.0",
        "First <Release>",
        "notes & <details>",
        false,
        false,
        owner_id,
    )
    .await
    .expect("insert release");
    db.insert_release(
        &Uuid::new_v4().to_string(),
        &repo.id,
        "v9.9.9",
        "Secret Draft",
        "unpublished",
        true,
        false,
        owner_id,
    )
    .await
    .expect("insert draft");

    // Anonymous: published only, escaped, link points at the release page.
    let (status, ctype, body) = feed_get(&app, "/api/repos/relown/shipit/releases.atom").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(ctype.starts_with("application/atom+xml"), "{ctype}");
    assert_well_formed_xml(&body);
    assert!(body.contains("First &lt;Release&gt;"), "{body}");
    assert!(body.contains("/relown/shipit/releases/v1.0.0"), "{body}");
    assert!(body.contains("notes &amp; &lt;details&gt;"), "{body}");
    assert!(
        !body.contains("Secret Draft") && !body.contains("v9.9.9"),
        "draft leaked to anonymous feed — {body}"
    );

    // Owner (Write) sees the draft entry too — matches release.list semantics.
    let (status, _c, body) = feed_get_with_cookie(
        &app,
        "/api/repos/relown/shipit/releases.atom",
        &owner_cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("Secret Draft"), "{body}");

    // Private repo feed is 404 anonymously.
    create_repo(&app, &owner_cookie, "locked", "private").await;
    let (status, _c, _b) = feed_get(&app, "/api/repos/relown/locked/releases.atom").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn user_activity_feed_is_public_only() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("feed-user.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    std::fs::create_dir_all(&repos_dir).unwrap();
    let app = test_app(db.clone(), repos_dir).await;

    let (owner_cookie, owner_login) = signup_and_login(&app, "uown@ex.com", "uown").await;
    let owner_id = owner_login["data"]["id"].as_str().unwrap();
    verify_user(&db, owner_id).await;
    create_repo(&app, &owner_cookie, "openrepo", "public").await;
    create_repo(&app, &owner_cookie, "secrepo", "private").await;

    let pub_repo = db
        .find_repository_by_owner_name(owner_id, "openrepo")
        .await
        .expect("find")
        .expect("openrepo");
    let priv_repo = db
        .find_repository_by_owner_name(owner_id, "secrepo")
        .await
        .expect("find")
        .expect("secrepo");
    db.insert_repo_activity(
        &Uuid::new_v4().to_string(),
        &pub_repo.id,
        owner_id,
        "push",
        "refs/heads/main",
        "0000000000000000000000000000000000000000",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        1,
        Some("public work"),
        None,
    )
    .await
    .expect("insert public activity");
    db.insert_repo_activity(
        &Uuid::new_v4().to_string(),
        &priv_repo.id,
        owner_id,
        "push",
        "refs/heads/classified",
        "0000000000000000000000000000000000000000",
        "cccccccccccccccccccccccccccccccccccccccc",
        3,
        Some("covert mission"),
        None,
    )
    .await
    .expect("insert private activity");

    let (status, ctype, body) = feed_get(&app, "/api/users/uown/activity.atom").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(ctype.starts_with("application/atom+xml"), "{ctype}");
    assert_well_formed_xml(&body);
    assert!(
        body.contains("uown pushed 1 commit to main in uown/openrepo"),
        "{body}"
    );
    assert!(body.contains("/uown/openrepo/"), "{body}");

    // Private-repo activity must never leak: no name, branch, or message.
    assert!(
        !body.contains("secrepo"),
        "private repo name leaked — {body}"
    );
    assert!(
        !body.contains("classified"),
        "private branch leaked — {body}"
    );
    assert!(
        !body.contains("covert"),
        "private commit subject leaked — {body}"
    );

    // Unknown user → 404.
    let (status, _c, body) = feed_get(&app, "/api/users/ghost-zz/activity.atom").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(body.contains("user.not_found"), "{body}");
}
