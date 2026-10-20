//! `POST /api/v1/repos/migrate` — Gitea-compatible git migration against a
//! real Smart-HTTP remote served by the same app instance.
//!
//! Unlike most REST tests this needs a live listener: the server fetches the
//! remote over HTTP itself, so `oneshot` cannot drive it.

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::sync::Arc;

use oxidean_api::auth::session::sha256_hex;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use tempfile::TempDir;
use uuid::Uuid;

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

async fn make_pat(db: &Database, user_id: &str) -> String {
    let token = format!("oxidean_pat_{}", Uuid::new_v4().simple());
    let hash = sha256_hex(token.as_bytes());
    db.create_pat(
        &Uuid::new_v4().to_string(),
        user_id,
        "classic",
        "migrate-test",
        &token[..16],
        &hash,
        Some(r#"["repo"]"#),
        None,
        None,
        None,
        None,
        &[],
    )
    .await
    .expect("create_pat");
    token
}

/// `git init --bare` + seed commits on `main`, plus a `dev` branch and `v1` tag.
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
    // Extra ref surface so the migrate asserts cover non-default refs.
    git(&["branch", "dev"]);
    git(&["tag", "v1"]);
    git(&["push", "-q", "origin", "dev", "v1"]);
    let _ = std::fs::remove_dir_all(&work);
}

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

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rest_migrate_clones_remote_repo() {
    let tmp = TempDir::new().unwrap();
    let db = test_db(&tmp, "migrate_clone").await;
    let repos = tmp.path().join("repos");

    // Source repo served by this instance over Smart HTTP.
    let src_uid = make_user(&db, "src@ex.com", "srcuser").await;
    let src_bare = repos.join("srcuser").join("origin.git");
    init_bare_repo(&src_bare, 2);
    db.insert_repository("src1", &src_uid, "user", "origin", "public", "d", "main")
        .await
        .unwrap();

    let uid = make_user(&db, "m@ex.com", "migrator").await;
    let pat = make_pat(&db, &uid).await;

    let base = serve_http(test_app(db.clone(), repos.clone())).await;
    let clone_addr = format!("{base}/srcuser/origin.git");

    // POST /repos/migrate — Gitea request shape.
    let resp = client()
        .post(format!("{base}/api/v1/repos/migrate"))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "clone_addr": clone_addr,
            "repo_name": "imported",
            "description": "migrated repo",
            "service": "git",
        }))
        .send()
        .await
        .expect("migrate request");
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    assert_eq!(status, 201, "migrate failed: {body}");
    assert_eq!(body["name"], "imported");
    assert_eq!(body["owner_username"], "migrator");
    assert_eq!(body["default_branch"], "main");

    // Branches + tags landed verbatim.
    let resp = client()
        .get(format!("{base}/api/v1/repos/migrator/imported/branches"))
        .send()
        .await
        .unwrap();
    let st = resp.status();
    let branches: serde_json::Value = resp.json().await.unwrap_or_default();
    assert_eq!(st, 200, "branches: {branches}");
    let names: Vec<&str> = branches
        .as_array()
        .unwrap_or_else(|| panic!("expected array, got {branches}"))
        .iter()
        .filter_map(|b| b["name"].as_str())
        .collect();
    assert!(names.contains(&"main"), "missing main: {branches}");
    assert!(names.contains(&"dev"), "missing dev: {branches}");

    let resp = client()
        .get(format!("{base}/api/v1/repos/migrator/imported/tags"))
        .send()
        .await
        .unwrap();
    let tags: serde_json::Value = resp.json().await.unwrap();
    let tag_names: Vec<&str> = tags["tags"]
        .as_array()
        .unwrap_or_else(|| panic!("expected tags array, got {tags}"))
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(tag_names.contains(&"v1"), "missing v1 tag: {tags}");

    // Blob content is readable — objects really landed, not just refs.
    let resp = client()
        .get(format!("{base}/api/v1/repos/migrator/imported/contents/f"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "contents: {}", resp.status());
    let content: serde_json::Value = resp.json().await.unwrap();
    assert!(
        content["content"].as_str().unwrap_or_default().len() > 4,
        "expected base64 content: {content}"
    );
    // Stronger assertion via git itself: clone the migrated repo and diff.
    let dest = tmp.path().join("migrated-clone");
    let out = StdCommand::new("git")
        .args([
            "clone",
            "-q",
            &format!("{base}/migrator/imported.git"),
            &dest.to_string_lossy(),
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git clone");
    assert!(
        out.status.success(),
        "clone of migrated repo failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("f")).unwrap(),
        "content 1\n"
    );
    // `dev` cloned too, and HEAD tracks main.
    let br = StdCommand::new("git")
        .arg("-C")
        .arg(&dest)
        .args(["branch", "-r"])
        .output()
        .unwrap();
    let remote_branches = String::from_utf8_lossy(&br.stdout);
    assert!(remote_branches.contains("origin/main"), "{remote_branches}");
    assert!(remote_branches.contains("origin/dev"), "{remote_branches}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rest_migrate_derives_name_and_compensates() {
    let tmp = TempDir::new().unwrap();
    let db = test_db(&tmp, "migrate_err").await;
    let repos = tmp.path().join("repos");
    let src_uid = make_user(&db, "s@ex.com", "src2").await;
    let src_bare = repos.join("src2").join("autosrc.git");
    init_bare_repo(&src_bare, 1);
    db.insert_repository("s2", &src_uid, "user", "autosrc", "public", "", "main")
        .await
        .unwrap();
    let uid = make_user(&db, "m2@ex.com", "mig2").await;
    let pat = make_pat(&db, &uid).await;
    let base = serve_http(test_app(db.clone(), repos.clone())).await;

    // Empty repo_name → derived from clone URL basename ("autosrc").
    let resp = client()
        .post(format!("{base}/api/v1/repos/migrate"))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "clone_addr": format!("{base}/src2/autosrc.git"),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        201,
        "{}",
        resp.text().await.unwrap_or_default()
    );

    // Unreachable remote → error, and the repo row/dir are compensated away.
    let resp = client()
        .post(format!("{base}/api/v1/repos/migrate"))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "clone_addr": format!("{base}/src2/ghost.git"),
            "repo_name": "ghostcopy",
        }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    assert!(
        status.is_client_error() || status.is_server_error(),
        "unreachable remote should fail, got {status}"
    );
    let resp = client()
        .get(format!("{base}/api/v1/repos/mig2/ghostcopy"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        404,
        "compensation should remove the failed repo row"
    );

    // service other than git is rejected up front.
    let resp = client()
        .post(format!("{base}/api/v1/repos/migrate"))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "clone_addr": format!("{base}/src2/autosrc.git"),
            "repo_name": "svc",
            "service": "github",
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_client_error());
}
