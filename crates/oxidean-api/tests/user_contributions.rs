//! COL-12 — `user.getContributions`: anonymous viewers see public-repo
//! contributions only; the profile owner sees private-repo rows and
//! `includes_private`. Unknown/banned users → `user.not_found`.

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

async fn signup_and_login(app: &axum::Router, email: &str, username: &str) -> (String, String) {
    let signup_body = format!(
        r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password": "password1"}}}}"#
    );
    let signup = app.clone().oneshot(rpc_req(&signup_body)).await.unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;

    let login_body = format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password": "password1","remember_me":false}}}}"#
    );
    let login = app.clone().oneshot(rpc_req(&login_body)).await.unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (cookie, v["data"]["id"].as_str().unwrap().to_string())
}

async fn rpc_json(app: &axum::Router, cookie: Option<&str>, body: &str) -> serde_json::Value {
    let res = match cookie {
        Some(c) => app
            .clone()
            .oneshot(rpc_req_with_cookie(body, c))
            .await
            .unwrap(),
        None => app.clone().oneshot(rpc_req(body)).await.unwrap(),
    };
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

async fn create_repo(app: &axum::Router, cookie: &str, name: &str, visibility: &str) -> String {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
    );
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(&body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "repo.create — {v}");
    v["data"]["id"].as_str().unwrap().to_string()
}

/// Seed one of each contribution kind in `repo_id` for `user_id`, all dated
/// "now" (today's bucket). Commits come from `authored_commits` (attributed
/// through the user's verified primary email), mirroring the push path.
async fn seed_contributions(db: &Database, repo_id: &str, user_id: &str, email: &str) {
    let push_id = uuid::Uuid::new_v4().to_string();
    db.insert_repo_activity(
        &push_id,
        repo_id,
        user_id,
        "push",
        Some("refs/heads/main"),
        Some("a".repeat(40).as_str()),
        Some("b".repeat(40).as_str()),
        Some(3),
        Some("three commits"),
        None,
    )
    .await
    .expect("insert activity");

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let commits: Vec<oxidean_db::ObservedCommit> = (0..3)
        .map(|i| oxidean_db::ObservedCommit {
            sha: format!("{:040x}", i + 1),
            author_email: email.to_string(),
            authored_at: now.clone(),
        })
        .collect();
    let n = db
        .record_authored_commits(repo_id, &commits)
        .await
        .expect("record authored");
    assert_eq!(n, 3, "authored commits should land");

    let issue_id = uuid::Uuid::new_v4().to_string();
    db.insert_issue(&issue_id, repo_id, user_id, "issue title", "body")
        .await
        .expect("insert issue");

    let pull_num = db
        .allocate_next_issue_number(repo_id)
        .await
        .expect("alloc number");
    let pull_id = uuid::Uuid::new_v4().to_string();
    db.insert_pull(
        &pull_id,
        repo_id,
        pull_num,
        "pull title",
        "body",
        user_id,
        "main",
        "a".repeat(40).as_str(),
        repo_id,
        "feature",
        "b".repeat(40).as_str(),
        false,
    )
    .await
    .expect("insert pull");

    let review_id = uuid::Uuid::new_v4().to_string();
    db.insert_pull_review(&review_id, &pull_id, user_id, "approved", "", None)
        .await
        .expect("insert review");
}

fn contribution_call(username: &str) -> String {
    format!(r#"{{"procedure":"user.getContributions","input":{{"username":"{username}"}}}}"#)
}

/// The first contributions read spawns lazy default-branch backfills that
/// attribute `repo.create`'s scaffold commit (+1 authored commit per repo).
/// Poll until the walk lands so assertions are deterministic.
async fn wait_commits(
    app: &axum::Router,
    cookie: Option<&str>,
    username: &str,
    want: i64,
) -> serde_json::Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let last = rpc_json(app, cookie, &contribution_call(username)).await;
        if last["data"]["by_type"]["commits"].as_i64().unwrap_or(0) >= want {
            return last;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "authored-commit backfill did not land: {last}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn contributions_anonymous_sees_public_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("contrib_pub.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "cg@ex.com", "cgu").await;
    verify_user(&db, &user_id).await;
    let pub_repo = create_repo(&app, &cookie, "pubrepo", "public").await;
    let priv_repo = create_repo(&app, &cookie, "privrepo", "private").await;
    seed_contributions(&db, &pub_repo, &user_id, "cg@ex.com").await;
    seed_contributions(&db, &priv_repo, &user_id, "cg@ex.com").await;

    // 3 seeded commits + the scaffold commit the backfill attributes.
    let v = wait_commits(&app, None, "cgu", 4).await;
    assert_eq!(v["ok"], true, "{v}");
    let data = &v["data"];
    assert_eq!(data["includes_private"], false);
    // public repo only: 4 commits + 1 issue + 1 pull + 1 review + 1 repo created
    assert_eq!(data["by_type"]["commits"], 4, "{data}");
    assert_eq!(data["by_type"]["issues"], 1, "{data}");
    assert_eq!(data["by_type"]["pull_requests"], 1, "{data}");
    assert_eq!(data["by_type"]["reviews"], 1, "{data}");
    assert_eq!(data["total"], 8, "{data}");
    assert_eq!(data["days"].as_array().unwrap().len(), 365, "{data}");
    assert!(!data["months"].as_array().unwrap().is_empty());
    assert!(!data["years"].as_array().unwrap().is_empty());
    // no private-repo slugs anywhere
    let s = serde_json::to_string(data).unwrap();
    assert!(!s.contains("privrepo"), "{s}");
}

#[tokio::test]
async fn contributions_owner_includes_private() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("contrib_priv.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "self@ex.com", "selfu").await;
    verify_user(&db, &user_id).await;
    let pub_repo = create_repo(&app, &cookie, "pubrepo", "public").await;
    let priv_repo = create_repo(&app, &cookie, "privrepo", "private").await;
    seed_contributions(&db, &pub_repo, &user_id, "self@ex.com").await;
    seed_contributions(&db, &priv_repo, &user_id, "self@ex.com").await;

    let v = wait_commits(&app, Some(&cookie), "selfu", 8).await;
    assert_eq!(v["ok"], true, "{v}");
    let data = &v["data"];
    assert_eq!(data["includes_private"], true, "{data}");
    // both repos: 8 commits (3 seeded + 1 scaffold each) + 2 issues +
    // 2 pulls + 2 reviews + 2 repo creations
    assert_eq!(data["by_type"]["commits"], 8, "{data}");
    assert_eq!(data["by_type"]["issues"], 2, "{data}");
    assert_eq!(data["by_type"]["pull_requests"], 2, "{data}");
    assert_eq!(data["by_type"]["reviews"], 2, "{data}");
    assert_eq!(data["total"], 16, "{data}");

    // monthly section shape: commits + itemized kinds grouped under this month
    let month = &data["months"][0];
    let kinds: Vec<&str> = month["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["kind"].as_str().unwrap())
        .collect();
    for k in [
        "commits",
        "pull_requests",
        "issues",
        "reviews",
        "repositories",
    ] {
        assert!(kinds.contains(&k), "missing section {k} in {month}");
    }
    let pr_section = month["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "pull_requests")
        .unwrap();
    assert_eq!(pr_section["repo_count"], 2);
    assert_eq!(pr_section["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn contributions_unknown_and_banned_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("contrib_nf.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let v = rpc_json(&app, None, &contribution_call("ghost")).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "user.not_found", "{v}");

    let (_cookie, user_id) = signup_and_login(&app, "ban@ex.com", "banu").await;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_user_banned_at(&user_id, &now).await.expect("ban");
    let v = rpc_json(&app, None, &contribution_call("banu")).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "user.not_found", "{v}");
}

#[tokio::test]
async fn contributions_bad_range_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("contrib_range.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (_cookie, _uid) = signup_and_login(&app, "r@ex.com", "rangeu").await;
    let body = r#"{"procedure":"user.getContributions","input":{"username":"rangeu","from":"2026-06-01","to":"2026-01-01"}}"#;
    let v = rpc_json(&app, None, body).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "rpc.bad_input", "{v}");
}

#[tokio::test]
async fn contributions_owner_opted_out_sees_public_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("contrib_optout.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "opt@ex.com", "optu").await;
    verify_user(&db, &user_id).await;
    let pub_repo = create_repo(&app, &cookie, "pubrepo", "public").await;
    let priv_repo = create_repo(&app, &cookie, "privrepo", "private").await;
    seed_contributions(&db, &pub_repo, &user_id, "opt@ex.com").await;
    seed_contributions(&db, &priv_repo, &user_id, "opt@ex.com").await;

    // Opt out through the profile RPC — same path the settings toggle uses.
    let body = r#"{"procedure":"user.update_profile","input":{"display_name":"Opt","username":"optu","bio":"","show_private_contributions":false}}"#;
    let v = rpc_json(&app, Some(&cookie), body).await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["show_private_contributions"], false, "{v}");

    // Backfill lands the scaffold commit (async) — public repo only here.
    let v = wait_commits(&app, Some(&cookie), "optu", 4).await;
    assert_eq!(v["ok"], true, "{v}");
    let data = &v["data"];
    // Opted-out owner sees the same public-only view as everyone else.
    assert_eq!(data["includes_private"], false, "{data}");
    assert_eq!(data["by_type"]["commits"], 4, "{data}");
    assert_eq!(data["total"], 8, "{data}");
    let s = serde_json::to_string(data).unwrap();
    assert!(!s.contains("privrepo"), "{s}");

    // Re-enabling restores the private-inclusive self view.
    let body = r#"{"procedure":"user.update_profile","input":{"display_name":"Opt","username":"optu","bio":"","show_private_contributions":true}}"#;
    rpc_json(&app, Some(&cookie), body).await;
    let v = wait_commits(&app, Some(&cookie), "optu", 8).await;
    let data = &v["data"];
    assert_eq!(data["includes_private"], true, "{data}");
    assert_eq!(data["total"], 16, "{data}");
}

#[tokio::test]
async fn contributions_streaks_and_commit_dedup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("contrib_streak.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "sk@ex.com", "sku").await;
    verify_user(&db, &user_id).await;
    let repo = create_repo(&app, &cookie, "streakrepo", "public").await;

    // Three consecutive days ending today, a gap, then a two-day run —
    // current 3, longest 3.
    let day = |offset: i64| {
        (chrono::Utc::now() - chrono::Duration::days(offset))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    };
    let commits: Vec<oxidean_db::ObservedCommit> = [0i64, 1, 2, 30, 31]
        .iter()
        .map(|off| oxidean_db::ObservedCommit {
            sha: format!("{:040x}", off + 0x100),
            author_email: "sk@ex.com".to_string(),
            authored_at: day(*off),
        })
        .collect();
    db.record_authored_commits(&repo, &commits)
        .await
        .expect("record authored");

    // Re-recording the same shas must not double count (repo, sha) dedup.
    let again = db
        .record_authored_commits(&repo, &commits)
        .await
        .expect("record again");
    assert_eq!(again, 0, "re-pushed commits dedupe");

    let v = wait_commits(&app, Some(&cookie), "sku", 6).await;
    assert_eq!(v["ok"], true, "{v}");
    let data = &v["data"];
    // 5 seeded commits + the scaffold commit the backfill attributes.
    assert_eq!(data["by_type"]["commits"], 6, "{data}");
    assert_eq!(data["current_streak"], 3, "{data}");
    assert_eq!(data["longest_streak"], 3, "{data}");
}

#[tokio::test]
async fn contributions_rollup_rebuild_covers_preexisting_rows() {
    // Rows written before rollups existed get picked up by the lazy
    // `user:<id>` rebuild on first read — insert straight into the source
    // tables, bypassing the facade bumps.
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("contrib_rebuild.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "rb@ex.com", "rbu").await;
    verify_user(&db, &user_id).await;
    let repo = create_repo(&app, &cookie, "rbrepo", "public").await;

    let issue_id = uuid::Uuid::new_v4().to_string();
    db.insert_issue(&issue_id, &repo, &user_id, "old issue", "body")
        .await
        .expect("insert issue");

    // Simulate a pre-rollup state by wiping the day rows the facade bump
    // just wrote, then let the read-path rebuild restore them.
    db.clear_user_contribution_days(&user_id)
        .await
        .expect("clear rollups");

    let v = rpc_json(&app, Some(&cookie), &contribution_call("rbu")).await;
    assert_eq!(v["ok"], true, "{v}");
    let data = &v["data"];
    assert_eq!(data["by_type"]["issues"], 1, "{data}");
    assert!(
        data["total"].as_i64().unwrap() >= 2,
        "issue + repo creation — {data}"
    );
}
