//! COL-07: GitHub-style auto-merge-when-green on plain PRs.
//!
//! Parity anchors:
//! - `allow_auto_merge` is a repo merge setting; enabling requires Write+.
//! - Arming stores method + enabler on the PR (`auto_merge` embed).
//! - Armed PRs merge automatically the moment the merge gate goes green —
//!   required commit status success, check-run completion, or an approval.
//! - Head moves disarm (`auto_merge_disabled`); retarget/close disarm too.
//! - Queue-required bases reject arming (`pull.merge_queue_required`).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, FileChange, GitBackend};
use tower::ServiceExt;

struct T {
    app: axum::Router,
    db: Database,
    repos_dir: std::path::PathBuf,
    git: CliGitBackend,
    _dir: tempfile::TempDir,
}

async fn test_app() -> T {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let pages = dir.path().join("pages");
    let url = format!("sqlite:{}", dir.path().join("am.db").display());
    let db = support::connect_test_db(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_repos_dir(repos.clone())
    .with_pages_dir(pages);
    let cors = build_cors("development", None).expect("cors");
    T {
        app: router_with_state(state, cors),
        db,
        repos_dir: repos,
        git: CliGitBackend::new(),
        _dir: dir,
    }
}

impl T {
    fn bare(&self, owner: &str, name: &str) -> std::path::PathBuf {
        self.repos_dir.join(owner).join(format!("{name}.git"))
    }
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

fn rpc_req_cookie(body: &str, cookie: &str) -> Request<Body> {
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
        .oneshot(rpc_req_cookie(body, cookie))
        .await
        .unwrap();
    serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn signup_and_login(app: &axum::Router, email: &str, username: &str) -> (String, String) {
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
    let cookie = login
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string();
    let v: serde_json::Value =
        serde_json::from_slice(&login.into_body().collect().await.unwrap().to_bytes()).unwrap();
    (cookie, v["data"]["id"].as_str().unwrap().to_string())
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

/// Signup + verify + repo + `feature` branch carrying a commit `main` lacks.
async fn seed(t: &T, email: &str, username: &str, name: &str) -> (String, String) {
    let (cookie, uid) = signup_and_login(&t.app, email, username).await;
    verify_user(&t.db, &uid).await;
    let create = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
        ),
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let br = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.branchCreate","input":{{"owner":"{username}","name":"{name}","branch":"feature","start":"main"}}}}"#
        ),
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    t.git
        .commit_files(
            &t.bare(username, name),
            "feature",
            "feature",
            "feature change",
            &[FileChange::Upsert {
                path: "feature.txt".into(),
                content: b"feature\n".to_vec(),
            }],
            "T",
            "t@t",
            None,
            "admin",
        )
        .await
        .expect("seed feature commit");
    (cookie, uid)
}

async fn repo_id(t: &T, uid: &str, name: &str) -> String {
    t.db.find_repository_by_owner_name(uid, name)
        .await
        .unwrap()
        .unwrap()
        .id
}

async fn open_pr(t: &T, cookie: &str, username: &str, name: &str) -> serde_json::Value {
    let pr = rpc_json(
        &t.app,
        cookie,
        &format!(
            r#"{{"procedure":"pull.create","input":{{"owner":"{username}","name":"{name}","title":"Change","base_ref":"main","head_ref":"feature"}}}}"#
        ),
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    pr["data"].clone()
}

async fn get_pull(t: &T, cookie: &str, username: &str, name: &str, n: i64) -> serde_json::Value {
    let get = rpc_json(
        &t.app,
        cookie,
        &format!(
            r#"{{"procedure":"pull.get","input":{{"owner":"{username}","name":"{name}","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(get["ok"], true, "{get}");
    get["data"].clone()
}

async fn allow_auto_merge(t: &T, cookie: &str, owner: &str, name: &str) {
    let upd = rpc_json(
        &t.app,
        cookie,
        &format!(
            r#"{{"procedure":"repo.mergeSettings.update","input":{{"owner":"{owner}","name":"{name}","allow_auto_merge":true}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["allow_auto_merge"], true, "{upd}");
}

async fn arm(
    t: &T,
    cookie: &str,
    owner: &str,
    name: &str,
    n: i64,
    method: &str,
) -> serde_json::Value {
    rpc_json(
        &t.app,
        cookie,
        &format!(
            r#"{{"procedure":"pull.auto_merge.enable","input":{{"owner":"{owner}","name":"{name}","number":{n},"method":"{method}"}}}}"#
        ),
    )
    .await
}

async fn protect_status(t: &T, cookie: &str, owner: &str, name: &str, ctx: &str) {
    let rule = rpc_json(
        &t.app,
        cookie,
        &format!(
            r#"{{"procedure":"repo.branchProtection.create","input":{{"owner":"{owner}","name":"{name}","pattern":"main","required_status_contexts":["{ctx}"],"enforce_admins":true}}}}"#
        ),
    )
    .await;
    assert_eq!(rule["ok"], true, "{rule}");
}

/// Enable path validation: repo flag gates arming, disallowed methods and
/// non-open PRs reject, arming exposes the `auto_merge` embed.
#[tokio::test]
async fn auto_merge_enable_requires_flag_method_and_open_state() {
    let t = test_app().await;
    let (cookie, _uid) = seed(&t, "am1@ex.com", "am1", "core").await;
    let pr = open_pr(&t, &cookie, "am1", "core").await;
    let n = pr["number"].as_i64().unwrap();

    // Repo flag off — rejected.
    let off = arm(&t, &cookie, "am1", "core", n, "merge").await;
    assert_eq!(off["ok"], false, "{off}");
    assert_eq!(off["error"]["code"], "pull.auto_merge_disabled", "{off}");

    allow_auto_merge(&t, &cookie, "am1", "core").await;

    // Squash disabled repo-wide — rejected with the merge-method error.
    let upd = rpc_json(
        &t.app,
        &cookie,
        r#"{"procedure":"repo.mergeSettings.update","input":{"owner":"am1","name":"core","allow_squash_merge":false}}"#,
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    let bad_method = arm(&t, &cookie, "am1", "core", n, "squash").await;
    assert_eq!(bad_method["ok"], false, "{bad_method}");
    assert_eq!(
        bad_method["error"]["code"], "pull.merge_disabled",
        "{bad_method}"
    );

    // Required status keeps the gate red so the arm persists — without
    // protection an unprotected PR merges the moment it arms.
    protect_status(&t, &cookie, "am1", "core", "ci").await;

    // Merge method — arms; embed reports method + enabler.
    let en = arm(&t, &cookie, "am1", "core", n, "merge").await;
    assert_eq!(en["ok"], true, "{en}");
    assert_eq!(en["data"]["auto_merge"]["method"], "merge", "{en}");
    assert_eq!(
        en["data"]["auto_merge"]["set_by_username"], "am1",
        "{en}"
    );

    // Disable requires armed state or actor rights — owner can disable.
    let dis = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.auto_merge.disable","input":{{"owner":"am1","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(dis["ok"], true, "{dis}");
    assert!(dis["data"]["auto_merge"].is_null(), "{dis}");

    // Second disable on an unarmed PR — invalid_state.
    let dis2 = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.auto_merge.disable","input":{{"owner":"am1","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(dis2["ok"], false, "{dis2}");
    assert_eq!(dis2["error"]["code"], "pull.invalid_state", "{dis2}");
}

/// Armed PR merges automatically when the required status context greens.
#[tokio::test]
async fn auto_merge_fires_when_required_status_goes_green() {
    let t = test_app().await;
    let (cookie, uid) = seed(&t, "am2@ex.com", "am2", "core").await;
    allow_auto_merge(&t, &cookie, "am2", "core").await;
    protect_status(&t, &cookie, "am2", "core", "ci/test").await;
    let pr = open_pr(&t, &cookie, "am2", "core").await;
    let n = pr["number"].as_i64().unwrap();
    let head = pr["head_sha"].as_str().unwrap().to_string();

    // Armed while red — stays open.
    let en = arm(&t, &cookie, "am2", "core", n, "squash").await;
    assert_eq!(en["ok"], true, "{en}");
    let still = get_pull(&t, &cookie, "am2", "core", n).await;
    assert_eq!(still["state"], "open", "{still}");

    // Required context greens — auto-merge fires with the armed method.
    let st = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.commitStatus.create","input":{{"owner":"am2","name":"core","sha":"{head}","context":"ci/test","state":"success"}}}}"#
        ),
    )
    .await;
    assert_eq!(st["ok"], true, "{st}");

    let merged = get_pull(&t, &cookie, "am2", "core", n).await;
    assert_eq!(merged["state"], "merged", "{merged}");
    assert_eq!(merged["merge_method"], "squash", "{merged}");
    assert_eq!(merged["merged_by"], uid, "{merged}");
}

/// Check-run completion (not just suite completion) satisfies required
/// contexts — a run named after the context fires the merge.
#[tokio::test]
async fn auto_merge_fires_on_check_run_completion() {
    let t = test_app().await;
    let (cookie, _uid) = seed(&t, "am3@ex.com", "am3", "core").await;
    allow_auto_merge(&t, &cookie, "am3", "core").await;
    protect_status(&t, &cookie, "am3", "core", "build").await;
    let pr = open_pr(&t, &cookie, "am3", "core").await;
    let n = pr["number"].as_i64().unwrap();
    let head = pr["head_sha"].as_str().unwrap().to_string();

    let en = arm(&t, &cookie, "am3", "core", n, "merge").await;
    assert_eq!(en["ok"], true, "{en}");

    let run = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"check.runs.create","input":{{"owner":"am3","name":"core","check_name":"build","head_sha":"{head}","status":"completed","conclusion":"success"}}}}"#
        ),
    )
    .await;
    assert_eq!(run["ok"], true, "{run}");

    let merged = get_pull(&t, &cookie, "am3", "core", n).await;
    assert_eq!(merged["state"], "merged", "{merged}");
    assert_eq!(merged["merge_method"], "merge", "{merged}");
}

/// Approval landing greens the gate — the armed PR merges.
#[tokio::test]
async fn auto_merge_fires_on_review_approval() {
    let t = test_app().await;
    let (owner_cookie, _uid) = seed(&t, "am4@ex.com", "am4", "core").await;
    let (rev_cookie, rev_uid) = signup_and_login(&t.app, "am4r@ex.com", "am4r").await;
    verify_user(&t.db, &rev_uid).await;
    let add = rpc_json(
        &t.app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"am4","name":"core","username":"am4r","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "{add}");
    allow_auto_merge(&t, &owner_cookie, "am4", "core").await;
    let rule = rpc_json(
        &t.app,
        &owner_cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"am4","name":"core","pattern":"main","require_reviews":true,"required_approving_review_count":1,"enforce_admins":true}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "{rule}");
    let pr = open_pr(&t, &owner_cookie, "am4", "core").await;
    let n = pr["number"].as_i64().unwrap();

    let en = arm(&t, &owner_cookie, "am4", "core", n, "merge").await;
    assert_eq!(en["ok"], true, "{en}");

    let review = rpc_json(
        &t.app,
        &rev_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"am4","name":"core","number":{n},"state":"approved","body":"lgtm"}}}}"#
        ),
    )
    .await;
    assert_eq!(review["ok"], true, "{review}");

    let merged = get_pull(&t, &owner_cookie, "am4", "core", n).await;
    assert_eq!(merged["state"], "merged", "{merged}");
}

/// GitHub disable-on-push: a head move disarms auto-merge.
#[tokio::test]
async fn auto_merge_disabled_on_head_push() {
    let t = test_app().await;
    let (cookie, uid) = seed(&t, "am5@ex.com", "am5", "core").await;
    allow_auto_merge(&t, &cookie, "am5", "core").await;
    protect_status(&t, &cookie, "am5", "core", "ci").await;
    let pr = open_pr(&t, &cookie, "am5", "core").await;
    let n = pr["number"].as_i64().unwrap();

    let en = arm(&t, &cookie, "am5", "core", n, "merge").await;
    assert_eq!(en["ok"], true, "{en}");

    // Push a new commit onto feature, then run the same synchronize path a
    // real push triggers.
    let new_tip = t
        .git
        .commit_files(
            &t.bare("am5", "core"),
            "feature",
            "feature",
            "more work",
            &[FileChange::Upsert {
                path: "feature.txt".into(),
                content: b"feature v2\n".to_vec(),
            }],
            "T",
            "t@t",
            None,
            "admin",
        )
        .await
        .expect("push");
    let rid = repo_id(&t, &uid, "core").await;
    let row = t
        .db
        .find_pull_by_repo_number(&rid, n)
        .await
        .unwrap()
        .unwrap();
    oxidean_api::pull::synchronize_pull_after_head_move(
        &t.db,
        &t.repos_dir,
        &t.git,
        &row,
        "am5",
        "core",
        "am5",
        "core",
        &new_tip.sha,
        "am5",
        &uid,
        "development",
        false,
    )
    .await
    .expect("synchronize");

    let after = get_pull(&t, &cookie, "am5", "core", n).await;
    assert_eq!(after["state"], "open", "{after}");
    assert!(after["auto_merge"].is_null(), "{after}");

    // Status green on the new head — nothing fires (disarmed).
    let new_sha = new_tip.sha;
    let st = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.commitStatus.create","input":{{"owner":"am5","name":"core","sha":"{new_sha}","context":"ci","state":"success"}}}}"#
        ),
    )
    .await;
    assert_eq!(st["ok"], true, "{st}");
    let still = get_pull(&t, &cookie, "am5", "core", n).await;
    assert_eq!(still["state"], "open", "{still}");
}

/// Queue-required bases reject arming — the merge queue owns those merges.
#[tokio::test]
async fn auto_merge_enable_rejected_on_queue_required_base() {
    let t = test_app().await;
    let (cookie, _uid) = seed(&t, "am6@ex.com", "am6", "core").await;
    allow_auto_merge(&t, &cookie, "am6", "core").await;
    let rule = rpc_json(
        &t.app,
        &cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"am6","name":"core","pattern":"main","required_merge_queue":true}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "{rule}");
    let pr = open_pr(&t, &cookie, "am6", "core").await;
    let n = pr["number"].as_i64().unwrap();

    let en = arm(&t, &cookie, "am6", "core", n, "merge").await;
    assert_eq!(en["ok"], false, "{en}");
    assert_eq!(en["error"]["code"], "pull.merge_queue_required", "{en}");
}

/// Drafts may arm; the merge fires when the PR flips ready (gate clears).
#[tokio::test]
async fn auto_merge_draft_arms_and_fires_on_ready() {
    let t = test_app().await;
    let (cookie, _uid) = seed(&t, "am7@ex.com", "am7", "core").await;
    allow_auto_merge(&t, &cookie, "am7", "core").await;
    protect_status(&t, &cookie, "am7", "core", "ci").await;
    let pr = rpc_json(
        &t.app,
        &cookie,
        r#"{"procedure":"pull.create","input":{"owner":"am7","name":"core","title":"Change","base_ref":"main","head_ref":"feature","draft":true}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();
    let head = pr["data"]["head_sha"].as_str().unwrap().to_string();

    let en = arm(&t, &cookie, "am7", "core", n, "merge").await;
    assert_eq!(en["ok"], true, "{en}");

    // Green status on the draft head — still open (draft blocks).
    let st = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.commitStatus.create","input":{{"owner":"am7","name":"core","sha":"{head}","context":"ci","state":"success"}}}}"#
        ),
    )
    .await;
    assert_eq!(st["ok"], true, "{st}");
    let open = get_pull(&t, &cookie, "am7", "core", n).await;
    assert_eq!(open["state"], "open", "{open}");

    // Flip ready — the fire seam inside pull.update merges it.
    let upd = rpc_json(
        &t.app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.update","input":{{"owner":"am7","name":"core","number":{n},"draft":false}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    let merged = get_pull(&t, &cookie, "am7", "core", n).await;
    assert_eq!(merged["state"], "merged", "{merged}");
}
