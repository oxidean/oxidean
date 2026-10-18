//! Actions configuration variables — repo/org/environment store, merge
//! precedence (narrowest scope wins, like GitHub), RPC surface, and
//! `vars` propagation into claimed tasks.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::actions::mint_registration_token;
use oxidean_api::actions::vars::{variables_for_context, variables_for_job};
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use tower::ServiceExt;

async fn test_app(db: Database) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_actions_enabled(true);
    router_with_state(state, build_cors("development", None).unwrap())
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

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn session_cookie_from_response(res: &axum::http::Response<Body>) -> String {
    res.headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now).await.unwrap();
}

async fn signup_login_verify(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
) -> (String, String) {
    let _ = app
        .clone()
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
        )))
        .await
        .unwrap();
    let login = app
        .clone()
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
        )))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let user_id = v["data"]["id"].as_str().unwrap().to_string();
    verify_user(db, &user_id).await;
    (cookie, user_id)
}

/// Repo variables round-trip — unlike secrets, `list` returns plaintext
/// values (GitHub exposes variable values on GET).
#[tokio::test]
async fn actions_vars_repo_rpc_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("v.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;
    let (cookie, _uid) = signup_login_verify(&app, &db, "vars@ex.com", "varown").await;

    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"varrepo","visibility":"private"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "create — {create}");

    let put = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.actions.variables.put","input":{"owner":"varown","name":"varrepo","variable_name":"REGION","value":"us-east"}}"#,
    )
    .await;
    assert_eq!(put["ok"], true, "put — {put}");

    // Upsert on the same name updates the value.
    let put2 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.actions.variables.put","input":{"owner":"varown","name":"varrepo","variable_name":"REGION","value":"eu-west"}}"#,
    )
    .await;
    assert_eq!(put2["ok"], true, "put2 — {put2}");

    let list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.actions.variables.list","input":{"owner":"varown","name":"varrepo"}}"#,
    )
    .await;
    assert_eq!(list["ok"], true, "list — {list}");
    let vars = list["data"]["variables"].as_array().unwrap();
    assert_eq!(vars.len(), 1, "upsert replaces, not duplicates — {list}");
    assert_eq!(vars[0]["name"], "REGION");
    // Variables are plaintext config — the value is returned, unlike secrets.
    assert_eq!(vars[0]["value"], "eu-west");

    let del = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.actions.variables.delete","input":{"owner":"varown","name":"varrepo","variable_name":"REGION"}}"#,
    )
    .await;
    assert_eq!(del["ok"], true, "delete — {del}");
    let list2 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.actions.variables.list","input":{"owner":"varown","name":"varrepo"}}"#,
    )
    .await;
    assert_eq!(
        list2["data"]["variables"].as_array().unwrap().len(),
        0,
        "deleted variable gone — {list2}"
    );
}

/// GitHub's variable-name rules + value cap are enforced at the RPC edge.
#[tokio::test]
async fn actions_vars_validation() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("vv.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;
    let (cookie, _uid) = signup_login_verify(&app, &db, "vv@ex.com", "vvown").await;
    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"vvrepo","visibility":"private"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "create — {create}");

    for bad in ["9LIVES", "GITHUB_TOKEN", "has-dash", "has space"] {
        let body = format!(
            r#"{{"procedure":"repo.actions.variables.put","input":{{"owner":"vvown","name":"vvrepo","variable_name":"{bad}","value":"v"}}}}"#
        );
        let res = rpc_json(&app, &cookie, &body).await;
        assert_eq!(res["ok"], false, "name '{bad}' must be rejected — {res}");
    }

    let big = "x".repeat(49 * 1024);
    let body = format!(
        r#"{{"procedure":"repo.actions.variables.put","input":{{"owner":"vvown","name":"vvrepo","variable_name":"TOO_BIG","value":"{big}"}}}}"#
    );
    let res = rpc_json(&app, &cookie, &body).await;
    assert_eq!(res["ok"], false, "over-limit value rejected — {res}");

    // Empty values are legal on GitHub.
    let ok = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.actions.variables.put","input":{"owner":"vvown","name":"vvrepo","variable_name":"EMPTY_OK","value":""}}"#,
    )
    .await;
    assert_eq!(ok["ok"], true, "empty value allowed — {ok}");
}

/// Merge precedence — narrowest scope wins on name collisions:
/// environment > repository > organization.
#[tokio::test]
async fn actions_vars_merge_precedence() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("mp.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();

    let org = db
        .insert_organization("org-mp", "mp-org", "MP Org", "read")
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-mp", &org.id, "org", "mp-demo", "private", "", "main")
        .await
        .unwrap();

    db.upsert_action_org_variable("ov1", &org.id, "SHARED", "org-val", "all")
        .await
        .unwrap();
    db.upsert_action_org_variable("ov2", &org.id, "ORG_ONLY", "org-only", "all")
        .await
        .unwrap();
    db.upsert_action_variable("rv1", &repo.id, "SHARED", "repo-val")
        .await
        .unwrap();
    let env = db
        .ensure_action_environment("env-mp", &repo.id, "prod")
        .await
        .unwrap();
    db.upsert_action_environment_variable("ev1", &env.id, "SHARED", "env-val")
        .await
        .unwrap();

    // Job-scoped merge: env beats repo beats org.
    let job_vars = variables_for_job(&db, &repo.id, Some("prod"))
        .await
        .unwrap();
    assert_eq!(job_vars.get("SHARED").map(String::as_str), Some("env-val"));
    assert_eq!(
        job_vars.get("ORG_ONLY").map(String::as_str),
        Some("org-only")
    );

    // No environment → repo wins over org.
    let job_vars = variables_for_job(&db, &repo.id, None).await.unwrap();
    assert_eq!(job_vars.get("SHARED").map(String::as_str), Some("repo-val"));

    // Run-level `vars` context (enqueue/if evaluation) merges org+repo too.
    let ctx = variables_for_context(&db, &repo.id).await.unwrap();
    assert_eq!(ctx["SHARED"], "repo-val");
    assert_eq!(ctx["ORG_ONLY"], "org-only");
}

/// Org `selected` visibility only flows to allow-listed repos; `private`
/// only to private repos.
#[tokio::test]
async fn actions_vars_org_visibility_scoping() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("vs.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let org = db
        .insert_organization("org-vs", "vs-org", "VS Org", "read")
        .await
        .unwrap();
    let allowed = db
        .insert_repository("r-vs-a", &org.id, "org", "allowed", "private", "", "main")
        .await
        .unwrap();
    let denied = db
        .insert_repository("r-vs-d", &org.id, "org", "denied", "private", "", "main")
        .await
        .unwrap();
    let public = db
        .insert_repository("r-vs-p", &org.id, "org", "pubrepo", "public", "", "main")
        .await
        .unwrap();

    db.upsert_action_org_variable("sv1", &org.id, "SEL", "sel-val", "selected")
        .await
        .unwrap();
    db.set_action_org_variable_repos("sv1", std::slice::from_ref(&allowed.id))
        .await
        .unwrap();
    db.upsert_action_org_variable("pv1", &org.id, "PRIV", "priv-val", "private")
        .await
        .unwrap();
    db.upsert_action_org_variable("av1", &org.id, "ALLV", "all-val", "all")
        .await
        .unwrap();

    let a = variables_for_job(&db, &allowed.id, None).await.unwrap();
    assert_eq!(a.get("SEL").map(String::as_str), Some("sel-val"));
    assert_eq!(a.get("PRIV").map(String::as_str), Some("priv-val"));
    assert_eq!(a.get("ALLV").map(String::as_str), Some("all-val"));

    let d = variables_for_job(&db, &denied.id, None).await.unwrap();
    assert!(!d.contains_key("SEL"), "selected var must not leak");
    assert_eq!(d.get("PRIV").map(String::as_str), Some("priv-val"));

    // `private` visibility does not reach public repos.
    let p = variables_for_job(&db, &public.id, None).await.unwrap();
    assert!(!p.contains_key("PRIV"), "private var must not leak");
    assert_eq!(p.get("ALLV").map(String::as_str), Some("all-val"));

    // A user-owned repo sees nothing org-scoped.
    let u = db
        .create_user(
            "u-vs",
            "vs@example.com",
            "vsown",
            Some("h"),
            "V",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let user_repo = db
        .insert_repository("r-vs-u", &u.id, "user", "userrepo", "private", "", "main")
        .await
        .unwrap();
    let u_vars = variables_for_job(&db, &user_repo.id, None).await.unwrap();
    assert!(u_vars.is_empty(), "user repo must not see org vars");
}

/// Variables ride the fetch_task response into the runner (`vars` map).
#[tokio::test]
async fn actions_vars_injected_into_fetch_task() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("ft.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let owner = db
        .create_user(
            "u-vft",
            "vft@example.com",
            "vftown",
            Some("h"),
            "F",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-vft", &owner.id, "user", "vftown", "private", "", "main")
        .await
        .unwrap();
    db.upsert_action_variable("v-ft", &repo.id, "REGION", "us-east")
        .await
        .unwrap();
    db.insert_action_run(
        "run-vft",
        &repo.id,
        ".github/workflows/ci.yml",
        "CI",
        "push",
        "abc",
        "refs/heads/main",
        "CI",
        None,
        None,
        None,
    )
    .await
    .unwrap();
    db.insert_action_payload(
        "sha-vft",
        &repo.id,
        ".github/workflows/ci.yml",
        "abc",
        r#"{"version":1,"steps":[{"run":"echo hi"}],"contents_permission":"read"}"#,
    )
    .await
    .unwrap();
    let mut ft = oxidean_db::JobInsert::basic(
        "job-vft",
        "run-vft",
        "build",
        "build",
        r#"["ubuntu-latest"]"#,
    );
    ft.payload_sha = Some("sha-vft".into());
    db.insert_action_job(ft).await.unwrap();

    let app = test_app(db.clone()).await;
    let reg = mint_registration_token(&db).await.unwrap();
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "r1",
                        "labels": ["ubuntu-latest"],
                        "token": reg
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let reg_v: serde_json::Value =
        serde_json::from_slice(&reg_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let runner_token = reg_v["runner_token"].as_str().unwrap();

    let fetch = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/fetch_task")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {runner_token}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fetch.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&fetch.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["job_id"], "job-vft");
    assert_eq!(v["vars"]["REGION"], "us-east", "vars must reach the runner");
}
