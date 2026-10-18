//! Job execution: `actions/checkout` builtin, `uses:` action dispatch,
//! `run:` steps on the host or in Docker, expression evaluation (`${{ }}`),
//! log streaming with secret masking.

pub(crate) mod action;
pub(crate) mod artifacts;
pub(crate) mod checkout;
pub(crate) mod commands;
pub(crate) mod containers;
pub(crate) mod context;
pub(crate) mod env;
pub(crate) mod glob;
pub(crate) mod run_step;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use expr::{Context, JobStatus};
use oxidean_actions_expr as expr;

use crate::client::{Client, Task};
use crate::config::{Config, ExecMode};

use context::{
    interpolate_step, record_step, runner_context, step_allowed, step_outputs_report, step_timeout,
    sync_job_ctx, JobDeadline,
};
use env::{apply_file_commands, job_env, layer_env, parse_output_file, prepare_runner_temp};
use run_step::run_step;

#[cfg(test)]
pub(crate) use {
    checkout::checkout_auth_args,
    env::{
        env_get, env_set, parse_env_file, parse_path_file, prepare_workspace, DOCKER_DEFAULT_PATH,
        PAIRED_VARS,
    },
    glob::{glob_match, glob_walk, hash_files_fn},
    run_step::shell_argv,
};

/// Mirrors `StepSpec` in `oxidean-api::actions::parse`.
#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    pub id: Option<String>,
    pub name: Option<String>,
    pub uses: Option<String>,
    pub run: Option<String>,
    #[serde(rename = "if")]
    pub if_expr: Option<String>,
    #[serde(rename = "timeout-minutes")]
    pub timeout_minutes: Option<serde_json::Value>,
    #[serde(rename = "continue-on-error")]
    pub continue_on_error: Option<serde_json::Value>,
    pub shell: Option<String>,
    pub working_directory: Option<String>,
    pub with: Option<serde_json::Value>,
    pub env: Option<serde_json::Value>,
}

/// A `post:` entry deferred by an action (`runs.post` / `post-if`) — executed
/// after the main step loop in reverse-declaration order, like GitHub.
pub(crate) struct PendingPost {
    pub name: String,
    /// `post-if` source (default `always()`).
    pub if_expr: Option<String>,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
}

/// Secret values (repo secrets + the job token) are masked out of log chunks
/// before upload.
fn mask_secrets(text: &str, mask_values: &[String]) -> String {
    let mut out = text.to_string();
    for v in mask_values {
        if !v.is_empty() {
            out = out.replace(v.as_str(), "***");
        }
    }
    out
}

/// Pick the declared label matching this task's `runs-on` (API already
/// label-matched at claim time — first hit wins). Falls back to host mode.
fn exec_mode_for(task: &Task, config: &Config) -> ExecMode {
    let wants = task.runs_on.clone().unwrap_or_default();
    for want in &wants {
        if let Some(l) = config.labels.iter().find(|l| &l.name == want) {
            return l.mode.clone();
        }
    }
    ExecMode::Host
}

pub(crate) struct Logger<'a> {
    pub client: &'a Client,
    pub run_id: String,
    pub job_id: String,
    /// Secret values + the job token — masked out of every log chunk.
    pub mask_values: &'a [String],
    /// `::add-mask::` values collected mid-run — masked alongside `mask_values`.
    pub extra_masks: std::sync::Mutex<Vec<String>>,
    /// `::stop-commands::token` suppression — while set, all lines are literal.
    pub stop_token: std::sync::Mutex<Option<String>>,
    /// Unterminated output tail — a command can split across read chunks.
    pub partial_line: std::sync::Mutex<String>,
    /// Warnings queued by the sync interpreter; `send` drains them next to the
    /// triggering chunk (kept off `send_masked` — warning text can contain
    /// `::cmd::` markers and would re-enter interpretation).
    pub pending_warnings: std::sync::Mutex<Vec<String>>,
    /// Server-side cancel signal — set when an `update_log` ack reports
    /// `cancelled`; the step loop checks `take_cancelled()` between steps.
    pub cancelled: std::sync::atomic::AtomicBool,
    /// Next log-batch index the server expects. Shared because log sends can
    /// overlap with step output; every ack (and every 409) resyncs it.
    pub log_cursor: std::sync::atomic::AtomicI64,
    /// Once-warned command keys (uninterpreted matchers, deprecated commands).
    pub seen_cmd_warns: std::sync::Mutex<std::collections::BTreeSet<&'static str>>,
    /// `runner_temp` — the `$GITHUB_*` file dir `::` file-commands append to.
    /// Set once `prepare_runner_temp` succeeds (early claim errors predate it).
    pub runner_temp: std::sync::OnceLock<PathBuf>,
    /// `ACTIONS_STEP_DEBUG` — when set, `::debug::` lines forward to the log.
    pub step_debug: std::sync::atomic::AtomicBool,
}

impl<'a> Logger<'a> {
    pub(crate) fn take_cancelled(&self) -> bool {
        self.cancelled
            .swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    /// One send attempt at the shared cursor. Cursor serializes batches: the
    /// claim is single-runner and sends are `await`ed in-order, so claiming
    /// the slot then awaiting the response can't interleave.
    async fn send_once(&self, masked: &str) -> Result<crate::client::UpdateTaskAck, String> {
        let index = self.log_cursor.load(std::sync::atomic::Ordering::SeqCst);
        let ack = self
            .client
            .update_log(&self.run_id, &self.job_id, masked, index)
            .await?;
        // Accepted batch consumed our slot; a rejected gap didn't — in both
        // cases the server reports its next expected index, so trust it.
        let next = if ack.ok {
            ack.log_index.max(index + 1)
        } else {
            ack.log_index
        };
        self.log_cursor
            .store(next, std::sync::atomic::Ordering::SeqCst);
        Ok(ack)
    }

    /// Outbound chunk: interpret `::cmd::` workflow commands (file effects
    /// land in `runner_temp`, masks extend, group markers substitute), then
    /// mask (base secrets + `::add-mask::` values) and upload. Queued command
    /// warnings go first so they read next to the line that triggered them.
    pub(crate) async fn send(&self, chunk: &str) {
        let text = self.interpret_chunk(chunk);
        self.drain_warnings().await;
        let mut masked = mask_secrets(&text, self.mask_values);
        {
            let extra = self.extra_masks.lock().unwrap();
            if !extra.is_empty() {
                masked = mask_secrets(&masked, &extra);
            }
        }
        self.send_masked(&masked).await;
    }

    /// Masked-text send: upload with cursor/ack handling + stdout mirror.
    async fn send_masked(&self, masked: &str) {
        if masked.is_empty() {
            return;
        }
        match self.send_once(masked).await {
            Ok(ack) if !ack.ok => {
                // 409 gap — cursor already resynced to the server's expected
                // index; resend this chunk once. A second gap drops it (log
                // loss is tolerable; wedging is not).
                match self.send_once(masked).await {
                    Ok(ack) if ack.cancelled => {
                        self.cancelled
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(e) => tracing::warn!("update_log retry failed: {e}"),
                    _ => {}
                }
            }
            Ok(ack) if ack.cancelled => {
                self.cancelled
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => {
                // Transport failure — the chunk may have landed with the ack
                // lost. Resend once at the same index: the server answers
                // Duplicate (drop, resync) or Accept (append) — exactly-once
                // either way. A second failure drops the chunk.
                match self.send_once(masked).await {
                    Ok(ack) if ack.cancelled => {
                        self.cancelled
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(e2) => {
                        tracing::warn!("update_log failed twice, chunk dropped: {e} / {e2}")
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        // Mirror to runner stdout for operator visibility (masked).
        use std::io::Write;
        let _ = std::io::stdout().write_all(masked.as_bytes());
        let _ = std::io::stdout().flush();
    }

    pub(crate) async fn line(&self, msg: &str) {
        self.send(&format!("{msg}\n")).await;
    }
}

/// The step-loop environment shared with action dispatch (`action.rs`):
/// everything `uses:`/post handling needs to build env, run commands, and
/// record outcomes without re-deriving them.
pub(crate) struct StepEnv<'a> {
    pub task: &'a Task,
    pub config: &'a Config,
    pub mode: &'a ExecMode,
    pub workspace: &'a Path,
    /// Host-side runner temp dir.
    pub runner_temp: &'a Path,
    /// Job env list; steps overlay their own copies.
    pub env: &'a mut Vec<(String, String)>,
    pub reserved: &'a std::collections::HashSet<String>,
    pub logger: &'a Logger<'a>,
    /// Post-action queue — actions push `post:` closures here; drained
    /// reversed after the main loop (GitHub runs post steps last-in-first-out).
    pub posts: &'a mut Vec<PendingPost>,
}

impl StepEnv<'_> {
    /// Merge the env/path/output/state file commands after a step or action
    /// and overlay `$GITHUB_ENV` pairs into the expression context. Returns
    /// what was applied (the caller may re-overlay env into a second ctx —
    /// composite actions keep an inner ctx but env writes persist job-wide).
    pub(crate) async fn apply_commands(&mut self, ctx: &mut Context) -> StepApplied {
        let applied = apply_file_commands(self.runner_temp, self.env, self.reserved, self.mode);
        for w in &applied.warnings {
            self.logger.line(w).await;
        }
        if !applied.env.is_empty() {
            let m: serde_json::Map<String, serde_json::Value> = applied
                .env
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            ctx.overlay_env(&serde_json::Value::Object(m));
        }
        StepApplied {
            outputs: parse_output_file(&self.runner_temp.join("output")),
            env: applied.env,
        }
    }
}

/// What [`StepEnv::apply_commands`] drained after a step.
pub(crate) struct StepApplied {
    /// `$GITHUB_OUTPUT` pairs → `steps.<id>.outputs`.
    pub outputs: HashMap<String, String>,
    /// `$GITHUB_ENV` pairs actually applied (for re-overlay into other ctxs).
    pub env: Vec<(String, String)>,
}

/// Execute a claimed task end to end; reports state transitions on the client.
/// Returns the terminal job state.
pub async fn execute(client: &Client, task: &Task, config: &Config) -> &'static str {
    let job_id = task.job_id.clone().unwrap_or_default();
    let run_id = task.run_id.clone().unwrap_or_default();
    let secrets = task.secrets.clone().unwrap_or_default();
    // Mask repo secrets AND the job token out of every uploaded log chunk.
    let mut mask_values: Vec<String> = secrets.values().cloned().collect();
    if let Some(t) = task.job_token.as_ref().filter(|t| !t.is_empty()) {
        mask_values.push(t.clone());
    }
    let logger = Logger {
        client,
        run_id: run_id.clone(),
        job_id: job_id.clone(),
        mask_values: &mask_values,
        extra_masks: std::sync::Mutex::new(Vec::new()),
        stop_token: std::sync::Mutex::new(None),
        partial_line: std::sync::Mutex::new(String::new()),
        pending_warnings: std::sync::Mutex::new(Vec::new()),
        cancelled: std::sync::atomic::AtomicBool::new(false),
        log_cursor: std::sync::atomic::AtomicI64::new(0),
        seen_cmd_warns: std::sync::Mutex::new(std::collections::BTreeSet::new()),
        runner_temp: std::sync::OnceLock::new(),
        step_debug: std::sync::atomic::AtomicBool::new(false),
    };

    match client.update_task(&job_id, "in_progress", None, None).await {
        // Superseded between claim and start — still say so in the log,
        // otherwise the job page shows an empty log with no explanation.
        Ok(ack) if ack.cancelled => {
            logger
                .line("::notice::job cancelled before it could start")
                .await;
            return "cancelled";
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!("update_task in_progress failed: {e}");
            return "failure";
        }
    }

    let workspace = config.work_dir.join(&job_id);
    if let Err(e) = env::prepare_workspace(&workspace) {
        logger
            .line(&format!("::error::failed to prepare workspace: {e}"))
            .await;
        let _ = client.update_task(&job_id, "failure", None, None).await;
        return "failure";
    }
    // Per-job runner temp: event.json + env/path/output/step_summary files.
    let runner_temp = config.work_dir.join("_runner").join(&job_id);
    if let Err(e) = prepare_runner_temp(&runner_temp, task) {
        logger
            .line(&format!("::error::failed to prepare runner temp dir: {e}"))
            .await;
        let _ = client.update_task(&job_id, "failure", None, None).await;
        return "failure";
    }
    logger.set_runner_temp(runner_temp.clone());

    let steps: Vec<Step> = task
        .steps
        .clone()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    // `container:`/`services:` — bring the network + containers up before
    // env building so the ContainerExec mode and path aliases line up. The
    // guard's Drop (`docker rm -f`/`network rm`) covers every return path.
    let mut mode = exec_mode_for(task, config);
    // Held for its Drop (container teardown) — never read after setup.
    let mut _containers = None;
    if task.container.is_some() || task.services.is_some() {
        match containers::JobContainers::setup(task, &workspace, &runner_temp, &logger).await {
            Ok(c) => {
                if let Some(id) = c.job_container_id() {
                    mode = ExecMode::ContainerExec(id.to_string());
                    if steps.iter().any(|s| s.uses.is_some()) {
                        logger
                            .line("::warning::uses: steps run on the runner host inside container jobs")
                            .await;
                    }
                }
                _containers = Some(c);
            }
            Err(e) => {
                logger.line(&format!("::error::{e}")).await;
                let _ = client.update_task(&job_id, "failure", None, None).await;
                return "failure";
            }
        }
    }

    // Inside a container the workspace mounts at /workspace and the runner
    // temp dir at /__runner_temp.
    let (workspace_env, runner_temp_env) = match &mode {
        ExecMode::Docker(_) | ExecMode::ContainerExec(_) => {
            ("/workspace".to_string(), "/__runner_temp".to_string())
        }
        ExecMode::Host => (
            workspace.to_string_lossy().to_string(),
            runner_temp.to_string_lossy().to_string(),
        ),
    };
    let mut env = job_env(task, config, &workspace_env, &runner_temp_env, &secrets);
    for w in &env.warnings {
        logger.line(w).await;
    }
    // Expression context — github/env/runner/matrix/secrets/steps + hashFiles
    // over the workspace.
    let mut ctx = runner_context(
        task,
        config,
        &workspace,
        &workspace_env,
        &runner_temp_env,
        &secrets,
    );
    // Workflow env → job env layering: each evaluates against the context of
    // the layers before it and feeds both `env.*` expressions and the step
    // process env.
    if let Some(v) = &task.workflow_env {
        for w in layer_env(&mut ctx, &mut env.vars, v) {
            logger.line(&w).await;
        }
    }
    if let Some(v) = &task.env {
        for w in layer_env(&mut ctx, &mut env.vars, v) {
            logger.line(&w).await;
        }
    }
    // `::debug::` lines forward only when the workflow asked for them —
    // GitHub reads `ACTIONS_STEP_DEBUG` from repo/org variables OR secrets
    // (truthy: "true"/"1"); an explicit `env:` entry wins over both.
    fn truthy(v: impl AsRef<str>) -> bool {
        let v = v.as_ref().trim();
        v.eq_ignore_ascii_case("true") || v == "1"
    }
    let step_debug = env::env_get(&env.vars, "ACTIONS_STEP_DEBUG").is_some_and(truthy)
        || task
            .vars
            .as_ref()
            .is_some_and(|m| m.get("ACTIONS_STEP_DEBUG").is_some_and(truthy))
        || secrets.get("ACTIONS_STEP_DEBUG").is_some_and(truthy);
    // `ACTIONS_RUNNER_DEBUG` gates runner-internal diagnostics on GitHub; our
    // only debug surface is the job log, so it implies step-debug here.
    let runner_debug = env::env_get(&env.vars, "ACTIONS_RUNNER_DEBUG").is_some_and(truthy)
        || task
            .vars
            .as_ref()
            .is_some_and(|m| m.get("ACTIONS_RUNNER_DEBUG").is_some_and(truthy))
        || secrets.get("ACTIONS_RUNNER_DEBUG").is_some_and(truthy);
    logger.set_step_debug(step_debug || runner_debug);
    let default_timeout = Duration::from_secs(config.job_timeout_secs);
    let deadline = JobDeadline::new(task.timeout_seconds);

    logger
        .line(&format!(
            "Job {} — {} / {} step(s) [{}]",
            task.job_key.clone().unwrap_or_default(),
            task.workflow_name.clone().unwrap_or_default(),
            steps.len(),
            match &mode {
                ExecMode::Host => "host".to_string(),
                ExecMode::Docker(img) => format!("docker:{img}"),
                ExecMode::ContainerExec(_) => "container".to_string(),
            }
        ))
        .await;

    let mut posts: Vec<PendingPost> = Vec::new();
    let mut cancelled = false;
    {
        let mut step_env = StepEnv {
            task,
            config,
            mode: &mode,
            workspace: &workspace,
            runner_temp: &runner_temp,
            env: &mut env.vars,
            reserved: &env.reserved,
            logger: &logger,
            posts: &mut posts,
        };
        for (i, raw_step) in steps.iter().enumerate() {
            let step_id = raw_step.id.clone().unwrap_or_else(|| format!("__step{i}"));
            // Job deadline gates every subsequent step (GitHub `cancelled`).
            if deadline.expired() {
                logger
                    .line(&format!("::notice::step '{step_id}' skipped — job timeout"))
                    .await;
                record_step(&mut ctx, &step_id, "skipped", "skipped", HashMap::new());
                cancelled = true;
                ctx.set_job_status(JobStatus::Cancelled);
                sync_job_ctx(&mut ctx);
                continue;
            }
            // Server-side cancel (concurrency supersede, fail-fast sibling,
            // run cancel) arrives via update_log acks.
            if logger.take_cancelled() {
                logger
                    .line(&format!(
                        "::notice::step '{step_id}' skipped — job cancelled"
                    ))
                    .await;
                record_step(&mut ctx, &step_id, "skipped", "skipped", HashMap::new());
                cancelled = true;
                ctx.set_job_status(JobStatus::Cancelled);
                sync_job_ctx(&mut ctx);
                continue;
            }
            // Step `if` — implicit `success()` unless a status fn is present.
            let allowed = match step_allowed(raw_step.if_expr.as_deref(), &ctx) {
                Ok(a) => a,
                Err(e) => {
                    logger
                        .line(&format!(
                            "::error::step '{step_id}' if evaluation failed: {e}"
                        ))
                        .await;
                    record_step(&mut ctx, &step_id, "failure", "failure", HashMap::new());
                    ctx.set_job_status(JobStatus::Failure);
                    sync_job_ctx(&mut ctx);
                    continue;
                }
            };
            if !allowed {
                record_step(&mut ctx, &step_id, "skipped", "skipped", HashMap::new());
                continue;
            }
            // Interpolate all expression-bearing fields once per step.
            let step = match interpolate_step(raw_step, &ctx) {
                Ok(s) => s,
                Err(e) => {
                    logger
                        .line(&format!("::error::step '{step_id}' expression error: {e}"))
                        .await;
                    record_step(&mut ctx, &step_id, "failure", "failure", HashMap::new());
                    ctx.set_job_status(JobStatus::Failure);
                    sync_job_ctx(&mut ctx);
                    continue;
                }
            };
            let title = step
                .name
                .clone()
                .or_else(|| step.uses.clone())
                .or_else(|| step.run.as_ref().map(|r| truncate(r, 60)))
                .unwrap_or_else(|| format!("step {i}"));
            logger.line(&format!("##[step]{title}")).await;

            let (timeout, timeout_warnings) =
                step_timeout(&step, &ctx, default_timeout, deadline.remaining());
            for w in timeout_warnings {
                logger.line(&w).await;
            }
            // Step-level env overlays job env — both the `env` context and
            // the process env for this step.
            let mut this_step_env = step_env.env.clone();
            if let Some(step_env_map) = &step.env {
                let mut step_ctx = ctx.clone();
                for w in layer_env(&mut step_ctx, &mut this_step_env, step_env_map) {
                    logger.line(&w).await;
                }
            }
            let result: Result<(), String>;
            let mut outputs = HashMap::new();
            if let Some(uses) = &step.uses {
                result = if uses.starts_with("actions/checkout") {
                    let with = step
                        .with
                        .as_ref()
                        .and_then(|v| v.as_object().cloned())
                        .unwrap_or_default();
                    checkout::checkout(
                        task,
                        config,
                        &workspace,
                        &with,
                        &this_step_env,
                        &logger,
                        timeout,
                    )
                    .await
                } else {
                    match action::run_action(
                        &mut step_env,
                        &mut ctx,
                        &step,
                        &step_id,
                        &this_step_env,
                        timeout,
                        0,
                        &workspace,
                    )
                    .await
                    {
                        Ok(action_outputs) => {
                            outputs.extend(action_outputs);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                };
                // Actions may write $GITHUB_ENV/$GITHUB_OUTPUT too.
                outputs.extend(step_env.apply_commands(&mut ctx).await.outputs);
            } else if step.run.is_some() {
                result = run_step(
                    &step,
                    &mode,
                    &workspace,
                    &runner_temp,
                    step.working_directory.as_deref(),
                    &this_step_env,
                    &logger,
                    timeout,
                )
                .await;
                // File commands apply to subsequent steps even when this one
                // failed (GitHub semantics); the job still fails below.
                outputs = step_env.apply_commands(&mut ctx).await.outputs;
            } else {
                result = Ok(());
            };

            // `$GITHUB_OUTPUT` → `steps.<id>.outputs` (consumed by `needs` in
            // the scheduler phase; also readable via the steps context).
            let continue_on_error = step_continue_on_error(&step, &ctx);
            let outcome = match &result {
                Ok(()) => "success",
                Err(e) if deadline.expired() && e == "step timed out" => {
                    cancelled = true;
                    "cancelled"
                }
                Err(_) => "failure",
            };
            let conclusion = if continue_on_error && outcome == "failure" {
                "success"
            } else {
                outcome
            };
            record_step(&mut ctx, &step_id, outcome, conclusion, outputs);
            if let Err(e) = result {
                if continue_on_error {
                    // GitHub: continue-on-error failure marks the step failed
                    // but leaves job.status success — downstream `if` gates
                    // (which read success()) keep running.
                    logger
                        .line(&format!("::warning::{e} (continue-on-error)"))
                        .await;
                } else {
                    logger.line(&format!("::error::{e}")).await;
                    ctx.set_job_status(if cancelled {
                        JobStatus::Cancelled
                    } else {
                        JobStatus::Failure
                    });
                    sync_job_ctx(&mut ctx);
                }
            }
            // An unterminated output tail must land inside *this* step's
            // section — flush before the done marker.
            logger.flush_partial().await;
            logger.line(&format!("##[step-done]{title}")).await;
        }
    }

    // Post actions — reverse declaration order, gated by `post-if`
    // (default `always()`).
    let posts = std::mem::take(&mut posts);
    for post in posts.into_iter().rev() {
        let allowed = match &post.if_expr {
            None => true,
            Some(src) => match expr::eval_bool(src, &ctx) {
                Ok(b) => b,
                Err(e) => {
                    logger
                        .line(&format!(
                            "::warning::post step '{}' if evaluation failed: {e} — skipped",
                            post.name
                        ))
                        .await;
                    false
                }
            },
        };
        if !allowed {
            continue;
        }
        logger.line(&format!("##[post]{}", post.name)).await;
        let post_env = post.env.clone();
        let mut cmd = tokio::process::Command::new(&post.argv[0]);
        cmd.args(&post.argv[1..]).current_dir(&post.cwd);
        match run_step::run_streamed(
            &mut cmd,
            &post_env,
            &logger,
            deadline.remaining().unwrap_or(default_timeout),
        )
        .await
        {
            Ok(0) => {}
            Ok(code) => {
                logger
                    .line(&format!("::warning::post '{}' exited {code}", post.name))
                    .await;
            }
            Err(e) => {
                logger
                    .line(&format!("::warning::post '{}' failed: {e}", post.name))
                    .await;
            }
        }
    }

    // Final unterminated tail, if any.
    logger.flush_partial().await;

    // `$GITHUB_STEP_SUMMARY` (accumulated across steps) ships once at
    // terminal — capped like GitHub's 1MiB. Read before temp cleanup.
    let summary = std::fs::read_to_string(runner_temp.join("step_summary"))
        .ok()
        .map(|s| {
            if s.len() > 1_048_576 {
                s[..1_048_576].to_string()
            } else {
                s
            }
        })
        .filter(|s| !s.trim().is_empty());
    let _ = std::fs::remove_dir_all(&runner_temp);
    let final_state = match ctx.job_status() {
        JobStatus::Cancelled => "cancelled",
        JobStatus::Failure => "failure",
        _ => "success",
    };
    // `steps.<id>.outputs` ride the terminal report so the API materializes
    // `jobs.<id>.outputs` for `needs.*` consumers.
    let outputs = step_outputs_report(&ctx);
    if let Err(e) = client
        .update_task(&job_id, final_state, Some(&outputs), summary.as_deref())
        .await
    {
        tracing::error!("update_task {final_state} failed: {e}");
    }
    final_state
}

/// `continue-on-error` — evaluated (may be a bool or `${{ }}` expression).
/// Absent/false → normal failure semantics.
fn step_continue_on_error(step: &Step, ctx: &Context) -> bool {
    match &step.continue_on_error {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::String(s)) => expr::eval_embedded(s, ctx)
            .ok()
            .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")))
            .unwrap_or(false),
        _ => false,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::path::PathBuf;

    fn cfg(labels: &str) -> Config {
        Config {
            origin: "http://localhost".into(),
            registration_token: None,
            name: "t".into(),
            labels: crate::config::parse_labels(labels).unwrap(),
            state_path: PathBuf::from("/tmp/x"),
            work_dir: PathBuf::from("/tmp/xwork"),
            poll_ms: 1,
            wait_ms: 0,
            git_token: None,
            job_timeout_secs: 60,
            actions_mirror: None,
        }
    }

    fn task() -> Task {
        Task {
            job_id: Some("job1".into()),
            run_id: Some("run1".into()),
            job_key: Some("build".into()),
            runs_on: None,
            workflow_name: Some("CI".into()),
            head_sha: Some("abc123".into()),
            head_ref: Some("refs/heads/main".into()),
            steps: None,
            secrets: None,
            repository_owner: Some("acme".into()),
            repository_name: Some("demo".into()),
            event: Some("push".into()),
            run_number: Some(7),
            run_attempt: Some(1),
            actor: Some("alice".into()),
            actor_id: Some("u1".into()),
            repository_id: Some("r1".into()),
            repository_owner_id: Some("u1".into()),
            server_url: Some("https://oxidean.example".into()),
            base_ref: None,
            pull_number: None,
            job_token: Some("oxidean_job_deadbeef".into()),
            workflow_ref: Some("acme/demo/.oxidean/workflows/ci.yml@refs/heads/main".into()),
            env: None,
            workflow_env: None,
            timeout_at: None,
            timeout_seconds: None,
            event_payload: None,
            matrix: None,
            needs: None,
            container: None,
            services: None,
            environment: None,
            environment_url: None,
            vars: None,
            id_token_request_url: None,
            id_token_request_token: None,
            cache_url: None,
            cache_token: None,
        }
    }

    fn env_map(vars: &[(String, String)]) -> HashMap<String, String> {
        vars.iter().cloned().collect()
    }

    #[test]
    fn secrets_masked_in_logs() {
        let mask = vec!["hunter2".to_string(), String::new()];
        assert_eq!(
            mask_secrets("token is hunter2 ok", &mask),
            "token is *** ok"
        );
        assert_eq!(mask_secrets("nothing", &mask), "nothing");
    }

    #[test]
    fn job_token_masked_in_logs() {
        let mask = vec!["oxidean_job_aaaabbbb".to_string()];
        assert_eq!(
            mask_secrets("token=oxidean_job_aaaabbbb done", &mask),
            "token=*** done"
        );
    }

    #[test]
    fn checkout_auth_prefers_job_token() {
        // Job token (OXIDEAN_TOKEN) wins over the runner-wide git token.
        let args = checkout_auth_args(Some("oxidean_job_secret"), Some("oxidean_pat_runner"));
        assert_eq!(args.len(), 2);
        let encoded = args[1]
            .strip_prefix("http.extraHeader=Authorization: Basic ")
            .unwrap();
        let decoded = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(decoded, "oauth2:oxidean_job_secret");

        // Empty/absent job token falls back to the configured runner token.
        let args = checkout_auth_args(Some(""), Some("oxidean_pat_runner"));
        assert!(args[1].contains(
            &base64::engine::general_purpose::STANDARD.encode("oauth2:oxidean_pat_runner")
        ));
        let args = checkout_auth_args(None, Some("oxidean_pat_runner"));
        assert_eq!(args.len(), 2);

        // Neither → no auth header.
        assert!(checkout_auth_args(None, None).is_empty());
    }

    #[test]
    fn exec_mode_matches_declared_label() {
        let mut task = task();
        task.runs_on = Some(vec!["ubuntu-latest".into()]);
        assert_eq!(exec_mode_for(&task, &cfg("ubuntu-latest")), ExecMode::Host);
        assert_eq!(
            exec_mode_for(&task, &cfg("ubuntu-latest:docker://node:20")),
            ExecMode::Docker("node:20".into())
        );
        // Unmatched label falls back to host.
        assert_eq!(exec_mode_for(&task, &cfg("other")), ExecMode::Host);
    }

    /// Every `OXIDEAN_*` var has a `GITHUB_*` twin with the identical value
    /// (except bare `OXIDEAN` and `CI` which have no GitHub counterpart).
    #[test]
    fn oxidean_vars_have_github_twins() {
        let je = job_env(&task(), &cfg("x"), "/ws", "/tmp/rt", &HashMap::new());
        let map = env_map(&je.vars);
        // Every OXIDEAN_* / GITHUB_* key is a declared pair member (no drift,
        // no stray unpaired keys).
        for k in map.keys() {
            if k.starts_with("OXIDEAN_") {
                assert!(
                    PAIRED_VARS.iter().any(|(ox, _)| ox == k),
                    "unpaired key {k}"
                );
            }
            if k.starts_with("GITHUB_") {
                assert!(
                    PAIRED_VARS.iter().any(|(_, gh)| gh == k),
                    "unpaired key {k}"
                );
            }
        }
        // Every declared pair emitted with equal values — including the
        // asymmetric OXIDEAN_ENV_FILE/GITHUB_ENV pair.
        for (ox, gh) in PAIRED_VARS {
            assert_eq!(map.get(*ox), map.get(*gh), "pair {ox}/{gh} diverged");
            assert!(map.contains_key(*ox), "missing {ox}");
        }
    }

    /// `OXIDEAN_ENV` is reserved by the server runtime-mode config
    /// (development/production) — job_env must never emit a key with that
    /// exact name; the file-command variable is `OXIDEAN_ENV_FILE`.
    #[test]
    fn job_env_never_emits_oxidean_env() {
        let je = job_env(&task(), &cfg("x"), "/ws", "/tmp/rt", &HashMap::new());
        assert!(!je.vars.iter().any(|(k, _)| k == "OXIDEAN_ENV"));
    }

    #[test]
    fn job_env_values() {
        let je = job_env(&task(), &cfg("x"), "/ws", "/tmp/rt", &HashMap::new());
        let map = env_map(&je.vars);
        assert_eq!(map["CI"], "true");
        assert_eq!(map["OXIDEAN"], "true");
        assert_eq!(map["OXIDEAN_ACTIONS"], "true");
        assert_eq!(map["OXIDEAN_SHA"], "abc123");
        assert_eq!(map["GITHUB_SHA"], "abc123");
        assert_eq!(map["OXIDEAN_REPOSITORY"], "acme/demo");
        assert_eq!(map["OXIDEAN_REF_NAME"], "main");
        assert_eq!(map["OXIDEAN_REF_TYPE"], "branch");
        assert_eq!(map["OXIDEAN_API_URL"], "https://oxidean.example/api");
        assert_eq!(map["OXIDEAN_TOKEN"], "oxidean_job_deadbeef");
        assert_eq!(map["OXIDEAN_ENV_FILE"], "/tmp/rt/env");
        assert_eq!(map["GITHUB_ENV"], "/tmp/rt/env");
        assert_eq!(map["OXIDEAN_EVENT_PATH"], "/tmp/rt/event.json");
        assert_eq!(map["RUNNER_TEMP"], "/tmp/rt");
        assert_eq!(map["RUNNER_TOOL_CACHE"], "/tmp/rt/tool-cache");
        assert_eq!(map["RUNNER_NAME"], "t");
        // push event → BASE_REF / HEAD_REF empty.
        assert_eq!(map["OXIDEAN_BASE_REF"], "");
        assert_eq!(map["OXIDEAN_HEAD_REF"], "");
    }

    #[test]
    fn job_env_token_empty_when_absent() {
        let mut t = task();
        t.job_token = None;
        let je = job_env(&t, &cfg("x"), "/ws", "/tmp/rt", &HashMap::new());
        let map = env_map(&je.vars);
        assert_eq!(map["OXIDEAN_TOKEN"], "");
        assert_eq!(map["GITHUB_TOKEN"], "");
    }

    #[test]
    fn job_env_cache_creds_flow_when_minted() {
        let mut t = task();
        t.cache_url = Some("https://oxidean.example/api/actions/_apis/artifactcache".into());
        t.cache_token = Some("oxrt_secret".into());
        let je = job_env(&t, &cfg("x"), "/ws", "/tmp/rt", &HashMap::new());
        let map = env_map(&je.vars);
        assert_eq!(
            map["ACTIONS_CACHE_URL"],
            "https://oxidean.example/api/actions/_apis/artifactcache"
        );
        assert_eq!(map["ACTIONS_RESULTS_URL"], map["ACTIONS_CACHE_URL"]);
        assert_eq!(map["ACTIONS_RUNTIME_TOKEN"], "oxrt_secret");
        // Absent creds → the vars are never emitted (actions/cache skips).
        let t2 = task();
        let je2 = job_env(&t2, &cfg("x"), "/ws", "/tmp/rt", &HashMap::new());
        let map2 = env_map(&je2.vars);
        assert!(!map2.contains_key("ACTIONS_CACHE_URL"));
        assert!(!map2.contains_key("ACTIONS_RUNTIME_TOKEN"));
        // And a repo secret can't shadow the runtime token.
        let mut secrets = HashMap::new();
        secrets.insert("ACTIONS_RUNTIME_TOKEN".to_string(), "evil".to_string());
        let je3 = job_env(&t, &cfg("x"), "/ws", "/tmp/rt", &secrets);
        let map3 = env_map(&je3.vars);
        assert_eq!(map3["ACTIONS_RUNTIME_TOKEN"], "oxrt_secret");
    }

    #[test]
    fn job_env_tag_ref() {
        let mut t = task();
        t.head_ref = Some("refs/tags/v1".into());
        let je = job_env(&t, &cfg("x"), "/ws", "/tmp/rt", &HashMap::new());
        let map = env_map(&je.vars);
        assert_eq!(map["OXIDEAN_REF_NAME"], "v1");
        assert_eq!(map["OXIDEAN_REF_TYPE"], "tag");
    }

    #[test]
    fn job_env_pull_request_head_ref() {
        let mut t = task();
        t.event = Some("pull_request".into());
        t.head_ref = Some("feature-x".into());
        t.base_ref = Some("main".into());
        t.pull_number = Some(42);
        let je = job_env(&t, &cfg("x"), "/ws", "/tmp/rt", &HashMap::new());
        let map = env_map(&je.vars);
        assert_eq!(map["OXIDEAN_HEAD_REF"], "feature-x");
        assert_eq!(map["OXIDEAN_BASE_REF"], "main");
    }

    #[test]
    fn secrets_cannot_override_reserved_vars() {
        let mut secrets = HashMap::new();
        secrets.insert("OXIDEAN_SHA".into(), "evil".into());
        secrets.insert("GITHUB_SHA".into(), "evil".into());
        secrets.insert("RUNNER_OS".into(), "evil".into());
        secrets.insert("MY_SECRET".into(), "ok".into());
        let je = job_env(&task(), &cfg("x"), "/ws", "/tmp/rt", &secrets);
        let map = env_map(&je.vars);
        assert_eq!(map["OXIDEAN_SHA"], "abc123");
        assert_eq!(map["GITHUB_SHA"], "abc123");
        assert_ne!(map["RUNNER_OS"], "evil");
        assert_eq!(map["MY_SECRET"], "ok");
        assert_eq!(je.warnings.len(), 3);
    }

    #[test]
    fn env_file_simple_and_blank_lines() {
        let parsed = parse_env_file("FOO=bar\n\nBAZ=qux=with=eq\n   \n");
        assert_eq!(
            parsed,
            vec![
                ("FOO".to_string(), "bar".to_string()),
                ("BAZ".to_string(), "qux=with=eq".to_string())
            ]
        );
    }

    #[test]
    fn env_file_heredoc() {
        let parsed = parse_env_file("MULTI<<EOF\nline1\nline2\nEOF\nAFTER=ok\n");
        assert_eq!(
            parsed,
            vec![
                ("MULTI".to_string(), "line1\nline2".to_string()),
                ("AFTER".to_string(), "ok".to_string())
            ]
        );
    }

    #[test]
    fn env_file_later_wins() {
        let parsed = parse_env_file("A=1\nB=x\nA=2\n");
        assert_eq!(
            parsed,
            vec![
                ("A".to_string(), "2".to_string()),
                ("B".to_string(), "x".to_string())
            ]
        );
    }

    #[test]
    fn path_file_parsing() {
        assert_eq!(
            parse_path_file("/opt/bin\n\n/usr/local/bin\n"),
            vec!["/opt/bin".to_string(), "/usr/local/bin".to_string()]
        );
    }

    #[test]
    fn file_commands_merge_env_and_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("env"), "FOO=bar\nGITHUB_SHA=bad\n").unwrap();
        std::fs::write(dir.path().join("path"), "/opt/toolbin\n").unwrap();
        let mut env = vec![
            ("GITHUB_SHA".to_string(), "real".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ];
        let mut reserved = std::collections::HashSet::new();
        reserved.insert("GITHUB_SHA".to_string());
        let applied = apply_file_commands(dir.path(), &mut env, &reserved, &ExecMode::Host);
        let map = env_map(&env);
        assert_eq!(map["FOO"], "bar");
        assert_eq!(map["GITHUB_SHA"], "real"); // reserved not overridden
        assert_eq!(map["PATH"], "/opt/toolbin:/usr/bin");
        assert_eq!(applied.warnings.len(), 1);
        assert_eq!(applied.env, vec![("FOO".to_string(), "bar".to_string())]);
        // Files truncated.
        assert_eq!(std::fs::read_to_string(dir.path().join("env")).unwrap(), "");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("path")).unwrap(),
            ""
        );
    }

    /// Docker mode: `$OXIDEAN_PATH` dirs prepend onto the container default
    /// PATH — never the runner host's PATH. Host mode inherits process PATH.
    #[test]
    fn path_merge_base_is_mode_aware() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("path"), "/opt/toolbin\n").unwrap();
        let reserved = std::collections::HashSet::new();

        // Docker: no env PATH entry → container default, not the host's.
        let mut env: Vec<(String, String)> = Vec::new();
        apply_file_commands(
            dir.path(),
            &mut env,
            &reserved,
            &ExecMode::Docker("img".into()),
        );
        assert_eq!(
            env_get(&env, "PATH"),
            Some(format!("/opt/toolbin:{DOCKER_DEFAULT_PATH}").as_str())
        );

        // Host: no env PATH entry → process PATH.
        std::fs::write(dir.path().join("path"), "/opt/toolbin\n").unwrap();
        let mut env: Vec<(String, String)> = Vec::new();
        apply_file_commands(dir.path(), &mut env, &reserved, &ExecMode::Host);
        let process_path = std::env::var("PATH").unwrap_or_default();
        assert_eq!(
            env_get(&env, "PATH"),
            Some(format!("/opt/toolbin:{process_path}").as_str())
        );

        // Subsequent merges accumulate onto the already-merged env PATH.
        std::fs::write(dir.path().join("path"), "/opt/more\n").unwrap();
        let mut env: Vec<(String, String)> = Vec::new();
        env_set(&mut env, "PATH", "/opt/toolbin:/usr/bin".to_string());
        apply_file_commands(
            dir.path(),
            &mut env,
            &reserved,
            &ExecMode::Docker("img".into()),
        );
        assert_eq!(
            env_get(&env, "PATH"),
            Some("/opt/more:/opt/toolbin:/usr/bin")
        );
    }

    #[test]
    fn runner_temp_dir_files() {
        let dir = tempfile::tempdir().unwrap();
        let rt = dir.path().join("_runner").join("j1");
        prepare_runner_temp(&rt, &task()).unwrap();
        for name in ["event.json", "env", "path", "output", "step_summary"] {
            assert!(rt.join(name).exists(), "{name}");
        }
        assert!(rt.join("tool-cache").is_dir());
        let ev: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(rt.join("event.json")).unwrap()).unwrap();
        assert_eq!(ev["event"], "push");
        assert_eq!(ev["sha"], "abc123");
        assert_eq!(ev["repository"], "acme/demo");
        assert_eq!(ev["run_number"], 7);
        assert!(ev["pull_request"].is_null());
        // Re-prepare cleans stale files.
        std::fs::write(rt.join("stale"), b"x").unwrap();
        prepare_runner_temp(&rt, &task()).unwrap();
        assert!(!rt.join("stale").exists());
    }

    #[test]
    fn event_json_pull_request() {
        let dir = tempfile::tempdir().unwrap();
        let rt = dir.path().join("j1");
        let mut t = task();
        t.event = Some("pull_request".into());
        t.pull_number = Some(5);
        t.base_ref = Some("main".into());
        t.head_ref = Some("feat".into());
        prepare_runner_temp(&rt, &t).unwrap();
        let ev: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(rt.join("event.json")).unwrap()).unwrap();
        assert_eq!(ev["pull_request"]["number"], 5);
        assert_eq!(ev["pull_request"]["base_ref"], "main");
        assert_eq!(ev["pull_request"]["head_ref"], "feat");
    }

    #[test]
    fn shell_argv_defaults() {
        // GitHub default shell (unset) is `bash -e {0}` — staged file, no
        // pipefail; explicit `shell: bash` adds `--noprofile --norc` and
        // pipefail.
        let stage = tempfile::tempdir().unwrap();
        let argv = shell_argv(None, "echo hi", stage.path(), "/tmp");
        assert_eq!(argv[..2], ["bash", "-e"]);
        assert!(argv[2].ends_with("_scripts/step-0.sh"));
        assert!(!argv.iter().any(|a| a == "pipefail"));
        let argv = shell_argv(Some("bash"), "echo hi", stage.path(), "/tmp");
        assert_eq!(
            argv[..6],
            ["bash", "--noprofile", "--norc", "-e", "-o", "pipefail"]
        );
        assert!(argv[6].ends_with("_scripts/step-1.sh"));
        let argv = shell_argv(Some("sh"), "x", stage.path(), "/tmp");
        assert_eq!(argv[..2], ["sh", "-e"]);
        assert!(argv[2].ends_with(".sh"));
    }

    #[test]
    fn step_deserialize_matches_api_shape() {
        let v = serde_json::json!([
            {"id": "co", "name": "Checkout", "uses": "actions/checkout@v4", "run": null,
             "if": "always()", "timeout-minutes": 5, "continue-on-error": false, "shell": null,
             "working_directory": null, "with": {"fetch-depth": 1}, "env": null},
            {"name": null, "uses": null, "run": "echo hi", "shell": "sh", "working_directory": "sub", "env": {"A": "1"}}
        ]);
        let steps: Vec<Step> = serde_json::from_value(v).unwrap();
        assert_eq!(steps[0].id.as_deref(), Some("co"));
        assert_eq!(steps[0].uses.as_deref(), Some("actions/checkout@v4"));
        assert_eq!(steps[0].if_expr.as_deref(), Some("always()"));
        assert_eq!(steps[0].with.as_ref().unwrap()["fetch-depth"], 1);
        assert_eq!(steps[1].working_directory.as_deref(), Some("sub"));
    }

    #[test]
    fn glob_match_basics() {
        assert!(glob_match("**/Cargo.toml", "Cargo.toml"));
        assert!(glob_match("**/Cargo.toml", "crates/x/Cargo.toml"));
        assert!(glob_match("src/*.rs", "src/main.rs"));
        assert!(!glob_match("src/*.rs", "src/a/b.rs"));
        assert!(glob_match("src/**/b.rs", "src/a/b.rs"));
        assert!(glob_match("*.txt", "a.txt"));
        assert!(!glob_match("*.txt", "sub/a.txt"));
        assert!(glob_match("a?c", "abc"));
        assert!(glob_match("[a-c]bc", "bbc"));
        assert!(!glob_match("[!a-c]bc", "bbc"));
    }

    #[test]
    fn glob_walk_and_brace_expansion() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src/sub")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), b"fn main()").unwrap();
        std::fs::write(dir.path().join("src/sub/lib.rs"), b"// lib").unwrap();
        std::fs::write(dir.path().join("README.md"), b"# x").unwrap();
        assert_eq!(glob_walk(dir.path(), "src/*.rs").len(), 1);
        assert_eq!(glob_walk(dir.path(), "**/*.rs").len(), 2);
        assert_eq!(glob_walk(dir.path(), "**/*.{rs,md}").len(), 3);
        assert_eq!(glob_walk(dir.path(), "*.{rs,md}").len(), 1);
        assert_eq!(glob_walk(dir.path(), "/README.md").len(), 1);
        assert!(glob_walk(dir.path(), "nope/*.rs").is_empty());
    }

    #[test]
    fn hash_files_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"one").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"two").unwrap();
        let f = hash_files_fn(dir.path().to_path_buf());
        let h1 = f(&["*.txt".into()]).unwrap();
        let h2 = f(&["*.txt".into()]).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // sha256 hex
                                  // Different file set → different hash.
        std::fs::write(dir.path().join("c.txt"), b"three").unwrap();
        let h3 = f(&["*.txt".into()]).unwrap();
        assert_ne!(h1, h3);
        // `!` excludes.
        let excluded = f(&["*.txt".into(), "!b.txt".into()]).unwrap();
        let only_ac = f(&["a.txt".into(), "c.txt".into()]).unwrap();
        assert_eq!(excluded, only_ac);
    }

    #[test]
    fn step_allowed_implicit_success_gate() {
        let mut ctx = Context::new().with_job_status("success");
        // No if → runs only when job is succeeding.
        assert!(step_allowed(None, &ctx).unwrap());
        ctx.set_job_status(JobStatus::Failure);
        assert!(!step_allowed(None, &ctx).unwrap());
        // Plain expr also gated by implicit success().
        assert!(!step_allowed(Some("true"), &ctx).unwrap());
        // Status fn carries its own semantics.
        assert!(step_allowed(Some("failure()"), &ctx).unwrap());
        assert!(step_allowed(Some("always()"), &ctx).unwrap());
        ctx.set_job_status(JobStatus::Success);
        assert!(!step_allowed(Some("failure()"), &ctx).unwrap());
        assert!(step_allowed(Some("true && always()"), &ctx).unwrap());
    }

    #[test]
    fn step_timeout_parsing() {
        let ctx = Context::new().with_context("matrix", serde_json::json!({"t": 2}));
        let mut step = Step {
            continue_on_error: None,
            id: None,
            name: None,
            uses: None,
            run: Some("x".into()),
            if_expr: None,
            timeout_minutes: Some(serde_json::json!(5)),
            shell: None,
            working_directory: None,
            with: None,
            env: None,
        };
        let (t, w) = step_timeout(&step, &ctx, Duration::from_secs(60), None);
        assert_eq!(t, Duration::from_secs(300));
        assert!(w.is_empty());
        step.timeout_minutes = Some(serde_json::json!("${{ matrix.t }}"));
        let (t, _) = step_timeout(&step, &ctx, Duration::from_secs(60), None);
        assert_eq!(t, Duration::from_secs(120));
        step.timeout_minutes = Some(serde_json::json!("bogus"));
        let (t, w) = step_timeout(&step, &ctx, Duration::from_secs(60), None);
        assert_eq!(t, Duration::from_secs(60));
        assert_eq!(w.len(), 1);
        // Job deadline clips.
        let (t, _) = step_timeout(
            &step,
            &ctx,
            Duration::from_secs(60),
            Some(Duration::from_secs(10)),
        );
        assert_eq!(t, Duration::from_secs(10));
    }

    #[test]
    fn interpolate_step_fields() {
        let ctx = Context::new()
            .with_context("github", serde_json::json!({"sha": "deadbeef"}))
            .with_context("env", serde_json::json!({"TAG": "v1"}));
        let step = Step {
            continue_on_error: None,
            id: None,
            name: Some("Build ${{ env.TAG }}".into()),
            uses: None,
            run: Some("echo ${{ github.sha }}".into()),
            if_expr: None,
            timeout_minutes: None,
            shell: None,
            working_directory: Some("dir-${{ env.TAG }}".into()),
            with: None,
            env: Some(serde_json::json!({"OUT": "${{ github.sha }}"})),
        };
        let s = interpolate_step(&step, &ctx).unwrap();
        assert_eq!(s.name.as_deref(), Some("Build v1"));
        assert_eq!(s.run.as_deref(), Some("echo deadbeef"));
        assert_eq!(s.working_directory.as_deref(), Some("dir-v1"));
        assert_eq!(s.env.unwrap()["OUT"], "deadbeef");
    }

    #[test]
    fn workspace_cleaned_between_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("job1");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("stale"), b"x").unwrap();
        prepare_workspace(&ws).unwrap();
        assert!(!ws.join("stale").exists());
        assert!(ws.exists());
    }
}
