//! `run:` step execution on the host or inside a Docker container — shell
//! resolution, script-file staging for non-POSIX shells, output streaming.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::config::ExecMode;

use super::Logger;
use super::Step;

/// Resolved shell argv for `run:` scripts. `bash`/`sh` keep the POSIX `-c`
/// form (historical behavior; `$0` stays the shell name). Other shells —
/// `pwsh`, `python`, or a custom template containing `{0}` — run the script
/// from a staged file. `stage_dir` is where the file is written (host path);
/// `visible_dir` is the path the step sees (identical on host, the container
/// mount point under Docker).
pub(crate) fn shell_argv(
    shell: Option<&str>,
    script: &str,
    stage_dir: &Path,
    visible_dir: &str,
) -> Vec<String> {
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
        "pwsh" => {
            let f = stage_script(stage_dir, visible_dir, script, "ps1");
            vec!["pwsh".into(), "-command".into(), format!(". '{f}'")]
        }
        "python" => {
            let f = stage_script(stage_dir, visible_dir, script, "py");
            vec!["python".into(), f]
        }
        custom if custom.contains("{0}") => {
            let f = stage_script(stage_dir, visible_dir, script, "sh");
            custom
                .split_whitespace()
                .map(|tok| tok.replace("{0}", &f))
                .collect()
        }
        other => vec![other.into(), "-c".into(), script.into()],
    }
}

/// Write `script` to `<stage_dir>/_scripts/step-<n>.<ext>` and return the path
/// the step sees (`<visible_dir>/_scripts/…`). Non-POSIX shells read scripts
/// from files, matching GitHub's staged-script behavior.
fn stage_script(stage_dir: &Path, visible_dir: &str, script: &str, ext: &str) -> String {
    let dir = stage_dir.join("_scripts");
    let _ = std::fs::create_dir_all(&dir);
    let n = std::fs::read_dir(&dir).map(|rd| rd.count()).unwrap_or(0);
    let name = format!("step-{n}.{ext}");
    let _ = std::fs::write(dir.join(&name), script);
    format!("{visible_dir}/_scripts/{name}")
}

/// Execute one `run:` step either on the host or inside a Docker container.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_step(
    step: &Step,
    mode: &ExecMode,
    workspace: &Path,
    runner_temp_host: &Path,
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
            let argv = shell_argv(
                step.shell.as_deref(),
                &script,
                runner_temp_host,
                &runner_temp_host.to_string_lossy(),
            );
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
                "-v".into(),
                format!("{}:/__runner_temp", runner_temp_host.to_string_lossy()),
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
            args.extend(shell_argv(
                step.shell.as_deref(),
                &script,
                runner_temp_host,
                "/__runner_temp",
            ));

            let mut cmd = Command::new("docker");
            cmd.args(&args).current_dir(workspace);
            let code = run_streamed(&mut cmd, env, logger, timeout).await?;
            if code != 0 {
                return Err(format!("step exited {code}"));
            }
        }
        ExecMode::ContainerExec(id) => {
            // `docker exec` env: `--env-file` is read host-side by the CLI.
            // Values are already container-visible (/workspace, /__runner_temp
            // aliases); container-config env (from `container.env`) is
            // inherited by exec'd processes automatically.
            let env_file = runner_temp_host.join("container-exec.env");
            let body: String = env
                .iter()
                .filter(|(_, v)| !v.contains('\n'))
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(&env_file, body).map_err(|e| e.to_string())?;
            let workdir = match workdir_rel {
                Some(rel) => format!("/workspace/{rel}"),
                None => "/workspace".into(),
            };
            let argv = shell_argv(
                step.shell.as_deref(),
                &script,
                runner_temp_host,
                "/__runner_temp",
            );
            let mut cmd = Command::new("docker");
            cmd.args([
                "exec".into(),
                "--env-file".into(),
                env_file.to_string_lossy().to_string(),
                "-w".into(),
                workdir,
                id.clone(),
            ]);
            cmd.args(argv);
            let code = run_streamed(&mut cmd, env, logger, timeout).await?;
            if code != 0 {
                return Err(format!("step exited {code}"));
            }
        }
    }
    Ok(())
}

/// Spawn a command, stream combined stdout+stderr to the log, and return the
/// exit code. Applies `timeout`; kills the child on expiry.
pub(crate) async fn run_streamed(
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
