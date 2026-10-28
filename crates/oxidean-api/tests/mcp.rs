//! AGT-01: streamable-HTTP MCP endpoint — JSON-RPC framing, session/PAT auth,
//! tool dispatch over the typed RPC layer, scope denial, and no private-repo
//! existence leaks to anonymous callers.

mod support;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::auth::hash_password_str;
use oxidean_api::auth::session::sha256_hex;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, GitBackend};
use tower::ServiceExt;
use uuid::Uuid;

async fn test_app(db: Database) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development");
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

async fn test_app_repos(db: Database, repos_dir: std::path::PathBuf) -> axum::Router {
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

fn mcp_req(body: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/mcp")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn mcp_req_bearer(body: &serde_json::Value, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/mcp")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn mcp_req_cookie(body: &serde_json::Value, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/mcp")
        .header("content-type", "application/json")
        .header("cookie", cookie)
        .body(Body::from(body.to_string()))
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

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
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

async fn mcp_json(app: &axum::Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("non-JSON MCP response: {bytes:?}"));
    (status, v)
}

fn rpc_call(id: i64, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn tool_call(id: i64, name: &str, args: serde_json::Value) -> serde_json::Value {
    rpc_call(
        id,
        "tools/call",
        serde_json::json!({"name": name, "arguments": args}),
    )
}

/// initialize → tools/list → tools/call with a classic `repo` PAT.
#[tokio::test]
async fn mcp_initialize_list_and_call_with_pat() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_basic.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "mcp@ex.com", "mcpuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;
    db.insert_repository("r-mcp-1", &user_id, "user", "demo", "public", "", "main")
        .await
        .expect("insert repo");

    let (_, pat_v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createClassic","input":{"name":"mcp","scopes":["repo"]}}"#,
        &cookie,
    )
    .await;
    let token = pat_v["data"]["token"].as_str().expect("token").to_string();

    // initialize — protocol negotiation.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &rpc_call(
                1,
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "0"},
                }),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["id"], 1);
    assert_eq!(v["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(v["result"]["serverInfo"]["name"], "oxidean");

    // Notification → 202 + empty body.
    let res = app
        .clone()
        .oneshot(mcp_req_bearer(
            &serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert!(bytes.is_empty(), "notification must have empty body");

    // tools/list — exposes the repo/issue/pull/actions/packages/search surface.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(&rpc_call(2, "tools/list", serde_json::json!({})), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let tools = v["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for expected in [
        "repo_list",
        "repo_get",
        "issue_list",
        "issue_get",
        "issue_create",
        "pull_list",
        "pull_get",
        "actions_list_runs",
        "actions_get_run",
        "packages_list",
        "search_repos",
        "search_issues",
        "search_code",
    ] {
        assert!(
            names.contains(&expected),
            "missing tool {expected} — {names:?}"
        );
    }

    // tools/call repo_get — classic `repo` scope covers it.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(
                3,
                "repo_get",
                serde_json::json!({"owner": "mcpuser", "name": "demo"}),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");
    let text = v["result"]["content"][0]["text"].as_str().expect("text");
    assert!(text.contains("\"demo\""), "repo content — {text}");
    assert_eq!(v["result"]["structuredContent"]["name"], "demo");

    // tools/call issue_create — write path through the same PAT.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(
                4,
                "issue_create",
                serde_json::json!({"owner": "mcpuser", "name": "demo", "title": "first", "body": "hi"}),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");
    assert_eq!(v["result"]["structuredContent"]["number"], 1);

    // resources/read — issue body via URI template shape.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &rpc_call(
                5,
                "resources/read",
                serde_json::json!({"uri": "oxidean://repo/mcpuser/demo/issue/1"}),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let text = v["result"]["contents"][0]["text"]
        .as_str()
        .expect("issue text");
    assert!(text.contains("Issue #1: first"), "{text}");
    assert!(text.contains("hi"), "{text}");

    // Unknown tool → -32602.
    let (_, v) = mcp_json(
        &app,
        mcp_req_bearer(&tool_call(6, "nope_tool", serde_json::json!({})), &token),
    )
    .await;
    assert_eq!(v["error"]["code"], -32602, "{v}");

    // Unknown method → -32601.
    let (_, v) = mcp_json(
        &app,
        mcp_req_bearer(&rpc_call(7, "bogus/method", serde_json::json!({})), &token),
    )
    .await;
    assert_eq!(v["error"]["code"], -32601, "{v}");
}

/// Session-cookie auth path (no Authorization header).
#[tokio::test]
async fn mcp_session_cookie_repo_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_cookie.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "ck@ex.com", "ckuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;
    db.insert_repository("r-ck-1", &user_id, "user", "mine", "private", "", "main")
        .await
        .expect("insert repo");

    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(&tool_call(1, "repo_list", serde_json::json!({})), &cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");
    let text = v["result"]["content"][0]["text"].as_str().expect("text");
    assert!(text.contains("\"mine\""), "own repo in list — {text}");
}

/// Fine-grained PAT scopes: `contents:read` may read but not write; a private
/// repo outside `repository_ids` answers `repo.not_found` (no existence leak).
#[tokio::test]
async fn mcp_fine_grained_scope_denial() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_fg.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "fg@ex.com", "fguser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;
    let covered = db
        .insert_repository(
            "r-fg-cov", &user_id, "user", "covered", "private", "", "main",
        )
        .await
        .expect("covered repo");
    db.insert_repository(
        "r-fg-out", &user_id, "user", "outside", "private", "", "main",
    )
    .await
    .expect("outside repo");

    let (_, pat_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pat.createFineGrained","input":{{"name":"fg-read","repo_access":"selected","contents":"read","repository_ids":["{}"]}}}}"#,
            covered.id
        ),
        &cookie,
    )
    .await;
    assert_eq!(pat_v["ok"], true, "{pat_v}");
    let token = pat_v["data"]["token"].as_str().expect("token").to_string();

    // Read on the covered repo → OK.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(
                1,
                "repo_get",
                serde_json::json!({"owner": "fguser", "name": "covered"}),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");

    // Write with contents:read → isError + scope denial.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(
                2,
                "issue_create",
                serde_json::json!({"owner": "fguser", "name": "covered", "title": "denied"}),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], true, "{v}");
    let text = v["result"]["content"][0]["text"]
        .as_str()
        .expect("err text");
    assert!(text.contains("insufficient_scope"), "{text}");

    // Private repo outside the selection → repo.not_found, same as a missing repo.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(
                3,
                "repo_get",
                serde_json::json!({"owner": "fguser", "name": "outside"}),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], true, "{v}");
    let text = v["result"]["content"][0]["text"]
        .as_str()
        .expect("err text");
    assert!(text.contains("repo.not_found"), "{text}");

    // `packages_list` without package scope → scope denial (repo scope ≠ packages).
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(4, "packages_list", serde_json::json!({"owner": "fguser"})),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], true, "{v}");
    let text = v["result"]["content"][0]["text"]
        .as_str()
        .expect("err text");
    assert!(text.contains("insufficient_scope"), "{text}");
}

/// Anonymous callers: initialize/tools/list work; a private repo tool call
/// returns isError `repo.not_found` — identical to a nonexistent repo (no leak).
#[tokio::test]
async fn mcp_anonymous_private_repo_no_leak() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_anon.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_cookie, login_v) = signup_and_login(&app, "own@ex.com", "ownuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    db.insert_repository(
        "r-anon-1", &user_id, "user", "secret", "private", "", "main",
    )
    .await
    .expect("insert private repo");

    // initialize + tools/list are anonymous-safe.
    let (status, v) = mcp_json(
        &app,
        mcp_req(&rpc_call(
            1,
            "initialize",
            serde_json::json!({"protocolVersion": "2025-03-26"}),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["protocolVersion"], "2025-03-26");
    let (status, v) = mcp_json(
        &app,
        mcp_req(&rpc_call(2, "tools/list", serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert!(v["result"]["tools"]
        .as_array()
        .map(|t| !t.is_empty())
        .unwrap_or(false));

    // Private repo → isError repo.not_found.
    let (status, v) = mcp_json(
        &app,
        mcp_req(&tool_call(
            3,
            "repo_get",
            serde_json::json!({"owner": "ownuser", "name": "secret"}),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], true, "{v}");
    let secret_err = v["result"]["content"][0]["text"]
        .as_str()
        .expect("err")
        .to_string();
    assert!(secret_err.contains("repo.not_found"), "{secret_err}");
    assert!(
        !secret_err.contains("secret"),
        "must not echo repo name — {secret_err}"
    );

    // Nonexistent repo → identical error shape (no oracle).
    let (_, v) = mcp_json(
        &app,
        mcp_req(&tool_call(
            4,
            "repo_get",
            serde_json::json!({"owner": "ownuser", "name": "ghost"}),
        )),
    )
    .await;
    let ghost_err = v["result"]["content"][0]["text"]
        .as_str()
        .expect("err")
        .to_string();
    assert!(ghost_err.contains("repo.not_found"), "{ghost_err}");

    // Write as anonymous → auth-required tool error, not a 401 (no credential presented).
    let (status, v) = mcp_json(
        &app,
        mcp_req(&tool_call(
            5,
            "issue_create",
            serde_json::json!({"owner": "ownuser", "name": "secret", "title": "x"}),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], true, "{v}");
}

/// Invalid Bearer token → HTTP 401 + `WWW-Authenticate: Bearer`.
#[tokio::test]
async fn mcp_invalid_bearer_returns_401() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_401.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;

    let res = app
        .clone()
        .oneshot(mcp_req_bearer(
            &rpc_call(1, "ping", serde_json::json!({})),
            "oxidean_pat_deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let www = res
        .headers()
        .get("www-authenticate")
        .expect("WWW-Authenticate")
        .to_str()
        .unwrap();
    assert!(www.starts_with("Bearer"), "{www}");
}

/// GET /api/mcp → 405 (no standalone SSE stream); POST-only endpoint.
#[tokio::test]
async fn mcp_get_returns_405() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_get.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let app = test_app(db).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/mcp")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
}

/// AGT-03: instance gate — admin override off → 404 `mcp.disabled` for POST and
/// GET; clearing the override falls back to the env default (on).
#[tokio::test]
async fn mcp_instance_toggle_gate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_gate.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let app = test_app(db.clone()).await;

    // Default: migration seeds no override → env default on.
    let (status, v) = mcp_json(&app, mcp_req(&rpc_call(1, "ping", serde_json::json!({})))).await;
    assert_eq!(status, StatusCode::OK, "{v}");

    // Admin override off → 404 + mcp.disabled (POST and GET alike).
    db.update_mcp_settings(Some(false))
        .await
        .expect("disable mcp");
    let (status, v) = mcp_json(&app, mcp_req(&rpc_call(2, "ping", serde_json::json!({})))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{v}");
    let msg = v["error"]["message"].as_str().unwrap_or_default();
    assert!(msg.contains("mcp.disabled"), "{msg}");
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/mcp")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Clear override → env default applies again.
    db.update_mcp_settings(None).await.expect("clear override");
    let (status, v) = mcp_json(&app, mcp_req(&rpc_call(3, "ping", serde_json::json!({})))).await;
    assert_eq!(status, StatusCode::OK, "{v}");

    // Env default off (OXIDEAN_MCP_ENABLED=false) + no override → 404.
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_mcp_enabled(false);
    let cors = build_cors("development", None).expect("cors");
    let app_off = router_with_state(state, cors);
    let (status, v) = mcp_json(
        &app_off,
        mcp_req(&rpc_call(4, "ping", serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{v}");

    // Admin override still wins over a `false` env default (env is the
    // default, not a hard kill switch — same split as admin.lfs.*).
    db.update_mcp_settings(Some(true))
        .await
        .expect("enable mcp");
    let (status, v) = mcp_json(
        &app_off,
        mcp_req(&rpc_call(5, "ping", serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
}

/// AGT-03: `oxidean_oat_` bearer tokens dispatch to the OAuth seam — until the
/// OAuth resolver lands (API-03) they fail closed with 401 + WWW-Authenticate.
#[tokio::test]
async fn mcp_oat_bearer_fails_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_oat.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let app = test_app(db).await;

    let res = app
        .clone()
        .oneshot(mcp_req_bearer(
            &rpc_call(1, "ping", serde_json::json!({})),
            "oxidean_oat_deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let www = res
        .headers()
        .get("www-authenticate")
        .expect("WWW-Authenticate")
        .to_str()
        .unwrap();
    assert!(www.starts_with("Bearer"), "{www}");
}

/// AGT-03: `admin.mcp.*` RPCs — sys-admin reads/toggles the endpoint state;
/// non-admins are refused with `admin.forbidden`.
#[tokio::test]
async fn admin_mcp_settings_rpc() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_admin.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;

    let hash = hash_password_str("password1").expect("hash");
    let admin_id = Uuid::new_v4().to_string();
    db.create_user(
        &admin_id,
        "mcp-admin@ex.com",
        "mcpadmin",
        Some(&hash),
        "Admin",
        "",
        None,
        oxidean_core::Role::SysAdmin,
    )
    .await
    .expect("create admin");

    let app = test_app(db.clone()).await;

    // Non-admin → admin.forbidden.
    let res = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"non@ex.com","username":"nonadmin","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let _ = res.into_body().collect().await;
    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"non@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    let non_admin_cookie = session_cookie_from_response(&login);
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.mcp.getSettings","input":{}}"#,
            &non_admin_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Sys-admin login.
    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"mcp-admin@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    let cookie = session_cookie_from_response(&login);

    // getSettings — env default on, no override.
    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"admin.mcp.getSettings","input":{}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["enabled"], true);
    assert_eq!(v["data"]["enabled_overridden"], false);

    // updateSettings enabled:false → endpoint gates off.
    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"admin.mcp.updateSettings","input":{"enabled":false}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["enabled"], false);
    assert_eq!(v["data"]["enabled_overridden"], true);
    let (status, _) = mcp_json(&app, mcp_req(&rpc_call(9, "ping", serde_json::json!({})))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // clear_overrides → env default again.
    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"admin.mcp.updateSettings","input":{"clear_overrides":true}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["enabled"], true);
    assert_eq!(v["data"]["enabled_overridden"], false);
    let (status, _) = mcp_json(&app, mcp_req(&rpc_call(10, "ping", serde_json::json!({})))).await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Framing, negotiation, list surfaces
// ---------------------------------------------------------------------------

fn mcp_req_raw(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/mcp")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn mcp_req_auth(body: &serde_json::Value, authorization: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/mcp")
        .header("content-type", "application/json")
        .header("authorization", authorization.to_owned())
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Simulate a request arriving via the private web→api hop: `ConnectInfo`
/// stamps a non-public peer so the edge honors X-Forwarded-For (direct
/// exposure ignores client-supplied XFF — M-1).
fn mcp_req_bearer_xff(body: &serde_json::Value, token: &str, ip: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/mcp")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .header("x-forwarded-for", ip.to_owned())
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            40000,
        )))
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// JSON-RPC framing edges — parse error, batch rejection, version check, the
/// no-reply cases for notifications and client responses.
#[tokio::test]
async fn mcp_framing_edges() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_framing.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let app = test_app(db).await;

    // Unparseable body → HTTP 400 + -32700.
    let (status, v) = mcp_json(&app, mcp_req_raw("{\"jsonrpc\": \"2.0\",")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], -32700, "{v}");
    assert_eq!(v["id"], serde_json::Value::Null);

    // Non-object payloads (batch array, scalar) → 400 + -32600.
    for raw in [
        "[]",
        "[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}]",
        "42",
        "\"hi\"",
    ] {
        let (status, v) = mcp_json(&app, mcp_req_raw(raw)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{raw} → {v}");
        assert_eq!(v["error"]["code"], -32600, "{raw} → {v}");
    }

    // `jsonrpc` present but not "2.0" → 400 + -32600.
    for raw in [
        r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
        r#"{"jsonrpc":2.0,"id":1,"method":"ping"}"#,
    ] {
        let (status, v) = mcp_json(&app, mcp_req_raw(raw)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{raw} → {v}");
        assert_eq!(v["error"]["code"], -32600, "{raw} → {v}");
    }

    // Missing `method` without result/error keys → 400 + -32600.
    for raw in [
        r#"{"jsonrpc":"2.0","id":9}"#,
        r#"{"jsonrpc":"2.0","id":9,"method":42}"#,
    ] {
        let (status, v) = mcp_json(&app, mcp_req_raw(raw)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{raw} → {v}");
        assert_eq!(v["error"]["code"], -32600, "{raw} → {v}");
        assert_eq!(v["id"], 9);
    }

    // Client response frames (result/error, no method) → 202 no body.
    for raw in [
        r#"{"jsonrpc":"2.0","id":3,"result":{}}"#,
        r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32601,"message":"x"}}"#,
    ] {
        let res = app.clone().oneshot(mcp_req_raw(raw)).await.unwrap();
        assert_eq!(res.status(), StatusCode::ACCEPTED, "{raw}");
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        assert!(bytes.is_empty(), "{raw} must have empty body");
    }

    // Notification-style requests (no id) → 202 for ANY method.
    let res = app
        .clone()
        .oneshot(mcp_req_raw(r#"{"jsonrpc":"2.0","method":"bogus/method"}"#))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);

    // `jsonrpc` key absent entirely → still served (lenient framing).
    let (status, v) = mcp_json(&app, mcp_req_raw(r#"{"id":7,"method":"ping"}"#)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"], serde_json::json!({}));

    // String and null ids echo back unchanged.
    let (status, v) = mcp_json(
        &app,
        mcp_req_raw(r#"{"jsonrpc":"2.0","id":"req-42","method":"ping"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["id"], "req-42");
    let (status, v) = mcp_json(
        &app,
        mcp_req_raw(r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["id"], serde_json::Value::Null);
}

/// initialize negotiates: unknown or missing protocol versions fall back to the
/// newest supported; every advertised version echoes; capabilities advertised.
#[tokio::test]
async fn mcp_initialize_negotiation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_init.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let app = test_app(db).await;

    for version in ["2024-11-05", "2025-03-26", "2025-06-18"] {
        let (status, v) = mcp_json(
            &app,
            mcp_req(&rpc_call(
                1,
                "initialize",
                serde_json::json!({
                    "protocolVersion": version,
                    "capabilities": {},
                    "clientInfo": {"name": "t", "version": "0"},
                }),
            )),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["result"]["protocolVersion"], version, "{v}");
    }

    // Unknown / absent version → newest supported.
    for params in [
        serde_json::json!({"protocolVersion": "1999-01-01"}),
        serde_json::json!({}),
    ] {
        let (status, v) = mcp_json(&app, mcp_req(&rpc_call(2, "initialize", params))).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["result"]["protocolVersion"], "2025-06-18", "{v}");
    }

    let (status, v) = mcp_json(
        &app,
        mcp_req(&rpc_call(3, "initialize", serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["serverInfo"]["name"], "oxidean");
    assert!(v["result"]["serverInfo"]["version"].is_string());
    assert_eq!(v["result"]["capabilities"]["tools"]["listChanged"], false);
    assert_eq!(
        v["result"]["capabilities"]["resources"]["listChanged"],
        false
    );
    assert_eq!(v["result"]["capabilities"]["prompts"]["listChanged"], false);
    assert!(v["result"]["instructions"]
        .as_str()
        .expect("instructions")
        .contains("oxidean_pat_"));
}

/// tools/list shape: every entry exposes name + description + an object input
/// schema; resources/prompts list surfaces answer their documented shapes.
#[tokio::test]
async fn mcp_list_surfaces() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_lists.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let app = test_app(db).await;

    let (status, v) = mcp_json(
        &app,
        mcp_req(&rpc_call(1, "tools/list", serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let tools = v["result"]["tools"].as_array().expect("tools");
    assert!(tools.len() >= 16, "tool registry shrank: {}", tools.len());
    let mut names = std::collections::HashSet::new();
    for t in tools {
        let name = t["name"].as_str().expect("tool name");
        assert!(names.insert(name.to_string()), "duplicate {name}");
        assert!(
            t["description"]
                .as_str()
                .map(|d| !d.is_empty())
                .unwrap_or(false),
            "{name}"
        );
        assert_eq!(t["inputSchema"]["type"], "object", "{name}");
    }

    let (status, v) = mcp_json(
        &app,
        mcp_req(&rpc_call(2, "resources/list", serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["resources"], serde_json::json!([]));

    let (status, v) = mcp_json(
        &app,
        mcp_req(&rpc_call(
            3,
            "resources/templates/list",
            serde_json::json!({}),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let tpls = v["result"]["resourceTemplates"]
        .as_array()
        .expect("templates");
    assert_eq!(tpls.len(), 3, "{v}");
    for t in tpls {
        assert!(t["uriTemplate"]
            .as_str()
            .expect("uriTemplate")
            .starts_with("oxidean://repo/"));
    }

    let (status, v) = mcp_json(
        &app,
        mcp_req(&rpc_call(4, "prompts/list", serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["prompts"], serde_json::json!([]));
}

/// tools/call parameter edges — `-32602` for malformed params; a null
/// `arguments` is the same as an empty object.
#[tokio::test]
async fn mcp_tool_call_param_edges() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_params.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, _login_v) = signup_and_login(&app, "pe@ex.com", "peuser").await;

    // Missing / non-string params.name → -32602.
    for params in [
        serde_json::json!({}),
        serde_json::json!({"name": 42}),
        serde_json::json!({"arguments": {}}),
    ] {
        let (_, v) = mcp_json(
            &app,
            mcp_req_cookie(&rpc_call(1, "tools/call", params), &cookie),
        )
        .await;
        assert_eq!(v["error"]["code"], -32602, "{v}");
    }

    // Non-object arguments → -32602.
    for args in [
        serde_json::json!("x"),
        serde_json::json!([]),
        serde_json::json!(5),
    ] {
        let (_, v) = mcp_json(
            &app,
            mcp_req_cookie(
                &rpc_call(
                    2,
                    "tools/call",
                    serde_json::json!({"name": "repo_list", "arguments": args}),
                ),
                &cookie,
            ),
        )
        .await;
        assert_eq!(v["error"]["code"], -32602, "{v}");
    }

    // Null arguments ≡ {} — repo_list runs for the authed user.
    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &rpc_call(
                3,
                "tools/call",
                serde_json::json!({"name": "repo_list", "arguments": null}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");
}

// ---------------------------------------------------------------------------
// Bearer auth edges + rate limiting
// ---------------------------------------------------------------------------

/// Presented credentials must validate: non-Bearer schemes, unknown prefixes,
/// empty tokens, expired/revoked PATs, and banned owners all → 401 with
/// `WWW-Authenticate`; a lowercase `bearer` scheme is accepted.
#[tokio::test]
async fn mcp_bearer_auth_edges() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_auth.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let ping = rpc_call(1, "ping", serde_json::json!({}));
    let assert_401 = |res: axum::http::Response<Body>, label: &str| {
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{label}");
        let www = res
            .headers()
            .get("www-authenticate")
            .expect("WWW-Authenticate")
            .to_str()
            .unwrap()
            .to_string();
        assert!(www.starts_with("Bearer"), "{label}: {www}");
    };

    // Non-Bearer scheme.
    let res = app
        .clone()
        .oneshot(mcp_req_auth(&ping, "Basic dXNlcjpwYXNz"))
        .await
        .unwrap();
    assert_401(res, "basic scheme");

    // Unrecognized token families.
    for token in [
        "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "oxidean_zzz_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "", // `Bearer ` with an empty token
    ] {
        let res = app
            .clone()
            .oneshot(mcp_req_bearer(&ping, token))
            .await
            .unwrap();
        assert_401(res, token);
    }

    // Valid PAT minted via RPC → 200, including lowercase `bearer`.
    let (cookie, login_v) = signup_and_login(&app, "ae@ex.com", "aeuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;
    let (_, pat_v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createClassic","input":{"name":"ok","scopes":["repo"]}}"#,
        &cookie,
    )
    .await;
    let good = pat_v["data"]["token"].as_str().expect("token").to_string();
    let (status, _) = mcp_json(&app, mcp_req_bearer(&ping, &good)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = mcp_json(&app, mcp_req_auth(&ping, &format!("bearer {good}"))).await;
    assert_eq!(status, StatusCode::OK, "lowercase bearer accepted");

    // Expired PAT row → 401.
    let expired_token = format!("oxidean_pat_{}", "e".repeat(40));
    db.create_pat(
        "pat-expired",
        &user_id,
        "classic",
        "expired",
        &expired_token[..15],
        &sha256_hex(expired_token.as_bytes()),
        Some(r#"["repo"]"#),
        None,
        None,
        None,
        Some("2020-01-01T00:00:00Z"),
        &[],
    )
    .await
    .expect("insert expired pat");
    let res = app
        .clone()
        .oneshot(mcp_req_bearer(&ping, &expired_token))
        .await
        .unwrap();
    assert_401(res, "expired pat");

    // Revoked PAT → 401.
    let revoked_token = format!("oxidean_pat_{}", "r".repeat(40));
    db.create_pat(
        "pat-revoked",
        &user_id,
        "classic",
        "revoked",
        &revoked_token[..15],
        &sha256_hex(revoked_token.as_bytes()),
        Some(r#"["repo"]"#),
        None,
        None,
        None,
        None,
        &[],
    )
    .await
    .expect("insert revoked pat");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.revoke_pat("pat-revoked", &now).await.expect("revoke");
    let res = app
        .clone()
        .oneshot(mcp_req_bearer(&ping, &revoked_token))
        .await
        .unwrap();
    assert_401(res, "revoked pat");

    // PAT whose owner is banned → 401.
    let (_, pat_v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createClassic","input":{"name":"ban","scopes":["repo"]}}"#,
        &cookie,
    )
    .await;
    let banned_token = pat_v["data"]["token"].as_str().expect("token").to_string();
    db.set_user_banned_at(&user_id, &now)
        .await
        .expect("ban user");
    let res = app
        .clone()
        .oneshot(mcp_req_bearer(&ping, &banned_token))
        .await
        .unwrap();
    assert_401(res, "banned owner pat");
}

/// Failed bearer auth shares the D-26 limiter with Smart HTTP: after 20 bad
/// attempts on one IP the next call → 429 + `Retry-After`; another IP is still
/// evaluated normally (per-IP buckets).
#[tokio::test]
async fn mcp_failed_auth_rate_limit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_rl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let app = test_app(db).await;

    let ping = rpc_call(1, "ping", serde_json::json!({}));
    let bad = "oxidean_zzz_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    for i in 0..20 {
        let res = app
            .clone()
            .oneshot(mcp_req_bearer_xff(&ping, bad, "10.9.8.7"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "attempt {i}");
        let _ = res.into_body().collect().await;
    }
    // 21st on the same IP → 429 + Retry-After, still a JSON-RPC error body.
    let res = app
        .clone()
        .oneshot(mcp_req_bearer_xff(&ping, bad, "10.9.8.7"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(res.headers().get("retry-after").is_some(), "Retry-After");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], -32000, "{v}");

    // A different IP is a fresh bucket → 401 (evaluated, not blocked).
    let res = app
        .clone()
        .oneshot(mcp_req_bearer_xff(&ping, bad, "10.9.8.8"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

/// A classic PAT without the `repo` scope passes authentication but fails the
/// tool-scope gate: repo tools → `mcp.insufficient_scope` isError, while
/// `packages_list` succeeds on a `package:read` grant.
#[tokio::test]
async fn mcp_classic_pat_scope_gating() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_classic.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "cs@ex.com", "csuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;
    db.insert_repository("r-cs-1", &user_id, "user", "cdemo", "public", "", "main")
        .await
        .expect("insert repo");

    let (_, pat_v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createClassic","input":{"name":"pkg","scopes":["package:read"]}}"#,
        &cookie,
    )
    .await;
    let token = pat_v["data"]["token"].as_str().expect("token").to_string();

    // repo_get without `repo` scope → isError insufficient_scope.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(
                1,
                "repo_get",
                serde_json::json!({"owner": "csuser", "name": "cdemo"}),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], true, "{v}");
    let text = v["result"]["content"][0]["text"].as_str().expect("err");
    assert!(text.contains("insufficient_scope"), "{text}");

    // packages_list with `package:read` → allowed.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(2, "packages_list", serde_json::json!({"owner": "csuser"})),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");
}

// ---------------------------------------------------------------------------
// Full tool surface + resources on a seeded repo
// ---------------------------------------------------------------------------

/// Seed `{repos_dir}/{owner}/{name}.git` with a text file and a binary blob.
async fn seed_repo_tree(repos_dir: &std::path::Path, owner: &str, name: &str) {
    let bare = repos_dir.join(owner).join(format!("{name}.git"));
    let git = CliGitBackend::new();
    git.init_bare(&bare, "main").await.expect("init bare");
    git.seed_commit(
        &bare,
        "main",
        "seed",
        &[
            ("README.md".into(), b"hello oxidean\n".to_vec()),
            (
                "src/main.rs".into(),
                b"fn main() { println!(\"hi\"); }\n".to_vec(),
            ),
            ("bin.dat".into(), vec![0x00, 0x01, 0xff, 0x00]),
        ],
    )
    .await
    .expect("seed commit");
}

/// Session-cookie caller across the whole tool surface on a seeded repo:
/// identity, repo, issue, pull, actions, packages, and search tools all run;
/// a bogus `run_id` lands as `isError` rather than a protocol failure.
#[tokio::test]
async fn mcp_full_tool_surface_on_seeded_repo() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_tools.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    let app = test_app_repos(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "ts@ex.com", "tsuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;
    let repo = db
        .insert_repository("r-ts-1", &user_id, "user", "toolbox", "public", "", "main")
        .await
        .expect("insert repo");
    seed_repo_tree(&repos_dir, "tsuser", "toolbox").await;
    db.insert_pull(
        "p-ts-1",
        &repo.id,
        1,
        "first pr",
        "pr body",
        &user_id,
        "main",
        "0".repeat(40).as_str(),
        &repo.id,
        "feature",
        "1".repeat(40).as_str(),
        false,
    )
    .await
    .expect("insert pull");

    // repo_list — no owner → own repos; with owner → listByOwner.
    for args in [
        serde_json::json!({}),
        serde_json::json!({"owner": "tsuser"}),
    ] {
        let (status, v) = mcp_json(
            &app,
            mcp_req_cookie(&tool_call(1, "repo_list", args), &cookie),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["result"]["isError"], false, "{v}");
        let text = v["result"]["content"][0]["text"].as_str().expect("text");
        assert!(text.contains("\"toolbox\""), "{text}");
    }

    // repo_get + repo_tree on the seeded tree.
    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(
                2,
                "repo_get",
                serde_json::json!({"owner": "tsuser", "name": "toolbox"}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["structuredContent"]["name"], "toolbox");

    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            // `ref` omitted — the tool schema advertises it as optional and the
            // dispatch defaults it to the default branch.
            &tool_call(
                3,
                "repo_tree",
                serde_json::json!({"owner": "tsuser", "name": "toolbox"}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");
    let text = v["result"]["content"][0]["text"].as_str().expect("tree");
    assert!(text.contains("README.md"), "{text}");

    // issue_create → issue_list/issue_get → issue_comment → issue_list_comments.
    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(
                4,
                "issue_create",
                serde_json::json!({"owner": "tsuser", "name": "toolbox", "title": "it broke", "body": "details"}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["structuredContent"]["number"], 1, "{v}");

    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(
                5,
                "issue_list",
                serde_json::json!({"owner": "tsuser", "name": "toolbox"}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");
    let text = v["result"]["content"][0]["text"].as_str().expect("issues");
    assert!(text.contains("it broke"), "{text}");

    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(
                6,
                "issue_get",
                serde_json::json!({"owner": "tsuser", "name": "toolbox", "number": 1}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["structuredContent"]["title"], "it broke", "{v}");

    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(
                7,
                "issue_comment",
                serde_json::json!({"owner": "tsuser", "name": "toolbox", "number": 1, "body": "me too"}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");

    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(
                8,
                "issue_list_comments",
                serde_json::json!({"owner": "tsuser", "name": "toolbox", "number": 1}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let text = v["result"]["content"][0]["text"]
        .as_str()
        .expect("comments");
    assert!(text.contains("me too"), "{text}");

    // pull_list + pull_get on the seeded pull.
    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(
                9,
                "pull_list",
                serde_json::json!({"owner": "tsuser", "name": "toolbox"}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");
    let text = v["result"]["content"][0]["text"].as_str().expect("pulls");
    assert!(text.contains("first pr"), "{text}");

    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(
                10,
                "pull_get",
                serde_json::json!({"owner": "tsuser", "name": "toolbox", "number": 1}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");

    // actions tools — empty run list is fine; a bogus run_id → isError.
    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(
                11,
                "actions_list_runs",
                serde_json::json!({"owner": "tsuser", "name": "toolbox"}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");

    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(
                12,
                "actions_get_run",
                serde_json::json!({"owner": "tsuser", "name": "toolbox", "run_id": "r-nope"}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], true, "{v}");

    // packages_list on the owner.
    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(13, "packages_list", serde_json::json!({"owner": "tsuser"})),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");

    // search tools — explore + repo.search types.
    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &tool_call(14, "search_repos", serde_json::json!({"q": "toolbox"})),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");

    for (id, tool) in [
        (15, "search_issues"),
        (16, "search_pulls"),
        (17, "search_code"),
    ] {
        let (status, v) = mcp_json(
            &app,
            mcp_req_cookie(
                &tool_call(
                    id,
                    tool,
                    serde_json::json!({"owner": "tsuser", "name": "toolbox", "q": "the"}),
                ),
                &cookie,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{tool} → {v}");
        assert_eq!(v["result"]["isError"], false, "{tool} → {v}");
    }
}

/// resources/read — blob (text + binary), pull markdown — plus the -32602 URI
/// errors and the -32000 domain-error wrap on private repos.
#[tokio::test]
async fn mcp_resource_reads_and_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_res.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    let app = test_app_repos(db.clone(), repos_dir.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "rs@ex.com", "rsuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;
    let repo = db
        .insert_repository("r-rs-1", &user_id, "user", "rdemo", "public", "", "main")
        .await
        .expect("insert repo");
    seed_repo_tree(&repos_dir, "rsuser", "rdemo").await;
    db.insert_pull(
        "p-rs-1",
        &repo.id,
        1,
        "ship it",
        "pull body",
        &user_id,
        "main",
        "0".repeat(40).as_str(),
        &repo.id,
        "feature",
        "1".repeat(40).as_str(),
        false,
    )
    .await
    .expect("insert pull");

    // Text blob → text/plain + contents.
    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &rpc_call(
                1,
                "resources/read",
                serde_json::json!({"uri": "oxidean://repo/rsuser/rdemo/blob/README.md"}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let entry = &v["result"]["contents"][0];
    assert_eq!(entry["mimeType"], "text/plain", "{v}");
    assert!(
        entry["text"]
            .as_str()
            .expect("text")
            .contains("hello oxidean"),
        "{v}"
    );

    // Binary blob → application/octet-stream + base64 blob field.
    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &rpc_call(
                2,
                "resources/read",
                serde_json::json!({"uri": "oxidean://repo/rsuser/rdemo/blob/bin.dat?ref=main"}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let entry = &v["result"]["contents"][0];
    assert_eq!(entry["mimeType"], "application/octet-stream", "{v}");
    assert!(entry.get("blob").is_some(), "{v}");

    // Pull resource → markdown doc.
    let (status, v) = mcp_json(
        &app,
        mcp_req_cookie(
            &rpc_call(
                3,
                "resources/read",
                serde_json::json!({"uri": "oxidean://repo/rsuser/rdemo/pull/1"}),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let text = v["result"]["contents"][0]["text"]
        .as_str()
        .expect("pull md");
    assert!(text.contains("# Pull request #1: ship it"), "{text}");
    assert!(text.contains("pull body"), "{text}");

    // URI errors → -32602.
    for params in [
        serde_json::json!({}),
        serde_json::json!({"uri": "https://example.com/x"}),
        serde_json::json!({"uri": "oxidean://repo/rsuser/rdemo/issue/nope"}),
        serde_json::json!({"uri": "oxidean://repo/rsuser/rdemo/unknown/1"}),
        serde_json::json!({"uri": 42}),
    ] {
        let (_, v) = mcp_json(
            &app,
            mcp_req_cookie(&rpc_call(4, "resources/read", params), &cookie),
        )
        .await;
        assert_eq!(v["error"]["code"], -32602, "{v}");
    }

    // Private repo read as anonymous → -32000 carrying repo.not_found.
    db.insert_repository("r-rs-2", &user_id, "user", "hidden", "private", "", "main")
        .await
        .expect("insert private repo");
    let (status, v) = mcp_json(
        &app,
        mcp_req(&rpc_call(
            5,
            "resources/read",
            serde_json::json!({"uri": "oxidean://repo/rsuser/hidden/issue/1"}),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["error"]["code"], -32000, "{v}");
    let msg = v["error"]["message"].as_str().expect("message");
    assert!(msg.contains("repo.not_found"), "{msg}");
}

/// FG `repo_list` filtering: `selected` drops private repos outside the grant
/// but keeps covered + public repos — on both `repo.listMine` and
/// `repo.listByOwner`. FG `all` covers the owner's personal namespace.
#[tokio::test]
async fn mcp_fg_repo_list_filtering() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_fglist.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "fl@ex.com", "fluser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;
    let covered = db
        .insert_repository("r-fl-1", &user_id, "user", "fgkeep", "private", "", "main")
        .await
        .expect("covered repo");
    db.insert_repository("r-fl-2", &user_id, "user", "fgdrop", "private", "", "main")
        .await
        .expect("dropped repo");
    db.insert_repository("r-fl-3", &user_id, "user", "fgpub", "public", "", "main")
        .await
        .expect("public repo");

    let (_, pat_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pat.createFineGrained","input":{{"name":"fl","repo_access":"selected","contents":"read","repository_ids":["{}"]}}}}"#,
            covered.id
        ),
        &cookie,
    )
    .await;
    assert_eq!(pat_v["ok"], true, "{pat_v}");
    let token = pat_v["data"]["token"].as_str().expect("token").to_string();

    // repo_listMine and repo.listByOwner: covered + public stay, dropped goes.
    for args in [
        serde_json::json!({}),
        serde_json::json!({"owner": "fluser"}),
    ] {
        let (status, v) = mcp_json(
            &app,
            mcp_req_bearer(&tool_call(1, "repo_list", args.clone()), &token),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["result"]["isError"], false, "{v}");
        let text = v["result"]["content"][0]["text"].as_str().expect("repos");
        assert!(text.contains("fgkeep"), "{args} → {text}");
        assert!(text.contains("fgpub"), "{args} → {text}");
        assert!(!text.contains("fgdrop"), "{args} → {text}");
    }

    // FG `all` covers the owner's whole personal namespace.
    let (_, pat_v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createFineGrained","input":{"name":"fl-all","repo_access":"all","contents":"read"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(pat_v["ok"], true, "{pat_v}");
    let all_token = pat_v["data"]["token"].as_str().expect("token").to_string();
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(2, "repo_list", serde_json::json!({})),
            &all_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let text = v["result"]["content"][0]["text"].as_str().expect("repos");
    for name in ["fgkeep", "fgdrop", "fgpub"] {
        assert!(text.contains(name), "all-access → {text}");
    }
}

/// FG `contents:write` on a covered repo may write — the write-side mirror of
/// the read-only denial above.
#[tokio::test]
async fn mcp_fg_write_scope_allows_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_fgw.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "fw@ex.com", "fwuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;
    let covered = db
        .insert_repository("r-fw-1", &user_id, "user", "fwdemo", "private", "", "main")
        .await
        .expect("insert repo");

    let (_, pat_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pat.createFineGrained","input":{{"name":"fw","repo_access":"selected","contents":"write","repository_ids":["{}"]}}}}"#,
            covered.id
        ),
        &cookie,
    )
    .await;
    assert_eq!(pat_v["ok"], true, "{pat_v}");
    let token = pat_v["data"]["token"].as_str().expect("token").to_string();

    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(
                1,
                "issue_create",
                serde_json::json!({"owner": "fwuser", "name": "fwdemo", "title": "written"}),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");
    assert_eq!(v["result"]["structuredContent"]["number"], 1, "{v}");

    // Writes stay denied on a repo outside the grant.
    db.insert_repository("r-fw-2", &user_id, "user", "fwother", "private", "", "main")
        .await
        .expect("insert repo");
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(
                2,
                "issue_create",
                serde_json::json!({"owner": "fwuser", "name": "fwother", "title": "nope"}),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], true, "{v}");
}

// ---------------------------------------------------------------------------
// OAuth bearer (oxidean_oat_) — resolves through the provider's token table;
// OAuth scopes land in the synthetic PAT so the Access gates apply unchanged.
// ---------------------------------------------------------------------------

fn oauth_expiry_in(secs: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(secs))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

async fn seed_oauth_app(db: &Database, app_id: &str, user_id: &str) {
    db.insert_oauth_app(
        app_id,
        user_id,
        "mcp client",
        &format!("oxidean_client_{app_id}"),
        "hash",
        "oxidean_cs_",
        "[]",
    )
    .await
    .expect("insert oauth app");
}

async fn seed_oauth_token(
    db: &Database,
    tok_id: &str,
    app_id: &str,
    user_id: &str,
    raw: &str,
    scopes: &str,
    expires_at: &str,
) {
    db.insert_oauth_token(
        tok_id,
        app_id,
        user_id,
        &raw[..20],
        &sha256_hex(raw.as_bytes()),
        scopes,
        expires_at,
    )
    .await
    .expect("insert oauth token");
}

#[tokio::test]
async fn mcp_oauth_access_token_repo_scope() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_oat.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_cookie, login_v) = signup_and_login(&app, "oat@ex.com", "oatuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;
    db.insert_repository("r-oat-1", &user_id, "user", "demo", "private", "", "main")
        .await
        .expect("insert repo");
    seed_oauth_app(&db, "app-a", &user_id).await;
    let raw = "oxidean_oat_mcp_repo_scoped";
    seed_oauth_token(
        &db,
        "tok-1",
        "app-a",
        &user_id,
        raw,
        "repo",
        &oauth_expiry_in(3600),
    )
    .await;

    // initialize — same handshake as PAT.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &rpc_call(
                1,
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "0"},
                }),
            ),
            raw,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["protocolVersion"], "2025-06-18");

    // repo_get on own private repo — `repo` scope covers RepoRead.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(
                2,
                "repo_get",
                serde_json::json!({"owner": "oatuser", "name": "demo"}),
            ),
            raw,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");
    assert_eq!(v["result"]["structuredContent"]["name"], "demo");

    // issue_create — write path covered by the same `repo` scope.
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(
                3,
                "issue_create",
                serde_json::json!({"owner": "oatuser", "name": "demo", "title": "via oat"}),
            ),
            raw,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], false, "{v}");
}

#[tokio::test]
async fn mcp_oauth_scope_denied() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_oat_scope.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_cookie, login_v) = signup_and_login(&app, "ro@ex.com", "rouser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;
    db.insert_repository("r-ro-1", &user_id, "user", "demo", "private", "", "main")
        .await
        .expect("insert repo");
    seed_oauth_app(&db, "app-b", &user_id).await;
    let raw = "oxidean_oat_readuser_only";
    seed_oauth_token(
        &db,
        "tok-2",
        "app-b",
        &user_id,
        raw,
        "read:user",
        &oauth_expiry_in(3600),
    )
    .await;

    // initialize passes (identity-level access), repo_get is scope-denied.
    let (status, _v) = mcp_json(
        &app,
        mcp_req_bearer(
            &rpc_call(
                1,
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "0"},
                }),
            ),
            raw,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{_v}");
    let (status, v) = mcp_json(
        &app,
        mcp_req_bearer(
            &tool_call(
                2,
                "repo_get",
                serde_json::json!({"owner": "rouser", "name": "demo"}),
            ),
            raw,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["isError"], true, "{v}");
    let text = v["result"]["content"][0]["text"].as_str().expect("text");
    assert!(text.contains("insufficient_scope"), "{text}");
}

#[tokio::test]
async fn mcp_oauth_rejected_states() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_oat_rej.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_cookie, login_v) = signup_and_login(&app, "rej@ex.com", "rejuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;

    let init = rpc_call(
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0"},
        }),
    );

    // Unknown OAT → 401 + RFC 9728 resource_metadata pointer.
    let res = app
        .clone()
        .oneshot(mcp_req_bearer(&init, "oxidean_oat_no_such_token"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let challenge = res
        .headers()
        .get("www-authenticate")
        .expect("challenge")
        .to_str()
        .unwrap();
    assert!(challenge.contains("resource_metadata="), "{challenge}");
    assert!(
        challenge.contains("/.well-known/oauth-protected-resource"),
        "{challenge}"
    );

    // Expired OAT → 401.
    seed_oauth_app(&db, "app-c", &user_id).await;
    let expired = "oxidean_oat_expired_tok";
    seed_oauth_token(
        &db,
        "tok-3",
        "app-c",
        &user_id,
        expired,
        "repo",
        &oauth_expiry_in(-60),
    )
    .await;
    let res = app
        .clone()
        .oneshot(mcp_req_bearer(&init, expired))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Revoked OAT → 401 (revoke all the user's tokens for the app).
    let revoked = "oxidean_oat_revoked_tok";
    seed_oauth_token(
        &db,
        "tok-4",
        "app-c",
        &user_id,
        revoked,
        "repo",
        &oauth_expiry_in(3600),
    )
    .await;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.revoke_oauth_tokens_for_user_app(&user_id, "app-c", &now)
        .await
        .expect("revoke");
    let res = app
        .clone()
        .oneshot(mcp_req_bearer(&init, revoked))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Banned owner → 401.
    let banned = "oxidean_oat_banned_tok";
    seed_oauth_app(&db, "app-d", &user_id).await;
    seed_oauth_token(
        &db,
        "tok-5",
        "app-d",
        &user_id,
        banned,
        "repo",
        &oauth_expiry_in(3600),
    )
    .await;
    db.set_user_banned_at(&user_id, &now).await.expect("ban");
    let res = app
        .clone()
        .oneshot(mcp_req_bearer(&init, banned))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

/// RFC 8414 + RFC 9728 discovery docs — MCP clients resolve the authorization
/// server from the protected-resource pointer on the 401 challenge.
#[tokio::test]
async fn mcp_oauth_discovery_metadata() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("mcp_disc.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let app = test_app(db).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/.well-known/oauth-protected-resource")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let resource = v["resource"].as_str().expect("resource");
    assert!(resource.ends_with("/api/mcp"), "{resource}");
    assert_eq!(
        v["authorization_servers"]
            .as_array()
            .expect("as list")
            .len(),
        1
    );
    assert_eq!(v["bearer_methods_supported"], serde_json::json!(["header"]));

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/.well-known/oauth-authorization-server")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let issuer = v["issuer"].as_str().expect("issuer");
    assert_eq!(
        v["authorization_endpoint"],
        format!("{issuer}/oauth/authorize")
    );
    assert_eq!(v["token_endpoint"], format!("{issuer}/oauth/token"));
    assert!(v["grant_types_supported"]
        .as_array()
        .expect("grants")
        .iter()
        .any(|g| g == "authorization_code"));
    assert!(v["scopes_supported"]
        .as_array()
        .expect("scopes")
        .iter()
        .any(|s| s == "repo"));
}
