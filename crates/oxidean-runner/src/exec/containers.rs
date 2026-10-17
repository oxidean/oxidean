//! `jobs.<id>.container:` / `services:` — Docker-backed job containers.
//!
//! GitHub semantics: one long-lived job container per job (steps run via
//! `docker exec`), sibling service containers on a shared bridge network
//! reachable at `service-name` inside the job container. Workspace mounts at
//! `/workspace`, runner temp at `/__runner_temp` — the same aliases
//! `ExecMode::Docker` uses, so `OXIDEAN_*`/`GITHUB_*` env paths stay correct.
//!
//! Teardown is Drop-based (sync `docker rm -f` / `network rm`) so every
//! `execute()` return path cleans up — cancel, failure, timeout included.

use std::path::Path;
use std::time::Duration;

use serde_json::Value as JsonValue;
use tokio::process::Command;

use super::Logger;
use crate::client::Task;

/// Pull/start operations can take a while on a cold image cache.
const DOCKER_OP_TIMEOUT: Duration = Duration::from_secs(600);

/// Running container set for one job. `job_container` is `None` when the job
/// declares only `services:` (steps stay on the host; services still get the
/// shared network — reachable via mapped ports on localhost).
pub(crate) struct JobContainers {
    network: String,
    job_container: Option<String>,
    service_ids: Vec<String>,
}

/// One parsed `container:`/service spec (object form or `image` shorthand).
#[derive(Debug)]
struct ContainerSpec {
    image: String,
    env: Vec<(String, String)>,
    ports: Vec<String>,
    volumes: Vec<String>,
    options: Vec<String>,
    credentials: Option<(String, String)>,
}

fn parse_spec(v: &JsonValue, what: &str) -> Result<ContainerSpec, String> {
    let mut spec = ContainerSpec {
        image: String::new(),
        env: Vec::new(),
        ports: Vec::new(),
        volumes: Vec::new(),
        options: Vec::new(),
        credentials: None,
    };
    match v {
        JsonValue::String(img) => spec.image = img.clone(),
        JsonValue::Object(m) => {
            spec.image = m
                .get("image")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if let Some(env) = m.get("env").and_then(|v| v.as_object()) {
                for (k, val) in env {
                    spec.env.push((
                        k.clone(),
                        val.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| val.to_string()),
                    ));
                }
            }
            for (key, dst) in [("ports", &mut spec.ports), ("volumes", &mut spec.volumes)] {
                if let Some(arr) = m.get(key).and_then(|v| v.as_array()) {
                    for p in arr {
                        dst.push(
                            p.as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| p.to_string()),
                        );
                    }
                }
            }
            if let Some(opts) = m.get("options").and_then(|v| v.as_str()) {
                // GitHub forwards `options` verbatim to docker create — a
                // whitespace split covers the documented cases; quoted args
                // are not supported by upstream either.
                spec.options = opts.split_whitespace().map(str::to_string).collect();
            }
            if let Some(creds) = m.get("credentials").and_then(|v| v.as_object()) {
                let u = creds.get("username").and_then(|v| v.as_str());
                let p = creds.get("password").and_then(|v| v.as_str());
                if let (Some(u), Some(p)) = (u, p) {
                    spec.credentials = Some((u.to_string(), p.to_string()));
                }
            }
        }
        _ => {}
    }
    if spec.image.is_empty() {
        return Err(format!("{what} spec is missing 'image'"));
    }
    Ok(spec)
}

/// `docker` on PATH + daemon reachable? Loud failure beats silent fallback —
/// a container job that ran on the host would mis-execute.
async fn docker_available() -> bool {
    tokio::time::timeout(
        Duration::from_secs(10),
        Command::new("docker")
            .args(["version", "--format", "{{.Server.Version}}"])
            .output(),
    )
    .await
    .ok()
    .and_then(|r| r.ok())
    .map(|o| o.status.success())
    .unwrap_or(false)
}

async fn docker(args: &[String]) -> Result<std::process::Output, String> {
    let out = tokio::time::timeout(
        DOCKER_OP_TIMEOUT,
        Command::new("docker").args(args).output(),
    )
    .await
    .map_err(|_| {
        format!(
            "docker {} timed out",
            args.first().map(String::as_str).unwrap_or("")
        )
    })?
    .map_err(|e| format!("docker unavailable: {e}"))?;
    Ok(out)
}

/// `docker login` for private registries — password via stdin, never argv.
async fn docker_login(image: &str, creds: &(String, String)) -> Result<(), String> {
    // Registry = first image segment when it looks like a host (has '.'/':'
    // or is localhost); empty → default registry.
    let first = image.split('/').next().unwrap_or_default();
    let registry = if first.contains('.') || first.contains(':') || first == "localhost" {
        first
    } else {
        ""
    };
    let mut cmd = Command::new("docker");
    cmd.arg("login");
    if !registry.is_empty() {
        cmd.arg(registry);
    }
    cmd.args(["-u", &creds.0, "--password-stdin"])
        .stdin(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    use tokio::io::AsyncWriteExt;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(creds.1.as_bytes()).await;
    }
    let out = tokio::time::timeout(Duration::from_secs(60), child.wait_with_output())
        .await
        .map_err(|_| "docker login timed out".to_string())?
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "docker login failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

impl JobContainers {
    pub(crate) fn job_container_id(&self) -> Option<&str> {
        self.job_container.as_deref()
    }

    /// Bring up the network, services, then the job container.
    pub(crate) async fn setup(
        task: &Task,
        workspace: &Path,
        runner_temp: &Path,
        logger: &Logger<'_>,
    ) -> Result<JobContainers, String> {
        if !docker_available().await {
            return Err(
                "job requires 'container:'/'services:' but docker is not available on this runner"
                    .into(),
            );
        }
        let job_id = task.job_id.clone().unwrap_or_else(|| "job".into());
        let tag: String = job_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        let network = format!("oxidean-{tag}");

        // Any failure below must still tear down what started — build the
        // guard up-front and fill ids as they come up.
        let mut guard = JobContainers {
            network: network.clone(),
            job_container: None,
            service_ids: Vec::new(),
        };
        let out = docker(&["network".into(), "create".into(), network.clone()]).await?;
        if !out.status.success() {
            return Err(format!(
                "docker network create failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        logger.line(&format!("Created job network {network}")).await;

        // Services first — the job container joins the same network and can
        // resolve them by `--network-alias`.
        if let Some(svcs) = task.services.as_ref().and_then(|v| v.as_object()) {
            for (name, raw) in svcs {
                let spec = parse_spec(raw, &format!("service '{name}'"))?;
                if let Some(creds) = &spec.credentials {
                    docker_login(&spec.image, creds).await?;
                }
                let svc_name = format!("oxidean-svc-{tag}-{name}");
                let mut args = vec![
                    "run".into(),
                    "-d".into(),
                    "--name".into(),
                    svc_name,
                    "--network".into(),
                    network.clone(),
                    "--network-alias".into(),
                    name.clone(),
                ];
                for (k, v) in &spec.env {
                    args.push("-e".into());
                    args.push(format!("{k}={v}"));
                }
                for p in &spec.ports {
                    args.push("-p".into());
                    args.push(p.clone());
                }
                for v in &spec.volumes {
                    args.push("-v".into());
                    args.push(v.clone());
                }
                args.extend(spec.options.iter().cloned());
                args.push(spec.image.clone());
                let out = docker(&args).await?;
                if !out.status.success() {
                    return Err(format!(
                        "service '{name}' failed to start: {}",
                        String::from_utf8_lossy(&out.stderr)
                    ));
                }
                let cid = String::from_utf8_lossy(&out.stdout).trim().to_string();
                guard.service_ids.push(cid);
                logger
                    .line(&format!("Started service '{name}' ({})", spec.image))
                    .await;
            }
        }

        if let Some(raw) = &task.container {
            let spec = parse_spec(raw, "container")?;
            if let Some(creds) = &spec.credentials {
                docker_login(&spec.image, creds).await?;
            }
            let mut args = vec![
                "run".into(),
                "-d".into(),
                "--name".into(),
                format!("oxidean-job-{tag}"),
                "--network".into(),
                network.clone(),
                "-v".into(),
                format!("{}:/workspace", workspace.to_string_lossy()),
                "-v".into(),
                format!("{}:/__runner_temp", runner_temp.to_string_lossy()),
                "-w".into(),
                "/workspace".into(),
            ];
            for (k, v) in &spec.env {
                args.push("-e".into());
                args.push(format!("{k}={v}"));
            }
            for p in &spec.ports {
                args.push("-p".into());
                args.push(p.clone());
            }
            for v in &spec.volumes {
                args.push("-v".into());
                args.push(v.clone());
            }
            args.extend(spec.options.iter().cloned());
            args.push(spec.image.clone());
            // Keep the container alive; steps arrive via `docker exec`.
            args.extend(["tail".into(), "-f".into(), "/dev/null".into()]);
            let out = docker(&args).await?;
            if !out.status.success() {
                return Err(format!(
                    "job container failed to start: {}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
            let cid = String::from_utf8_lossy(&out.stdout).trim().to_string();
            logger
                .line(&format!("Started job container ({})", spec.image))
                .await;
            guard.job_container = Some(cid);
        }
        Ok(guard)
    }
}

impl Drop for JobContainers {
    fn drop(&mut self) {
        // Sync teardown — Drop can't await; `docker rm -f`/`network rm` are
        // fast and best-effort. Silent on error: teardown must never panic.
        for id in self.service_ids.iter().chain(self.job_container.iter()) {
            let _ = std::process::Command::new("docker")
                .args(["rm", "-f", id])
                .output();
        }
        if self.job_container.is_some() || !self.service_ids.is_empty() {
            let _ = std::process::Command::new("docker")
                .args(["network", "rm", &self.network])
                .output();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_image_shorthand() {
        let s = parse_spec(&serde_json::json!("node:20"), "container").unwrap();
        assert_eq!(s.image, "node:20");
        assert!(s.env.is_empty() && s.ports.is_empty() && s.options.is_empty());
    }

    #[test]
    fn spec_object_form() {
        let s = parse_spec(
            &serde_json::json!({
                "image": "node:20",
                "env": {"A": "1", "B": 2},
                "ports": ["80", "8080:80"],
                "volumes": ["/a:/b"],
                "options": "--cpus 1 --memory 1g",
                "credentials": {"username": "u", "password": "p"},
            }),
            "container",
        )
        .unwrap();
        assert_eq!(s.image, "node:20");
        assert_eq!(s.env.len(), 2);
        assert_eq!(s.ports, vec!["80", "8080:80"]);
        assert_eq!(s.volumes, vec!["/a:/b"]);
        assert_eq!(s.options, vec!["--cpus", "1", "--memory", "1g"]);
        assert_eq!(s.credentials, Some(("u".into(), "p".into())));
    }

    #[test]
    fn spec_missing_image_fails() {
        let e = parse_spec(&serde_json::json!({"env": {}}), "service 'db'").unwrap_err();
        assert!(e.contains("service 'db'") && e.contains("image"));
    }
}
