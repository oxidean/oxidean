//! AGT-01: streamable-HTTP MCP endpoint — JSON-RPC framing, session/PAT auth,
//! tool dispatch over the typed RPC layer, scope denial, and no private-repo
//! existence leaks to anonymous callers.

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
