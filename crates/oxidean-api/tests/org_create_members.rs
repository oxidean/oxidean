//! ORG-01 / ORG-02: org.create happy path + deferred Wave 0 stubs for members/ACL.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

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

/// Verified `org.create` reserves a slug shared with the user namespace (D-ORG-01).
#[tokio::test]
async fn org_create_reserves_shared_slug_namespace() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("org_create_slug.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "owner@ex.com", "owner1").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;

    // Collision with an existing username must fail (shared namespace).
    let collide = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"owner1"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(collide.status(), StatusCode::BAD_REQUEST);
    let collide_bytes = collide.into_body().collect().await.unwrap().to_bytes();
    let collide_v: serde_json::Value = serde_json::from_slice(&collide_bytes).unwrap();
    assert_eq!(collide_v["ok"], false);
    assert_eq!(collide_v["error"]["code"], "org.slug_taken");

    // Happy path: unique slug succeeds.
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"acme-labs","display_name":"Acme Labs"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK, "org.create must succeed");
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "org.create ok=true — {v}");
    assert_eq!(v["data"]["slug"], "acme-labs");
    assert_eq!(v["data"]["display_name"], "Acme Labs");
    assert_eq!(v["data"]["member_base_permission"], "none");

    // Second create with same slug fails (org↔org uniqueness).
    let dup = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"acme-labs"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(dup.status(), StatusCode::BAD_REQUEST);
    let dup_bytes = dup.into_body().collect().await.unwrap().to_bytes();
    let dup_v: serde_json::Value = serde_json::from_slice(&dup_bytes).unwrap();
    assert_eq!(dup_v["error"]["code"], "org.slug_taken");
}

/// Signup must not claim an existing org slug (D-ORG-01 reverse direction).
#[tokio::test]
async fn signup_rejects_existing_org_slug() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("signup_org_slug.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "owner@ex.com", "owner1").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"acme","display_name":"Acme"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);

    let collide = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"attacker@ex.com","username":"acme","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(collide.status(), StatusCode::BAD_REQUEST);
    let bytes = collide.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "auth.taken");
}

/// Org-owned public repo resolves via org slug (D-ORG-01 / OwnerRef) — not user-only lookup.
#[tokio::test]
async fn org_create_owned_repo_resolves_by_org_slug() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_resolve_slug.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "resolve@ex.com", "resolve1").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"acme-resolve"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    let org_id = v["data"]["id"].as_str().expect("org id");

    // Fixture: org-owned row (owner_id = org). owner_type set via insert API once GREEN;
    // until then insert defaults owner_type=user but lookup keys on owner_id.
    db.insert_repository(
        "r-org-resolve",
        org_id,
        "org",
        "widget",
        "public",
        "",
        "main",
    )
    .await
    .expect("insert org-owned repo");

    let get = app
        .oneshot(rpc_req(
            r#"{"procedure":"repo.get","input":{"owner":"acme-resolve","name":"widget"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(
        get.status(),
        StatusCode::OK,
        "org slug must resolve like username"
    );
    let get_bytes = get.into_body().collect().await.unwrap().to_bytes();
    let get_v: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
    assert_eq!(get_v["ok"], true, "repo.get under org slug — {get_v}");
    assert_eq!(get_v["data"]["name"], "widget");
    assert_eq!(
        get_v["data"]["owner_username"], "acme-resolve",
        "AccessibleRepo.owner_username is the org slug"
    );
    assert_eq!(get_v["data"]["owner_id"], org_id);
    assert_eq!(get_v["data"]["owner_type"], "org");
}

/// Creator of an org is Owner (ORG-01 / D-ORG-02a).
#[tokio::test]
async fn org_create_creator_is_owner() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_create_owner.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "boss@ex.com", "boss1").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;

    let create = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"boss-org"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    let org_id = v["data"]["id"].as_str().expect("org id");

    let member = db
        .find_org_member(org_id, &user_id)
        .await
        .expect("find member")
        .expect("creator membership row");
    assert_eq!(member.role, "owner", "creator must be Owner");
}

async fn rpc_json(
    app: &axum::Router,
    body: &str,
    cookie: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let req = match cookie {
        Some(c) => rpc_req_with_cookie(body, c),
        None => rpc_req(body),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, v)
}

async fn close_signup(db: &Database) {
    let settings = db.get_auth_settings().await.expect("auth settings");
    db.update_auth_settings(
        &settings.provider_mode,
        &settings.email_provider,
        settings.from_address.as_deref(),
        settings.oidc_issuer.as_deref(),
        settings.oidc_client_id.as_deref(),
        settings.workos_client_id.as_deref(),
        false,
        &settings.default_visibility,
    )
    .await
    .expect("close allow_signup");
}

/// `org.get` / `org.listMine` return public fields for UI (ASSUME / ORG-01).
#[tokio::test]
async fn org_get_and_list_mine() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_get_list_mine.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "getter@ex.com", "getter1").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"get-org","display_name":"Get Org"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");

    let (get_status, get_v) = rpc_json(
        &app,
        r#"{"procedure":"org.get","input":{"slug":"get-org"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(get_status, StatusCode::OK, "org.get — {get_v}");
    assert_eq!(get_v["ok"], true, "{get_v}");
    assert_eq!(get_v["data"]["slug"], "get-org");
    assert_eq!(get_v["data"]["display_name"], "Get Org");
    assert_eq!(get_v["data"]["member_base_permission"], "none");

    // Anonymous org overview (public profile parity with user.getPublicProfile).
    let (anon_status, anon_v) = rpc_json(
        &app,
        r#"{"procedure":"org.get","input":{"slug":"get-org"}}"#,
        None,
    )
    .await;
    assert_eq!(anon_status, StatusCode::OK, "org.get anonymous — {anon_v}");
    assert_eq!(anon_v["ok"], true, "{anon_v}");
    assert_eq!(anon_v["data"]["slug"], "get-org");
    assert_eq!(anon_v["data"]["display_name"], "Get Org");

    let (missing_status, missing_v) = rpc_json(
        &app,
        r#"{"procedure":"org.get","input":{"slug":"no-such-org"}}"#,
        None,
    )
    .await;
    assert_eq!(
        missing_status,
        StatusCode::BAD_REQUEST,
        "org.get missing — {missing_v}"
    );
    assert_eq!(missing_v["ok"], false, "{missing_v}");
    assert_eq!(missing_v["error"]["code"], "org.not_found");

    let (list_anon_status, list_anon_v) =
        rpc_json(&app, r#"{"procedure":"org.listMine","input":{}}"#, None).await;
    assert_eq!(list_anon_v["ok"], false, "{list_anon_v}");
    assert_eq!(list_anon_v["error"]["code"], "auth.unauthenticated");
    let _ = list_anon_status;

    let (list_status, list_v) = rpc_json(
        &app,
        r#"{"procedure":"org.listMine","input":{}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(list_status, StatusCode::OK, "org.listMine — {list_v}");
    assert_eq!(list_v["ok"], true, "{list_v}");
    let orgs = list_v["data"]["orgs"].as_array().expect("orgs array");
    assert_eq!(orgs.len(), 1, "{list_v}");
    assert_eq!(orgs[0]["slug"], "get-org");
    assert_eq!(orgs[0]["role"], "owner");
}

/// `members.add` by username adds an existing instance user (ORG-01 / D-ORG-03).
#[tokio::test]
async fn org_members_add_by_username() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("org_members_add.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "addowner@ex.com", "addowner1").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    verify_user(&db, owner_id).await;

    let (member_cookie, member_v) = signup_and_login(&app, "addmem@ex.com", "addmem1").await;
    let member_id = member_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &member_id).await;
    let _ = member_cookie;

    // D-ORG-03: username add works regardless of allow_signup.
    close_signup(&db).await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"add-org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");

    let (add_status, add_v) = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"add-org","username":"addmem1","role":"member"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(add_status, StatusCode::OK, "members.add — {add_v}");
    assert_eq!(add_v["ok"], true, "{add_v}");
    assert_eq!(add_v["data"]["username"], "addmem1");
    assert_eq!(add_v["data"]["role"], "member");
    assert_eq!(add_v["data"]["user_id"], member_id);

    let (list_status, list_v) = rpc_json(
        &app,
        r#"{"procedure":"org.members.list","input":{"slug":"add-org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(list_status, StatusCode::OK, "members.list — {list_v}");
    assert_eq!(list_v["ok"], true, "{list_v}");
    let members = list_v["data"]["members"].as_array().expect("members");
    assert!(
        members
            .iter()
            .any(|m| m["username"] == "addmem1" && m["role"] == "member"),
        "list must include added member — {list_v}"
    );
    assert!(
        members.iter().all(|m| m.get("email").is_none()),
        "members.list must not leak emails — {list_v}"
    );
}

/// `members.updateRole` changes Owner/Admin/Member; only Owner can grant Owner (ORG-02 / T-10-09).
#[tokio::test]
async fn org_members_update_role() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_members_role.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "roleowner@ex.com", "roleowner1").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    verify_user(&db, owner_id).await;

    let (_, admin_v) = signup_and_login(&app, "roleadmin@ex.com", "roleadmin1").await;
    let admin_id = admin_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &admin_id).await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"role-org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");

    let (_, add_v) = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"role-org","username":"roleadmin1","role":"member"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(add_v["ok"], true, "{add_v}");

    let (up_status, up_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.members.updateRole","input":{{"slug":"role-org","user_id":"{admin_id}","role":"admin"}}}}"#
        ),
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(up_status, StatusCode::OK, "updateRole — {up_v}");
    assert_eq!(up_v["ok"], true, "{up_v}");
    assert_eq!(up_v["data"]["role"], "admin");

    // Admin cannot grant Owner (T-10-09) — re-login as the admin we added.
    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"roleadmin@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let admin_cookie = session_cookie_from_response(&login);
    let _ = login.into_body().collect().await;

    let (_, deny_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.members.updateRole","input":{{"slug":"role-org","user_id":"{admin_id}","role":"owner"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(deny_v["ok"], false, "Admin must not grant Owner — {deny_v}");
    assert_eq!(
        deny_v["error"]["code"], "org.forbidden",
        "Admin grant Owner → org.forbidden — {deny_v}"
    );

    // Owner can grant a second Owner.
    let (_, mem2_v) = signup_and_login(&app, "roleown2@ex.com", "roleown2").await;
    let mem2_id = mem2_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &mem2_id).await;
    let (_, add2) = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"role-org","username":"roleown2","role":"member"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(add2["ok"], true, "{add2}");
    let (_, grant_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.members.updateRole","input":{{"slug":"role-org","user_id":"{mem2_id}","role":"owner"}}}}"#
        ),
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(grant_v["ok"], true, "Owner can grant Owner — {grant_v}");
    assert_eq!(grant_v["data"]["role"], "owner");
}

/// Demoting or removing the last Owner → `org.last_owner` (ORG-01 / T-10-10).
#[tokio::test]
async fn org_members_last_owner_demote_or_remove_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("org_last_owner.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "lastown@ex.com", "lastown1").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &owner_id).await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"last-org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");

    let (_, demote_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.members.updateRole","input":{{"slug":"last-org","user_id":"{owner_id}","role":"admin"}}}}"#
        ),
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(demote_v["ok"], false, "{demote_v}");
    assert_eq!(
        demote_v["error"]["code"], "org.last_owner",
        "demote last Owner — {demote_v}"
    );

    let (_, remove_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.members.remove","input":{{"slug":"last-org","user_id":"{owner_id}"}}}}"#
        ),
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(remove_v["ok"], false, "{remove_v}");
    assert_eq!(
        remove_v["error"]["code"], "org.last_owner",
        "remove last Owner — {remove_v}"
    );
}

/// Member with member_base=none cannot read private org repo (ORG-02 / D-ORG-02b / T-10-02).
#[tokio::test]
async fn org_member_base_none_denies_private_repo_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("org_base_none.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "basenone@ex.com", "basenone1").await;
    verify_user(&db, owner_v["data"]["id"].as_str().expect("id")).await;

    let (_, mem_v) = signup_and_login(&app, "basemem@ex.com", "basemem1").await;
    let mem_id = mem_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &mem_id).await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"base-none-org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");
    assert_eq!(create_v["data"]["member_base_permission"], "none");

    let (_, add_v) = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"base-none-org","username":"basemem1","role":"member"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(add_v["ok"], true, "{add_v}");

    let (_, repo_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","owner":"base-none-org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(repo_v["ok"], true, "{repo_v}");

    // Owner still admin despite member_base none.
    let (_, owner_get) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"base-none-org","name":"secret"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(owner_get["ok"], true, "Owner unaffected — {owner_get}");
    assert_eq!(owner_get["data"]["can_admin"], true);

    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"basemem@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    let mem_cookie = session_cookie_from_response(&login);
    let _ = login.into_body().collect().await;

    let (_, mem_get) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"base-none-org","name":"secret"}}"#,
        Some(&mem_cookie),
    )
    .await;
    assert_eq!(
        mem_get["error"]["code"], "repo.not_found",
        "Member + base none → soft not_found — {mem_get}"
    );
}

/// Member with member_base=read can read private org repo (ORG-02 / D-ORG-02b).
#[tokio::test]
async fn org_member_base_read_allows_private_repo_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("org_base_read.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "baseread@ex.com", "baseread1").await;
    verify_user(&db, owner_v["data"]["id"].as_str().expect("id")).await;

    let (_, mem_v) = signup_and_login(&app, "readmem@ex.com", "readmem1").await;
    verify_user(&db, mem_v["data"]["id"].as_str().expect("id")).await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"base-read-org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");

    let (_, add_v) = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"base-read-org","username":"readmem1","role":"member"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(add_v["ok"], true, "{add_v}");

    let (_, repo_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","owner":"base-read-org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(repo_v["ok"], true, "{repo_v}");

    let (set_status, set_v) = rpc_json(
        &app,
        r#"{"procedure":"org.updateSettings","input":{"slug":"base-read-org","member_base_permission":"read"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(set_status, StatusCode::OK, "updateSettings — {set_v}");
    assert_eq!(set_v["ok"], true, "{set_v}");
    assert_eq!(set_v["data"]["member_base_permission"], "read");

    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"readmem@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    let mem_cookie = session_cookie_from_response(&login);
    let _ = login.into_body().collect().await;

    let (_, mem_get) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"base-read-org","name":"secret"}}"#,
        Some(&mem_cookie),
    )
    .await;
    assert_eq!(mem_get["ok"], true, "Member + base read — {mem_get}");
    assert_eq!(
        mem_get["data"]["can_write"], false,
        "read is not write — {mem_get}"
    );
    assert_eq!(mem_get["data"]["can_admin"], false);
}

/// Member with member_base=write can write private org repo (ORG-02 / D-ORG-02b).
#[tokio::test]
async fn org_member_base_write_allows_private_repo_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("org_base_write.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "basewrite@ex.com", "basewrite1").await;
    verify_user(&db, owner_v["data"]["id"].as_str().expect("id")).await;

    let (_, mem_v) = signup_and_login(&app, "writemem@ex.com", "writemem1").await;
    verify_user(&db, mem_v["data"]["id"].as_str().expect("id")).await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"base-write-org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");

    let (_, add_v) = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"base-write-org","username":"writemem1","role":"member"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(add_v["ok"], true, "{add_v}");

    let (_, repo_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","owner":"base-write-org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(repo_v["ok"], true, "{repo_v}");

    // Public org repo remains readable with any base (D-ORG-02b).
    let (_, pub_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"open","visibility":"public","owner":"base-write-org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(pub_v["ok"], true, "{pub_v}");

    let (set_status, set_v) = rpc_json(
        &app,
        r#"{"procedure":"org.updateSettings","input":{"slug":"base-write-org","member_base_permission":"write"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(set_status, StatusCode::OK, "{set_v}");
    assert_eq!(set_v["data"]["member_base_permission"], "write");

    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"writemem@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    let mem_cookie = session_cookie_from_response(&login);
    let _ = login.into_body().collect().await;

    let (_, mem_get) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"base-write-org","name":"secret"}}"#,
        Some(&mem_cookie),
    )
    .await;
    assert_eq!(mem_get["ok"], true, "Member + base write — {mem_get}");
    assert_eq!(mem_get["data"]["can_write"], true, "{mem_get}");
    assert_eq!(
        mem_get["data"]["can_admin"], false,
        "write is not admin — {mem_get}"
    );

    let (_, pub_get) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"base-write-org","name":"open"}}"#,
        Some(&mem_cookie),
    )
    .await;
    assert_eq!(pub_get["ok"], true, "public readable — {pub_get}");
}

/// Wave 0 / ORG-01: live lookup shape for member add — prefix/limit/no-email (D-ORG-03).
#[tokio::test]
async fn org_lookup_shape_prefix_limit_no_email() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_lookup_shape.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "lookupowner@ex.com", "lookupown").await;
    verify_user(&db, owner_v["data"]["id"].as_str().expect("id")).await;

    for i in 0..3 {
        let uname = format!("orghit{i}");
        let (_, _) = signup_and_login(&app, &format!("{uname}@ex.com"), &uname).await;
    }

    // Short prefix → empty
    let (_, short_v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"o"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(short_v["ok"], true, "{short_v}");
    assert!(
        short_v["data"]["users"]
            .as_array()
            .expect("users")
            .is_empty(),
        "short prefix empty — {short_v}"
    );

    // Valid prefix → hits without email, capped
    let (_, hit_v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(hit_v["ok"], true, "{hit_v}");
    let users = hit_v["data"]["users"].as_array().expect("users");
    assert!(!users.is_empty(), "org* prefix should match — {hit_v}");
    assert!(users.len() <= 10, "≤10 — {hit_v}");
    for hit in users {
        assert!(hit.get("email").is_none(), "no email — {hit}");
        assert!(hit.get("username").is_some());
        assert!(hit.get("display_name").is_some());
    }

    // Email-shaped → empty
    let (_, email_v) = rpc_json(
        &app,
        r#"{"procedure":"user.lookup","input":{"prefix":"orghit0@ex.com"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(email_v["ok"], true, "{email_v}");
    assert!(
        email_v["data"]["users"]
            .as_array()
            .expect("users")
            .is_empty(),
        "email-shaped empty — {email_v}"
    );
}
