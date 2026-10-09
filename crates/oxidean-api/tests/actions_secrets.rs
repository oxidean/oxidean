//! Phase 19 — Actions secrets encrypt/store + enable (ACT-06 / D-ACT-17 / D-ACT-06).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::actions::mint_registration_token;
use oxidean_api::actions::secrets::{decrypt_secret, encrypt_secret};
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use tower::ServiceExt;

async fn test_app(db: Database) -> axum::Router {
    std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "integration-test-key");
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_actions_enabled(true);
    router_with_state(state, build_cors("development", None).unwrap())
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
    res.headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now).await.unwrap();
}

async fn signup_login_verify(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
) -> (String, String) {
    let _ = app
        .clone()
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
        )))
        .await
        .unwrap();
    let login = app
        .clone()
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
        )))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let user_id = v["data"]["id"].as_str().unwrap().to_string();
    verify_user(db, &user_id).await;
    (cookie, user_id)
}

#[test]
fn actions_secrets_encrypt_never_stores_plaintext_blob() {
    std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "integration-test-key");
    let ct = encrypt_secret("hunter2-token").unwrap();
    assert!(!ct.contains("hunter2"));
    assert_eq!(decrypt_secret(&ct).unwrap(), "hunter2-token");
}

#[tokio::test]
async fn actions_secrets_list_names_only_never_echoes_value() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("sec.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;
    let (cookie, _uid) = signup_login_verify(&app, &db, "sec@ex.com", "secown").await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"secrepo","visibility":"private"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);

    let put = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.actions.secrets.put","input":{"owner":"secown","name":"secrepo","secret_name":"DEPLOY_KEY","value":"super-secret-value"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK);
    let put_bytes = put.into_body().collect().await.unwrap().to_bytes();
    let put_v: serde_json::Value = serde_json::from_slice(&put_bytes).unwrap();
    assert_eq!(put_v["data"]["ok"], true);
    let put_s = String::from_utf8_lossy(&put_bytes);
    assert!(!put_s.contains("super-secret-value"));

    let list = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.actions.secrets.list","input":{"owner":"secown","name":"secrepo"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_bytes = list.into_body().collect().await.unwrap().to_bytes();
    let list_s = String::from_utf8_lossy(&list_bytes);
    assert!(!list_s.contains("super-secret-value"));
    let list_v: serde_json::Value = serde_json::from_slice(&list_bytes).unwrap();
    let secrets = list_v["data"]["secrets"].as_array().unwrap();
    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets[0]["name"], "DEPLOY_KEY");
    assert!(secrets[0].get("value").is_none());
    assert!(secrets[0].get("ciphertext").is_none());

    let repo = db
        .find_repository_by_owner_name(&_uid, "secrepo")
        .await
        .unwrap()
        .unwrap();
    let cts = db.list_action_secret_ciphertexts(&repo.id).await.unwrap();
    assert_eq!(cts.len(), 1);
    assert!(!cts[0].ciphertext.contains("super-secret-value"));
    assert_eq!(
        decrypt_secret(&cts[0].ciphertext).unwrap(),
        "super-secret-value"
    );
}

#[tokio::test]
async fn actions_secrets_enable_toggle_persists() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("en.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;
    let (cookie, _) = signup_login_verify(&app, &db, "en@ex.com", "enown").await;
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"enrepo","visibility":"private"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);

    let get1 = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.actions.getEnabled","input":{"owner":"enown","name":"enrepo"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let g1: serde_json::Value =
        serde_json::from_slice(&get1.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(g1["data"]["enabled"], true);

    let set = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.actions.setEnabled","input":{"owner":"enown","name":"enrepo","enabled":false}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(set.status(), StatusCode::OK);

    let get2 = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.actions.getEnabled","input":{"owner":"enown","name":"enrepo"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let g2: serde_json::Value =
        serde_json::from_slice(&get2.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(g2["data"]["enabled"], false);
}

#[tokio::test]
async fn actions_secrets_injected_into_fetch_task() {
    std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "integration-test-key");
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("ft.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let owner = db
        .create_user(
            "u-ft",
            "ft@example.com",
            "ftown",
            Some("h"),
            "F",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-ft", &owner.id, "user", "ftown", "private", "", "main")
        .await
        .unwrap();
    let ct = encrypt_secret("from-fetch").unwrap();
    db.insert_action_secret("sec-ft", &repo.id, "API_TOKEN", &ct)
        .await
        .unwrap();
    db.insert_action_run(
        "run-ft",
        &repo.id,
        ".github/workflows/ci.yml",
        "CI",
        "push",
        "abc",
        "refs/heads/main",
        "CI",
        None,
    )
    .await
    .unwrap();
    db.insert_action_job("job-ft", "run-ft", "build", "build", r#"["ubuntu-latest"]"#)
        .await
        .unwrap();

    let app = test_app(db.clone()).await;
    let reg = mint_registration_token(&db).await.unwrap();
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "r1",
                        "labels": ["ubuntu-latest"],
                        "token": reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let reg_v: serde_json::Value =
        serde_json::from_slice(&reg_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let runner_token = reg_v["runner_token"].as_str().unwrap();

    let fetch = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/fetch_task")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {runner_token}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fetch.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&fetch.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["job_id"], "job-ft");
    assert_eq!(v["secrets"]["API_TOKEN"], "from-fetch");
}

#[tokio::test]
async fn actions_secrets_admin_create_registration_token() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("tok.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let hash = oxidean_api::auth::hash_password_str("password1").unwrap();
    let admin = db
        .create_user(
            "u-adm",
            "adm@ex.com",
            "admtok",
            Some(&hash),
            "A",
            "",
            None,
            Role::SysAdmin,
        )
        .await
        .unwrap();
    verify_user(&db, &admin.id).await;

    let app = test_app(db).await;
    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"adm@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);

    let mint = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.actions.createRegistrationToken","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(mint.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&mint.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(v["data"]["token"].as_str().unwrap().starts_with("reg_"));
    // token-only response (AdminActionsCreateRegistrationTokenResponse)

    let list = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.actions.listRunners","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
}
