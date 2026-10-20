//! GIT-06 / D-27 / D-28: owner branch CRUD + default-branch soft-protect.

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

async fn rpc_json(app: &axum::Router, body: &str, cookie: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    // RPC maps domain errors to 4xx with JSON body; always parse for assertions.
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("rpc json body")
}

async fn seed_owner_repo(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
    repo: &str,
) -> String {
    let (cookie, login_v) = signup_and_login(app, email, username).await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let create = rpc_json(
        app,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{repo}","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "seed create — {create}");
    cookie
}

/// Owner cannot rename or delete the repository default branch (D-28).
#[tokio::test]
async fn repo_branch_soft_protect_blocks_default_rename_and_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("branch_soft_protect.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let cookie = seed_owner_repo(&app, &db, "owner@ex.com", "owner1", "softprot").await;

    let rename = rpc_json(
        &app,
        r#"{"procedure":"repo.branchRename","input":{"owner":"owner1","name":"softprot","from":"main","to":"trunk"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(
        rename["ok"], false,
        "default rename must fail soft-protect — {rename}"
    );
    assert_eq!(
        rename["error"]["code"], "repo.default_branch_protected",
        "stable soft-protect code on rename — {rename}"
    );

    let delete = rpc_json(
        &app,
        r#"{"procedure":"repo.branchDelete","input":{"owner":"owner1","name":"softprot","branch":"main"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(
        delete["ok"], false,
        "default delete must fail soft-protect — {delete}"
    );
    assert_eq!(
        delete["error"]["code"], "repo.default_branch_protected",
        "stable soft-protect code on delete — {delete}"
    );
}

/// Owner can create/rename/delete non-default branches (GIT-06 CRUD).
#[tokio::test]
async fn repo_branch_soft_protect_allows_non_default_crud() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("branch_crud.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let cookie = seed_owner_repo(&app, &db, "crud@ex.com", "crudowner", "branchy").await;

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"crudowner","name":"branchy","branch":"feature-x","start":"main"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "owner create must succeed — {create}");
    assert_eq!(create["data"]["branch"], "feature-x");

    let rename = rpc_json(
        &app,
        r#"{"procedure":"repo.branchRename","input":{"owner":"crudowner","name":"branchy","from":"feature-x","to":"feature-y"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(rename["ok"], true, "owner rename must succeed — {rename}");
    assert_eq!(rename["data"]["branch"], "feature-y");

    let delete = rpc_json(
        &app,
        r#"{"procedure":"repo.branchDelete","input":{"owner":"crudowner","name":"branchy","branch":"feature-y"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(delete["ok"], true, "owner delete must succeed — {delete}");

    let refs = rpc_json(
        &app,
        r#"{"procedure":"repo.refs","input":{"owner":"crudowner","name":"branchy"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(refs["ok"], true, "refs after delete — {refs}");
    let names: Vec<&str> = refs["data"]["refs"]
        .as_array()
        .expect("refs array")
        .iter()
        .filter_map(|r| r["name"].as_str())
        .collect();
    assert!(
        !names
            .iter()
            .any(|n| n.contains("feature-y") || n.contains("feature-x")),
        "deleted branch must not appear in refs — {names:?}"
    );
    assert!(
        names
            .iter()
            .any(|n| n.contains("main") || n.ends_with("/main")),
        "default branch must remain — {names:?}"
    );
}

/// CR-02 / D-28: option-like branchCreate(-D) must not force-delete the default.
#[tokio::test]
async fn repo_branch_create_rejects_option_like_name_leaves_default_intact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("branch_option_inject.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let cookie = seed_owner_repo(&app, &db, "inject@ex.com", "injectown", "injecty").await;

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"injectown","name":"injecty","branch":"-D","start":"main"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(
        create["ok"], false,
        "option-like branchCreate must fail — {create}"
    );
    let code = create["error"]["code"].as_str().unwrap_or("");
    assert!(
        code == "repo.invalid_ref" || code == "rpc.bad_input" || code.starts_with("repo."),
        "expected invalid-ref style error, got {code} — {create}"
    );
    assert_ne!(code, "ok", "must not succeed — {create}");

    // Soft-protect still observes the default: delete of main must return protected.
    let delete = rpc_json(
        &app,
        r#"{"procedure":"repo.branchDelete","input":{"owner":"injectown","name":"injecty","branch":"main"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(
        delete["ok"], false,
        "default must still exist for soft-protect — {delete}"
    );
    assert_eq!(
        delete["error"]["code"], "repo.default_branch_protected",
        "default intact + soft-protect path — {delete}"
    );
}

/// Non-owner must not mutate branches (D-27) — identical not_found (no leak).
#[tokio::test]
async fn repo_branch_soft_protect_non_owner_mutate_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("branch_non_owner.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let _owner = seed_owner_repo(&app, &db, "own@ex.com", "repoown", "shared").await;
    let (stranger_cookie, stranger_v) =
        signup_and_login(&app, "stranger@ex.com", "stranger1").await;
    let stranger_id = stranger_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&stranger_id, &now)
        .await
        .expect("verify stranger");

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"repoown","name":"shared","branch":"evil","start":"main"}}"#,
        &stranger_cookie,
    )
    .await;
    assert_eq!(create["ok"], false, "non-owner create blocked — {create}");
    assert_eq!(
        create["error"]["code"], "repo.not_found",
        "non-owner mutate must not leak — {create}"
    );
}

/// ORG-04 / T-10-14: Write collaborator may create branches; Read cannot.
#[tokio::test]
async fn repo_branch_write_collaborator_can_create_read_cannot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("branch_collab_write.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let owner_cookie = seed_owner_repo(&app, &db, "bown@ex.com", "bown1", "shared").await;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    let (write_cookie, write_v) = signup_and_login(&app, "bwrite@ex.com", "bwrite1").await;
    let write_id = write_v["data"]["id"].as_str().expect("id").to_string();
    db.set_email_verified_at(&write_id, &now)
        .await
        .expect("verify write");

    let (read_cookie, read_v) = signup_and_login(&app, "bread@ex.com", "bread1").await;
    let read_id = read_v["data"]["id"].as_str().expect("id").to_string();
    db.set_email_verified_at(&read_id, &now)
        .await
        .expect("verify read");

    let add_write = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"bown1","name":"shared","username":"bwrite1","permission":"write"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(add_write["ok"], true, "add write collab — {add_write}");

    let add_read = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"bown1","name":"shared","username":"bread1","permission":"read"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(add_read["ok"], true, "add read collab — {add_read}");

    let create_ok = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"bown1","name":"shared","branch":"feature-w","start":"main"}}"#,
        &write_cookie,
    )
    .await;
    assert_eq!(
        create_ok["ok"], true,
        "Write collaborator must create branch — {create_ok}"
    );
    assert_eq!(create_ok["data"]["branch"], "feature-w");

    let create_deny = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"bown1","name":"shared","branch":"feature-r","start":"main"}}"#,
        &read_cookie,
    )
    .await;
    assert_eq!(
        create_deny["ok"], false,
        "Read collaborator must not create branch — {create_deny}"
    );
    assert_eq!(
        create_deny["error"]["code"], "repo.not_found",
        "Read mutate must soft not_found — {create_deny}"
    );
}

async fn rest(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    body: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie);
    if body.is_some() {
        b = b.header("content-type", "application/json");
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::from(body.unwrap_or("").to_owned())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

/// GitHub REST surface: `/branches` list/get/rename, `/branches/{b}/protection`
/// get/put/delete, `/git/matching-refs`, `/git/ref`, `/git/refs` create/delete.
#[tokio::test]
async fn repo_rest_branches_protection_refs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("rest_branches.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let cookie = seed_owner_repo(&app, &db, "b@ex.com", "bown", "core").await;
    let base = "/api/v1/repos/bown/core";

    // ---- branches ------------------------------------------------------------
    let (s, v) = rest(&app, "GET", &format!("{base}/branches"), &cookie, None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let branches = v.as_array().unwrap();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0]["name"], "main");
    assert_eq!(branches[0]["protected"], false);
    let main_sha = branches[0]["commit"]["sha"].as_str().unwrap().to_string();

    let (s, v) = rest(&app, "GET", &format!("{base}/branches/main"), &cookie, None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["commit"]["sha"], main_sha);
    let (s, _) = rest(&app, "GET", &format!("{base}/branches/nope"), &cookie, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // create a branch via the git-refs route
    let (s, v) = rest(
        &app,
        "POST",
        &format!("{base}/git/refs"),
        &cookie,
        Some(&format!(
            r#"{{"ref":"refs/heads/feature","sha":"{main_sha}"}}"#
        )),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["ref"], "refs/heads/feature");

    // non-heads ref rejected
    let (s, _) = rest(
        &app,
        "POST",
        &format!("{base}/git/refs"),
        &cookie,
        Some(&format!(r#"{{"ref":"refs/tags/v1","sha":"{main_sha}"}}"#)),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // git ref lookups
    let (s, v) = rest(
        &app,
        "GET",
        &format!("{base}/git/matching-refs/heads/f"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["ref"], "refs/heads/feature");
    assert_eq!(v[0]["object"]["sha"], main_sha);

    let (s, v) = rest(
        &app,
        "GET",
        &format!("{base}/git/ref/heads/main"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["ref"], "refs/heads/main");
    let (s, _) = rest(
        &app,
        "GET",
        &format!("{base}/git/ref/heads/ghost"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // ---- protection ------------------------------------------------------------
    let (s, _) = rest(
        &app,
        "GET",
        &format!("{base}/branches/main/protection"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (s, v) = rest(
        &app,
        "PUT",
        &format!("{base}/branches/main/protection"),
        &cookie,
        Some(r#"{
            "required_status_checks": {"strict": true, "checks": [{"context": "ci/test"}]},
            "required_pull_request_reviews": {"required_approving_review_count": 2, "dismiss_stale_reviews": true},
            "enforce_admins": true,
            "required_linear_history": false,
            "allow_force_pushes": false,
            "allow_deletions": false
        }"#),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");

    let (s, v) = rest(
        &app,
        "GET",
        &format!("{base}/branches/main/protection"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["required_status_checks"]["strict"], true);
    assert_eq!(v["required_status_checks"]["contexts"][0], "ci/test");
    assert_eq!(
        v["required_pull_request_reviews"]["required_approving_review_count"],
        2
    );
    assert_eq!(v["enforce_admins"]["enabled"], true);

    // the branch now reports protected
    let (s, v) = rest(&app, "GET", &format!("{base}/branches/main"), &cookie, None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["protected"], true);

    // PUT again = update (same pattern) — relax reviews
    let (s, _) = rest(
        &app,
        "PUT",
        &format!("{base}/branches/main/protection"),
        &cookie,
        Some(r#"{"required_pull_request_reviews": {"required_approving_review_count": 1}}"#),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = rest(
        &app,
        "GET",
        &format!("{base}/branches/main/protection"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        v["required_pull_request_reviews"]["required_approving_review_count"],
        1
    );
    // required_status_checks cleared by the second PUT
    assert!(v["required_status_checks"].is_null());

    let (s, _) = rest(
        &app,
        "DELETE",
        &format!("{base}/branches/main/protection"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = rest(
        &app,
        "GET",
        &format!("{base}/branches/main/protection"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // ---- rename + ref delete -----------------------------------------------------
    let (s, _) = rest(
        &app,
        "POST",
        &format!("{base}/branches/feature/rename"),
        &cookie,
        Some(r#"{"new_name":"feat2"}"#),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _) = rest(
        &app,
        "GET",
        &format!("{base}/branches/feature"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = rest(
        &app,
        "GET",
        &format!("{base}/branches/feat2"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let (s, _) = rest(
        &app,
        "DELETE",
        &format!("{base}/git/refs/heads/feat2"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = rest(
        &app,
        "GET",
        &format!("{base}/branches/feat2"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
