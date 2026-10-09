//! GIT-05 / D-23–D-25: private non-owner and missing → identical `repo.not_found`;
//! anonymous can read public repos.

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

/// Missing repo and private non-owner must share `repo.not_found` (no existence leak).
#[tokio::test]
async fn repo_private_404_identical_not_found_for_missing_and_private() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_private_404.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    // Owner creates a real private repository.
    let (owner_cookie, owner_v) = signup_and_login(&app, "owner@ex.com", "owner1").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&owner_id, &now)
        .await
        .expect("verify owner");

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"secret-private","visibility":"private","description":""}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        create.status(),
        StatusCode::OK,
        "private create must succeed"
    );
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    assert_eq!(create_v["ok"], true, "private create ok — {create_v}");

    // Stranger session (non-owner).
    let (stranger_cookie, _) = signup_and_login(&app, "stranger@ex.com", "stranger1").await;

    let missing = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.get","input":{"owner":"nobody","name":"missing-repo"}}"#,
            &stranger_cookie,
        ))
        .await
        .unwrap();
    let missing_bytes = missing.into_body().collect().await.unwrap().to_bytes();
    let missing_v: serde_json::Value = serde_json::from_slice(&missing_bytes).unwrap();

    let private = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.get","input":{"owner":"owner1","name":"secret-private"}}"#,
            &stranger_cookie,
        ))
        .await
        .unwrap();
    let private_bytes = private.into_body().collect().await.unwrap().to_bytes();
    let private_v: serde_json::Value = serde_json::from_slice(&private_bytes).unwrap();

    assert_eq!(
        missing_v["error"]["code"], "repo.not_found",
        "missing repo → repo.not_found — {missing_v}"
    );
    assert_eq!(
        private_v["error"]["code"], "repo.not_found",
        "private non-owner → repo.not_found (identical) — {private_v}"
    );
    assert_eq!(
        missing_v["error"]["code"], private_v["error"]["code"],
        "codes must match to avoid existence leak"
    );
    assert_eq!(
        missing_v["error"]["message"], private_v["error"]["message"],
        "messages must match to avoid existence leak"
    );
}

/// Anonymous callers can read public repo metadata (D-24).
#[tokio::test]
async fn repo_private_404_public_anonymous_get_succeeds() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_public_anon.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "pub@ex.com", "pubowner").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&owner_id, &now)
        .await
        .expect("verify");

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"open-src","visibility":"public","description":"hi"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let _ = create.into_body().collect().await;

    // No session cookie — anonymous.
    let get = app
        .oneshot(rpc_req(
            r#"{"procedure":"repo.get","input":{"owner":"pubowner","name":"open-src"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(
        get.status(),
        StatusCode::OK,
        "anonymous public get must be 200"
    );
    let bytes = get.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "anonymous public get ok — {v}");
    assert_eq!(v["data"]["name"], "open-src");
    assert_eq!(v["data"]["visibility"], "public");
    assert_eq!(v["data"]["owner_username"], "pubowner");
}

/// Empty public repo tree returns structured empty (no 500).
#[tokio::test]
async fn repo_private_404_empty_tree_structured() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("repo_empty_tree.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "empty@ex.com", "emptyown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&owner_id, &now)
        .await
        .expect("verify");

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"blank","visibility":"public"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let _ = create.into_body().collect().await;

    let tree = app
        .oneshot(rpc_req(
            r#"{"procedure":"repo.tree","input":{"owner":"emptyown","name":"blank","ref":"main","path":""}}"#,
        ))
        .await
        .unwrap();
    assert_ne!(
        tree.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "empty tree must not 500"
    );
    let bytes = tree.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "empty tree ok — {v}");
    assert_eq!(v["data"]["empty"], true);
    assert_eq!(v["data"]["entries"], serde_json::json!([]));
}

/// ORG-04 / D-ORG-05: org Owner can read private org repo; stranger → soft `repo.not_found`.
/// Owner `repo.get` reports `can_admin: true`.
#[tokio::test]
async fn repo_private_404_org_non_member_soft_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_private_org_acl.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "orgboss@ex.com", "orgboss1").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&owner_id, &now)
        .await
        .expect("verify owner");

    let org = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"acl-org"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(org.status(), StatusCode::OK);
    let org_bytes = org.into_body().collect().await.unwrap().to_bytes();
    let org_v: serde_json::Value = serde_json::from_slice(&org_bytes).unwrap();
    assert_eq!(org_v["ok"], true, "{org_v}");

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"private-widget","visibility":"private","owner":"acl-org"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK, "Owner create under org");
    let _ = create.into_body().collect().await;

    // Creator Owner can read + can_admin.
    let owner_get = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.get","input":{"owner":"acl-org","name":"private-widget"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    let owner_bytes = owner_get.into_body().collect().await.unwrap().to_bytes();
    let owner_get_v: serde_json::Value = serde_json::from_slice(&owner_bytes).unwrap();
    assert_eq!(
        owner_get_v["ok"], true,
        "org Owner must read private — {owner_get_v}"
    );
    assert_eq!(
        owner_get_v["data"]["can_admin"], true,
        "Owner can_admin — {owner_get_v}"
    );
    assert_eq!(owner_get_v["data"]["can_write"], true);

    // Stranger → identical soft not_found (D-25 / T-10-01).
    let (stranger_cookie, _) = signup_and_login(&app, "stranger2@ex.com", "stranger2").await;
    let missing = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.get","input":{"owner":"nobody","name":"missing-repo"}}"#,
            &stranger_cookie,
        ))
        .await
        .unwrap();
    let missing_bytes = missing.into_body().collect().await.unwrap().to_bytes();
    let missing_v: serde_json::Value = serde_json::from_slice(&missing_bytes).unwrap();

    let private = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.get","input":{"owner":"acl-org","name":"private-widget"}}"#,
            &stranger_cookie,
        ))
        .await
        .unwrap();
    let private_bytes = private.into_body().collect().await.unwrap().to_bytes();
    let private_v: serde_json::Value = serde_json::from_slice(&private_bytes).unwrap();

    assert_eq!(missing_v["error"]["code"], "repo.not_found");
    assert_eq!(
        private_v["error"]["code"], "repo.not_found",
        "org private stranger → repo.not_found — {private_v}"
    );
    assert_eq!(
        missing_v["error"]["message"], private_v["error"]["message"],
        "messages must match to avoid existence leak"
    );
}

/// Public org repo: anonymous read OK even when member_base is none (D-ORG-02b / D-ORG-05).
#[tokio::test]
async fn repo_private_404_org_public_anonymous_ok() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_org_public_anon.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "puborg@ex.com", "puborg1").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&owner_id, &now)
        .await
        .expect("verify");

    let org = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"pub-org"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(org.status(), StatusCode::OK);
    let _ = org.into_body().collect().await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"open-widget","visibility":"public","owner":"pub-org"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let _ = create.into_body().collect().await;

    let get = app
        .oneshot(rpc_req(
            r#"{"procedure":"repo.get","input":{"owner":"pub-org","name":"open-widget"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let bytes = get.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "anonymous public org get — {v}");
    assert_eq!(v["data"]["name"], "open-widget");
    assert_eq!(v["data"]["can_admin"], false, "anonymous must not be admin");
    assert_eq!(v["data"]["can_write"], false);
}

/// Collaborator-granted read on private org repo (ORG-03/04).
#[tokio::test]
async fn repo_private_404_collaborator_granted_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_collab_grant_read.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "cown@ex.com", "cown1").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&owner_id, &now)
        .await
        .expect("verify");

    let (collab_cookie, collab_v) = signup_and_login(&app, "cread@ex.com", "cread1").await;
    let collab_id = collab_v["data"]["id"].as_str().expect("id").to_string();
    db.set_email_verified_at(&collab_id, &now)
        .await
        .expect("verify collab");

    let (stranger_cookie, _) = signup_and_login(&app, "cstranger@ex.com", "cstranger1").await;

    let org = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"grant-org"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(org.status(), StatusCode::OK);
    let _ = org.into_body().collect().await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"locked","visibility":"private","owner":"grant-org"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let _ = create.into_body().collect().await;

    // Before grant: collaborator (outside) → not_found.
    let before = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.get","input":{"owner":"grant-org","name":"locked"}}"#,
            &collab_cookie,
        ))
        .await
        .unwrap();
    let before_bytes = before.into_body().collect().await.unwrap().to_bytes();
    let before_v: serde_json::Value = serde_json::from_slice(&before_bytes).unwrap();
    assert_eq!(before_v["ok"], false, "pre-grant — {before_v}");
    assert_eq!(before_v["error"]["code"], "repo.not_found");

    let grant = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"grant-org","name":"locked","username":"cread1","permission":"read"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    let grant_bytes = grant.into_body().collect().await.unwrap().to_bytes();
    let grant_v: serde_json::Value = serde_json::from_slice(&grant_bytes).unwrap();
    assert_eq!(grant_v["ok"], true, "grant — {grant_v}");

    // After grant: read collaborator can get; write not required.
    let after = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.get","input":{"owner":"grant-org","name":"locked"}}"#,
            &collab_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::OK);
    let after_bytes = after.into_body().collect().await.unwrap().to_bytes();
    let after_v: serde_json::Value = serde_json::from_slice(&after_bytes).unwrap();
    assert_eq!(after_v["ok"], true, "granted read — {after_v}");
    assert_eq!(after_v["data"]["name"], "locked");
    assert_eq!(after_v["data"]["can_write"], false);
    assert_eq!(after_v["data"]["can_admin"], false);

    // Stranger still soft not_found.
    let stranger = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.get","input":{"owner":"grant-org","name":"locked"}}"#,
            &stranger_cookie,
        ))
        .await
        .unwrap();
    let stranger_bytes = stranger.into_body().collect().await.unwrap().to_bytes();
    let stranger_v: serde_json::Value = serde_json::from_slice(&stranger_bytes).unwrap();
    assert_eq!(stranger_v["ok"], false, "stranger — {stranger_v}");
    assert_eq!(stranger_v["error"]["code"], "repo.not_found");
}

/// Wave 0 / D-ISS-20: unauthorized private `issue.list` → soft not-found (T-11-01).
#[tokio::test]
async fn repo_private_404_issue_list_unauthorized_soft_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_list_soft404.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "isspriv@ex.com", "isspriv").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify");

    let create_repo = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","description":""}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_repo.status(), StatusCode::OK);
    let _ = create_repo.into_body().collect().await;

    let create_issue = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.create","input":{"owner":"isspriv","name":"secret","title":"hidden"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    let cib = create_issue.into_body().collect().await.unwrap().to_bytes();
    let civ: serde_json::Value = serde_json::from_slice(&cib).unwrap();
    assert_eq!(civ["ok"], true, "owner create issue — {civ}");

    let (stranger_cookie, _) = signup_and_login(&app, "issstranger@ex.com", "issstranger").await;

    let missing = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.list","input":{"owner":"isspriv","name":"no-such-repo"}}"#,
            &stranger_cookie,
        ))
        .await
        .unwrap();
    let missing_bytes = missing.into_body().collect().await.unwrap().to_bytes();
    let missing_v: serde_json::Value = serde_json::from_slice(&missing_bytes).unwrap();
    assert_eq!(missing_v["ok"], false, "missing — {missing_v}");
    assert_eq!(missing_v["error"]["code"], "repo.not_found");

    let private = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.list","input":{"owner":"isspriv","name":"secret"}}"#,
            &stranger_cookie,
        ))
        .await
        .unwrap();
    let private_bytes = private.into_body().collect().await.unwrap().to_bytes();
    let private_v: serde_json::Value = serde_json::from_slice(&private_bytes).unwrap();
    assert_eq!(private_v["ok"], false, "private stranger — {private_v}");
    assert_eq!(
        private_v["error"]["code"], "repo.not_found",
        "unauthorized private issue.list → soft not_found — {private_v}"
    );
}

/// Wave 0 / D-ISS-20: unauthorized private `issue.get` → soft not-found (T-11-01).
#[tokio::test]
async fn repo_private_404_issue_get_unauthorized_soft_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_get_soft404.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "issget@ex.com", "issget").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify");

    let create_repo = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"vault","visibility":"private","description":""}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    let _ = create_repo.into_body().collect().await;

    let create_issue = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.create","input":{"owner":"issget","name":"vault","title":"secret issue"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    let cib = create_issue.into_body().collect().await.unwrap().to_bytes();
    let civ: serde_json::Value = serde_json::from_slice(&cib).unwrap();
    assert_eq!(civ["ok"], true, "{civ}");
    assert_eq!(civ["data"]["number"], 1);

    let (stranger_cookie, _) = signup_and_login(&app, "getstranger@ex.com", "getstranger").await;

    let missing = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.get","input":{"owner":"issget","name":"nope","number":1}}"#,
            &stranger_cookie,
        ))
        .await
        .unwrap();
    let missing_bytes = missing.into_body().collect().await.unwrap().to_bytes();
    let missing_v: serde_json::Value = serde_json::from_slice(&missing_bytes).unwrap();
    assert_eq!(missing_v["error"]["code"], "repo.not_found");

    let private = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.get","input":{"owner":"issget","name":"vault","number":1}}"#,
            &stranger_cookie,
        ))
        .await
        .unwrap();
    let private_bytes = private.into_body().collect().await.unwrap().to_bytes();
    let private_v: serde_json::Value = serde_json::from_slice(&private_bytes).unwrap();
    assert_eq!(private_v["ok"], false, "{private_v}");
    assert_eq!(
        private_v["error"]["code"], "repo.not_found",
        "unauthorized private issue.get → soft not_found — {private_v}"
    );
}

/// D-ISS-20: issue_private alias — private issue enumeration must not leak.
#[tokio::test]
async fn issue_private_unauthorized_soft_not_found() {
    // Covered by list + get soft-not-found cases above; keep alias name for Wave 0 filter.
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_private_alias.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "aliasown@ex.com", "aliasown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify");

    let create_repo = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"locked","visibility":"private","description":""}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    let _ = create_repo.into_body().collect().await;

    let anon = app
        .oneshot(rpc_req(
            r#"{"procedure":"issue.list","input":{"owner":"aliasown","name":"locked"}}"#,
        ))
        .await
        .unwrap();
    let bytes = anon.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(
        v["error"]["code"], "repo.not_found",
        "anonymous private issue.list → soft not_found — {v}"
    );
}

// --- Phase 12 pull private ACL Wave 0 stubs ---

#[tokio::test]
#[ignore = "Wave 0 stub — greened with pull.list soft not_found"]
async fn repo_private_404_pull_list_unauthorized() {
    assert!(false, "Wave 0: private pull.list soft not_found");
}

#[tokio::test]
#[ignore = "Wave 0 stub — greened with pull.get soft not_found"]
async fn repo_private_404_pull_get_unauthorized() {
    assert!(false, "Wave 0: private pull.get soft not_found");
}
