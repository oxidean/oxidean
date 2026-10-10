//! ORG-03 / ORG-04: repository collaborator CRUD + ACL matrix.
//!
//! Threat: T-10-01 web private deny → repo.not_found; T-10-02 Member none + Collaborator raise.

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

async fn rpc_json(app: &axum::Router, body: &str, cookie: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("rpc json body");
    // Success → 200; soft `repo.not_found` → 404. Other AppErrors may be 400.
    if status != StatusCode::OK && status != StatusCode::NOT_FOUND {
        // Allow domain errors used by these tests (exists / bad input) as JSON 400.
        let code = v["error"]["code"].as_str().unwrap_or("");
        assert!(
            code == "repo.collaborator_exists"
                || code == "rpc.bad_input"
                || code == "repo.invalid_permission"
                || code == "repo.user_not_found"
                || code == "repo.collaborator_not_found",
            "rpc http status {:?} for {body} — {v}",
            status
        );
    }
    v
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

/// Collaborator CRUD on a personal-owned repository (ORG-03 / D-ORG-04).
#[tokio::test]
async fn collab_crud_on_personal_repo() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("collab_personal.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "owner@ex.com", "owner1").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &owner_id).await;

    let (collab_cookie, collab_v) = signup_and_login(&app, "collab@ex.com", "collab1").await;
    let collab_id = collab_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &collab_id).await;

    let (stranger_cookie, stranger_v) =
        signup_and_login(&app, "stranger@ex.com", "stranger1").await;
    let stranger_id = stranger_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &stranger_id).await;

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"personal-priv","visibility":"private"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "create — {create}");

    // Non-admin cannot manage collaborators → soft not_found (T-10-02 / T-10-01).
    let deny = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"owner1","name":"personal-priv","username":"collab1","permission":"read"}}"#,
        &stranger_cookie,
    )
    .await;
    assert_eq!(deny["ok"], false, "stranger add must fail — {deny}");
    assert_eq!(deny["error"]["code"], "repo.not_found");

    let add = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"owner1","name":"personal-priv","username":"collab1","permission":"write"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(add["ok"], true, "owner add — {add}");
    assert_eq!(add["data"]["user_id"], collab_id);
    assert_eq!(add["data"]["username"], "collab1");
    assert_eq!(add["data"]["permission"], "write");

    let dup = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"owner1","name":"personal-priv","username":"collab1","permission":"read"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(dup["ok"], false, "duplicate add — {dup}");
    assert_eq!(dup["error"]["code"], "repo.collaborator_exists");

    let list = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.list","input":{"owner":"owner1","name":"personal-priv"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(list["ok"], true, "list — {list}");
    let collabs = list["data"]["collaborators"].as_array().expect("array");
    assert_eq!(collabs.len(), 1);
    assert_eq!(collabs[0]["username"], "collab1");
    assert_eq!(collabs[0]["permission"], "write");

    // Non-admin list → soft not_found (T-10-01).
    let list_deny = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.list","input":{"owner":"owner1","name":"personal-priv"}}"#,
        &collab_cookie,
    )
    .await;
    assert_eq!(list_deny["ok"], false, "collab list deny — {list_deny}");
    assert_eq!(list_deny["error"]["code"], "repo.not_found");

    let update = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"repo.collaborators.update","input":{{"owner":"owner1","name":"personal-priv","user_id":"{collab_id}","permission":"admin"}}}}"#
        ),
        &owner_cookie,
    )
    .await;
    assert_eq!(update["ok"], true, "update — {update}");
    assert_eq!(update["data"]["permission"], "admin");

    let remove = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"repo.collaborators.remove","input":{{"owner":"owner1","name":"personal-priv","user_id":"{collab_id}"}}}}"#
        ),
        &owner_cookie,
    )
    .await;
    assert_eq!(remove["ok"], true, "remove — {remove}");

    let list_empty = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.list","input":{"owner":"owner1","name":"personal-priv"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(list_empty["ok"], true);
    assert_eq!(
        list_empty["data"]["collaborators"]
            .as_array()
            .expect("array")
            .len(),
        0
    );
}

/// Collaborator CRUD on an org-owned repository (ORG-03 / D-ORG-04).
#[tokio::test]
async fn collab_crud_on_org_repo() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("collab_org.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "orgowner@ex.com", "orgowner1").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &owner_id).await;

    let (_out_cookie, out_v) = signup_and_login(&app, "outside@ex.com", "outside1").await;
    let outside_id = out_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &outside_id).await;

    let org = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"acme-collab"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(org["ok"], true, "org.create — {org}");

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"org-priv","visibility":"private","owner":"acme-collab"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "org repo create — {create}");

    let add = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"acme-collab","name":"org-priv","username":"outside1","permission":"read"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(add["ok"], true, "org owner add outside collab — {add}");
    assert_eq!(add["data"]["user_id"], outside_id);
    assert_eq!(add["data"]["permission"], "read");

    let list = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.list","input":{"owner":"acme-collab","name":"org-priv"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(list["ok"], true, "list — {list}");
    let collabs = list["data"]["collaborators"].as_array().expect("array");
    assert_eq!(collabs.len(), 1);
    assert_eq!(collabs[0]["username"], "outside1");

    let update = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"repo.collaborators.update","input":{{"owner":"acme-collab","name":"org-priv","user_id":"{outside_id}","permission":"write"}}}}"#
        ),
        &owner_cookie,
    )
    .await;
    assert_eq!(update["ok"], true, "update — {update}");
    assert_eq!(update["data"]["permission"], "write");

    let remove = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"repo.collaborators.remove","input":{{"owner":"acme-collab","name":"org-priv","user_id":"{outside_id}"}}}}"#
        ),
        &owner_cookie,
    )
    .await;
    assert_eq!(remove["ok"], true, "remove — {remove}");
}

/// Collaborator permission ladder: read | write | admin (ORG-03 / D-ORG-02c).
#[tokio::test]
async fn collab_permission_read_write_admin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("collab_perms.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "permowner@ex.com", "permowner1").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &owner_id).await;

    for name in ["uread1", "uwrite1", "uadmin1"] {
        let (_c, v) = signup_and_login(&app, &format!("{name}@ex.com"), name).await;
        let id = v["data"]["id"].as_str().expect("id").to_string();
        verify_user(&db, &id).await;
    }

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"ladder","visibility":"private"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "create — {create}");

    for (username, permission) in [
        ("uread1", "read"),
        ("uwrite1", "write"),
        ("uadmin1", "admin"),
    ] {
        let add = rpc_json(
            &app,
            &format!(
                r#"{{"procedure":"repo.collaborators.add","input":{{"owner":"permowner1","name":"ladder","username":"{username}","permission":"{permission}"}}}}"#
            ),
            &owner_cookie,
        )
        .await;
        assert_eq!(add["ok"], true, "add {username} — {add}");
        assert_eq!(add["data"]["permission"], permission);
    }

    let bad = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"permowner1","name":"ladder","username":"uread1","permission":"triage"}}"#,
        &owner_cookie,
    )
    .await;
    // Either already exists (if bad perm rejected after lookup) or bad_input —
    // invalid permission must not succeed.
    assert_eq!(bad["ok"], false, "invalid permission — {bad}");
    let code = bad["error"]["code"].as_str().unwrap_or("");
    assert!(
        code == "rpc.bad_input"
            || code == "repo.invalid_permission"
            || code == "repo.collaborator_exists",
        "unexpected code {code} — {bad}"
    );

    // Fresh user + invalid permission → bad_input / invalid_permission.
    let (_fresh_c, fresh_v) = signup_and_login(&app, "fresh@ex.com", "fresh1").await;
    let fresh_id = fresh_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &fresh_id).await;
    let bad2 = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"permowner1","name":"ladder","username":"fresh1","permission":"maintain"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(bad2["ok"], false, "invalid perm fresh — {bad2}");
    let code2 = bad2["error"]["code"].as_str().unwrap_or("");
    assert!(
        code2 == "rpc.bad_input" || code2 == "repo.invalid_permission",
        "unexpected code {code2} — {bad2}"
    );
}

/// Unauthorized private → soft `repo.not_found` on web RPC (ORG-04 / D-ORG-05 / T-10-01).
/// Covered by `repo_private_404` org Owner vs stranger cases in plan 04; grant path below.
#[tokio::test]
#[ignore = "covered by repo_private_404 stranger cases"]
async fn collab_unauthorized_private_soft_not_found_web() {
    panic!("Wave 0: private non-grantee web path → repo.not_found (ORG-04 / D-ORG-05 / D-25)");
}

/// Collaborator grant raises Member with member_base=none on private org repo (ORG-02/03 / T-10-02).
#[tokio::test]
async fn collab_raises_member_base_none_on_private_org_repo() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("collab_raise.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "raiseown@ex.com", "raiseown1").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;

    let (member_cookie, member_v) = signup_and_login(&app, "raisemem@ex.com", "raisemem1").await;
    let member_id = member_v["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &member_id).await;

    let org = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"raise-org"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(org["ok"], true, "{org}");

    // member_base defaults to none — Member alone cannot read private org repos.
    let add_member = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"raise-org","username":"raisemem1","role":"member"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(add_member["ok"], true, "{add_member}");

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","owner":"raise-org"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let denied = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"raise-org","name":"secret"}}"#,
        &member_cookie,
    )
    .await;
    assert_eq!(denied["ok"], false, "member base none → deny — {denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found");

    let grant = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"raise-org","name":"secret","username":"raisemem1","permission":"read"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(grant["ok"], true, "collab grant — {grant}");

    let raised = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"raise-org","name":"secret"}}"#,
        &member_cookie,
    )
    .await;
    assert_eq!(raised["ok"], true, "collaborator raise — {raised}");
    assert_eq!(raised["data"]["can_admin"], false);
    assert_eq!(raised["data"]["can_write"], false);
}

/// Visibility changes remain admin-gated with collaborators present (ORG-03).
#[tokio::test]
async fn collab_visibility_change_requires_admin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("collab_vis.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "visorg@ex.com", "visorg1").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;

    let (read_cookie, read_v) = signup_and_login(&app, "visread@ex.com", "visread1").await;
    verify_user(&db, read_v["data"]["id"].as_str().unwrap()).await;

    let (admin_cookie, admin_v) = signup_and_login(&app, "visadmin@ex.com", "visadmin1").await;
    verify_user(&db, admin_v["data"]["id"].as_str().unwrap()).await;

    let org = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"vis-org"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(org["ok"], true, "{org}");

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"vis-repo","visibility":"private","owner":"vis-org"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"vis-org","name":"vis-repo","username":"visread1","permission":"read"}}"#,
        &owner_cookie,
    )
    .await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"vis-org","name":"vis-repo","username":"visadmin1","permission":"admin"}}"#,
        &owner_cookie,
    )
    .await;

    // Org Owner (capability Admin) can toggle visibility — not mere owner_id equality.
    let owner_vis = rpc_json(
        &app,
        r#"{"procedure":"repo.updateVisibility","input":{"owner":"vis-org","name":"vis-repo","visibility":"public"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(
        owner_vis["ok"], true,
        "org Owner must update visibility via Admin capability — {owner_vis}"
    );
    assert_eq!(owner_vis["data"]["visibility"], "public");

    // Read collaborator cannot change visibility → soft not_found.
    let read_deny = rpc_json(
        &app,
        r#"{"procedure":"repo.updateVisibility","input":{"owner":"vis-org","name":"vis-repo","visibility":"private"}}"#,
        &read_cookie,
    )
    .await;
    assert_eq!(read_deny["ok"], false, "read collab deny — {read_deny}");
    assert_eq!(read_deny["error"]["code"], "repo.not_found");

    // Admin collaborator can change visibility.
    let admin_vis = rpc_json(
        &app,
        r#"{"procedure":"repo.updateVisibility","input":{"owner":"vis-org","name":"vis-repo","visibility":"private"}}"#,
        &admin_cookie,
    )
    .await;
    assert_eq!(
        admin_vis["ok"], true,
        "admin collab must update visibility — {admin_vis}"
    );
    assert_eq!(admin_vis["data"]["visibility"], "private");
}
