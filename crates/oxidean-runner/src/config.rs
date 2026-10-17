//! Runner configuration from `OXIDEAN_RUNNER_*` env vars.

use std::path::PathBuf;

/// How a matched `runs-on` label executes job steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecMode {
    /// Run steps directly on the runner host (`sh`/`bash`).
    Host,
    /// Run steps inside `docker run` with the given image.
    Docker(String),
    /// `docker exec` steps into this long-lived job container — set by
    /// `jobs.<id>.container:` provisioning, never by a label.
    ContainerExec(String),
}

/// A declared runner label. Syntax:
/// - `ubuntu-latest` → host execution
/// - `ubuntu-latest:host` → host execution (explicit)
/// - `ubuntu-latest:docker://node:20-bookworm` → Docker execution
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerLabel {
    pub name: String,
    pub mode: ExecMode,
}

impl RunnerLabel {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("empty label".into());
        }
        let (name, spec) = match raw.split_once(':') {
            Some((n, s)) => (n.trim(), s.trim()),
            None => (raw, ""),
        };
        if name.is_empty() {
            return Err(format!("label '{raw}' has an empty name"));
        }
        let mode = if spec.is_empty() || spec == "host" {
            ExecMode::Host
        } else if let Some(image) = spec.strip_prefix("docker://") {
            if image.trim().is_empty() {
                return Err(format!("label '{raw}' has an empty docker image"));
            }
            ExecMode::Docker(image.trim().to_string())
        } else {
            return Err(format!(
                "label '{raw}': unsupported executor spec '{spec}' (want 'host' or 'docker://<image>')"
            ));
        };
        Ok(RunnerLabel {
            name: name.to_string(),
            mode,
        })
    }
}

pub fn parse_labels(csv: &str) -> Result<Vec<RunnerLabel>, String> {
    csv.split(',')
        .map(RunnerLabel::parse)
        .collect::<Result<Vec<_>, _>>()
        .and_then(|v| {
            if v.is_empty() {
                Err("no labels configured".into())
            } else {
                Ok(v)
            }
        })
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Base URL of the Oxidean instance, e.g. `http://localhost` or
    /// `http://oxidean-api.railway.internal` inside a private network.
    pub origin: String,
    /// One-shot registration token (`OXIDEAN_RUNNER_REGISTRATION_TOKEN`).
    /// Only needed until the runner has registered once.
    pub registration_token: Option<String>,
    pub name: String,
    pub labels: Vec<RunnerLabel>,
    /// Persisted registration state (`{runner_id, token}`).
    pub state_path: PathBuf,
    /// Per-job workspaces are created under this directory.
    pub work_dir: PathBuf,
    pub poll_ms: u64,
    /// Server-side `fetch_task` long-poll hold (`OXIDEAN_RUNNER_WAIT_MS`,
    /// default 20 000, capped server-side at 30s). `0` disables long-poll
    /// and returns to pure `poll_ms` cadence.
    pub wait_ms: u64,
    /// Optional PAT used for cloning private repositories over smart HTTP.
    pub git_token: Option<String>,
    pub job_timeout_secs: u64,
    /// Optional instance action mirror — replaces the public `github.com`
    /// fallback in remote `uses:` resolution (`OXIDEAN_ACTIONS_MIRROR`, e.g.
    /// `https://mirror.internal/actions`). The Oxidean origin is always tried
    /// first; when unset, `github.com` remains the fallback.
    pub actions_mirror: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let origin = std::env::var("OXIDEAN_PUBLIC_ORIGIN")
            .map_err(|_| "OXIDEAN_PUBLIC_ORIGIN is required".to_string())?;
        let origin = origin.trim().trim_end_matches('/').to_string();
        if origin.is_empty() {
            return Err("OXIDEAN_PUBLIC_ORIGIN is empty".into());
        }
        let registration_token = std::env::var("OXIDEAN_RUNNER_REGISTRATION_TOKEN")
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        let name = std::env::var("OXIDEAN_RUNNER_NAME")
            .ok()
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| hostname_or("oxidean-runner"));
        let labels_csv =
            std::env::var("OXIDEAN_RUNNER_LABELS").unwrap_or_else(|_| "ubuntu-latest".into());
        let labels =
            parse_labels(&labels_csv).map_err(|e| format!("OXIDEAN_RUNNER_LABELS: {e}"))?;
        let state_path = std::env::var("OXIDEAN_RUNNER_STATE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/data/runner.json"));
        let work_dir = std::env::var("OXIDEAN_RUNNER_WORK_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                state_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("/data"))
                    .join("work")
            });
        let poll_ms = std::env::var("OXIDEAN_RUNNER_POLL_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(2000);
        let wait_ms = std::env::var("OXIDEAN_RUNNER_WAIT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(20_000);
        let git_token = std::env::var("OXIDEAN_RUNNER_GIT_TOKEN")
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        let job_timeout_secs = std::env::var("OXIDEAN_RUNNER_JOB_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3600);
        let actions_mirror = std::env::var("OXIDEAN_ACTIONS_MIRROR")
            .ok()
            .map(|m| m.trim().trim_end_matches('/').to_string())
            .filter(|m| !m.is_empty());
        Ok(Config {
            origin,
            registration_token,
            name,
            labels,
            state_path,
            work_dir,
            poll_ms,
            wait_ms,
            git_token,
            job_timeout_secs,
            actions_mirror,
        })
    }
}

fn hostname_or(fallback: &str) -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|h| !h.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_bare_name_is_host() {
        let l = RunnerLabel::parse("ubuntu-latest").unwrap();
        assert_eq!(l.name, "ubuntu-latest");
        assert_eq!(l.mode, ExecMode::Host);
    }

    #[test]
    fn label_explicit_host() {
        let l = RunnerLabel::parse("self-hosted:host").unwrap();
        assert_eq!(l.name, "self-hosted");
        assert_eq!(l.mode, ExecMode::Host);
    }

    #[test]
    fn label_docker_image() {
        let l = RunnerLabel::parse("ubuntu-latest:docker://node:20-bookworm").unwrap();
        assert_eq!(l.name, "ubuntu-latest");
        assert_eq!(l.mode, ExecMode::Docker("node:20-bookworm".into()));
    }

    #[test]
    fn label_rejects_empty_and_bad_spec() {
        assert!(RunnerLabel::parse("").is_err());
        assert!(RunnerLabel::parse(":host").is_err());
        assert!(RunnerLabel::parse("x:docker://").is_err());
        assert!(RunnerLabel::parse("x:kubernetes://pod").is_err());
    }

    #[test]
    fn parse_labels_csv() {
        let v = parse_labels("ubuntu-latest, self-hosted:docker://alpine:3 ").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].name, "self-hosted");
    }

    #[test]
    fn parse_labels_rejects_blank_list() {
        assert!(parse_labels("   ").is_err());
    }
}
