//! `actions/checkout` — clone the run's repository into the workspace via the
//! instance's git smart-HTTP, detach at the requested ref, optional submodules.

use std::path::Path;
use std::time::Duration;

use base64::Engine;
use tokio::process::Command;

use oxidean_actions_expr as expr;

use crate::client::Task;
use crate::config::Config;

use super::run_step::run_streamed;
use super::Logger;

/// `actions/checkout[@ref]` — clone the repo into the workspace and detach at
/// the run's head sha. Supported `with:` inputs: `repository`, `ref`, `path`,
/// `fetch-depth`, `submodules`, `persist-credentials` (no-op — our credential
/// rides per-command `http.extraHeader`, never persisted). Everything else is
/// acknowledged with a `::warning::` and ignored.
pub(crate) async fn checkout(
    task: &Task,
    config: &Config,
    workspace: &Path,
    with: &serde_json::Map<String, serde_json::Value>,
    env: &[(String, String)],
    logger: &Logger<'_>,
    timeout: Duration,
) -> Result<(), String> {
    let input = |key: &str| -> Option<String> {
        with.get(key).map(expr::stringify).filter(|s| !s.is_empty())
    };
    // `repository` — `owner/name` override (defaults to the run's repo).
    let (owner, name) = match input("repository") {
        Some(repo) => {
            let mut parts = repo.splitn(2, '/');
            match (parts.next(), parts.next()) {
                (Some(o), Some(n)) => (o.to_string(), n.to_string()),
                _ => return Err(format!("checkout 'repository' '{repo}' is not owner/name")),
            }
        }
        None => match (&task.repository_owner, &task.repository_name) {
            (Some(o), Some(n)) => (o.clone(), n.clone()),
            _ => return Err("task lacks repository coordinates for checkout".into()),
        },
    };
    // Unsupported inputs — acknowledged, not silently dropped.
    for (k, _) in with {
        if !matches!(
            k.as_str(),
            "repository"
                | "ref"
                | "path"
                | "fetch-depth"
                | "submodules"
                | "persist-credentials"
                | "clean"
                | "token"
        ) {
            logger
                .line(&format!(
                    "::warning::actions/checkout input '{k}' not supported — ignored"
                ))
                .await;
        }
    }
    let url = format!("{}/{}/{}.git", config.origin, owner, name);
    let dest = match input("path") {
        Some(p) => {
            let d = workspace.join(&p);
            std::fs::create_dir_all(&d).map_err(|e| format!("checkout path '{p}': {e}"))?;
            d
        }
        None => workspace.to_path_buf(),
    };
    logger.line(&format!("Cloning {owner}/{name}")).await;

    let mut args = checkout_auth_args(task.job_token.as_deref(), config.git_token.as_deref());
    args.push("clone".into());
    // `fetch-depth` — `--depth N` (0/absent = full clone).
    let depth = input("fetch-depth").and_then(|s| s.parse::<u32>().ok());
    if let Some(d) = depth {
        if d > 0 {
            args.push("--depth".into());
            args.push(d.to_string());
        }
    }
    args.extend([url, dest.to_string_lossy().to_string()]);

    let mut cmd = Command::new("git");
    cmd.args(&args).current_dir(workspace);
    let code = run_streamed(&mut cmd, env, logger, timeout).await?;
    if code != 0 {
        return Err(format!("git clone exited {code}"));
    }

    // `ref` — explicit checkout target wins over the run's head sha.
    let target = input("ref").unwrap_or_else(|| task.head_sha.clone().unwrap_or_default());
    if !target.is_empty() {
        let mut cmd = Command::new("git");
        cmd.args(["checkout", &target]).current_dir(&dest);
        let code = run_streamed(&mut cmd, env, logger, timeout).await?;
        if code != 0 {
            return Err(format!("git checkout {target} exited {code}"));
        }
    }
    // `submodules` — `true` → first level, `recursive` → nested too.
    match input("submodules").as_deref() {
        Some("true") | Some("recursive") => {
            let mut args = vec!["submodule", "update", "--init"];
            if input("submodules").as_deref() == Some("recursive") {
                args.push("--recursive");
            }
            let mut cmd = Command::new("git");
            cmd.args(&args).current_dir(&dest);
            let code = run_streamed(&mut cmd, env, logger, timeout).await?;
            if code != 0 {
                return Err(format!("git submodule update exited {code}"));
            }
        }
        _ => {}
    }
    Ok(())
}

/// `git -c http.extraHeader=…` args for clone/fetch auth. The per-job
/// `OXIDEAN_TOKEN` (`task.job_token`) wins — it is repo-locked and minted for
/// exactly this checkout; the runner-wide `OXIDEAN_RUNNER_GIT_TOKEN` is the
/// fallback for tasks minted without a token. The credential rides in
/// `http.extraHeader` — never in the URL or ps-visible args.
pub(crate) fn checkout_auth_args(job_token: Option<&str>, git_token: Option<&str>) -> Vec<String> {
    let token = job_token.filter(|t| !t.is_empty()).or(git_token);
    match token {
        Some(t) => {
            let creds = base64::engine::general_purpose::STANDARD.encode(format!("oauth2:{t}"));
            vec![
                "-c".into(),
                format!("http.extraHeader=Authorization: Basic {creds}"),
            ]
        }
        None => Vec::new(),
    }
}
