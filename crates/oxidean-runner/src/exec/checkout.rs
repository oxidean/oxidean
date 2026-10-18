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

    // Never let git prompt — a private `repository:` override without usable
    // credentials would otherwise block the job on a tty read forever.
    let mut git_env: Vec<(String, String)> = env.to_vec();
    git_env.push(("GIT_TERMINAL_PROMPT".into(), "0".into()));
    let env = git_env.as_slice();

    let mut cmd = Command::new("git");
    cmd.args(&args).current_dir(workspace);
    let code = run_streamed(&mut cmd, env, logger, timeout).await?;
    if code != 0 {
        return Err(format!("git clone exited {code}"));
    }

    // `ref` — explicit checkout target wins over the run's head sha.
    let ref_override = input("ref");
    let target = ref_override
        .clone()
        .unwrap_or_else(|| task.head_sha.clone().unwrap_or_default());
    if !target.is_empty() {
        let mut cmd = Command::new("git");
        cmd.args(["checkout", &target]).current_dir(&dest);
        let code = run_streamed(&mut cmd, env, logger, timeout).await?;
        if code != 0 {
            // `git clone` only fetches heads + tags: `pull_request` runs pin a
            // test-merge commit living on `refs/pull/N/{merge,head}`, and
            // `with: ref` may name a ref clone skipped or a sha (needs
            // `uploadpack.allowReachableSHA1InWant` server-side). Fetch the
            // candidates in order — explicit ref, head ref, the raw target —
            // and detach once the object lands.
            let mut specs: Vec<String> = Vec::new();
            if let Some(r) = ref_override {
                specs.push(r);
            }
            if let Some(r) = task.head_ref.as_deref() {
                if r != target && !specs.iter().any(|s| s == r) {
                    specs.push(r.to_string());
                }
            }
            if !specs.iter().any(|s| s == &target) {
                specs.push(target.clone());
            }
            let mut checked_out = false;
            for spec in specs {
                let mut args =
                    checkout_auth_args(task.job_token.as_deref(), config.git_token.as_deref());
                args.push("fetch".into());
                if let Some(d) = depth {
                    if d > 0 {
                        args.push("--depth".into());
                        args.push(d.to_string());
                    }
                }
                args.extend(["origin".into(), spec]);
                let mut cmd = Command::new("git");
                cmd.args(&args).current_dir(&dest);
                if run_streamed(&mut cmd, env, logger, timeout).await? != 0 {
                    continue;
                }
                // A ref-spec fetch only sets FETCH_HEAD; a sha fetch leaves the
                // object for `checkout <target>` to find. Try both.
                for cand in [target.as_str(), "FETCH_HEAD"] {
                    let mut cmd = Command::new("git");
                    cmd.args(["checkout", cand]).current_dir(&dest);
                    if run_streamed(&mut cmd, env, logger, timeout).await? == 0 {
                        checked_out = true;
                        break;
                    }
                }
                if checked_out {
                    break;
                }
            }
            if !checked_out {
                return Err(format!("git checkout {target} exited {code}"));
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{Client, Task};
    use crate::exec::Logger;
    use std::path::PathBuf;
    use std::process::Command as SyncCommand;
    use std::sync::atomic::{AtomicBool, AtomicI64};
    use std::sync::Mutex;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = SyncCommand::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("spawn git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn base_task() -> Task {
        Task {
            job_id: None,
            run_id: None,
            job_key: None,
            runs_on: None,
            workflow_name: None,
            head_sha: None,
            head_ref: None,
            steps: None,
            secrets: None,
            repository_owner: None,
            repository_name: None,
            event: None,
            run_number: None,
            run_attempt: None,
            actor: None,
            actor_id: None,
            repository_id: None,
            repository_owner_id: None,
            server_url: None,
            base_ref: None,
            pull_number: None,
            job_token: None,
            workflow_ref: None,
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
            id_token_request_url: None,
            id_token_request_token: None,
            cache_url: None,
            cache_token: None,
        }
    }

    /// `file://` origin — a plain local path would clone with hardlinks and
    /// copy every object (defeating the point); `file://` forces upload-pack,
    /// which only serves advertised refs + allowed shas like real smart-HTTP.
    fn test_config(origin: &Path) -> Config {
        Config {
            origin: format!("file://{}", origin.display()),
            registration_token: None,
            name: "t".into(),
            labels: vec![],
            state_path: PathBuf::from("/nonexistent"),
            work_dir: PathBuf::from("/nonexistent"),
            poll_ms: 0,
            wait_ms: 0,
            git_token: None,
            job_timeout_secs: 60,
            actions_mirror: None,
        }
    }

    fn test_logger<'a>(client: &'a Client) -> Logger<'a> {
        Logger {
            client,
            run_id: "r".into(),
            job_id: "j".into(),
            mask_values: &[],
            cancelled: AtomicBool::new(false),
            log_cursor: AtomicI64::new(0),
            seen_cmd_warns: Mutex::new(std::collections::BTreeSet::new()),
        }
    }

    /// Bare remote holding `main` plus a test-merge commit reachable only via
    /// `refs/pull/1/merge` — the `pull_request` run shape that broke checkout.
    fn seed_pull_merge_remote(base: &Path) -> (PathBuf, String) {
        let remote = base.join("o").join("remote.git");
        std::fs::create_dir_all(remote.parent().unwrap()).unwrap();
        git(base, &["init", "--bare", remote.to_str().unwrap()]);

        let work = base.join("seed");
        git(base, &["init", work.to_str().unwrap()]);
        git(&work, &["config", "user.email", "t@t"]);
        git(&work, &["config", "user.name", "t"]);
        std::fs::write(work.join("main.txt"), "main\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-m", "main"]);
        let main_tip = git(&work, &["rev-parse", "HEAD"]);
        std::fs::write(work.join("pr.txt"), "pr\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-m", "pr head"]);
        let pr_tip = git(&work, &["rev-parse", "HEAD"]);
        let tree = git(&work, &["rev-parse", "HEAD^{tree}"]);
        let merge = git(
            &work,
            &[
                "commit-tree",
                &tree,
                "-p",
                &main_tip,
                "-p",
                &pr_tip,
                "-m",
                "merge",
            ],
        );
        let remote_s = remote.to_string_lossy().to_string();
        let main_spec = format!("{main_tip}:refs/heads/main");
        git(&work, &["push", &remote_s, &main_spec]);
        let merge_spec = format!("{merge}:refs/pull/1/merge");
        git(&work, &["push", &remote_s, &merge_spec]);
        // Same enforcement the HTTP/SSH layer applies: sha-in-want only for
        // commits reachable from a ref tip.
        git(
            &remote,
            &["config", "uploadpack.allowReachableSHA1InWant", "true"],
        );
        (remote, merge)
    }

    #[tokio::test]
    async fn checkout_pull_merge_ref_not_in_default_clone() {
        let base = tempfile::tempdir().unwrap();
        let (_remote, merge) = seed_pull_merge_remote(base.path());
        let client = Client::new("", None);
        let logger = test_logger(&client);
        let config = test_config(base.path());
        let ws = base.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();

        let task = Task {
            head_sha: Some(merge.clone()),
            head_ref: Some("refs/pull/1/merge".into()),
            repository_owner: Some("o".into()),
            repository_name: Some("remote".into()),
            ..base_task()
        };
        checkout(
            &task,
            &config,
            &ws,
            &serde_json::Map::new(),
            &[],
            &logger,
            Duration::from_secs(60),
        )
        .await
        .expect("checkout must fetch the pull merge ref and detach at the sha");
        assert_eq!(git(&ws, &["rev-parse", "HEAD"]), merge);
        assert!(ws.join("pr.txt").exists(), "merge tree materialized");
    }

    #[tokio::test]
    async fn checkout_ref_override_fetches_named_ref() {
        let base = tempfile::tempdir().unwrap();
        let (_remote, merge) = seed_pull_merge_remote(base.path());
        let client = Client::new("", None);
        let logger = test_logger(&client);
        let config = test_config(base.path());
        let ws = base.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();

        let mut with = serde_json::Map::new();
        with.insert("ref".into(), "refs/pull/1/merge".into());
        let task = Task {
            repository_owner: Some("o".into()),
            repository_name: Some("remote".into()),
            ..base_task()
        };
        checkout(
            &task,
            &config,
            &ws,
            &with,
            &[],
            &logger,
            Duration::from_secs(60),
        )
        .await
        .expect("with: ref must fetch the named ref and detach at FETCH_HEAD");
        assert_eq!(git(&ws, &["rev-parse", "HEAD"]), merge);
    }
}
