//! GIT-23: per-repo deploy keys — admin RPC CRUD, fingerprint rules, and
//! Git-over-SSH transport authorization (read-only vs read/write scope).

mod support;

use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
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
use std::sync::Mutex;
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

// Real OpenSSH lines (same fixtures as ssh_key_rpc.rs) — add is validated.
const ED25519_A: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJqxgqAG6vw46mOJ8QZKNpHEoPuP5sW2YoBlT/24OycR laptop@oxidean";
const FP_A: &str = "SHA256:QfwEFKimvVWrMaGsq4bBXsvtQgICjbmQryZxKXx/WyI";
const ED25519_B: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkMMC69ejkQKaKcprTo6FkLAcsqsUEGD5dbJ7ma5tyi ci@oxidean";

// ---------- RPC harness (mirrors branch_protection_rpc.rs) ----------

async fn test_app(db: Database, repos_dir: PathBuf) -> axum::Router {
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

async fn make_user_and_repo(
    db: &Database,
    email: &str,
    username: &str,
    repo_id: &str,
    repo_name: &str,
    visibility: &str,
) -> String {
    let user_id = Uuid::new_v4().to_string();
    db.create_user(
        &user_id,
        email,
        username,
        Some("hash"),
        "U",
        "",
        None,
        Role::User,
    )
    .await
    .expect("create user");
    db.insert_repository(repo_id, &user_id, "user", repo_name, visibility, "", "main")
        .await
        .expect("insert repo");
    user_id
}

// ---------- SSH harness (mirrors git_ssh.rs) ----------

fn ssh_state(db: Database, repos_dir: PathBuf) -> SshState {
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

async fn connect_auth(
    addr: std::net::SocketAddr,
    user: &str,
    key: Arc<PrivateKey>,
) -> Result<client::Handle<AcceptingClient>, String> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    });
    let mut session = client::connect(config, addr, AcceptingClient)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let ok = session
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(key, None))
        .await
        .map_err(|e| format!("auth: {e}"))?;
    if !ok.success() {
        return Err("auth rejected".into());
    }
    Ok(session)
}

fn write_keypair(dir: &std::path::Path, stem: &str) -> (PathBuf, String, String) {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("gen key");
    let priv_path = dir.join(stem);
    let pem = key.to_openssh(LineEnding::LF).expect("encode");
    std::fs::write(&priv_path, pem.as_bytes()).expect("write priv");
    let pub_line = key.public_key().to_openssh().expect("pub");
    let fp = PublicKey::from_openssh(&pub_line)
        .expect("parse pub")
        .fingerprint(HashAlg::Sha256)
        .to_string();
    (priv_path, pub_line, fp)
}

fn init_bare_repo(bare: &std::path::Path) {
    std::fs::create_dir_all(bare.parent().unwrap()).expect("parent");
    let st = StdCommand::new("git")
        .args(["init", "--bare"])
        .arg(bare)
        .status()
        .expect("git init");
    assert!(st.success());
    let work = bare.parent().unwrap().join(format!(
        "seed-work-{}",
        bare.file_stem().unwrap().to_string_lossy()
    ));
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
    for args in [
        vec!["add", "README"],
        vec!["-c", "commit.gpgsign=false", "commit", "-m", "init"],
    ] {
        assert!(StdCommand::new("git")
            .args(["-C"])
            .arg(&work)
            .args(&args)
            .status()
            .unwrap()
            .success());
    }
    assert!(StdCommand::new("git")
        .args(["-C"])
        .arg(&work)
        .args(["remote", "add", "origin"])
        .arg(bare)
        .status()
        .unwrap()
        .success());
    assert!(StdCommand::new("git")
        .args(["-C"])
        .arg(&work)
        .args(["push", "-u", "origin", "main"])
        .status()
        .unwrap()
        .success());
}

/// Drain the channel until a git stderr "ERROR:" / deny text appears or EOF.
async fn saw_deny(channel: &mut russh::Channel<client::Msg>) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(400), channel.wait()).await {
            Ok(Some(russh::ChannelMsg::ExtendedData { ref data, .. }))
            | Ok(Some(russh::ChannelMsg::Data { ref data })) => {
                let s = String::from_utf8_lossy(data);
                if s.contains("Permission denied") || s.contains("ERROR:") {
                    return true;
                }
            }
            Ok(None) | Ok(Some(russh::ChannelMsg::Eof)) => break,
            _ => {}
        }
    }
    false
}

/// Drain the channel until pack data or a clean exit appears.
async fn saw_pack_data(channel: &mut russh::Channel<client::Msg>) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(400), channel.wait()).await {
            Ok(Some(russh::ChannelMsg::Data { ref data })) if !data.is_empty() => return true,
            Ok(Some(russh::ChannelMsg::ExitStatus { exit_status: 0 })) => return true,
            Ok(None) | Ok(Some(russh::ChannelMsg::Eof)) => break,
            _ => {}
        }
    }
    false
}

// ---------- RPC tests ----------

/// Admin can create/list/delete a deploy key (GIT-23).
#[tokio::test]
async fn deploy_key_rpc_admin_crud() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("dk_crud.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login) = signup_and_login(&app, "adm@ex.com", "dkadmin").await;
    let uid = login["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &uid).await;
    db.insert_repository("r-dk", &uid, "user", "core", "public", "", "main")
        .await
        .unwrap();

    let pk = serde_json::to_string(ED25519_A).unwrap();
    let created = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.deployKey.create","input":{{"owner":"dkadmin","name":"core","title":"CI","public_key":{pk},"can_write":true}}}}"#
        ),
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(created["data"]["fingerprint"], FP_A);
    assert_eq!(created["data"]["key_type"], "ssh-ed25519");
    assert_eq!(created["data"]["can_write"], true);
    assert_eq!(created["data"]["created_by"], uid);
    assert!(created["data"].get("token").is_none());
    assert!(created["data"].get("secret").is_none());
    let key_id = created["data"]["id"].as_str().unwrap().to_string();

    let listed = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.deployKey.list","input":{"owner":"dkadmin","name":"core"}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    let keys = listed["data"]["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["title"], "CI");

    let deleted = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.deployKey.delete","input":{{"owner":"dkadmin","name":"core","id":"{key_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(deleted["ok"], true, "{deleted}");

    let listed = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.deployKey.list","input":{"owner":"dkadmin","name":"core"}}"#,
    )
    .await;
    assert_eq!(listed["data"]["keys"].as_array().unwrap().len(), 0);

    let again = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.deployKey.delete","input":{{"owner":"dkadmin","name":"core","id":"{key_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(again["ok"], false, "{again}");
    assert_eq!(again["error"]["code"], "deployKey.not_found");
}

/// Non-admin (and anonymous) callers get the soft `repo.not_found` (D-ORG-04).
#[tokio::test]
async fn deploy_key_rpc_non_admin_denied() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("dk_acl.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let owner_id = Uuid::new_v4().to_string();
    db.create_user(
        &owner_id,
        "own@ex.com",
        "dkowner",
        Some("hash"),
        "O",
        "",
        None,
        Role::User,
    )
    .await
    .unwrap();
    db.insert_repository("r-priv", &owner_id, "user", "core", "public", "", "main")
        .await
        .unwrap();

    let (cookie, login) = signup_and_login(&app, "oth@ex.com", "dkother").await;
    verify_user(&db, login["data"]["id"].as_str().unwrap()).await;

    let pk = serde_json::to_string(ED25519_A).unwrap();
    for proc_name in ["create", "list", "delete"] {
        let input = if proc_name == "delete" {
            format!(r#"{{"owner":"dkowner","name":"core","id":"x"}}"#)
        } else if proc_name == "create" {
            format!(
                r#"{{"owner":"dkowner","name":"core","title":"CI","public_key":{pk},"can_write":false}}"#
            )
        } else {
            r#"{"owner":"dkowner","name":"core"}"#.to_string()
        };
        let v = rpc_json(
            &app,
            &cookie,
            &format!(r#"{{"procedure":"repo.deployKey.{proc_name}","input":{input}}}"#),
        )
        .await;
        assert_eq!(v["ok"], false, "{proc_name} — {v}");
        assert_eq!(v["error"]["code"], "repo.not_found", "{proc_name} — {v}");
    }
}

/// Fingerprint rules: same key twice on one repo is rejected; the same key on
/// a *different* repo is allowed; a key already registered as an account key is
/// rejected (deploy keys are a distinct credential class).
#[tokio::test]
async fn deploy_key_fingerprint_rules() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("dk_fp.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login) = signup_and_login(&app, "fp@ex.com", "fpuser").await;
    let uid = login["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &uid).await;
    db.insert_repository("r-a", &uid, "user", "repoa", "public", "", "main")
        .await
        .unwrap();
    db.insert_repository("r-b", &uid, "user", "repob", "public", "", "main")
        .await
        .unwrap();

    let pk = serde_json::to_string(ED25519_A).unwrap();
    let first = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.deployKey.create","input":{{"owner":"fpuser","name":"repoa","title":"one","public_key":{pk}}}}}"#
        ),
    )
    .await;
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(first["data"]["can_write"], false, "default read-only");

    // Same key, same repo → dedupe.
    let dup = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.deployKey.create","input":{{"owner":"fpuser","name":"repoa","title":"two","public_key":{pk}}}}}"#
        ),
    )
    .await;
    assert_eq!(dup["ok"], false, "{dup}");
    assert_eq!(dup["error"]["code"], "deployKey.fingerprint_taken");

    // Same key, other repo → allowed (cross-repo reuse is deliberate).
    let second = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.deployKey.create","input":{{"owner":"fpuser","name":"repob","title":"one-b","public_key":{pk}}}}}"#
        ),
    )
    .await;
    assert_eq!(second["ok"], true, "{second}");

    // A key already registered as an account SSH key cannot be a deploy key.
    let pkb = serde_json::to_string(ED25519_B).unwrap();
    let acct = rpc_json(
        &app,
        &cookie,
        &format!(r#"{{"procedure":"sshKey.add","input":{{"title":"mine","public_key":{pkb}}}}}"#),
    )
    .await;
    assert_eq!(acct["ok"], true, "{acct}");
    let acct_as_deploy = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.deployKey.create","input":{{"owner":"fpuser","name":"repoa","title":"acct","public_key":{pkb}}}}}"#
        ),
    )
    .await;
    assert_eq!(acct_as_deploy["ok"], false, "{acct_as_deploy}");
    assert_eq!(
        acct_as_deploy["error"]["code"],
        "deployKey.fingerprint_taken"
    );
}

/// `sshKey.add` rejects a fingerprint already attached as a deploy key
/// (account path would silently widen a read-only scope).
#[tokio::test]
async fn ssh_key_add_rejects_deploy_key_fingerprint() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("dk_mutual.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login) = signup_and_login(&app, "mx@ex.com", "mxuser").await;
    let uid = login["data"]["id"].as_str().unwrap().to_string();
    verify_user(&db, &uid).await;
    db.insert_repository("r-mx", &uid, "user", "core", "public", "", "main")
        .await
        .unwrap();

    let pk = serde_json::to_string(ED25519_A).unwrap();
    let created = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.deployKey.create","input":{{"owner":"mxuser","name":"core","title":"ci","public_key":{pk}}}}}"#
        ),
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");

    let acct = rpc_json(
        &app,
        &cookie,
        &format!(r#"{{"procedure":"sshKey.add","input":{{"title":"mine","public_key":{pk}}}}}"#),
    )
    .await;
    assert_eq!(acct["ok"], false, "{acct}");
    assert_eq!(acct["error"]["code"], "sshKey.fingerprint_taken");
}

// ---------- SSH transport tests ----------

/// Read-only deploy key authenticates and gets upload-pack on its repo only;
/// receive-pack is denied; another repo (even public) is denied.
#[tokio::test]
async fn deploy_key_ssh_read_scope() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("dk_ssh.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    let repos = tmp.path().join("repos");

    let owner_id =
        make_user_and_repo(&db, "own@ex.com", "dkown", "r-main", "mainrepo", "private").await;
    db.insert_repository(
        "r-other",
        &owner_id,
        "user",
        "otherrepo",
        "public",
        "",
        "main",
    )
    .await
    .unwrap();
    let bare = repos.join("dkown").join("mainrepo.git");
    init_bare_repo(&bare);
    let bare_other = repos.join("dkown").join("otherrepo.git");
    init_bare_repo(&bare_other);

    let (priv_path, pub_line, fp) = write_keypair(tmp.path(), "id_ed25519_dk");
    db.create_deploy_key(
        "dk-1",
        "r-main",
        "ci-ro",
        &pub_line,
        &fp,
        "ssh-ed25519",
        false,
        &owner_id,
    )
    .await
    .unwrap();

    let (addr, _stop) =
        spawn_listener(ssh_state(db.clone(), repos), "127.0.0.1:0".parse().unwrap())
            .await
            .expect("listen");

    // 1. Auth succeeds — deploy key is a valid transport credential.
    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let session = connect_auth(addr, "git", key)
        .await
        .expect("deploy key must authenticate as git");

    // 2. upload-pack on the attached repo → allowed.
    let mut ch = session.channel_open_session().await.unwrap();
    ch.exec(true, "git-upload-pack 'dkown/mainrepo.git'")
        .await
        .expect("exec");
    assert!(saw_pack_data(&mut ch).await, "read deploy key must fetch");

    // 3. receive-pack on the attached repo → denied (read-only).
    let mut ch = session.channel_open_session().await.unwrap();
    ch.exec(true, "git-receive-pack 'dkown/mainrepo.git'")
        .await
        .expect("exec");
    assert!(
        saw_deny(&mut ch).await,
        "read-only deploy key must not push"
    );

    // 4. upload-pack on a repo the key is NOT attached to → denied, even public.
    let mut ch = session.channel_open_session().await.unwrap();
    ch.exec(true, "git-upload-pack 'dkown/otherrepo.git'")
        .await
        .expect("exec");
    assert!(
        saw_deny(&mut ch).await,
        "deploy key must not reach unattached repos"
    );
}

/// A `can_write` deploy key gets receive-pack on its repo.
#[tokio::test]
async fn deploy_key_ssh_write_scope_pushes() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("dk_ssh_rw.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    let repos = tmp.path().join("repos");

    let owner_id = make_user_and_repo(&db, "rw@ex.com", "dkrw", "r-rw", "rwrepo", "private").await;
    let bare = repos.join("dkrw").join("rwrepo.git");
    init_bare_repo(&bare);

    let (priv_path, pub_line, fp) = write_keypair(tmp.path(), "id_ed25519_rw");
    db.create_deploy_key(
        "dk-rw",
        "r-rw",
        "ci-rw",
        &pub_line,
        &fp,
        "ssh-ed25519",
        true,
        &owner_id,
    )
    .await
    .unwrap();

    let (addr, _stop) =
        spawn_listener(ssh_state(db.clone(), repos), "127.0.0.1:0".parse().unwrap())
            .await
            .expect("listen");
    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let session = connect_auth(addr, "git", key).await.expect("auth");

    let mut ch = session.channel_open_session().await.unwrap();
    ch.exec(true, "git-receive-pack 'dkrw/rwrepo.git'")
        .await
        .expect("exec");
    assert!(
        saw_pack_data(&mut ch).await,
        "write deploy key must reach receive-pack"
    );
}

/// Deleting a deploy key revokes SSH auth entirely.
#[tokio::test]
async fn deploy_key_ssh_delete_revokes() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("dk_ssh_del.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    let repos = tmp.path().join("repos");
    std::fs::create_dir_all(&repos).unwrap();

    let owner_id =
        make_user_and_repo(&db, "del@ex.com", "dkdel", "r-del", "delrepo", "private").await;
    let (priv_path, pub_line, fp) = write_keypair(tmp.path(), "id_ed25519_del");
    db.create_deploy_key(
        "dk-del",
        "r-del",
        "ci",
        &pub_line,
        &fp,
        "ssh-ed25519",
        false,
        &owner_id,
    )
    .await
    .unwrap();

    let (addr, _stop) =
        spawn_listener(ssh_state(db.clone(), repos), "127.0.0.1:0".parse().unwrap())
            .await
            .expect("listen");
    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    connect_auth(addr, "git", key.clone())
        .await
        .expect("attached deploy key authenticates");

    db.revoke_deploy_key("r-del", "dk-del").await.unwrap();
    let err = connect_auth(addr, "git", key).await;
    assert!(err.is_err(), "deleted deploy key must not authenticate");
}

/// An account key on the same instance is unaffected by deploy keys — it still
/// resolves to the full user identity and reaches both repos' upload-pack.
#[tokio::test]
async fn deploy_key_does_not_affect_account_key() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("dk_ssh_acct.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    let repos = tmp.path().join("repos");

    let owner_id =
        make_user_and_repo(&db, "acc@ex.com", "dkacc", "r-acc", "accrepo", "private").await;
    let bare = repos.join("dkacc").join("accrepo.git");
    init_bare_repo(&bare);

    // Deploy key on the repo (different fingerprint).
    let (_dk_priv, dk_pub, dk_fp) = write_keypair(tmp.path(), "id_ed25519_dk2");
    db.create_deploy_key(
        "dk-x",
        "r-acc",
        "ci",
        &dk_pub,
        &dk_fp,
        "ssh-ed25519",
        false,
        &owner_id,
    )
    .await
    .unwrap();

    // Account key for the owner.
    let (acct_priv, acct_pub, acct_fp) = write_keypair(tmp.path(), "id_ed25519_acct");
    db.create_ssh_key(
        "sk-1",
        &owner_id,
        "laptop",
        &acct_pub,
        &acct_fp,
        "ssh-ed25519",
        true,
        true,
    )
    .await
    .unwrap();

    let (addr, _stop) =
        spawn_listener(ssh_state(db.clone(), repos), "127.0.0.1:0".parse().unwrap())
            .await
            .expect("listen");

    let key = Arc::new(load_secret_key(&acct_priv, None).unwrap());
    let session = connect_auth(addr, "git", key)
        .await
        .expect("account key still authenticates");
    let mut ch = session.channel_open_session().await.unwrap();
    ch.exec(true, "git-upload-pack 'dkacc/accrepo.git'")
        .await
        .expect("exec");
    assert!(
        saw_pack_data(&mut ch).await,
        "account key keeps full repo access"
    );
}
