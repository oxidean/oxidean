//! Runner-side expression context: `github`/`env`/`runner`/`matrix`/`secrets`/
//! `needs`/`steps` construction, `if:` gating, and timeout bookkeeping.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use expr::{Context, JobStatus};
use oxidean_actions_expr as expr;

use crate::client::Task;
use crate::config::Config;

use super::env::{runner_arch, runner_os};
use super::glob::hash_files_fn;
use super::Step;

/// Build the runner-side expression context for the claimed task.
pub(crate) fn runner_context(
    task: &Task,
    config: &Config,
    workspace: &Path,
    workspace_env: &str,
    runner_temp_env: &str,
    secrets: &HashMap<String, String>,
) -> Context {
    let head_ref = task.head_ref.clone().unwrap_or_default();
    let ref_name = head_ref
        .strip_prefix("refs/heads/")
        .or_else(|| head_ref.strip_prefix("refs/tags/"))
        .unwrap_or(&head_ref)
        .to_string();
    let server_url = task.server_url.clone().unwrap_or_default();
    let is_pr = task.event.as_deref() == Some("pull_request");
    let github = serde_json::json!({
        "sha": task.head_sha.clone().unwrap_or_default(),
        "ref": head_ref,
        "ref_name": ref_name,
        "ref_type": if head_ref.starts_with("refs/tags/") { "tag" } else { "branch" },
        "event_name": task.event.clone().unwrap_or_default(),
        "event": task.event_payload.clone().unwrap_or_else(|| serde_json::json!({})),
        "repository": format!("{}/{}",
            task.repository_owner.clone().unwrap_or_default(),
            task.repository_name.clone().unwrap_or_default()),
        "repository_owner": task.repository_owner.clone().unwrap_or_default(),
        "repository_id": task.repository_id.clone().unwrap_or_default(),
        "repository_owner_id": task.repository_owner_id.clone().unwrap_or_default(),
        "actor": task.actor.clone().unwrap_or_default(),
        "actor_id": task.actor_id.clone().unwrap_or_default(),
        "workflow": task.workflow_name.clone().unwrap_or_default(),
        "workflow_ref": task.workflow_ref.clone().unwrap_or_default(),
        "workflow_sha": task.head_sha.clone().unwrap_or_default(),
        "job": task.job_key.clone().unwrap_or_default(),
        "run_id": task.run_id.clone().unwrap_or_default(),
        "run_number": task.run_number.unwrap_or_default(),
        "run_attempt": task.run_attempt.unwrap_or_default(),
        "server_url": server_url,
        "api_url": if server_url.is_empty() { String::new() } else { format!("{server_url}/api") },
        "base_ref": if is_pr { task.base_ref.clone().unwrap_or_default() } else { String::new() },
        "head_ref": if is_pr { ref_name } else { String::new() },
        "workspace": workspace_env,
        "token": task.job_token.clone().unwrap_or_default(),
    });
    Context::new()
        .with_context("github", github)
        .with_context("env", serde_json::json!({}))
        .with_context(
            "runner",
            serde_json::json!({
                "os": runner_os(),
                "arch": runner_arch(),
                "name": config.name,
                "temp": runner_temp_env,
                "tool_cache": format!("{runner_temp_env}/tool-cache"),
            }),
        )
        .with_context(
            "matrix",
            task.matrix.clone().unwrap_or_else(|| serde_json::json!({})),
        )
        .with_context(
            "secrets",
            serde_json::to_value(secrets).unwrap_or_else(|_| serde_json::json!({})),
        )
        .with_context(
            "needs",
            task.needs.clone().unwrap_or_else(|| serde_json::json!({})),
        )
        .with_context("steps", serde_json::json!({}))
        // `inputs` mirrors `github.event.inputs` — workflow_dispatch inputs
        // and reusable-call `with:` values both land there server-side.
        .with_context(
            "inputs",
            task.event_payload
                .as_ref()
                .and_then(|e| e.get("inputs").cloned())
                .unwrap_or_else(|| serde_json::json!({})),
        )
        .with_context(
            "vars",
            task.vars
                .clone()
                .map(|m| serde_json::to_value(m).unwrap_or_else(|_| serde_json::json!({})))
                .unwrap_or_else(|| serde_json::json!({})),
        )
        .with_context("strategy", serde_json::json!({}))
        .with_context(
            // `jobs.<id>.environment` — `{name, url}` for job expressions.
            "environment",
            match &task.environment {
                Some(name) => serde_json::json!({
                    "name": name,
                    "url": task.environment_url.clone().unwrap_or_default(),
                }),
                None => serde_json::json!({}),
            },
        )
        .with_context("job", serde_json::json!({"status": "success"}))
        .with_job_status("success")
        .with_hash_files(hash_files_fn(workspace.to_path_buf()))
}

/// Interpolate every expression-bearing field of a step against `ctx`.
pub(crate) fn interpolate_step(step: &Step, ctx: &Context) -> Result<Step, String> {
    let mut s = step.clone();
    let interp = |v: &Option<String>| -> Result<Option<String>, String> {
        match v {
            Some(raw) => expr::interpolate(raw, ctx)
                .map(Some)
                .map_err(|e| e.to_string()),
            None => Ok(None),
        }
    };
    s.name = interp(&s.name)?;
    s.uses = interp(&s.uses)?;
    s.run = interp(&s.run)?;
    s.shell = interp(&s.shell)?;
    s.working_directory = interp(&s.working_directory)?;
    if let Some(v) = &s.env {
        s.env = Some(expr::eval_json(v, ctx).map_err(|e| e.to_string())?);
    }
    if let Some(v) = &s.with {
        s.with = Some(expr::eval_json(v, ctx).map_err(|e| e.to_string())?);
    }
    Ok(s)
}

/// GitHub's `if:` gate for steps: absent → implicit `success()`; present
/// without a status fn → `success() && expr`; present with a status fn → the
/// expression carries the semantics itself.
pub(crate) fn step_allowed(src: Option<&str>, ctx: &Context) -> Result<bool, String> {
    match src {
        None => Ok(ctx.job_status() == JobStatus::Success),
        Some(s) if expr::uses_status_fn(s) => expr::eval_bool(s, ctx).map_err(|e| e.to_string()),
        Some(s) => {
            if ctx.job_status() != JobStatus::Success {
                return Ok(false);
            }
            expr::eval_bool(s, ctx).map_err(|e| e.to_string())
        }
    }
}

/// Record a finished step into `steps.<id>` for `steps.*` expressions and
/// `$GITHUB_OUTPUT` consumers (`needs` lands with the scheduler phase).
/// `outcome` is the step's raw result; `conclusion` is the normalized job
/// effect — GitHub reports `conclusion: success` for a `continue-on-error`
/// failure while `outcome` stays `failure`.
pub(crate) fn record_step(
    ctx: &mut Context,
    id: &str,
    outcome: &str,
    conclusion: &str,
    outputs: HashMap<String, String>,
) {
    let mut steps = ctx
        .get_context("steps")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = steps.as_object_mut() {
        obj.insert(
            id.to_string(),
            serde_json::json!({
                "outcome": outcome,
                "conclusion": conclusion,
                "outputs": outputs,
            }),
        );
    }
    ctx.set_context("steps", steps);
}

/// Keep `job.status` in the context in sync with the runner's JobStatus.
pub(crate) fn sync_job_ctx(ctx: &mut Context) {
    let status = match ctx.job_status() {
        JobStatus::Success => "success",
        JobStatus::Failure => "failure",
        JobStatus::Cancelled => "cancelled",
        JobStatus::Skipped => "skipped",
        JobStatus::Pending => "pending",
    };
    ctx.set_context("job", serde_json::json!({"status": status}));
}

/// Step timeout: `timeout-minutes` (evaluated) else the runner-wide cap,
/// clipped by the job deadline when present.
pub(crate) fn step_timeout(
    step: &Step,
    ctx: &Context,
    default: Duration,
    job_remaining: Option<Duration>,
) -> (Duration, Vec<String>) {
    let mut warnings = Vec::new();
    let mut t = default;
    if let Some(v) = &step.timeout_minutes {
        let minutes = match v {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => expr::eval_embedded(s, ctx)
                .ok()
                .and_then(|v| v.as_f64())
                .or_else(|| s.parse::<f64>().ok()),
            _ => None,
        };
        match minutes {
            Some(m) if m > 0.0 => t = Duration::from_secs_f64(m * 60.0),
            _ => warnings.push(format!(
                "::warning::timeout-minutes '{v}' is not a positive number — using job default"
            )),
        }
    }
    if let Some(rem) = job_remaining {
        t = t.min(rem);
    }
    (t, warnings)
}

/// Seconds remaining until the job deadline (from `timeout_seconds` at
/// claim + elapsed runtime).
pub(crate) struct JobDeadline(Option<Instant>);

impl JobDeadline {
    pub(crate) fn new(timeout_seconds: Option<i64>) -> Self {
        Self(timeout_seconds.map(|s| Instant::now() + Duration::from_secs(s.max(0) as u64)))
    }
    pub(crate) fn remaining(&self) -> Option<Duration> {
        self.0.map(|d| d.saturating_duration_since(Instant::now()))
    }
    pub(crate) fn expired(&self) -> bool {
        matches!(self.remaining(), Some(d) if d.is_zero())
    }
}

/// `{step_id: {name: value}}` from the `steps` context — the shape
/// `update_task.outputs` and `needs.<id>.outputs.*` consume.
pub(crate) fn step_outputs_report(ctx: &Context) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    if let Some(serde_json::Value::Object(steps)) = ctx.get_context("steps") {
        for (id, entry) in steps {
            if let Some(outputs) = entry.get("outputs") {
                out.insert(id.clone(), outputs.clone());
            }
        }
    }
    serde_json::Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_task() -> Task {
        Task {
            job_id: Some("j1".into()),
            run_id: Some("r1".into()),
            job_key: Some("build".into()),
            runs_on: None,
            workflow_name: Some("CI".into()),
            head_sha: Some("abc123".into()),
            head_ref: Some("refs/heads/main".into()),
            steps: None,
            secrets: None,
            repository_owner: Some("acme".into()),
            repository_name: Some("demo".into()),
            repository_id: Some("repo-1".into()),
            repository_owner_id: Some("u1".into()),
            event: Some("push".into()),
            run_number: Some(4),
            run_attempt: Some(1),
            actor: Some("alice".into()),
            actor_id: Some("u1".into()),
            server_url: Some("https://oxidean.example".into()),
            base_ref: None,
            pull_number: None,
            job_token: Some("oxidean_job_x".into()),
            workflow_ref: Some("acme/demo/.github/workflows/ci.yml@refs/heads/main".into()),
            env: None,
            workflow_env: None,
            timeout_at: None,
            timeout_seconds: None,
            event_payload: None,
            matrix: Some(serde_json::json!({"os": "linux"})),
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

    fn test_config() -> Config {
        Config {
            origin: "http://localhost".into(),
            registration_token: None,
            name: "runner-1".into(),
            labels: Vec::new(),
            state_path: std::path::PathBuf::from("/tmp/x"),
            work_dir: std::path::PathBuf::from("/tmp/xwork"),
            poll_ms: 1,
            wait_ms: 0,
            git_token: None,
            job_timeout_secs: 60,
            actions_mirror: None,
        }
    }

    #[test]
    fn runner_context_exposes_github_runner_and_job() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = test_config();
        let ctx = runner_context(
            &test_task(),
            &cfg,
            dir.path(),
            "/ws",
            "/tmp/rt",
            &HashMap::new(),
        );
        let gh = ctx.get_context("github").unwrap();
        assert_eq!(gh["sha"], "abc123");
        assert_eq!(gh["ref"], "refs/heads/main");
        assert_eq!(gh["ref_name"], "main");
        assert_eq!(gh["ref_type"], "branch");
        assert_eq!(gh["repository"], "acme/demo");
        assert_eq!(gh["actor"], "alice");
        assert_eq!(gh["workflow"], "CI");
        assert_eq!(gh["job"], "build");
        assert_eq!(gh["server_url"], "https://oxidean.example");
        assert_eq!(gh["api_url"], "https://oxidean.example/api");
        assert_eq!(gh["run_number"], 4);
        let runner = ctx.get_context("runner").unwrap();
        assert_eq!(runner["name"], "runner-1");
        assert_eq!(runner["temp"], "/tmp/rt");
        assert_eq!(runner["tool_cache"], "/tmp/rt/tool-cache");
        let job = ctx.get_context("job").unwrap();
        assert_eq!(job["status"], "success");
        assert_eq!(ctx.get_context("matrix").unwrap()["os"], "linux");
        // `vars` and `inputs` exist but are empty — pinned divergences.
        assert_eq!(ctx.get_context("vars").unwrap(), &serde_json::json!({}));
        assert_eq!(ctx.get_context("inputs").unwrap(), &serde_json::json!({}));
    }
}
