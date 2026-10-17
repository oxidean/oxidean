//! Workflow template catalog (Unit B): three-source merge, suggested
//! detection, $default-branch substitution, workflows.create paths (direct,
//! new_branch+PR, existing PR head, fork rejection), admin CRUD + enabled
//! filtering, and the anonymous icon route.

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

fn rpc_req(body: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1");
    if let Some(c) = cookie {
        b = b.header("cookie", c);
    }
    b.body(Body::from(body.to_owned())).unwrap()
}

async fn rpc_json(app: &axum::Router, body: &str, cookie: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req(body, Some(cookie)))
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("rpc json body: {e} (status={status}) body={bytes:?}"))
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

async fn signup_and_login(
    app: &axum::Router,
    email: &str,
    username: &str,
) -> (String, serde_json::Value) {
    let signup = app
        .clone()
        .oneshot(rpc_req(
            &format!(
                r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
            ),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;

    let login = app
        .clone()
        .oneshot(rpc_req(
            &format!(
                r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
            ),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    (cookie, serde_json::from_slice(&bytes).unwrap())
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

async fn login_as_admin(db: &Database) -> String {
    let admins = db.list_users_page(None, 10, 0).await.expect("list");
    let admin = admins
        .0
        .into_iter()
        .find(|u| u.role.is_sys_admin())
        .expect("sys-admin from unlock_signup");
    let sessions = oxidean_api::auth::SessionService::new("development");
    let (raw, _cookie) = sessions
        .create(db, &admin.id, false, None, None)
        .await
        .expect("session");
    format!("oxidean_session={raw}")
}

async fn create_repo(
    app: &axum::Router,
    cookie: &str,
    owner_field: Option<&str>,
    name: &str,
) -> serde_json::Value {
    let owner = owner_field
        .map(|o| format!(r#""owner":"{o}","#))
        .unwrap_or_default();
    rpc_json(
        app,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"public",{owner}"stack_id":"rust"}}}}"#
        ),
        cookie,
    )
    .await
}

async fn branch_create(app: &axum::Router, cookie: &str, owner: &str, name: &str, branch: &str) {
    let v = rpc_json(
        app,
        &format!(
            r#"{{"procedure":"repo.branchCreate","input":{{"owner":"{owner}","name":"{name}","branch":"{branch}","start":"main"}}}}"#
        ),
        cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "branchCreate — {v}");
}

async fn blob_content(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    repo: &str,
    ref_name: &str,
    path: &str,
) -> serde_json::Value {
    rpc_json(
        app,
        &format!(
            r#"{{"procedure":"repo.blob","input":{{"owner":"{owner}","name":"{repo}","ref":"{ref_name}","path":"{path}"}}}}"#
        ),
        cookie,
    )
    .await
}

/// Three-source merge: builtin + instance (admin) + org `.github` templates;
/// Cargo.toml fixture suggests `builtin:ci/rust`.
#[tokio::test]
async fn workflow_templates_list_merges_three_sources() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wt_list.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "own@ex.com", "ownu").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;

    // Org + org `.github` repo with a workflow-templates/ dir.
    let org = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"acme-tpl","display_name":"Acme"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(org["ok"], true, "org.create — {org}");
    let gh = create_repo(&app, &cookie, Some("acme-tpl"), ".github").await;
    assert_eq!(gh["ok"], true, "create .github — {gh}");
    let tpl = rpc_json(
        &app,
        r#"{"procedure":"repo.file.create","input":{"owner":"acme-tpl","name":".github","path":"workflow-templates/org-ci.yml","content":"name: Org CI\non: [push]\njobs:\n  build:\n    runs-on: linux\n    steps:\n      - run: echo hi\n","message":"add org template"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(tpl["ok"], true, "org template file — {tpl}");

    // Org-owned repo with a rust stack (Cargo.toml in the root tree).
    let app_repo = create_repo(&app, &cookie, Some("acme-tpl"), "app").await;
    assert_eq!(app_repo["ok"], true, "create app — {app_repo}");

    // Instance template via admin CRUD (needs sys-admin session).
    let admin_cookie = login_as_admin(&db).await;
    let inst = rpc_json(
        &app,
        r#"{"procedure":"admin.workflowTemplates.create","input":{"slug":"inst-lint","name":"Instance Lint","description":"admin-managed","categories":["Automation"],"content":"name: Lint\non: [push]\njobs:\n  lint:\n    runs-on: linux\n    steps:\n      - run: echo ok\n"}}"#,
        &admin_cookie,
    )
    .await;
    assert_eq!(inst["ok"], true, "admin create — {inst}");

    // Default request: no pagination — every template + total_count.
    let unpaged = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflowTemplates.list","input":{"owner":"acme-tpl","name":"app"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(unpaged["ok"], true, "unpaged — {unpaged}");
    let total = unpaged["data"]["total_count"]
        .as_u64()
        .expect("total_count");
    assert!(
        total >= 89,
        "87 builtin + 1 instance + 1 org expected, got {total}"
    );
    assert_eq!(
        unpaged["data"]["templates"].as_array().unwrap().len() as u64,
        total
    );

    // Opt-in pagination: per_page limits the page but not the count.
    let paged = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflowTemplates.list","input":{"owner":"acme-tpl","name":"app","per_page":5}}"#,
        &cookie,
    )
    .await;
    assert_eq!(paged["ok"], true, "{paged}");
    assert_eq!(paged["data"]["templates"].as_array().unwrap().len(), 5);
    assert_eq!(paged["data"]["total_count"].as_u64().unwrap(), total);

    let list = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflowTemplates.list","input":{"owner":"acme-tpl","name":"app"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(list["ok"], true, "list — {list}");
    let templates = list["data"]["templates"].as_array().expect("templates");
    let ids: Vec<&str> = templates.iter().filter_map(|t| t["id"].as_str()).collect();
    assert!(
        ids.contains(&"builtin:ci/rust"),
        "builtin missing — {ids:?}"
    );
    assert!(
        ids.contains(&"instance:inst-lint"),
        "instance missing — {ids:?}"
    );
    assert!(ids.contains(&"org:org-ci"), "org missing — {ids:?}");

    let rust = templates
        .iter()
        .find(|t| t["id"] == "builtin:ci/rust")
        .expect("rust");
    assert_eq!(rust["suggested"], true, "Cargo.toml should suggest rust");
    assert_eq!(rust["icon_url"], "/api/actions/template-icons/rust.svg");

    let org_tpl = templates
        .iter()
        .find(|t| t["id"] == "org:org-ci")
        .expect("org tpl");
    assert_eq!(org_tpl["source_label"], "@acme-tpl/.github");

    let suggested = list["data"]["suggested"].as_array().expect("suggested");
    assert!(
        suggested.iter().any(|t| t["id"] == "builtin:ci/rust"),
        "suggested should include rust — {suggested:?}"
    );
    let cats = list["data"]["categories"].as_array().expect("categories");
    assert!(cats.iter().any(|c| c == "Continuous integration"));

    // Source filter: org only.
    let org_only = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflowTemplates.list","input":{"owner":"acme-tpl","name":"app","source":"org"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(org_only["ok"], true, "{org_only}");
    assert_eq!(org_only["data"]["templates"].as_array().unwrap().len(), 1);

    // User-owned repo: no org source.
    let personal = create_repo(&app, &cookie, None, "mine").await;
    assert_eq!(personal["ok"], true, "{personal}");
    let mine = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflowTemplates.list","input":{"owner":"ownu","name":"mine","source":"org"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(mine["ok"], true, "{mine}");
    assert_eq!(mine["data"]["templates"].as_array().unwrap().len(), 0);
}

/// `get` returns content with `$default-branch` substituted.
#[tokio::test]
async fn workflow_templates_get_substitutes_default_branch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wt_get.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "g@ex.com", "getu").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let c = create_repo(&app, &cookie, None, "getrepo").await;
    assert_eq!(c["ok"], true, "{c}");

    let get = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflowTemplates.get","input":{"owner":"getu","name":"getrepo","id":"builtin:ci/rust"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(get["ok"], true, "get — {get}");
    let content = get["data"]["content"].as_str().expect("content");
    assert!(
        content.contains("branches: [ main ]"),
        "$default-branch must become main — {content}"
    );
    assert!(!content.contains("$default-branch"));
    assert_eq!(get["data"]["template"]["id"], "builtin:ci/rust");

    let missing = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflowTemplates.get","input":{"owner":"getu","name":"getrepo","id":"builtin:ci/nope"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(missing["ok"], false);
    assert_eq!(missing["error"]["code"], "actions.template_not_found");
}

/// `workflows.create` input validation: both/neither source, bad path, bad YAML.
#[tokio::test]
async fn workflows_create_input_validation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wt_create.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "c@ex.com", "createu").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let c = create_repo(&app, &cookie, None, "createrepo").await;
    assert_eq!(c["ok"], true, "{c}");

    // Both template_id and content.
    let both = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"createu","name":"createrepo","template_id":"builtin:ci/rust","content":"name: x\non: [push]\njobs:\n  j:\n    runs-on: linux\n    steps:\n      - run: echo ok\n","path":".github/workflows/x.yml"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(both["ok"], false);
    assert_eq!(both["error"]["code"], "rpc.bad_input");

    // Neither.
    let neither = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"createu","name":"createrepo","path":".github/workflows/x.yml"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(neither["ok"], false);
    assert_eq!(neither["error"]["code"], "rpc.bad_input");

    // Authored content without path.
    let nopath = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"createu","name":"createrepo","content":"name: x\non: [push]\njobs:\n  j:\n    runs-on: linux\n    steps:\n      - run: echo ok\n"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(nopath["ok"], false);
    assert_eq!(nopath["error"]["code"], "rpc.bad_input");

    // Path escaping the workflows dir.
    let badpath = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"createu","name":"createrepo","template_id":"builtin:ci/rust","path":"docs/evil.yml"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(badpath["ok"], false);
    assert_eq!(badpath["error"]["code"], "actions.invalid_workflow_path");

    // Nested path (only flat files allowed).
    let nested = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"createu","name":"createrepo","template_id":"builtin:ci/rust","path":".github/workflows/nested/x.yml"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(nested["ok"], false);
    assert_eq!(nested["error"]["code"], "actions.invalid_workflow_path");

    // Unparsable content.
    let badyaml = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"createu","name":"createrepo","content":"not: [a workflow\n","path":".github/workflows/bad.yml"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(badyaml["ok"], false);
    assert_eq!(badyaml["error"]["code"], "actions.invalid_workflow");

    // Unknown template id.
    let notfound = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"createu","name":"createrepo","template_id":"builtin:ci/ghost"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(notfound["ok"], false);
    assert_eq!(notfound["error"]["code"], "actions.template_not_found");
}

/// Template create: default path lands on main; authored content honours path.
#[tokio::test]
async fn workflows_create_direct_commit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wt_direct.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "d@ex.com", "directu").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let c = create_repo(&app, &cookie, None, "directrepo").await;
    assert_eq!(c["ok"], true, "{c}");

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"directu","name":"directrepo","template_id":"builtin:ci/rust"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "create — {create}");
    assert_eq!(create["data"]["branch"], "main");
    assert_eq!(create["data"]["created_branch"], false);

    let blob = blob_content(
        &app,
        &cookie,
        "directu",
        "directrepo",
        "main",
        ".github/workflows/rust.yml",
    )
    .await;
    assert_eq!(blob["ok"], true, "{blob}");
    let content = blob["data"]["content"].as_str().unwrap();
    assert!(content.contains("branches: [ main ]"));

    // Second create at the same default path → path conflict.
    let again = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"directu","name":"directrepo","template_id":"builtin:ci/rust"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(again["ok"], false);
    assert_eq!(again["error"]["code"], "repo.path_conflict");

    // Authored content with explicit path.
    let authored = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"directu","name":"directrepo","content":"name: Authored\non: [push]\njobs:\n  j:\n    runs-on: linux\n    steps:\n      - run: echo ok\n","path":".github/workflows/authored.yml","message":"add authored workflow"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(authored["ok"], true, "{authored}");
    let blob = blob_content(
        &app,
        &cookie,
        "directu",
        "directrepo",
        "main",
        ".github/workflows/authored.yml",
    )
    .await;
    assert_eq!(blob["data"]["content"], "name: Authored\non: [push]\njobs:\n  j:\n    runs-on: linux\n    steps:\n      - run: echo ok\n");
}

/// `workflows.create` onto an open same-repo PR head + fork PR rejection.
#[tokio::test]
async fn workflows_create_onto_pull_head() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wt_pr.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "p@ex.com", "pru").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let c = create_repo(&app, &cookie, None, "prrepo").await;
    assert_eq!(c["ok"], true, "{c}");
    branch_create(&app, &cookie, "pru", "prrepo", "feature").await;
    let pr = rpc_json(
        &app,
        r#"{"procedure":"pull.create","input":{"owner":"pru","name":"prrepo","title":"Feat","base_ref":"main","head_ref":"feature"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let number = pr["data"]["number"].as_i64().unwrap();

    // Authored content with only a `workflow_dispatch` trigger — committing a
    // push/pull_request-triggered workflow would enqueue runs through the
    // actions dispatch chain (deep async fan-out; covered by e2e instead).
    let body = r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"pru","name":"prrepo","content":"name: Manual\non: [workflow_dispatch]\njobs:\n  j:\n    runs-on: linux\n    steps:\n      - run: echo ok\n","path":".github/workflows/manual.yml","pull_number":N}}"#;
    let create = rpc_json(&app, &body.replace('N', &number.to_string()), &cookie).await;
    assert_eq!(create["ok"], true, "create onto PR head — {create}");
    assert_eq!(create["data"]["branch"], "feature");
    assert_eq!(create["data"]["created_branch"], false);
    assert!(create["data"]["pr_number"].is_null(), "{create}");

    let blob = blob_content(
        &app,
        &cookie,
        "pru",
        "prrepo",
        "feature",
        ".github/workflows/manual.yml",
    )
    .await;
    assert_eq!(blob["ok"], true, "workflow on PR head — {blob}");

    // Nonexistent PR.
    let nopr = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"pru","name":"prrepo","template_id":"builtin:ci/blank","pull_number":999}}"#,
        &cookie,
    )
    .await;
    assert_eq!(nopr["ok"], false);
    assert_eq!(nopr["error"]["code"], "pull.not_found");

    // Closed PR → rejected.
    let close = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pull.close","input":{{"owner":"pru","name":"prrepo","number":{number}}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(close["ok"], true, "{close}");
    let closed = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"repo.actions.workflows.create","input":{{"owner":"pru","name":"prrepo","template_id":"builtin:ci/blank","pull_number":{number}}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(closed["ok"], false);
    assert_eq!(closed["error"]["code"], "actions.pull_not_open");
}

/// Fork PR heads are refused with `actions.pull_from_fork`.
#[tokio::test]
async fn workflows_create_rejects_fork_pull() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wt_fork.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "src@ex.com", "srcown").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    let c = create_repo(&app, &owner_cookie, None, "upstream").await;
    assert_eq!(c["ok"], true, "{c}");
    branch_create(&app, &owner_cookie, "srcown", "upstream", "feature").await;

    let (fork_cookie, fork_v) = signup_and_login(&app, "fk@ex.com", "forkown").await;
    verify_user(&db, fork_v["data"]["id"].as_str().unwrap()).await;
    let forked = rpc_json(
        &app,
        r#"{"procedure":"repo.fork","input":{"owner":"srcown","name":"upstream"}}"#,
        &fork_cookie,
    )
    .await;
    assert_eq!(forked["ok"], true, "{forked}");

    let pr = rpc_json(
        &app,
        r#"{"procedure":"pull.create","input":{"owner":"srcown","name":"upstream","title":"From fork","base_ref":"main","head_ref":"feature","head_owner":"forkown","head_name":"upstream"}}"#,
        &fork_cookie,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let number = pr["data"]["number"].as_i64().unwrap();

    // Repo owner tries to push a workflow onto the fork PR head → refused.
    let create = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"repo.actions.workflows.create","input":{{"owner":"srcown","name":"upstream","template_id":"builtin:ci/rust","pull_number":{number}}}}}"#
        ),
        &owner_cookie,
    )
    .await;
    assert_eq!(create["ok"], false);
    assert_eq!(create["error"]["code"], "actions.pull_from_fork");
}

/// `new_branch` + `open_pr` without pull_number commits to a new branch and
/// opens a PR — same semantics as repo.file.create.
#[tokio::test]
async fn workflows_create_new_branch_opens_pr() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wt_nb.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "n@ex.com", "nbu").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let c = create_repo(&app, &cookie, None, "nbrepo").await;
    assert_eq!(c["ok"], true, "{c}");

    // `workflow_dispatch`-only content: committing a push/pull_request
    // template would enqueue runs through the deep actions dispatch chain.
    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"nbu","name":"nbrepo","content":"name: Manual\non: [workflow_dispatch]\njobs:\n  j:\n    runs-on: linux\n    steps:\n      - run: echo ok\n","path":".github/workflows/manual.yml","new_branch":"add-ci","open_pr":true}}"#,
        &cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "create — {create}");
    assert_eq!(create["data"]["branch"], "add-ci");
    assert_eq!(create["data"]["created_branch"], true);
    assert!(create["data"]["pr_number"].is_i64(), "PR opened — {create}");

    let blob = blob_content(
        &app,
        &cookie,
        "nbu",
        "nbrepo",
        "add-ci",
        ".github/workflows/manual.yml",
    )
    .await;
    assert_eq!(blob["ok"], true, "{blob}");
}

/// Admin CRUD + enabled filtering in the catalog.
#[tokio::test]
async fn workflow_templates_admin_crud() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wt_admin.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let admin_cookie = login_as_admin(&db).await;

    let (cookie, login_v) = signup_and_login(&app, "u@ex.com", "plainu").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;

    // Non-admin cannot create.
    let denied = rpc_json(
        &app,
        r#"{"procedure":"admin.workflowTemplates.create","input":{"slug":"x","name":"X","content":"name: x\non: [push]\njobs:\n  j:\n    runs-on: linux\n    steps:\n      - run: echo ok\n"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(denied["ok"], false);
    assert_eq!(denied["error"]["code"], "admin.forbidden");

    // Create (default filename <slug>.yml).
    let created = rpc_json(
        &app,
        r#"{"procedure":"admin.workflowTemplates.create","input":{"slug":"sec-scan","name":"Sec Scan","description":"d","categories":["Automation"],"file_patterns":["*.rs"],"content":"name: S\non: [push]\njobs:\n  j:\n    runs-on: linux\n    steps:\n      - run: echo ok\n"}}"#,
        &admin_cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "create — {created}");
    let id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["filename"], "sec-scan.yml");
    assert_eq!(created["data"]["enabled"], true);

    // Slug uniqueness + invalid slug.
    let dup = rpc_json(
        &app,
        r#"{"procedure":"admin.workflowTemplates.create","input":{"slug":"sec-scan","name":"Dup","content":"name: x\non: [push]\njobs:\n  j:\n    runs-on: linux\n    steps:\n      - run: echo ok\n"}}"#,
        &admin_cookie,
    )
    .await;
    assert_eq!(dup["error"]["code"], "admin.template_slug_taken");
    let badslug = rpc_json(
        &app,
        r#"{"procedure":"admin.workflowTemplates.create","input":{"slug":"bad slug","name":"X","content":"name: x\non: [push]\njobs:\n  j:\n    runs-on: linux\n    steps:\n      - run: echo ok\n"}}"#,
        &admin_cookie,
    )
    .await;
    assert_eq!(badslug["error"]["code"], "admin.invalid_template_slug");

    // Invalid workflow content rejected.
    let badcontent = rpc_json(
        &app,
        r#"{"procedure":"admin.workflowTemplates.create","input":{"slug":"badyaml","name":"X","content":"not: [valid\n"}}"#,
        &admin_cookie,
    )
    .await;
    assert_eq!(badcontent["error"]["code"], "actions.invalid_workflow");

    // List shows it.
    let list = rpc_json(
        &app,
        r#"{"procedure":"admin.workflowTemplates.list","input":{}}"#,
        &admin_cookie,
    )
    .await;
    assert_eq!(list["ok"], true, "{list}");
    assert!(
        list["data"]["templates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == id),
        "{list}"
    );

    // Update name + content.
    let updated = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.workflowTemplates.update","input":{{"id":"{id}","name":"Sec Scan v2","content":"name: S2\non: [push]\njobs:\n  j:\n    runs-on: linux\n    steps:\n      - run: echo ok\n"}}}}"#
        ),
        &admin_cookie,
    )
    .await;
    assert_eq!(updated["ok"], true, "{updated}");
    assert_eq!(updated["data"]["name"], "Sec Scan v2");

    // Repo catalog shows the enabled instance template; disabling hides it.
    let c = create_repo(&app, &cookie, None, "catrepo").await;
    assert_eq!(c["ok"], true, "{c}");
    let catalog = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflowTemplates.list","input":{"owner":"plainu","name":"catrepo","source":"instance"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(catalog["ok"], true, "{catalog}");
    assert_eq!(catalog["data"]["templates"].as_array().unwrap().len(), 1);

    let disabled = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.workflowTemplates.setEnabled","input":{{"id":"{id}","enabled":false}}}}"#
        ),
        &admin_cookie,
    )
    .await;
    assert_eq!(disabled["ok"], true, "{disabled}");
    let catalog2 = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflowTemplates.list","input":{"owner":"plainu","name":"catrepo","source":"instance"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(catalog2["data"]["templates"].as_array().unwrap().len(), 0);
    // Disabled template is not usable as a create source either.
    let gone = rpc_json(
        &app,
        r#"{"procedure":"repo.actions.workflows.create","input":{"owner":"plainu","name":"catrepo","template_id":"instance:sec-scan"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(gone["error"]["code"], "actions.template_not_found");

    // Delete removes it from admin list too.
    let del = rpc_json(
        &app,
        &format!(r#"{{"procedure":"admin.workflowTemplates.delete","input":{{"id":"{id}"}}}}"#),
        &admin_cookie,
    )
    .await;
    assert_eq!(del["ok"], true, "{del}");
    let list2 = rpc_json(
        &app,
        r#"{"procedure":"admin.workflowTemplates.list","input":{}}"#,
        &admin_cookie,
    )
    .await;
    assert!(!list2["data"]["templates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"] == id));
}

/// Anonymous icon route: 200 + headers for vendored icons, 404 otherwise.
#[tokio::test]
async fn template_icons_route() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wt_icons.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let app = test_app(db, repos).await;

    let ok = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/actions/template-icons/rust.svg")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    assert_eq!(ok.headers().get("content-type").unwrap(), "image/svg+xml");
    assert_eq!(
        ok.headers().get("cache-control").unwrap(),
        "public, max-age=86400"
    );
    let body = ok.into_body().collect().await.unwrap().to_bytes();
    assert!(body.starts_with(b"<svg") || body.windows(4).any(|w| w == *b"<svg"));

    // URI-safe but invalid/absent names; `..` segments and spaces are rejected
    // at URI parse level before reaching the handler, and `valid_icon_name`
    // unit tests cover them directly.
    for bad in [
        "nope.svg", "Rust.svg", "a.b.svg", "rust.png", "rust", "x.svg",
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/actions/template-icons/{bad}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{bad}");
        let _ = res.into_body().collect().await;
    }
}
