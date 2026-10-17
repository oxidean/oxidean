//! Phase 0 conformance harness — one corpus workflow per feature cell in
//! `tests/conformance/workflows/`, expectations in `tests/conformance/cases.json`.
//!
//! A scripted runner claims jobs over the real `/api/actions/*` HTTP protocol
//! (register → fetch_task → update_task → update_log), interprets `run:` steps
//! with `sh`, stubs `uses:` steps, then asserts run/job statuses and log
//! contents. Cases for features the scheduler can't execute yet are asserted
//! as `validation_failure` (the loud-failure contract) and flip to `run` as
//! later phases land.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use oxidean_api::actions::dispatch_push_for_sha;
use oxidean_api::actions::mint_registration_token;
use oxidean_api::actions::secrets::encrypt_secret;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, GitBackend};
use serde::Deserialize;
use tower::ServiceExt;
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
    head_ref: Option<String>,
    expect_run: Option<String>,
    expect_jobs: Option<HashMap<String, String>>,
    expect_job_names: Option<HashMap<String, String>>,
    expect_unsupported: Option<Vec<String>>,
    log_contains: Option<Vec<String>>,
    log_absent: Option<Vec<String>>,
}

fn cases() -> Vec<Case> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/cases.json");
    let raw = std::fs::read_to_string(&path).expect("read cases.json");
    let file: CasesFile = serde_json::from_str(&raw).expect("parse cases.json");
    file.cases
}

fn workflow_path(id: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/conformance/workflows")
        .join(format!("{id}.yml"))
}

static INIT_ONCE: std::sync::Once = std::sync::Once::new();
static SHARED_LOG_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Process-wide env the API resolves at call time: the secret crypto key and
/// the actions log dir. The log dir is shared (runs/jobs are UUID-namespaced)
/// because `enqueue_run` resolves it from env, not `AppState`.
fn ensure_env() -> &'static PathBuf {
    INIT_ONCE.call_once(|| {
        std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "conformance-test-key");
        let dir = std::env::temp_dir().join(format!("oxidean-conf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("OXIDEAN_ACTIONS_LOG_DIR", &dir);
        SHARED_LOG_DIR.set(dir).unwrap();
    });
    SHARED_LOG_DIR.get().unwrap()
}

struct Fixture {
    app: axum::Router,
    db: Database,
    dir: tempfile::TempDir,
    log_dir: PathBuf,
    repo_id: String,
    n_dispatched: usize,
}

async fn fixture(case: &Case) -> Fixture {
    let log_dir = ensure_env().clone();
    let dir = tempfile::tempdir().unwrap();
    let repos_dir = dir.path().join("repos");
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("c.db").display()))
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
    .with_repos_dir(repos_dir.clone());
    let app = router_with_state(state, build_cors("development", None).unwrap());

    let slug = format!("cf-{}", &case.id[..case.id.len().min(16)]);
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
            &format!("s-{}", Uuid::new_v4()),
            &repo.id,
            &name,
            &encrypt_secret(&value).unwrap(),
        )
        .await
        .unwrap();
    }

    // fetch_task re-reads the workflow via {repos_dir}/{owner}/{repo}.git.
    let bare = repos_dir.join(&slug).join(format!("{slug}.git"));
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let git = CliGitBackend::new();
    git.init_bare(&bare, "main").await.unwrap();
    let workflow = std::fs::read(workflow_path(&case.id))
        .unwrap_or_else(|e| panic!("read corpus workflow for {}: {e}", case.id));
    git.seed_commit(
        &bare,
        "main",
        "conformance seed",
        &[(".github/workflows/conformance.yml".into(), workflow)],
    )
    .await
    .unwrap();
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

    Fixture {
        app,
        db,
        dir,
        log_dir,
        repo_id: repo.id,
        n_dispatched,
    }
}

async fn post(
    app: &axum::Router,
    uri: &str,
    token: Option<&str>,
    body: serde_json::Value,
) -> serde_json::Value {
    let mut req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_else(|_| serde_json::json!({}))
}

fn mask(text: &str, secrets: &HashMap<String, String>) -> String {
    let mut out = text.to_string();
    for v in secrets.values() {
        if !v.is_empty() {
            out = out.replace(v.as_str(), "***");
        }
    }
    out
}

/// Scripted runner: claim every job it matches, run the steps, report results.
/// Mirrors `oxidean-runner`'s step semantics (`##[step]` markers, `${{ }}`
/// warning, masking, host `sh`) without the binary.
async fn drive_runner(fx: &Fixture, labels: &[String]) {
    let reg = mint_registration_token(&fx.db).await.unwrap();
    let reg_v = post(
        &fx.app,
        "/api/actions/register",
        None,
        serde_json::json!({"name": "conformance", "labels": labels, "token": reg}),
    )
    .await;
    let token = reg_v["runner_token"].as_str().expect("runner_token");

    for _ in 0..32 {
        let task = post(
            &fx.app,
            "/api/actions/fetch_task",
            Some(token),
            serde_json::json!({}),
        )
        .await;
        let Some(job_id) = task["job_id"].as_str() else {
            break;
        };
        let run_id = task["run_id"].as_str().unwrap().to_string();
        let job_id = job_id.to_string();
        post(
            &fx.app,
            "/api/actions/update_task",
            Some(token),
            serde_json::json!({"job_id": job_id, "state": "in_progress"}),
        )
        .await;

        let secrets: HashMap<String, String> =
            serde_json::from_value(task["secrets"].clone()).unwrap_or_default();
        let steps = task["steps"].as_array().cloned().unwrap_or_default();
        let workspace = fx.dir.path().join("ws").join(&job_id);
        std::fs::create_dir_all(&workspace).unwrap();

        let mut failed = false;
        for (i, step) in steps.iter().enumerate() {
            let title = step["name"]
                .as_str()
                .map(str::to_string)
                .or_else(|| step["uses"].as_str().map(str::to_string))
                .or_else(|| step["run"].as_str().map(|r| r.chars().take(60).collect()))
                .unwrap_or_else(|| format!("step {i}"));
            let mut log = format!("##[step]{title}\n");

            let contains_expr = |v: &serde_json::Value| {
                v.as_str().is_some_and(|s| s.contains("${{"))
                    || (!v.is_string() && v.to_string().contains("${{"))
            };
            if contains_expr(&step["run"])
                || contains_expr(&step["uses"])
                || contains_expr(&step["env"])
            {
                log.push_str("::warning::${{ }} expressions are not evaluated — used literally\n");
            }

            let step_result: Result<(), String> = if let Some(uses) = step["uses"].as_str() {
                if uses.starts_with("actions/checkout") {
                    log.push_str("##[checkout] stubbed checkout\n");
                    Ok(())
                } else {
                    Err(format!(
                        "unsupported action '{uses}' — only actions/checkout and run: steps are supported"
                    ))
                }
            } else if let Some(script) = step["run"].as_str() {
                let cwd = step["working_directory"]
                    .as_str()
                    .map(|d| workspace.join(d))
                    .unwrap_or_else(|| workspace.clone());
                let shell = step["shell"].as_str().unwrap_or("sh");
                let mut cmd = tokio::process::Command::new(shell);
                cmd.arg("-c").arg(script).current_dir(&cwd);
                // Mirror runner job_env: base vars + secrets + step env overlay.
                cmd.env("CI", "true")
                    .env("OXIDEAN", "true")
                    .env("GITHUB_WORKSPACE", &workspace)
                    .env("GITHUB_SHA", task["head_sha"].as_str().unwrap_or_default())
                    .env("GITHUB_REF", task["head_ref"].as_str().unwrap_or_default())
                    .env("GITHUB_RUN_ID", &run_id)
                    .env(
                        "GITHUB_REPOSITORY",
                        format!(
                            "{}/{}",
                            task["repository_owner"].as_str().unwrap_or_default(),
                            task["repository_name"].as_str().unwrap_or_default()
                        ),
                    );
                for (k, v) in &secrets {
                    cmd.env(k, v);
                }
                if let Some(map) = step["env"].as_object() {
                    for (k, v) in map {
                        cmd.env(k, v.as_str().unwrap_or_default());
                    }
                }
                match cmd.output().await {
                    Ok(out) => {
                        log.push_str(&String::from_utf8_lossy(&out.stdout));
                        log.push_str(&String::from_utf8_lossy(&out.stderr));
                        if out.status.success() {
                            Ok(())
                        } else {
                            Err(format!("step exited {}", out.status))
                        }
                    }
                    Err(e) => Err(format!("spawn {shell}: {e}")),
                }
            } else {
                Ok(())
            };

            if let Err(e) = step_result {
                log.push_str(&format!("::error::{e}\n"));
                failed = true;
            }
            let masked = mask(&log, &secrets);
            if !masked.is_empty() {
                post(
                    &fx.app,
                    "/api/actions/update_log",
                    Some(token),
                    serde_json::json!({"run_id": run_id, "job_id": job_id, "chunk": masked}),
                )
                .await;
            }
            if failed {
                break;
            }
        }
        post(
            &fx.app,
            "/api/actions/update_task",
            Some(token),
            serde_json::json!({"job_id": job_id, "state": if failed { "failure" } else { "success" }}),
        )
        .await;
    }
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
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/golden.json");
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
        "queued" | "run" => {
            assert_eq!(fx.n_dispatched, 1, "{}: expected one enqueue", case.id);
            assert_eq!(runs.len(), 1, "{}: expected one run", case.id);
            let labels = case
                .runner_labels
                .clone()
                .unwrap_or_else(|| vec!["ubuntu-latest".into()]);
            drive_runner(&fx, &labels).await;
            let jobs = fx.db.list_action_jobs_for_run(&runs[0].id).await.unwrap();
            for (key, want) in case.expect_jobs.clone().unwrap_or_default() {
                let job = jobs
                    .iter()
                    .find(|j| &j.job_key == key.as_str())
                    .unwrap_or_else(|| panic!("{}: job '{key}' missing", case.id));
                assert_eq!(
                    job.status,
                    want,
                    "{}: job '{key}' status (log: {})",
                    case.id,
                    read_job_log(&fx, &runs[0].id, &job.id)
                );
            }
            for (key, want) in case.expect_job_names.clone().unwrap_or_default() {
                let job = jobs.iter().find(|j| &j.job_key == key.as_str()).unwrap();
                assert_eq!(job.name, want, "{}: job '{key}' name", case.id);
            }
            if case.kind == "run" {
                let run = fx
                    .db
                    .find_action_run_by_id(&runs[0].id)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    run.status,
                    case.expect_run.as_deref().unwrap_or("success"),
                    "{}: run status",
                    case.id
                );
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
conformance_case!(push_tag_ref, "push-tag-ref");
conformance_case!(trigger_map_form, "trigger-map-form");
conformance_case!(trigger_list_form, "trigger-list-form");

// ── queued / never_dispatch ───────────────────────────────────────────
conformance_case!(runs_on_nomatch, "runs-on-nomatch");
conformance_case!(dispatch_only, "dispatch-only");
conformance_case!(pr_only, "pr-only");
conformance_case!(prt_only, "prt-only");
conformance_case!(runs_on_missing, "runs-on-missing");

// ── validation_failure (loud-failure contract) ────────────────────────
conformance_case!(needs_basic, "needs-basic");
conformance_case!(needs_diamond, "needs-diamond");
conformance_case!(strategy_matrix, "strategy-matrix");
conformance_case!(container_job, "container-job");
conformance_case!(services_job, "services-job");
conformance_case!(environment_job, "environment-job");
conformance_case!(concurrency_workflow, "concurrency-workflow");
conformance_case!(concurrency_job, "concurrency-job");
conformance_case!(job_if, "job-if");
conformance_case!(step_if, "step-if");
conformance_case!(job_outputs, "job-outputs");
conformance_case!(reusable_workflow_local, "reusable-workflow-local");
conformance_case!(reusable_workflow_remote, "reusable-workflow-remote");
conformance_case!(continue_on_error_job, "continue-on-error-job");
conformance_case!(continue_on_error_step, "continue-on-error-step");
conformance_case!(timeout_minutes_job, "timeout-minutes-job");
conformance_case!(timeout_minutes_step, "timeout-minutes-step");
conformance_case!(prt_push, "prt-push");
conformance_case!(empty_steps, "empty-steps");

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
