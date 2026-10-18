//! `uses:` action dispatch — resolution (`./local`, `owner/repo@ref`,
//! `docker://`), `action.yml` loading, and per-runtime execution
//! (composite expansion, node, docker). `with:` inputs become `INPUT_*`
//! env vars and the `inputs` context; declared `outputs:` map back to the
//! caller via expression evaluation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use serde::Deserialize;
use tokio::process::Command;

use expr::{Context, JobStatus};
use oxidean_actions_expr as expr;

use super::context::{record_step, step_allowed, sync_job_ctx};
use super::env::{env_set, layer_env, parse_env_file};
use super::run_step::{run_step, run_streamed};
use super::{PendingPost, Step, StepEnv};

/// Max nesting for composite `uses:` inside composite actions.
const MAX_ACTION_DEPTH: usize = 16;

/// Recursion-boxed action future — composite expansion re-enters `run_action`
/// from inside `run_action_spec`, so the cycle is broken with a boxed future.
type BoxActionFut<'x> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<HashMap<String, String>, String>> + Send + 'x>,
>;

/// Parsed `action.yml` (the fields we execute).
#[derive(Debug, Deserialize)]
pub(crate) struct ActionSpec {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub inputs: Option<serde_yaml::Mapping>,
    #[serde(default)]
    pub outputs: Option<serde_yaml::Mapping>,
    /// Surfaced as a `::warning::` when the action runs (GitHub parity).
    #[serde(default, rename = "deprecationMessage")]
    pub deprecation_message: Option<String>,
    pub runs: ActionRuns,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActionRuns {
    pub using: String,
    // composite
    #[serde(default)]
    pub steps: Option<serde_yaml::Value>,
    // node
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub pre: Option<String>,
    #[serde(rename = "pre-if", default)]
    pub pre_if: Option<String>,
    #[serde(default)]
    pub post: Option<String>,
    #[serde(rename = "post-if", default)]
    pub post_if: Option<String>,
    // docker
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(rename = "post-entrypoint", default)]
    pub post_entrypoint: Option<String>,
    #[serde(default)]
    pub args: Option<serde_yaml::Value>,
    #[serde(default)]
    pub env: Option<serde_yaml::Value>,
}

/// A resolved `uses:` target.
enum ActionTarget {
    /// `./path` under the workspace.
    Local(PathBuf),
    /// `owner/repo[/path]@ref` materialized under the tool cache.
    Remote(PathBuf),
    /// `docker://image` — run the image as the step.
    DockerImage(String),
}

/// Execute a `uses:` step. Returns the step's outputs on success
/// (`$GITHUB_OUTPUT` writes are drained by the caller). `local_base` is what
/// `./` resolves against — the workspace at job level, the composite
/// action's own dir for nested `uses:`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_action(
    senv: &mut StepEnv<'_>,
    ctx: &mut Context,
    step: &Step,
    step_id: &str,
    step_env: &[(String, String)],
    timeout: Duration,
    depth: usize,
    local_base: &Path,
) -> Result<HashMap<String, String>, String> {
    let uses = step.uses.clone().unwrap_or_default();
    // Builtins are addressable inside composites too.
    if uses.starts_with("actions/checkout")
        || uses.starts_with("actions/upload-artifact")
        || uses.starts_with("actions/download-artifact")
    {
        let with = step
            .with
            .as_ref()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        if uses.starts_with("actions/upload-artifact") {
            return super::artifacts::upload(senv, &with).await;
        }
        if uses.starts_with("actions/download-artifact") {
            return super::artifacts::download(senv, &with).await;
        }
        super::checkout::checkout(
            senv.task,
            senv.config,
            senv.workspace,
            &with,
            step_env,
            senv.logger,
            timeout,
        )
        .await?;
        return Ok(HashMap::new());
    }
    let target = resolve_target(senv, &uses, local_base).await?;
    match target {
        ActionTarget::DockerImage(image) => {
            let args = step
                .with
                .as_ref()
                .and_then(|w| w.get("args"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            docker_run_image(senv, &image, args, step_env, timeout).await?;
            Ok(HashMap::new())
        }
        ActionTarget::Local(dir) | ActionTarget::Remote(dir) => {
            let spec = load_action_spec(&dir)?;
            run_action_spec(
                senv, ctx, step, step_id, step_env, &dir, spec, timeout, depth,
            )
            .await
        }
    }
}

/// Turn `uses:` into a concrete `ActionTarget`. `local_base` — see
/// [`run_action`].
async fn resolve_target(
    senv: &StepEnv<'_>,
    uses: &str,
    local_base: &Path,
) -> Result<ActionTarget, String> {
    if let Some(img) = uses.strip_prefix("docker://") {
        return Ok(ActionTarget::DockerImage(img.to_string()));
    }
    if let Some(rel) = uses.strip_prefix("./") {
        let dir = local_base.join(rel);
        if !dir.is_dir() {
            return Err(format!(
                "local action '{uses}' not found — run actions/checkout first"
            ));
        }
        return Ok(ActionTarget::Local(dir));
    }
    // `owner/repo[/path]@ref`
    let (path_part, git_ref) = uses
        .rsplit_once('@')
        .ok_or_else(|| format!("action '{uses}' is missing a @ref"))?;
    if git_ref.is_empty() {
        return Err(format!("action '{uses}' has an empty @ref"));
    }
    let segs: Vec<&str> = path_part.split('/').collect();
    if segs.len() < 2 {
        return Err(format!("action '{uses}' is not owner/repo[/path]@ref"));
    }
    let (owner, repo) = (segs[0], segs[1]);
    let subdir = segs[2..].join("/");
    let dir = fetch_remote_action(senv, owner, repo, &subdir, git_ref).await?;
    Ok(ActionTarget::Remote(dir))
}

/// Clone `owner/repo` at `ref` into `RUNNER_TOOL_CACHE/actions/…` and return
/// the action directory. Instance repos resolve via `config.origin` first;
/// on clone failure fall back to `https://github.com/…`. Auth rides per-command
/// `http.extraHeader` — the repo-locked job token first (same-repo actions),
/// then the runner PAT (`OXIDEAN_RUNNER_GIT_TOKEN`, cross-repo), then
/// unauthenticated (public github.com).
async fn fetch_remote_action(
    senv: &StepEnv<'_>,
    owner: &str,
    repo: &str,
    subdir: &str,
    git_ref: &str,
) -> Result<PathBuf, String> {
    let cache_root = senv
        .runner_temp
        .join("tool-cache")
        .join("actions")
        .join(owner)
        .join(repo);
    let sha_pinned = git_ref.len() == 40 && git_ref.chars().all(|c| c.is_ascii_hexdigit());

    let creds: Vec<Vec<String>> = [
        senv.task.job_token.as_deref().filter(|t| !t.is_empty()),
        senv.config.git_token.as_deref().filter(|t| !t.is_empty()),
        None,
    ]
    .into_iter()
    .map(auth_args)
    .collect();
    // Instance mirror (`OXIDEAN_ACTIONS_MIRROR`) replaces the public github
    // fallback — an airgapped instance mirrors vetted action repos instead of
    // letting the runner reach out directly.
    let fallback = senv
        .config
        .actions_mirror
        .as_deref()
        .unwrap_or("https://github.com");
    let urls = [
        format!("{}/{}/{}.git", senv.config.origin, owner, repo),
        format!("{fallback}/{}/{}.git", owner, repo),
    ];

    // Git must never prompt — a `uses:` repo that hits an auth/askpass
    // prompt would otherwise block the job forever (no tty to answer it).
    let mut git_env: Vec<(String, String)> = senv.env.to_vec();
    git_env.push(("GIT_TERMINAL_PROMPT".into(), "0".into()));
    let git_env = git_env.as_slice();

    // Phase 9: key the download cache on the resolved commit, not the ref
    // name — a re-pushed tag or moved branch must fetch fresh code (GitHub
    // resolves `@v4` to its current sha before consulting its toolcache).
    // `ls-remote` is a single cheap round-trip; when every endpoint is
    // unreachable (airgapped, mirror down) fall back to the legacy
    // ref-keyed cache so a warm cache still serves.
    let resolved_sha = if sha_pinned {
        Some(git_ref.to_string())
    } else {
        let mut sha = None;
        'resolve: for url in &urls {
            for auth in &creds {
                if let Some(s) = ls_remote_commit(git_env, url, auth, git_ref).await {
                    sha = Some(s);
                    break 'resolve;
                }
            }
        }
        sha
    };
    let ref_key = git_ref.replace('/', "_");
    let dest = match &resolved_sha {
        Some(sha) => cache_root.join(format!("{ref_key}@{sha}")),
        None => cache_root.join(&ref_key),
    };
    if dest.is_dir() {
        return Ok(if subdir.is_empty() {
            dest
        } else {
            dest.join(subdir)
        });
    }
    std::fs::create_dir_all(&cache_root).map_err(|e| e.to_string())?;

    let mut last_err = String::new();
    let logger = senv.logger;
    // Unique temp dir per attempt — concurrent jobs on one runner may fetch
    // the same action; a shared `.clone-<pid>` path would let one job remove
    // the other's in-flight clone mid-rename.
    static CLONE_TMP_SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let tmp = cache_root.join(format!(
        ".clone-{}-{}",
        std::process::id(),
        CLONE_TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    for url in &urls {
        for auth in &creds {
            let _ = std::fs::remove_dir_all(&tmp);
            let attempts = [
                vec![
                    "clone".into(),
                    "--depth".into(),
                    "1".into(),
                    "--branch".into(),
                    git_ref.to_string(),
                    url.clone(),
                    tmp.to_string_lossy().to_string(),
                ],
                vec![
                    "clone".into(),
                    url.clone(),
                    tmp.to_string_lossy().to_string(),
                ],
            ];
            let mut cloned = false;
            for args in attempts {
                let mut cmd = Command::new("git");
                cmd.args(auth.iter().chain(args.iter()));
                match run_streamed(&mut cmd, git_env, logger, Duration::from_secs(180)).await {
                    Ok(0) => {
                        cloned = true;
                        break;
                    }
                    Ok(code) => last_err = format!("git clone {url} exited {code}"),
                    Err(e) => last_err = format!("git clone {url}: {e}"),
                }
                let _ = std::fs::remove_dir_all(&tmp);
            }
            // SHA-pinned ref: `clone --branch` can't name a commit, but a
            // targeted `fetch <sha>` can (servers permitting sha-in-want —
            // Oxidean's git-http and github.com both do). Cheaper than the
            // full-clone fallback for pinned releases.
            if !cloned && sha_pinned {
                let _ = std::fs::create_dir_all(&tmp);
                let seq: [Vec<String>; 4] = [
                    vec!["init".into(), "-q".into()],
                    vec!["remote".into(), "add".into(), "origin".into(), url.clone()],
                    vec![
                        "fetch".into(),
                        "--depth".into(),
                        "1".into(),
                        "origin".into(),
                        git_ref.to_string(),
                    ],
                    vec!["checkout".into(), "-q".into(), "FETCH_HEAD".into()],
                ];
                let mut ok = true;
                for args in seq {
                    let mut cmd = Command::new("git");
                    cmd.args(auth.iter().chain(args.iter())).current_dir(&tmp);
                    match run_streamed(&mut cmd, git_env, logger, Duration::from_secs(180)).await {
                        Ok(0) => {}
                        Ok(code) => {
                            last_err = format!("git fetch {git_ref} from {url} exited {code}");
                            ok = false;
                            break;
                        }
                        Err(e) => {
                            last_err = format!("git fetch {git_ref} from {url}: {e}");
                            ok = false;
                            break;
                        }
                    }
                }
                cloned = ok;
                if !cloned {
                    let _ = std::fs::remove_dir_all(&tmp);
                }
            }
            if !cloned {
                continue;
            }
            // Full-clone fallback path may leave HEAD on the default branch.
            let mut cmd = Command::new("git");
            cmd.args(auth.iter().cloned()).current_dir(&tmp);
            cmd.args(["checkout", git_ref]);
            let _ = run_streamed(&mut cmd, git_env, logger, Duration::from_secs(60)).await;
            // Download integrity: record the resolved commit; for a
            // SHA-pinned ref, hard-verify HEAD matches the pin (a substituted
            // clone would silently run different code).
            let head = Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&tmp)
                .output()
                .await
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
            match head.as_deref() {
                Some(h)
                    if git_ref.len() == 40
                        && git_ref.chars().all(|c| c.is_ascii_hexdigit())
                        && !h.eq_ignore_ascii_case(git_ref) =>
                {
                    let _ = std::fs::remove_dir_all(&tmp);
                    return Err(format!(
                        "action {owner}/{repo}@{git_ref} resolved to {h} — SHA pin mismatch"
                    ));
                }
                _ => {}
            }
            // Atomic install: rename wins; a loser removes its clone and uses
            // the racing job's identical result (same resolved sha).
            if std::fs::rename(&tmp, &dest).is_err() {
                let _ = std::fs::remove_dir_all(&tmp);
                if !dest.is_dir() {
                    last_err = format!("install {owner}/{repo}@{git_ref} into cache failed");
                    continue;
                }
            }
            senv.logger
                .line(&format!(
                    "Resolved {owner}/{repo}@{git_ref} → {url}{}",
                    head.as_deref()
                        .map(|h| format!(" (commit {h})"))
                        .unwrap_or_default()
                ))
                .await;
            return Ok(if subdir.is_empty() {
                dest
            } else {
                dest.join(subdir)
            });
        }
    }
    Err(format!(
        "could not resolve action {owner}/{repo}@{git_ref}: {last_err}"
    ))
}

/// `git ls-remote` resolution of a movable ref → commit sha (Phase 9 cache
/// key). Prefers the peeled annotated-tag commit (`ref^{}`) so
/// `uses: x/y@v4` caches under the tag's target commit, not the tag object.
/// `None` on transport/auth failure — the caller tries the next url/auth
/// pair, and a fully-unreachable network falls back to ref-keyed caching.
async fn ls_remote_commit(
    env: &[(String, String)],
    url: &str,
    auth: &[String],
    git_ref: &str,
) -> Option<String> {
    let peeled = format!("refs/tags/{git_ref}^{{}}");
    let out = tokio::time::timeout(Duration::from_secs(30), async {
        Command::new("git")
            .args(auth.iter())
            .args([
                "ls-remote",
                url,
                &peeled,
                &format!("refs/tags/{git_ref}"),
                &format!("refs/heads/{git_ref}"),
                git_ref,
            ])
            .envs(env.iter().cloned())
            .output()
            .await
    })
    .await
    .ok()?
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut first: Option<String> = None;
    for line in text.lines() {
        let Some((sha, name)) = line.split_once('\t') else {
            continue;
        };
        // Peeled entries carry the real commit for annotated tags.
        if name == peeled {
            return Some(sha.to_string());
        }
        if first.is_none() {
            first = Some(sha.to_string());
        }
    }
    first
}

/// `git -c http.extraHeader=…` for clone auth (same scheme as checkout).
fn auth_args(token: Option<&str>) -> Vec<String> {
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

/// Read `action.yml`/`action.yaml` from `dir`.
fn load_action_spec(dir: &Path) -> Result<ActionSpec, String> {
    for name in ["action.yml", "action.yaml"] {
        let p = dir.join(name);
        if let Ok(text) = std::fs::read_to_string(&p) {
            return serde_yaml::from_str(&text)
                .map_err(|e| format!("invalid {}: {e}", p.display()));
        }
    }
    Err(format!("no action.yml in {}", dir.display()))
}

/// Dispatch on `runs.using`. Boxed for the composite→action recursion; `'x`
/// is the caller's reborrow — `StepEnv`'s own lifetime stays invariant.
#[allow(clippy::too_many_arguments)]
fn run_action_spec<'x>(
    senv: &'x mut StepEnv<'_>,
    ctx: &'x mut Context,
    step: &'x Step,
    step_id: &'x str,
    step_env: &'x [(String, String)],
    action_dir: &'x Path,
    spec: ActionSpec,
    timeout: Duration,
    depth: usize,
) -> BoxActionFut<'x> {
    Box::pin(async move {
        if depth > MAX_ACTION_DEPTH {
            return Err("action nesting too deep".into());
        }
        // Resolve `with:` inputs → INPUT_* env + `inputs` context.
        let inputs = resolve_inputs(&spec, step, ctx)?;
        // GitHub warns for deprecated actions and `with:` keys the action
        // never declared.
        if let Some(msg) = spec
            .deprecation_message
            .as_deref()
            .filter(|m| !m.is_empty())
        {
            senv.logger.line(&format!("::warning::{msg}")).await;
        }
        if let (Some(declared), Some(with)) = (&spec.inputs, step.with.as_ref()) {
            let unexpected: Vec<String> = with
                .as_object()
                .into_iter()
                .flat_map(|m| m.keys())
                .filter(|k| !declared.contains_key(serde_yaml::Value::String(k.to_string())))
                .cloned()
                .collect();
            if !unexpected.is_empty() {
                senv.logger
                    .line(&format!(
                        "::warning::unexpected input(s) '{}' supplied to {}",
                        unexpected.join("', '"),
                        step.uses.as_deref().unwrap_or("action")
                    ))
                    .await;
            }
        }
        let mut action_env = step_env.to_vec();
        let mut inputs_json = serde_json::Map::new();
        for (k, v) in &inputs {
            env_set(
                &mut action_env,
                &format!("INPUT_{}", normalize_input_key(k)),
                v.clone(),
            );
            inputs_json.insert(k.clone(), serde_json::Value::String(v.clone()));
        }
        env_set(
            &mut action_env,
            "GITHUB_ACTION_PATH",
            action_dir.to_string_lossy().to_string(),
        );
        let mut action_ctx = ctx.clone();
        action_ctx.set_context("inputs", serde_json::Value::Object(inputs_json));

        if let Some(n) = &spec.name {
            senv.logger.line(&format!("##[action]{n}")).await;
        }
        match spec.runs.using.as_str() {
            "composite" => {
                run_composite(
                    senv,
                    ctx,
                    &action_ctx,
                    &spec,
                    action_dir,
                    &action_env,
                    timeout,
                    depth,
                )
                .await
            }
            u if u.starts_with("node") => {
                run_node_action(
                    senv,
                    ctx,
                    step_id,
                    &spec,
                    action_dir,
                    &action_env,
                    u,
                    timeout,
                )
                .await
            }
            "docker" => {
                run_docker_action(senv, step, &spec, action_dir, &action_env, timeout).await
            }
            other => Err(format!("unsupported action runtime '{other}'")),
        }
    })
}

/// `with:` → resolved inputs: declared defaults + required checks.
/// `default:` may be any YAML scalar (`default: false` is a bool, not a
/// string) and may embed `${{ }}` (e.g. `${{ github.token }}`) — GitHub
/// evaluates it in the step context, so we do the same.
fn resolve_inputs(
    spec: &ActionSpec,
    step: &Step,
    ctx: &Context,
) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    if let Some(inputs) = &spec.inputs {
        for (k, v) in inputs {
            let name = k.as_str().unwrap_or_default();
            let default = v
                .get(serde_yaml::Value::String("default".into()))
                .and_then(yaml_scalar_string)
                .map(|s| expr::interpolate(&s, ctx).unwrap_or(s));
            let required = v
                .get(serde_yaml::Value::String("required".into()))
                .and_then(|d| d.as_bool())
                .unwrap_or(false);
            let supplied = step
                .with
                .as_ref()
                .and_then(|w| w.get(name))
                .map(expr::stringify);
            match supplied.or(default) {
                Some(val) => {
                    out.insert(name.to_string(), val);
                }
                None if required => {
                    return Err(format!("action requires input '{name}'"));
                }
                None => {}
            }
        }
    }
    // `with:` keys not declared in inputs still arrive as INPUT_* (GitHub
    // forwards them).
    if let Some(w) = &step.with {
        if let Some(obj) = w.as_object() {
            for (k, v) in obj {
                out.entry(k.clone()).or_insert_with(|| expr::stringify(v));
            }
        }
    }
    Ok(out)
}

/// YAML scalar → input string. Null is "no default"; bool/number stringify
/// like GitHub; composite defaults collapse through the JSON stringifier.
fn yaml_scalar_string(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Null => None,
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::String(s) => Some(s.clone()),
        other => serde_json::to_value(other)
            .ok()
            .map(|j| expr::stringify(&j)),
    }
}

/// `INPUT_*` env naming — GitHub's toolkit reads
/// `INPUT_${name.replace(/ /g, '_').toUpperCase()}`: only *spaces* become
/// underscores; hyphens and every other character survive verbatim
/// (`node-version` → `INPUT_NODE-VERSION`, legal in process env blocks).
fn normalize_input_key(k: &str) -> String {
    k.chars()
        .map(|c| {
            if c == ' ' {
                '_'
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect()
}

/// Composite expansion: run each `runs.steps` entry through the same
/// machinery as workflow steps, then map `outputs:` expressions back out.
/// `ctx` is the job ctx — `$GITHUB_ENV` writes inside the composite persist
/// job-wide, so they overlay there as well as the inner `actx`.
#[allow(clippy::too_many_arguments)]
async fn run_composite(
    senv: &mut StepEnv<'_>,
    ctx: &mut Context,
    action_ctx: &Context,
    spec: &ActionSpec,
    action_dir: &Path,
    action_env: &[(String, String)],
    timeout: Duration,
    depth: usize,
) -> Result<HashMap<String, String>, String> {
    let steps_val = spec
        .runs
        .steps
        .clone()
        .ok_or("composite action declares no steps")?;
    let inner: Vec<Step> =
        serde_yaml::from_value(steps_val).map_err(|e| format!("composite steps: {e}"))?;
    let mut actx = action_ctx.clone();
    // Inner `steps` context is isolated — only steps inside this composite
    // are addressable (GitHub parity).
    actx.set_context("steps", serde_json::Value::Object(serde_json::Map::new()));
    // Inner steps write through the SAME file-command files — drain them per
    // step so env/path propagate to the job while outputs route into the
    // composite's own `steps` context.
    for (i, istep) in inner.iter().enumerate() {
        let iid = istep.id.clone().unwrap_or_else(|| format!("__inner{i}"));
        let allowed = step_allowed(istep.if_expr.as_deref(), &actx).unwrap_or(false);
        if !allowed {
            record_step(&mut actx, &iid, "skipped", "skipped", HashMap::new());
            continue;
        }
        let result: Result<(), String> = if istep.run.is_some() {
            let is = super::context::interpolate_step(istep, &actx)
                .map_err(|e| format!("composite step '{iid}': {e}"))?;
            let mut ienv = action_env.to_vec();
            if let Some(em) = &is.env {
                let mut c = actx.clone();
                for w in layer_env(&mut c, &mut ienv, em) {
                    senv.logger.line(&w).await;
                }
            }
            let r = run_step(
                &is,
                senv.mode,
                senv.workspace,
                senv.runner_temp,
                is.working_directory.as_deref(),
                &ienv,
                senv.logger,
                timeout,
            )
            .await;
            let applied = senv.apply_commands(&mut actx).await;
            reoverlay_env(ctx, &applied.env);
            let ioutcome = if r.is_ok() { "success" } else { "failure" };
            record_step(&mut actx, &iid, ioutcome, ioutcome, applied.outputs);
            r
        } else if istep.uses.is_some() {
            let is = super::context::interpolate_step(istep, &actx)
                .map_err(|e| format!("composite step '{iid}': {e}"))?;
            match Box::pin(run_action(
                senv,
                &mut actx,
                &is,
                &iid,
                action_env,
                timeout,
                depth + 1,
                action_dir,
            ))
            .await
            {
                Ok(mut outs) => {
                    let applied = senv.apply_commands(&mut actx).await;
                    reoverlay_env(ctx, &applied.env);
                    outs.extend(applied.outputs);
                    record_step(&mut actx, &iid, "success", "success", outs);
                    Ok(())
                }
                Err(e) => {
                    let applied = senv.apply_commands(&mut actx).await;
                    reoverlay_env(ctx, &applied.env);
                    record_step(&mut actx, &iid, "failure", "failure", applied.outputs);
                    Err(e)
                }
            }
        } else {
            Ok(())
        };
        if let Err(e) = result {
            let coe = istep
                .continue_on_error
                .as_ref()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if coe {
                senv.logger
                    .line(&format!("::warning::{e} (continue-on-error)"))
                    .await;
            } else {
                actx.set_job_status(JobStatus::Failure);
                sync_job_ctx(&mut actx);
                return Err(format!("composite step '{iid}': {e}"));
            }
        }
    }
    // `outputs:` — `{name: {value: "${{ … }}"}}` evaluated against the
    // composite's own steps+inputs context.
    let mut outputs = HashMap::new();
    if let Some(map) = &spec.outputs {
        for (k, v) in map {
            let name = k.as_str().unwrap_or_default();
            let src = v
                .get(serde_yaml::Value::String("value".into()))
                .and_then(|d| d.as_str())
                .unwrap_or_default();
            match expr::eval_embedded(src, &actx) {
                Ok(val) => {
                    outputs.insert(name.to_string(), expr::stringify(&val));
                }
                Err(e) => {
                    senv.logger
                        .line(&format!(
                            "::warning::composite output '{name}' eval failed: {e}"
                        ))
                        .await;
                }
            }
        }
    }
    Ok(outputs)
}

/// Re-apply `$GITHUB_ENV` pairs written inside a composite onto the outer
/// job ctx — env writes persist job-wide.
fn reoverlay_env(ctx: &mut Context, pairs: &[(String, String)]) {
    if pairs.is_empty() {
        return;
    }
    let m: serde_json::Map<String, serde_json::Value> = pairs
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    ctx.overlay_env(&serde_json::Value::Object(m));
}

/// node12/16/20/24 — resolve a node binary (tool-cache → PATH → download),
/// run `pre` then `main`, and queue `post` for after the job.
#[allow(clippy::too_many_arguments)]
async fn run_node_action(
    senv: &mut StepEnv<'_>,
    ctx: &mut Context,
    step_id: &str,
    spec: &ActionSpec,
    action_dir: &Path,
    action_env: &[(String, String)],
    using: &str,
    timeout: Duration,
) -> Result<HashMap<String, String>, String> {
    let wanted = using.trim_start_matches("node");
    let major = wanted.parse::<u32>().unwrap_or(20);
    let (major, warned) = if major < 17 {
        (
            20,
            Some(format!(
                "::warning::action runtime '{using}' is end-of-life — running under node20 (GitHub parity)"
            )),
        )
    } else {
        (major, None)
    };
    if let Some(w) = warned {
        senv.logger.line(&w).await;
    }
    let node = resolve_node(senv, major).await?;

    let env = action_env.to_vec();
    // `pre` runs before the main entrypoint, gated by `pre-if`
    // (default `always()` per GitHub).
    if let Some(pre) = &spec.runs.pre {
        let pre_allowed =
            match expr::eval_bool(spec.runs.pre_if.as_deref().unwrap_or("always()"), ctx) {
                Ok(b) => b,
                Err(e) => {
                    senv.logger
                        .line(&format!(
                            "::warning::pre-if evaluation failed: {e} — skipped"
                        ))
                        .await;
                    false
                }
            };
        if pre_allowed {
            let script = action_dir.join(pre);
            let mut cmd = Command::new(&node);
            cmd.arg(&script).current_dir(senv.workspace);
            let code = run_streamed(&mut cmd, &env, senv.logger, timeout).await?;
            if code != 0 {
                return Err(format!("pre '{pre}' exited {code}"));
            }
        }
    }
    let main = spec
        .runs
        .main
        .clone()
        .ok_or("node action declares no 'main'")?;
    let script = action_dir.join(&main);
    let mut cmd = Command::new(&node);
    cmd.arg(&script).current_dir(senv.workspace);
    let code = run_streamed(&mut cmd, &env, senv.logger, timeout).await?;
    if code != 0 {
        return Err(format!("'{main}' exited {code}"));
    }
    // `post` — LIFO after the job; `STATE_*` written via $GITHUB_STATE by
    // this action's pre/main is scoped to *its* post — drain it now.
    if let Some(post) = &spec.runs.post {
        let script = action_dir.join(post);
        let if_expr = spec
            .runs
            .post_if
            .clone()
            .unwrap_or_else(|| "always()".into());
        let mut post_env = env;
        state_env_for_post(senv.runner_temp, &mut post_env);
        senv.posts.push(PendingPost {
            name: format!("Post {step_id}"),
            if_expr: Some(if_expr),
            argv: vec![node, script.to_string_lossy().to_string()],
            env: post_env,
            cwd: senv.workspace.to_path_buf(),
        });
    }
    let _ = ctx;
    Ok(HashMap::new())
}

/// node binary resolution: tool-cache hit → PATH `node` with a matching
/// major → dist download into the tool cache → PATH `node` of any major
/// (self-hosted fallback — GitHub bundles exact externals, we degrade
/// gracefully with a loud warning when the network can't reach nodejs.org).
async fn resolve_node(senv: &StepEnv<'_>, major: u32) -> Result<String, String> {
    let cache = senv
        .runner_temp
        .join("tool-cache")
        .join("node")
        .join(major.to_string());
    let cached = cache.join("bin/node");
    if cached.is_file() {
        return Ok(cached.to_string_lossy().to_string());
    }
    // PATH node — record the discovered major for the fallback path.
    let path_major = match Command::new("node").arg("--version").output().await {
        Ok(out) => {
            let v = String::from_utf8_lossy(&out.stdout);
            let found = v
                .trim()
                .trim_start_matches('v')
                .split('.')
                .next()
                .and_then(|m| m.parse::<u32>().ok());
            if found == Some(major) {
                return Ok("node".into());
            }
            found
        }
        Err(_) => None,
    };
    // Download `node-vX.Y.Z-linux-arch.tar.gz` — resolve latest patch of the
    // major via the dist index (network required).
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    let index_url = "https://nodejs.org/dist/index.json";
    let releases: serde_json::Value = match reqwest::get(index_url).await {
        Ok(resp) => match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                return path_node_fallback(senv, major, path_major, &e.to_string()).await;
            }
        },
        Err(e) => {
            return path_node_fallback(senv, major, path_major, &e.to_string()).await;
        }
    };
    let ver = releases
        .as_array()
        .and_then(|a| {
            a.iter()
                .filter_map(|r| r.get("version").and_then(|v| v.as_str()))
                .find(|v| {
                    v.trim_start_matches('v')
                        .split('.')
                        .next()
                        .and_then(|m| m.parse::<u32>().ok())
                        == Some(major)
                })
        })
        .ok_or_else(|| format!("no node{major} release in dist index"));
    let ver = match ver {
        Ok(v) => v,
        Err(e) => return path_node_fallback(senv, major, path_major, &e).await,
    };
    let pkg = format!("node-{ver}-linux-{arch}.tar.gz");
    let url = format!("https://nodejs.org/dist/{ver}/{pkg}");
    senv.logger
        .line(&format!("Downloading node {ver} → tool-cache"))
        .await;
    let bytes = match reqwest::get(&url).await {
        Ok(resp) => match resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return path_node_fallback(senv, major, path_major, &e.to_string()).await;
            }
        },
        Err(e) => {
            return path_node_fallback(senv, major, path_major, &e.to_string()).await;
        }
    };
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let tmp = cache.join(&pkg);
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    let mut cmd = Command::new("tar");
    cmd.args(["xzf"])
        .arg(&tmp)
        .arg("--strip-components=1")
        .current_dir(&cache);
    let out = cmd.output().await.map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&cache);
        return Err(format!(
            "node tar extract failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(cached.to_string_lossy().to_string())
}

/// Last-resort node: whatever major PATH `node` is. Loud warning — the action
/// asked for a specific runtime and we could not supply it.
async fn path_node_fallback(
    senv: &StepEnv<'_>,
    major: u32,
    path_major: Option<u32>,
    why: &str,
) -> Result<String, String> {
    match path_major {
        Some(found) => {
            senv.logger
                .line(&format!(
                    "::warning::could not provision node{major} ({why}) — running action under PATH node v{found}"
                ))
                .await;
            Ok("node".into())
        }
        None => Err(format!(
            "node{major} unavailable: {why}, and no `node` on PATH"
        )),
    }
}

/// `docker://image` `uses:` — pull if absent, then run with the standard
/// workspace/temp mounts.
async fn docker_run_image(
    senv: &StepEnv<'_>,
    image: &str,
    args: &str,
    step_env: &[(String, String)],
    timeout: Duration,
) -> Result<(), String> {
    docker_pull_if_missing(senv, image, step_env).await?;
    let mut argv: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "-v".into(),
        format!("{}:/workspace", senv.workspace.to_string_lossy()),
        "-v".into(),
        format!("{}:/__runner_temp", senv.runner_temp.to_string_lossy()),
    ];
    for (k, _) in step_env {
        argv.push("-e".into());
        argv.push(k.clone());
    }
    argv.push("-w".into());
    argv.push("/workspace".into());
    argv.push(image.to_string());
    argv.extend(args.split_whitespace().map(String::from));
    let mut cmd = Command::new("docker");
    cmd.args(&argv).current_dir(senv.workspace);
    let code = run_streamed(&mut cmd, step_env, senv.logger, timeout).await?;
    if code != 0 {
        return Err(format!("docker run exited {code}"));
    }
    Ok(())
}

/// `runs.using: docker` — `image: Dockerfile` builds from the action dir;
/// `image: <tag>` pulls. entrypoint+args run inside the container;
/// `post-entrypoint` queues a LIFO post like node actions.
async fn run_docker_action(
    senv: &mut StepEnv<'_>,
    step: &Step,
    spec: &ActionSpec,
    action_dir: &Path,
    action_env: &[(String, String)],
    timeout: Duration,
) -> Result<HashMap<String, String>, String> {
    let image = spec
        .runs
        .image
        .clone()
        .ok_or("docker action declares no 'image'")?;
    let tag = if image == "Dockerfile" || image.ends_with("/Dockerfile") {
        // Build from the action's Dockerfile.
        let t = format!(
            "oxidean-action-{:x}",
            fnv(action_dir.to_string_lossy().as_bytes())
        );
        let mut cmd = Command::new("docker");
        cmd.args(["build", "-t", &t, "-f"])
            .arg(action_dir.join(&image))
            .arg(action_dir);
        let code = run_streamed(&mut cmd, action_env, senv.logger, timeout).await?;
        if code != 0 {
            return Err(format!("docker build exited {code}"));
        }
        t
    } else {
        docker_pull_if_missing(senv, &image, action_env).await?;
        image.clone()
    };
    let mut argv: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "-v".into(),
        format!("{}:/github/workspace", senv.workspace.to_string_lossy()),
        "-v".into(),
        format!("{}:/github/home", senv.runner_temp.to_string_lossy()),
    ];
    for (k, _) in action_env {
        argv.push("-e".into());
        argv.push(k.clone());
    }
    // action-level `env:` entries — forwarded into the container.
    if let Some(em) = &spec.runs.env {
        if let Ok(em) = serde_json::to_value(em) {
            let mut e = action_env.to_vec();
            let mut c = Context::new();
            for _ in layer_env(&mut c, &mut e, &em) {}
            for (k, v) in &e {
                if !action_env.iter().any(|(ak, _)| ak == k) {
                    argv.push("-e".into());
                    argv.push(format!("{k}={v}"));
                }
            }
        }
    }
    argv.push("-w".into());
    argv.push("/github/workspace".into());
    let argv_base = argv.clone();
    argv.extend(entrypoint_argv(&spec.runs.entrypoint));
    argv.push(tag.clone());
    if let Some(args) = &spec.runs.args {
        if let Some(arr) = args.as_sequence() {
            for a in arr {
                argv.push(a.as_str().unwrap_or_default().to_string());
            }
        }
    }
    let mut cmd = Command::new("docker");
    cmd.args(&argv).current_dir(senv.workspace);
    let code = run_streamed(&mut cmd, action_env, senv.logger, timeout).await?;
    if code != 0 {
        return Err(format!("docker action exited {code}"));
    }
    // `post-entrypoint` — same mounts/env, LIFO after the job; STATE_* from
    // this action's run scopes to its post.
    if let Some(pep) = &spec.runs.post_entrypoint {
        let if_expr = spec
            .runs
            .post_if
            .clone()
            .unwrap_or_else(|| "always()".into());
        let mut post_env = action_env.to_vec();
        state_env_for_post(senv.runner_temp, &mut post_env);
        let mut pargv = vec!["docker".to_string()];
        pargv.extend(argv_base);
        pargv.extend(entrypoint_argv(&Some(pep.clone())));
        pargv.push(tag);
        senv.posts.push(PendingPost {
            name: format!("Post {}", step.id.as_deref().unwrap_or("docker-action")),
            if_expr: Some(if_expr),
            argv: pargv,
            env: post_env,
            cwd: senv.workspace.to_path_buf(),
        });
    }
    Ok(HashMap::new())
}

fn entrypoint_argv(ep: &Option<String>) -> Vec<String> {
    match ep {
        Some(e) => vec!["--entrypoint".into(), e.clone()],
        None => Vec::new(),
    }
}

/// `docker pull` only when the image isn't in the local store.
async fn docker_pull_if_missing(
    senv: &StepEnv<'_>,
    image: &str,
    env: &[(String, String)],
) -> Result<(), String> {
    let mut inspect = Command::new("docker");
    inspect.args(["image", "inspect", image]);
    match inspect.output().await {
        Ok(o) if o.status.success() => return Ok(()),
        Ok(_) => {}
        Err(e) => return Err(format!("docker unavailable: {e}")),
    }
    senv.logger.line(&format!("Pulling {image}")).await;
    let mut cmd = Command::new("docker");
    cmd.args(["pull", image]);
    let code = run_streamed(&mut cmd, env, senv.logger, Duration::from_secs(600)).await?;
    if code != 0 {
        return Err(format!("docker pull {image} exited {code}"));
    }
    Ok(())
}

/// Drain `$GITHUB_STATE` into `STATE_*` env vars for post actions. Callers
/// (mod.rs post loop) read this before running posts.
pub(crate) fn state_env_for_post(runner_temp: &Path, env: &mut Vec<(String, String)>) {
    let f = runner_temp.join("state");
    if let Ok(text) = std::fs::read_to_string(&f) {
        for (k, v) in parse_env_file(&text) {
            env_set(env, &format!("STATE_{k}"), v);
        }
        let _ = std::fs::write(&f, b"");
    }
}

/// Tiny FNV-1a for deterministic docker tag names.
fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::ls_remote_commit;
    use crate::exec::Step;
    use oxidean_actions_expr::Context;

    /// Declared `default:` values survive YAML non-string scalars and get
    /// `${{ }}` evaluation — the github-script `debug: false` default must
    /// reach `INPUT_DEBUG` as "false" or `getBooleanInput` throws.
    #[test]
    fn resolve_inputs_applies_scalar_and_expr_defaults() {
        use super::{resolve_inputs, ActionSpec};
        let spec: ActionSpec = serde_yaml::from_str(
            r#"
name: t
inputs:
  debug: {default: false}
  retries: {default: 3}
  github-token: {default: "${{ github.token }}"}
  script: {required: true}
runs: {using: node20, main: index.js}
"#,
        )
        .unwrap();
        let step = Step {
            with: Some(serde_json::json!({"script": "1+1"})),
            ..step_stub()
        };
        let ctx = Context::new().with_context("github", serde_json::json!({"token": "ghs_test"}));
        let inputs = resolve_inputs(&spec, &step, &ctx).unwrap();
        assert_eq!(inputs["debug"], "false");
        assert_eq!(inputs["retries"], "3");
        assert_eq!(inputs["github-token"], "ghs_test");
        assert_eq!(inputs["script"], "1+1");
        // required input missing → error
        let step2 = Step {
            with: None,
            ..step_stub()
        };
        assert!(resolve_inputs(&spec, &step2, &ctx).is_err());
    }

    fn step_stub() -> Step {
        Step {
            continue_on_error: None,
            id: None,
            name: None,
            uses: None,
            run: None,
            if_expr: None,
            timeout_minutes: None,
            shell: None,
            working_directory: None,
            with: None,
            env: None,
        }
    }

    /// `INPUT_*` naming matches the toolkit's own lookup —
    /// `process.env['INPUT_' + name.replace(/ /g,'_').toUpperCase()]`: only
    /// spaces become underscores; hyphens (node-version, check-latest,
    /// github-token) survive or `@actions/core.getInput` sees an empty string.
    #[test]
    fn input_env_names_preserve_hyphens() {
        use super::normalize_input_key;
        assert_eq!(normalize_input_key("node-version"), "NODE-VERSION");
        assert_eq!(normalize_input_key("check-latest"), "CHECK-LATEST");
        assert_eq!(normalize_input_key("github-token"), "GITHUB-TOKEN");
        assert_eq!(normalize_input_key("path"), "PATH");
        assert_eq!(normalize_input_key("token file"), "TOKEN_FILE");
    }

    /// `git ls-remote` against a local bare repo (file transport — no
    /// network): the movable ref resolves to its commit sha, an annotated
    /// tag resolves to the peeled commit, and a missing ref yields None.
    #[tokio::test]
    async fn ls_remote_resolves_local_repo() {
        let dir = std::env::temp_dir().join(format!("oxidean-lsremote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str], cwd: &std::path::Path| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(cwd)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("HOME", cwd)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {:?}", out);
        };
        let src = dir.join("src");
        git(&["init", "-q", "--bare", "src"], &dir);
        let work = dir.join("work");
        git(&["init", "-q", "work"], &dir);
        git(&["-C", "work", "config", "user.email", "t@t"], &dir);
        git(&["-C", "work", "config", "user.name", "t"], &dir);
        std::fs::write(work.join("f"), b"x").unwrap();
        git(&["-C", "work", "add", "f"], &dir);
        git(&["-C", "work", "commit", "-qm", "c1"], &dir);
        let head = String::from_utf8(
            std::process::Command::new("git")
                .args(["-C", "work", "rev-parse", "HEAD"])
                .current_dir(&dir)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let head = head.trim().to_string();
        git(&["-C", "work", "tag", "-a", "v1", "-m", "v1"], &dir);
        git(&["-C", "work", "tag", "v2"], &dir);
        git(&["-C", "work", "branch", "dev"], &dir);
        git(
            &["-C", "work", "push", "-q", src.to_str().unwrap(), "--all"],
            &dir,
        );
        git(
            &["-C", "work", "push", "-q", src.to_str().unwrap(), "--tags"],
            &dir,
        );

        let env: Vec<(String, String)> = vec![];
        let url = src.to_str().unwrap();
        // Annotated tag → peeled commit; lightweight tag → commit; branch → commit.
        assert_eq!(
            ls_remote_commit(&env, url, &[], "v1").await.as_deref(),
            Some(head.as_str())
        );
        assert_eq!(
            ls_remote_commit(&env, url, &[], "v2").await.as_deref(),
            Some(head.as_str())
        );
        assert_eq!(
            ls_remote_commit(&env, url, &[], "dev").await.as_deref(),
            Some(head.as_str())
        );
        assert_eq!(ls_remote_commit(&env, url, &[], "no-such-ref").await, None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase 9: the tool-cache keys on the *resolved* commit — moving a tag
    /// to a new commit must fetch fresh code into a second cache dir instead
    /// of serving the stale ref-keyed one.
    #[tokio::test]
    async fn action_cache_rekeys_on_moved_ref() {
        use super::{fetch_remote_action, PendingPost, StepEnv};
        use crate::client::{Client, Task};
        use crate::config::{Config, ExecMode};
        use crate::exec::Logger;
        use std::collections::HashSet;
        use std::path::PathBuf;

        let dir = std::env::temp_dir().join(format!("oxidean-actcache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str], cwd: &std::path::Path| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(cwd)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("HOME", cwd)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {:?}", out);
        };

        // Local-transport "instance": origin/<owner>/<repo>.git is a bare repo.
        let origin = dir.join("origin");
        let bare = origin.join("acme").join("demo.git");
        git(&["init", "-q", "--bare", bare.to_str().unwrap()], &dir);
        git(
            &[
                "-C",
                bare.to_str().unwrap(),
                "config",
                "uploadpack.allowAnySHA1InWant",
                "true",
            ],
            &dir,
        );
        let work = dir.join("work");
        git(&["init", "-q", work.to_str().unwrap()], &dir);
        git(
            &["-C", work.to_str().unwrap(), "config", "user.email", "t@t"],
            &dir,
        );
        git(
            &["-C", work.to_str().unwrap(), "config", "user.name", "t"],
            &dir,
        );
        std::fs::write(
            work.join("action.yml"),
            b"runs:\n  using: composite\n  steps: []\n",
        )
        .unwrap();
        git(&["-C", work.to_str().unwrap(), "add", "."], &dir);
        git(&["-C", work.to_str().unwrap(), "commit", "-qm", "c1"], &dir);
        let rev = |w: &std::path::Path| -> String {
            String::from_utf8(
                std::process::Command::new("git")
                    .args(["rev-parse", "HEAD"])
                    .current_dir(w)
                    .output()
                    .unwrap()
                    .stdout,
            )
            .unwrap()
            .trim()
            .to_string()
        };
        let sha1 = rev(&work);
        git(&["-C", work.to_str().unwrap(), "tag", "-f", "v1"], &dir);
        git(
            &[
                "-C",
                work.to_str().unwrap(),
                "push",
                "-qf",
                bare.to_str().unwrap(),
                "--all",
            ],
            &dir,
        );
        git(
            &[
                "-C",
                work.to_str().unwrap(),
                "push",
                "-qf",
                bare.to_str().unwrap(),
                "--tags",
            ],
            &dir,
        );

        let runner_temp = dir.join("rt");
        let workspace = dir.join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let config = Config {
            origin: origin.to_str().unwrap().to_string(),
            registration_token: None,
            name: "t".into(),
            labels: vec![],
            state_path: PathBuf::from("/tmp/x"),
            work_dir: workspace.clone(),
            poll_ms: 1,
            wait_ms: 0,
            git_token: None,
            job_timeout_secs: 60,
            // Point the fallback at an empty local root — no network egress.
            actions_mirror: Some(dir.join("mirror").to_str().unwrap().to_string()),
        };
        // Logger's client needs a *real* socket — a dead port blackholes the
        // connect on some networks instead of refusing it, stalling send().
        let stub = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = stub.local_addr().unwrap().port();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            while let Ok((mut s, _)) = stub.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 8192];
                    let _ = s.read(&mut buf).await;
                    let _ = s
                        .write_all(b"HTTP/1.1 500\r\ncontent-length: 0\r\n\r\n")
                        .await;
                });
            }
        });
        let client = Client::new(&format!("http://127.0.0.1:{port}"), None);
        let logger = Logger {
            client: &client,
            run_id: "r".into(),
            job_id: "j".into(),
            mask_values: &[],
            cancelled: std::sync::atomic::AtomicBool::new(false),
            log_cursor: std::sync::atomic::AtomicI64::new(0),
            seen_cmd_warns: std::sync::Mutex::new(std::collections::BTreeSet::new()),
        };
        let mode = ExecMode::Host;
        let mut env: Vec<(String, String)> = vec![];
        let reserved: HashSet<String> = HashSet::new();
        let mut posts: Vec<PendingPost> = vec![];
        let task: Task = serde_json::from_value(serde_json::json!({})).unwrap();
        let senv = StepEnv {
            task: &task,
            config: &config,
            mode: &mode,
            workspace: &workspace,
            runner_temp: &runner_temp,
            env: &mut env,
            reserved: &reserved,
            logger: &logger,
            posts: &mut posts,
        };

        let d1 = fetch_remote_action(&senv, "acme", "demo", "", "v1")
            .await
            .expect("first fetch");
        assert!(
            d1.ends_with(format!("v1@{sha1}")),
            "first cache keyed by resolved sha: {d1:?}"
        );
        assert!(d1.join("action.yml").is_file(), "clone materialized");

        // Move the tag → a second commit; the cache must re-resolve.
        std::fs::write(
            work.join("action.yml"),
            b"runs:\n  using: composite\n  steps: [{ run: 'echo v2' }]\n",
        )
        .unwrap();
        git(
            &["-C", work.to_str().unwrap(), "commit", "-qam", "c2"],
            &dir,
        );
        let sha2 = rev(&work);
        git(&["-C", work.to_str().unwrap(), "tag", "-f", "v1"], &dir);
        git(
            &[
                "-C",
                work.to_str().unwrap(),
                "push",
                "-qf",
                bare.to_str().unwrap(),
                "--tags",
            ],
            &dir,
        );

        let d2 = fetch_remote_action(&senv, "acme", "demo", "", "v1")
            .await
            .expect("second fetch after tag move");
        assert!(
            d2.ends_with(format!("v1@{sha2}")),
            "moved ref produced a fresh sha-keyed cache: {d2:?}"
        );
        assert_ne!(d1, d2);
        assert!(
            std::fs::read_to_string(d2.join("action.yml"))
                .map(|s| s.contains("v2"))
                .unwrap_or(false),
            "fresh content under the new sha key"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
