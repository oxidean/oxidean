//! U7 `vcs.*` host surface — dispatch arms, repo ACL tiers, flag gate,
//! activity rows, object routes (Bearer-only auth), factory reset, and the
//! `OXIDEAN_VCS_*` bootstrap. Runs through the real axum router.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

/// `OXIDEAN_VCS_OBJECTS_DIR` is read by `AppState::new` — env is process-wide,
/// so the shared lock serializes this construction against other env-mutating
/// tests.
async fn test_app(
    db: Database,
    repos_dir: std::path::PathBuf,
    vcs_dir: std::path::PathBuf,
) -> axum::Router {
    let state = test_state(db, repos_dir, vcs_dir).await;
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

async fn test_state(
    db: Database,
    repos_dir: std::path::PathBuf,
    vcs_dir: std::path::PathBuf,
) -> AppState {
    let _guard = support::lock_admin_env().await;
    std::env::set_var("OXIDEAN_VCS_OBJECTS_DIR", &vcs_dir);
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir);
    std::env::remove_var("OXIDEAN_VCS_OBJECTS_DIR");
    state
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
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":""}}}}"#
    );
    let (status, v) = rpc_json(app, &body, cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

fn vcs_rpc(proc: &str, owner: &str, name: &str, extra: &str) -> String {
    let input = if extra.is_empty() {
        format!(r#"{{"owner":"{owner}","name":"{name}"}}"#)
    } else {
        format!(r#"{{"owner":"{owner}","name":"{name}",{extra}}}"#)
    };
    format!(r#"{{"procedure":"{proc}","input":{input}}}"#)
}

async fn enable_vcs(app: &axum::Router, cookie: &str, owner: &str, name: &str) {
    let (_, v) = rpc_json(
        app,
        &vcs_rpc("vcs.repo_enable", owner, name, r#""enabled":true"#),
        cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "vcs.repo_enable — {v}");
    assert_eq!(v["data"]["enabled"], true, "{v}");
}

/// Mint a classic PAT (`repo` scope) and return the one-time token.
async fn mint_pat(app: &axum::Router, cookie: &str, name: &str) -> String {
    let body = format!(
        r#"{{"procedure":"pat.createClassic","input":{{"name":"{name}","scopes":["repo"]}}}}"#
    );
    let (_, v) = rpc_json(app, &body, cookie).await;
    assert_eq!(v["ok"], true, "pat.createClassic — {v}");
    v["data"]["token"].as_str().expect("token").to_string()
}

/// Seed one changeset; returns its `cs_…` id.
async fn seed_changeset(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    name: &str,
    path: &str,
    content_b64: &str,
) -> String {
    let body = vcs_rpc(
        "vcs.changeset_create",
        owner,
        name,
        &format!(
            r#""parents":[],"message":"seed","changes":[{{"path":"{path}","kind":"add","mode":"100644","content_b64":"{content_b64}"}}]"#
        ),
    );
    let (_, v) = rpc_json(app, &body, cookie).await;
    assert_eq!(v["ok"], true, "vcs.changeset_create — {v}");
    v["data"]["changeset_id"].as_str().unwrap().to_string()
}

fn object_put_req(owner: &str, name: &str, bearer: Option<&str>, body: &[u8]) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri(format!("/api/vcs/{owner}/{name}/objects"))
        .header("content-type", "application/octet-stream");
    if let Some(t) = bearer {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::from(body.to_vec())).unwrap()
}

fn object_get_req(owner: &str, name: &str, hash: &str, bearer: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method("GET")
        .uri(format!("/api/vcs/{owner}/{name}/objects/{hash}"));
    if let Some(t) = bearer {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::empty()).unwrap()
}

async fn err_json(res: axum::http::Response<Body>) -> (StatusCode, serde_json::Value) {
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, v)
}

fn b64(input: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(input)
}

// ---------------------------------------------------------------------------
// Flag gate — never-enabled denies everything (including `vcs.status`).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vcs_status_never_enabled_denied() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("vcs_flag.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos"), dir.path().join("vcs")).await;

    let (cookie, login_v) = signup_and_login(&app, "vcs@ex.com", "vcsown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "vrepo", "public").await;

    // No flag row → status itself errors `vcs.disabled` (AC1.2.1).
    let (_, v) = rpc_json(&app, &vcs_rpc("vcs.status", "vcsown", "vrepo", ""), &cookie).await;
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "vcs.disabled", "{v}");
    // Reads and writes deny identically on a never-enabled repo.
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc("vcs.ref_list", "vcsown", "vrepo", ""),
        &cookie,
    )
    .await;
    assert_eq!(v["error"]["code"], "vcs.disabled", "{v}");
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "vcs.changeset_create",
            "vcsown",
            "vrepo",
            r#""parents":[],"message":"x","changes":[]"#,
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["error"]["code"], "vcs.disabled", "{v}");
}

// ---------------------------------------------------------------------------
// repo_enable → status counts → write path → activity row.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vcs_enable_status_changeset_log_activity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("vcs_flow.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos"), dir.path().join("vcs")).await;

    let (cookie, login_v) = signup_and_login(&app, "flow@ex.com", "flowown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "flowrepo", "public").await;
    enable_vcs(&app, &cookie, "flowown", "flowrepo").await;

    // `vcs.repo_enable` wrote its activity row (all-null push columns).
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "repo.activity.list",
            "flowown",
            "flowrepo",
            r#""push_type":"vcs.repo_enable""#,
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    let rows = v["data"]["items"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{v}");
    assert!(rows[0]["ref_name"].is_null(), "{v}");
    assert!(
        rows[0]["before"].is_null() && rows[0]["after"].is_null(),
        "{v}"
    );
    assert!(rows[0]["commits_count"].is_null(), "{v}");

    // status: enabled, empty heads, zero counts.
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc("vcs.status", "flowown", "flowrepo", ""),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["enabled"], true, "{v}");
    assert_eq!(v["data"]["changeset_count"], 0, "{v}");

    // changeset_create → get → log → activity.
    let cs = seed_changeset(
        &app,
        &cookie,
        "flowown",
        "flowrepo",
        "a.txt",
        &b64(b"alpha"),
    )
    .await;
    assert!(cs.starts_with("cs_"), "{cs}");

    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "vcs.changeset_get",
            "flowown",
            "flowrepo",
            &format!(r#""id":"{cs}""#),
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["changeset_id"], cs);
    assert_eq!(v["data"]["message"], "seed");

    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "vcs.log",
            "flowown",
            "flowrepo",
            &format!(r#""from":"{cs}""#),
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["entries"][0]["changeset_id"], cs, "{v}");

    let (_, v) = rpc_json(
        &app,
        &vcs_rpc("vcs.status", "flowown", "flowrepo", ""),
        &cookie,
    )
    .await;
    assert_eq!(v["data"]["changeset_count"], 1, "{v}");
    assert!(v["data"]["object_count"].as_u64().unwrap() >= 1, "{v}");

    // C4 row: after = changeset id, commits_count = 1.
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "repo.activity.list",
            "flowown",
            "flowrepo",
            r#""push_type":"vcs.changeset_create""#,
        ),
        &cookie,
    )
    .await;
    let rows = v["data"]["items"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{v}");
    assert_eq!(rows[0]["after"], cs, "{v}");
    assert_eq!(rows[0]["commits_count"], 1, "{v}");
}

// ---------------------------------------------------------------------------
// enabled:false — reads still serve; writes deny `vcs.disabled`.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vcs_disabled_reads_pass_writes_deny() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("vcs_dis.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos"), dir.path().join("vcs")).await;

    let (cookie, login_v) = signup_and_login(&app, "dis@ex.com", "disown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "disrepo", "public").await;
    enable_vcs(&app, &cookie, "disown", "disrepo").await;
    let cs = seed_changeset(&app, &cookie, "disown", "disrepo", "f.txt", &b64(b"x")).await;

    // Disable — flag row now `enabled:false`.
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc("vcs.repo_enable", "disown", "disrepo", r#""enabled":false"#),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");

    // Reads still serve: status reports enabled:false, reads succeed.
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc("vcs.status", "disown", "disrepo", ""),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["enabled"], false, "{v}");
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "vcs.changeset_get",
            "disown",
            "disrepo",
            &format!(r#""id":"{cs}""#),
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "read on disabled repo — {v}");
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc("vcs.ref_list", "disown", "disrepo", ""),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "ref_list on disabled repo — {v}");

    // Writes deny.
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "vcs.changeset_create",
            "disown",
            "disrepo",
            &format!(r#""parents":["{cs}"],"message":"no","changes":[]"#),
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["error"]["code"], "vcs.disabled", "{v}");
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "vcs.ref_update",
            "disown",
            "disrepo",
            &format!(r#""ref":"main","expected_old":null,"new":"{cs}""#),
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["error"]["code"], "vcs.disabled", "{v}");
}

// ---------------------------------------------------------------------------
// ref_update CAS — create-only, conflict payload, delete.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vcs_ref_update_cas_conflict_and_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("vcs_cas.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos"), dir.path().join("vcs")).await;

    let (cookie, login_v) = signup_and_login(&app, "cas@ex.com", "casown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "casrepo", "public").await;
    enable_vcs(&app, &cookie, "casown", "casrepo").await;
    let cs = seed_changeset(&app, &cookie, "casown", "casrepo", "f.txt", &b64(b"x")).await;

    // Create-only (expected_old:null) succeeds.
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "vcs.ref_update",
            "casown",
            "casrepo",
            &format!(r#""ref":"main","expected_old":null,"new":"{cs}""#),
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["changeset_id"], cs);

    // Retried create is NOT idempotent — ref_conflict with data.actual.
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "vcs.ref_update",
            "casown",
            "casrepo",
            &format!(r#""ref":"main","expected_old":null,"new":"{cs}""#),
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "vcs.ref_conflict", "{v}");
    assert_eq!(v["error"]["data"]["actual"], cs, "{v}");

    // Delete (new:null) succeeds.
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "vcs.ref_update",
            "casown",
            "casrepo",
            &format!(r#""ref":"main","expected_old":"{cs}","new":null"#),
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert!(v["data"]["changeset_id"].is_null(), "{v}");
}

// ---------------------------------------------------------------------------
// Archived repo — writes deny `repo.archived`; reads pass.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vcs_archived_repo_denies_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("vcs_arc.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos"), dir.path().join("vcs")).await;

    let (cookie, login_v) = signup_and_login(&app, "arc@ex.com", "arcown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "arcrepo", "public").await;
    enable_vcs(&app, &cookie, "arcown", "arcrepo").await;

    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "repo.setArchived",
            "arcown",
            "arcrepo",
            r#""archived":true"#,
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");

    // Write-class: `repo.archived` before dispatch.
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc(
            "vcs.changeset_create",
            "arcown",
            "arcrepo",
            r#""parents":[],"message":"x","changes":[]"#,
        ),
        &cookie,
    )
    .await;
    assert_eq!(v["error"]["code"], "repo.archived", "{v}");
    // Read-class still serves.
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc("vcs.ref_list", "arcown", "arcrepo", ""),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "read on archived — {v}");
}

// ---------------------------------------------------------------------------
// Object routes — PUT/GET round-trip, auth matrix, flag gates.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vcs_object_route_put_get_roundtrip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("vcs_obj.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos"), dir.path().join("vcs")).await;

    let (cookie, login_v) = signup_and_login(&app, "obj@ex.com", "objown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "objrepo", "public").await;
    enable_vcs(&app, &cookie, "objown", "objrepo").await;
    let pat = mint_pat(&app, &cookie, "obj-pat").await;

    // POST raw bytes → {ok:true,data:{hash:"sha256:…"}}.
    let bytes = b"hello vcs object".to_vec();
    let expected = format!("sha256:{}", oxidean_api::auth::session::sha256_hex(&bytes));
    let res = app
        .clone()
        .oneshot(object_put_req("objown", "objrepo", Some(&pat), &bytes))
        .await
        .unwrap();
    let (status, v) = err_json(res).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["hash"], expected, "{v}");

    // Dedup no-op — re-POST same bytes returns the same hash.
    let res = app
        .clone()
        .oneshot(object_put_req("objown", "objrepo", Some(&pat), &bytes))
        .await
        .unwrap();
    let (status, v) = err_json(res).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["data"]["hash"], expected);

    // GET → 200 octet-stream + immutable cache + exact bytes.
    let res = app
        .clone()
        .oneshot(object_get_req("objown", "objrepo", &expected, Some(&pat)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()[header::CONTENT_TYPE],
        "application/octet-stream"
    );
    assert_eq!(
        res.headers()[header::CACHE_CONTROL],
        "immutable, max-age=31536000"
    );
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), bytes.as_slice());

    // Unknown hash → 404 vcs.not_found envelope.
    let missing = format!("sha256:{}", "0".repeat(64));
    let res = app
        .clone()
        .oneshot(object_get_req("objown", "objrepo", &missing, Some(&pat)))
        .await
        .unwrap();
    let (status, v) = err_json(res).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(v["error"]["code"], "vcs.not_found", "{v}");
}

#[tokio::test]
async fn vcs_object_route_cookie_denied() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("vcs_ck.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos"), dir.path().join("vcs")).await;

    let (cookie, login_v) = signup_and_login(&app, "ck@ex.com", "ckown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "ckrepo", "public").await;
    enable_vcs(&app, &cookie, "ckown", "ckrepo").await;

    // Cookie-only POST → 401 vcs.unauthenticated (session cookie never counts).
    let mut req = object_put_req("ckown", "ckrepo", None, b"x");
    req.headers_mut()
        .insert(header::COOKIE, cookie.parse().unwrap());
    let res = app.clone().oneshot(req).await.unwrap();
    let (status, v) = err_json(res).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{v}");
    assert_eq!(v["error"]["code"], "vcs.unauthenticated", "{v}");

    // Cookie-only GET → 401 as well.
    let hash = format!("sha256:{}", "a".repeat(64));
    let mut req = object_get_req("ckown", "ckrepo", &hash, None);
    req.headers_mut()
        .insert(header::COOKIE, cookie.parse().unwrap());
    let res = app.clone().oneshot(req).await.unwrap();
    let (status, v) = err_json(res).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{v}");
    assert_eq!(v["error"]["code"], "vcs.unauthenticated", "{v}");

    // No credentials at all → 401.
    let res = app
        .clone()
        .oneshot(object_get_req("ckown", "ckrepo", &hash, None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn vcs_object_route_flag_gates() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("vcs_of.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos"), dir.path().join("vcs")).await;

    let (cookie, login_v) = signup_and_login(&app, "of@ex.com", "ofown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "ofrepo", "public").await;
    let pat = mint_pat(&app, &cookie, "of-pat").await;

    // Never-enabled repo → 403 vcs.disabled on both routes.
    let res = app
        .clone()
        .oneshot(object_put_req("ofown", "ofrepo", Some(&pat), b"x"))
        .await
        .unwrap();
    let (status, v) = err_json(res).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(v["error"]["code"], "vcs.disabled", "{v}");

    enable_vcs(&app, &cookie, "ofown", "ofrepo").await;

    // Put an object while enabled.
    let bytes = b"flag-gated".to_vec();
    let res = app
        .clone()
        .oneshot(object_put_req("ofown", "ofrepo", Some(&pat), &bytes))
        .await
        .unwrap();
    let (_, v) = err_json(res).await;
    let hash = v["data"]["hash"].as_str().unwrap().to_string();

    // enabled:false → GET still serves (read), POST denies (write).
    let (_, v) = rpc_json(
        &app,
        &vcs_rpc("vcs.repo_enable", "ofown", "ofrepo", r#""enabled":false"#),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");

    let res = app
        .clone()
        .oneshot(object_get_req("ofown", "ofrepo", &hash, Some(&pat)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "GET serves on enabled:false");

    let res = app
        .clone()
        .oneshot(object_put_req("ofown", "ofrepo", Some(&pat), b"more"))
        .await
        .unwrap();
    let (status, v) = err_json(res).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(v["error"]["code"], "vcs.disabled", "{v}");
}

#[tokio::test]
async fn vcs_object_route_forbidden_capability() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("vcs_fb.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos"), dir.path().join("vcs")).await;

    let (cookie, login_v) = signup_and_login(&app, "own@ex.com", "repoown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "shared", "public").await;
    enable_vcs(&app, &cookie, "repoown", "shared").await;

    // Second verified user — public repo → reads fine, writes denied (403).
    let (cookie2, login2) = signup_and_login(&app, "other@ex.com", "otherusr").await;
    verify_user(&db, login2["data"]["id"].as_str().unwrap()).await;
    let pat2 = mint_pat(&app, &cookie2, "other-pat").await;

    let res = app
        .clone()
        .oneshot(object_put_req("repoown", "shared", Some(&pat2), b"x"))
        .await
        .unwrap();
    let (status, v) = err_json(res).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["error"]["code"], "vcs.forbidden", "{v}");
}

// ---------------------------------------------------------------------------
// Factory reset — DatabaseAndRepositories sweeps the VCS objects dir.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vcs_factory_reset_wipes_objects_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("vcs_fr.db").display());
    let vcs_dir = dir.path().join("vcs");
    let object_file = vcs_dir.join("ab").join("cd").join("deadbeef");

    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    // Sys-admin for the reset procedure.
    let hash = oxidean_api::auth::hash_password_str("password1").expect("hash");
    db.create_user(
        "u-admin",
        "vcsadmin@ex.com",
        "vcsadmin",
        Some(&hash),
        "Admin",
        "",
        None,
        oxidean_core::Role::SysAdmin,
    )
    .await
    .expect("admin");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at("u-admin", &now)
        .await
        .expect("verify");

    let app = test_app(db, dir.path().join("repos"), vcs_dir.clone()).await;
    // Seed a stored object AFTER the store is marker-initialized — seeding
    // first would make the dir foreign → `vcs: None` → nothing to wipe.
    tokio::fs::create_dir_all(object_file.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&object_file, b"blob").await.unwrap();
    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"vcsadmin@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    let cookie = session_cookie_from_response(&login);

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
    assert!(
        !object_file.exists(),
        "factory_reset must wipe the VCS objects dir"
    );
}

// ---------------------------------------------------------------------------
// Bootstrap — a foreign/refused objects dir degrades to `vcs: None` (no panic);
// procedures then answer `vcs.disabled` exactly like an unenabled repo.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vcs_appstate_foreign_objects_dir_disables() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vcs_dir = dir.path().join("foreign");
    // Foreign content with no marker → the store refuses → `vcs: None`.
    std::fs::create_dir_all(&vcs_dir).unwrap();
    std::fs::write(vcs_dir.join("unrelated.txt"), b"x").unwrap();

    let url = format!("sqlite:{}", dir.path().join("vcs_bad.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let state = test_state(db, dir.path().join("repos"), vcs_dir).await;
    assert!(
        state.vcs.is_none(),
        "foreign-content objects dir must refuse (vcs: None)"
    );
}

// ---------------------------------------------------------------------------
// GC job — `Some(slot)` + `gc_interval_secs > 0` ⇒ the spawned interval task
// actually invokes `gc_once` (observable: a stale `tmp/*.tmp` is reclaimed).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vcs_gc_job_invokes_gc_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("vcs_gc.db").display());
    let vcs_dir = dir.path().join("vcs");
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let _guard = support::lock_admin_env().await;
    std::env::set_var("OXIDEAN_VCS_OBJECTS_DIR", &vcs_dir);
    std::env::set_var("OXIDEAN_VCS_GC_INTERVAL_SECS", "1");
    std::env::set_var("OXIDEAN_VCS_GC_GRACE_SECS", "1");
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_repos_dir(dir.path().join("repos"));
    std::env::remove_var("OXIDEAN_VCS_OBJECTS_DIR");
    std::env::remove_var("OXIDEAN_VCS_GC_INTERVAL_SECS");
    std::env::remove_var("OXIDEAN_VCS_GC_GRACE_SECS");
    let slot = state.vcs.clone().expect("vcs slot configured");

    // A `tmp/*.tmp` past the 1s grace is reclaimed — backdate the mtime so
    // the first (immediate) tick must remove it.
    let stale = vcs_dir.join("tmp").join("stale-abandoned.tmp");
    tokio::fs::write(&stale, b"partial").await.unwrap();
    std::fs::File::options()
        .write(true)
        .open(&stale)
        .unwrap()
        .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(120))
        .unwrap();

    oxidean_api::jobs::spawn_background_jobs(
        db,
        state.git.clone(),
        dir.path().join("repos"),
        dir.path().join("lfs"),
        dir.path().join("pkgs"),
        oxidean_api::jobs::JobConfig {
            orphan_interval: std::time::Duration::ZERO,
            gc_interval: std::time::Duration::ZERO,
            lfs_gc_interval: std::time::Duration::ZERO,
            packages_gc_interval: std::time::Duration::ZERO,
        },
        Some(slot),
    );

    for _ in 0..50 {
        if !stale.exists() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("vcs gc job never reclaimed the stale tmp staging file");
}

// ---------------------------------------------------------------------------
// Route aliases classify but are NOT public procedures.
// ---------------------------------------------------------------------------

#[test]
fn vcs_object_aliases_classify_but_not_procedures() {
    use oxidean_core::proc_classes::{classify_pat_proc, PatProc};

    assert_eq!(classify_pat_proc("vcs.object_get"), PatProc::RepoRead);
    assert_eq!(classify_pat_proc("vcs.object_put"), PatProc::RepoWrite);
    for name in ["vcs.object_get", "vcs.object_put"] {
        assert!(
            !oxidean_api::rpc::PROCEDURES.contains(&name),
            "{name} must not be a public RPC procedure"
        );
    }
    // The 17 real procedures are all registered.
    for name in [
        "vcs.repo_enable",
        "vcs.status",
        "vcs.changeset_create",
        "vcs.changeset_get",
        "vcs.log",
        "vcs.diff",
        "vcs.tree",
        "vcs.file_get",
        "vcs.blame",
        "vcs.path_history",
        "vcs.merge_base",
        "vcs.merge",
        "vcs.rebase",
        "vcs.ref_list",
        "vcs.ref_update",
        "vcs.git_import",
        "vcs.git_export",
    ] {
        assert!(
            oxidean_api::rpc::PROCEDURES.contains(&name),
            "{name} missing from PROCEDURES"
        );
    }
}
