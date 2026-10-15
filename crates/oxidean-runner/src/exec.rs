//! Job execution: `actions/checkout` builtin, `run:` steps on the host or in
//! Docker, log streaming with secret masking.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use base64::Engine;
use serde::Deserialize;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::client::{Client, Task};
use crate::config::{Config, ExecMode};

/// Mirrors `StepSpec` in `oxidean-api::actions::parse`.
#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    pub name: Option<String>,
    pub uses: Option<String>,
    pub run: Option<String>,
    pub shell: Option<String>,
    pub working_directory: Option<String>,
    pub env: Option<serde_json::Value>,
}

/// Secret values are masked out of log chunks before upload.
fn mask_secrets(text: &str, secrets: &HashMap<String, String>) -> String {
    let mut out = text.to_string();
    for v in secrets.values() {
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

fn job_env(
    task: &Task,
    workspace_env: &str,
    secrets: &HashMap<String, String>,
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = vec![
        ("CI".into(), "true".into()),
        ("OXIDEAN".into(), "true".into()),
        ("GITHUB_WORKSPACE".into(), workspace_env.to_string()),
        (
            "GITHUB_SHA".into(),
            task.head_sha.clone().unwrap_or_default(),
        ),
        (
            "GITHUB_REF".into(),
            task.head_ref.clone().unwrap_or_default(),
        ),
        (
            "GITHUB_REPOSITORY".into(),
            format!(
                "{}/{}",
                task.repository_owner.clone().unwrap_or_default(),
                task.repository_name.clone().unwrap_or_default()
            ),
        ),
        (
            "GITHUB_RUN_ID".into(),
            task.run_id.clone().unwrap_or_default(),
        ),
    ];
    for (k, v) in secrets {
        env.push((k.clone(), v.clone()));
    }
    env
}

struct Logger<'a> {
    client: &'a Client,
    run_id: String,
    job_id: String,
    secrets: &'a HashMap<String, String>,
}

impl<'a> Logger<'a> {
    async fn send(&self, chunk: &str) {
        let masked = mask_secrets(chunk, self.secrets);
        if masked.is_empty() {
            return;
        }
        if let Err(e) = self
            .client
            .update_log(&self.run_id, &self.job_id, &masked)
            .await
        {
            tracing::warn!("update_log failed: {e}");
        }
        // Mirror to runner stdout for operator visibility (masked).
        use std::io::Write;
        let _ = std::io::stdout().write_all(masked.as_bytes());
        let _ = std::io::stdout().flush();
    }

    async fn line(&self, msg: &str) {
        self.send(&format!("{msg}\n")).await;
    }
}

/// Spawn a command, stream combined stdout+stderr to the log, and return the
/// exit code. Applies `timeout`; kills the child on expiry.
async fn run_streamed(
    cmd: &mut Command,
    env: &[(String, String)],
    logger: &Logger<'_>,
    timeout: Duration,
) -> Result<i32, String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .envs(env.iter().cloned())
        .spawn()
        .map_err(|e| format!("spawn failed: {e}"))?;

    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let tx_err = tx.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx_err.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Stream chunks until the child exits, then drain. Bounded by timeout.
    let result = tokio::time::timeout(timeout, async {
        let status = loop {
            tokio::select! {
                res = child.wait() => break res,
                chunk = rx.recv() => match chunk {
                    Some(c) => logger.send(&String::from_utf8_lossy(&c)).await,
                    // Pipes closed — wait for exit directly.
                    None => break child.wait().await,
                },
            }
        };
        // Drain remaining output — senders drop when pipes hit EOF, which is
        // guaranteed now that the child has exited.
        while let Some(c) = rx.recv().await {
            logger.send(&String::from_utf8_lossy(&c)).await;
        }
        status
    })
    .await;

    match result {
        Ok(Ok(s)) => Ok(s.code().unwrap_or(-1)),
        Ok(Err(e)) => Err(format!("wait failed: {e}")),
        Err(_) => {
            let _ = child.kill().await;
            logger.line("::error::step timed out").await;
            Err("step timed out".into())
        }
    }
}

/// `actions/checkout[@ref]` — clone the repo into the workspace and detach at
/// the run's head sha.
async fn checkout(
    task: &Task,
    config: &Config,
    workspace: &Path,
    env: &[(String, String)],
    logger: &Logger<'_>,
    timeout: Duration,
) -> Result<(), String> {
    let (owner, name) = match (&task.repository_owner, &task.repository_name) {
        (Some(o), Some(n)) => (o.clone(), n.clone()),
        _ => return Err("task lacks repository coordinates for checkout".into()),
    };
    let url = format!("{}/{}/{}.git", config.origin, owner, name);
    logger.line(&format!("Cloning {owner}/{name}")).await;

    // Token rides in http.extraHeader — never in the URL or ps-visible args.
    let mut args: Vec<String> = Vec::new();
    if let Some(token) = &config.git_token {
        let creds = base64::engine::general_purpose::STANDARD.encode(format!("oauth2:{token}"));
        args.push("-c".into());
        args.push(format!("http.extraHeader=Authorization: Basic {creds}"));
    }
    args.extend(["clone".into(), url, ".".into()]);

    let mut cmd = Command::new("git");
    cmd.args(&args).current_dir(workspace);
    let code = run_streamed(&mut cmd, env, logger, timeout).await?;
    if code != 0 {
        return Err(format!("git clone exited {code}"));
    }

    let sha = task.head_sha.clone().unwrap_or_default();
    if sha.is_empty() {
        return Ok(());
    }
    let mut cmd = Command::new("git");
    cmd.args(["checkout", &sha]).current_dir(workspace);
    let code = run_streamed(&mut cmd, env, logger, timeout).await?;
    if code != 0 {
        return Err(format!("git checkout {sha} exited {code}"));
    }
    Ok(())
}

fn shell_argv(shell: Option<&str>, script: &str) -> Vec<String> {
    match shell.unwrap_or("bash") {
        "bash" => vec![
            "bash".into(),
            "-e".into(),
            "-o".into(),
            "pipefail".into(),
            "-c".into(),
            script.into(),
        ],
        "sh" => vec!["sh".into(), "-e".into(), "-c".into(), script.into()],
        other => vec![other.into(), "-c".into(), script.into()],
    }
}

/// Execute one `run:` step either on the host or inside a Docker container.
async fn run_step(
    step: &Step,
    mode: &ExecMode,
    workspace: &Path,
    workdir_rel: Option<&str>,
    env: &[(String, String)],
    logger: &Logger<'_>,
    timeout: Duration,
) -> Result<(), String> {
    let script = step.run.clone().unwrap_or_default();
    match mode {
        ExecMode::Host => {
            let step_dir = match workdir_rel {
                Some(rel) => workspace.join(rel),
                None => workspace.to_path_buf(),
            };
            let argv = shell_argv(step.shell.as_deref(), &script);
            let mut cmd = Command::new(&argv[0]);
            cmd.args(&argv[1..]).current_dir(&step_dir);
            let code = run_streamed(&mut cmd, env, logger, timeout).await?;
            if code != 0 {
                return Err(format!("step exited {code}"));
            }
        }
        ExecMode::Docker(image) => {
            // `-e KEY` (no value) forwards the value from this process env —
            // keeps secrets out of `ps`.
            let mut args: Vec<String> = vec![
                "run".into(),
                "--rm".into(),
                "-v".into(),
                format!("{}:/workspace", workspace.to_string_lossy()),
            ];
            for (k, _) in env {
                args.push("-e".into());
                args.push(k.clone());
            }
            args.push("-w".into());
            args.push(match workdir_rel {
                Some(rel) => format!("/workspace/{rel}"),
                None => "/workspace".into(),
            });
            args.push(image.clone());
            args.extend(shell_argv(step.shell.as_deref(), &script));

            let mut cmd = Command::new("docker");
            cmd.args(&args).current_dir(workspace);
            let code = run_streamed(&mut cmd, env, logger, timeout).await?;
            if code != 0 {
                return Err(format!("step exited {code}"));
            }
        }
    }
    Ok(())
}

/// Execute a claimed task end to end; reports state transitions on the client.
/// Returns the terminal job state.
pub async fn execute(client: &Client, task: &Task, config: &Config) -> &'static str {
    let job_id = task.job_id.clone().unwrap_or_default();
    let run_id = task.run_id.clone().unwrap_or_default();
    let secrets = task.secrets.clone().unwrap_or_default();
    let logger = Logger {
        client,
        run_id: run_id.clone(),
        job_id: job_id.clone(),
        secrets: &secrets,
    };

    if let Err(e) = client.update_task(&job_id, "in_progress").await {
        tracing::error!("update_task in_progress failed: {e}");
        return "failure";
    }

    let workspace = config.work_dir.join(&job_id);
    if let Err(e) = prepare_workspace(&workspace) {
        logger
            .line(&format!("::error::failed to prepare workspace: {e}"))
            .await;
        let _ = client.update_task(&job_id, "failure").await;
        return "failure";
    }

    let steps: Vec<Step> = task
        .steps
        .clone()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let mode = exec_mode_for(task, config);
    // Inside a container the workspace is mounted at /workspace.
    let workspace_env = match &mode {
        ExecMode::Docker(_) => "/workspace".to_string(),
        ExecMode::Host => workspace.to_string_lossy().to_string(),
    };
    let env = job_env(task, &workspace_env, &secrets);
    let timeout = Duration::from_secs(config.job_timeout_secs);

    logger
        .line(&format!(
            "Job {} — {} / {} step(s) [{}]",
            task.job_key.clone().unwrap_or_default(),
            task.workflow_name.clone().unwrap_or_default(),
            steps.len(),
            match &mode {
                ExecMode::Host => "host".to_string(),
                ExecMode::Docker(img) => format!("docker:{img}"),
            }
        ))
        .await;

    let mut failed = false;
    for (i, step) in steps.iter().enumerate() {
        let title = step
            .name
            .clone()
            .or_else(|| step.uses.clone())
            .or_else(|| step.run.as_ref().map(|r| truncate(r, 60)))
            .unwrap_or_else(|| format!("step {i}"));
        logger.line(&format!("##[step]{title}")).await;

        let result: Result<(), String> = if let Some(uses) = &step.uses {
            if uses.starts_with("actions/checkout") {
                checkout(task, config, &workspace, &env, &logger, timeout).await
            } else {
                Err(format!(
                    "unsupported action '{uses}' — only actions/checkout and run: steps are supported"
                ))
            }
        } else if step.run.is_some() {
            // Step-level env overlays job env.
            let mut step_env = env.clone();
            if let Some(serde_json::Value::Object(map)) = &step.env {
                for (k, v) in map {
                    step_env.push((k.clone(), v.as_str().unwrap_or_default().to_string()));
                }
            }
            run_step(
                step,
                &mode,
                &workspace,
                step.working_directory.as_deref(),
                &step_env,
                &logger,
                timeout,
            )
            .await
        } else {
            Ok(())
        };

        if let Err(e) = result {
            logger.line(&format!("::error::{e}")).await;
            failed = true;
            break;
        }
        logger.line(&format!("##[step-done]{title}")).await;
    }

    let final_state = if failed { "failure" } else { "success" };
    if let Err(e) = client.update_task(&job_id, final_state).await {
        tracing::error!("update_task {final_state} failed: {e}");
    }
    final_state
}

fn prepare_workspace(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())
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
            git_token: None,
            job_timeout_secs: 60,
        }
    }

    #[test]
    fn secrets_masked_in_logs() {
        let mut secrets = HashMap::new();
        secrets.insert("TOKEN".into(), "hunter2".into());
        secrets.insert("EMPTY".into(), "".into());
        assert_eq!(
            mask_secrets("token is hunter2 ok", &secrets),
            "token is *** ok"
        );
        assert_eq!(mask_secrets("nothing", &secrets), "nothing");
    }

    #[test]
    fn exec_mode_matches_declared_label() {
        let task = Task {
            job_id: Some("j".into()),
            run_id: Some("r".into()),
            job_key: None,
            runs_on: Some(vec!["ubuntu-latest".into()]),
            workflow_name: None,
            head_sha: None,
            head_ref: None,
            steps: None,
            secrets: None,
            repository_owner: None,
            repository_name: None,
        };
        assert_eq!(exec_mode_for(&task, &cfg("ubuntu-latest")), ExecMode::Host);
        assert_eq!(
            exec_mode_for(&task, &cfg("ubuntu-latest:docker://node:20")),
            ExecMode::Docker("node:20".into())
        );
        // Unmatched label falls back to host.
        assert_eq!(exec_mode_for(&task, &cfg("other")), ExecMode::Host);
    }

    #[test]
    fn shell_argv_defaults() {
        assert_eq!(
            shell_argv(None, "echo hi"),
            vec!["bash", "-e", "-o", "pipefail", "-c", "echo hi"]
        );
        assert_eq!(shell_argv(Some("sh"), "x"), vec!["sh", "-e", "-c", "x"]);
    }

    #[test]
    fn step_deserialize_matches_api_shape() {
        let v = serde_json::json!([
            {"name": "Checkout", "uses": "actions/checkout@v4", "run": null, "shell": null, "working_directory": null, "with": null, "env": null},
            {"name": null, "uses": null, "run": "echo hi", "shell": "sh", "working_directory": "sub", "env": {"A": "1"}}
        ]);
        let steps: Vec<Step> = serde_json::from_value(v).unwrap();
        assert_eq!(steps[0].uses.as_deref(), Some("actions/checkout@v4"));
        assert_eq!(steps[1].working_directory.as_deref(), Some("sub"));
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
