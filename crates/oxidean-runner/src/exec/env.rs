//! Environment construction and file commands: `OXIDEAN_*`/`GITHUB_*` pairs,
//! workflow→job→step env layering, and the `$GITHUB_ENV`/`$GITHUB_PATH`/
//! `$GITHUB_OUTPUT` file protocol.

use std::collections::HashMap;
use std::path::Path;

use expr::Context;
use oxidean_actions_expr as expr;

use crate::client::Task;
use crate::config::{Config, ExecMode};

/// The `OXIDEAN_*`/`GITHUB_*` variable pairs, in emission order. Each row
/// produces the Oxidean name immediately followed by its GitHub alias carrying
/// the identical value — this list is the single source of truth so the two
/// namespaces cannot drift. `OXIDEAN_ENV_FILE`/`GITHUB_ENV` is the one
/// asymmetric pair: bare `OXIDEAN_ENV` is reserved by the server's runtime-mode
/// config (`development`/`production`), so the file-command variable gets a
/// distinct name while the GitHub alias keeps GitHub's exact spelling.
pub(crate) const PAIRED_VARS: &[(&str, &str)] = &[
    ("OXIDEAN_ACTIONS", "GITHUB_ACTIONS"),
    ("OXIDEAN_ACTOR", "GITHUB_ACTOR"),
    ("OXIDEAN_ACTOR_ID", "GITHUB_ACTOR_ID"),
    ("OXIDEAN_API_URL", "GITHUB_API_URL"),
    ("OXIDEAN_SERVER_URL", "GITHUB_SERVER_URL"),
    ("OXIDEAN_BASE_REF", "GITHUB_BASE_REF"),
    ("OXIDEAN_HEAD_REF", "GITHUB_HEAD_REF"),
    ("OXIDEAN_EVENT_NAME", "GITHUB_EVENT_NAME"),
    ("OXIDEAN_EVENT_PATH", "GITHUB_EVENT_PATH"),
    ("OXIDEAN_JOB", "GITHUB_JOB"),
    ("OXIDEAN_REF", "GITHUB_REF"),
    ("OXIDEAN_REF_NAME", "GITHUB_REF_NAME"),
    ("OXIDEAN_REF_TYPE", "GITHUB_REF_TYPE"),
    ("OXIDEAN_REPOSITORY", "GITHUB_REPOSITORY"),
    ("OXIDEAN_REPOSITORY_ID", "GITHUB_REPOSITORY_ID"),
    ("OXIDEAN_REPOSITORY_OWNER", "GITHUB_REPOSITORY_OWNER"),
    ("OXIDEAN_REPOSITORY_OWNER_ID", "GITHUB_REPOSITORY_OWNER_ID"),
    ("OXIDEAN_RUN_ID", "GITHUB_RUN_ID"),
    ("OXIDEAN_RUN_NUMBER", "GITHUB_RUN_NUMBER"),
    ("OXIDEAN_RUN_ATTEMPT", "GITHUB_RUN_ATTEMPT"),
    ("OXIDEAN_SHA", "GITHUB_SHA"),
    ("OXIDEAN_WORKFLOW", "GITHUB_WORKFLOW"),
    ("OXIDEAN_WORKFLOW_REF", "GITHUB_WORKFLOW_REF"),
    ("OXIDEAN_WORKFLOW_SHA", "GITHUB_WORKFLOW_SHA"),
    ("OXIDEAN_WORKSPACE", "GITHUB_WORKSPACE"),
    ("OXIDEAN_TOKEN", "GITHUB_TOKEN"),
    ("OXIDEAN_ENV_FILE", "GITHUB_ENV"),
    ("OXIDEAN_PATH", "GITHUB_PATH"),
    ("OXIDEAN_OUTPUT", "GITHUB_OUTPUT"),
    ("OXIDEAN_STEP_SUMMARY", "GITHUB_STEP_SUMMARY"),
    ("OXIDEAN_STATE", "GITHUB_STATE"),
];

/// Result of [`job_env`]: the ordered variable list, the set of reserved names
/// (for secret/env-file collision checks), and collision warnings to log.
pub(crate) struct JobEnv {
    pub vars: Vec<(String, String)>,
    pub reserved: std::collections::HashSet<String>,
    pub warnings: Vec<String>,
}

/// GitHub-style `RUNNER_OS` spelling.
pub(crate) fn runner_os() -> String {
    match std::env::consts::OS {
        "linux" => "Linux".into(),
        "macos" => "macOS".into(),
        "windows" => "Windows".into(),
        other => other.into(),
    }
}

/// GitHub-style `RUNNER_ARCH` spelling (X64/ARM64/…).
pub(crate) fn runner_arch() -> String {
    match std::env::consts::ARCH {
        "x86_64" => "X64".into(),
        "aarch64" => "ARM64".into(),
        "x86" => "X86".into(),
        "arm" => "ARM".into(),
        other => other.to_uppercase(),
    }
}

pub(crate) fn env_set(env: &mut Vec<(String, String)>, key: &str, value: String) {
    if let Some(entry) = env.iter_mut().find(|(k, _)| k == key) {
        entry.1 = value;
    } else {
        env.push((key.to_string(), value));
    }
}

pub(crate) fn env_get<'a>(env: &'a [(String, String)], key: &str) -> Option<&'a str> {
    env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// The default job environment: `CI`/`OXIDEAN`, each `OXIDEAN_<X>` paired with
/// its `GITHUB_<X>` alias, `RUNNER_*` metadata, then repo secrets (which may
/// never override the reserved names above).
///
/// `workspace_env` and `runner_temp_env` are the paths visible *inside the
/// step*: host paths in host mode, `/workspace` / `/__runner_temp` in Docker.
pub(crate) fn job_env(
    task: &Task,
    config: &Config,
    workspace_env: &str,
    runner_temp_env: &str,
    secrets: &HashMap<String, String>,
) -> JobEnv {
    let head_ref = task.head_ref.clone().unwrap_or_default();
    let head_sha = task.head_sha.clone().unwrap_or_default();
    let ref_name = head_ref
        .strip_prefix("refs/heads/")
        .or_else(|| head_ref.strip_prefix("refs/tags/"))
        .unwrap_or(&head_ref)
        .to_string();
    let ref_type = if head_ref.starts_with("refs/tags/") {
        "tag"
    } else {
        "branch"
    };
    let repository = format!(
        "{}/{}",
        task.repository_owner.clone().unwrap_or_default(),
        task.repository_name.clone().unwrap_or_default()
    );
    let server_url = task.server_url.clone().unwrap_or_default();
    let is_pr = task.event.as_deref() == Some("pull_request");
    let file = |name: &str| format!("{runner_temp_env}/{name}");

    let paired = |suffix: &str| -> String {
        match suffix {
            "ACTIONS" => "true".into(),
            "ACTOR" => task.actor.clone().unwrap_or_default(),
            "ACTOR_ID" => task.actor_id.clone().unwrap_or_default(),
            "API_URL" => {
                if server_url.is_empty() {
                    String::new()
                } else {
                    format!("{server_url}/api")
                }
            }
            "SERVER_URL" => server_url.clone(),
            "BASE_REF" => {
                if is_pr {
                    task.base_ref.clone().unwrap_or_default()
                } else {
                    String::new()
                }
            }
            // HEAD_REF is the short source branch on pull_request events only.
            "HEAD_REF" => {
                if is_pr {
                    ref_name.clone()
                } else {
                    String::new()
                }
            }
            "EVENT_NAME" => task.event.clone().unwrap_or_default(),
            "EVENT_PATH" => file("event.json"),
            "JOB" => task.job_key.clone().unwrap_or_default(),
            "REF" => head_ref.clone(),
            "REF_NAME" => ref_name.clone(),
            "REF_TYPE" => ref_type.into(),
            "REPOSITORY" => repository.clone(),
            "REPOSITORY_ID" => task.repository_id.clone().unwrap_or_default(),
            "REPOSITORY_OWNER" => task.repository_owner.clone().unwrap_or_default(),
            "REPOSITORY_OWNER_ID" => task.repository_owner_id.clone().unwrap_or_default(),
            "RUN_ID" => task.run_id.clone().unwrap_or_default(),
            "RUN_NUMBER" => task.run_number.map(|n| n.to_string()).unwrap_or_default(),
            "RUN_ATTEMPT" => task.run_attempt.map(|n| n.to_string()).unwrap_or_default(),
            "SHA" => head_sha.clone(),
            "WORKFLOW" => task.workflow_name.clone().unwrap_or_default(),
            "WORKFLOW_REF" => task.workflow_ref.clone().unwrap_or_default(),
            "WORKFLOW_SHA" => head_sha.clone(),
            "WORKSPACE" => workspace_env.to_string(),
            "TOKEN" => task.job_token.clone().unwrap_or_default(),
            // `OXIDEAN_ENV` is reserved by the server config — the env-file
            // command lives under `OXIDEAN_ENV_FILE` (GITHUB_ENV alias).
            "ENV_FILE" => file("env"),
            "PATH" => file("path"),
            "OUTPUT" => file("output"),
            "STEP_SUMMARY" => file("step_summary"),
            "STATE" => file("state"),
            _ => String::new(),
        }
    };

    let mut vars: Vec<(String, String)> = vec![
        ("CI".into(), "true".into()),
        ("OXIDEAN".into(), "true".into()),
    ];
    for (ox_name, gh_name) in PAIRED_VARS {
        let suffix = ox_name.strip_prefix("OXIDEAN_").unwrap_or(ox_name);
        let value = paired(suffix);
        vars.push((ox_name.to_string(), value.clone()));
        vars.push((gh_name.to_string(), value));
    }
    vars.extend([
        ("RUNNER_OS".into(), runner_os()),
        ("RUNNER_ARCH".into(), runner_arch()),
        ("RUNNER_NAME".into(), config.name.clone()),
        ("RUNNER_TEMP".into(), runner_temp_env.to_string()),
        (
            "RUNNER_TOOL_CACHE".into(),
            format!("{runner_temp_env}/tool-cache"),
        ),
    ]);

    // `ACTIONS_ID_TOKEN_REQUEST_*` — unpaired (already ACTIONS-namespaced);
    // present only when `id-token: write` survived provenance gating.
    if let (Some(url), Some(tok)) = (
        task.id_token_request_url.as_ref(),
        task.id_token_request_token.as_ref(),
    ) {
        vars.push(("ACTIONS_ID_TOKEN_REQUEST_URL".into(), url.clone()));
        vars.push(("ACTIONS_ID_TOKEN_REQUEST_TOKEN".into(), tok.clone()));
    }

    // Repo secrets last: a collision with a reserved name is skipped loudly
    // rather than silently overriding a default.
    let reserved: std::collections::HashSet<String> = vars.iter().map(|(k, _)| k.clone()).collect();
    let mut warnings = Vec::new();
    for (k, v) in secrets {
        if reserved.contains(k) {
            warnings.push(format!(
                "::warning::secret '{k}' collides with a reserved environment variable — skipped"
            ));
            continue;
        }
        vars.push((k.clone(), v.clone()));
    }
    JobEnv {
        vars,
        reserved,
        warnings,
    }
}

/// Evaluate an `env:` map against `ctx`, overlay it into the `env` context
/// (later layers win), and push the values into the process env list.
pub(crate) fn layer_env(
    ctx: &mut Context,
    vars: &mut Vec<(String, String)>,
    env_map: &serde_json::Value,
) -> Vec<String> {
    match expr::eval_json(env_map, ctx) {
        Ok(evaled) => {
            ctx.overlay_env(&evaled);
            if let serde_json::Value::Object(m) = evaled {
                for (k, v) in &m {
                    env_set(vars, k, expr::stringify(v));
                }
            }
            Vec::new()
        }
        Err(e) => vec![format!("::warning::env evaluation failed: {e}")],
    }
}

/// Create the per-job runner temp dir (`work_dir/_runner/{job_id}`) — clean
/// slate, then `event.json` plus the empty file-command files.
pub(crate) fn prepare_runner_temp(dir: &Path, task: &Task) -> Result<(), String> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join("tool-cache")).map_err(|e| e.to_string())?;
    // The persisted event payload wins; the synthetic envelope is the
    // fallback for pre-Phase-2 jobs.
    let event = task.event_payload.clone().unwrap_or_else(|| {
        serde_json::json!({
            "event": task.event.clone().unwrap_or_default(),
            "ref": task.head_ref.clone().unwrap_or_default(),
            "sha": task.head_sha.clone().unwrap_or_default(),
            "repository": format!(
                "{}/{}",
                task.repository_owner.clone().unwrap_or_default(),
                task.repository_name.clone().unwrap_or_default()
            ),
            "repository_id": task.repository_id.clone().unwrap_or_default(),
            "actor": task.actor.clone().unwrap_or_default(),
            "actor_id": task.actor_id.clone().unwrap_or_default(),
            "run_id": task.run_id.clone().unwrap_or_default(),
            "run_number": task.run_number.unwrap_or_default(),
            "pull_request": match task.event.as_deref() {
                Some("pull_request") if task.pull_number.unwrap_or_default() != 0 => {
                    serde_json::json!({
                        "number": task.pull_number.unwrap_or_default(),
                        "base_ref": task.base_ref.clone().unwrap_or_default(),
                        "head_ref": task.head_ref.clone().unwrap_or_default(),
                    })
                }
                _ => serde_json::Value::Null,
            },
        })
    });
    std::fs::write(dir.join("event.json"), event.to_string()).map_err(|e| e.to_string())?;
    for name in ["env", "path", "output", "step_summary", "state"] {
        std::fs::write(dir.join(name), b"").map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Parse the `$OXIDEAN_ENV_FILE` (`$GITHUB_ENV`) file: `KEY=VALUE` lines plus
/// heredocs (`KEY<<DELIM` … `DELIM`). Later entries win — callers apply in order.
pub(crate) fn parse_env_file(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        // Heredoc (`KEY<<DELIM`) only when `<<` precedes any `=` — otherwise
        // `KEY=va<<lue` is a plain assignment.
        let heredoc = match (line.find("<<"), line.find('=')) {
            (Some(hh), Some(eq)) => hh < eq,
            (Some(_), None) => true,
            _ => false,
        };
        if heredoc {
            let (key, delim) = line.split_once("<<").expect("heredoc marker");
            let key = key.trim();
            let delim = delim.trim();
            if key.is_empty() || delim.is_empty() {
                continue;
            }
            let mut value = String::new();
            for l in lines.by_ref() {
                if l.trim_end() == delim {
                    break;
                }
                if !value.is_empty() {
                    value.push('\n');
                }
                value.push_str(l);
            }
            upsert(&mut out, key, value);
        } else if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            upsert(&mut out, key, value.to_string());
        }
    }
    out
}

fn upsert(list: &mut Vec<(String, String)>, key: &str, value: String) {
    if let Some(e) = list.iter_mut().find(|(k, _)| k == key) {
        e.1 = value;
    } else {
        list.push((key.to_string(), value));
    }
}

/// Parse the `$OXIDEAN_PATH` file: one directory per line.
pub(crate) fn parse_path_file(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// Default `PATH` inside a step container when no base PATH exists yet —
/// Docker's conventional image PATH, so `$OXIDEAN_PATH` dirs never get
/// prepended onto *host* paths inside a container.
pub(crate) const DOCKER_DEFAULT_PATH: &str =
    "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Result of [`apply_file_commands`]: warnings plus the `$GITHUB_ENV` pairs
/// actually applied (callers also feed them to the `env` context).
pub(crate) struct AppliedCommands {
    pub warnings: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Merge the env/path file-command files into `env` for subsequent steps and
/// truncate them. `mode` decides the PATH base for `$OXIDEAN_PATH` merges:
/// Docker steps get the container default; host steps inherit the process PATH.
pub(crate) fn apply_file_commands(
    runner_temp: &Path,
    env: &mut Vec<(String, String)>,
    reserved: &std::collections::HashSet<String>,
    mode: &ExecMode,
) -> AppliedCommands {
    let mut warnings = Vec::new();
    let mut applied = Vec::new();
    let env_file = runner_temp.join("env");
    if let Ok(text) = std::fs::read_to_string(&env_file) {
        if !text.trim().is_empty() {
            for (k, v) in parse_env_file(&text) {
                if reserved.contains(&k) {
                    warnings.push(format!(
                        "::warning::env file entry '{k}' collides with a reserved variable — skipped"
                    ));
                    continue;
                }
                env_set(env, &k, v.clone());
                applied.push((k, v));
            }
        }
        let _ = std::fs::write(&env_file, b"");
    }
    let path_file = runner_temp.join("path");
    if let Ok(text) = std::fs::read_to_string(&path_file) {
        let dirs = parse_path_file(&text);
        if !dirs.is_empty() {
            let base = env_get(env, "PATH")
                .map(String::from)
                .unwrap_or_else(|| match mode {
                    ExecMode::Docker(_) | ExecMode::ContainerExec(_) => {
                        DOCKER_DEFAULT_PATH.to_string()
                    }
                    ExecMode::Host => std::env::var("PATH").unwrap_or_default(),
                });
            let merged = format!("{}:{base}", dirs.join(":"));
            env_set(env, "PATH", merged);
        }
        let _ = std::fs::write(&path_file, b"");
    }
    // `$GITHUB_STATE` is scoped to the action that wrote it — node/docker
    // actions drain it into their own post env at push time; clearing it
    // here keeps `run:`-step writes from leaking into a later action's post.
    let state_file = runner_temp.join("state");
    if state_file.exists() {
        let _ = std::fs::write(&state_file, b"");
    }
    AppliedCommands {
        warnings,
        env: applied,
    }
}

/// Parse `$GITHUB_OUTPUT` (`KEY=VALUE` / heredoc — same format as env files)
/// and truncate it for the next step.
pub(crate) fn parse_output_file(path: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(path) {
        if !text.trim().is_empty() {
            for (k, v) in parse_env_file(&text) {
                out.insert(k, v);
            }
        }
        let _ = std::fs::write(path, b"");
    }
    out
}

pub(crate) fn prepare_workspace(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())
}
