//! Branded status pages (#207): unmatched paths and method mismatches answer
//! the shared themed document for `Accept: text/html` requesters and the JSON
//! error envelope for API/CLI clients — from `oxidean-status-pages`, the same
//! crate the web tier renders.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

async fn app() -> axum::Router {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", tmp.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    );
    router_with_state(state, build_cors("development", None).unwrap())
}

async fn req(
    app: &axum::Router,
    method: &str,
    uri: &str,
    accept: Option<&str>,
) -> (StatusCode, String, axum::http::HeaderMap) {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(a) = accept {
        b = b.header(header::ACCEPT, a);
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned(), headers)
}

#[tokio::test]
async fn unmatched_path_negotiates_branded_page_or_json() {
    let app = app().await;

    // Browser: the branded document.
    let (status, body, headers) = req(&app, "GET", "/no/such/route", Some("text/html")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    assert!(body.contains("Page not found"), "{body}");
    assert!(body.contains("Oxidean"), "{body}");

    // API/CLI client: the JSON error envelope.
    for accept in [None, Some("application/json"), Some("*/*")] {
        let (status, body, headers) = req(&app, "GET", "/no/such/route", accept).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "accept={accept:?}");
        let ct = headers.get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
        assert!(
            ct.starts_with("application/json"),
            "accept={accept:?} ct={ct}"
        );
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "route.not_found");
    }
}

#[tokio::test]
async fn method_mismatch_negotiates_page_or_json_and_keeps_allow() {
    let app = app().await;

    // `/health` is GET-only — a POST is a method mismatch, not a 404.
    let (status, _, headers) = req(&app, "POST", "/health", Some("text/html")).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(headers.contains_key(header::ALLOW));
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );

    let (status, body, _) = req(&app, "POST", "/health", None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["error"]["code"], "route.method_not_allowed");
}

#[tokio::test]
async fn bare_route_errors_upgrade_for_browsers_only() {
    let app = app().await;

    // `/uploads` routes answer bare 4xx bodies for misses — browsers get the
    // branded page, everyone else keeps the terse response.
    let (status, body, headers) = req(
        &app,
        "GET",
        "/uploads/avatars/definitely-missing.png",
        Some("text/html"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    assert!(body.contains("Page not found"), "{body}");
}
