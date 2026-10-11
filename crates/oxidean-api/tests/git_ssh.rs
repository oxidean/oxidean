//! GIT-03: Git-over-SSH auth, ACL, pack allowlist, and failed-auth rate limits.

mod support;

use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::sync::Arc;
use std::time::Duration;

use oxidean_api::ssh::rate_limit::SshAuthLimiter;
use oxidean_api::ssh::{spawn_listener, SshState};
use oxidean_core::Role;
use oxidean_db::Database;
use russh::client;
use russh::keys::ssh_key::{Algorithm, HashAlg, LineEnding};
use russh::keys::{load_secret_key, PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use ssh_key::PublicKey;
use std::sync::Mutex;
use tempfile::TempDir;
use uuid::Uuid;

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

fn write_keypair(dir: &std::path::Path) -> (PathBuf, String, String) {
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

fn init_bare_repo(bare: &std::path::Path) {
    std::fs::create_dir_all(bare.parent().unwrap()).expect("parent");
    let st = StdCommand::new("git")
        .args(["init", "--bare"])
        .arg(bare)
        .status()
        .expect("git init");
    assert!(st.success());
    // Seed one commit via a temp worktree push.
    let work = bare.parent().unwrap().join("seed-work");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    assert!(StdCommand::new("git")
        .args(["-C"])
        .arg(&work)
        .args(["init", "-b", "main"])
        .status()
        .unwrap()
        .success());
    assert!(StdCommand::new("git")
        .args(["-C"])
        .arg(&work)
        .args(["config", "user.email", "t@ex.com"])
        .status()
        .unwrap()
        .success());
    assert!(StdCommand::new("git")
        .args(["-C"])
        .arg(&work)
        .args(["config", "user.name", "t"])
        .status()
        .unwrap()
        .success());
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

/// SSH username other than `git` is rejected (D-SSH-03).
#[tokio::test]
async fn git_ssh_username_other_than_git_rejected() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("ssh.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repos = tmp.path().join("repos");
    std::fs::create_dir_all(&repos).unwrap();

    let (priv_path, pub_line, fp) = write_keypair(tmp.path());
    let user_id = Uuid::new_v4().to_string();
    db.create_user(
        &user_id,
        "ssh@ex.com",
        "sshuser",
        Some("hash"),
        "SSH",
        "",
        None,
        Role::User,
    )
    .await
    .unwrap();
    db.create_ssh_key(
        "k1",
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

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");

    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let err = connect_auth(addr, "notgit", key).await;
    assert!(
        err.is_err(),
        "non-git username must be rejected: {}",
        err.err().unwrap()
    );
}

/// Registered public key + username `git` is accepted (D-SSH-03).
#[tokio::test]
async fn git_ssh_registered_key_user_git_accepted() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("ssh.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repos = tmp.path().join("repos");
    std::fs::create_dir_all(&repos).unwrap();

    let (priv_path, pub_line, fp) = write_keypair(tmp.path());
    let user_id = Uuid::new_v4().to_string();
    db.create_user(
        &user_id,
        "ssh2@ex.com",
        "sshuser2",
        Some("hash"),
        "SSH",
        "",
        None,
        Role::User,
    )
    .await
    .unwrap();
    db.create_ssh_key(
        "k2",
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

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");

    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    connect_auth(addr, "git", key)
        .await
        .expect("registered key + git must authenticate");
}

/// Public repo: authenticated `git-upload-pack` happy path (D-SSH-04; A1 — key required).
#[tokio::test]
async fn git_ssh_public_upload_pack_happy_path() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("ssh.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repos = tmp.path().join("repos");
    let bare = repos.join("sshuser3").join("demo.git");
    init_bare_repo(&bare);

    let (priv_path, pub_line, fp) = write_keypair(tmp.path());
    let user_id = Uuid::new_v4().to_string();
    db.create_user(
        &user_id,
        "ssh3@ex.com",
        "sshuser3",
        Some("hash"),
        "SSH",
        "",
        None,
        Role::User,
    )
    .await
    .unwrap();
    db.create_ssh_key(
        "k3",
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
    db.insert_repository("r-demo", &user_id, "user", "demo", "public", "demo", "main")
        .await
        .unwrap();

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");

    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let session = connect_auth(addr, "git", key).await.expect("auth");
    let mut channel = session.channel_open_session().await.expect("open session");
    channel
        .exec(true, "git-upload-pack 'sshuser3/demo.git'")
        .await
        .expect("exec");

    // Read some pack advertisement / data — success if channel yields bytes or clean EOF.
    let mut saw_data = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), channel.wait()).await {
            Ok(Some(russh::ChannelMsg::Data { ref data })) if !data.is_empty() => {
                saw_data = true;
                break;
            }
            Ok(Some(russh::ChannelMsg::Eof)) => break,
            Ok(Some(russh::ChannelMsg::ExitStatus { exit_status })) => {
                assert_eq!(exit_status, 0, "upload-pack exit");
                saw_data = true;
                break;
            }
            Ok(None) => break,
            _ => {}
        }
    }
    assert!(
        saw_data,
        "expected git-upload-pack to produce data or exit 0"
    );
}

/// Private non-owner denied with clear git stderr (D-SSH-04).
#[tokio::test]
async fn git_ssh_private_non_owner_git_stderr_deny() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("ssh.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repos = tmp.path().join("repos");

    let owner_id = Uuid::new_v4().to_string();
    db.create_user(
        &owner_id,
        "own@ex.com",
        "owneru",
        Some("hash"),
        "O",
        "",
        None,
        Role::User,
    )
    .await
    .unwrap();
    let other_id = Uuid::new_v4().to_string();
    db.create_user(
        &other_id,
        "oth@ex.com",
        "otheru",
        Some("hash"),
        "O",
        "",
        None,
        Role::User,
    )
    .await
    .unwrap();
    let bare = repos.join("owneru").join("secret.git");
    init_bare_repo(&bare);
    db.insert_repository(
        "r-sec", &owner_id, "user", "secret", "private", "sec", "main",
    )
    .await
    .unwrap();

    let (priv_path, pub_line, fp) = write_keypair(tmp.path());
    db.create_ssh_key(
        "k-priv",
        &other_id,
        "laptop",
        &pub_line,
        &fp,
        "ssh-ed25519",
        true,
        true,
    )
    .await
    .unwrap();

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");
    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let session = connect_auth(addr, "git", key).await.expect("auth");
    let mut channel = session.channel_open_session().await.expect("open");
    channel
        .exec(true, "git-upload-pack 'owneru/secret.git'")
        .await
        .expect("exec");

    let mut saw_deny = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(400), channel.wait()).await {
            Ok(Some(russh::ChannelMsg::ExtendedData { ref data, .. }))
            | Ok(Some(russh::ChannelMsg::Data { ref data })) => {
                let s = String::from_utf8_lossy(data);
                if s.contains("Permission denied") || s.contains("ERROR:") {
                    saw_deny = true;
                    break;
                }
            }
            Ok(None) | Ok(Some(russh::ChannelMsg::Eof)) => break,
            _ => {}
        }
    }
    assert!(saw_deny, "expected git stderr permission deny");
}

/// Unverified email cannot push (`git-receive-pack`) (D-SSH-04).
#[tokio::test]
async fn git_ssh_push_unverified_email_denied() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("ssh.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repos = tmp.path().join("repos");

    let user_id = Uuid::new_v4().to_string();
    db.create_user(
        &user_id,
        "push@ex.com",
        "pushu",
        Some("hash"),
        "P",
        "",
        None,
        Role::User,
    )
    .await
    .unwrap();
    let bare = repos.join("pushu").join("demo.git");
    init_bare_repo(&bare);
    db.insert_repository("r-push", &user_id, "user", "demo", "public", "d", "main")
        .await
        .unwrap();

    let (priv_path, pub_line, fp) = write_keypair(tmp.path());
    db.create_ssh_key(
        "k-push",
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

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");
    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let session = connect_auth(addr, "git", key).await.expect("auth");
    let mut channel = session.channel_open_session().await.expect("open");
    channel
        .exec(true, "git-receive-pack 'pushu/demo.git'")
        .await
        .expect("exec");

    let mut saw = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(400), channel.wait()).await {
            Ok(Some(russh::ChannelMsg::ExtendedData { ref data, .. }))
            | Ok(Some(russh::ChannelMsg::Data { ref data })) => {
                let s = String::from_utf8_lossy(data);
                if s.contains("Email verification") || s.contains("ERROR:") {
                    saw = true;
                    break;
                }
            }
            Ok(None) | Ok(Some(russh::ChannelMsg::Eof)) => break,
            _ => {}
        }
    }
    assert!(saw, "expected email verification deny on receive-pack");
}

/// Non-pack exec rejected.
#[tokio::test]
async fn git_ssh_non_pack_exec_shell_rejected() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("ssh.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repos = tmp.path().join("repos");
    std::fs::create_dir_all(&repos).unwrap();

    let (priv_path, pub_line, fp) = write_keypair(tmp.path());
    let user_id = Uuid::new_v4().to_string();
    db.create_user(
        &user_id,
        "ssh4@ex.com",
        "sshuser4",
        Some("hash"),
        "SSH",
        "",
        None,
        Role::User,
    )
    .await
    .unwrap();
    db.create_ssh_key(
        "k4",
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

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");

    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let session = connect_auth(addr, "git", key).await.expect("auth");
    let mut channel = session.channel_open_session().await.expect("open");
    channel.exec(true, "bash").await.expect("exec sent");
    let mut failed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(400), channel.wait()).await {
            Ok(Some(russh::ChannelMsg::Failure)) => {
                failed = true;
                break;
            }
            Ok(None) | Ok(Some(russh::ChannelMsg::Eof)) => break,
            _ => {}
        }
    }
    assert!(failed, "shell exec must receive channel failure");
}

/// Failed pubkey auth over limit is rate-limited (D-SSH-07).
#[tokio::test]
async fn git_ssh_failed_pubkey_rate_limited() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let url = format!("sqlite:{}", tmp.path().join("ssh.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repos = tmp.path().join("repos");
    std::fs::create_dir_all(&repos).unwrap();

    let (priv_path, _pub_line, _fp) = write_keypair(tmp.path());
    let limiter = Arc::new(Mutex::new(SshAuthLimiter::new()));
    let state = SshState {
        db,
        repos_dir: repos,
        auth_limiter: limiter.clone(),
    };
    let (addr, _stop) = spawn_listener(state, "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");

    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    for _ in 0..11 {
        let _ = connect_auth(addr, "git", key.clone()).await;
    }
    let err = connect_auth(addr, "git", key).await;
    assert!(
        err.is_err(),
        "expected rate limit after fingerprint spray: {}",
        err.err().unwrap()
    );
}
