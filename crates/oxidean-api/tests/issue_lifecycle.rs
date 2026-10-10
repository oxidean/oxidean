//! ISS-01 create/list/get/edit/close/reopen/history (D-ISS-01..04 / D-ISS-20).
//!
//! Hard-delete covered in `issue_delete.rs` (11-04-T2).

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

async fn create_repo(app: &axum::Router, cookie: &str, name: &str, visibility: &str) {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":""}}}}"#
    );
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(&body, cookie))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

/// Verified Write+ can create an issue and receives per-repo `#1` (ISS-01 / D-ISS-01).
#[tokio::test]
async fn issue_lifecycle_create_allocates_per_repo_number() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("issue_create_n1.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "issowner@ex.com", "issowner").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &cookie, "hello", "public").await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.create","input":{"owner":"issowner","name":"hello","title":"First","body":"body"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "issue.create ok — {v}");
    assert_eq!(v["data"]["number"], 1, "first issue is #1 — {v}");
    assert_eq!(v["data"]["title"], "First");
    assert_eq!(v["data"]["state"], "open");
    assert_eq!(v["data"]["author_username"], "issowner");

    let get = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.get","input":{"owner":"issowner","name":"hello","number":1}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let get_bytes = get.into_body().collect().await.unwrap().to_bytes();
    let get_v: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
    assert_eq!(get_v["ok"], true, "issue.get — {get_v}");
    assert_eq!(get_v["data"]["number"], 1);

    let list = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.list","input":{"owner":"issowner","name":"hello"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let list_bytes = list.into_body().collect().await.unwrap().to_bytes();
    let list_v: serde_json::Value = serde_json::from_slice(&list_bytes).unwrap();
    assert_eq!(list_v["ok"], true, "issue.list — {list_v}");
    assert_eq!(list_v["data"]["total"], 1);
    assert_eq!(list_v["data"]["issues"][0]["number"], 1);
}

/// Second create in same repo gets `#2`; second repo starts at `#1` (D-ISS-01).
#[tokio::test]
async fn issue_lifecycle_second_create_monotonic_number() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("issue_create_n2.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "mono@ex.com", "monoown").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &cookie, "repo-a", "public").await;
    create_repo(&app, &cookie, "repo-b", "public").await;

    let c1 = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.create","input":{"owner":"monoown","name":"repo-a","title":"A1"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let c1b = c1.into_body().collect().await.unwrap().to_bytes();
    let c1v: serde_json::Value = serde_json::from_slice(&c1b).unwrap();
    assert_eq!(c1v["ok"], true, "{c1v}");
    assert_eq!(c1v["data"]["number"], 1);

    let c2 = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.create","input":{"owner":"monoown","name":"repo-a","title":"A2"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let c2b = c2.into_body().collect().await.unwrap().to_bytes();
    let c2v: serde_json::Value = serde_json::from_slice(&c2b).unwrap();
    assert_eq!(c2v["ok"], true, "{c2v}");
    assert_eq!(c2v["data"]["number"], 2, "second in same repo is #2");

    let b1 = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.create","input":{"owner":"monoown","name":"repo-b","title":"B1"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let b1b = b1.into_body().collect().await.unwrap().to_bytes();
    let b1v: serde_json::Value = serde_json::from_slice(&b1b).unwrap();
    assert_eq!(b1v["ok"], true, "{b1v}");
    assert_eq!(b1v["data"]["number"], 1, "second repo starts at #1");
}

/// Author or Write+ may edit title/body after create (ISS-01 / D-ISS-03).
#[tokio::test]
async fn issue_lifecycle_edit_title_body() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("issue_edit.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, login_v) = signup_and_login(&app, "editown@ex.com", "editown").await;
    let owner_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, owner_id).await;
    create_repo(&app, &owner_cookie, "edits", "public").await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.create","input":{"owner":"editown","name":"edits","title":"Original","body":"v1"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    let create_b = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_b).unwrap();
    assert_eq!(create_v["ok"], true, "{create_v}");

    let update = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.update","input":{"owner":"editown","name":"edits","number":1,"title":"Edited","body":"v2"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    let update_b = update.into_body().collect().await.unwrap().to_bytes();
    let update_v: serde_json::Value = serde_json::from_slice(&update_b).unwrap();
    assert_eq!(update_v["ok"], true, "author can update — {update_v}");
    assert_eq!(update_v["data"]["title"], "Edited");
    assert_eq!(update_v["data"]["body"], "v2");

    // Read collaborator cannot update (D-ISS-03 / D-ISS-20).
    let (reader_cookie, reader_v) = signup_and_login(&app, "editread@ex.com", "editread").await;
    let reader_id = reader_v["data"]["id"].as_str().expect("id");
    verify_user(&db, reader_id).await;
    let add = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"editown","name":"edits","username":"editread","permission":"read"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    let add_b = add.into_body().collect().await.unwrap().to_bytes();
    let add_v: serde_json::Value = serde_json::from_slice(&add_b).unwrap();
    assert_eq!(add_v["ok"], true, "add read collab — {add_v}");

    let denied = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.update","input":{"owner":"editown","name":"edits","number":1,"title":"Nope"}}"#,
            &reader_cookie,
        ))
        .await
        .unwrap();
    let denied_b = denied.into_body().collect().await.unwrap().to_bytes();
    let denied_v: serde_json::Value = serde_json::from_slice(&denied_b).unwrap();
    assert_eq!(denied_v["ok"], false, "read cannot update — {denied_v}");
    assert_eq!(
        denied_v["error"]["code"], "repo.not_found",
        "soft deny — {denied_v}"
    );
}

/// Lifecycle is open ↔ closed; reopen allowed (ISS-01 / D-ISS-02).
#[tokio::test]
async fn issue_lifecycle_close_and_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("issue_close.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "closeown@ex.com", "closeown").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &cookie, "cycle", "public").await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.create","input":{"owner":"closeown","name":"cycle","title":"Toggle me"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let create_b = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_b).unwrap();
    assert_eq!(create_v["ok"], true, "{create_v}");
    assert_eq!(create_v["data"]["state"], "open");

    let close = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.close","input":{"owner":"closeown","name":"cycle","number":1}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let close_b = close.into_body().collect().await.unwrap().to_bytes();
    let close_v: serde_json::Value = serde_json::from_slice(&close_b).unwrap();
    assert_eq!(close_v["ok"], true, "close — {close_v}");
    assert_eq!(close_v["data"]["state"], "closed");
    assert!(close_v["data"]["closed_at"].as_str().is_some());

    let reopen = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.reopen","input":{"owner":"closeown","name":"cycle","number":1}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let reopen_b = reopen.into_body().collect().await.unwrap().to_bytes();
    let reopen_v: serde_json::Value = serde_json::from_slice(&reopen_b).unwrap();
    assert_eq!(reopen_v["ok"], true, "reopen — {reopen_v}");
    assert_eq!(reopen_v["data"]["state"], "open");
    assert!(reopen_v["data"]["closed_at"].is_null() || reopen_v["data"].get("closed_at").is_none());
}

/// Full edit history trail for title/body (ISS-01 / D-ISS-04).
#[tokio::test]
async fn issue_history_full_title_body_trail() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("issue_hist.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "histown@ex.com", "histown").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &cookie, "trail", "public").await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.create","input":{"owner":"histown","name":"trail","title":"T0","body":"B0"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let create_b = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_b).unwrap();
    assert_eq!(create_v["ok"], true, "{create_v}");

    for (title, body) in [("T1", "B1"), ("T2", "B2")] {
        let body_json = format!(
            r#"{{"procedure":"issue.update","input":{{"owner":"histown","name":"trail","number":1,"title":"{title}","body":"{body}"}}}}"#
        );
        let upd = app
            .clone()
            .oneshot(rpc_req_with_cookie(&body_json, &cookie))
            .await
            .unwrap();
        let upd_b = upd.into_body().collect().await.unwrap().to_bytes();
        let upd_v: serde_json::Value = serde_json::from_slice(&upd_b).unwrap();
        assert_eq!(upd_v["ok"], true, "update {title} — {upd_v}");
    }

    let hist = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"issue.history","input":{"owner":"histown","name":"trail","number":1}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let hist_b = hist.into_body().collect().await.unwrap().to_bytes();
    let hist_v: serde_json::Value = serde_json::from_slice(&hist_b).unwrap();
    assert_eq!(hist_v["ok"], true, "history — {hist_v}");
    let revs = hist_v["data"]["revisions"]
        .as_array()
        .expect("revisions array");
    assert_eq!(revs.len(), 2, "two prior snapshots — {hist_v}");
    assert_eq!(revs[0]["title"], "T0");
    assert_eq!(revs[0]["body"], "B0");
    assert_eq!(revs[1]["title"], "T1");
    assert_eq!(revs[1]["body"], "B1");
    assert_eq!(revs[0]["editor_username"], "histown");
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

/// Open/Closed/All + author/label/assignee/text filters + offset pagination (D-ISS-16..18).
#[tokio::test]
async fn issue_list_filters_and_offset_pagination() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_list_filters.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "filtown@ex.com", "filtown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &owner_id).await;
    create_repo(&app, &owner_cookie, "filterbox", "public").await;

    let (writer_cookie, writer_v) = signup_and_login(&app, "filtwrite@ex.com", "filtwrite").await;
    let writer_id = writer_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &writer_id).await;
    let add = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"filtown","name":"filterbox","username":"filtwrite","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "add write collab — {add}");

    // #1 owner-authored open with unique title/body for text search.
    let i1 = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.create","input":{"owner":"filtown","name":"filterbox","title":"Alpha uniquephrase","body":"body one"}}"#,
    )
    .await;
    assert_eq!(i1["ok"], true, "{i1}");
    assert_eq!(i1["data"]["number"], 1);

    // #2 writer-authored open — label + assignee targets.
    let i2 = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"issue.create","input":{"owner":"filtown","name":"filterbox","title":"Beta other","body":"body two"}}"#,
    )
    .await;
    assert_eq!(i2["ok"], true, "{i2}");
    assert_eq!(i2["data"]["number"], 2);

    // #3 owner-authored then closed.
    let i3 = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.create","input":{"owner":"filtown","name":"filterbox","title":"Gamma closed","body":"body three"}}"#,
    )
    .await;
    assert_eq!(i3["ok"], true, "{i3}");
    let closed = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.close","input":{"owner":"filtown","name":"filterbox","number":3}}"#,
    )
    .await;
    assert_eq!(closed["ok"], true, "close #3 — {closed}");

    let label = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"label.create","input":{"scope":"repo","owner":"filtown","repo":"filterbox","name":"bug","color":"d73a4a","description":"bugs"}}"#,
    )
    .await;
    assert_eq!(label["ok"], true, "label.create — {label}");
    let label_id = label["data"]["id"].as_str().expect("label id");

    let set_labels = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"issue.labels.set","input":{{"owner":"filtown","name":"filterbox","number":2,"labelIds":["{label_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(set_labels["ok"], true, "labels.set — {set_labels}");

    let set_assignees = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"issue.assignees.set","input":{{"owner":"filtown","name":"filterbox","number":2,"userIds":["{writer_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(set_assignees["ok"], true, "assignees.set — {set_assignees}");

    // Bump #2 updated_at so newest-updated sort is deterministic (D-ISS-18).
    // SQLite second-resolution timestamps need a gap so ORDER BY is stable.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let bump = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"issue.update","input":{"owner":"filtown","name":"filterbox","number":2,"title":"Beta other bumped","body":"body two"}}"#,
    )
    .await;
    assert_eq!(bump["ok"], true, "bump #2 — {bump}");

    // Default list = open only (D-ISS-16).
    let open_default = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.list","input":{"owner":"filtown","name":"filterbox"}}"#,
    )
    .await;
    assert_eq!(open_default["ok"], true, "{open_default}");
    assert_eq!(
        open_default["data"]["total"], 2,
        "default open total — {open_default}"
    );
    let open_nums: Vec<i64> = open_default["data"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["number"].as_i64().unwrap())
        .collect();
    assert!(
        !open_nums.contains(&3),
        "closed #3 excluded from default open"
    );
    assert!(open_nums.contains(&1) && open_nums.contains(&2));

    let closed_only = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.list","input":{"owner":"filtown","name":"filterbox","state":"closed"}}"#,
    )
    .await;
    assert_eq!(closed_only["ok"], true, "{closed_only}");
    assert_eq!(closed_only["data"]["total"], 1);
    assert_eq!(closed_only["data"]["issues"][0]["number"], 3);

    let all = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.list","input":{"owner":"filtown","name":"filterbox","state":"all"}}"#,
    )
    .await;
    assert_eq!(all["ok"], true, "{all}");
    assert_eq!(all["data"]["total"], 3, "all states — {all}");

    // Newest-updated first: assignees.set on #2 bumps it above #1 (D-ISS-18).
    let all_nums: Vec<i64> = all["data"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["number"].as_i64().unwrap())
        .collect();
    assert_eq!(all_nums[0], 2, "newest-updated first — {all_nums:?}");

    let by_author = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.list","input":{"owner":"filtown","name":"filterbox","state":"all","author":"filtwrite"}}"#,
    )
    .await;
    assert_eq!(by_author["ok"], true, "{by_author}");
    assert_eq!(by_author["data"]["total"], 1);
    assert_eq!(by_author["data"]["issues"][0]["number"], 2);
    assert_eq!(
        by_author["data"]["issues"][0]["author_username"],
        "filtwrite"
    );

    let by_label = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"issue.list","input":{{"owner":"filtown","name":"filterbox","state":"all","label":"{label_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(by_label["ok"], true, "{by_label}");
    assert_eq!(by_label["data"]["total"], 1);
    assert_eq!(by_label["data"]["issues"][0]["number"], 2);

    let by_assignee = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.list","input":{"owner":"filtown","name":"filterbox","state":"all","assignee":"filtwrite"}}"#,
    )
    .await;
    assert_eq!(by_assignee["ok"], true, "{by_assignee}");
    assert_eq!(by_assignee["data"]["total"], 1);
    assert_eq!(by_assignee["data"]["issues"][0]["number"], 2);

    let by_q = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.list","input":{"owner":"filtown","name":"filterbox","state":"all","q":"uniquephrase"}}"#,
    )
    .await;
    assert_eq!(by_q["ok"], true, "{by_q}");
    assert_eq!(by_q["data"]["total"], 1);
    assert_eq!(by_q["data"]["issues"][0]["number"], 1);

    // Offset pagination (D-ISS-18).
    let page1 = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.list","input":{"owner":"filtown","name":"filterbox","state":"all","limit":2,"offset":0}}"#,
    )
    .await;
    assert_eq!(page1["ok"], true, "{page1}");
    assert_eq!(page1["data"]["total"], 3);
    assert_eq!(page1["data"]["issues"].as_array().unwrap().len(), 2);

    let page2 = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.list","input":{"owner":"filtown","name":"filterbox","state":"all","limit":2,"offset":2}}"#,
    )
    .await;
    assert_eq!(page2["ok"], true, "{page2}");
    assert_eq!(page2["data"]["total"], 3);
    assert_eq!(page2["data"]["issues"].as_array().unwrap().len(), 1);
    let page1_nums: Vec<i64> = page1["data"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["number"].as_i64().unwrap())
        .collect();
    let page2_num = page2["data"]["issues"][0]["number"].as_i64().unwrap();
    assert!(
        !page1_nums.contains(&page2_num),
        "pages disjoint — page1={page1_nums:?} page2={page2_num}"
    );
}

/// Verified non-collaborator on a public repo participates (create/comment/react/close
/// own) but cannot moderate others' issues (D-ISS-20).
#[tokio::test]
async fn issue_public_participation_read_only_user() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_public_participation.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "pubown@ex.com", "pubown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    verify_user(&db, owner_id).await;
    create_repo(&app, &owner_cookie, "town", "public").await;
    let owner_issue = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.create","input":{"owner":"pubown","name":"town","title":"owner issue"}}"#,
    )
    .await;
    assert_eq!(owner_issue["ok"], true, "{owner_issue}");

    // Stranger: verified, no collaborator row at all.
    let (stranger_cookie, stranger_v) =
        signup_and_login(&app, "pubstranger@ex.com", "pubstranger").await;
    let stranger_id = stranger_v["data"]["id"].as_str().expect("id");
    verify_user(&db, stranger_id).await;

    // Can file an issue.
    let filed = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"issue.create","input":{"owner":"pubown","name":"town","title":"from the community","body":"found a bug"}}"#,
    )
    .await;
    assert_eq!(filed["ok"], true, "stranger files issue — {filed}");
    assert_eq!(filed["data"]["number"], 2);

    // Can comment on someone else's issue.
    let comment = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"issue.comments.create","input":{"owner":"pubown","name":"town","number":1,"body":"same here"}}"#,
    )
    .await;
    assert_eq!(comment["ok"], true, "stranger comments — {comment}");

    // Can react.
    let react = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"issue.reactions.toggle","input":{"owner":"pubown","name":"town","number":1,"target":"issue","content":"+1"}}"#,
    )
    .await;
    assert_eq!(react["ok"], true, "stranger reacts — {react}");

    // Can edit + close own issue.
    let own_edit = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"issue.update","input":{"owner":"pubown","name":"town","number":2,"title":"renamed"}}"#,
    )
    .await;
    assert_eq!(own_edit["ok"], true, "author edits own — {own_edit}");
    let own_close = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"issue.close","input":{"owner":"pubown","name":"town","number":2}}"#,
    )
    .await;
    assert_eq!(own_close["ok"], true, "author closes own — {own_close}");
    let own_reopen = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"issue.reopen","input":{"owner":"pubown","name":"town","number":2}}"#,
    )
    .await;
    assert_eq!(own_reopen["ok"], true, "author reopens own — {own_reopen}");

    // Cannot close someone else's issue.
    let close_denied = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"issue.close","input":{"owner":"pubown","name":"town","number":1}}"#,
    )
    .await;
    assert_eq!(
        close_denied["ok"], false,
        "cannot close others' — {close_denied}"
    );
    assert_eq!(close_denied["error"]["code"], "repo.not_found");

    // Cannot moderate: labels/assignees stay Write+.
    let labels_denied = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"issue.labels.set","input":{"owner":"pubown","name":"town","number":1,"labelIds":[]}}"#,
    )
    .await;
    assert_eq!(
        labels_denied["ok"], false,
        "labels stay Write+ — {labels_denied}"
    );
    let assign_denied = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"issue.assignees.set","input":{"owner":"pubown","name":"town","number":1,"userIds":[]}}"#,
    )
    .await;
    assert_eq!(
        assign_denied["ok"], false,
        "assignees stay Write+ — {assign_denied}"
    );

    // Anonymous cannot participate.
    let anon = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"issue.comments.create","input":{"owner":"pubown","name":"town","number":1,"body":"anon"}}"#,
        ))
        .await
        .unwrap();
    let anon_b = anon.into_body().collect().await.unwrap().to_bytes();
    let anon_v: serde_json::Value = serde_json::from_slice(&anon_b).unwrap();
    assert_eq!(anon_v["ok"], false, "anonymous denied — {anon_v}");
    assert_eq!(anon_v["error"]["code"], "auth.unauthenticated");
}
