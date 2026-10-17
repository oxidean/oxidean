//! Job execution: `actions/checkout` builtin, `run:` steps on the host or in
//! Docker, expression evaluation (`${{ }}`), log streaming with secret masking.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use expr::{Context, JobStatus};
use oxidean_actions_expr as expr;

use crate::client::{Client, Task};
use crate::config::{Config, ExecMode};

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
    pub shell: Option<String>,
    pub working_directory: Option<String>,
    pub with: Option<serde_json::Value>,
    pub env: Option<serde_json::Value>,
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

/// The `OXIDEAN_*`/`GITHUB_*` variable pairs, in emission order. Each row
/// produces the Oxidean name immediately followed by its GitHub alias carrying
/// the identical value — this list is the single source of truth so the two
/// namespaces cannot drift. `OXIDEAN_ENV_FILE`/`GITHUB_ENV` is the one
/// asymmetric pair: bare `OXIDEAN_ENV` is reserved by the server's runtime-mode
/// config (`development`/`production`), so the file-command variable gets a
/// distinct name while the GitHub alias keeps GitHub's exact spelling.
const PAIRED_VARS: &[(&str, &str)] = &[
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
];

/// Result of [`job_env`]: the ordered variable list, the set of reserved names
/// (for secret/env-file collision checks), and collision warnings to log.
struct JobEnv {
    vars: Vec<(String, String)>,
    reserved: std::collections::HashSet<String>,
    warnings: Vec<String>,
}

/// GitHub-style `RUNNER_OS` spelling.
fn runner_os() -> String {
    match std::env::consts::OS {
        "linux" => "Linux".into(),
        "macos" => "macOS".into(),
        "windows" => "Windows".into(),
        other => other.into(),
    }
}

/// GitHub-style `RUNNER_ARCH` spelling (X64/ARM64/…).
fn runner_arch() -> String {
    match std::env::consts::ARCH {
        "x86_64" => "X64".into(),
        "aarch64" => "ARM64".into(),
        "x86" => "X86".into(),
        "arm" => "ARM".into(),
        other => other.to_uppercase(),
    }
}

fn env_set(env: &mut Vec<(String, String)>, key: &str, value: String) {
    if let Some(entry) = env.iter_mut().find(|(k, _)| k == key) {
        entry.1 = value;
    } else {
        env.push((key.to_string(), value));
    }
}

fn env_get<'a>(env: &'a [(String, String)], key: &str) -> Option<&'a str> {
    env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// The default job environment: `CI`/`OXIDEAN`, each `OXIDEAN_<X>` paired with
/// its `GITHUB_<X>` alias, `RUNNER_*` metadata, then repo secrets (which may
/// never override the reserved names above).
///
/// `workspace_env` and `runner_temp_env` are the paths visible *inside the
/// step*: host paths in host mode, `/workspace` / `/__runner_temp` in Docker.
fn job_env(
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

// ---------- expression context -------------------------------------------

/// `hashFiles(patterns…)` over the workspace — glob match, then SHA-256 of
/// the concatenated per-file SHA-256 digests (sorted for determinism).
/// Runner-only: server-side contexts leave the callback unset ("" result).
fn hash_files_fn(workspace: PathBuf) -> impl Fn(&[String]) -> Result<String, String> + Send + Sync {
    move |patterns| {
        let mut include: Vec<PathBuf> = Vec::new();
        for raw in patterns {
            let (neg, pat) = match raw.strip_prefix('!') {
                Some(p) => (true, p),
                None => (false, raw.as_str()),
            };
            let mut hits = glob_walk(&workspace, pat);
            hits.sort();
            hits.dedup();
            if neg {
                let hits: std::collections::HashSet<PathBuf> = hits.into_iter().collect();
                include.retain(|f| !hits.contains(f));
            } else {
                include.extend(hits);
            }
        }
        include.sort();
        include.dedup();
        let mut outer = Sha256::new();
        for f in &include {
            let bytes = std::fs::read(f).map_err(|e| e.to_string())?;
            outer.update(Sha256::digest(&bytes));
        }
        let digest = outer.finalize();
        Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
    }
}

/// Files under `root` whose relative path matches `pattern` (minimatch-lite:
/// `**` crosses segments, `*`/`?`/`[]` within a segment, `{a,b}` alternation,
/// leading `/` = workspace-rooted). `.git` is never walked.
fn glob_walk(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let pat = pattern.trim().trim_start_matches('/');
    let expanded = expand_braces(pat);
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                if entry.file_name() == ".git" {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            for p in &expanded {
                if glob_match(p, &rel) {
                    out.push(path.clone());
                    break;
                }
            }
        }
    }
    out
}

/// Expand `{a,b}` alternation into separate patterns (nested braces OK).
fn expand_braces(pat: &str) -> Vec<String> {
    let Some(open) = pat.find('{') else {
        return vec![pat.to_string()];
    };
    let mut depth = 0;
    let mut close = None;
    for (i, c) in pat.char_indices().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![pat.to_string()];
    };
    let (head, rest) = pat.split_at(open);
    let (body, tail) = (&rest[1..close - open], &rest[close - open + 1..]);
    body.split(',')
        .flat_map(|alt| expand_braces(&format!("{head}{alt}{tail}")))
        .collect()
}

/// Segment-wise glob match: `**` consumes zero or more path segments.
fn glob_match(pattern: &str, path: &str) -> bool {
    let ps: Vec<&str> = pattern.split('/').collect();
    let fs: Vec<&str> = path.split('/').collect();
    fn go(ps: &[&str], fs: &[&str]) -> bool {
        if ps.is_empty() {
            return fs.is_empty();
        }
        if ps[0] == "**" {
            for skip in 0..=fs.len() {
                if go(&ps[1..], &fs[skip..]) {
                    return true;
                }
            }
            return false;
        }
        match fs.split_first() {
            Some((seg, rest)) if seg_match(ps[0], seg) => go(&ps[1..], rest),
            _ => false,
        }
    }
    go(&ps, &fs)
}

/// Single-segment wildcard match (`*`, `?`, `[class]`).
fn seg_match(pat: &str, seg: &str) -> bool {
    let (p, s): (Vec<char>, Vec<char>) = (pat.chars().collect(), seg.chars().collect());
    // Iterative wildcard match with backtracking on `*`.
    let (mut pi, mut si) = (0usize, 0usize);
    let (mut star_p, mut star_s) = (usize::MAX, 0usize);
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '[' {
            // Character class.
            let mut j = pi + 1;
            let neg = p.get(j) == Some(&'!') || p.get(j) == Some(&'^');
            if neg {
                j += 1;
            }
            let mut matched = false;
            let mut k = j;
            while k < p.len() && p[k] != ']' {
                if k + 2 < p.len() && p[k + 1] == '-' && p[k + 2] != ']' {
                    if p[k] <= s[si] && s[si] <= p[k + 2] {
                        matched = true;
                    }
                    k += 3;
                } else {
                    if p[k] == s[si] {
                        matched = true;
                    }
                    k += 1;
                }
            }
            if k >= p.len() || matched == neg {
                if star_p != usize::MAX {
                    pi = star_p;
                    star_s += 1;
                    si = star_s;
                    continue;
                }
                return false;
            }
            pi = k + 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_p = pi;
            star_s = si;
            pi += 1;
        } else if star_p != usize::MAX {
            pi = star_p + 1;
            star_s += 1;
            si = star_s;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Build the runner-side expression context for the claimed task.
fn runner_context(
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
        .with_context("inputs", serde_json::json!({}))
        .with_context("vars", serde_json::json!({}))
        .with_context("strategy", serde_json::json!({}))
        .with_context("job", serde_json::json!({"status": "success"}))
        .with_job_status("success")
        .with_hash_files(hash_files_fn(workspace.to_path_buf()))
}

/// Evaluate an `env:` map against `ctx`, overlay it into the `env` context
/// (later layers win), and push the values into the process env list.
fn layer_env(
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

/// Interpolate every expression-bearing field of a step against `ctx`.
fn interpolate_step(step: &Step, ctx: &Context) -> Result<Step, String> {
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
fn step_allowed(src: Option<&str>, ctx: &Context) -> Result<bool, String> {
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
fn record_step(ctx: &mut Context, id: &str, outcome: &str, outputs: HashMap<String, String>) {
    let mut steps = ctx
        .get_context("steps")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = steps.as_object_mut() {
        obj.insert(
            id.to_string(),
            serde_json::json!({
                "outcome": outcome,
                "conclusion": outcome,
                "outputs": outputs,
            }),
        );
    }
    ctx.set_context("steps", steps);
}

/// Keep `job.status` in the context in sync with the runner's JobStatus.
fn sync_job_ctx(ctx: &mut Context) {
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
fn step_timeout(
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
struct JobDeadline(Option<Instant>);

impl JobDeadline {
    fn new(timeout_seconds: Option<i64>) -> Self {
        Self(timeout_seconds.map(|s| Instant::now() + Duration::from_secs(s.max(0) as u64)))
    }
    fn remaining(&self) -> Option<Duration> {
        self.0.map(|d| d.saturating_duration_since(Instant::now()))
    }
    fn expired(&self) -> bool {
        matches!(self.remaining(), Some(d) if d.is_zero())
    }
}

struct Logger<'a> {
    client: &'a Client,
    run_id: String,
    job_id: String,
    /// Secret values + the job token — masked out of every log chunk.
    mask_values: &'a [String],
    /// Server-side cancel signal — set when an `update_log` ack reports
    /// `cancelled`; the step loop checks `take_cancelled()` between steps.
    cancelled: std::sync::atomic::AtomicBool,
}

impl<'a> Logger<'a> {
    fn take_cancelled(&self) -> bool {
        self.cancelled
            .swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    async fn send(&self, chunk: &str) {
        let masked = mask_secrets(chunk, self.mask_values);
        if masked.is_empty() {
            return;
        }
        match self
            .client
            .update_log(&self.run_id, &self.job_id, &masked)
            .await
        {
            Ok(ack) if ack.cancelled => {
                self.cancelled
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => {
                tracing::warn!("update_log failed: {e}");
            }
            _ => {}
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
/// the run's head sha. Supported `with:` inputs: `repository`, `ref`, `path`,
/// `fetch-depth`, `submodules`, `persist-credentials` (no-op — our credential
/// rides per-command `http.extraHeader`, never persisted). Everything else is
/// acknowledged with a `::warning::` and ignored.
async fn checkout(
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
fn checkout_auth_args(job_token: Option<&str>, git_token: Option<&str>) -> Vec<String> {
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
#[allow(clippy::too_many_arguments)]
async fn run_step(
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
        cancelled: std::sync::atomic::AtomicBool::new(false),
    };

    match client.update_task(&job_id, "in_progress", None).await {
        Ok(ack) if ack.cancelled => return "cancelled",
        Ok(_) => {}
        Err(e) => {
            tracing::error!("update_task in_progress failed: {e}");
            return "failure";
        }
    }

    let workspace = config.work_dir.join(&job_id);
    if let Err(e) = prepare_workspace(&workspace) {
        logger
            .line(&format!("::error::failed to prepare workspace: {e}"))
            .await;
        let _ = client.update_task(&job_id, "failure", None).await;
        return "failure";
    }
    // Per-job runner temp: event.json + env/path/output/step_summary files.
    let runner_temp = config.work_dir.join("_runner").join(&job_id);
    if let Err(e) = prepare_runner_temp(&runner_temp, task) {
        logger
            .line(&format!("::error::failed to prepare runner temp dir: {e}"))
            .await;
        let _ = client.update_task(&job_id, "failure", None).await;
        return "failure";
    }

    let steps: Vec<Step> = task
        .steps
        .clone()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let mode = exec_mode_for(task, config);
    // Inside a container the workspace mounts at /workspace and the runner
    // temp dir at /__runner_temp.
    let (workspace_env, runner_temp_env) = match &mode {
        ExecMode::Docker(_) => ("/workspace".to_string(), "/__runner_temp".to_string()),
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
            }
        ))
        .await;

    let mut cancelled = false;
    for (i, raw_step) in steps.iter().enumerate() {
        let step_id = raw_step.id.clone().unwrap_or_else(|| format!("__step{i}"));
        // Job deadline gates every subsequent step (GitHub `cancelled`).
        if deadline.expired() {
            logger
                .line(&format!("::notice::step '{step_id}' skipped — job timeout"))
                .await;
            record_step(&mut ctx, &step_id, "skipped", HashMap::new());
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
            record_step(&mut ctx, &step_id, "skipped", HashMap::new());
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
                record_step(&mut ctx, &step_id, "failure", HashMap::new());
                ctx.set_job_status(JobStatus::Failure);
                sync_job_ctx(&mut ctx);
                continue;
            }
        };
        if !allowed {
            record_step(&mut ctx, &step_id, "skipped", HashMap::new());
            continue;
        }
        // Interpolate all expression-bearing fields once per step.
        let step = match interpolate_step(raw_step, &ctx) {
            Ok(s) => s,
            Err(e) => {
                logger
                    .line(&format!("::error::step '{step_id}' expression error: {e}"))
                    .await;
                record_step(&mut ctx, &step_id, "failure", HashMap::new());
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

        // Step env layers onto the context only for `run:` steps (GitHub
        // evaluates step env before the step's own commands run).
        let (timeout, timeout_warnings) =
            step_timeout(&step, &ctx, default_timeout, deadline.remaining());
        for w in timeout_warnings {
            logger.line(&w).await;
        }
        let result: Result<(), String> = if let Some(uses) = &step.uses {
            if uses.starts_with("actions/checkout") {
                let with = step
                    .with
                    .as_ref()
                    .and_then(|v| v.as_object().cloned())
                    .unwrap_or_default();
                checkout(task, config, &workspace, &with, &env.vars, &logger, timeout).await
            } else {
                Err(format!(
                    "unsupported action '{uses}' — only actions/checkout and run: steps are supported"
                ))
            }
        } else if step.run.is_some() {
            // Step-level env overlays job env — both the `env` context and
            // the process env for this step.
            let mut step_env = env.vars.clone();
            if let Some(step_env_map) = &step.env {
                let mut step_ctx = ctx.clone();
                for w in layer_env(&mut step_ctx, &mut step_env, step_env_map) {
                    logger.line(&w).await;
                }
            }
            let r = run_step(
                &step,
                &mode,
                &workspace,
                &runner_temp,
                step.working_directory.as_deref(),
                &step_env,
                &logger,
                timeout,
            )
            .await;
            // File commands apply to subsequent steps even when this one
            // failed (GitHub semantics); the job still fails below.
            let applied = apply_file_commands(&runner_temp, &mut env.vars, &env.reserved, &mode);
            for w in &applied.warnings {
                logger.line(w).await;
            }
            // `$GITHUB_ENV` writes also become `env.*` context entries.
            if !applied.env.is_empty() {
                let m: serde_json::Map<String, serde_json::Value> = applied
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                ctx.overlay_env(&serde_json::Value::Object(m));
            }
            r
        } else {
            Ok(())
        };

        // `$GITHUB_OUTPUT` → `steps.<id>.outputs` (consumed by `needs` in
        // the scheduler phase; also readable via the steps context).
        let outputs = parse_output_file(&runner_temp.join("output"));
        let outcome = match &result {
            Ok(()) => "success",
            Err(e) if deadline.expired() && e == "step timed out" => {
                cancelled = true;
                "cancelled"
            }
            Err(_) => "failure",
        };
        record_step(&mut ctx, &step_id, outcome, outputs);
        if let Err(e) = result {
            logger.line(&format!("::error::{e}")).await;
            ctx.set_job_status(if cancelled {
                JobStatus::Cancelled
            } else {
                JobStatus::Failure
            });
            sync_job_ctx(&mut ctx);
        }
        logger.line(&format!("##[step-done]{title}")).await;
    }

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
        .update_task(&job_id, final_state, Some(&outputs))
        .await
    {
        tracing::error!("update_task {final_state} failed: {e}");
    }
    final_state
}

/// `{step_id: {name: value}}` from the `steps` context — the shape
/// `update_task.outputs` and `needs.<id>.outputs.*` consume.
fn step_outputs_report(ctx: &Context) -> serde_json::Value {
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

/// Create the per-job runner temp dir (`work_dir/_runner/{job_id}`) — clean
/// slate, then `event.json` plus the empty file-command files.
fn prepare_runner_temp(dir: &Path, task: &Task) -> Result<(), String> {
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
    for name in ["env", "path", "output", "step_summary"] {
        std::fs::write(dir.join(name), b"").map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Parse the `$OXIDEAN_ENV_FILE` (`$GITHUB_ENV`) file: `KEY=VALUE` lines plus
/// heredocs (`KEY<<DELIM` … `DELIM`). Later entries win — callers apply in order.
fn parse_env_file(text: &str) -> Vec<(String, String)> {
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
fn parse_path_file(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// Default `PATH` inside a step container when no base PATH exists yet —
/// Docker's conventional image PATH, so `$OXIDEAN_PATH` dirs never get
/// prepended onto *host* paths inside a container.
const DOCKER_DEFAULT_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Result of [`apply_file_commands`]: warnings plus the `$GITHUB_ENV` pairs
/// actually applied (callers also feed them to the `env` context).
struct AppliedCommands {
    warnings: Vec<String>,
    env: Vec<(String, String)>,
}

/// Merge the env/path file-command files into `env` for subsequent steps and
/// truncate them. `mode` decides the PATH base for `$OXIDEAN_PATH` merges:
/// Docker steps get the container default; host steps inherit the process PATH.
fn apply_file_commands(
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
                    ExecMode::Docker(_) => DOCKER_DEFAULT_PATH.to_string(),
                    ExecMode::Host => std::env::var("PATH").unwrap_or_default(),
                });
            let merged = format!("{}:{base}", dirs.join(":"));
            env_set(env, "PATH", merged);
        }
        let _ = std::fs::write(&path_file, b"");
    }
    AppliedCommands {
        warnings,
        env: applied,
    }
}

/// Parse `$GITHUB_OUTPUT` (`KEY=VALUE` / heredoc — same format as env files)
/// and truncate it for the next step.
fn parse_output_file(path: &Path) -> HashMap<String, String> {
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
        assert_eq!(
            shell_argv(None, "echo hi"),
            vec!["bash", "-e", "-o", "pipefail", "-c", "echo hi"]
        );
        assert_eq!(shell_argv(Some("sh"), "x"), vec!["sh", "-e", "-c", "x"]);
    }

    #[test]
    fn step_deserialize_matches_api_shape() {
        let v = serde_json::json!([
            {"id": "co", "name": "Checkout", "uses": "actions/checkout@v4", "run": null,
             "if": "always()", "timeout-minutes": 5, "shell": null,
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
