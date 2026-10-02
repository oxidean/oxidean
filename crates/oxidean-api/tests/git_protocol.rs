//! GIT-27: Git protocol surface audit — protocol v2, partial clone (filter),
//! and shallow clone/fetch/push on both transports (Smart HTTP + SSH).
//!
//! These tests spawn the real `git`/`ssh` client binaries against a bound
//! Axum listener and the in-process russh listener, so negotiation is verified
//! at the wire level rather than assumed.

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Output};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oxidean_api::auth::session::sha256_hex;
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
use tempfile::TempDir;
use uuid::Uuid;

// ---------- shared helpers ----------

fn test_app(db: Database, repos_dir: PathBuf) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir);
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

async fn test_db(tmp: &TempDir, name: &str) -> Database {
    let url = format!("sqlite:{}", tmp.path().join(format!("{name}.db")).display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    db
}

/// Create a verified user row directly (no signup RPC needed for git tests).
async fn make_user(db: &Database, email: &str, username: &str) -> String {
    let id = Uuid::new_v4().to_string();
    db.create_user(
        &id,
        email,
        username,
        Some("hash"),
        username,
        "",
        None,
        Role::User,
    )
    .await
    .expect("create_user");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&id, &now).await.expect("verify");
    id
}

/// Mint a classic `repo`-scope PAT and return the bearer token string.
async fn make_pat(db: &Database, user_id: &str) -> String {
    let token = format!("oxidean_pat_{}", Uuid::new_v4().simple());
    let hash = sha256_hex(token.as_bytes());
    db.create_pat(
        &Uuid::new_v4().to_string(),
        user_id,
        "classic",
        "git-protocol-test",
        &token[..16],
        &hash,
        Some(r#"["repo"]"#),
        None,
        None,
        None,
        &[],
    )
    .await
    .expect("create_pat");
    token
}

/// `git init --bare` + seed `count` commits on `main`.
fn init_bare_repo(bare: &Path, count: usize) {
    std::fs::create_dir_all(bare.parent().unwrap()).expect("parent");
    let st = StdCommand::new("git")
        .args(["init", "--bare", "-b", "main"])
        .arg(bare)
        .status()
        .expect("git init --bare");
    assert!(st.success());

    let work = bare.parent().unwrap().join(format!(
        "seed-{}",
        bare.file_stem().unwrap().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let git = |args: &[&str]| {
        let st = StdCommand::new("git")
            .arg("-C")
            .arg(&work)
            .args(args)
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?} failed");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "t@ex.com"]);
    git(&["config", "user.name", "t"]);
    for i in 0..count {
        std::fs::write(work.join("f"), format!("content {i}\n")).unwrap();
        git(&["add", "f"]);
        git(&[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            &format!("c{i}"),
        ]);
    }
    git(&["remote", "add", "origin", &bare.to_string_lossy()]);
    git(&["push", "-q", "origin", "main"]);
    let _ = std::fs::remove_dir_all(&work);
}

/// Run `git` with extra env; returns captured Output (never panics on exit).
fn git_capture(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = StdCommand::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("NO_COLOR", "1");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("spawn git")
}

/// Serve the app on 127.0.0.1:0 for the duration of the test.
/// Returns the `http://host:port` base. The task aborts when the test exits.
async fn serve_http(app: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app.into_make_service()).await;
    });
    format!("http://127.0.0.1:{port}")
}

// ---------- Smart HTTP ----------

/// Protocol v2 is advertised and honored on Smart HTTP (Git-Protocol → GIT_PROTOCOL).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_protocol_v2_ls_remote() {
    let tmp = TempDir::new().unwrap();
    let db = test_db(&tmp, "proto_http_v2").await;
    let repos = tmp.path().join("repos");
    let uid = make_user(&db, "v2@ex.com", "v2owner").await;
    let bare = repos.join("v2owner").join("demo.git");
    init_bare_repo(&bare, 2);
    db.insert_repository("r1", &uid, "user", "demo", "public", "d", "main")
        .await
        .unwrap();

    let base = serve_http(test_app(db, repos)).await;
    let out = git_capture(
        tmp.path(),
        &[
            "-c",
            "protocol.version=2",
            "ls-remote",
            &format!("{base}/v2owner/demo.git"),
        ],
        &[("GIT_TRACE_PACKET", "1")],
    );
    assert!(
        out.status.success(),
        "ls-remote failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("version 2"),
        "protocol v2 must be negotiated over Smart HTTP; trace:\n{stderr}"
    );
}

/// `--filter=blob:none` must be honored (server advertises `filter`, client
/// records promisor config — GIT-27).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_partial_clone_blob_none() {
    let tmp = TempDir::new().unwrap();
    let db = test_db(&tmp, "proto_http_filter").await;
    let repos = tmp.path().join("repos");
    let uid = make_user(&db, "f@ex.com", "fowner").await;
    let bare = repos.join("fowner").join("demo.git");
    init_bare_repo(&bare, 2);
    db.insert_repository("r1", &uid, "user", "demo", "public", "d", "main")
        .await
        .unwrap();

    let base = serve_http(test_app(db, repos)).await;
    let dest = tmp.path().join("filtered");
    let out = git_capture(
        tmp.path(),
        &[
            "clone",
            "--filter=blob:none",
            &format!("{base}/fowner/demo.git"),
            &dest.to_string_lossy(),
        ],
        &[],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "filtered clone failed: {stderr}");
    assert!(
        !stderr.contains("filtering not recognized"),
        "server must advertise filter: {stderr}"
    );
    let cfg = git_capture(&dest, &["config", "remote.origin.promisor"], &[]);
    assert_eq!(
        String::from_utf8_lossy(&cfg.stdout).trim(),
        "true",
        "promisor flag must be set on a real partial clone"
    );
    let flt = git_capture(&dest, &["config", "remote.origin.partialclonefilter"], &[]);
    assert_eq!(String::from_utf8_lossy(&flt.stdout).trim(), "blob:none");
    // Lazy backfill: reading the blob must fetch it on demand.
    let cat = git_capture(&dest, &["cat-file", "-p", "HEAD:f"], &[]);
    assert!(
        cat.status.success(),
        "blob backfill failed: {}",
        String::from_utf8_lossy(&cat.stderr)
    );
}

/// `--depth=1` shallow clone and `--deepen` on Smart HTTP.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_shallow_clone_and_deepen() {
    let tmp = TempDir::new().unwrap();
    let db = test_db(&tmp, "proto_http_shallow").await;
    let repos = tmp.path().join("repos");
    let uid = make_user(&db, "s@ex.com", "sowner").await;
    let bare = repos.join("sowner").join("demo.git");
    init_bare_repo(&bare, 3);
    db.insert_repository("r1", &uid, "user", "demo", "public", "d", "main")
        .await
        .unwrap();

    let base = serve_http(test_app(db, repos)).await;
    let dest = tmp.path().join("shallow");
    let out = git_capture(
        tmp.path(),
        &[
            "clone",
            "--depth=1",
            &format!("{base}/sowner/demo.git"),
            &dest.to_string_lossy(),
        ],
        &[],
    );
    assert!(
        out.status.success(),
        "shallow clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let shallow = git_capture(&dest, &["rev-parse", "--is-shallow-repository"], &[]);
    assert_eq!(String::from_utf8_lossy(&shallow.stdout).trim(), "true");
    let count = git_capture(&dest, &["rev-list", "--count", "HEAD"], &[]);
    assert_eq!(String::from_utf8_lossy(&count.stdout).trim(), "1");

    let deepen = git_capture(&dest, &["fetch", "--deepen=1"], &[]);
    assert!(
        deepen.status.success(),
        "fetch --deepen failed: {}",
        String::from_utf8_lossy(&deepen.stderr)
    );
    let count = git_capture(&dest, &["rev-list", "--count", "HEAD"], &[]);
    assert_eq!(String::from_utf8_lossy(&count.stdout).trim(), "2");
}

/// Push from a shallow clone is accepted when connectivity holds (GIT-27).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_push_from_shallow_clone() {
    let tmp = TempDir::new().unwrap();
    let db = test_db(&tmp, "proto_http_push").await;
    let repos = tmp.path().join("repos");
    let uid = make_user(&db, "p@ex.com", "powner").await;
    let bare = repos.join("powner").join("demo.git");
    init_bare_repo(&bare, 2);
    db.insert_repository("r1", &uid, "user", "demo", "public", "d", "main")
        .await
        .unwrap();
    let token = make_pat(&db, &uid).await;

    let base = serve_http(test_app(db, repos)).await;
    let authed = base.replacen("http://", &format!("http://powner:{token}@"), 1);
    let dest = tmp.path().join("pusher");
    let out = git_capture(
        tmp.path(),
        &[
            "clone",
            "--depth=1",
            &format!("{authed}/powner/demo.git"),
            &dest.to_string_lossy(),
        ],
        &[],
    );
    assert!(
        out.status.success(),
        "authed shallow clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    std::fs::write(dest.join("new.txt"), "from shallow\n").unwrap();
    let ok = |args: &[&str]| {
        let o = git_capture(&dest, args, &[]);
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    };
    ok(&["config", "user.email", "p@ex.com"]);
    ok(&["config", "user.name", "p"]);
    ok(&["add", "new.txt"]);
    ok(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-qm",
        "shallow push",
    ]);
    let push = git_capture(&dest, &["push", "origin", "main"], &[]);
    assert!(
        push.status.success(),
        "push from shallow clone must be accepted (receive-pack shallow-info): {}",
        String::from_utf8_lossy(&push.stderr)
    );

    let ls = git_capture(
        tmp.path(),
        &[
            "ls-remote",
            &format!("{authed}/powner/demo.git"),
            "refs/heads/main",
        ],
        &[],
    );
    let head = git_capture(&dest, &["rev-parse", "HEAD"], &[]);
    assert_eq!(
        String::from_utf8_lossy(&ls.stdout)
            .split_whitespace()
            .next(),
        Some(String::from_utf8_lossy(&head.stdout).trim()),
        "remote main must point at the pushed commit"
    );
}

// ---------- SSH (russh client) ----------

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

fn ssh_state(db: Database, repos_dir: PathBuf) -> SshState {
    SshState {
        db,
        repos_dir,
        auth_limiter: Arc::new(Mutex::new(SshAuthLimiter::new())),
    }
}

fn write_keypair(dir: &Path) -> (PathBuf, String, String) {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("gen key");
    let priv_path = dir.join("id_ed25519");
    let pem = key.to_openssh(LineEnding::LF).expect("encode");
    std::fs::write(&priv_path, pem.as_bytes()).expect("write priv");
    // OpenSSH refuses to use a group/other-readable private key.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&priv_path, std::fs::Permissions::from_mode(0o600))
            .expect("chmod key");
    }
    let pub_line = key.public_key().to_openssh().expect("pub");
    let fp = PublicKey::from_openssh(&pub_line)
        .expect("parse pub")
        .fingerprint(HashAlg::Sha256)
        .to_string();
    (priv_path, pub_line, fp)
}

async fn connect_auth(
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
    assert!(ok.success(), "pubkey auth must succeed");
    session
}

/// Register user + repo + ssh key; returns (user_id, owner, name).
async fn ssh_repo_fixture(tmp: &TempDir, name: &str, owner: &str) -> (Database, PathBuf, PathBuf) {
    let db = test_db(tmp, name).await;
    let repos = tmp.path().join("repos");
    let uid = make_user(&db, &format!("{owner}@ex.com"), owner).await;
    let bare = repos.join(owner).join("demo.git");
    init_bare_repo(&bare, 3);
    db.insert_repository("r1", &uid, "user", "demo", "public", "d", "main")
        .await
        .unwrap();
    let (priv_path, pub_line, fp) = write_keypair(tmp.path());
    db.create_ssh_key(
        &Uuid::new_v4().to_string(),
        &uid,
        "k",
        &pub_line,
        &fp,
        "ssh-ed25519",
        true,
        true,
    )
    .await
    .unwrap();
    (db, repos, priv_path)
}

/// GIT-27: `GIT_PROTOCOL` env request is accepted and reaches upload-pack —
/// first packet of a v2 session is the `version 2` banner.
#[tokio::test]
async fn ssh_env_request_git_protocol_negotiates_v2() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let (db, repos, priv_path) = ssh_repo_fixture(&tmp, "ssh_v2", "v2u").await;

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");
    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let session = connect_auth(addr, key).await;
    let mut channel = session.channel_open_session().await.expect("open session");
    channel
        .set_env(true, "GIT_PROTOCOL", "version=2")
        .await
        .expect("set_env");

    // Consume the env reply (want_reply=true → ChannelMsg::Success).
    let mut env_ok = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), channel.wait()).await {
            Ok(Some(russh::ChannelMsg::Success)) => {
                env_ok = true;
                break;
            }
            Ok(Some(russh::ChannelMsg::Failure)) => break,
            Ok(None) => break,
            _ => {}
        }
    }
    assert!(env_ok, "GIT_PROTOCOL env request must be accepted");

    channel
        .exec(true, "git-upload-pack 'v2u/demo.git'")
        .await
        .expect("exec");

    let mut saw_v2 = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), channel.wait()).await {
            Ok(Some(russh::ChannelMsg::Data { ref data })) => {
                if String::from_utf8_lossy(data).contains("version 2") {
                    saw_v2 = true;
                    break;
                }
            }
            Ok(Some(russh::ChannelMsg::Eof)) | Ok(None) => break,
            _ => {}
        }
    }
    assert!(
        saw_v2,
        "upload-pack must emit the protocol v2 banner when GIT_PROTOCOL=version=2"
    );
}

/// Control: without the env request, upload-pack speaks protocol v0
/// (first advertisement refs, no `version 2` banner).
#[tokio::test]
async fn ssh_no_env_request_stays_v0() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let (db, repos, priv_path) = ssh_repo_fixture(&tmp, "ssh_v0", "v0u").await;

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");
    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let session = connect_auth(addr, key).await;
    let mut channel = session.channel_open_session().await.expect("open session");
    channel
        .exec(true, "git-upload-pack 'v0u/demo.git'")
        .await
        .expect("exec");

    let mut saw_v2 = false;
    let mut saw_data = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), channel.wait()).await {
            Ok(Some(russh::ChannelMsg::Data { ref data })) => {
                saw_data = true;
                if String::from_utf8_lossy(data).contains("version 2") {
                    saw_v2 = true;
                }
                break;
            }
            Ok(Some(russh::ChannelMsg::Eof)) | Ok(None) => break,
            _ => {}
        }
    }
    assert!(saw_data, "v0 upload-pack must still advertise refs");
    assert!(
        !saw_v2,
        "v0 session must not emit the v2 banner (no silent upgrade)"
    );
}

/// Non-allowlisted env vars are refused (AcceptEnv-style policy).
#[tokio::test]
async fn ssh_env_request_non_allowlisted_denied() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let (db, repos, priv_path) = ssh_repo_fixture(&tmp, "ssh_envd", "envu").await;

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");
    let key = Arc::new(load_secret_key(&priv_path, None).unwrap());
    let session = connect_auth(addr, key).await;
    let mut channel = session.channel_open_session().await.expect("open session");
    channel
        .set_env(true, "LD_PRELOAD", "/tmp/evil.so")
        .await
        .expect("set_env");

    let mut denied = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), channel.wait()).await {
            Ok(Some(russh::ChannelMsg::Failure)) => {
                denied = true;
                break;
            }
            Ok(Some(russh::ChannelMsg::Success)) => break,
            Ok(None) => break,
            _ => {}
        }
    }
    assert!(denied, "non-allowlisted env request must be refused");
}

// ---------- SSH (real OpenSSH client e2e) ----------

fn git_ssh_command_env(priv_path: &Path) -> String {
    format!(
        "ssh -i {} -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o PreferredAuthentications=publickey",
        priv_path.display()
    )
}

/// End-to-end: real `git` + `ssh` binaries negotiate protocol v2 against the
/// russh listener (GIT-27).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ssh_e2e_protocol_v2_clone() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let (db, repos, priv_path) = ssh_repo_fixture(&tmp, "ssh_e2e_v2", "e2eu").await;

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");

    let url = format!("ssh://git@127.0.0.1:{}/e2eu/demo.git", addr.port());
    let dest = tmp.path().join("e2e-v2");
    let mut cmd = StdCommand::new("timeout");
    cmd.arg("25")
        .arg("git")
        .arg("-C")
        .arg(tmp.path())
        .args([
            "-c",
            "protocol.version=2",
            "clone",
            &url,
            &dest.to_string_lossy(),
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_TRACE_PACKET", "1")
        .env("GIT_SSH_COMMAND", git_ssh_command_env(&priv_path));
    let out = cmd.output().expect("git clone");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "ssh clone failed: {stderr}");
    assert!(
        stderr.contains("version 2"),
        "protocol v2 must be negotiated over SSH; trace:\n{stderr}"
    );
}

/// End-to-end over real ssh: partial clone advertises `filter`, shallow clone
/// works, and the shallow client can deepen.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ssh_e2e_filter_and_shallow_clone() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", tmp.path().join("host"));
    let (db, repos, priv_path) = ssh_repo_fixture(&tmp, "ssh_e2e_fs", "e2fs").await;

    let (addr, _stop) = spawn_listener(ssh_state(db, repos), "127.0.0.1:0".parse().unwrap())
        .await
        .expect("listen");
    let url = format!("ssh://git@127.0.0.1:{}/e2fs/demo.git", addr.port());
    let ssh_cmd = git_ssh_command_env(&priv_path);
    let envs: Vec<(&str, &str)> = vec![
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_SSH_COMMAND", ssh_cmd.as_str()),
    ];

    // Partial clone.
    let filtered = tmp.path().join("e2e-filtered");
    let out = git_capture(
        tmp.path(),
        &[
            "clone",
            "--filter=blob:none",
            &url,
            &filtered.to_string_lossy(),
        ],
        &envs,
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "ssh filtered clone failed: {stderr}");
    assert!(
        !stderr.contains("filtering not recognized"),
        "filter must be advertised over SSH: {stderr}"
    );
    let cfg = git_capture(&filtered, &["config", "remote.origin.promisor"], &[]);
    assert_eq!(String::from_utf8_lossy(&cfg.stdout).trim(), "true");

    // Shallow clone + deepen.
    let shallow = tmp.path().join("e2e-shallow");
    let out = git_capture(
        tmp.path(),
        &["clone", "--depth=1", &url, &shallow.to_string_lossy()],
        &envs,
    );
    assert!(
        out.status.success(),
        "ssh shallow clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let is_shallow = git_capture(&shallow, &["rev-parse", "--is-shallow-repository"], &[]);
    assert_eq!(String::from_utf8_lossy(&is_shallow.stdout).trim(), "true");
    let deepen = git_capture(&shallow, &["fetch", "--deepen=1"], &envs);
    assert!(
        deepen.status.success(),
        "ssh fetch --deepen failed: {}",
        String::from_utf8_lossy(&deepen.stderr)
    );
    let count = git_capture(&shallow, &["rev-list", "--count", "HEAD"], &[]);
    assert_eq!(String::from_utf8_lossy(&count.stdout).trim(), "2");
}
