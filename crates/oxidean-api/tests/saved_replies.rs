//! WI-0001 pr-new-ui parity: `savedReplies.*` over the RPC shell —
//! session-scoped CRUD, anonymous callers rejected, validation bounds.

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

fn rpc(body: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1");
    if let Some(c) = cookie {
        b = b.header("cookie", c);
    }
    b.body(Body::from(body.to_owned())).unwrap()
}

fn session_cookie(res: &axum::http::Response<Body>) -> String {
    res.headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

async fn seed_and_login(app: &axum::Router) -> String {
    let res = app
        .clone()
        .oneshot(rpc(
            r#"{"procedure":"auth.signup","input":{"email":"sr@ex.com","username":"sruser","password":"password1"}}"#,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let _ = res.into_body().collect().await;
    let login = app
        .clone()
        .oneshot(rpc(
            r#"{"procedure":"auth.login","input":{"identifier":"sr@ex.com","password":"password1","remember_me":false}}"#,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    session_cookie(&login)
}

async fn rpc_json(app: &axum::Router, body: &str, cookie: Option<&str>) -> serde_json::Value {
    let res = app.clone().oneshot(rpc(body, cookie)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn saved_replies_crud_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("saved-replies.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;
    let cookie = seed_and_login(&app).await;

    let empty = rpc_json(&app, r#"{"procedure":"savedReplies.list","input":{}}"#, Some(&cookie)).await;
    assert_eq!(empty["ok"], true, "{empty}");
    assert_eq!(empty["data"]["replies"].as_array().unwrap().len(), 0);

    let created = rpc_json(
        &app,
        r#"{"procedure":"savedReplies.create","input":{"name":"  LGTM  ","body":"Looks good, thanks!"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(created["data"]["name"], "LGTM");
    let id = created["data"]["id"].as_str().unwrap().to_string();

    // Empty name rejected.
    let bad = rpc_json(
        &app,
        r#"{"procedure":"savedReplies.create","input":{"name":"  ","body":"x"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(bad["ok"], false, "{bad}");

    let updated = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"savedReplies.update","input":{{"id":"{id}","body":"Updated body"}}}}"#
        ),
        Some(&cookie),
    )
    .await;
    assert_eq!(updated["ok"], true, "{updated}");
    assert_eq!(updated["data"]["body"], "Updated body");
    assert_eq!(updated["data"]["name"], "LGTM");

    let list = rpc_json(&app, r#"{"procedure":"savedReplies.list","input":{}}"#, Some(&cookie)).await;
    let replies = list["data"]["replies"].as_array().unwrap();
    assert_eq!(replies.len(), 1, "{list}");
    assert_eq!(replies[0]["id"], id.as_str());

    let deleted = rpc_json(
        &app,
        &format!(r#"{{"procedure":"savedReplies.delete","input":{{"id":"{id}"}}}}"#),
        Some(&cookie),
    )
    .await;
    assert_eq!(deleted["ok"], true, "{deleted}");
    assert_eq!(deleted["data"]["ok"], true);

    let after = rpc_json(&app, r#"{"procedure":"savedReplies.list","input":{}}"#, Some(&cookie)).await;
    assert_eq!(after["data"]["replies"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn saved_replies_require_session() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("saved-replies-anon.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;

    for body in [
        r#"{"procedure":"savedReplies.list","input":{}}"#,
        r#"{"procedure":"savedReplies.create","input":{"name":"x","body":"y"}}"#,
        r#"{"procedure":"savedReplies.update","input":{"id":"00000000-0000-0000-0000-000000000000"}}"#,
        r#"{"procedure":"savedReplies.delete","input":{"id":"00000000-0000-0000-0000-000000000000"}}"#,
    ] {
        let res = rpc_json(&app, body, None).await;
        assert_eq!(res["ok"], false, "{body} → {res}");
        assert_eq!(res["error"]["code"], "auth.unauthenticated", "{res}");
    }
}
