//! GIT-26 repo insights — contributors, weekly commit activity, fork network.
//! Git-backed sections run against real commits pushed into the bare repo;
//! fork network is DB-backed and visibility-aware.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::git::bare_repo_path;
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
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":"src","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
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

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn rpc_json_anon(app: &axum::Router, body: &str) -> serde_json::Value {
    let res = app.clone().oneshot(rpc_req(body)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Clone the bare repo, commit `count` files as `(name, email)`, push HEAD back.
fn push_commits(bare: &std::path::Path, name: &str, email: &str, count: usize, tag: &str) {
    let wt = tempfile::tempdir().unwrap();
    let bare_s = bare.to_str().unwrap();
    let wt_s = wt.path().to_str().unwrap();
    let status = std::process::Command::new("git")
        .args(["clone", bare_s, wt_s])
        .status()
        .unwrap();
    assert!(status.success(), "clone");
    for args in [
        vec!["-C", wt_s, "config", "user.email", email],
        vec!["-C", wt_s, "config", "user.name", name],
    ] {
        let status = std::process::Command::new("git")
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }
    for i in 0..count {
        let dest = wt.path().join(format!("{tag}-{i}.txt"));
        std::fs::write(&dest, format!("{tag} {i}")).unwrap();
        for args in [
            vec!["-C", wt_s, "add", "-A"],
            vec!["-C", wt_s, "commit", "-m", &format!("{tag} commit {i}")],
        ] {
            let status = std::process::Command::new("git")
                .args(&args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        }
    }
    let status = std::process::Command::new("git")
        .args(["-C", wt_s, "push", "origin", "HEAD"])
        .status()
        .unwrap();
    assert!(status.success(), "push");
}

/// Sunday 00:00 UTC weeks are day-3-aligned epoch seconds (1970-01-04 = day 3).
const SUNDAY_EPOCH_MOD: i64 = 3 * 86_400;

#[tokio::test]
async fn repo_insights_contributors_counts_and_account_link() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("insights_contrib.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "c1@ex.com", "c1owner").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "insightful", "public").await;

    // Two commits authored by the account's verified email → username resolve.
    let bare = bare_repo_path(&repos, "c1owner", "insightful").expect("bare");
    push_commits(&bare, "C1 Owner", "c1@ex.com", 2, "a");
    // One commit from a non-account author.
    push_commits(&bare, "External Dev", "ext@other.example", 1, "b");

    let res = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.insights.contributors","input":{"owner":"c1owner","name":"insightful","limit":50}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    let contributors = res["data"]["contributors"].as_array().unwrap();
    // The seeded initial commit is authored by the creator's email, so the
    // distinct-author set is {c1 (seed + 2 pushes), external}.
    assert_eq!(contributors.len(), 2, "contributors: {contributors:?}");
    // Sorted by commit_count desc — c1 outranks the singleton external.
    assert_eq!(contributors[0]["commit_count"].as_i64().unwrap(), 3);
    let c1 = contributors
        .iter()
        .find(|c| c["email"].as_str() == Some("c1@ex.com"))
        .expect("c1 row");
    assert_eq!(c1["commit_count"].as_i64().unwrap(), 3);
    assert_eq!(
        c1["username"].as_str(),
        Some("c1owner"),
        "email→account link"
    );
    assert!(c1["last_commit_sha"].as_str().unwrap().len() >= 7);
    assert!(c1["last_commit_unix"].as_i64().unwrap() > 0);
    assert!(c1["first_commit_unix"].as_i64().unwrap() > 0);
    let ext = contributors
        .iter()
        .find(|c| c["email"].as_str() == Some("ext@other.example"))
        .expect("ext row");
    assert_eq!(ext["commit_count"].as_i64().unwrap(), 1);
    assert!(ext["username"].is_null(), "unknown email must not link");
    assert_eq!(res["data"]["scanned_commits"].as_u64().unwrap(), 4);
    assert!(!res["data"]["truncated"].as_bool().unwrap());

    // limit=1 clips the list but the scan metadata stays honest.
    let capped = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.insights.contributors","input":{"owner":"c1owner","name":"insightful","limit":1}}"#,
    )
    .await;
    assert_eq!(capped["ok"], true, "{capped}");
    assert_eq!(capped["data"]["contributors"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn repo_insights_commit_activity_week_shape() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("insights_act.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "act@ex.com", "actowner").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "pulse", "public").await;

    let bare = bare_repo_path(&repos, "actowner", "pulse").expect("bare");
    push_commits(&bare, "Act Owner", "act@ex.com", 3, "pulse");

    let res = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.insights.commitActivity","input":{"owner":"actowner","name":"pulse","weeks":4}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    let weeks = res["data"]["weeks"].as_array().unwrap();
    assert_eq!(weeks.len(), 4, "requested window size");
    let mut sum = 0i64;
    for w in weeks {
        let wk = w["week"].as_i64().unwrap();
        assert_eq!(wk % (7 * 86_400), SUNDAY_EPOCH_MOD, "Sunday-anchored: {wk}");
        let days = w["days"].as_array().unwrap();
        assert_eq!(days.len(), 7, "seven day buckets");
        let day_sum: i64 = days.iter().map(|d| d.as_i64().unwrap()).sum();
        assert_eq!(day_sum, w["total"].as_i64().unwrap(), "days sum to total");
        sum += w["total"].as_i64().unwrap();
    }
    // Weeks are contiguous and oldest-first.
    for pair in weeks.windows(2) {
        assert_eq!(
            pair[1]["week"].as_i64().unwrap() - pair[0]["week"].as_i64().unwrap(),
            7 * 86_400
        );
    }
    assert_eq!(sum, res["data"]["total"].as_i64().unwrap());
    // Seed commit + 3 pushed commits all landed this week.
    assert!(res["data"]["total"].as_i64().unwrap() >= 4);
    assert!(
        weeks[3]["total"].as_i64().unwrap() >= 4,
        "current week holds commits"
    );
    assert!(!res["data"]["truncated"].as_bool().unwrap());
    assert!(res["data"]["scanned_commits"].as_u64().unwrap() >= 4);
}

#[tokio::test]
async fn repo_insights_private_repo_denied_anonymous_and_stranger() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("insights_priv.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, v) = signup_and_login(&app, "priv@ex.com", "privowner").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "hidden", "private").await;

    let (stranger_c, stranger_v) = signup_and_login(&app, "priv-s@ex.com", "privstranger").await;
    verify_user(&db, stranger_v["data"]["id"].as_str().unwrap()).await;

    for proc in [
        "repo.insights.contributors",
        "repo.insights.commitActivity",
        "repo.insights.forkNetwork",
    ] {
        let body =
            format!(r#"{{"procedure":"{proc}","input":{{"owner":"privowner","name":"hidden"}}}}"#);
        let anon = rpc_json_anon(&app, &body).await;
        assert_eq!(anon["ok"], false, "{proc} anonymous: {anon}");
        assert_eq!(
            anon["error"]["code"].as_str(),
            Some("repo.not_found"),
            "{proc} anonymous error: {anon}"
        );
        let stranger = rpc_json(&app, &stranger_c, &body).await;
        assert_eq!(stranger["ok"], false, "{proc} stranger: {stranger}");
        assert_eq!(
            stranger["error"]["code"].as_str(),
            Some("repo.not_found"),
            "{proc} stranger error: {stranger}"
        );
        // Owner keeps read access to their own private repo.
        let owner = rpc_json(&app, &cookie, &body).await;
        assert_eq!(owner["ok"], true, "{proc} owner: {owner}");
    }
}

/// Empty (unborn) repo — all three scans return empty results, not errors.
#[tokio::test]
async fn repo_insights_empty_repo_ok() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("insights_empty.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, v) = signup_and_login(&app, "em@ex.com", "emowner").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    // No seed files: the default branch stays unborn.
    let body = r#"{"procedure":"repo.create","input":{"name":"unborn","visibility":"public","description":"","stack_id":"","license_id":"","gitignore_id":""}}"#;
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, &cookie))
        .await
        .unwrap();
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(created["ok"], true, "empty repo.create: {created}");

    let contrib = rpc_json_anon(
        &app,
        r#"{"procedure":"repo.insights.contributors","input":{"owner":"emowner","name":"unborn"}}"#,
    )
    .await;
    assert_eq!(contrib["ok"], true, "{contrib}");
    assert_eq!(contrib["data"]["contributors"].as_array().unwrap().len(), 0);
    assert_eq!(contrib["data"]["scanned_commits"].as_u64().unwrap(), 0);
    assert!(!contrib["data"]["truncated"].as_bool().unwrap());

    let act = rpc_json_anon(
        &app,
        r#"{"procedure":"repo.insights.commitActivity","input":{"owner":"emowner","name":"unborn","weeks":4}}"#,
    )
    .await;
    assert_eq!(act["ok"], true, "{act}");
    assert_eq!(act["data"]["total"].as_i64().unwrap(), 0);
    assert_eq!(act["data"]["scanned_commits"].as_u64().unwrap(), 0);
    assert!(act["data"]["weeks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|w| w["total"].as_i64().unwrap() == 0));

    // Fork network still lists the repo itself (root + current).
    let net = rpc_json_anon(
        &app,
        r#"{"procedure":"repo.insights.forkNetwork","input":{"owner":"emowner","name":"unborn"}}"#,
    )
    .await;
    assert_eq!(net["ok"], true, "{net}");
    let nodes = net["data"]["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1, "{net}");
    assert_eq!(nodes[0]["owner"].as_str(), Some("emowner"));
    assert_eq!(nodes[0]["is_root"].as_bool(), Some(true));
    assert_eq!(nodes[0]["is_current"].as_bool(), Some(true));
    assert!(nodes[0]["parent_owner"].is_null());
    assert_eq!(net["data"]["total"].as_i64().unwrap(), 1);
    assert!(!net["data"]["truncated"].as_bool().unwrap());
}

#[tokio::test]
async fn repo_insights_fork_network_members_and_visibility() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("insights_net.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (src_c, src_v) = signup_and_login(&app, "net-src@ex.com", "netsrc").await;
    verify_user(&db, src_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &src_c, "upstream", "public").await;

    let (fork_c, fork_v) = signup_and_login(&app, "net-f@ex.com", "netfork").await;
    verify_user(&db, fork_v["data"]["id"].as_str().unwrap()).await;
    let forked = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.fork","input":{"owner":"netsrc","name":"upstream"}}"#,
    )
    .await;
    assert_eq!(forked["ok"], true, "{forked}");

    // Private fork by a third user must stay invisible to outsiders.
    let (priv_c, priv_v) = signup_and_login(&app, "net-p@ex.com", "netpriv").await;
    verify_user(&db, priv_v["data"]["id"].as_str().unwrap()).await;
    let priv_fork = rpc_json(
        &app,
        &priv_c,
        r#"{"procedure":"repo.fork","input":{"owner":"netsrc","name":"upstream"}}"#,
    )
    .await;
    assert_eq!(priv_fork["ok"], true, "{priv_fork}");
    let priv_name = priv_fork["data"]["name"].as_str().unwrap().to_string();
    let vis = rpc_json(
        &app,
        &priv_c,
        &format!(
            r#"{{"procedure":"repo.updateVisibility","input":{{"owner":"netpriv","name":"{priv_name}","visibility":"private"}}}}"#
        ),
    )
    .await;
    assert_eq!(vis["ok"], true, "{vis}");

    // Anonymous view of the public root: public members only.
    let anon = rpc_json_anon(
        &app,
        r#"{"procedure":"repo.insights.forkNetwork","input":{"owner":"netsrc","name":"upstream"}}"#,
    )
    .await;
    assert_eq!(anon["ok"], true, "{anon}");
    let nodes = anon["data"]["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2, "private fork hidden: {nodes:?}");
    assert_eq!(anon["data"]["total"].as_i64().unwrap(), 2);
    assert!(!anon["data"]["truncated"].as_bool().unwrap());

    // Root is ordered first (network-root before forks) for one-pass tree builds.
    let root = nodes
        .iter()
        .find(|n| n["is_root"].as_bool() == Some(true))
        .expect("network root node");
    assert_eq!(root["owner"].as_str(), Some("netsrc"));
    assert_eq!(root["name"].as_str(), Some("upstream"));
    assert_eq!(root["is_current"].as_bool(), Some(true));
    assert!(root["parent_owner"].is_null());
    assert_eq!(
        nodes[0]["is_root"].as_bool(),
        Some(true),
        "root must be first in fork-network list: {nodes:?}"
    );

    let fork_node = nodes
        .iter()
        .find(|n| n["owner"].as_str() == Some("netfork"))
        .expect("public fork node");
    assert_eq!(fork_node["is_root"].as_bool(), Some(false));
    assert_eq!(fork_node["is_current"].as_bool(), Some(false));
    assert_eq!(fork_node["parent_owner"].as_str(), Some("netsrc"));
    assert_eq!(fork_node["parent_name"].as_str(), Some("upstream"));

    // The private fork querying itself: node present (is_current), root listed.
    let own = rpc_json(
        &app,
        &priv_c,
        &format!(
            r#"{{"procedure":"repo.insights.forkNetwork","input":{{"owner":"netpriv","name":"{priv_name}"}}}}"#
        ),
    )
    .await;
    assert_eq!(own["ok"], true, "{own}");
    let own_nodes = own["data"]["nodes"].as_array().unwrap();
    let slugs: Vec<String> = own_nodes
        .iter()
        .map(|n| {
            format!(
                "{}/{}",
                n["owner"].as_str().unwrap(),
                n["name"].as_str().unwrap()
            )
        })
        .collect();
    assert!(
        slugs.contains(&format!("netpriv/{priv_name}")),
        "queried private repo included: {slugs:?}"
    );
    assert!(
        slugs.contains(&"netsrc/upstream".to_string()),
        "root listed"
    );
    let me = own_nodes
        .iter()
        .find(|n| n["is_current"].as_bool() == Some(true))
        .expect("is_current flag");
    assert_eq!(me["owner"].as_str(), Some("netpriv"));
    assert_eq!(me["parent_owner"].as_str(), Some("netsrc"));

    // An unrelated private repo is never listed for outsiders.
    let other = rpc_json_anon(
        &app,
        r#"{"procedure":"repo.insights.forkNetwork","input":{"owner":"netpriv","name":"upstream"}}"#,
    )
    .await;
    assert_eq!(other["ok"], false, "private repo read denied: {other}");
}
