//! Phase 0 conformance harness — one corpus workflow per feature cell in
//! `tests/conformance/workflows/`, expectations in `tests/conformance/cases.json`.
//!
//! A scripted runner claims jobs over the real `/api/actions/*` HTTP protocol
//! (register → fetch_task → update_task → update_log), interprets `run:` steps
//! with `sh`, stubs `uses:` steps, then asserts run/job statuses and log
//! contents. Cases for features the scheduler can't execute yet are asserted
//! as `validation_failure` (the loud-failure contract) and flip to `run` as
//! later phases land.

mod support;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use oxidean_api::actions::dispatch_push_for_sha;
use oxidean_api::actions::mint_registration_token;
use oxidean_api::actions::secrets::encrypt_secret;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, GitBackend};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct CasesFile {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    kind: String, // run | validation_failure | queued | never_dispatch
    runner_labels: Option<Vec<String>>,
    secrets: Option<HashMap<String, String>>,
    /// Repo-level configuration variables (`vars` context) seeded on the
    /// fixture repo.
    vars: Option<HashMap<String, String>>,
    head_ref: Option<String>,
    /// Push the same workflow twice — exercises concurrency supersede.
    second_dispatch: Option<bool>,
    expect_run: Option<String>,
    /// `run-name:` evaluates to the run's display title at enqueue.
    expect_title: Option<String>,
    expect_jobs: Option<HashMap<String, String>>,
    expect_job_names: Option<HashMap<String, String>>,
    /// Per-job_key status multiset over all legs: {"build": ["success","success"]}.
    expect_job_statuses: Option<HashMap<String, Vec<String>>>,
    /// Status multiset over every run in the repo: ["cancelled","success"].
    expect_runs: Option<Vec<String>>,
    /// Event-name multiset over every run: ["push","workflow_run"].
    expect_events: Option<Vec<String>>,
    /// Assert `expect_jobs`/`log_contains` against the run with this `event`
    /// instead of the newest non-cancelled run — needed when fan-out leaves
    /// same-second `created_at` ties whose `id DESC` break is arbitrary.
    pick_event: Option<String>,
    expect_unsupported: Option<Vec<String>>,
    /// Extra files seeded into the main repo's commit (local actions etc.).
    files: Option<HashMap<String, String>>,
    /// Extra repos seeded under the same owner for remote `uses:` resolution
    /// — `{name: {path: content}}` becomes `{slug}/{name}.git` on the origin.
    action_repos: Option<HashMap<String, HashMap<String, String>>>,
    /// Mint `server_url`/`ACTIONS_ID_TOKEN_REQUEST_URL` against the ephemeral
    /// listener (`OXIDEAN_PUBLIC_ORIGIN`) — only set for cases that actually
    /// call the API from inside a step, so a stale env never leaks into
    /// sibling cases sharing the process under plain `cargo test`.
    needs_public_origin: Option<bool>,
    log_contains: Option<Vec<String>>,
    log_absent: Option<Vec<String>>,
    /// Compatibility-doc row this case pins — set when the case exists to
    /// document a `partial`/`unsupported` divergence. Validated by the
    /// `actions_compat_doc` drift gate, not by the harness itself.
    #[allow(dead_code)]
    divergence: Option<String>,
    /// Seed-commit message override — skip-marker cases need `[skip ci]` in
    /// the head commit message.
    commit_message: Option<String>,
}

fn base_case(id: &str) -> Case {
    Case {
        id: id.into(),
        kind: "run".into(),
        runner_labels: None,
        secrets: None,
        vars: None,
        head_ref: None,
        second_dispatch: None,
        expect_run: None,
        expect_title: None,
        expect_jobs: None,
        expect_job_names: None,
        expect_job_statuses: None,
        expect_runs: None,
        expect_events: None,
        pick_event: None,
        expect_unsupported: None,
        files: None,
        action_repos: None,
        needs_public_origin: None,
        log_contains: None,
        log_absent: None,
        divergence: None,
        commit_message: None,
    }
}

/// Runtime `CARGO_MANIFEST_DIR` — nextest sets it under `--workspace-remap`
/// where the build-time `env!` still points at the (absent) image path.
fn manifest_dir() -> PathBuf {
    match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    }
}

fn cases() -> Vec<Case> {
    let path = manifest_dir().join("tests/conformance/cases.json");
    let raw = std::fs::read_to_string(&path).expect("read cases.json");
    let file: CasesFile = serde_json::from_str(&raw).expect("parse cases.json");
    file.cases
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Docker daemon reachable? `docker_gated` cases assert dispatch only when
/// absent (CI hosts without a daemon still cover parse/queue semantics).
fn docker_present() -> bool {
    std::process::Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn workflow_path(id: &str) -> PathBuf {
    manifest_dir()
        .join("tests/conformance/workflows")
        .join(format!("{id}.yml"))
}

static INIT_ONCE: std::sync::Once = std::sync::Once::new();
static SHARED_LOG_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
static SHARED_REPOS_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Process-wide env the API resolves at call time: the secret crypto key,
/// the actions log dir, and the bare-repos root. Log/repos dirs are shared
/// (runs/jobs/repos are slug/UUID-namespaced) because `enqueue_run` and the
/// scheduler resolve them from env, not `AppState` — per-case tempdirs would
/// race on the process-global `set_var`.
fn ensure_env() -> &'static PathBuf {
    INIT_ONCE.call_once(|| {
        // Runner-side warnings (dropped log chunks, command diagnostics) go
        // through `tracing` — install a subscriber so failures surface in
        // test output instead of vanishing.
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
            )
            .try_init();
        std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "conformance-test-key");
        let dir = std::env::temp_dir().join(format!("oxidean-conf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("OXIDEAN_ACTIONS_LOG_DIR", &dir);
        let repos = dir.join("repos");
        std::fs::create_dir_all(&repos).unwrap();
        std::env::set_var("OXIDEAN_REPOS_DIR", &repos);
        SHARED_LOG_DIR.set(dir).unwrap();
        SHARED_REPOS_DIR.set(repos).unwrap();
    });
    SHARED_LOG_DIR.get().unwrap()
}

struct Fixture {
    app: axum::Router,
    db: Database,
    dir: tempfile::TempDir,
    log_dir: PathBuf,
    repo_id: String,
    bare: PathBuf,
    head_sha: String,
    owner_id: String,
    /// Runner `git_token` — a user PAT minted when the case seeds remote
    /// `uses:` action repos (job tokens are repo-locked).
    git_token: Option<String>,
    head_ref: String,
    n_dispatched: usize,
    needs_public_origin: bool,
}

async fn fixture(case: &Case) -> Fixture {
    let log_dir = ensure_env().clone();
    let dir = tempfile::tempdir().unwrap();
    let repos_dir = SHARED_REPOS_DIR.get().unwrap().clone();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("c.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_actions_enabled(true)
    .with_actions_log_dir(log_dir.clone())
    .with_actions_artifacts_dir(dir.path().join("artifacts"))
    .with_repos_dir(repos_dir.clone());
    let app = router_with_state(state, build_cors("development", None).unwrap());

    // Shared repos root is slug-namespaced — widen the slice so distinct
    // case ids never collide on the same `{slug}/{slug}.git` path
    // (`reusable-workflow-local`/`reusable-workflow-remote` share 16 chars),
    // plus a per-fixture counter so tests seeding the same case id (rerun /
    // repo-deletion flows) never push into the same bare repo concurrently.
    static FIXTURE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = FIXTURE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let slug = format!("cf-{}-{seq}", &case.id[..case.id.len().min(24)]);
    let owner = db
        .create_user(
            &format!("u-{slug}"),
            &format!("{slug}@example.com"),
            &slug,
            Some("h"),
            &slug,
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository(
            &format!("r-{slug}"),
            &owner.id,
            "user",
            &slug,
            "private",
            "",
            "main",
        )
        .await
        .unwrap();
    for (name, value) in case.secrets.clone().unwrap_or_default() {
        db.insert_action_secret(
            // CHAR(36) on MySQL — a bare UUID fits; `s-<uuid>` does not.
            &Uuid::new_v4().to_string(),
            &repo.id,
            &name,
            &encrypt_secret(&value).unwrap(),
        )
        .await
        .unwrap();
    }
    for (name, value) in case.vars.clone().unwrap_or_default() {
        db.upsert_action_variable(&Uuid::new_v4().to_string(), &repo.id, &name, &value)
            .await
            .unwrap();
    }

    // fetch_task re-reads the workflow via {repos_dir}/{owner}/{repo}.git.
    let bare = repos_dir.join(&slug).join(format!("{slug}.git"));
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let git = CliGitBackend::new();
    git.init_bare(&bare, "main").await.unwrap();
    let workflow = String::from_utf8(
        std::fs::read(workflow_path(&case.id))
            .unwrap_or_else(|e| panic!("read corpus workflow for {}: {e}", case.id)),
    )
    .unwrap()
    .replace("{{SLUG}}", &slug);
    let mut seed_files: Vec<(String, Vec<u8>)> = vec![(
        ".github/workflows/conformance.yml".into(),
        workflow.into_bytes(),
    )];
    for (p, c) in case.files.clone().unwrap_or_default() {
        seed_files.push((p, c.replace("{{SLUG}}", &slug).into_bytes()));
    }
    git.seed_commit(
        &bare,
        "main",
        case.commit_message.as_deref().unwrap_or("conformance seed"),
        &seed_files,
    )
    .await
    .unwrap();
    // Remote `uses:` repos — `{slug}/{name}.git` on the fixture origin, plus a
    // repository row so smart-HTTP resolves them. The runner's `git_token` is
    // a user PAT: job tokens are repo-locked and can't clone a sibling repo.
    let mut git_token = None;
    let has_action_repos = case.action_repos.is_some();
    for (name, files) in case.action_repos.clone().unwrap_or_default() {
        let abare = repos_dir.join(&slug).join(format!("{name}.git"));
        git.init_bare(&abare, "main").await.unwrap();
        let afiles: Vec<(String, Vec<u8>)> = files
            .into_iter()
            .map(|(p, c)| (p, c.replace("{{SLUG}}", &slug).into_bytes()))
            .collect();
        git.seed_commit(&abare, "main", "action seed", &afiles)
            .await
            .unwrap();
        db.insert_repository(
            // CHAR(36) on MySQL — `r-{slug}-{name}` overflows for longer
            // case slugs; a bare uuid fits everywhere.
            &Uuid::new_v4().to_string(),
            &owner.id,
            "user",
            &name,
            "private",
            "",
            "main",
        )
        .await
        .unwrap();
    }
    if has_action_repos {
        let token = format!("oxidean_pat_{}{slug}", "x".repeat(64));
        let prefix = &token[..oxidean_core::CLASSIC_PAT_PREFIX.len() + 8];
        db.create_pat(
            &format!("pat-{slug}"),
            &owner.id,
            "classic",
            "conformance",
            prefix,
            &sha256_hex(token.as_bytes()),
            Some(r#"["repo"]"#),
            None,
            None,
            None,
            None,
            &[],
        )
        .await
        .unwrap();
        git_token = Some(token);
    }
    let head_sha = git
        .list_refs(&bare)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.name.ends_with("main"))
        .unwrap()
        .oid;

    let n_dispatched = dispatch_push_for_sha(
        &db,
        &git as &dyn GitBackend,
        &bare,
        &repo.id,
        &head_sha,
        case.head_ref.as_deref().unwrap_or("refs/heads/main"),
        Some(&owner.id),
        true,
    )
    .await
    .unwrap();
    if case.second_dispatch.unwrap_or(false) {
        // A second push lands while the first run's jobs are still
        // nonterminal — concurrency `cancel-in-progress` supersedes it.
        let n2 = dispatch_push_for_sha(
            &db,
            &git as &dyn GitBackend,
            &bare,
            &repo.id,
            &head_sha,
            case.head_ref.as_deref().unwrap_or("refs/heads/main"),
            Some(&owner.id),
            true,
        )
        .await
        .unwrap();
        assert_eq!(n2, 1, "{}: second dispatch must enqueue", case.id);
    }

    Fixture {
        app,
        db,
        dir,
        log_dir,
        repo_id: repo.id,
        bare,
        head_sha,
        owner_id: owner.id,
        git_token,
        head_ref: case
            .head_ref
            .clone()
            .unwrap_or_else(|| "refs/heads/main".into()),
        n_dispatched,
        needs_public_origin: case.needs_public_origin.unwrap_or(false),
    }
}

/// Real runner: serve the app on an ephemeral TCP port and drive
/// `oxidean_runner::exec::execute` over the actual `/api/actions/*` HTTP
/// protocol — checkout performs a real `git clone` through the server's git
/// smart-HTTP, file commands and expression evaluation run in the real
/// runner engine (no mirrored interpreter to drift).
/// Spin up the real API server + a registered runner; returns the client,
/// config, and server handle. The claim/execute loop is separate so tests
/// can run it in the background while they mutate state underneath.
async fn start_runner(
    fx: &Fixture,
    labels: &[String],
    name: &str,
) -> (
    oxidean_runner::client::Client,
    oxidean_runner::config::Config,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = fx.app.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let origin = format!("http://127.0.0.1:{port}");
    // `resolve_public_origin()` is env-driven — point it at this ephemeral
    // listener so minted `server_url`/`ACTIONS_ID_TOKEN_REQUEST_URL` reach the
    // server-under-test. Only cases that call the API from inside a step opt
    // in — a process-global env write could otherwise cross-wire a sibling
    // case's port under plain `cargo test` (nextest runs each in-process).
    if fx.needs_public_origin {
        std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", &origin);
    }

    let mut client = oxidean_runner::client::Client::new(&origin, None);
    let reg = mint_registration_token(&fx.db).await.unwrap();
    let (_, token) = client
        .register(name, labels, &reg)
        .await
        .expect("runner register");
    client.set_token(token);
    client.declare(labels).await.expect("runner declare");

    let config = oxidean_runner::config::Config {
        origin: origin.clone(),
        registration_token: None,
        name: name.into(),
        labels: oxidean_runner::config::parse_labels(&labels.join(",")).unwrap(),
        state_path: fx.dir.path().join(format!("runner-state-{name}.json")),
        work_dir: fx.dir.path().join(format!("runner-work-{name}")),
        poll_ms: 1,
        wait_ms: 0,
        git_token: fx.git_token.clone(),
        job_timeout_secs: 300,
        actions_mirror: None,
    };
    (client, config, server)
}

async fn claim_loop(
    client: &oxidean_runner::client::Client,
    config: &oxidean_runner::config::Config,
) {
    // Generous bound — the loop exits on `None` once the queue drains; the
    // cap only guards a wedge. Reusable-call fan-out tests claim >64 legs.
    for _ in 0..512 {
        match client.fetch_task(0).await.expect("fetch_task") {
            Some(task) => {
                oxidean_runner::exec::execute(client, &task, config).await;
            }
            None => break,
        }
    }
}

async fn drive_runner(fx: &Fixture, labels: &[String]) {
    let (client, config, server) = start_runner(fx, labels, "conformance").await;
    claim_loop(&client, &config).await;
    server.abort();
}

fn read_job_log(fx: &Fixture, run_id: &str, job_id: &str) -> String {
    let p = fx.log_dir.join(run_id).join(format!("{job_id}.log"));
    std::fs::read_to_string(p).unwrap_or_default()
}

async fn all_logs(fx: &Fixture, run_id: &str) -> String {
    let jobs = fx.db.list_action_jobs_for_run(run_id).await.unwrap();
    let mut out = String::new();
    for j in jobs {
        out.push_str(&read_job_log(fx, run_id, &j.id));
    }
    out
}

fn assert_logs(case: &Case, logs: &str) {
    for needle in case.log_contains.clone().unwrap_or_default() {
        assert!(
            logs.contains(&needle),
            "{}: job logs missing '{needle}'\n--- logs ---\n{logs}",
            case.id
        );
    }
    for needle in case.log_absent.clone().unwrap_or_default() {
        assert!(
            !logs.contains(&needle),
            "{}: job logs must not contain '{needle}'\n--- logs ---\n{logs}",
            case.id
        );
    }
}

/// Advisory diff vs the real-GitHub oracle (`scripts/conformance-oracle.sh`).
/// Opt-in via OXIDEAN_CONFORMANCE_GOLDEN=1 — prints mismatches, never fails.
fn golden_diff(case: &Case, our_run: &str, our_jobs: &HashMap<String, String>) {
    if std::env::var_os("OXIDEAN_CONFORMANCE_GOLDEN").is_none() {
        return;
    }
    let path = manifest_dir().join("tests/conformance/golden.json");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        eprintln!("golden: {path:?} missing — run scripts/conformance-oracle.sh");
        return;
    };
    let golden: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let Some(entry) = golden["cases"].get(&case.id) else {
        return;
    };
    let gh_run = entry["run"].as_str().unwrap_or_default();
    if !gh_run.is_empty() && gh_run != our_run {
        eprintln!(
            "golden-diff {}: run github={gh_run} ours={our_run}",
            case.id
        );
    }
    for j in entry["jobs"].as_array().into_iter().flatten() {
        let key = j["key"].as_str().unwrap_or_default();
        let gh = j["conclusion"].as_str().unwrap_or_default();
        if let Some(ours) = our_jobs.get(key) {
            if !gh.is_empty() && gh != ours {
                eprintln!("golden-diff {} job {key}: github={gh} ours={ours}", case.id);
            }
        }
    }
}

async fn run_case(case: &Case) {
    let fx = fixture(case).await;
    let runs = fx
        .db
        .list_action_runs_for_repo(&fx.repo_id, &Default::default(), 10, 0)
        .await
        .unwrap();

    match case.kind.as_str() {
        "never_dispatch" => {
            assert_eq!(
                fx.n_dispatched, 0,
                "{}: expected dispatch to enqueue nothing",
                case.id
            );
            assert!(
                runs.is_empty(),
                "{}: expected no dispatched runs, found {}",
                case.id,
                runs.len()
            );
        }
        "validation_failure" => {
            // Still dispatched — the run exists and fails visibly.
            assert_eq!(fx.n_dispatched, 1, "{}: expected one enqueue", case.id);
            assert_eq!(runs.len(), 1, "{}: expected one run", case.id);
            let run = &runs[0];
            assert_eq!(
                run.status, "failure",
                "{}: validation run must end failed",
                case.id
            );
            let jobs = fx.db.list_action_jobs_for_run(&run.id).await.unwrap();
            assert_eq!(jobs.len(), 1, "{}: validation run has one job", case.id);
            let job = &jobs[0];
            assert_eq!(job.job_key, "__validation");
            assert_eq!(job.status, "failure");
            let log = read_job_log(&fx, &run.id, &job.id);
            for key in case.expect_unsupported.clone().unwrap_or_default() {
                assert!(
                    job.name.contains(&key),
                    "{}: validation job name must name '{key}': {}",
                    case.id,
                    job.name
                );
                assert!(
                    log.contains(&key),
                    "{}: validation log must name '{key}'",
                    case.id
                );
            }
        }
        "queued" | "run" | "docker_gated" => {
            assert_eq!(fx.n_dispatched, 1, "{}: expected one enqueue", case.id);
            // `container:`/`services:` jobs execute against a Docker daemon;
            // without one we still assert parse+dispatch (jobs must queue,
            // not bounce to the __validation job).
            if case.kind == "docker_gated" && !docker_present() {
                let jobs = fx.db.list_action_jobs_for_run(&runs[0].id).await.unwrap();
                assert!(
                    !jobs.is_empty() && jobs.iter().all(|j| j.job_key != "__validation"),
                    "{}: container/services jobs must dispatch (docker absent)",
                    case.id
                );
                return;
            }
            let labels = case
                .runner_labels
                .clone()
                .unwrap_or_else(|| vec!["ubuntu-latest".into()]);
            drive_runner(&fx, &labels).await;
            let runs = fx
                .db
                .list_action_runs_for_repo(&fx.repo_id, &Default::default(), 10, 0)
                .await
                .unwrap();
            // The "current" run — the one the assertions target. A
            // superseded run is flagged `concurrency_cancel_pending`.
            let run = runs
                .iter()
                .find(|r| {
                    !r.concurrency_cancel_pending
                        && case.pick_event.as_deref().is_none_or(|e| r.event == e)
                })
                .unwrap_or(&runs[0]);
            if let Some(want) = case.expect_runs.clone() {
                let mut got: Vec<String> = runs.iter().map(|r| r.status.clone()).collect();
                let mut want = want;
                got.sort();
                want.sort();
                assert_eq!(got, want, "{}: run status multiset", case.id);
            }
            if let Some(want) = case.expect_events.clone() {
                let mut got: Vec<String> = runs.iter().map(|r| r.event.clone()).collect();
                let mut want = want;
                got.sort();
                want.sort();
                assert_eq!(got, want, "{}: run event multiset", case.id);
            }
            let jobs = fx.db.list_action_jobs_for_run(&run.id).await.unwrap();
            for (key, want) in case.expect_jobs.clone().unwrap_or_default() {
                let job = jobs
                    .iter()
                    .find(|j| j.job_key == key.as_str())
                    .unwrap_or_else(|| panic!("{}: job '{key}' missing", case.id));
                assert_eq!(
                    job.status,
                    want,
                    "{}: job '{key}' status (log: {})",
                    case.id,
                    read_job_log(&fx, &run.id, &job.id)
                );
            }
            for (key, want) in case.expect_job_statuses.clone().unwrap_or_default() {
                let mut got: Vec<String> = jobs
                    .iter()
                    .filter(|j| j.job_key == key.as_str())
                    .map(|j| j.status.clone())
                    .collect();
                let mut want = want;
                got.sort();
                want.sort();
                assert_eq!(
                    got,
                    want,
                    "{}: job '{key}' leg statuses (log: {})",
                    case.id,
                    all_logs(&fx, &run.id).await
                );
            }
            for (key, want) in case.expect_job_names.clone().unwrap_or_default() {
                let job = jobs.iter().find(|j| j.job_key == key.as_str()).unwrap();
                assert_eq!(job.name, want, "{}: job '{key}' name", case.id);
            }
            if case.kind == "run" || case.kind == "docker_gated" {
                let run = fx.db.find_action_run_by_id(&run.id).await.unwrap().unwrap();
                if let Some(want) = &case.expect_run {
                    assert_eq!(run.status, *want, "{}: run status", case.id);
                }
                if let Some(want) = &case.expect_title {
                    assert_eq!(run.title, *want, "{}: run title", case.id);
                }
                golden_diff(
                    case,
                    &run.status,
                    &jobs
                        .iter()
                        .map(|j| (j.job_key.clone(), j.status.clone()))
                        .collect(),
                );
                assert_logs(case, &all_logs(&fx, &run.id).await);
            }
        }
        other => panic!("{}: unknown case kind '{other}'", case.id),
    }
}

macro_rules! conformance_case {
    ($name:ident, $id:literal) => {
        #[tokio::test]
        async fn $name() {
            let all = cases();
            let case = all
                .iter()
                .find(|c| c.id == $id)
                .expect("case in cases.json");
            run_case(case).await;
        }
    };
}

// ── run-kind ───────────────────────────────────────────────────────────
conformance_case!(run_basic, "run-basic");
conformance_case!(run_fail, "run-fail");
conformance_case!(run_multi_step, "run-multi-step");
conformance_case!(run_env_step, "run-env-step");
conformance_case!(run_env_multiline, "run-env-multiline");
conformance_case!(run_workdir, "run-workdir");
conformance_case!(run_workdir_missing, "run-workdir-missing");
conformance_case!(run_shell_sh, "run-shell-sh");
conformance_case!(run_defaults_shell, "run-defaults-shell");
conformance_case!(run_defaults_workdir, "run-defaults-workdir");
conformance_case!(checkout_basic, "checkout-basic");
conformance_case!(checkout_with_inputs, "checkout-with-inputs");
conformance_case!(uses_unknown, "uses-unknown");
conformance_case!(multi_job_parallel, "multi-job-parallel");
conformance_case!(job_name_override, "job-name-override");
conformance_case!(exit_code_nonzero, "exit-code-nonzero");
conformance_case!(echo_unicode, "echo-unicode");
conformance_case!(runs_on_array, "runs-on-array");
conformance_case!(secrets_mask, "secrets-mask");
conformance_case!(secrets_env, "secrets-env");
conformance_case!(expr_in_run, "expr-in-run");
conformance_case!(workflow_env, "workflow-env");
conformance_case!(job_env, "job-env");
conformance_case!(job_if_skip, "job-if-skip");
conformance_case!(expr_functions, "expr-functions");
conformance_case!(expr_env_layering, "expr-env-layering");
conformance_case!(expr_steps_context, "expr-steps-context");
conformance_case!(expr_hashfiles, "expr-hashfiles");
conformance_case!(expr_job_name, "expr-job-name");
conformance_case!(runs_on_expr, "runs-on-expr");
conformance_case!(expr_github_ctx, "expr-github-ctx");
conformance_case!(job_timeout_kills, "job-timeout-kills");
conformance_case!(checkout_path, "checkout-path");
conformance_case!(secrets_context, "secrets-context");
conformance_case!(step_failure_flow, "step-failure-flow");
conformance_case!(job_if, "job-if");
conformance_case!(step_if, "step-if");
conformance_case!(concurrency_workflow, "concurrency-workflow");
conformance_case!(concurrency_job, "concurrency-job");
conformance_case!(job_outputs, "job-outputs");
conformance_case!(timeout_minutes_job, "timeout-minutes-job");
conformance_case!(timeout_minutes_step, "timeout-minutes-step");
conformance_case!(push_tag_ref, "push-tag-ref");
conformance_case!(trigger_map_form, "trigger-map-form");
conformance_case!(trigger_list_form, "trigger-list-form");
// Phase 3 — scheduler semantics.
conformance_case!(needs_basic, "needs-basic");
conformance_case!(needs_diamond, "needs-diamond");
conformance_case!(needs_fail_default, "needs-fail-default");
conformance_case!(needs_always, "needs-always");
conformance_case!(needs_failure_fn, "needs-failure-fn");
conformance_case!(needs_success_fn, "needs-success-fn");
conformance_case!(needs_outputs, "needs-outputs");
conformance_case!(needs_ctx_if, "needs-ctx-if");
conformance_case!(needs_skipped_dep, "needs-skipped-dep");
conformance_case!(needs_matrix_dep, "needs-matrix-dep");
conformance_case!(strategy_matrix, "strategy-matrix");
conformance_case!(matrix_basic, "matrix-basic");
conformance_case!(strategy_context, "strategy-context");
conformance_case!(matrix_include_exclude, "matrix-include-exclude");
conformance_case!(matrix_max_parallel, "matrix-max-parallel");
conformance_case!(matrix_fail_fast, "matrix-fail-fast");
conformance_case!(matrix_fail_fast_off, "matrix-fail-fast-off");
conformance_case!(concurrency_cancel, "concurrency-cancel");
conformance_case!(concurrency_job_cancel, "concurrency-job-cancel");
conformance_case!(concurrency_pending, "concurrency-pending");
conformance_case!(concurrency_job_pending, "concurrency-job-pending");

// ── queued / never_dispatch ───────────────────────────────────────────
conformance_case!(runs_on_nomatch, "runs-on-nomatch");
conformance_case!(dispatch_only, "dispatch-only");
conformance_case!(pr_only, "pr-only");
conformance_case!(prt_only, "prt-only");
conformance_case!(runs_on_missing, "runs-on-missing");

// ── validation_failure (loud-failure contract) ────────────────────────
conformance_case!(needs_cycle, "needs-cycle");
conformance_case!(container_job, "container-job");
conformance_case!(services_job, "services-job");
conformance_case!(docker_uses_args, "docker-uses-args");
conformance_case!(environment_job, "environment-job");
conformance_case!(reusable_workflow_local, "reusable-workflow-local");
conformance_case!(reusable_workflow_remote, "reusable-workflow-remote");
conformance_case!(reusable_nested, "reusable-nested");
conformance_case!(reusable_matrix_call, "reusable-matrix-call");
conformance_case!(reusable_secrets_map, "reusable-secrets-map");
conformance_case!(reusable_secrets_none, "reusable-secrets-none");
conformance_case!(reusable_missing_input, "reusable-missing-input");
conformance_case!(reusable_needs_scope, "reusable-needs-scope");
conformance_case!(reusable_no_trigger, "reusable-no-trigger");
conformance_case!(reusable_caller_keywords, "reusable-caller-keywords");
conformance_case!(reusable_recursion, "reusable-recursion");
conformance_case!(reusable_fanout, "reusable-fanout");
conformance_case!(reusable_fanout_accumulated, "reusable-fanout-accumulated");
conformance_case!(reusable_undeclared_input, "reusable-undeclared-input");
conformance_case!(reusable_missing_secret, "reusable-missing-secret");
conformance_case!(reusable_inherit_secret, "reusable-inherit-secret");
conformance_case!(reusable_type_mismatch, "reusable-type-mismatch");
conformance_case!(reusable_depth, "reusable-depth");
conformance_case!(reusable_coe_callee, "reusable-coe-callee");
conformance_case!(continue_on_error_job, "continue-on-error-job");
conformance_case!(continue_on_error_matrix, "continue-on-error-matrix");
conformance_case!(continue_on_error_step, "continue-on-error-step");
// Phase 4 — step lifecycle, shells, action runtimes.
conformance_case!(shell_python, "shell-python");
conformance_case!(shell_custom, "shell-custom");
conformance_case!(shell_pwsh, "shell-pwsh");
conformance_case!(uses_local_composite, "uses-local-composite");
conformance_case!(uses_remote_composite, "uses-remote-composite");
conformance_case!(uses_node_action, "uses-node-action");
conformance_case!(uses_node_post, "uses-node-post");
conformance_case!(composite_env, "composite-env");
conformance_case!(step_summary, "step-summary");
conformance_case!(artifact_upload_download, "artifact-upload-download");
conformance_case!(artifact_search_path_root, "artifact-search-path-root");
conformance_case!(artifact_overwrite, "artifact-overwrite");
conformance_case!(artifact_pattern_download, "artifact-pattern-download");
conformance_case!(artifact_compression_level, "artifact-compression-level");
conformance_case!(artifact_missing, "artifact-missing");
conformance_case!(artifact_no_files_error, "artifact-no-files-error");
conformance_case!(expr_malformed_run, "expr-malformed-run");
conformance_case!(expr_malformed_if, "expr-malformed-if");
conformance_case!(prt_push, "prt-push");
conformance_case!(empty_steps, "empty-steps");
conformance_case!(oidc_curl, "oidc-curl");
// P11a — corner pins.
conformance_case!(push_skip_ci, "push-skip-ci");
conformance_case!(on_unsupported_event, "on-unsupported-event");
conformance_case!(matrix_over_256, "matrix-over-256");
conformance_case!(uses_node_pre_if, "uses-node-pre-if");
conformance_case!(action_metadata_warnings, "action-metadata-warnings");
conformance_case!(step_conclusion_coe, "step-conclusion-coe");
conformance_case!(step_if_false_literal, "step-if-false-literal");
conformance_case!(expr_vars_empty, "expr-vars-empty");
conformance_case!(vars_context, "vars-context");
conformance_case!(run_name, "run-name");
conformance_case!(workflow_run_listen, "workflow-run-listen");
conformance_case!(workflow_run_stem, "workflow-run-stem");
conformance_case!(workflow_run_branch_filter, "workflow-run-branch-filter");
conformance_case!(ctx_github_extra, "ctx-github-extra");
conformance_case!(cache_service_absent, "cache-service-absent");

/// Every `cases.json` entry is wired to a generated test and every corpus
/// workflow is referenced — guards the corpus against silent drift.
#[test]
fn corpus_is_complete_and_wired() {
    let all = cases();
    assert!(all.len() >= 40, "corpus must enumerate >=40 cases");
    let src = include_str!("workflow_conformance.rs");
    for case in &all {
        assert!(
            src.contains(&format!("\"{}\"", case.id)),
            "case '{}' is not instantiated in the harness",
            case.id
        );
        assert!(
            workflow_path(&case.id).exists(),
            "case '{}' has no workflow file",
            case.id
        );
    }
}

/// A `container:` job claimed on a daemon-less runner fails loudly — never
/// silently executes on the host (GitHub would fail the job the same way).
#[tokio::test]
async fn container_job_fails_loudly_without_docker() {
    if docker_present() {
        eprintln!("docker present — skipping no-docker failure coverage");
        return;
    }
    let case = cases()
        .into_iter()
        .find(|c| c.id == "container-job")
        .expect("container-job case");
    let fx = fixture(&case).await;
    drive_runner(&fx, &["ubuntu-latest".into()]).await;
    let runs = fx
        .db
        .list_action_runs_for_repo(&fx.repo_id, &Default::default(), 10, 0)
        .await
        .unwrap();
    let jobs = fx.db.list_action_jobs_for_run(&runs[0].id).await.unwrap();
    let job = jobs.iter().find(|j| j.job_key == "build").unwrap();
    assert_eq!(job.status, "failure");
    let log = read_job_log(&fx, &runs[0].id, &job.id);
    assert!(
        log.contains("docker is not available"),
        "log must explain the failure:\n{log}"
    );
}

/// 0053: fetch_task serves steps from the durable payload — the bare repo is
/// deleted after dispatch, so a git re-read would hand the runner nothing.
#[tokio::test]
async fn fetch_task_survives_repo_deletion() {
    let case = cases()
        .into_iter()
        .find(|c| c.id == "run-basic")
        .expect("run-basic case");
    let fx = fixture(&case).await;
    assert_eq!(fx.n_dispatched, 1);
    std::fs::remove_dir_all(&fx.bare).unwrap();
    drive_runner(&fx, &["ubuntu-latest".into()]).await;
    let runs = fx
        .db
        .list_action_runs_for_repo(&fx.repo_id, &Default::default(), 10, 0)
        .await
        .unwrap();
    let run = &runs[0];
    assert_eq!(run.status, "success", "run must succeed from payload alone");
    let jobs = fx.db.list_action_jobs_for_run(&run.id).await.unwrap();
    let job = jobs.iter().find(|j| j.job_key == "build").unwrap();
    let log = read_job_log(&fx, &run.id, &job.id);
    assert!(log.contains("hello conformance"), "log:\n{log}");
}

/// Mid-flight cancellation: run 2 supersedes run 1's *in-progress* job; the
/// runner learns of it through the update_log ack, aborts its remaining
/// steps, and its terminal report cannot overwrite the cancelled status.
#[tokio::test]
async fn running_job_aborts_on_supersede() {
    let case = base_case("concurrency-cancel-running");
    let fx = fixture(&case).await;
    let (client, config, server) = start_runner(&fx, &["ubuntu-latest".into()], "r1").await;
    let runner = tokio::spawn(async move { claim_loop(&client, &config).await });

    let run1_id = {
        let runs = fx
            .db
            .list_action_runs_for_repo(&fx.repo_id, &Default::default(), 10, 0)
            .await
            .unwrap();
        runs[0].id.clone()
    };
    // Wait until run 1's job is actually mid-flight. `in_progress` alone is
    // set at claim — on slower engines the supersede commit can land before
    // the runner's `update_task(in_progress)`, which then aborts before any
    // step (empty log, no "skipped" markers). Requiring a step boundary line
    // in the log proves the step loop is running when the cancel arrives.
    for _ in 0..200 {
        let jobs = fx.db.list_action_jobs_for_run(&run1_id).await.unwrap();
        if let Some(j) = jobs.first() {
            if j.status == "in_progress" && read_job_log(&fx, &run1_id, &j.id).contains("##[step]")
            {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let jobs1 = fx.db.list_action_jobs_for_run(&run1_id).await.unwrap();
    assert!(
        jobs1.iter().any(|j| j.status == "in_progress"),
        "run1 job never reached in_progress: {:?}",
        jobs1.iter().map(|j| &j.status).collect::<Vec<_>>()
    );

    // Second push supersedes the running job — cancel-in-progress: true.
    let n = dispatch_push_for_sha(
        &fx.db,
        &CliGitBackend::new(),
        &fx.bare,
        &fx.repo_id,
        &fx.head_sha,
        &fx.head_ref,
        Some(&fx.owner_id),
        true,
    )
    .await
    .unwrap();
    assert_eq!(n, 1);

    runner.await.unwrap();
    server.abort();

    let runs = fx
        .db
        .list_action_runs_for_repo(&fx.repo_id, &Default::default(), 10, 0)
        .await
        .unwrap();
    assert_eq!(runs.len(), 2);
    // List order isn't stable for same-second inserts — identify run 2 by id.
    let run2_id = runs
        .iter()
        .find(|r| r.id != run1_id)
        .expect("second run")
        .id
        .clone();
    let run1 = runs.iter().find(|r| r.id == run1_id).unwrap();
    let run2 = runs.iter().find(|r| r.id == run2_id).unwrap();
    assert_eq!(run1.status, "cancelled", "run1 must be cancelled");
    assert_eq!(run2.status, "success", "run2 must succeed");

    let jobs1 = fx.db.list_action_jobs_for_run(&run1_id).await.unwrap();
    assert_eq!(jobs1[0].status, "cancelled", "run1 job stays cancelled");
    let log1 = read_job_log(&fx, &run1_id, &jobs1[0].id);
    assert!(
        log1.contains("skipped — job cancelled"),
        "runner must abort remaining steps; log:\n{log1}"
    );
    assert!(
        !log1.contains("s4-final"),
        "last step must never run; log:\n{log1}"
    );

    let jobs2 = fx.db.list_action_jobs_for_run(&run2_id).await.unwrap();
    assert_eq!(jobs2[0].status, "success");
    let log2 = read_job_log(&fx, &run2_id, &jobs2[0].id);
    assert!(
        log2.contains("s4-final"),
        "run2 must complete; log:\n{log2}"
    );
}

/// Rerun: a finished run requeues — jobs with needs go back to `waiting`
/// and re-gate, a need skipped at enqueue stays skipped, and the run's
/// attempt counter bumps.
#[tokio::test]
async fn rerun_resets_and_regates() {
    let case = cases()
        .into_iter()
        .find(|c| c.id == "needs-skipped-dep")
        .expect("needs-skipped-dep case");
    let fx = fixture(&case).await;
    drive_runner(&fx, &["ubuntu-latest".into()]).await;

    let run = {
        let runs = fx
            .db
            .list_action_runs_for_repo(&fx.repo_id, &Default::default(), 10, 0)
            .await
            .unwrap();
        runs[0].clone()
    };
    assert_eq!(run.status, "success");

    fx.db
        .requeue_action_run(&run.id, false, None, false)
        .await
        .unwrap();
    oxidean_api::actions::scheduler::recompute_ready_jobs(&fx.db, &run.id)
        .await
        .unwrap();

    let run2 = fx
        .db
        .find_action_run_by_id(&run.id)
        .await
        .unwrap()
        .expect("run row");
    assert_eq!(run2.attempt, 2, "requeue bumps run attempt");
    assert_eq!(run2.status, "queued");

    drive_runner(&fx, &["ubuntu-latest".into()]).await;

    let jobs = fx.db.list_action_jobs_for_run(&run.id).await.unwrap();
    let by_key: std::collections::HashMap<String, String> = jobs
        .iter()
        .map(|j| (j.job_key.clone(), j.status.clone()))
        .collect();
    assert_eq!(
        by_key,
        std::collections::HashMap::from([
            ("a".to_string(), "skipped".to_string()),
            ("b".to_string(), "skipped".to_string()),
            ("c".to_string(), "success".to_string()),
        ]),
        "rerun must reproduce the same outcome"
    );
    assert!(
        jobs.iter().all(|j| j.attempt == 2),
        "each job's attempt bumped"
    );
    let run3 = fx
        .db
        .find_action_run_by_id(&run.id)
        .await
        .unwrap()
        .expect("run row");
    assert_eq!(run3.status, "success");
}

/// Rerun of a reusable-call run: caller legs reset to `waiting` (never
/// claimable), stale callee rows are replaced by re-expansion, and the rerun
/// reproduces the same outcome with bumped attempts.
#[tokio::test]
async fn reusable_call_rerun_reexpands() {
    let case = cases()
        .into_iter()
        .find(|c| c.id == "reusable-workflow-local")
        .expect("reusable-workflow-local case");
    let fx = fixture(&case).await;
    drive_runner(&fx, &["ubuntu-latest".into()]).await;

    let run = fx
        .db
        .list_action_runs_for_repo(&fx.repo_id, &Default::default(), 10, 0)
        .await
        .unwrap()[0]
        .clone();
    assert_eq!(run.status, "success");
    let pre = fx.db.list_action_jobs_for_run(&run.id).await.unwrap();
    assert_eq!(pre.len(), 3, "call + callee build + report");

    fx.db
        .requeue_action_run(&run.id, false, None, false)
        .await
        .unwrap();
    oxidean_api::actions::scheduler::recompute_ready_jobs(&fx.db, &run.id)
        .await
        .unwrap();
    // Recompute expands the reset caller in place: it goes straight back to
    // `in_progress` and the callee rows are fresh (re-emitted, not reused).
    let mid = fx.db.list_action_jobs_for_run(&run.id).await.unwrap();
    let caller = mid.iter().find(|j| j.job_key == "call").unwrap();
    assert_eq!(caller.status, "in_progress");
    let pre_ids: std::collections::HashSet<&str> = pre.iter().map(|j| j.id.as_str()).collect();
    assert!(
        mid.iter()
            .filter(|j| j.job_key == "build")
            .all(|j| !pre_ids.contains(j.id.as_str())),
        "callee rows must be re-emitted, not recycled"
    );

    drive_runner(&fx, &["ubuntu-latest".into()]).await;

    let jobs = fx.db.list_action_jobs_for_run(&run.id).await.unwrap();
    let by_key: std::collections::HashMap<String, String> = jobs
        .iter()
        .map(|j| (j.job_key.clone(), j.status.clone()))
        .collect();
    assert_eq!(
        by_key,
        std::collections::HashMap::from([
            ("call".to_string(), "success".to_string()),
            ("build".to_string(), "success".to_string()),
            ("report".to_string(), "success".to_string()),
        ]),
        "rerun must re-expand the callee and reproduce the outcome"
    );
    // Reset rows (caller + top-level) bump `attempt`; re-emitted callee rows
    // are fresh inserts at attempt 1.
    assert_eq!(
        jobs.iter().find(|j| j.job_key == "call").unwrap().attempt,
        2
    );
    assert_eq!(
        jobs.iter().find(|j| j.job_key == "report").unwrap().attempt,
        2
    );
    assert_eq!(
        jobs.iter().find(|j| j.job_key == "build").unwrap().attempt,
        1
    );
}

/// `$GITHUB_STEP_SUMMARY` — the file-command file accumulates across steps
/// and lands on `action_jobs.summary_md` via the terminal update_task.
#[tokio::test]
async fn step_summary_persists_on_job() {
    let case = cases()
        .into_iter()
        .find(|c| c.id == "step-summary")
        .expect("step-summary case");
    let fx = fixture(&case).await;
    drive_runner(&fx, &["ubuntu-latest".into()]).await;
    let run = fx
        .db
        .list_action_runs_for_repo(&fx.repo_id, &Default::default(), 10, 0)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(run.status, "success");
    let jobs = fx.db.list_action_jobs_for_run(&run.id).await.unwrap();
    let job = jobs.iter().find(|j| j.job_key == "build").unwrap();
    let summary = job.summary_md.as_deref().unwrap_or_default();
    assert!(
        summary.contains("### Results") && summary.contains("- all green"),
        "job summary_md must carry the step summary — got {summary:?}"
    );
}
