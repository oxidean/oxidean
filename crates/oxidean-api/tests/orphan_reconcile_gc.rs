//! Orphan reconcile (D-36) + admin.repos.gc (D-37).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::auth::hash_password_str;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::jobs::{
    orphan_reconcile, orphan_reconcile_with_retention, soft_delete_retention_days,
};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, GitBackend};
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
async fn orphan_reconcile_keeps_soft_deleted_within_retention() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("orphan_keep.db").display());
    let repos = dir.path().join("repos");
    let bare = repos.join("adminuser").join("soft.git");
    tokio::fs::create_dir_all(&bare).await.unwrap();
    tokio::fs::write(bare.join("HEAD"), b"ref: refs/heads/main\n")
        .await
        .unwrap();

    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    let owner_id = seed_sys_admin(&db).await;
    let repo_id = Uuid::new_v4().to_string();
    db.insert_repository(&repo_id, &owner_id, "user", "soft", "public", "", "main")
        .await
        .expect("insert");
    db.soft_delete_repository(&repo_id).await.expect("soft");

    let stats = orphan_reconcile(&db, &repos).await.expect("reconcile");
    assert_eq!(stats.orphans_removed, 0);
    assert_eq!(stats.purged_soft_deleted, 0);
    assert!(bare.exists(), "within retention must keep disk");
    assert!(
        soft_delete_retention_days() >= 1,
        "default retention must be positive"
    );
}

#[tokio::test]
async fn orphan_reconcile_purges_soft_deleted_past_retention() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("orphan_purge.db").display());
    let repos = dir.path().join("repos");
    let bare = repos.join("adminuser").join("old.git");
    tokio::fs::create_dir_all(&bare).await.unwrap();
    tokio::fs::write(bare.join("HEAD"), b"ref: refs/heads/main\n")
        .await
        .unwrap();

    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    let owner_id = seed_sys_admin(&db).await;
    let repo_id = Uuid::new_v4().to_string();
    db.insert_repository(&repo_id, &owner_id, "user", "old", "public", "", "main")
        .await
        .expect("insert");
    db.soft_delete_repository(&repo_id).await.expect("soft");

    // retention_days=0 → any soft-deleted row is immediately eligible for purge.
    let stats = orphan_reconcile_with_retention(&db, &repos, 0)
        .await
        .expect("reconcile");
    assert_eq!(stats.purged_soft_deleted, 1, "past retention must purge");
    assert!(!bare.exists());
}

#[tokio::test]
async fn admin_repos_gc_runs_for_one_and_all() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("repo_gc.db").display());
    let repos = dir.path().join("repos");
    let bare = repos.join("adminuser").join("gcme.git");
    let git = CliGitBackend::new();
    tokio::fs::create_dir_all(bare.parent().unwrap())
        .await
        .unwrap();
    git.init_bare(&bare, "main").await.unwrap();
    git.seed_commit(&bare, "main", "seed", &[("a.txt".into(), b"a\n".to_vec())])
        .await
        .unwrap();

    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    let owner_id = seed_sys_admin(&db).await;
    let repo_id = Uuid::new_v4().to_string();
    db.insert_repository(&repo_id, &owner_id, "user", "gcme", "public", "", "main")
        .await
        .expect("insert");

    let app = test_app(db, repos).await;
    let cookie = login_admin(&app).await;

    let one = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.repos.gc","input":{"owner":"adminuser","name":"gcme"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(one.status(), StatusCode::OK);
    let one_v: serde_json::Value =
        serde_json::from_slice(&one.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(one_v["ok"], true, "{one_v}");
    assert_eq!(one_v["data"]["gc_count"], 1);

    let all = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.repos.gc","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let all_v: serde_json::Value =
        serde_json::from_slice(&all.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(all_v["ok"], true, "{all_v}");
    assert!(all_v["data"]["gc_count"].as_u64().unwrap() >= 1);
}
