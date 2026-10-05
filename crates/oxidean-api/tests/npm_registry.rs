//! npm registry — PKG-02 / D-PKG-14.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

fn b64(input: &[u8]) -> String {
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

async fn setup() -> (axum::Router, Database, String, tempfile::TempDir) {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "https://packages.example.com");
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("n.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_packages_dir(dir.path().join("pkg"));
    let app = router_with_state(state, build_cors("development", None).unwrap());

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(
                    r#"{"procedure":"auth.signup","input":{"email":"npm@ex.com","username":"npmowner","password":"password1"}}"#,
                ))
                .unwrap(),
        )
        .await;
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(
                    r#"{"procedure":"auth.login","input":{"identifier":"npm@ex.com","password":"password1","remember_me":false}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = login
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let uid = v["data"]["id"].as_str().unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(uid, &now).await.unwrap();
    let pat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .header("cookie", &cookie)
                .body(Body::from(
                    r#"{"procedure":"pat.createFineGrained","input":{"name":"npm","repo_access":"all","contents":"read","packages":"write"}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = pat.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v["data"]["token"].as_str().unwrap().to_string();
    (app, db, token, dir)
}

fn basic(token: &str) -> String {
    format!("Basic {}", b64(format!("npmowner:{token}").as_bytes()))
}

fn publish_body(name: &str, version: &str, tarball: &[u8]) -> String {
    let filename = format!("{name}-{version}.tgz");
    serde_json::json!({
        "name": name,
        "dist-tags": { "latest": version },
        "versions": {
            version: { "name": name, "version": version }
        },
        "_attachments": {
            filename: {
                "content_type": "application/octet-stream",
                "data": b64(tarball),
                "length": tarball.len()
            }
        }
    })
    .to_string()
}

async fn publish(app: &axum::Router, token: &str, name: &str, version: &str, tar: &[u8]) -> StatusCode {
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/npm/npmowner/{name}"))
        .header(header::AUTHORIZATION, basic(token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(publish_body(name, version, tar)))
        .unwrap();
    app.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn npm_registry_publish_put_attachment() {
    let (app, _db, token, _dir) = setup().await;
    assert_eq!(
        publish(&app, &token, "demo", "1.0.0", b"tarball-bytes").await,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn npm_registry_packument_get() {
    let (app, _db, token, _dir) = setup().await;
    assert_eq!(
        publish(&app, &token, "demo", "1.0.0", b"tarball-bytes").await,
        StatusCode::CREATED
    );
    let res = app
        .oneshot(
            Request::builder()
                .uri("/npm/npmowner/demo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(v["versions"]["1.0.0"].is_object());
    let dist = &v["versions"]["1.0.0"]["dist"];
    assert_eq!(
        dist["shasum"].as_str().unwrap(),
        "9ef2570c89e65b9fe47687b0b49e122e59354bef"
    );
    assert_eq!(
        dist["integrity"].as_str().unwrap(),
        "sha512-31GJ6SyG+dFBGDyQB1x8EHrdSW/FYRyb4mHdZIaXTme9WooJgWCCmKoHe70PgTrvis7oBfnw5WLWBp8KVj59oA=="
    );
}

#[tokio::test]
async fn npm_registry_tarball_get_public_origin() {
    let (app, _db, token, _dir) = setup().await;
    assert_eq!(
        publish(&app, &token, "demo", "1.0.0", b"tarball-bytes").await,
        StatusCode::CREATED
    );
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/npm/npmowner/demo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let tarball = v["versions"]["1.0.0"]["dist"]["tarball"].as_str().unwrap();
    assert!(
        tarball.starts_with("https://packages.example.com/npm/"),
        "expected dist.tarball under OXIDEAN_PUBLIC_ORIGIN — {tarball}"
    );
    let get = app
        .oneshot(
            Request::builder()
                .uri(tarball.strip_prefix("https://packages.example.com").unwrap())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    assert_eq!(
        &get.into_body().collect().await.unwrap().to_bytes()[..],
        b"tarball-bytes"
    );
}

#[tokio::test]
async fn npm_registry_version_overwrite_conflict() {
    let (app, _db, token, _dir) = setup().await;
    assert_eq!(
        publish(&app, &token, "demo", "1.0.0", b"a").await,
        StatusCode::CREATED
    );
    assert_eq!(
        publish(&app, &token, "demo", "1.0.0", b"b").await,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn npm_registry_dist_tags() {
    let (app, _db, token, _dir) = setup().await;
    assert_eq!(
        publish(&app, &token, "demo", "1.0.0", b"a").await,
        StatusCode::CREATED
    );
    let put = Request::builder()
        .method("PUT")
        .uri("/npm/npmowner/-/package/demo/dist-tags/beta")
        .header(header::AUTHORIZATION, basic(&token))
        .body(Body::from("\"1.0.0\""))
        .unwrap();
    assert_eq!(app.clone().oneshot(put).await.unwrap().status(), StatusCode::OK);
    let get = app
        .oneshot(
            Request::builder()
                .uri("/npm/npmowner/-/package/demo/dist-tags")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&get.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["beta"], "1.0.0");
}

#[tokio::test]
async fn npm_registry_deprecate() {
    let (app, _db, token, _dir) = setup().await;
    assert_eq!(
        publish(&app, &token, "demo", "1.0.0", b"a").await,
        StatusCode::CREATED
    );
    let body = serde_json::json!({
        "name": "demo",
        "versions": {
            "1.0.0": { "name": "demo", "version": "1.0.0", "deprecated": "use v2" }
        }
    })
    .to_string();
    let req = Request::builder()
        .method("PUT")
        .uri("/npm/npmowner/demo")
        .header(header::AUTHORIZATION, basic(&token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
    let pack = app
        .oneshot(
            Request::builder()
                .uri("/npm/npmowner/demo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_slice(&pack.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["versions"]["1.0.0"]["deprecated"], "use v2");
}

#[tokio::test]
async fn npm_registry_search_v1() {
    let (app, _db, token, _dir) = setup().await;
    assert_eq!(
        publish(&app, &token, "findme", "1.0.0", b"a").await,
        StatusCode::CREATED
    );
    let res = app
        .oneshot(
            Request::builder()
                .uri("/npm/npmowner/-/v1/search?text=find")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(!v["objects"].as_array().unwrap().is_empty());
}
