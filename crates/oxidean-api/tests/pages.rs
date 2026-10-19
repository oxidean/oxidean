//! Pages domain: site config CRUD + authz, legacy branch builds, the
//! `actions/deploy-pages` job-token seam, push-driven rebuilds, `/pages`
//! serving (index/404/traversal/CSP), and `page_build` fan-out.

mod support;

use std::io::Write;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::actions::dispatch::enqueue_run;
use oxidean_api::actions::parse_workflow_yaml;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, FileChange, GitBackend};
use tower::ServiceExt;

const ZERO: &str = "0000000000000000000000000000000000000000";

struct T {
    app: axum::Router,
    db: Database,
    repos_dir: std::path::PathBuf,
    pages_dir: std::path::PathBuf,
    artifacts_dir: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

async fn test_app() -> T {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos_dir = dir.path().join("repos");
    let pages_dir = dir.path().join("pages");
    let artifacts_dir = dir.path().join("artifacts");
    let url = format!("sqlite:{}", dir.path().join("pages.db").display());
    let db = support::connect_test_db(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_repos_dir(repos_dir.clone())
    .with_pages_dir(pages_dir.clone())
    .with_actions_artifacts_dir(artifacts_dir.clone())
    .with_actions_enabled(true);
    let cors = build_cors("development", None).expect("cors");
    T {
        app: router_with_state(state, cors),
        db,
        repos_dir,
        pages_dir,
        artifacts_dir,
        _dir: dir,
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

fn rpc_req_bearer(body: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn rpc_json(app: &axum::Router, req: Request<Body>) -> serde_json::Value {
    let res = app.clone().oneshot(req).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn get_page(
    app: &axum::Router,
    uri: &str,
    cookie: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut b = Request::builder().method("GET").uri(uri);
    if let Some(c) = cookie {
        b = b.header("cookie", c);
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, headers, bytes.to_vec())
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
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    (cookie, serde_json::from_slice(&bytes).unwrap())
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

async fn create_repo(app: &axum::Router, cookie: &str, name: &str, visibility: &str) {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":""}}}}"#
    );
    let res = app
        .clone()
        .oneshot(rpc_req_cookie(&body, cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["ok"], true, "repo.create {name} — {v}");
}

/// Seed `repos/{owner}/{name}.git` with a branch carrying `files`.
async fn seed_repo(
    repos_dir: &std::path::Path,
    owner: &str,
    name: &str,
    branch: &str,
    files: &[(&str, &str)],
) -> std::path::PathBuf {
    let git = CliGitBackend::new();
    let bare = repos_dir.join(owner).join(format!("{name}.git"));
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    git.init_bare(&bare, branch).await.expect("init bare");
    let changes: Vec<FileChange> = files
        .iter()
        .map(|(p, c)| FileChange::Upsert {
            path: p.to_string(),
            content: c.as_bytes().to_vec(),
        })
        .collect();
    git.commit_files(
        &bare, branch, branch, "seed", &changes, "T", "t@t", None, "admin",
    )
    .await
    .expect("commit files");
    bare
}

/// Raw tar writer — `tar::Builder` refuses `..`/absolute paths at write time,
/// but a hostile artifact ships exactly those, so hostile entries are built
/// byte-by-byte.
fn raw_tar_entry(name: &str, data: &[u8]) -> Vec<u8> {
    let mut h = [0u8; 512];
    let nb = name.as_bytes();
    h[..nb.len().min(100)].copy_from_slice(&nb[..nb.len().min(100)]);
    h[100..108].copy_from_slice(b"0000644\0");
    h[108..116].copy_from_slice(b"0000000\0");
    h[116..124].copy_from_slice(b"0000000\0");
    let size = format!("{:011o}\0", data.len());
    h[124..136].copy_from_slice(size.as_bytes());
    h[136..148].copy_from_slice(b"00000000000\0");
    h[148..156].copy_from_slice(b"        ");
    h[156] = b'0';
    let sum: u32 = h.iter().map(|b| *b as u32).sum();
    let chk = format!("{:06o}\0 ", sum);
    h[148..156].copy_from_slice(chk.as_bytes());
    let mut out = h.to_vec();
    out.extend_from_slice(data);
    out.resize(out.len().div_ceil(512) * 512, 0);
    out
}

fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tar_bytes = Vec::new();
    for (name, data) in entries {
        tar_bytes.extend(raw_tar_entry(name, data));
    }
    tar_bytes.extend([0u8; 1024]);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&tar_bytes).unwrap();
    gz.finish().unwrap()
}

// ---------------------------------------------------------------------------
// CRUD + authz
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pages_crud_and_authz() {
    let t = test_app().await;
    let (cookie, login_v) = signup_and_login(&t.app, "pg1@ex.com", "pg1").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    verify_user(&t.db, &user_id).await;
    create_repo(&t.app, &cookie, "demo", "public").await;
    let repo =
        t.db.find_repository_by_owner_name(&user_id, "demo")
            .await
            .unwrap()
            .unwrap();
    seed_repo(
        &t.repos_dir,
        "pg1",
        "demo",
        "main",
        &[("index.html", "<h1>hi</h1>")],
    )
    .await;

    // Unconfigured → pages.not_found.
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.get","input":{"owner":"pg1","name":"demo"}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "pages.not_found", "{v}");

    // A second (non-collaborator) user cannot enable — resolve yields 404.
    let (cookie2, login2) = signup_and_login(&t.app, "pg2@ex.com", "pg2").await;
    verify_user(&t.db, login2["data"]["id"].as_str().unwrap()).await;
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.enable","input":{"owner":"pg1","name":"demo"}}"#,
            &cookie2,
        ),
    )
    .await;
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "pages.not_found", "outsider — {v}");

    // Owner enables — legacy defaults to the default branch at `/`, and the
    // first build runs (GitHub POST /pages semantics).
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.enable","input":{"owner":"pg1","name":"demo"}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["build_type"], "legacy");
    assert_eq!(v["data"]["source"]["branch"], "main");
    assert_eq!(v["data"]["source"]["path"], "/");
    assert_eq!(v["data"]["status"], "built", "initial build — {v}");
    assert_eq!(v["data"]["html_url"], "/pages/pg1/demo/");

    // Enable again → conflict.
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.enable","input":{"owner":"pg1","name":"demo"}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["error"]["code"], "pages.conflict", "{v}");

    // Update to /docs + cname validation.
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.update","input":{"owner":"pg1","name":"demo","source":{"branch":"main","path":"/docs"},"cname":"www.EXAMPLE.com"}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["source"]["path"], "/docs");
    assert_eq!(v["data"]["cname"], "www.example.com");
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.update","input":{"owner":"pg1","name":"demo","source":{"branch":"main","path":"/site"}}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["error"]["code"], "pages.invalid", "{v}");

    // Disable → config + tree gone; get 404s again.
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.disable","input":{"owner":"pg1","name":"demo"}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert!(!t.pages_dir.join(&repo.id).exists());
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.get","input":{"owner":"pg1","name":"demo"}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["error"]["code"], "pages.not_found", "{v}");

    // Archived repo rejects writes.
    let other =
        t.db.insert_repository(
            &uuid::Uuid::new_v4().to_string(),
            &user_id,
            "user",
            "arc",
            "private",
            "",
            "main",
        )
        .await
        .unwrap();
    t.db.set_repository_archived(&other.id, true).await.unwrap();
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.enable","input":{"owner":"pg1","name":"arc"}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["ok"], false, "archived write must fail — {v}");
}

// ---------------------------------------------------------------------------
// Legacy build + serving
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pages_legacy_build_serves_with_security_headers() {
    let t = test_app().await;
    let (cookie, login_v) = signup_and_login(&t.app, "pg3@ex.com", "pg3").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    verify_user(&t.db, &user_id).await;
    create_repo(&t.app, &cookie, "site", "public").await;
    seed_repo(
        &t.repos_dir,
        "pg3",
        "site",
        "main",
        &[
            ("index.html", "<h1>home</h1>"),
            ("docs/index.html", "<h1>docs</h1>"),
            ("docsx/decoy.html", "<h1>decoy</h1>"),
            ("404.html", "<h1>not here</h1>"),
            ("img/logo.svg", "<svg/>"),
        ],
    )
    .await;

    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.enable","input":{"owner":"pg3","name":"site","source":{"branch":"main","path":"/docs"}}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["status"], "built");

    // `/docs` materialized — docsx/ must NOT leak in (prefix-boundary).
    let (status, headers, body) = get_page(&t.app, "/pages/pg3/site/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"<h1>docs</h1>");
    assert_eq!(
        headers.get("content-security-policy").unwrap(),
        "sandbox allow-scripts"
    );
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");

    // Prefix-sibling decoy file must not exist on the site — and the site
    // root under `/docs` has no 404.html, so the miss is a bare 404.
    let (status, _, body) = get_page(&t.app, "/pages/pg3/site/decoy.html", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, b"Not Found");

    // Custom 404 body on miss — covered below under the `/` source, whose
    // tree carries the repo's own 404.html.
    let (status, _, _) = get_page(&t.app, "/pages/pg3/site/missing", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Traversal attempts → 404, never an escape.
    for p in [
        "/pages/pg3/site/../index.html",
        "/pages/pg3/site/..%2f..%2fetc",
        "/pages/pg3/site/%2e%2e%2fsecret",
    ] {
        let (status, _, _) = get_page(&t.app, p, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{p}");
    }

    // Bare site URL redirects to trailing slash.
    let (status, headers, _) = get_page(&t.app, "/pages/pg3/site", None).await;
    assert_eq!(status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(headers.get("location").unwrap(), "/pages/pg3/site/");

    // SVG carries the sandbox too (navigated XML can script).
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.update","input":{"owner":"pg3","name":"site","source":{"branch":"main","path":"/"}}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    let _ = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.builds.request","input":{"owner":"pg3","name":"site"}}"#,
            &cookie,
        ),
    )
    .await;
    let (status, headers, body) = get_page(&t.app, "/pages/pg3/site/img/logo.svg", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-security-policy").unwrap(),
        "sandbox allow-scripts"
    );
    assert_eq!(body, b"<svg/>");

    // The `/` source carries 404.html — misses render it at 404.
    let (status, _, body) = get_page(&t.app, "/pages/pg3/site/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, b"<h1>not here</h1>");
}

// ---------------------------------------------------------------------------
// Push-driven rebuild + page_build fan-out
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pages_rebuild_after_push_and_page_build_event() {
    let t = test_app().await;
    let (cookie, login_v) = signup_and_login(&t.app, "pg4@ex.com", "pg4").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    verify_user(&t.db, &user_id).await;
    create_repo(&t.app, &cookie, "site", "public").await;
    let repo =
        t.db.find_repository_by_owner_name(&user_id, "site")
            .await
            .unwrap()
            .unwrap();
    let bare = seed_repo(
        &t.repos_dir,
        "pg4",
        "site",
        "main",
        &[
            ("index.html", "v1"),
            (
                ".github/workflows/pb.yml",
                "name: PB\non: [page_build]\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hit\n",
            ),
        ],
    )
    .await;

    rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.enable","input":{"owner":"pg4","name":"site"}}"#,
            &cookie,
        ),
    )
    .await;
    let (status, _, body) = get_page(&t.app, "/pages/pg4/site/", None).await;
    assert_eq!(
        (status, body.as_slice()),
        (StatusCode::OK, b"v1".as_slice())
    );

    // Hook subscribed to page_build sees the next build's event.
    t.db.insert_webhook(
        "wh-pg",
        &repo.id,
        "http://127.0.0.1:9/h",
        "s",
        true,
        &serde_json::to_string(&["page_build"]).unwrap(),
        "hook",
        &user_id,
    )
    .await
    .expect("hook");

    // A new commit on the source branch → rebuild_after_push republishes.
    let git = CliGitBackend::new();
    let c2 = git
        .commit_files(
            &bare,
            "main",
            "main",
            "v2",
            &[FileChange::Upsert {
                path: "index.html".into(),
                content: b"v2".to_vec(),
            }],
            "T",
            "t@t",
            None,
            "admin",
        )
        .await
        .unwrap();
    oxidean_api::pages::rebuild_after_push(
        &t.db,
        &git,
        &t.repos_dir,
        &t.pages_dir,
        "development",
        &repo.id,
        "pg4",
        &user_id,
        &[(ZERO.to_string(), c2.sha.clone(), "refs/heads/main".into())],
        false,
        true,
    )
    .await;
    let (status, _, body) = get_page(&t.app, "/pages/pg4/site/", None).await;
    assert_eq!(
        (status, body.as_slice()),
        (StatusCode::OK, b"v2".as_slice())
    );

    let builds = t.db.list_pages_builds(&repo.id, None, 10).await.unwrap();
    assert_eq!(builds.len(), 2);
    assert_eq!(builds[0].status, "built");
    assert_eq!(builds[0].commit_sha, c2.sha);

    let deliveries = t.db.list_webhook_deliveries("wh-pg", 50).await.unwrap();
    let pb = deliveries
        .iter()
        .find(|d| d.event == "page_build")
        .expect("page_build delivery");
    let payload: serde_json::Value = serde_json::from_str(&pb.payload_json).unwrap();
    assert_eq!(payload["build"]["status"], "built");
    assert_eq!(payload["repository"]["name"], "site");

    // The same completion fans out to `on: page_build` workflows.
    let runs =
        t.db.list_action_runs_for_repo(&repo.id, &Default::default(), 100, 0)
            .await
            .unwrap();
    assert!(
        runs.iter().any(|r| r.event == "page_build"),
        "page_build lifecycle run — {runs:?}"
    );

    // A push touching another branch does not rebuild.
    git.commit_files(
        &bare,
        "other",
        "main",
        "side",
        &[FileChange::Upsert {
            path: "index.html".into(),
            content: b"side".to_vec(),
        }],
        "T",
        "t@t",
        None,
        "admin",
    )
    .await
    .unwrap();
    oxidean_api::pages::rebuild_after_push(
        &t.db,
        &git,
        &t.repos_dir,
        &t.pages_dir,
        "development",
        &repo.id,
        "pg4",
        &user_id,
        &[(ZERO.to_string(), "f".repeat(40), "refs/heads/other".into())],
        false,
        true,
    )
    .await;
    assert_eq!(
        t.db.list_pages_builds(&repo.id, None, 10)
            .await
            .unwrap()
            .len(),
        2
    );

    // Deleting the source branch keeps the last built tree (GitHub parity).
    oxidean_api::pages::rebuild_after_push(
        &t.db,
        &git,
        &t.repos_dir,
        &t.pages_dir,
        "development",
        &repo.id,
        "pg4",
        &user_id,
        &[("e".repeat(40), ZERO.to_string(), "refs/heads/main".into())],
        false,
        true,
    )
    .await;
    assert_eq!(
        t.db.list_pages_builds(&repo.id, None, 10)
            .await
            .unwrap()
            .len(),
        2
    );
    let (status, _, body) = get_page(&t.app, "/pages/pg4/site/", None).await;
    assert_eq!(
        (status, body.as_slice()),
        (StatusCode::OK, b"v2".as_slice())
    );
}

// ---------------------------------------------------------------------------
// Workflow deploy seam
// ---------------------------------------------------------------------------

async fn enqueue_pages_job(
    db: &Database,
    repo_id: &str,
    actor_id: &str,
    environment: Option<&str>,
) -> (String, String) {
    let env_line = environment
        .map(|e| format!("    environment: {e}\n"))
        .unwrap_or_default();
    let yaml = format!(
        r#"name: P
on: [push]
jobs:
  deploy:
    runs-on: ubuntu-latest
{env}    permissions:
      contents: read
      pages: write
    steps:
      - run: echo deploy
"#,
        env = env_line
    );
    let doc = parse_workflow_yaml(yaml.as_bytes()).expect("parse");
    let (run_id, job_ids) = enqueue_run(
        db,
        repo_id,
        ".github/workflows/pages.yml",
        &doc,
        "push",
        "deadbeef",
        "refs/heads/main",
        Some(actor_id),
        None,
        None,
        false,
        None,
    )
    .await
    .expect("enqueue");
    (run_id, job_ids.into_iter().next().unwrap())
}

async fn mint_token(
    db: &Database,
    run_id: &str,
    job_id: &str,
    repo: &oxidean_db::RepositoryRow,
) -> String {
    let run = db.find_action_run_by_id(run_id).await.unwrap().unwrap();
    let owner_ref = oxidean_api::repo::owner_ref_for_repo(db, repo)
        .await
        .unwrap()
        .unwrap();
    let job = db.find_action_job_by_id(job_id).await.unwrap().unwrap();
    let p = db
        .find_action_payload(job.payload_sha.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&p.payload_json).unwrap();
    let requested = oxidean_api::actions::job_tokens::permissions_from_payload(&payload);
    oxidean_api::actions::job_tokens::mint_job_token(db, &run, repo, &owner_ref, job_id, &requested)
        .await
        .unwrap()
        .unwrap()
        .plaintext
        .expect("contents:read → plaintext")
}

fn seed_artifact(artifacts_dir: &std::path::Path, entries: &[(&str, &[u8])]) -> (String, String) {
    let rel = format!("art/{}/site.tar.gz", uuid::Uuid::new_v4());
    let abs = artifacts_dir.join(&rel);
    std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
    std::fs::write(&abs, tar_gz(entries)).unwrap();
    (rel, abs.to_string_lossy().to_string())
}

#[tokio::test]
async fn pages_workflow_deploy_job_token_seam() {
    let t = test_app().await;
    let (cookie, login_v) = signup_and_login(&t.app, "pg5@ex.com", "pg5").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    verify_user(&t.db, &user_id).await;
    create_repo(&t.app, &cookie, "site", "public").await;
    let repo =
        t.db.find_repository_by_owner_name(&user_id, "site")
            .await
            .unwrap()
            .unwrap();
    let (_, repo_id) = ((), repo.id.clone());

    // workflow-type site — `deploys.create` publishes into it.
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.enable","input":{"owner":"pg5","name":"site","build_type":"workflow"}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["build_type"], "workflow");

    let (run_id, job_id) = enqueue_pages_job(&t.db, &repo_id, &user_id, Some("github-pages")).await;
    let token = mint_token(&t.db, &run_id, &job_id, &repo).await;

    // Seed the `github-pages` artifact bound to THIS run+job.
    let (storage, _) = seed_artifact(
        &t.artifacts_dir,
        &[
            ("index.html", b"<h1>from actions</h1>".as_slice()),
            ("assets/app.js", b"console.log(1)".as_slice()),
            ("../escape.txt", b"evil".as_slice()),
            ("/absolute.txt", b"evil".as_slice()),
        ],
    );
    t.db.upsert_action_artifact(&oxidean_db::actions::ActionArtifactRow {
        id: uuid::Uuid::new_v4().to_string(),
        repository_id: repo_id.clone(),
        run_id: run_id.clone(),
        job_id: Some(job_id.clone()),
        name: "github-pages".into(),
        storage_path: storage,
        size_bytes: 1,
        digest_sha256: "d".into(),
        expires_at: None,
        state: "committed".into(),
        mime_type: "application/gzip".into(),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
    .await
    .expect("artifact");

    // Session callers can't publish — the seam is job-token only.
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.deploys.create","input":{"owner":"pg5","name":"site"}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["error"]["code"], "pages.forbidden", "{v}");

    // The job token publishes — traversal members are dropped, not fatal.
    let v = rpc_json(
        &t.app,
        rpc_req_bearer(
            r#"{"procedure":"pages.deploys.create","input":{"owner":"pg5","name":"site"}}"#,
            &token,
        ),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["status"], "built");
    assert_eq!(v["data"]["build_type"], "workflow");
    assert_eq!(v["data"]["run_id"], run_id);
    let (status, _, body) = get_page(&t.app, "/pages/pg5/site/", None).await;
    assert_eq!(
        (status, body.as_slice()),
        (StatusCode::OK, b"<h1>from actions</h1>".as_slice())
    );
    let (status, _, body) = get_page(&t.app, "/pages/pg5/site/assets/app.js", None).await;
    assert_eq!(
        (status, body.as_slice()),
        (StatusCode::OK, b"console.log(1)".as_slice())
    );
    // Traversal members didn't escape the site dir.
    assert!(!t.pages_dir.join("escape.txt").exists());
    assert!(!t.pages_dir.join("absolute.txt").exists());
    let site_root = t.pages_dir.join(&repo_id);
    assert!(!site_root.join("../escape.txt").exists());

    // A token for a job WITHOUT `environment: github-pages` is refused.
    let (run2, job2) = enqueue_pages_job(&t.db, &repo_id, &user_id, None).await;
    let token2 = mint_token(&t.db, &run2, &job2, &repo).await;
    let v = rpc_json(
        &t.app,
        rpc_req_bearer(
            r#"{"procedure":"pages.deploys.create","input":{"owner":"pg5","name":"site"}}"#,
            &token2,
        ),
    )
    .await;
    assert_eq!(v["error"]["code"], "pages.forbidden", "{v}");

    // Cross-repo: the repo-A token can't even resolve repo B — the PAT
    // repository pin fails at `resolve_repo_for_read` (GitHub 404 parity:
    // cross-repo GITHUB_TOKENs are indistinguishable from nonexistence).
    create_repo(&t.app, &cookie, "other", "public").await;
    let v = rpc_json(
        &t.app,
        rpc_req_bearer(
            r#"{"procedure":"pages.deploys.create","input":{"owner":"pg5","name":"other"}}"#,
            &token,
        ),
    )
    .await;
    assert_eq!(v["error"]["code"], "repo.not_found", "{v}");

    // builds.request on the workflow site → conflict.
    let v = rpc_json(
        &t.app,
        rpc_req_cookie(
            r#"{"procedure":"pages.builds.request","input":{"owner":"pg5","name":"site"}}"#,
            &cookie,
        ),
    )
    .await;
    assert_eq!(v["error"]["code"], "pages.conflict", "{v}");
}
