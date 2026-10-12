//! admin.instance.factory_reset scope radios (D-34): DB-only keeps disk; both wipes repos_dir.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::auth::hash_password_str;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, GitBackend};
use tower::ServiceExt;
use uuid::Uuid;

async fn test_app(
    db: Database,
    repos_dir: std::path::PathBuf,
    lfs_dir: std::path::PathBuf,
) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir)
        .with_lfs_dir(lfs_dir);
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

async fn seed_sys_admin(db: &Database) -> String {
    let hash = hash_password_str("password1").expect("hash");
    let id = Uuid::new_v4().to_string();
    db.create_user(
        &id,
        "admin@ex.com",
        "adminuser",
        Some(&hash),
        "Admin",
        "",
        None,
        oxidean_core::Role::SysAdmin,
    )
    .await
    .expect("create admin");
    id
}

async fn login_admin(app: &axum::Router) -> String {
    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"admin@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    session_cookie_from_response(&login)
}

#[tokio::test]
async fn factory_reset_database_only_keeps_repo_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("factory_reset_db_only.db").display()
    );
    let repos_dir = dir.path().join("repos");
    let bare = repos_dir.join("adminuser").join("keepme.git");
    tokio::fs::create_dir_all(&bare).await.unwrap();
    tokio::fs::write(bare.join("HEAD"), b"ref: refs/heads/main\n")
        .await
        .unwrap();

    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    seed_sys_admin(&db).await;

    let app = test_app(db, repos_dir.clone(), dir.path().join("lfs")).await;
    let cookie = login_admin(&app).await;

    let res = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.instance.factory_reset","input":{"confirmation":"RESET","scope":"database_only"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["needs_setup"], true);
    assert!(
        bare.join("HEAD").exists(),
        "database_only must keep bare repo files at {}",
        bare.display()
    );
}

#[tokio::test]
async fn factory_reset_database_and_repositories_wipes_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("factory_reset_both.db").display()
    );
    let repos_dir = dir.path().join("repos");
    let bare = repos_dir.join("adminuser").join("gone.git");
    tokio::fs::create_dir_all(&bare).await.unwrap();
    let git = CliGitBackend::new();
    git.init_bare(&bare, "main").await.expect("init_bare");

    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    seed_sys_admin(&db).await;

    let app = test_app(db, repos_dir.clone(), dir.path().join("lfs")).await;
    let cookie = login_admin(&app).await;

    let res = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.instance.factory_reset","input":{"confirmation":"RESET","scope":"database_and_repositories"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert!(repos_dir.exists(), "repos root must remain");
    assert!(
        !bare.exists(),
        "database_and_repositories must delete bare repo at {}",
        bare.display()
    );
}

#[tokio::test]
async fn factory_reset_defaults_scope_to_database_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("factory_reset_default.db").display()
    );
    let repos_dir = dir.path().join("repos");
    let bare = repos_dir.join("adminuser").join("default.git");
    tokio::fs::create_dir_all(&bare).await.unwrap();
    tokio::fs::write(bare.join("HEAD"), b"ref: refs/heads/main\n")
        .await
        .unwrap();

    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    seed_sys_admin(&db).await;

    let app = test_app(db, repos_dir, dir.path().join("lfs")).await;
    let cookie = login_admin(&app).await;

    // Omit scope — serde default database_only.
    let res = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.instance.factory_reset","input":{"confirmation":"RESET"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert!(bare.join("HEAD").exists(), "omitted scope keeps disk");
}

#[tokio::test]
async fn factory_reset_wrong_phrase_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("factory_reset_phrase.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    seed_sys_admin(&db).await;

    let app = test_app(db, dir.path().join("repos"), dir.path().join("lfs")).await;
    let cookie = login_admin(&app).await;

    let res = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.instance.factory_reset","input":{"confirmation":"reset","scope":"database_only"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "admin.factory_reset_confirm");
}

#[tokio::test]
async fn factory_reset_wipes_org_acl_and_repository_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("factory_reset_orgs_scope.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let admin_id = seed_sys_admin(&db).await;

    let org = db
        .insert_organization("o-wipe", "wipeorg", "Wipe Org", "none")
        .await
        .expect("org");
    db.insert_org_owner_membership(&org.id, &admin_id)
        .await
        .expect("membership");
    db.insert_org_invite(
        "inv-wipe",
        &org.id,
        Some("invitee@ex.com"),
        "member",
        "wipe-token-hash",
        Some("2099-01-01 00:00:00"),
        &admin_id,
        Some(1),
    )
    .await
    .expect("invite");
    let org_repo = db
        .insert_repository(
            "r-org-wipe",
            &org.id,
            "org",
            "teamrepo",
            "private",
            "",
            "main",
        )
        .await
        .expect("org repo");
    let user_repo = db
        .insert_repository(
            "r-user-wipe",
            &admin_id,
            "user",
            "mine",
            "private",
            "",
            "main",
        )
        .await
        .expect("user repo");

    let app = test_app(db.clone(), dir.path().join("repos"), dir.path().join("lfs")).await;
    let cookie = login_admin(&app).await;

    let res = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.instance.factory_reset","input":{"confirmation":"RESET","scope":"database_only"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["needs_setup"], true);

    assert_eq!(db.count_users().await.unwrap(), 0);
    assert!(db.find_organization_by_id(&org.id).await.unwrap().is_none());
    assert!(db
        .find_org_invite_by_id("inv-wipe")
        .await
        .unwrap()
        .is_none());
    assert!(db
        .find_repository_by_id(&org_repo.id)
        .await
        .unwrap()
        .is_none());
    assert!(db
        .find_repository_by_id(&user_repo.id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn factory_reset_wipes_issue_domain_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("factory_reset_issues_scope.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let admin_id = seed_sys_admin(&db).await;

    let repo = db
        .insert_repository(
            "r-iss-wipe",
            &admin_id,
            "user",
            "isswipe",
            "private",
            "",
            "main",
        )
        .await
        .expect("repo");
    let issue = db
        .insert_issue("i-rpc-wipe", &repo.id, &admin_id, "RPC wipe", "body")
        .await
        .expect("issue");
    let comment = db
        .insert_issue_comment("c-rpc-wipe", &issue.id, &admin_id, "hi")
        .await
        .expect("comment");
    let label = db
        .insert_label("lab-rpc-wipe", "bug", "#abcdef", "", None, Some(&repo.id))
        .await
        .expect("label");
    db.set_issue_labels(&issue.id, std::slice::from_ref(&label.id))
        .await
        .expect("labels");
    db.set_issue_assignees(&issue.id, std::slice::from_ref(&admin_id))
        .await
        .expect("assignees");
    assert!(db
        .toggle_issue_reaction(&issue.id, &admin_id, "+1")
        .await
        .expect("reaction"));
    db.insert_issue_link(
        "link-rpc-wipe",
        &issue.id,
        "pr_stub",
        Some(&repo.id),
        Some(7),
        None,
        Some("stub"),
        &admin_id,
    )
    .await
    .expect("link");

    let app = test_app(db.clone(), dir.path().join("repos"), dir.path().join("lfs")).await;
    let cookie = login_admin(&app).await;

    let res = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.instance.factory_reset","input":{"confirmation":"RESET","scope":"database_only"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["needs_setup"], true);

    assert_eq!(db.count_users().await.unwrap(), 0);
    assert!(db.find_repository_by_id(&repo.id).await.unwrap().is_none());
    assert!(db.find_issue_by_id(&issue.id).await.unwrap().is_none());
    assert!(db
        .find_issue_comment_by_id(&comment.id)
        .await
        .unwrap()
        .is_none());
    assert!(db.find_label_by_id(&label.id).await.unwrap().is_none());
    assert!(db
        .find_issue_link_by_id("link-rpc-wipe")
        .await
        .unwrap()
        .is_none());
}

/// D-LFS-04: `database_and_repositories` wipes LFS_DIR children, keeps root.
#[tokio::test]
async fn factory_reset_database_and_repositories_wipes_lfs_dir_children() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    std::fs::create_dir_all(lfs.join("ab").join("cd")).unwrap();
    std::fs::write(lfs.join("ab").join("cd").join("deadbeef"), b"blob").unwrap();
    let url = format!("sqlite:{}", dir.path().join("lfs_wipe.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    seed_sys_admin(&db).await;
    let app = test_app(db, repos, lfs.clone()).await;
    let cookie = login_admin(&app).await;

    let res = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.instance.factory_reset","input":{"confirmation":"RESET","scope":"database_and_repositories"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert!(lfs.is_dir(), "root kept");
    let mut entries = std::fs::read_dir(&lfs).unwrap();
    assert!(entries.next().is_none(), "lfs children wiped");
}
