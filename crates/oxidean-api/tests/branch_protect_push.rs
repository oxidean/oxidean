//! ORG-06 / D-14 / D-19: Direct push denied on protected branches.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::git::bare_repo_path;
use oxidean_api::protection::{
    check_ref_update, hooks_installed, reconcile_hooks, sweep_protection_hooks, ProtectionIntent,
    ZERO_SHA,
};
use oxidean_api::repo::Capability;
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

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

fn deny_reasons(err: &oxidean_core::AppError) -> Vec<String> {
    err.data.as_ref().unwrap()["reasons"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| r.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Run git in `dir` with a deterministic author identity; assert success.
fn git_env(dir: &std::path::Path, args: &[&str], extra: &[(&str, &str)]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Pusher")
        .env("GIT_AUTHOR_EMAIL", "pusher@ex.com")
        .env("GIT_COMMITTER_NAME", "Pusher")
        .env("GIT_COMMITTER_EMAIL", "pusher@ex.com")
        .env("GIT_TERMINAL_PROMPT", "0")
        .envs(extra.iter().copied())
        .output()
        .expect("git spawn");
    assert!(
        out.status.success(),
        "git {:?} in {} failed: {}",
        args,
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn git_ok(dir: &std::path::Path, args: &[&str]) {
    git_env(dir, args, &[]);
}

fn git_ok_env(dir: &std::path::Path, args: &[&str], extra: &[(&str, &str)]) {
    git_env(dir, args, extra);
}

fn git_out(dir: &std::path::Path, args: &[&str]) -> String {
    git_env(dir, args, &[])
}

/// Write non-Admin cannot push directly when required reviews are enabled (ORG-06, D-14).
#[tokio::test]
async fn branch_protect_push_denies_direct_push_when_reviews_required() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let db_path = dir.path().join("bp_push.db");
    let url = format!("sqlite:{}", db_path.display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (owner_cookie, owner_login) = signup_and_login(&app, "owner@ex.com", "bpown").await;
    verify_user(&db, owner_login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let bare = bare_repo_path(&repos, "bpown", "core").expect("bare path");
    assert!(
        hooks_installed(&bare).await,
        "init_bare must install protection hooks (D-19)"
    );

    let rule = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"bpown","name":"core","pattern":"main","require_reviews":true,"required_approving_review_count":1}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "create rule — {rule}");

    // Simulate Write collaborator push via shared evaluator (same path as hook helper).
    let err = check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/heads/main",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        Capability::Write,
    )
    .await
    .expect_err("Write push must be denied when reviews required");
    assert_eq!(err.code, "repo.branch_protection");
    let reasons = err.data.as_ref().unwrap()["reasons"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        reasons.iter().any(|r| r.as_str() == Some("reviews")),
        "expected reviews reason — {err:?}"
    );

    // Admin bypass when enforce_admins=false (default).
    check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/heads/main",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        Capability::Admin,
    )
    .await
    .expect("Admin may bypass when enforce_admins=false");
}

/// Force-push denied when allow_force_pushes is false (D-15) — greened in 13-05; keep ignored until then if needed.
#[tokio::test]
async fn branch_protect_push_denies_force_push() {
    use oxidean_api::protection::{evaluate_push, union_rules};
    use oxidean_api::repo::Capability;
    use oxidean_db::BranchProtectionRuleRow;

    let rule = BranchProtectionRuleRow {
        id: "1".into(),
        repo_id: "r".into(),
        pattern: "main".into(),
        require_reviews: false,
        required_approving_review_count: 1,
        dismiss_stale_reviews: false,
        require_conversation_resolution: false,
        require_last_push_approval: false,
        required_status_contexts: "[]".into(),
        strict_status_checks: false,
        allow_force_pushes: false,
        allow_deletions: false,
        enforce_admins: false,
        required_linear_history: false,
        lock_branch: false,
        require_signed_commits: false,
        created_at: String::new(),
        updated_at: String::new(),
    };
    let eff = union_rules(&[rule], "main");
    let err = evaluate_push(&eff, ProtectionIntent::ForcePush, Some(Capability::Write))
        .expect_err("force push denied");
    assert_eq!(err.code, "repo.branch_protection");
}

/// lock_branch rejects all pushes for non-bypass actors (D-18).
#[tokio::test]
async fn branch_protect_push_lock_branch() {
    use oxidean_api::protection::{evaluate_push, union_rules};
    use oxidean_api::repo::Capability;
    use oxidean_db::BranchProtectionRuleRow;

    let rule = BranchProtectionRuleRow {
        id: "1".into(),
        repo_id: "r".into(),
        pattern: "main".into(),
        require_reviews: false,
        required_approving_review_count: 1,
        dismiss_stale_reviews: false,
        require_conversation_resolution: false,
        require_last_push_approval: false,
        required_status_contexts: "[]".into(),
        strict_status_checks: false,
        allow_force_pushes: false,
        allow_deletions: false,
        enforce_admins: false,
        required_linear_history: false,
        lock_branch: true,
        require_signed_commits: false,
        created_at: String::new(),
        updated_at: String::new(),
    };
    let eff = union_rules(&[rule], "main");
    let err = evaluate_push(&eff, ProtectionIntent::Push, Some(Capability::Write))
        .expect_err("lock_branch denies push");
    assert_eq!(err.code, "repo.branch_protection");
}

/// Reconcile installs missing hooks (D-19).
#[tokio::test]
async fn branch_protect_push_reconcile_hooks() {
    let dir = tempfile::tempdir().unwrap();
    let bare = dir.path().join("orphan.git");
    std::fs::create_dir_all(&bare).unwrap();
    // Minimal bare layout without hooks.
    std::fs::create_dir_all(bare.join("refs/heads")).unwrap();
    assert!(!hooks_installed(&bare).await);
    reconcile_hooks(&bare).await.expect("reconcile");
    assert!(hooks_installed(&bare).await);
    let _ = ZERO_SHA;
}

/// D-PKG-01: env helper path wins; empty/unset falls back to default_helper_path input.
#[test]
fn branch_protect_default_helper_resolution_prefers_env() {
    use oxidean_api::protection::resolve_protection_helper_with;
    use std::path::PathBuf;

    let preferred = resolve_protection_helper_with(
        Some("/from/env/oxidean-protection-hook".into()),
        Some(PathBuf::from("/from/sibling/oxidean-protection-hook")),
    );
    assert_eq!(
        preferred.as_deref(),
        Some("/from/env/oxidean-protection-hook")
    );

    let fallback = resolve_protection_helper_with(
        None,
        Some(PathBuf::from("/from/sibling/oxidean-protection-hook")),
    );
    assert_eq!(
        fallback.as_deref(),
        Some("/from/sibling/oxidean-protection-hook")
    );
}

/// Invoke installed hooks/update with ref args under a given OXIDEAN_ENV / helper.
async fn run_protection_hook_script(
    bare: &std::path::Path,
    oxidean_env: Option<&str>,
    helper: Option<&std::path::Path>,
) -> std::process::Output {
    let update = bare.join("hooks").join("update");
    let mut cmd = tokio::process::Command::new(&update);
    cmd.args([
        "refs/heads/main",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    ])
    .env_remove("OXIDEAN_PROTECTION_HELPER");
    match oxidean_env {
        Some(v) => {
            cmd.env("OXIDEAN_ENV", v);
        }
        None => {
            cmd.env_remove("OXIDEAN_ENV");
        }
    }
    if let Some(h) = helper {
        cmd.env("OXIDEAN_PROTECTION_HELPER", h);
    }
    cmd.output().await.expect("spawn hooks/update")
}

/// D-PKG-02: missing helper + production|cloud → fail-closed (non-zero).
#[tokio::test]
async fn protection_hook_script_fail_closed_when_helper_missing_in_production() {
    let dir = tempfile::tempdir().unwrap();
    let bare = dir.path().join("prod.git");
    std::fs::create_dir_all(&bare).unwrap();
    oxidean_git::install_protection_hooks(&bare)
        .await
        .expect("install hooks");

    for env_name in ["production", "cloud"] {
        let out = run_protection_hook_script(&bare, Some(env_name), None).await;
        assert!(
            !out.status.success(),
            "OXIDEAN_ENV={env_name} must deny when helper missing — stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("protection helper") || stderr.contains("OXIDEAN"),
            "stderr should explain missing helper — {stderr}"
        );
    }
}

/// D-PKG-02: missing helper + compose|development|dev (or default) → fail-open.
#[tokio::test]
async fn protection_hook_script_fail_open_when_helper_missing_in_dev() {
    let dir = tempfile::tempdir().unwrap();
    let bare = dir.path().join("dev.git");
    std::fs::create_dir_all(&bare).unwrap();
    oxidean_git::install_protection_hooks(&bare)
        .await
        .expect("install hooks");

    for env_name in [Some("compose"), Some("development"), Some("dev"), None] {
        let out = run_protection_hook_script(&bare, env_name, None).await;
        assert!(
            out.status.success(),
            "OXIDEAN_ENV={env_name:?} must fail-open when helper missing — stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// D-PKG-02: executable helper is exec'd with ref args.
#[tokio::test]
async fn protection_hook_script_execs_helper_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let bare = dir.path().join("helper.git");
    std::fs::create_dir_all(&bare).unwrap();
    oxidean_git::install_protection_hooks(&bare)
        .await
        .expect("install hooks");

    let marker = dir.path().join("helper-ran");
    let helper = dir.path().join("fake-helper.sh");
    let script = format!("#!/bin/sh\necho \"$1 $2\" > {}\nexit 0\n", marker.display());
    std::fs::write(&helper, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&helper).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&helper, perms).unwrap();
    }

    let out = run_protection_hook_script(&bare, Some("production"), Some(&helper)).await;
    assert!(
        out.status.success(),
        "helper exec must succeed — stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ran = std::fs::read_to_string(&marker).expect("helper marker");
    assert!(
        ran.contains("update refs/heads/main"),
        "helper must receive update + refname — {ran}"
    );
}

/// Minimal bare layout under `repos_dir/owner/name.git` (HEAD + objects).
async fn make_sweep_bare(repos: &std::path::Path, owner: &str, name: &str) -> std::path::PathBuf {
    let bare = repos.join(owner).join(format!("{name}.git"));
    tokio::fs::create_dir_all(bare.join("objects"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(bare.join("refs/heads"))
        .await
        .unwrap();
    tokio::fs::write(bare.join("HEAD"), b"ref: refs/heads/main\n")
        .await
        .unwrap();
    bare
}

/// D-PKG-04: bare without hooks becomes hooks_installed after sweep.
#[tokio::test]
async fn sweep_protection_hooks_installs_missing_hooks() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let bare = make_sweep_bare(&repos, "alice", "core").await;
    assert!(
        !hooks_installed(&bare).await,
        "fixture must start without hooks"
    );

    let stats = sweep_protection_hooks(&repos).await;
    assert!(
        hooks_installed(&bare).await,
        "sweep must install hooks/update on bare without hooks (D-PKG-04)"
    );
    assert!(
        stats.installed >= 1,
        "stats.installed must count the install — {stats:?}"
    );
    assert_eq!(stats.errors, 0, "clean install must not error — {stats:?}");
}

/// D-PKG-04: outdated hook script is overwritten (install, not reconcile-skip).
#[tokio::test]
async fn sweep_protection_hooks_overwrites_outdated_hook() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let bare = make_sweep_bare(&repos, "bob", "legacy").await;
    let hooks = bare.join("hooks");
    tokio::fs::create_dir_all(&hooks).await.unwrap();
    let update = hooks.join("update");
    // Stale fail-open-only script (pre-D-PKG-02) — must be overwritten.
    tokio::fs::write(&update, b"#!/bin/sh\n# STALE_PRE_DPKG02_HOOK\nexit 0\n")
        .await
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&update).await.unwrap().permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&update, perms).await.unwrap();
    }
    assert!(hooks_installed(&bare).await);

    let stats = sweep_protection_hooks(&repos).await;
    let body = tokio::fs::read_to_string(&update)
        .await
        .expect("hooks/update after sweep");
    assert!(
        !body.contains("STALE_PRE_DPKG02_HOOK"),
        "sweep must overwrite stale script — {body}"
    );
    assert!(
        body.contains("D-PKG-02") || body.contains("protection helper"),
        "overwrite must install known-good packaged script — {body}"
    );
    assert!(
        stats.installed >= 1,
        "overwrite counts as install — {stats:?}"
    );
}

/// D-PKG-04: non-bare entries are skipped without panic.
#[tokio::test]
async fn sweep_protection_hooks_skips_non_bare() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let not_bare = repos.join("carol").join("notes.git");
    tokio::fs::create_dir_all(&not_bare).await.unwrap();
    tokio::fs::write(not_bare.join("README"), b"not a bare repo\n")
        .await
        .unwrap();
    // Loose file under owner (not a repo dir).
    tokio::fs::write(repos.join("carol").join("readme.txt"), b"x")
        .await
        .unwrap();

    let stats = sweep_protection_hooks(&repos).await;
    assert!(
        !hooks_installed(&not_bare).await,
        "non-bare must not get hooks forced"
    );
    assert_eq!(stats.errors, 0, "skip must not count as error — {stats:?}");
    assert!(
        stats.skipped >= 1 || stats.scanned == 0,
        "non-bare should be skipped or not scanned as bare — {stats:?}"
    );
}

/// GIT-22: `require_signed_commits` denies pushes that introduce unsigned
/// commits, allows forge-verified signatures, and leaves non-matching branches
/// alone.
#[tokio::test]
async fn branch_protect_push_requires_signed_commits() {
    let _env_guard = support::lock_admin_env().await;
    let dir = tempfile::tempdir().unwrap();
    // The web-flow keypair lands here: repo.create seeds a forge-signed commit
    // and the verify keyring binds noreply@oxidean.local to that public key.
    let ssh_dir = dir.path().join("ssh");
    std::fs::create_dir_all(&ssh_dir).unwrap();
    // Pre-generate the web-flow keypair up front: repo.create early-returns on
    // an existing key, so concurrent tests sharing this process env never race
    // ssh-keygen.
    let keygen = std::process::Command::new("ssh-keygen")
        .args(["-t", "ed25519", "-N", "", "-q", "-f"])
        .arg(ssh_dir.join("web-flow"))
        .status()
        .expect("ssh-keygen spawn");
    assert!(keygen.success());
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", &ssh_dir);
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("bp_sig.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (owner_cookie, owner_login) = signup_and_login(&app, "sig@ex.com", "sigown").await;
    verify_user(&db, owner_login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","license_id":"MIT"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let rule = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"sigown","name":"core","pattern":"main","require_signed_commits":true}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "create rule — {rule}");

    let bare = bare_repo_path(&repos, "sigown", "core").expect("bare path");
    let bare_s = bare.to_str().unwrap();
    let tip = git_out(&bare, &["rev-parse", "refs/heads/main"]);

    // Unsigned commit staged into the bare object store via `fetch` (lands in
    // FETCH_HEAD — no ref moves, mirroring what a real push introduces).
    let work_u = dir.path().join("work-u");
    let work_u_s = work_u.to_str().unwrap();
    git_ok(dir.path(), &["clone", "-q", bare_s, work_u_s]);
    std::fs::write(work_u.join("UNSIGNED.txt"), b"unsigned\n").unwrap();
    git_ok(&work_u, &["add", "UNSIGNED.txt"]);
    git_ok(
        &work_u,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "unsigned push",
        ],
    );
    let unsigned_sha = git_out(&work_u, &["rev-parse", "HEAD"]);
    git_ok(&bare, &["fetch", "-q", work_u_s, "HEAD"]);

    // Write push introducing the unsigned commit → denied with signed_commits.
    let err = check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/heads/main",
        &tip,
        &unsigned_sha,
        Capability::Write,
    )
    .await
    .expect_err("unsigned commit must be denied on protected main");
    assert_eq!(err.code, "repo.branch_protection");
    assert!(
        deny_reasons(&err).iter().any(|r| r == "signed_commits"),
        "expected signed_commits reason — {err:?}"
    );
    let unsigned = err.data.as_ref().unwrap()["unsigned_commits"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        unsigned
            .iter()
            .any(|s| s.as_str() == Some(&unsigned_sha[..7])),
        "denial must name the unsigned commit — {err:?}"
    );

    // Admin bypass still applies while enforce_admins=false.
    check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/heads/main",
        &tip,
        &unsigned_sha,
        Capability::Admin,
    )
    .await
    .expect("Admin may bypass when enforce_admins=false");

    // Non-matching branch is unaffected by the signed-commits rule.
    check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/heads/feature",
        ZERO_SHA,
        &unsigned_sha,
        Capability::Write,
    )
    .await
    .expect("non-matching branch must allow unsigned push");

    // A commit signed by the instance web-flow key under the forge committer
    // verifies (ssh + noreply policy shortcut) → push allowed.
    let work_s = dir.path().join("work-s");
    let work_s_s = work_s.to_str().unwrap();
    git_ok(dir.path(), &["clone", "-q", bare_s, work_s_s]);
    std::fs::write(work_s.join("SIGNED.txt"), b"signed\n").unwrap();
    git_ok(&work_s, &["add", "SIGNED.txt"]);
    let key = ssh_dir.join("web-flow");
    git_ok_env(
        &work_s,
        &[
            "-c",
            "gpg.format=ssh",
            "-c",
            &format!("user.signingkey={}", key.display()),
            "commit",
            "-q",
            "-S",
            "-m",
            "signed push",
        ],
        &[
            ("GIT_AUTHOR_EMAIL", oxidean_git::FORGE_NOREPLY_EMAIL),
            ("GIT_COMMITTER_EMAIL", oxidean_git::FORGE_NOREPLY_EMAIL),
        ],
    );
    let signed_sha = git_out(&work_s, &["rev-parse", "HEAD"]);
    git_ok(&bare, &["fetch", "-q", work_s_s, "HEAD"]);

    check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/heads/main",
        &tip,
        &signed_sha,
        Capability::Write,
    )
    .await
    .expect("forge-verified signature must satisfy require_signed_commits");

    std::env::remove_var("OXIDEAN_SSH_HOST_KEY_DIR");
}
