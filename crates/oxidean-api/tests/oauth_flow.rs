//! API-03: OAuth2 provider — app registration, authorize+consent, token
//! exchange (single-use codes), userinfo, and grant revocation.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::OAUTH_ACCESS_TOKEN_PREFIX;
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

async fn signup_login_verify(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
) -> (String, String) {
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
    let user_id = v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    (cookie, user_id)
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

/// Register an app owned by `cookie`'s user; returns (app_id, client_id, client_secret).
async fn create_app(app: &axum::Router, cookie: &str) -> (String, String, String) {
    let (status, v) = rpc_json(
        app,
        r#"{"procedure":"oauthApp.create","input":{"name":"test-cli","redirect_uris":["https://app.example/callback","http://localhost:9999/cb"]}}"#,
        cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create — {v}");
    assert_eq!(v["ok"], true, "{v}");
    let secret = v["data"]["client_secret"].as_str().expect("client_secret");
    assert!(
        secret.starts_with(oxidean_core::OAUTH_CLIENT_SECRET_PREFIX),
        "secret must carry the brand prefix — got {secret}"
    );
    (
        v["data"]["app"]["id"].as_str().unwrap().to_string(),
        v["data"]["app"]["client_id"].as_str().unwrap().to_string(),
        secret.to_string(),
    )
}

fn authorize_get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method("GET").uri(uri);
    if let Some(c) = cookie {
        b = b.header("cookie", c);
    }
    b.body(Body::empty()).unwrap()
}

fn token_post_form(form: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/oauth/token")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(form.to_owned()))
        .unwrap()
}

async fn location_of(res: axum::http::Response<Body>) -> String {
    res.headers()
        .get("location")
        .expect("Location")
        .to_str()
        .unwrap()
        .to_string()
}

/// Drive the consent decision through `oauthApp.authorize`; return redirect_to.
async fn authorize_decision(
    app: &axum::Router,
    cookie: &str,
    client_id: &str,
    redirect_uri: &str,
    scope: &str,
    approve: bool,
) -> String {
    let body = format!(
        r#"{{"procedure":"oauthApp.authorize","input":{{"client_id":"{client_id}","redirect_uri":"{redirect_uri}","scope":"{scope}","state":"xyz","approve":{approve}}}}}"#
    );
    let (status, v) = rpc_json(app, &body, cookie).await;
    assert_eq!(status, StatusCode::OK, "authorize — {v}");
    assert_eq!(v["ok"], true, "{v}");
    v["data"]["redirect_to"]
        .as_str()
        .expect("redirect_to")
        .to_string()
}

fn query_param(url: &str, key: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

async fn exchange_code(
    app: &axum::Router,
    code: &str,
    client_id: &str,
    secret: &str,
) -> (StatusCode, serde_json::Value) {
    let form = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&client_secret={}",
        url::form_urlencoded::byte_serialize(code.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(b"https://app.example/callback")
            .collect::<String>(),
        client_id,
        secret,
    );
    let res = app.clone().oneshot(token_post_form(&form)).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn oauth_app_crud_and_secret_hygiene() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("oauth_crud.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, _uid) = signup_login_verify(&app, &db, "dev@ex.com", "devuser").await;

    // Unauthenticated create denied.
    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"oauthApp.create","input":{"name":"x","redirect_uris":["https://a.example/cb"]}}"#,
        "",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{v}");

    let (app_id, client_id, secret) = create_app(&app, &cookie).await;
    assert!(client_id.starts_with(oxidean_core::OAUTH_CLIENT_ID_PREFIX));

    // List: no plaintext secret anywhere in the response.
    let (status, v) = rpc_json(&app, r#"{"procedure":"oauthApp.list","input":{}}"#, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let items = v["data"].as_array().expect("array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "test-cli");
    assert_eq!(items[0]["client_id"], client_id);
    assert!(
        !serde_json::to_string(&v).unwrap().contains(&secret),
        "list must never carry the plaintext secret"
    );

    // Update name + redirect URIs.
    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"oauthApp.update","input":{{"id":"{app_id}","name":"renamed","redirect_uris":["https://app.example/callback"]}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["name"], "renamed");
    assert_eq!(
        v["data"]["redirect_uris"],
        serde_json::json!(["https://app.example/callback"])
    );

    // Regenerate: new secret works for nothing yet, but old prefix changes.
    let (status, v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"oauthApp.regenerateSecret","input":{{"id":"{app_id}"}}}}"#),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let secret2 = v["data"]["client_secret"].as_str().unwrap();
    assert_ne!(secret, secret2);

    // Other user cannot see or delete the app.
    let (cookie2, _) = signup_login_verify(&app, &db, "other@ex.com", "otheruser").await;
    let (status, v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"oauthApp.delete","input":{{"id":"{app_id}"}}}}"#),
        &cookie2,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "oauth.app_not_found");

    // Owner delete succeeds.
    let (status, v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"oauthApp.delete","input":{{"id":"{app_id}"}}}}"#),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["ok"], true, "{v}");
    let (_s, v) = rpc_json(&app, r#"{"procedure":"oauthApp.list","input":{}}"#, &cookie).await;
    assert_eq!(v["data"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn oauth_authorize_endpoint_gating() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("oauth_gate.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;
    let (cookie, _uid) = signup_login_verify(&app, &db, "dev@ex.com", "devuser").await;
    let (_id, client_id, _secret) = create_app(&app, &cookie).await;

    // Unknown client → 400 JSON, never a redirect to an untrusted URI.
    let res = app
        .clone()
        .oneshot(authorize_get(
            "/oauth/authorize?client_id=nope&response_type=code",
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Unregistered redirect_uri → 400 JSON (no redirect).
    let res = app
        .clone()
        .oneshot(authorize_get(
            &format!(
                "/oauth/authorize?client_id={client_id}&response_type=code&redirect_uri=https%3A%2F%2Fevil.example%2F"
            ),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(res.headers().get("location").is_none());

    // Anonymous → redirect to /login carrying returnTo back to /oauth/authorize.
    let res = app
        .clone()
        .oneshot(authorize_get(
            &format!(
                "/oauth/authorize?client_id={client_id}&response_type=code&redirect_uri=https%3A%2F%2Fapp.example%2Fcallback&state=s1"
            ),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let loc = location_of(res).await;
    assert!(loc.starts_with("/login?returnTo="), "{loc}");
    let return_to = query_param(&format!("http://localhost{loc}"), "returnTo").expect("returnTo");
    assert!(
        return_to.starts_with("/oauth/authorize?") && return_to.contains("client_id="),
        "{return_to}"
    );

    // Signed in → redirect to the SPA consent page with params intact.
    let res = app
        .clone()
        .oneshot(authorize_get(
            &format!(
                "/oauth/authorize?client_id={client_id}&response_type=code&redirect_uri=https%3A%2F%2Fapp.example%2Fcallback&state=s1"
            ),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let loc = location_of(res).await;
    assert!(loc.starts_with("/oauth/consent?"), "{loc}");
    assert!(
        loc.contains("client_id=") && loc.contains("state=s1"),
        "{loc}"
    );

    // Bad response_type → redirect to registered redirect_uri with error.
    let res = app
        .clone()
        .oneshot(authorize_get(
            &format!(
                "/oauth/authorize?client_id={client_id}&response_type=token&redirect_uri=https%3A%2F%2Fapp.example%2Fcallback&state=st"
            ),
            Some(&cookie),
        ))
        .await
        .unwrap();
    let loc = location_of(res).await;
    assert!(loc.starts_with("https://app.example/callback?"), "{loc}");
    assert_eq!(
        query_param(&loc, "error").as_deref(),
        Some("unsupported_response_type")
    );
    assert_eq!(query_param(&loc, "state").as_deref(), Some("st"));
}

#[tokio::test]
async fn oauth_full_code_flow_and_revocation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("oauth_flow.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (dev_cookie, _dev_uid) = signup_login_verify(&app, &db, "dev@ex.com", "devuser").await;
    let (user_cookie, _uid) = signup_login_verify(&app, &db, "u@ex.com", "uuser").await;
    let (app_id, client_id, secret) = create_app(&app, &dev_cookie).await;

    // authorizeInfo reflects the app + parsed scopes for the consent screen.
    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"oauthApp.authorizeInfo","input":{{"client_id":"{client_id}","redirect_uri":"https://app.example/callback","scope":"repo read:user"}}}}"#
        ),
        &user_cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["app_name"], "test-cli");
    assert_eq!(v["data"]["owner_username"], "devuser");
    assert_eq!(
        v["data"]["scopes"],
        serde_json::json!(["repo", "read:user"])
    );

    // Deny → access_denied redirect, no code minted.
    let to = authorize_decision(
        &app,
        &user_cookie,
        &client_id,
        "https://app.example/callback",
        "repo read:user",
        false,
    )
    .await;
    assert_eq!(query_param(&to, "error").as_deref(), Some("access_denied"));
    assert_eq!(query_param(&to, "state").as_deref(), Some("xyz"));
    assert!(query_param(&to, "code").is_none());

    // Approve → code + state on the registered redirect_uri.
    let to = authorize_decision(
        &app,
        &user_cookie,
        &client_id,
        "https://app.example/callback",
        "repo read:user",
        true,
    )
    .await;
    assert!(to.starts_with("https://app.example/callback?"), "{to}");
    let code = query_param(&to, "code").expect("code");
    assert_eq!(query_param(&to, "state").as_deref(), Some("xyz"));
    assert!(code.starts_with(oxidean_core::OAUTH_CODE_PREFIX));

    // Wrong client_secret → invalid_client.
    let (status, v) = exchange_code(&app, &code, &client_id, "oxidean_osec_wrong").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{v}");
    assert_eq!(v["error"], "invalid_client");

    // Happy exchange → bearer token.
    let (status, v) = exchange_code(&app, &code, &client_id, &secret).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let access = v["access_token"].as_str().expect("access_token");
    assert!(access.starts_with(OAUTH_ACCESS_TOKEN_PREFIX), "{access}");
    assert_eq!(v["token_type"], "bearer");
    assert_eq!(v["scope"], "repo read:user");
    assert!(v["expires_in"].as_i64().unwrap() > 0);

    // Single-use: second exchange of the same code fails.
    let (status, v) = exchange_code(&app, &code, &client_id, &secret).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"], "invalid_grant");

    // userinfo: read:user works; email hidden without user:email scope.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/oauth/userinfo")
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let info: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(info["username"], "uuser");
    assert!(
        info.get("email").is_none() || info["email"].is_null(),
        "{info}"
    );

    // Missing bearer → 401 with WWW-Authenticate.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/oauth/userinfo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert!(res.headers().get("www-authenticate").is_some());

    // Grants list shows the app; revoke kills the token.
    let (_s, v) = rpc_json(
        &app,
        r#"{"procedure":"oauthApp.listGrants","input":{}}"#,
        &user_cookie,
    )
    .await;
    let grants = v["data"].as_array().expect("grants");
    assert_eq!(grants.len(), 1, "{v}");
    assert_eq!(grants[0]["app_name"], "test-cli");
    assert_eq!(grants[0]["application_id"], app_id);

    let (_s, v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"oauthApp.revoke","input":{{"id":"{app_id}"}}}}"#),
        &user_cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/oauth/userinfo")
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "revoked token must fail"
    );

    let (_s, v) = rpc_json(
        &app,
        r#"{"procedure":"oauthApp.listGrants","input":{}}"#,
        &user_cookie,
    )
    .await;
    assert_eq!(v["data"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn oauth_code_rejects_foreign_redirect_and_client() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("oauth_bind.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (dev_cookie, _d) = signup_login_verify(&app, &db, "dev@ex.com", "devuser").await;
    let (user_cookie, _u) = signup_login_verify(&app, &db, "u@ex.com", "uuser").await;
    let (_id, client_id, secret) = create_app(&app, &dev_cookie).await;
    let (_id2, client_id2, secret2) = {
        let (status, v) = rpc_json(
            &app,
            r#"{"procedure":"oauthApp.create","input":{"name":"other-app","redirect_uris":["https://other.example/cb"]}}"#,
            &dev_cookie,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        (
            v["data"]["app"]["id"].as_str().unwrap().to_string(),
            v["data"]["app"]["client_id"].as_str().unwrap().to_string(),
            v["data"]["client_secret"].as_str().unwrap().to_string(),
        )
    };

    let to = authorize_decision(
        &app,
        &user_cookie,
        &client_id,
        "https://app.example/callback",
        "repo",
        true,
    )
    .await;
    let code = query_param(&to, "code").expect("code");

    // redirect_uri mismatch → invalid_grant (and consumes the code).
    let form = format!(
        "grant_type=authorization_code&code={code}&redirect_uri=https%3A%2F%2Fapp.example%2Fother&client_id={client_id}&client_secret={secret}"
    );
    let res = app.clone().oneshot(token_post_form(&form)).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"], "invalid_grant");

    // Fresh code exchanged by the *other* client → invalid_grant.
    let to = authorize_decision(
        &app,
        &user_cookie,
        &client_id,
        "https://app.example/callback",
        "repo",
        true,
    )
    .await;
    let code2 = query_param(&to, "code").expect("code2");
    let (status, v) = exchange_code(&app, &code2, &client_id2, &secret2).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"], "invalid_grant");
}

#[tokio::test]
async fn oauth_authorize_requires_verified_user() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("oauth_verify.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (dev_cookie, _d) = signup_login_verify(&app, &db, "dev@ex.com", "devuser").await;
    let (_id, client_id, _s) = create_app(&app, &dev_cookie).await;

    // Second user stays UNVERIFIED — approve must be gated.
    let signup_body = r#"{"procedure":"auth.signup","input":{"email":"unv@ex.com","username":"unv","password":"password1"}}"#;
    let signup = app.clone().oneshot(rpc_req(signup_body)).await.unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let login_body = r#"{"procedure":"auth.login","input":{"identifier":"unv@ex.com","password":"password1","remember_me":false}}"#;
    let login = app.clone().oneshot(rpc_req(login_body)).await.unwrap();
    let unv_cookie = session_cookie_from_response(&login);

    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"oauthApp.authorize","input":{{"client_id":"{client_id}","redirect_uri":"https://app.example/callback","approve":true}}}}"#
        ),
        &unv_cookie,
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.email_unverified");
    let _ = status;
}
