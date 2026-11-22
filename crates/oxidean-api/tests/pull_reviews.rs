//! PR-04 reviews: approve / changes / comment / dismiss + author ACL.

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
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
        )))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;
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

#[tokio::test]
async fn pull_create_requests_matching_codeowners_from_base_revision() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_codeowners.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "co-owner@ex.com", "coowner").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    let (reviewer_cookie, reviewer_v) =
        signup_and_login(&app, "co-reviewer@ex.com", "coreviewer").await;
    verify_user(&db, reviewer_v["data"]["id"].as_str().unwrap()).await;
    let (other_reviewer_cookie, other_reviewer_v) =
        signup_and_login(&app, "co-reviewer2@ex.com", "otherreviewer").await;
    verify_user(&db, other_reviewer_v["data"]["id"].as_str().unwrap()).await;
    let (_, manual_reviewer_v) =
        signup_and_login(&app, "manual-reviewer@ex.com", "manualreviewer").await;
    verify_user(&db, manual_reviewer_v["data"]["id"].as_str().unwrap()).await;

    let create_repo = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create_repo["ok"], true, "{create_repo}");
    let add_collaborator = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"coowner","name":"core","username":"coreviewer","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add_collaborator["ok"], true, "{add_collaborator}");
    for username in ["otherreviewer", "manualreviewer"] {
        let add = rpc_json(
            &app,
            &owner_cookie,
            &format!(
                r#"{{"procedure":"repo.collaborators.add","input":{{"owner":"coowner","name":"core","username":"{username}","permission":"write"}}}}"#
            ),
        )
        .await;
        assert_eq!(add["ok"], true, "{add}");
    }
    let protection = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"coowner","name":"core","pattern":"main","require_code_owner_reviews":true,"dismiss_stale_reviews":false}}"#,
    )
    .await;
    assert_eq!(protection["ok"], true, "{protection}");
    assert_eq!(protection["data"]["require_code_owner_reviews"], true);

    let codeowners = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.file.create","input":{"owner":"coowner","name":"core","path":".github/CODEOWNERS","content":"docs/owned.rs @coowner @coreviewer\ndocs/other.rs @otherreviewer\n","message":"Add CODEOWNERS"}}"#,
    )
    .await;
    assert_eq!(codeowners["ok"], true, "{codeowners}");
    let branch = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"coowner","name":"core","branch":"feature","start":"main"}}"#,
    )
    .await;
    assert_eq!(branch["ok"], true, "{branch}");
    let head_codeowners = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.file.update","input":{"owner":"coowner","name":"core","branch":"feature","path":".github/CODEOWNERS","content":"docs/owned.rs @otherreviewer\n","message":"Change head CODEOWNERS"}}"#,
    )
    .await;
    assert_eq!(head_codeowners["ok"], true, "{head_codeowners}");
    let changed_file = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.file.create","input":{"owner":"coowner","name":"core","branch":"feature","path":"docs/owned.rs","content":"owned\n","message":"Add owned file"}}"#,
    )
    .await;
    assert_eq!(changed_file["ok"], true, "{changed_file}");

    let pull = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"pull.create","input":{"owner":"coowner","name":"core","title":"Owned change","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(pull["ok"], true, "{pull}");
    let number = pull["data"]["number"].as_i64().unwrap();
    let initial_requests = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.reviewRequests.list","input":{{"owner":"coowner","name":"core","number":{number}}}}}"#
        ),
    )
    .await;
    assert_eq!(initial_requests["ok"], true, "{initial_requests}");
    let initial_usernames = initial_requests["data"]["usernames"].as_array().unwrap();
    assert!(initial_usernames
        .iter()
        .any(|username| username.as_str() == Some("coreviewer")));
    assert!(initial_usernames
        .iter()
        .all(|username| username.as_str() != Some("otherreviewer")));
    let manual_request = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.reviewRequests.add","input":{{"owner":"coowner","name":"core","number":{number},"username":"manualreviewer"}}}}"#
        ),
    )
    .await;
    assert_eq!(manual_request["ok"], true, "{manual_request}");
    let stale_approval = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"coowner","name":"core","number":{number},"state":"approved"}}}}"#
        ),
    )
    .await;
    assert_eq!(stale_approval["ok"], true, "{stale_approval}");
    let changed_file = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.file.create","input":{"owner":"coowner","name":"core","branch":"feature","path":"docs/other.rs","content":"other\n","message":"Add second owned file"}}"#,
    )
    .await;
    assert_eq!(changed_file["ok"], true, "{changed_file}");
    let (notifications_before_repeat, _) = db
        .list_notifications(
            other_reviewer_v["data"]["id"].as_str().unwrap(),
            false,
            0,
            100,
        )
        .await
        .unwrap();
    let notification_count_before_repeat = notifications_before_repeat
        .iter()
        .filter(|notification| notification.reason == "pr_review_requested")
        .count();
    let repeated_change = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.file.update","input":{"owner":"coowner","name":"core","branch":"feature","path":"docs/other.rs","content":"other v2\n","message":"Update second owned file"}}"#,
    )
    .await;
    assert_eq!(repeated_change["ok"], true, "{repeated_change}");
    let (notifications_after_repeat, _) = db
        .list_notifications(
            other_reviewer_v["data"]["id"].as_str().unwrap(),
            false,
            0,
            100,
        )
        .await
        .unwrap();
    let notification_count_after_repeat = notifications_after_repeat
        .iter()
        .filter(|notification| notification.reason == "pr_review_requested")
        .count();
    assert_eq!(
        notification_count_after_repeat, notification_count_before_repeat,
        "recalculating unchanged ownership must not send another review-request notification; before={:?}, after={:?}",
        notifications_before_repeat.iter().map(|n| &n.reason).collect::<Vec<_>>(),
        notifications_after_repeat.iter().map(|n| &n.reason).collect::<Vec<_>>()
    );
    let requests = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.reviewRequests.list","input":{{"owner":"coowner","name":"core","number":{number}}}}}"#
        ),
    )
    .await;
    assert_eq!(requests["ok"], true, "{requests}");
    assert!(
        requests["data"]["usernames"]
            .as_array()
            .unwrap()
            .iter()
            .any(|username| username.as_str() == Some("coreviewer")),
        "matching CODEOWNERS owner must be requested — {requests}"
    );
    let usernames = requests["data"]["usernames"].as_array().unwrap();
    for expected in ["coreviewer", "otherreviewer", "manualreviewer"] {
        assert!(
            usernames
                .iter()
                .any(|username| username.as_str() == Some(expected)),
            "review request {expected} must be present after changed-file refresh — {requests}"
        );
    }
    assert!(
        usernames
            .iter()
            .all(|username| username.as_str() != Some("coowner")),
        "the pull request author cannot review their own change — {requests}"
    );

    let blocked = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"coowner","name":"core","number":{number},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(
        blocked["ok"], false,
        "code-owner approval must block merge — {blocked}"
    );
    assert_eq!(blocked["error"]["code"], "pull.merge_blocked");
    assert!(
        blocked["error"]["data"]["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason.as_str() == Some("codeowners")),
        "stable CODEOWNERS blocker — {blocked}"
    );

    let still_blocked = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"coowner","name":"core","number":{number},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(
        still_blocked["ok"], false,
        "each matched path needs an owner approval — {still_blocked}"
    );
    assert!(
        still_blocked["error"]["data"]["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason.as_str() == Some("codeowners")),
        "the second path remains blocked — {still_blocked}"
    );
    let second_approval = rpc_json(
        &app,
        &other_reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"coowner","name":"core","number":{number},"state":"approved"}}}}"#
        ),
    )
    .await;
    assert_eq!(second_approval["ok"], true, "{second_approval}");
    let stale_owner_still_blocked = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"coowner","name":"core","number":{number},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(
        stale_owner_still_blocked["ok"], false,
        "a stale owner approval cannot satisfy CODEOWNERS even when stale reviews are otherwise allowed — {stale_owner_still_blocked}"
    );
    assert!(stale_owner_still_blocked["error"]["data"]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason.as_str() == Some("codeowners")));
    let stale_detail = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.get","input":{{"owner":"coowner","name":"core","number":{number}}}}}"#
        ),
    )
    .await;
    assert_eq!(stale_detail["ok"], true, "{stale_detail}");
    assert!(stale_detail["data"]["codeowners_status"]["paths"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|path| path["path"] == "docs/owned.rs")
        .all(|path| path["approved"] == false));
    let current_head_approval = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"coowner","name":"core","number":{number},"state":"approved"}}}}"#
        ),
    )
    .await;
    assert_eq!(current_head_approval["ok"], true, "{current_head_approval}");
    let merged = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"coowner","name":"core","number":{number},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(
        merged["ok"], true,
        "current owner approval must satisfy CODEOWNERS — {merged}"
    );

    let detail = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.get","input":{{"owner":"coowner","name":"core","number":{number}}}}}"#
        ),
    )
    .await;
    assert_eq!(detail["ok"], true, "{detail}");
    assert_eq!(detail["data"]["codeowners_status"]["file_present"], true);
    let paths = detail["data"]["codeowners_status"]["paths"]
        .as_array()
        .unwrap();
    assert_eq!(paths.len(), 3);
    assert!(paths.iter().any(|path| {
        path["path"] == "docs/owned.rs"
            && path["owners"] == serde_json::json!(["@coowner", "@coreviewer"])
            && path["approved"] == true
    }));
    assert!(paths.iter().any(|path| {
        path["path"] == "docs/other.rs"
            && path["owners"] == serde_json::json!(["@otherreviewer"])
            && path["approved"] == true
    }));
    assert!(paths.iter().any(|path| {
        path["path"] == ".github/CODEOWNERS"
            && path["owners"] == serde_json::json!([])
            && path["approved"] == true
    }));
}

#[tokio::test]
async fn pull_create_requests_team_codeowners_from_current_org_grant() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("pull_team_codeowners.db").display()
    );
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "team-owner@ex.com", "teamowner").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    let (reviewer_cookie, reviewer_v) =
        signup_and_login(&app, "team-reviewer@ex.com", "teamreviewer").await;
    verify_user(&db, reviewer_v["data"]["id"].as_str().unwrap()).await;
    let (_, second_reviewer_v) =
        signup_and_login(&app, "team-reviewer2@ex.com", "teamreviewer2").await;
    verify_user(&db, second_reviewer_v["data"]["id"].as_str().unwrap()).await;

    let org = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"org.create","input":{"slug":"teamorg"}}"#,
    )
    .await;
    assert_eq!(org["ok"], true, "{org}");
    let member = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"org.members.add","input":{"slug":"teamorg","username":"teamreviewer","role":"member"}}"#,
    )
    .await;
    assert_eq!(member["ok"], true, "{member}");
    let second_member = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"org.members.add","input":{"slug":"teamorg","username":"teamreviewer2","role":"member"}}"#,
    )
    .await;
    assert_eq!(second_member["ok"], true, "{second_member}");
    let team = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"org.teams.create","input":{"slug":"teamorg","name":"Reviewers","team_slug":"reviewers"}}"#,
    )
    .await;
    assert_eq!(team["ok"], true, "{team}");
    let team_member = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"teamorg","team_slug":"reviewers","username":"teamreviewer"}}"#,
    )
    .await;
    assert_eq!(team_member["ok"], true, "{team_member}");
    let second_team_member = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"teamorg","team_slug":"reviewers","username":"teamreviewer2"}}"#,
    )
    .await;
    assert_eq!(second_team_member["ok"], true, "{second_team_member}");
    let team_id = team["data"]["id"].as_str().unwrap();
    let limited_members = db.list_team_members_limited(team_id, 1).await.unwrap();
    assert_eq!(limited_members.len(), 1);
    let repo = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"private","owner":"teamorg"}}"#,
    )
    .await;
    assert_eq!(repo["ok"], true, "{repo}");
    let grant = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"org.teams.repos.add","input":{"slug":"teamorg","team_slug":"reviewers","repo":"core","permission":"write"}}"#,
    )
    .await;
    assert_eq!(grant["ok"], true, "{grant}");
    let protection = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"teamorg","name":"core","pattern":"main","require_code_owner_reviews":true}}"#,
    )
    .await;
    assert_eq!(protection["ok"], true, "{protection}");
    let codeowners = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.file.create","input":{"owner":"teamorg","name":"core","path":".github/CODEOWNERS","content":"docs/** @teamorg/reviewers\n","message":"Add team CODEOWNERS"}}"#,
    )
    .await;
    assert_eq!(codeowners["ok"], true, "{codeowners}");
    let branch = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"teamorg","name":"core","branch":"feature","start":"main"}}"#,
    )
    .await;
    assert_eq!(branch["ok"], true, "{branch}");
    let changed_file = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.file.create","input":{"owner":"teamorg","name":"core","branch":"feature","path":"docs/owned.rs","content":"owned\n","message":"Add owned file"}}"#,
    )
    .await;
    assert_eq!(changed_file["ok"], true, "{changed_file}");
    let pull = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"pull.create","input":{"owner":"teamorg","name":"core","title":"Team-owned change","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(pull["ok"], true, "{pull}");
    let number = pull["data"]["number"].as_i64().unwrap();
    let requests = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.reviewRequests.list","input":{{"owner":"teamorg","name":"core","number":{number}}}}}"#
        ),
    )
    .await;
    assert_eq!(requests["ok"], true, "{requests}");
    let requested_usernames = requests["data"]["usernames"].as_array().unwrap();
    for username in ["teamreviewer", "teamreviewer2"] {
        assert!(requested_usernames
            .iter()
            .any(|requested| requested.as_str() == Some(username)));
    }
    let detail = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.get","input":{{"owner":"teamorg","name":"core","number":{number}}}}}"#
        ),
    )
    .await;
    assert_eq!(detail["ok"], true, "{detail}");
    assert_eq!(
        detail["data"]["codeowners_status"]["paths"][0]["owners"],
        serde_json::json!(["@teamorg/reviewers"])
    );
    let approval = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"teamorg","name":"core","number":{number},"state":"approved"}}}}"#
        ),
    )
    .await;
    assert_eq!(approval["ok"], true, "{approval}");
    let merged = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"teamorg","name":"core","number":{number},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(
        merged["ok"], true,
        "team member with current write access approves — {merged}"
    );
}

#[tokio::test]
async fn pull_reviews_submit_dismiss_author_cannot_approve() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_reviews.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "o@ex.com", "rown").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    let (rev_cookie, rev_v) = signup_and_login(&app, "r@ex.com", "reviewer").await;
    verify_user(&db, rev_v["data"]["id"].as_str().unwrap()).await;

    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    // Collaborator write for reviewer
    let collab = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"rown","name":"core","username":"reviewer","permission":"write"}}"#,
    )
    .await;
    assert_eq!(collab["ok"], true, "{collab}");

    let br = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"rown","name":"core","branch":"feature","start":"main"}}"#,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");

    let pr = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"pull.create","input":{"owner":"rown","name":"core","title":"PR","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();

    let author_approve = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"rown","name":"core","number":{n},"state":"approved"}}}}"#
        ),
    )
    .await;
    assert_eq!(author_approve["ok"], false, "{author_approve}");
    assert_eq!(
        author_approve["error"]["code"],
        "pull.author_cannot_approve"
    );

    let author_comment = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"rown","name":"core","number":{n},"state":"commented","body":"self note"}}}}"#
        ),
    )
    .await;
    assert_eq!(author_comment["ok"], true, "{author_comment}");

    let approve = rpc_json(
        &app,
        &rev_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"rown","name":"core","number":{n},"state":"approved","body":"lgtm"}}}}"#
        ),
    )
    .await;
    assert_eq!(approve["ok"], true, "{approve}");
    assert_eq!(approve["data"]["state"], "approved");
    let rid = approve["data"]["id"].as_str().unwrap();

    let changes = rpc_json(
        &app,
        &rev_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"rown","name":"core","number":{n},"state":"changes_requested","body":"nit"}}}}"#
        ),
    )
    .await;
    assert_eq!(changes["ok"], true, "{changes}");
    assert_eq!(changes["data"]["state"], "changes_requested");

    let list = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.list","input":{{"owner":"rown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(list["ok"], true, "{list}");
    let reviews = list["data"]["reviews"].as_array().unwrap();
    assert!(reviews.len() >= 3);

    let dismiss = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.dismiss","input":{{"owner":"rown","name":"core","number":{n},"review_id":"{rid}","reason":"stale"}}}}"#
        ),
    )
    .await;
    assert_eq!(dismiss["ok"], true, "{dismiss}");
    assert_eq!(dismiss["data"]["state"], "dismissed");

    let req_add = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.reviewRequests.add","input":{{"owner":"rown","name":"core","number":{n},"username":"reviewer"}}}}"#
        ),
    )
    .await;
    assert_eq!(req_add["ok"], true, "{req_add}");
    assert!(req_add["data"]["usernames"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u.as_str() == Some("reviewer")));
}

async fn rest(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    body: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie);
    if body.is_some() {
        b = b.header("content-type", "application/json");
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::from(body.unwrap_or("").to_owned())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

/// GitHub Pulls REST surface: requested_reviewers list/add/remove and
/// review dismissals (body `message` maps to the RPC `reason`).
#[tokio::test]
async fn pull_reviews_rest_github_surface() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("pull_reviews_rest.db").display()
    );
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "o@ex.com", "prown").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    let (rev_cookie, rev_v) = signup_and_login(&app, "r@ex.com", "previewer").await;
    verify_user(&db, rev_v["data"]["id"].as_str().unwrap()).await;

    rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"prown","name":"core","username":"previewer","permission":"write"}}"#,
    )
    .await;
    rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"prown","name":"core","branch":"feature","start":"main"}}"#,
    )
    .await;
    let pr = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"pull.create","input":{"owner":"prown","name":"core","title":"PR","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    let n = pr["data"]["number"].as_i64().unwrap();
    let base = format!("/api/v1/repos/prown/core/pulls/{n}");

    // empty reviewer set
    let (s, v) = rest(
        &app,
        "GET",
        &format!("{base}/requested_reviewers"),
        &owner_cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["users"].as_array().unwrap().len(), 0);
    assert_eq!(v["teams"].as_array().unwrap().len(), 0);

    // request one → 201 + the pull
    let (s, v) = rest(
        &app,
        "POST",
        &format!("{base}/requested_reviewers"),
        &owner_cookie,
        Some(r#"{"reviewers":["previewer"]}"#),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["number"], n);

    let (s, v) = rest(
        &app,
        "GET",
        &format!("{base}/requested_reviewers"),
        &owner_cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["users"][0]["login"], "previewer");

    // reviewer approves → dismiss via REST (`message` → `reason`)
    let approve = rpc_json(
        &app,
        &rev_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"prown","name":"core","number":{n},"state":"approved","body":"lgtm"}}}}"#
        ),
    )
    .await;
    assert_eq!(approve["ok"], true, "{approve}");
    let rid = approve["data"]["id"].as_str().unwrap();

    let (s, v) = rest(
        &app,
        "POST",
        &format!("{base}/reviews/{rid}/dismissals"),
        &owner_cookie,
        Some(r#"{"message":"stale approval"}"#),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["state"], "dismissed");
    assert_eq!(v["dismiss_reason"], "stale approval");

    // remove the requested reviewer → 200 + pull
    let (s, v) = rest(
        &app,
        "DELETE",
        &format!("{base}/requested_reviewers"),
        &owner_cookie,
        Some(r#"{"reviewers":["previewer"]}"#),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (s, v) = rest(
        &app,
        "GET",
        &format!("{base}/requested_reviewers"),
        &owner_cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["users"].as_array().unwrap().len(), 0);

    // bad body → 422
    let (s, _) = rest(
        &app,
        "POST",
        &format!("{base}/requested_reviewers"),
        &owner_cookie,
        Some(r#"{"reviewers":"previewer"}"#),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}
