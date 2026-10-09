//! email.list / add / remove / setPrimary / resendVerify

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

async fn test_app(db: Database) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development");
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

#[tokio::test]
async fn email_add_set_primary_remove_roundtrip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("emails.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "primary@ex.com", "emailuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify primary");

    let (status, list_v) =
        rpc_json(&app, r#"{"procedure":"email.list","input":{}}"#, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{list_v}");
    assert_eq!(list_v["data"].as_array().unwrap().len(), 1);
    assert_eq!(list_v["data"][0]["email"], "primary@ex.com");
    assert_eq!(list_v["data"][0]["is_primary"], true);

    let add = serde_json::json!({
        "procedure": "email.add",
        "input": { "email": "second@ex.com" }
    })
    .to_string();
    let (status, add_v) = rpc_json(&app, &add, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{add_v}");
    assert_eq!(add_v["data"]["email"], "second@ex.com");
    assert_eq!(add_v["data"]["is_primary"], false);
    assert_eq!(add_v["data"]["verified"], false);
    let second_id = add_v["data"]["id"].as_str().unwrap().to_string();

    // Cannot set primary until verified.
    let set_p = serde_json::json!({
        "procedure": "email.setPrimary",
        "input": { "id": second_id }
    })
    .to_string();
    let (status, set_v) = rpc_json(&app, &set_p, &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{set_v}");
    assert_eq!(set_v["error"]["code"], "email.not_verified");

    db.set_user_email_verified_at(&second_id, Some(&now))
        .await
        .expect("verify second");

    let (status, set_v) = rpc_json(&app, &set_p, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{set_v}");
    assert_eq!(set_v["data"]["is_primary"], true);

    let user = db.find_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.email, "second@ex.com");
    assert!(user.email_verified_at.is_some());

    // Remove old primary (now secondary).
    let (status, list_v) =
        rpc_json(&app, r#"{"procedure":"email.list","input":{}}"#, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{list_v}");
    let old = list_v["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["email"] == "primary@ex.com")
        .expect("old primary");
    let old_id = old["id"].as_str().unwrap();

    let rem = serde_json::json!({
        "procedure": "email.remove",
        "input": { "id": old_id }
    })
    .to_string();
    let (status, rem_v) = rpc_json(&app, &rem, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{rem_v}");

    // Cannot remove current primary.
    let rem_p = serde_json::json!({
        "procedure": "email.remove",
        "input": { "id": second_id }
    })
    .to_string();
    let (status, rem_v) = rpc_json(&app, &rem_p, &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rem_v}");
    assert_eq!(rem_v["error"]["code"], "email.cannot_remove_primary");
}

#[tokio::test]
async fn email_add_rejects_taken_and_max() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("emails_max.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "cap@ex.com", "capuser").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now).await.unwrap();

    let taken = serde_json::json!({
        "procedure": "email.add",
        "input": { "email": "cap@ex.com" }
    })
    .to_string();
    let (status, v) = rpc_json(&app, &taken, &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "email.taken");

    for i in 0..9 {
        let email = format!("extra{i}@ex.com");
        db.create_user_email(&format!("extra-{i}"), &user_id, &email, false, None)
            .await
            .unwrap();
    }
    let add = serde_json::json!({
        "procedure": "email.add",
        "input": { "email": "overflow@ex.com" }
    })
    .to_string();
    let (status, v) = rpc_json(&app, &add, &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "email.max_emails");
}

#[tokio::test]
async fn secondary_email_verify_otp_marks_row_verified() {
    use oxidean_api::auth::verify_reset;

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("emails_verify_sec.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "prim@ex.com", "secverify").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify primary");

    let add = serde_json::json!({
        "procedure": "email.add",
        "input": { "email": "second@ex.com" }
    })
    .to_string();
    let (status, add_v) = rpc_json(&app, &add, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{add_v}");
    assert_eq!(add_v["data"]["verified"], false);
    let second_id = add_v["data"]["id"].as_str().unwrap().to_string();

    let secrets = verify_reset::issue_verify_for_target(&db, &user_id, "second@ex.com")
        .await
        .expect("issue secondary verify");

    let verify_body = format!(
        r#"{{"procedure":"auth.verify","input":{{"code":"{}"}}}}"#,
        secrets.otp
    );
    let (status, verify_v) = rpc_json(&app, &verify_body, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{verify_v}");
    // Primary was already verified — me.email_verified stays true.
    assert_eq!(verify_v["data"]["email_verified"], true);

    let (status, list_v) =
        rpc_json(&app, r#"{"procedure":"email.list","input":{}}"#, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{list_v}");
    let second = list_v["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == second_id)
        .expect("secondary row");
    assert_eq!(second["verified"], true);
    assert_eq!(second["is_primary"], false);

    let set_p = serde_json::json!({
        "procedure": "email.setPrimary",
        "input": { "id": second_id }
    })
    .to_string();
    let (status, set_v) = rpc_json(&app, &set_p, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{set_v}");
    assert_eq!(set_v["data"]["is_primary"], true);
    assert_eq!(set_v["data"]["email"], "second@ex.com");
}

#[tokio::test]
async fn email_list_heals_missing_primary_flag() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("emails_heal.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "heal@ex.com", "healuser").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now).await.unwrap();

    let emails = db.list_user_emails(&user_id).await.expect("list");
    assert_eq!(emails.len(), 1);
    let primary_id = emails[0].id.clone();
    // Simulate interrupted setPrimary: no row marked primary.
    db.delete_user_email(&primary_id)
        .await
        .expect("delete primary");
    db.create_user_email(&primary_id, &user_id, "heal@ex.com", false, Some(&now))
        .await
        .expect("reinsert without primary");
    db.create_user_email("sec-heal", &user_id, "other@ex.com", false, Some(&now))
        .await
        .expect("secondary");
    let broken = db.list_user_emails(&user_id).await.expect("broken list");
    assert!(broken.iter().all(|e| !e.is_primary));

    let (status, list_v) =
        rpc_json(&app, r#"{"procedure":"email.list","input":{}}"#, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{list_v}");
    let rows = list_v["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let primary = rows
        .iter()
        .find(|e| e["is_primary"] == true)
        .expect("healed primary");
    assert_eq!(primary["email"], "heal@ex.com");
    assert_eq!(rows.iter().filter(|e| e["is_primary"] == true).count(), 1);
}
