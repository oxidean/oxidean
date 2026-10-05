//! API-06: GitHub-compatible subset — `refs/pull/{N}/*` advertisement + fetch
//! (Smart HTTP and SSH), read-only enforcement on pushes, and GitHub-shaped
//! commit status REST endpoints.

mod support;

use std::process::Command as StdCommand;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::ssh::rate_limit::SshAuthLimiter;
use oxidean_api::ssh::{spawn_listener, SshState};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use russh::client;
use russh::keys::ssh_key::{Algorithm, HashAlg, LineEnding};
use russh::keys::{load_secret_key, PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use ssh_key::PublicKey;
use tower::ServiceExt;
use uuid::Uuid;

// --- HTTP/RPC helpers (same shape as git_smart_http.rs / pull_merge.rs) ------

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

fn rest_req(method: &str, uri: &str, body: Option<&str>, cookie: &str) -> Request<Body> {
    let b = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie);
    let b = match body {
        Some(_) => b.header("content-type", "application/json"),
        None => b,
    };
    b.body(Body::from(body.unwrap_or_default().to_owned()))
        .unwrap()
}

fn session_cookie(res: &axum::http::Response<Body>) -> String {
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
    db: &Database,
    email: &str,
    username: &str,
) -> (String, String) {
    let signup = format!(
        r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
    );
    let res = app.clone().oneshot(rpc_req(&signup)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let _ = res.into_body().collect().await;

    let login = format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
    );
    let res = app.clone().oneshot(rpc_req(&login)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = session_cookie(&res);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let user_id = v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    (cookie, user_id)
}

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn encode_b64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let mut n = (chunk[0] as u32) << 16;
        if chunk.len() > 1 {
            n |= (chunk[1] as u32) << 8;
        }
        if chunk.len() > 2 {
            n |= chunk[2] as u32;
        }
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn basic_header(user: &str, password: &str) -> String {
    format!(
        "Basic {}",
        encode_b64(format!("{user}:{password}").as_bytes())
    )
}

/// `GET /{owner}/{repo}.git/info/refs?service=git-upload-pack` → raw pkt bytes.
async fn upload_pack_advertisement(app: &axum::Router, owner: &str, repo: &str) -> Vec<u8> {
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/{owner}/{repo}.git/info/refs?service=git-upload-pack"
        ))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK, "info/refs must succeed");
    res.into_body().collect().await.unwrap().to_bytes().to_vec()
}

/// Seed `owner/repo` (public) with real commits: create via RPC (bare repo,
/// no template stack), then push `main` + `feature` through a local work
/// clone. Avoids the template-seed path, which needs a provisioned web-flow
/// signing key not present in the test env.
async fn seed_repo_with_feature(
    app: &axum::Router,
    db: &Database,
    email: &str,
    user: &str,
    repo: &str,
    work_dir: &std::path::Path,
) -> String {
    let (cookie, _uid) = signup_and_login(app, db, email, user).await;
    let create = rpc_json(
        app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{repo}","visibility":"public","description":""}}}}"#
        ),
    )
    .await;
    assert_eq!(create["ok"], true, "repo.create — {create}");

    let bare = work_dir
        .join("repos")
        .join(user)
        .join(format!("{repo}.git"));
    assert!(bare.is_dir(), "bare repo must exist at {}", bare.display());
    let work = work_dir.join("seed-work");
    std::fs::create_dir_all(&work).unwrap();
    let run = |args: &[&str]| {
        let st = StdCommand::new("git")
            .args(["-C"])
            .arg(&work)
            .args(args)
            .output()
            .expect("git seed");
        assert!(st.status.success(), "git {args:?} failed: {:?}", st.stderr);
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "t@ex.com"]);
    run(&["config", "user.name", "t"]);
    std::fs::write(work.join("README"), "hi\n").unwrap();
    run(&["add", "README"]);
    run(&["-c", "commit.gpgsign=false", "commit", "-m", "init"]);
    run(&["push", &bare.to_string_lossy(), "main"]);
    // Feature branch with an extra commit on top.
    run(&["checkout", "-b", "feature"]);
    std::fs::write(work.join("feat.rs"), "fn f() {}\n").unwrap();
    run(&["add", "feat.rs"]);
    run(&["-c", "commit.gpgsign=false", "commit", "-m", "feat"]);
    run(&["push", &bare.to_string_lossy(), "feature"]);
    cookie
}

/// Create PR `feature` → `main`; returns `(number, head_sha)`.
async fn create_pull(app: &axum::Router, cookie: &str, owner: &str, repo: &str) -> (i64, String) {
    let pr = rpc_json(
        app,
        cookie,
        &format!(
            r#"{{"procedure":"pull.create","input":{{"owner":"{owner}","name":"{repo}","title":"PR","base_ref":"main","head_ref":"feature"}}}}"#
        ),
    )
    .await;
    assert_eq!(pr["ok"], true, "pull.create — {pr}");
    (
        pr["data"]["number"].as_i64().unwrap(),
        pr["data"]["head_sha"]
            .as_str()
            .expect("head_sha")
            .to_string(),
    )
}

// --- tests -------------------------------------------------------------------

/// `refs/pull/{N}/head` is advertised by upload-pack and resolves to the PR
/// head commit; `/merge` is absent until the PR merges (documented delta).
#[tokio::test]
async fn api06_pull_head_ref_advertised_and_fetchable_smart_http() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("api06_head.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let cookie = seed_repo_with_feature(&app, &db, "h@ex.com", "hown", "core", dir.path()).await;
    let (n, head_sha) = create_pull(&app, &cookie, "hown", "core").await;

    let adv = upload_pack_advertisement(&app, "hown", "core").await;
    let adv = String::from_utf8_lossy(&adv);
    assert!(
        adv.contains(&format!("refs/pull/{n}/head")),
        "advertisement must include refs/pull/{n}/head — {adv}"
    );
    assert!(
        !adv.contains(&format!("refs/pull/{n}/merge")),
        "no stored merge commit yet — /merge must be omitted"
    );

    // The synthesized ref resolves to the PR head commit on disk.
    let bare = repos.join("hown").join("core.git");
    let out = StdCommand::new("git")
        .args(["-C"])
        .arg(&bare)
        .args(["rev-parse", &format!("refs/pull/{n}/head")])
        .output()
        .expect("rev-parse");
    assert!(out.status.success(), "rev-parse pull head: {out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), head_sha);

    // And the object is fetchable: upload-pack answers `want <head_sha>` with a pack.
    let want = format!("want {head_sha}\n");
    let mut body = format!("{:04x}{want}", want.len() + 4);
    body.push_str("00000009done\n");
    let req = Request::builder()
        .method("POST")
        .uri("/hown/core.git/git-upload-pack")
        .header("content-type", "application/x-git-upload-pack-request")
        .body(Body::from(body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "upload-pack want must succeed"
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert!(
        bytes.windows(4).any(|w| w == b"PACK"),
        "fetch of refs/pull/{n}/head sha must yield a pack — {} bytes",
        bytes.len()
    );
}

/// `refs/pull/{N}/merge` appears once the PR merges and points at the recorded
/// merge commit.
#[tokio::test]
async fn api06_pull_merge_ref_appears_after_merge() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("api06_merge.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let cookie = seed_repo_with_feature(&app, &db, "mg@ex.com", "mgown", "core", dir.path()).await;
    let (n, _head_sha) = create_pull(&app, &cookie, "mgown", "core").await;

    let adv = upload_pack_advertisement(&app, "mgown", "core").await;
    assert!(
        !String::from_utf8_lossy(&adv).contains(&format!("refs/pull/{n}/merge")),
        "merge ref must be absent before merge"
    );

    let merge = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"mgown","name":"core","number":{n},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(merge["ok"], true, "merge — {merge}");
    let merge_sha = merge["data"]["merge_commit_sha"]
        .as_str()
        .unwrap()
        .to_string();

    let adv = upload_pack_advertisement(&app, "mgown", "core").await;
    let adv = String::from_utf8_lossy(&adv);
    assert!(
        adv.contains(&format!("refs/pull/{n}/merge")),
        "advertisement must include refs/pull/{n}/merge after merge — {adv}"
    );

    let bare = dir.path().join("repos").join("mgown").join("core.git");
    let out = StdCommand::new("git")
        .args(["-C"])
        .arg(&bare)
        .args(["rev-parse", &format!("refs/pull/{n}/merge")])
        .output()
        .expect("rev-parse");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), merge_sha);
}

/// Pushes to `refs/pull/*` over Smart HTTP are denied before receive-pack runs.
#[tokio::test]
async fn api06_push_to_pull_ref_denied_smart_http() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("api06_deny.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let cookie = seed_repo_with_feature(&app, &db, "d@ex.com", "down", "core", dir.path()).await;
    let (n, head_sha) = create_pull(&app, &cookie, "down", "core").await;

    let pat = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pat.createClassic","input":{"name":"cli","scopes":["repo"]}}"#,
    )
    .await;
    assert_eq!(pat["ok"], true, "pat.createClassic — {pat}");
    let token = pat["data"]["token"].as_str().unwrap();

    // receive-pack command pkt: update refs/pull/{n}/head → head_sha.
    let cmd = format!("{} {head_sha} refs/pull/{n}/head\n", "0".repeat(40));
    let mut body = format!("{:04x}{cmd}", cmd.len() + 4);
    body.push_str("0000");

    let req = Request::builder()
        .method("POST")
        .uri("/down/core.git/git-receive-pack")
        .header(header::AUTHORIZATION, basic_header("down", token))
        .header("content-type", "application/x-git-receive-pack-request")
        .body(Body::from(body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "push to refs/pull/* must be denied"
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "git.pull_refs_read_only", "{v}");

    // The synthesized ref must still resolve to the PR head, untouched.
    let bare = repos.join("down").join("core.git");
    let out = StdCommand::new("git")
        .args(["-C"])
        .arg(&bare)
        .args(["rev-parse", &format!("refs/pull/{n}/head")])
        .output()
        .expect("rev-parse");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), head_sha);
}

/// GitHub-shaped commit statuses: POST then GET round trip, plus the combined
/// `/commits/{sha}/status` rollup.
#[tokio::test]
async fn api06_statuses_rest_github_shape_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("api06_status.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, _uid) = signup_and_login(&app, &db, "st@ex.com", "stown").await;
    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"ci","visibility":"public","description":""}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "repo.create — {create}");

    let sha = "0123456789abcdef0123456789abcdef01234567";
    let uri = format!("/api/v1/repos/stown/ci/statuses/{sha}");

    // POST a GitHub-shaped status body.
    let res = app
        .clone()
        .oneshot(rest_req(
            "POST",
            &uri,
            Some(r#"{"state":"success","context":"ci/build","description":"all good","target_url":"https://ci.example/1"}"#),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED, "status create — {res:?}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["state"], "success");
    assert_eq!(v["context"], "ci/build");
    assert_eq!(v["description"], "all good");
    assert_eq!(v["target_url"], "https://ci.example/1");
    assert_eq!(v["sha"], sha);
    assert!(v["created_at"].as_str().is_some(), "created_at — {v}");
    assert_eq!(v["creator"]["login"], "stown", "creator — {v}");

    // GET the GitHub-shaped list.
    let res = app
        .clone()
        .oneshot(rest_req("GET", &uri, None, &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let list: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let arr = list.as_array().expect("statuses must be a bare array");
    assert_eq!(arr.len(), 1, "{list}");
    assert_eq!(arr[0]["context"], "ci/build");
    assert_eq!(arr[0]["state"], "success");
    assert_eq!(arr[0]["target_url"], "https://ci.example/1");
    assert_eq!(arr[0]["creator"]["login"], "stown");

    // Combined status endpoint.
    let res = app
        .clone()
        .oneshot(rest_req(
            "GET",
            &format!("/api/v1/repos/stown/ci/commits/{sha}/status"),
            None,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let combined: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(combined["state"], "success", "{combined}");
    assert_eq!(combined["total_count"], 1);
    assert_eq!(combined["statuses"].as_array().unwrap().len(), 1);

    // A failing context flips the rollup.
    let res = app
        .clone()
        .oneshot(rest_req(
            "POST",
            &uri,
            Some(r#"{"state":"failure","context":"ci/lint"}"#),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let _ = res.into_body().collect().await;

    let res = app
        .clone()
        .oneshot(rest_req(
            "GET",
            &format!("/api/v1/repos/stown/ci/commits/{sha}/status"),
            None,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let combined: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(combined["state"], "failure", "{combined}");
    assert_eq!(combined["total_count"], 2);

    // Invalid state → 400 with the REST error shape.
    let res = app
        .oneshot(rest_req(
            "POST",
            &uri,
            Some(r#"{"state":"green","context":"x"}"#),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// --- SSH (mirrors git_ssh.rs helpers) ----------------------------------------

fn ssh_state(db: Database, repos_dir: std::path::PathBuf) -> SshState {
    SshState {
        db,
        repos_dir,
        auth_limiter: Arc::new(Mutex::new(SshAuthLimiter::new())),
    }
}

struct AcceptingClient;

impl client::Handler for AcceptingClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

fn write_keypair(dir: &std::path::Path) -> (std::path::PathBuf, String, String) {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("gen key");
    let priv_path = dir.join("id_ed25519");
    let pem = key.to_openssh(LineEnding::LF).expect("encode");
    std::fs::write(&priv_path, pem.as_bytes()).expect("write priv");
    let pub_line = key.public_key().to_openssh().expect("pub");
    let fp = PublicKey::from_openssh(&pub_line)
        .expect("parse pub")
        .fingerprint(HashAlg::Sha256)
        .to_string();
    (priv_path, pub_line, fp)
}

async fn connect_git(
    addr: std::net::SocketAddr,
    key: Arc<PrivateKey>,
) -> client::Handle<AcceptingClient> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    });
    let mut session = client::connect(config, addr, AcceptingClient)
        .await
        .expect("connect");
    let ok = session
        .authenticate_publickey("git", PrivateKeyWithHashAlg::new(key, None))
        .await
        .expect("auth");
    assert!(ok.success(), "ssh auth must succeed");
    session
}

fn init_bare_repo(bare: &std::path::Path) -> String {
    std::fs::create_dir_all(bare.parent().unwrap()).expect("parent");
    assert!(StdCommand::new("git")
        .args(["init", "--bare"])
        .arg(bare)
        .status()
        .unwrap()
        .success());
    let work = bare.parent().unwrap().join("seed-work");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "t@ex.com"],
        vec!["config", "user.name", "t"],
    ] {
        assert!(StdCommand::new("git")
            .args(["-C"])
            .arg(&work)
            .args(&args)
            .status()
            .unwrap()
            .success());
    }
    std::fs::write(work.join("README"), "hi\n").unwrap();
    assert!(StdCommand::new("git")
        .args(["-C"])
        .arg(&work)
        .args(["add", "README"])
        .status()
        .unwrap()
        .success());
    assert!(StdCommand::new("git")
        .args(["-C"])
        .arg(&work)
        .args(["-c", "commit.gpgsign=false", "commit", "-m", "init"])
        .status()
        .unwrap()
        .success());
    assert!(StdCommand::new("git")
        .args(["-C"])
        .arg(&work)
        .args(["push"])
        .arg(&*bare.to_string_lossy().into_owned())
        .arg("main")
        .status()
        .unwrap()
        .success());
    let out = StdCommand::new("git")
        .args(["-C"])
        .arg(bare)
        .args(["rev-parse", "refs/heads/main"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

async fn seed_ssh_user_repo(
    db: &Database,
    repos: &std::path::Path,
    tmp: &std::path::Path,
    username: &str,
    repo: &str,
    verified: bool,
) -> (std::path::PathBuf, String, std::path::PathBuf) {
    let bare = repos.join(username).join(format!("{repo}.git"));
    let head_sha = init_bare_repo(&bare);
    let user_id = Uuid::new_v4().to_string();
    db.create_user(
        &user_id,
        &format!("{username}@ex.com"),
        username,
        Some("hash"),
        username,
        "",
        None,
        Role::User,
    )
    .await
    .unwrap();
    if verified {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        db.set_email_verified_at(&user_id, &now).await.unwrap();
    }
    let (_priv_path, pub_line, fp) = write_keypair(tmp);
    db.create_ssh_key(
        &format!("k-{username}"),
        &user_id,
        "laptop",
        &pub_line,
        &fp,
        "ssh-ed25519",
        true,
        true,
    )
    .await
    .unwrap();
    db.insert_repository(
        &format!("r-{repo}-{username}"),
        &user_id,
        "user",
        repo,
        "public",
        repo,
        "main",
    )
    .await
    .unwrap();
    (bare, head_sha, tmp.join("id_ed25519"))
}

/// SSH `git-upload-pack` advertises synthesized pull refs like HTTP does.
#[tokio::test]
async fn api06_ssh_upload_pack_advertises_pull_refs() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("api06_ssh_adv.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    let repos = tmp.path().join("repos");
    std::fs::create_dir_all(&repos).unwrap();

    let (bare, head_sha, priv_path) =
        seed_ssh_user_repo(&db, &repos, tmp.path(), "advown", "demo", false).await;
    // Synthesized pull head ref (what pull::refs::sync_head_ref writes).
    assert!(StdCommand::new("git")
        .args(["-C"])
        .arg(&bare)
        .args(["update-ref", "refs/pull/7/head", &head_sha])
        .status()
        .unwrap()
        .success());

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");
    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let session = connect_git(addr, key).await;
    let mut channel = session.channel_open_session().await.expect("open");
    channel
        .exec(true, "git-upload-pack 'advown/demo.git'")
        .await
        .expect("exec");

    let mut adv = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), channel.wait()).await {
            Ok(Some(russh::ChannelMsg::Data { ref data })) => {
                adv.extend_from_slice(data);
                if String::from_utf8_lossy(&adv).contains("refs/pull/7/head") {
                    break;
                }
            }
            Ok(Some(russh::ChannelMsg::Eof)) | Ok(None) => break,
            _ => {}
        }
    }
    assert!(
        String::from_utf8_lossy(&adv).contains("refs/pull/7/head"),
        "SSH upload-pack must advertise the pull head ref — {} bytes",
        adv.len()
    );
}

/// SSH `git-receive-pack` denies commands targeting `refs/pull/*` — the pack
/// bridge cuts the stream before git applies any update.
#[tokio::test]
async fn api06_ssh_receive_pack_denies_pull_ref_push() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("api06_ssh_deny.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    let repos = tmp.path().join("repos");
    std::fs::create_dir_all(&repos).unwrap();

    let (bare, head_sha, priv_path) =
        seed_ssh_user_repo(&db, &repos, tmp.path(), "denown", "demo", true).await;
    assert!(StdCommand::new("git")
        .args(["-C"])
        .arg(&bare)
        .args(["update-ref", "refs/pull/7/head", &head_sha])
        .status()
        .unwrap()
        .success());

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");
    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let session = connect_git(addr, key).await;
    let mut channel = session.channel_open_session().await.expect("open");
    channel
        .exec(true, "git-receive-pack 'denown/demo.git'")
        .await
        .expect("exec");

    // Read the ref advertisement first, then send the offending command.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut saw_adv = false;
    while tokio::time::Instant::now() < deadline && !saw_adv {
        if let Ok(Some(russh::ChannelMsg::Data { .. })) =
            tokio::time::timeout(Duration::from_millis(500), channel.wait()).await
        {
            saw_adv = true;
        }
    }
    assert!(saw_adv, "receive-pack must advertise refs first");

    let cmd = format!("{} {head_sha} refs/pull/7/head\n", "0".repeat(40));
    let pkt = format!("{:04x}{cmd}0000", cmd.len() + 4);
    channel.data(pkt.as_bytes()).await.expect("send command");

    let mut denied = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), channel.wait()).await {
            Ok(Some(ref msg)) => match msg {
                russh::ChannelMsg::ExtendedData { data, .. } | russh::ChannelMsg::Data { data } => {
                    let s = String::from_utf8_lossy(data);
                    if s.contains("refs/pull") || s.contains("ERROR:") {
                        denied = true;
                        break;
                    }
                }
                russh::ChannelMsg::ExitStatus { exit_status } => {
                    assert_ne!(*exit_status, 0, "receive-pack must not exit 0");
                    denied = true;
                    break;
                }
                russh::ChannelMsg::Eof => break,
                _ => {}
            },
            Ok(None) => break,
            Err(_) => {}
        }
    }
    assert!(denied, "push to refs/pull/* must be denied over SSH");

    // The ref still points at the original head — nothing was applied.
    let out = StdCommand::new("git")
        .args(["-C"])
        .arg(&bare)
        .args(["rev-parse", "refs/pull/7/head"])
        .output()
        .expect("rev-parse");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), head_sha);
}
