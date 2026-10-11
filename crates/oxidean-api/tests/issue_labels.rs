//! ISS-03: org+repo label defs Admin-only; Write+ assign; effective merge (D-ISS-05 / D-ISS-07).
//!
//! Threat: T-11-02 — Admin for defs; Write+ for assign.

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

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Admin can create org-default and repo-local label definitions (ISS-03 / D-ISS-05 / D-ISS-07).
#[tokio::test]
async fn issue_labels_admin_create_org_and_repo_defs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_labels_admin_create.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "lbladmin@ex.com", "lbladmin").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;

    let org = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"org.create","input":{"slug":"lbl-org","display_name":"Label Org"}}"#,
    )
    .await;
    assert_eq!(org["ok"], true, "org.create — {org}");

    let repo = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"widgets","visibility":"public","owner":"lbl-org"}}"#,
    )
    .await;
    assert_eq!(repo["ok"], true, "repo.create — {repo}");

    let org_label = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"label.create","input":{"scope":"org","owner":"lbl-org","name":"bug","color":"d73a4a","description":"Something broken"}}"#,
    )
    .await;
    assert_eq!(org_label["ok"], true, "label.create org — {org_label}");
    assert_eq!(org_label["data"]["name"], "bug");
    assert_eq!(org_label["data"]["scope"], "org");
    assert_eq!(org_label["data"]["color"], "d73a4a");

    let repo_label = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"label.create","input":{"scope":"repo","owner":"lbl-org","repo":"widgets","name":"needs-docs","color":"0075ca","description":"Docs"}}"#,
    )
    .await;
    assert_eq!(repo_label["ok"], true, "label.create repo — {repo_label}");
    assert_eq!(repo_label["data"]["scope"], "repo");
    assert_eq!(repo_label["data"]["name"], "needs-docs");

    let dup = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"label.create","input":{"scope":"org","owner":"lbl-org","name":"Bug","color":"ffffff"}}"#,
    )
    .await;
    assert_eq!(dup["ok"], false, "duplicate name within org scope — {dup}");
    assert_eq!(dup["error"]["code"], "rpc.bad_input");

    let listed = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"label.listForOrg","input":{"slug":"lbl-org"}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "label.listForOrg — {listed}");
    let labels = listed["data"]["labels"].as_array().expect("labels");
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0]["name"], "bug");
}

/// Non-Admin Write cannot create/edit/delete label definitions (D-ISS-07 / T-11-02).
#[tokio::test]
async fn issue_labels_write_cannot_mutate_defs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_labels_write_denied.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "lblown@ex.com", "lblown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    verify_user(&db, owner_id).await;

    let org = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"org.create","input":{"slug":"deny-org"}}"#,
    )
    .await;
    assert_eq!(org["ok"], true, "org.create — {org}");
    let repo = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"app","visibility":"public","owner":"deny-org"}}"#,
    )
    .await;
    assert_eq!(repo["ok"], true, "repo.create — {repo}");

    let created = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"label.create","input":{"scope":"repo","owner":"deny-org","repo":"app","name":"seed","color":"111111"}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "seed label — {created}");
    let label_id = created["data"]["id"].as_str().expect("id");

    let (writer_cookie, writer_v) = signup_and_login(&app, "lblwrite@ex.com", "lblwrite").await;
    let writer_id = writer_v["data"]["id"].as_str().expect("id");
    verify_user(&db, writer_id).await;
    let add = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"deny-org","name":"app","username":"lblwrite","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "add write collab — {add}");

    let create_denied = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"label.create","input":{"scope":"repo","owner":"deny-org","repo":"app","name":"hack","color":"222222"}}"#,
    )
    .await;
    assert_eq!(
        create_denied["ok"], false,
        "Write must not create defs — {create_denied}"
    );
    assert_eq!(create_denied["error"]["code"], "repo.not_found");

    let update_denied = rpc_json(
        &app,
        &writer_cookie,
        &format!(
            r#"{{"procedure":"label.update","input":{{"id":"{label_id}","owner":"deny-org","repo":"app","name":"renamed"}}}}"#
        ),
    )
    .await;
    assert_eq!(
        update_denied["ok"], false,
        "Write must not update defs — {update_denied}"
    );
    assert_eq!(update_denied["error"]["code"], "repo.not_found");

    let delete_denied = rpc_json(
        &app,
        &writer_cookie,
        &format!(
            r#"{{"procedure":"label.delete","input":{{"id":"{label_id}","owner":"deny-org","repo":"app"}}}}"#
        ),
    )
    .await;
    assert_eq!(
        delete_denied["ok"], false,
        "Write must not delete defs — {delete_denied}"
    );
    assert_eq!(delete_denied["error"]["code"], "repo.not_found");
}

/// Effective label set merges org defaults with repo hide/local-only overrides (D-ISS-05).
#[tokio::test]
async fn issue_labels_effective_set_org_plus_repo_overrides() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_labels_effective.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "efflbl@ex.com", "efflbl").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;

    assert_eq!(
        rpc_json(
            &app,
            &cookie,
            r#"{"procedure":"org.create","input":{"slug":"eff-org"}}"#,
        )
        .await["ok"],
        true
    );
    assert_eq!(
        rpc_json(
            &app,
            &cookie,
            r#"{"procedure":"repo.create","input":{"name":"svc","visibility":"public","owner":"eff-org"}}"#,
        )
        .await["ok"],
        true
    );

    let org_bug = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"label.create","input":{"scope":"org","owner":"eff-org","name":"bug","color":"d73a4a"}}"#,
    )
    .await;
    assert_eq!(org_bug["ok"], true, "{org_bug}");
    let bug_id = org_bug["data"]["id"].as_str().expect("id").to_string();

    let org_help = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"label.create","input":{"scope":"org","owner":"eff-org","name":"help","color":"0e8a16"}}"#,
    )
    .await;
    assert_eq!(org_help["ok"], true, "{org_help}");

    let local = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"label.create","input":{"scope":"repo","owner":"eff-org","repo":"svc","name":"local-only","color":"5319e7"}}"#,
    )
    .await;
    assert_eq!(local["ok"], true, "{local}");

    let hide = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"label.update","input":{{"id":"{bug_id}","owner":"eff-org","repo":"svc","hidden":true}}}}"#
        ),
    )
    .await;
    assert_eq!(hide["ok"], true, "hide org label — {hide}");
    assert_eq!(hide["data"]["hidden"], true);

    let effective = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"label.listForRepo","input":{"owner":"eff-org","name":"svc"}}"#,
    )
    .await;
    assert_eq!(effective["ok"], true, "listForRepo — {effective}");
    let labels = effective["data"]["labels"].as_array().expect("labels");
    let names: Vec<&str> = labels
        .iter()
        .map(|l| l["name"].as_str().unwrap_or(""))
        .collect();
    assert!(
        names.contains(&"help"),
        "inherited org label present — {names:?}"
    );
    assert!(
        names.contains(&"local-only"),
        "repo-local present — {names:?}"
    );
    assert!(
        !names.contains(&"bug"),
        "hidden org label excluded from effective set — {names:?}"
    );

    let with_hidden = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"label.listForRepo","input":{"owner":"eff-org","name":"svc","includeHidden":true}}"#,
    )
    .await;
    assert_eq!(with_hidden["ok"], true, "{with_hidden}");
    let all = with_hidden["data"]["labels"].as_array().expect("labels");
    let bug = all
        .iter()
        .find(|l| l["name"] == "bug")
        .expect("hidden bug visible when includeHidden");
    assert_eq!(bug["hidden"], true);
}

/// Write+ can assign/unassign labels on an issue (ISS-03 / D-ISS-07).
#[tokio::test]
async fn issue_labels_write_assign_on_issue() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_labels_assign.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "asglbl@ex.com", "asglbl").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    verify_user(&db, owner_id).await;

    assert_eq!(
        rpc_json(
            &app,
            &owner_cookie,
            r#"{"procedure":"org.create","input":{"slug":"asg-org"}}"#,
        )
        .await["ok"],
        true
    );
    assert_eq!(
        rpc_json(
            &app,
            &owner_cookie,
            r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","owner":"asg-org"}}"#,
        )
        .await["ok"],
        true
    );
    assert_eq!(
        rpc_json(
            &app,
            &owner_cookie,
            r#"{"procedure":"issue.create","input":{"owner":"asg-org","name":"core","title":"Labeled","body":""}}"#,
        )
        .await["ok"],
        true
    );

    let lab = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"label.create","input":{"scope":"org","owner":"asg-org","name":"prio","color":"fbca04"}}"#,
    )
    .await;
    assert_eq!(lab["ok"], true, "{lab}");
    let label_id = lab["data"]["id"].as_str().expect("id").to_string();

    let (writer_cookie, writer_v) = signup_and_login(&app, "asgwrite@ex.com", "asgwrite").await;
    let writer_id = writer_v["data"]["id"].as_str().expect("id");
    verify_user(&db, writer_id).await;
    assert_eq!(
        rpc_json(
            &app,
            &owner_cookie,
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"asg-org","name":"core","username":"asgwrite","permission":"write"}}"#,
        )
        .await["ok"],
        true
    );

    let (reader_cookie, reader_v) = signup_and_login(&app, "asgread@ex.com", "asgread").await;
    let reader_id = reader_v["data"]["id"].as_str().expect("id");
    verify_user(&db, reader_id).await;
    assert_eq!(
        rpc_json(
            &app,
            &owner_cookie,
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"asg-org","name":"core","username":"asgread","permission":"read"}}"#,
        )
        .await["ok"],
        true
    );

    let set_ok = rpc_json(
        &app,
        &writer_cookie,
        &format!(
            r#"{{"procedure":"issue.labels.set","input":{{"owner":"asg-org","name":"core","number":1,"labelIds":["{label_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(set_ok["ok"], true, "Write+ set labels — {set_ok}");
    let assigned = set_ok["data"]["labels"].as_array().expect("labels");
    assert_eq!(assigned.len(), 1);
    assert_eq!(assigned[0]["id"], label_id);

    let clear = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"issue.labels.set","input":{"owner":"asg-org","name":"core","number":1,"labelIds":[]}}"#,
    )
    .await;
    assert_eq!(clear["ok"], true, "clear labels — {clear}");
    assert_eq!(clear["data"]["labels"].as_array().unwrap().len(), 0);

    let read_denied = rpc_json(
        &app,
        &reader_cookie,
        &format!(
            r#"{{"procedure":"issue.labels.set","input":{{"owner":"asg-org","name":"core","number":1,"labelIds":["{label_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(read_denied["ok"], false, "Read cannot set — {read_denied}");
    assert_eq!(read_denied["error"]["code"], "repo.not_found");

    // Hide then reject assignment of hidden id.
    assert_eq!(
        rpc_json(
            &app,
            &owner_cookie,
            &format!(
                r#"{{"procedure":"label.update","input":{{"id":"{label_id}","owner":"asg-org","repo":"core","hidden":true}}}}"#
            ),
        )
        .await["ok"],
        true
    );
    let hidden_denied = rpc_json(
        &app,
        &writer_cookie,
        &format!(
            r#"{{"procedure":"issue.labels.set","input":{{"owner":"asg-org","name":"core","number":1,"labelIds":["{label_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(
        hidden_denied["ok"], false,
        "hidden label id rejected — {hidden_denied}"
    );
    assert_eq!(hidden_denied["error"]["code"], "rpc.bad_input");
}
