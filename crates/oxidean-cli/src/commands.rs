//! Command → procedure orchestration: config resolution, client wiring,
//! dispatch for every `ox` verb, and the multi-call chains behind
//! `pr checks` / `pr merge` / `issue edit`.

use std::io::Read;
use std::path::PathBuf;

use oxidean_core::{AppError, RpcResponse};
use serde_json::{json, Value};

use crate::args::{parse_repo, BodyArg, Command, Global, Invocation, ListFilters, ReviewAction};
use crate::config::{self, Config};
use crate::device;
use crate::keystore;
use crate::manifest;
use crate::output;
use crate::rpc::{self, CallError, Client};

pub const EXIT_OK: u8 = 0;
pub const EXIT_API: u8 = 1;
pub const EXIT_USAGE: u8 = 2;
pub const EXIT_AUTH: u8 = 3;

/// Failures that carry a process exit code.
#[derive(Debug)]
pub enum CliError {
    /// Exit 2 — bad flags, bad repo spec, no instance configured.
    Usage(String),
    /// Exit 2 — the instance manifest doesn't list a procedure this command
    /// needs (server older/different than the command expects).
    Unsupported(String),
    /// Exit 3 — no credentials, or the instance rejected them.
    Auth(String),
    /// Exit code derived from the error code (`auth.unauthenticated` → 3).
    Api(AppError),
    /// Exit 1 — network, IO, malformed responses.
    Transport(String),
    /// Exit 1 — config file problems.
    Config(config::ConfigError),
}

impl CliError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) | Self::Unsupported(_) => EXIT_USAGE,
            Self::Auth(_) => EXIT_AUTH,
            Self::Api(e) => rpc::error_exit_code(e),
            Self::Transport(_) | Self::Config(_) => EXIT_API,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(m) | Self::Unsupported(m) | Self::Auth(m) | Self::Transport(m) => {
                f.write_str(m)
            }
            Self::Api(e) => write!(f, "{}: {}", e.code, e.message),
            Self::Config(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<CallError> for CliError {
    fn from(e: CallError) -> Self {
        match e {
            CallError::Http {
                status: 401 | 403, ..
            } => CliError::Auth(e.to_string()),
            other => CliError::Transport(other.to_string()),
        }
    }
}

/// Loaded config plus its on-disk path (needed by `auth login` to persist).
pub struct Runtime {
    pub config: Config,
    pub config_path: PathBuf,
}

pub fn runtime() -> Result<Runtime, CliError> {
    let config_path = config::config_path().map_err(CliError::Config)?;
    let config = config::load(&config_path).map_err(CliError::Config)?;
    Ok(Runtime {
        config,
        config_path,
    })
}

/// What a command produced for stdout.
pub enum Out {
    /// Local (non-RPC) result. `json` is what `--json` prints instead.
    Report { text: String, json: Value },
    /// RPC result — `pretty` for default output; `envelope` verbatim for
    /// `--json`; `error` drives the exit code either way.
    Rpc {
        envelope: RpcResponse,
        pretty: String,
    },
}

pub(crate) fn resolve_instance(global: &Global, cfg: &Config) -> Result<String, CliError> {
    config::resolve_instance(
        global.instance.as_deref(),
        config::env_instance().as_deref(),
        cfg,
    )
    .map_err(CliError::Usage)?
    .ok_or_else(|| {
        CliError::Usage(
            "no instance configured — pass --instance, set OXIDEAN_INSTANCE, \
             or `ox auth login --instance <url>`"
                .to_string(),
        )
    })
}

fn client(global: &Global, rt: &Runtime) -> Result<Client, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
    let token = config::resolve_token(config::env_token().as_deref(), &rt.config, &instance);
    Ok(Client::new(&instance, token))
}

fn repo_parts(spec: &str) -> Result<(String, String), CliError> {
    parse_repo(spec).map_err(|e| CliError::Usage(e.0))
}

/// `{owner, name, number}` for issue/pull ref calls.
fn ref_input(spec: &str, number: u64) -> Result<Value, CliError> {
    let (owner, name) = repo_parts(spec)?;
    Ok(json!({ "owner": owner, "name": name, "number": number }))
}

/// `{owner, name}` for repo calls.
fn repo_input(spec: &str) -> Result<Value, CliError> {
    let (owner, name) = repo_parts(spec)?;
    Ok(json!({ "owner": owner, "name": name }))
}

/// Resolve a `--template <ref>` to the `stack_id` value `repo.create` wants:
/// a builtin stack id, `instance:<uuid>` / `repo:<uuid>` picker id, an
/// instance-pack slug, or `owner/name` of a template repo. Matching `id` is
/// exact; `source_label` is case-insensitive.
async fn resolve_template_ref(g: &Global, rt: &Runtime, tref: &str) -> Result<String, CliError> {
    let defaults = call(g, rt, "repo.createDefaults", json!({})).await?;
    let stacks = defaults["stacks"].as_array().cloned().unwrap_or_default();
    let lower = tref.to_lowercase();
    let hit = stacks.iter().find(|s| {
        s["id"].as_str() == Some(tref)
            || s["source_label"]
                .as_str()
                .is_some_and(|l| l.to_lowercase() == lower)
    });
    match hit {
        Some(s) => Ok(s["id"].as_str().unwrap_or_default().to_string()),
        None => {
            let mut refs: Vec<String> = stacks
                .iter()
                .map(|s| {
                    s["source_label"]
                        .as_str()
                        .unwrap_or_else(|| s["id"].as_str().unwrap_or("?"))
                        .to_string()
                })
                .collect();
            refs.sort();
            Err(CliError::Usage(format!(
                "ox repo create: unknown template '{tref}' — valid refs: {}",
                if refs.is_empty() {
                    "(none available on this instance)".into()
                } else {
                    refs.join(", ")
                }
            )))
        }
    }
}

/// One-line picker label for a stack option: `ref — label: description`.
fn template_label(opt_: &Value, ref_: &str) -> String {
    let label = opt_["label"].as_str().unwrap_or("");
    let desc = opt_["description"].as_str().unwrap_or("");
    match (label.is_empty(), desc.is_empty()) {
        (true, true) => ref_.to_string(),
        (false, true) => format!("{ref_} — {label}"),
        (true, false) => format!("{ref_} — {desc}"),
        (false, false) => format!("{ref_} — {label}: {desc}"),
    }
}

/// `issue.list` / `pull.list` request body — filters omitted unless set
/// (`--state all` passes through; the server treats it as "no filter").
fn list_input(spec: &str, filters: &ListFilters) -> Result<Value, CliError> {
    let (owner, name) = repo_parts(spec)?;
    let mut input = json!({ "owner": owner, "name": name });
    if let Some(map) = input.as_object_mut() {
        if let Some(v) = &filters.state {
            map.insert("state".into(), json!(v));
        }
        if let Some(v) = &filters.author {
            map.insert("author".into(), json!(v));
        }
        if let Some(v) = &filters.assignee {
            map.insert("assignee".into(), json!(v));
        }
        if let Some(v) = &filters.label {
            map.insert("label".into(), json!(v));
        }
        if let Some(v) = &filters.q {
            map.insert("q".into(), json!(v));
        }
        if let Some(v) = filters.limit {
            map.insert("limit".into(), json!(v));
        }
    }
    Ok(input)
}

/// Insert the optional fields into an `{owner, name}` input.
fn opt(map: &mut serde_json::Map<String, Value>, key: &str, v: Option<impl Into<Value>>) {
    if let Some(v) = v {
        map.insert(key.into(), v.into());
    }
}

/// Run one RPC and render `data` for text mode. The full envelope is kept so
/// `--json` prints it verbatim and `{ok:false}` drives the exit code.
async fn invoke(
    global: &Global,
    rt: &Runtime,
    procedure: &str,
    input: Value,
    render: impl Fn(&Value) -> String,
) -> Result<Out, CliError> {
    let resp = client(global, rt)?.call(procedure, input).await?;
    let pretty = match &resp {
        RpcResponse::Ok { data, .. } => render(data),
        RpcResponse::Err { .. } => String::new(),
    };
    Ok(Out::Rpc {
        envelope: resp,
        pretty,
    })
}

/// One RPC whose Ok data is returned raw (multi-call chains).
async fn call(
    global: &Global,
    rt: &Runtime,
    procedure: &str,
    input: Value,
) -> Result<Value, CliError> {
    match client(global, rt)?.call(procedure, input).await? {
        RpcResponse::Ok { data, .. } => Ok(data),
        RpcResponse::Err { error, .. } => Err(CliError::Api(error)),
    }
}

/// `--body` | `--body-file` (`-` or a path) | piped stdin — the gh-style
/// content flag used by create/comment/edit verbs.
fn body_text(b: &BodyArg, what: &str) -> Result<Option<String>, CliError> {
    if let Some(b) = &b.body {
        return Ok(Some(b.clone()));
    }
    if let Some(path) = &b.body_file {
        return read_body_source(path).map(Some);
    }
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        return Ok(None);
    }
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::Transport(format!("reading {what} from stdin: {e}")))?;
    if buf.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(buf))
}

/// Fail when no body came from anywhere (for verbs that require one).
fn require_body(b: &BodyArg, what: &str) -> Result<String, CliError> {
    body_text(b, what)?
        .ok_or_else(|| CliError::Usage(format!("{what} needs --body, --body-file, or piped stdin")))
}

/// `--file`/`--key-file` source: `-` or missing file → stdin.
fn read_body_source(path: &str) -> Result<String, CliError> {
    if path == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| CliError::Transport(format!("reading stdin: {e}")))?;
        return Ok(buf);
    }
    std::fs::read_to_string(path).map_err(|e| CliError::Transport(format!("reading {path}: {e}")))
}

/// `pull.get` → head sha — used by `pr checks`.
async fn pull_head_sha(
    global: &Global,
    rt: &Runtime,
    repo: &str,
    number: u64,
) -> Result<Value, CliError> {
    call(global, rt, "pull.get", ref_input(repo, number)?).await
}

async fn pr_checks(
    global: &Global,
    rt: &Runtime,
    repo: &str,
    number: u64,
) -> Result<Out, CliError> {
    let pull = pull_head_sha(global, rt, repo, number).await?;
    let sha = output::s(&pull, "head_sha").unwrap_or_default().to_string();
    invoke(
        global,
        rt,
        "repo.commitStatus.list",
        {
            let (owner, name) = repo_parts(repo)?;
            json!({ "owner": owner, "name": name, "sha": sha })
        },
        output::checks,
    )
    .await
}

/// `pr merge` without --method — first strategy `repo.mergeSettings.get` allows.
async fn default_merge_method(
    global: &Global,
    rt: &Runtime,
    repo: &str,
) -> Result<String, CliError> {
    let data = call(global, rt, "repo.mergeSettings.get", repo_input(repo)?).await?;
    let settings = data.get("settings").unwrap_or(&data);
    for (field, method) in [
        ("allow_merge_commit", "merge"),
        ("allow_squash_merge", "squash"),
        ("allow_rebase_merge", "rebase"),
    ] {
        if settings
            .get(field)
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(method.to_string());
        }
    }
    Err(CliError::Usage(
        "repo allows no merge strategy — pass --method or ask a repo admin".into(),
    ))
}

/// `issue edit --label` — resolve label names to ids via label.listForRepo.
async fn resolve_label_ids(
    global: &Global,
    rt: &Runtime,
    repo: &str,
    names: &[String],
) -> Result<Vec<String>, CliError> {
    let data = call(global, rt, "label.listForRepo", repo_input(repo)?).await?;
    let labels = data
        .get("labels")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut ids = Vec::new();
    for name in names {
        let hit = labels.iter().find(|l| {
            output::s(l, "name")
                .map(|n| n.eq_ignore_ascii_case(name))
                .unwrap_or(false)
        });
        match hit.and_then(|l| output::s(l, "id")) {
            Some(id) => ids.push(id.to_string()),
            None => {
                return Err(CliError::Usage(format!(
                    "no label '{name}' in {repo} — `ox label list {repo}`"
                )));
            }
        }
    }
    Ok(ids)
}

/// `issue edit --assignee` — resolve usernames to ids via issue.assigneeCandidates.
async fn resolve_assignee_ids(
    global: &Global,
    rt: &Runtime,
    repo: &str,
    usernames: &[String],
) -> Result<Vec<String>, CliError> {
    let data = call(global, rt, "issue.assigneeCandidates", repo_input(repo)?).await?;
    let users = data
        .get("users")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut ids = Vec::new();
    for u in usernames {
        let hit = users.iter().find(|c| {
            output::s(c, "username")
                .map(|n| n.eq_ignore_ascii_case(u))
                .unwrap_or(false)
        });
        match hit.and_then(|c| output::s(c, "user_id")) {
            Some(id) => ids.push(id.to_string()),
            None => {
                return Err(CliError::Usage(format!(
                    "'{u}' isn't an assignee candidate on {repo}"
                )));
            }
        }
    }
    Ok(ids)
}

/// collaborator username → user_id (collaborators.update/remove take ids).
async fn resolve_collaborator_id(
    global: &Global,
    rt: &Runtime,
    repo: &str,
    username: &str,
) -> Result<String, CliError> {
    let data = call(global, rt, "repo.collaborators.list", repo_input(repo)?).await?;
    data.get("collaborators")
        .and_then(Value::as_array)
        .and_then(|cs| {
            cs.iter().find(|c| {
                output::s(c, "username")
                    .map(|u| u.eq_ignore_ascii_case(username))
                    .unwrap_or(false)
            })
        })
        .and_then(|c| output::s(c, "user_id"))
        .map(str::to_string)
        .ok_or_else(|| CliError::Usage(format!("'{username}' is not a collaborator on {repo}")))
}

/// `ox browse` / `pr checkout` / `repo clone` — open a URL in the user's
/// browser (or spawn git). Kept dependency-free: platform opener binaries.
fn open_url(url: &str) -> Result<(), CliError> {
    let (prog, pre): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(target_os = "windows") {
        ("cmd", &["/c", "start", ""])
    } else {
        // linux, freebsd, and the rest fall back to xdg-open.
        ("xdg-open", &[])
    };
    let mut cmd = std::process::Command::new(prog);
    cmd.args(pre).arg(url);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| CliError::Transport(format!("cannot open browser ({prog}): {e}")))
}

fn run_git(args: &[&str]) -> Result<(), CliError> {
    let status = std::process::Command::new("git")
        .args(args)
        .status()
        .map_err(|e| CliError::Transport(format!("cannot run git: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::Transport(format!("git exited {status}")))
    }
}

/// Procedures each command needs — the manifest gate rejects commands the
/// instance doesn't advertise (older servers) with exit 2.
fn required_procedures(cmd: &Command) -> Vec<String> {
    let one = |p: &str| vec![p.to_string()];
    match cmd {
        Command::AuthLogin { .. }
        | Command::AuthToken
        | Command::AuthLogout
        | Command::AuthSwitch { .. }
        | Command::AuthSetupGit
        | Command::AuthGitCredential { .. }
        | Command::ConfigList
        | Command::ConfigGet { .. }
        | Command::ConfigSet { .. }
        | Command::Completion { .. } => vec![],
        // `pat.self` rides along best-effort (older servers lack it) — only
        // `auth.me` is strictly required.
        Command::AuthStatus => one("auth.me"),
        Command::RepoList { owner } => vec![if owner.is_some() {
            "repo.listByOwner"
        } else {
            "repo.listMine"
        }
        .into()],
        Command::RepoView { .. } => one("repo.get"),
        // createDefaults is needed to resolve --template or run the wizard.
        Command::RepoCreate { name, template, .. } => {
            if name.is_none() || template.is_some() {
                vec!["repo.create".into(), "repo.createDefaults".into()]
            } else {
                vec!["repo.create".into()]
            }
        }
        Command::RepoTemplates => one("repo.createDefaults"),
        Command::RepoFork { .. } => one("repo.fork"),
        Command::RepoSync { .. } => vec!["repo.forkStatus".into(), "repo.syncFork".into()],
        Command::RepoClone { .. } => vec![], // git only
        Command::RepoEdit { visibility, .. } => vec![if visibility.is_some() {
            "repo.updateVisibility"
        } else {
            "repo.updateMetadata"
        }
        .into()],
        Command::RepoRename { .. } => one("repo.rename"),
        Command::RepoSetArchived { .. } => one("repo.setArchived"),
        Command::RepoDelete { .. } => one("repo.softDelete"),
        Command::RepoTransfer { .. } => one("repo.transfer"),
        Command::RepoStar { star, .. } => one(if *star { "repo.star" } else { "repo.unstar" }),
        Command::RepoWatch { watch, .. } => one(if *watch { "repo.watch" } else { "repo.unwatch" }),
        Command::DeployKeyList { .. } => one("repo.deployKey.list"),
        Command::DeployKeyAdd { .. } => one("repo.deployKey.create"),
        Command::DeployKeyDelete { .. } => one("repo.deployKey.delete"),
        Command::CollaboratorList { .. } => one("repo.collaborators.list"),
        Command::CollaboratorAdd { .. } => one("repo.collaborators.add"),
        Command::CollaboratorSetRole { .. } => vec![
            "repo.collaborators.list".into(),
            "repo.collaborators.update".into(),
        ],
        Command::CollaboratorRemove { .. } => vec![
            "repo.collaborators.list".into(),
            "repo.collaborators.remove".into(),
        ],
        Command::RepoInviteList { .. } => one("repo.invites.list"),
        Command::RepoInviteCreate { .. } => one("repo.invites.create"),
        Command::RepoInviteLink { .. } => one("repo.invites.createLink"),
        Command::RepoInviteRevoke { .. } => one("repo.invites.revoke"),

        Command::BranchProtectionList { .. } => one("repo.branchProtection.list"),
        Command::BranchProtectionSet { .. } => one("repo.branchProtection.create"),
        Command::BranchProtectionDelete { .. } => one("repo.branchProtection.delete"),
        Command::TagProtectionList { .. } => one("repo.tagProtection.list"),
        Command::TagProtectionSet { .. } => one("repo.tagProtection.create"),
        Command::TagProtectionDelete { .. } => one("repo.tagProtection.delete"),
        Command::MirrorGet { .. } => one("repo.mirror.get"),
        Command::MirrorSet { .. } => one("repo.mirror.upsert"),
        Command::MirrorSync { .. } => one("repo.mirror.syncNow"),
        Command::MirrorDelete { .. } => one("repo.mirror.delete"),
        Command::MirrorGenKey { .. } => one("repo.mirror.generateSshKey"),
        Command::MirrorRotateSecret { .. } => one("repo.mirror.rotateWebhookSecret"),
        Command::MergeSettings {
            merge,
            squash,
            rebase,
            ..
        } => one(if merge.is_some() || squash.is_some() || rebase.is_some() {
            "repo.mergeSettings.update"
        } else {
            "repo.mergeSettings.get"
        }),
        Command::RepoLfs {
            usage,
            objects,
            enable,
            ..
        } => one(if enable.is_some() {
            "repo.lfs.setEnabled"
        } else if *usage {
            "repo.lfs.getUsage"
        } else if *objects {
            "repo.lfs.listObjects"
        } else {
            "repo.lfs.getStatus"
        }),
        Command::RepoQuota { set_bytes, .. } => one(if set_bytes.is_some() {
            "repo.quota.set"
        } else {
            "repo.quota.get"
        }),
        Command::IssueList { .. } => one("issue.list"),
        Command::IssueView { .. } => one("issue.get"),
        Command::IssueCreate { .. } => one("issue.create"),
        Command::IssueEdit {
            labels,
            assignees,
            title,
            body,
            ..
        } => {
            let mut p = Vec::new();
            if title.is_some() || body.body.is_some() || body.body_file.is_some() {
                p.push("issue.update".to_string());
            }
            if !labels.is_empty() {
                p.push("label.listForRepo".into());
                p.push("issue.labels.set".into());
            }
            if !assignees.is_empty() {
                p.push("issue.assigneeCandidates".into());
                p.push("issue.assignees.set".into());
            }
            if p.is_empty() {
                p.push("issue.get".to_string());
            }
            p
        }
        Command::IssueClose { .. } => one("issue.close"),
        Command::IssueReopen { .. } => one("issue.reopen"),
        Command::IssueDelete { .. } => one("issue.delete"),
        Command::IssueComment { .. } => one("issue.comments.create"),
        Command::PrList { .. } => one("pull.list"),
        Command::PrView { .. } => one("pull.get"),
        Command::PrChecks { .. } => vec!["pull.get".into(), "repo.commitStatus.list".into()],
        Command::PrCreate { .. } => one("pull.create"),
        Command::PrMerge { method, .. } => {
            let mut p = vec!["pull.merge".to_string()];
            if method.is_none() {
                // strategy resolved via repo.mergeSettings.get
                p.insert(0, "repo.mergeSettings.get".into());
            }
            p
        }
        Command::PrDiff { .. } | Command::PrFiles { .. } => one("pull.files"),
        Command::PrReview {
            list,
            dismiss_id,
            request,
            unrequest,
            ..
        } => {
            if *list {
                return one("pull.reviews.list");
            }
            if dismiss_id.is_some() {
                return one("pull.reviews.dismiss");
            }
            if request.is_some() {
                return one("pull.reviewRequests.add");
            }
            if unrequest.is_some() {
                return one("pull.reviewRequests.remove");
            }
            one("pull.reviews.submit")
        }
        Command::PrComment { .. } => one("pull.comments.create"),
        Command::PrReady { .. } => one("pull.update"),
        Command::PrUpdateBranch { .. } => {
            vec!["pull.branchStatus".into(), "pull.updateBranch".into()]
        }
        Command::PrClose { .. } => one("pull.close"),
        Command::PrReopen { .. } => one("pull.reopen"),
        Command::PrCheckout { .. } => one("pull.get"),
        Command::RunList { .. } => one("repo.actions.listRuns"),
        Command::RunView { log, job, .. } => {
            if *log && job.is_none() {
                return vec![
                    "repo.actions.getRun".into(),
                    "repo.actions.getJobLog".into(),
                ];
            }
            one(if *log {
                "repo.actions.getJobLog"
            } else {
                "repo.actions.getRun"
            })
        }
        Command::RunRerun { .. } => one("repo.actions.rerunRun"),
        Command::RunCancel { .. } => one("repo.actions.cancelRun"),
        Command::RunWatch { .. } => one("repo.actions.getRun"),
        Command::WorkflowList { .. } => one("repo.actions.listWorkflows"),
        Command::WorkflowView { git_ref, .. } => {
            let mut p = vec!["repo.blob".to_string()];
            if git_ref.is_none() {
                // --ref defaults to the repo's default branch via repo.get
                p.push("repo.get".into());
            }
            p
        }
        Command::WorkflowRun { git_ref, .. } => {
            let mut p = vec!["repo.actions.dispatchWorkflow".to_string()];
            if git_ref.is_none() {
                p.push("repo.get".into());
            }
            p
        }
        Command::SecretList { .. } => one("repo.actions.secrets.list"),
        Command::SecretSet { .. } => one("repo.actions.secrets.put"),
        Command::SecretDelete { .. } => one("repo.actions.secrets.delete"),
        Command::ReleaseList { .. } => one("release.list"),
        Command::ReleaseView { .. } => one("release.get"),
        Command::ReleaseCreate { .. } => one("release.create"),
        Command::ReleaseEdit { .. } => one("release.update"),
        Command::ReleaseDelete { .. } => one("release.delete"),
        Command::ReleaseUpload { .. } | Command::ReleaseDownload { .. } => one("release.get"),
        Command::ReleaseDeleteAsset { .. } => {
            vec!["release.get".into(), "release.deleteAsset".into()]
        }
        Command::Search { .. } => one("search.global"),
        Command::Browse { .. } => vec![],
        Command::StatusCreate { .. } => one("repo.commitStatus.create"),
        Command::OrgList => one("org.listMine"),
        Command::OrgView { .. } => one("org.get"),
        Command::OrgCreate { .. } => one("org.create"),
        Command::OrgMembers { .. } => one("org.members.list"),
        Command::OrgMemberAdd { .. } => one("org.members.add"),
        Command::OrgMemberRemove { .. } => {
            vec!["org.members.list".into(), "org.members.remove".into()]
        }
        Command::OrgMemberRole { .. } => {
            vec!["org.members.list".into(), "org.members.updateRole".into()]
        }
        Command::OrgInviteList { .. } => one("org.invites.list"),
        Command::OrgInviteCreate { .. } => one("org.invites.create"),
        Command::OrgInviteLink { .. } => one("org.invites.createLink"),
        Command::OrgInviteRevoke { .. } => one("org.invites.revoke"),
        Command::NotificationList { .. } => one("notification.list"),
        Command::NotificationRead { ids, all } => one(if *all || ids.is_empty() {
            "notification.markAllRead"
        } else {
            "notification.markRead"
        }),
        Command::NotificationUnread => one("notification.unreadCount"),
        Command::UserView { .. } => one("user.getPublicProfile"),
        Command::UserLookup { .. } => one("user.lookup"),
        Command::UserFollow { follow, .. } => one(if *follow {
            "user.follow"
        } else {
            "user.unfollow"
        }),
        Command::UserStarred => one("user.listStarred"),
        Command::UserWatched => one("user.listWatched"),
        Command::SshKeyList => one("sshKey.list"),
        Command::SshKeyAdd { .. } => one("sshKey.add"),
        Command::SshKeyDelete { .. } => one("sshKey.delete"),
        Command::GpgKeyList => one("gpgKey.list"),
        Command::GpgKeyAdd { .. } => one("gpgKey.add"),
        Command::GpgKeyDelete { .. } => one("gpgKey.delete"),
        Command::EmailList => one("email.list"),
        Command::EmailAdd { .. } => one("email.add"),
        Command::EmailRemove { .. } => one("email.remove"),
        Command::EmailSetPrimary { .. } => one("email.setPrimary"),
        Command::EmailResend { .. } => one("email.resendVerify"),
        Command::AuthRevoke => vec!["pat.self".into(), "pat.revoke".into()],
        Command::AuthRefresh { .. } => vec!["pat.self".into(), "pat.revoke".into()],
        Command::LabelList { org, .. } => one(if org.is_some() {
            "label.listForOrg"
        } else {
            "label.listForRepo"
        }),
        Command::LabelCreate { org, .. }
        | Command::LabelEdit { org, .. }
        | Command::LabelDelete { org, .. } => {
            let _ = org;
            one("label.create") // update/delete share the label.* family
        }
        Command::WebhookList { .. } => one("webhook.list"),
        Command::WebhookCreate { .. } => one("webhook.create"),
        Command::WebhookEdit { .. } => one("webhook.update"),
        Command::WebhookDelete { .. } => one("webhook.delete"),
        Command::WebhookPing { .. } => one("webhook.ping"),
        Command::WebhookDeliveries { .. } => one("webhook.deliveries.list"),
        Command::WebhookDelivery { .. } => one("webhook.deliveries.get"),
        Command::WebhookRedeliver { .. } => one("webhook.redeliver"),
        Command::PkgList { .. } => one("packages.list"),
        Command::Api { procedure, .. } => vec![procedure.clone()],
        // self-update talks to /cli/* HTTP, not /api/rpc — no manifest gate.
        // skill and mcp commands are fully local (mcp probes /api/mcp
        // directly, outside the RPC surface).
        Command::SelfUpdate { .. }
        | Command::SkillPrint
        | Command::SkillInstall { .. }
        | Command::McpPrint { .. }
        | Command::McpInstall { .. }
        | Command::McpStatus
        | Command::McpRemove { .. }
        | Command::Help
        | Command::HelpTopic(_)
        | Command::Version => vec![],
    }
}

/// CLI-02 compatibility gate: fetch `system.manifest` once (fail-open on
/// pre-manifest/unreachable instances), print advisory notices, and reject
/// commands whose procedures the instance doesn't advertise.
async fn gate(inv: &Invocation, rt: &Runtime) -> Result<(), CliError> {
    let required = required_procedures(&inv.command);
    if required.is_empty() {
        return Ok(());
    }
    let client = client(&inv.global, rt)?;
    let Some(manifest) = manifest::fetch(&client).await else {
        return Ok(());
    };
    for notice in manifest::compat_notices(&manifest, env!("CARGO_PKG_VERSION")) {
        eprintln!("ox: {notice}");
    }
    for procedure in required {
        if !manifest.supports(&procedure) {
            return Err(CliError::Unsupported(format!(
                "command not supported by instance (missing {procedure})"
            )));
        }
    }
    Ok(())
}

pub async fn dispatch(inv: &Invocation, rt: &Runtime) -> Result<Out, CliError> {
    let g = &inv.global;
    gate(inv, rt).await?;
    match &inv.command {
        Command::Help => Ok(Out::Report {
            text: crate::args::USAGE.to_string(),
            json: json!({"ok": true}),
        }),
        Command::HelpTopic(topic) => {
            let mut parts = topic.split_whitespace();
            let (grp, sub) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            let text = crate::args::help_topic(grp, sub)
                .unwrap_or(crate::args::USAGE)
                .to_string();
            Ok(Out::Report {
                text,
                json: json!({"ok": true}),
            })
        }
        Command::Version => Ok(Out::Report {
            text: format!("ox {}", env!("CARGO_PKG_VERSION")),
            json: json!({"ok": true, "data": {"version": env!("CARGO_PKG_VERSION")}}),
        }),
        Command::AuthLogin {
            token,
            token_stdin,
            web,
            scopes,
        } => {
            if *web {
                auth_login_web(g, rt, scopes.as_deref()).await
            } else {
                auth_login(g, rt, token.as_deref(), *token_stdin).await
            }
        }
        Command::AuthStatus => auth_status(g, rt).await,
        Command::AuthLogout => auth_logout(g, rt),
        Command::AuthToken => auth_token(g, rt),
        Command::AuthRefresh {
            scopes,
            reset_scopes,
        } => auth_refresh(g, rt, scopes.as_deref(), *reset_scopes).await,
        Command::AuthRevoke => auth_revoke(g, rt).await,
        Command::AuthSetupGit => auth_setup_git(g, rt),
        Command::AuthGitCredential { op } => auth_git_credential(g, rt, op),
        Command::AuthSwitch { instance } => auth_switch(rt, instance.as_deref()),
        Command::ConfigList => config_list(rt),
        Command::ConfigGet { key } => config_get(rt, key),
        Command::ConfigSet { key, value } => config_set(rt, key, value),
        Command::Completion { shell } => crate::completion::run(shell),

        // ── repos ───────────────────────────────────────────────────
        Command::RepoList { owner } => {
            let (procedure, input) = match owner {
                Some(owner) => ("repo.listByOwner", json!({ "owner": owner })),
                None => ("repo.listMine", json!({})),
            };
            invoke(g, rt, procedure, input, output::repo_list).await
        }
        Command::RepoView { repo } => {
            invoke(g, rt, "repo.get", repo_input(repo)?, output::repo_view).await
        }
        Command::RepoCreate {
            name,
            owner,
            description,
            visibility,
            license,
            gitignore,
            template,
        } => {
            let mut name = name.clone();
            let mut owner = owner.clone();
            let mut visibility = visibility.clone();
            let mut stack_id: Option<String> = None;

            if let Some(tref) = template {
                stack_id = Some(resolve_template_ref(g, rt, &tref).await?);
            } else if name.is_none() {
                // Interactive wizard — TTY only; non-TTY keeps the usage error.
                if !crate::term::can_prompt() {
                    return Err(CliError::Usage("usage: ox repo create <name>".into()));
                }
                let defaults = call(g, rt, "repo.createDefaults", json!({})).await?;
                let stacks = defaults["stacks"].as_array().cloned().unwrap_or_default();

                name = Some(
                    crate::prompt::input("Repository name")
                        .map_err(|e| CliError::Transport(e.to_string()))?
                        .trim()
                        .to_string(),
                );
                if name.as_deref().is_none_or(|n| n.is_empty()) {
                    return Err(CliError::Usage(
                        "ox repo create: name cannot be empty".into(),
                    ));
                }

                if owner.is_none() {
                    let mut owners = vec!["(your user account)".to_string()];
                    let orgs = call(g, rt, "org.listMine", json!({}))
                        .await
                        .ok()
                        .and_then(|d| d.as_array().cloned())
                        .unwrap_or_default();
                    for o in &orgs {
                        if let Some(s) = o["slug"].as_str() {
                            owners.push(s.to_string());
                        }
                    }
                    let pick = crate::prompt::select("Owner", &owners)
                        .map_err(|e| CliError::Transport(e.to_string()))?;
                    if pick > 0 {
                        owner = Some(owners[pick].clone());
                    }
                }

                let mut items = vec!["none — empty repository".to_string()];
                for s in &stacks {
                    let label = s["source_label"]
                        .as_str()
                        .unwrap_or_else(|| s["id"].as_str().unwrap_or("?"));
                    items.push(template_label(s, label));
                }
                let pick = crate::prompt::select("Template", &items)
                    .map_err(|e| CliError::Transport(e.to_string()))?;
                if pick > 0 {
                    stack_id = stacks[pick - 1]["id"].as_str().map(|s| s.to_string());
                }

                if visibility.is_none() {
                    let default_v = defaults["default_visibility"].as_str().unwrap_or("private");
                    let items = vec![
                        format!(
                            "private{}",
                            if default_v == "private" {
                                " (default)"
                            } else {
                                ""
                            }
                        ),
                        format!(
                            "public{}",
                            if default_v == "public" {
                                " (default)"
                            } else {
                                ""
                            }
                        ),
                    ];
                    let pick = crate::prompt::select("Visibility", &items)
                        .map_err(|e| CliError::Transport(e.to_string()))?;
                    visibility = Some(if pick == 0 { "private" } else { "public" }.to_string());
                }
            }

            let name = name.expect("name resolved by wizard or flag");
            let mut input = json!({ "name": name });
            let m = input.as_object_mut().unwrap();
            opt(m, "owner", owner.clone());
            opt(m, "description", description.clone());
            opt(m, "visibility", visibility.clone());
            opt(m, "license_id", license.clone());
            opt(m, "gitignore_id", gitignore.clone());
            opt(m, "stack_id", stack_id.clone());
            invoke(g, rt, "repo.create", input, output::repo_view).await
        }
        Command::RepoTemplates => {
            invoke(
                g,
                rt,
                "repo.createDefaults",
                json!({}),
                output::repo_templates,
            )
            .await
        }
        Command::RepoFork {
            repo,
            into_owner,
            into_name,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            opt(m, "into_owner", into_owner.clone());
            opt(m, "into_name", into_name.clone());
            invoke(g, rt, "repo.fork", input, output::repo_view).await
        }
        Command::RepoSync { repo } => {
            invoke(g, rt, "repo.syncFork", repo_input(repo)?, output::generic).await
        }
        Command::RepoClone { repo, dir } => {
            let instance = resolve_instance(g, &rt.config)?;
            let url = format!("{instance}/{repo}.git");
            let mut args = vec!["clone", url.as_str()];
            if let Some(d) = dir {
                args.push(d);
            }
            run_git(&args)?;
            Ok(Out::Report {
                text: format!("cloned {url}"),
                json: json!({"ok": true}),
            })
        }
        Command::RepoEdit {
            repo,
            description,
            homepage,
            topics,
            visibility,
        } => {
            // visibility lives on its own procedure; the rest ride updateMetadata.
            if let Some(vis) = visibility {
                let mut input = repo_input(repo)?;
                input
                    .as_object_mut()
                    .unwrap()
                    .insert("visibility".into(), json!(vis));
                let r = invoke(g, rt, "repo.updateVisibility", input, output::generic).await?;
                if description.is_none() && homepage.is_none() && topics.is_none() {
                    return Ok(r);
                }
            }
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            opt(m, "description", description.clone());
            opt(m, "homepage", homepage.clone());
            if let Some(t) = topics {
                m.insert(
                    "topics".into(),
                    Value::Array(t.iter().cloned().map(Value::String).collect()),
                );
            }
            invoke(g, rt, "repo.updateMetadata", input, output::generic).await
        }
        Command::RepoRename { repo, new_name } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("newName".into(), json!(new_name));
            invoke(g, rt, "repo.rename", input, output::generic).await
        }
        Command::RepoSetArchived { repo, archived } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("archived".into(), json!(archived));
            invoke(g, rt, "repo.setArchived", input, output::generic).await
        }
        Command::RepoDelete { repo, confirm } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("confirmName".into(), json!(confirm));
            invoke(g, rt, "repo.softDelete", input, output::generic).await
        }
        Command::RepoTransfer {
            repo,
            dest_owner,
            dest_owner_type,
            confirm,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("destOwner".into(), json!(dest_owner));
            m.insert("destOwnerType".into(), json!(dest_owner_type));
            m.insert("confirmName".into(), json!(confirm));
            invoke(g, rt, "repo.transfer", input, output::generic).await
        }
        Command::RepoStar { repo, star } => {
            invoke(
                g,
                rt,
                if *star { "repo.star" } else { "repo.unstar" },
                repo_input(repo)?,
                output::generic,
            )
            .await
        }
        Command::RepoWatch { repo, watch } => {
            invoke(
                g,
                rt,
                if *watch { "repo.watch" } else { "repo.unwatch" },
                repo_input(repo)?,
                output::generic,
            )
            .await
        }
        Command::DeployKeyList { repo } => {
            invoke(
                g,
                rt,
                "repo.deployKey.list",
                repo_input(repo)?,
                output::deploy_key_list,
            )
            .await
        }
        Command::DeployKeyAdd {
            repo,
            title,
            key_file,
            can_write,
        } => {
            let public_key = read_body_source(key_file)?;
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("title".into(), json!(title));
            m.insert("public_key".into(), json!(public_key.trim()));
            m.insert("can_write".into(), json!(can_write));
            invoke(g, rt, "repo.deployKey.create", input, output::generic).await
        }
        Command::DeployKeyDelete { repo, id } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("id".into(), json!(id));
            invoke(g, rt, "repo.deployKey.delete", input, output::generic).await
        }
        Command::CollaboratorList { repo } => {
            invoke(
                g,
                rt,
                "repo.collaborators.list",
                repo_input(repo)?,
                output::collaborator_list,
            )
            .await
        }
        Command::CollaboratorAdd {
            repo,
            username,
            permission,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("username".into(), json!(username));
            m.insert("permission".into(), json!(permission));
            invoke(g, rt, "repo.collaborators.add", input, output::generic).await
        }
        Command::CollaboratorSetRole {
            repo,
            username,
            permission,
        } => {
            let user_id = resolve_collaborator_id(g, rt, repo, username).await?;
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("user_id".into(), json!(user_id));
            m.insert("permission".into(), json!(permission));
            invoke(g, rt, "repo.collaborators.update", input, output::generic).await
        }
        Command::CollaboratorRemove { repo, username } => {
            let user_id = resolve_collaborator_id(g, rt, repo, username).await?;
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("user_id".into(), json!(user_id));
            invoke(g, rt, "repo.collaborators.remove", input, output::generic).await
        }
        Command::RepoInviteList { repo } => {
            invoke(
                g,
                rt,
                "repo.invites.list",
                repo_input(repo)?,
                output::invite_list,
            )
            .await
        }
        Command::RepoInviteCreate {
            repo,
            emails,
            permission,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert(
                "emails".into(),
                Value::Array(emails.iter().cloned().map(Value::String).collect()),
            );
            m.insert("permission".into(), json!(permission));
            invoke(
                g,
                rt,
                "repo.invites.create",
                input,
                output::invite_create_result,
            )
            .await
        }
        Command::RepoInviteLink {
            repo,
            permission,
            expires_at,
            max_uses,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("permission".into(), json!(permission));
            opt(m, "expires_at", expires_at.clone());
            opt(m, "max_uses", *max_uses);
            invoke(
                g,
                rt,
                "repo.invites.createLink",
                input,
                output::invite_link_result,
            )
            .await
        }
        Command::RepoInviteRevoke { repo, id } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("invite_id".into(), json!(id));
            invoke(g, rt, "repo.invites.revoke", input, output::generic).await
        }
        Command::BranchProtectionList { repo } => {
            invoke(
                g,
                rt,
                "repo.branchProtection.list",
                repo_input(repo)?,
                output::protection_rules,
            )
            .await
        }
        Command::BranchProtectionSet {
            repo,
            pattern,
            opts,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("pattern".into(), json!(pattern));
            if let Some(o) = opts.as_object() {
                for (k, v) in o {
                    m.insert(k.clone(), v.clone());
                }
            }
            invoke(
                g,
                rt,
                "repo.branchProtection.create",
                input,
                output::generic,
            )
            .await
        }
        Command::BranchProtectionDelete { repo, id } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("id".into(), json!(id));
            invoke(
                g,
                rt,
                "repo.branchProtection.delete",
                input,
                output::generic,
            )
            .await
        }
        Command::TagProtectionList { repo } => {
            invoke(
                g,
                rt,
                "repo.tagProtection.list",
                repo_input(repo)?,
                output::tag_protection_rules,
            )
            .await
        }
        Command::TagProtectionSet {
            repo,
            pattern,
            opts,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("pattern".into(), json!(pattern));
            if let Some(o) = opts.as_object() {
                for (k, v) in o {
                    m.insert(k.clone(), v.clone());
                }
            }
            invoke(g, rt, "repo.tagProtection.create", input, output::generic).await
        }
        Command::TagProtectionDelete { repo, id } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("id".into(), json!(id));
            invoke(g, rt, "repo.tagProtection.delete", input, output::generic).await
        }
        Command::MirrorGet { repo } => {
            invoke(g, rt, "repo.mirror.get", repo_input(repo)?, output::mirror).await
        }
        Command::MirrorSet { repo, opts } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            if let Some(o) = opts.as_object() {
                for (k, v) in o {
                    m.insert(k.clone(), v.clone());
                }
            }
            invoke(g, rt, "repo.mirror.upsert", input, output::mirror).await
        }
        Command::MirrorSync { repo } => {
            invoke(
                g,
                rt,
                "repo.mirror.syncNow",
                repo_input(repo)?,
                output::generic,
            )
            .await
        }
        Command::MirrorDelete { repo } => {
            invoke(
                g,
                rt,
                "repo.mirror.delete",
                repo_input(repo)?,
                output::generic,
            )
            .await
        }
        Command::MirrorGenKey { repo } => {
            invoke(
                g,
                rt,
                "repo.mirror.generateSshKey",
                repo_input(repo)?,
                output::generic,
            )
            .await
        }
        Command::MirrorRotateSecret { repo } => {
            invoke(
                g,
                rt,
                "repo.mirror.rotateWebhookSecret",
                repo_input(repo)?,
                output::generic,
            )
            .await
        }
        Command::MergeSettings {
            repo,
            merge,
            squash,
            rebase,
        } => {
            if merge.is_none() && squash.is_none() && rebase.is_none() {
                return invoke(
                    g,
                    rt,
                    "repo.mergeSettings.get",
                    repo_input(repo)?,
                    output::merge_settings,
                )
                .await;
            }
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            opt(m, "allow_merge_commit", *merge);
            opt(m, "allow_squash_merge", *squash);
            opt(m, "allow_rebase_merge", *rebase);
            invoke(
                g,
                rt,
                "repo.mergeSettings.update",
                input,
                output::merge_settings,
            )
            .await
        }
        Command::RepoLfs {
            repo,
            usage,
            objects,
            enable,
        } => {
            if let Some(on) = enable {
                let mut input = repo_input(repo)?;
                input
                    .as_object_mut()
                    .unwrap()
                    .insert("enabled".into(), json!(on));
                return invoke(g, rt, "repo.lfs.setEnabled", input, output::generic).await;
            }
            let (proc, render): (&str, fn(&Value) -> String) = if *objects {
                ("repo.lfs.listObjects", output::lfs_objects)
            } else if *usage {
                ("repo.lfs.getUsage", output::generic)
            } else {
                ("repo.lfs.getStatus", output::generic)
            };
            invoke(g, rt, proc, repo_input(repo)?, render).await
        }
        Command::RepoQuota { repo, set_bytes } => {
            if let Some(bytes) = set_bytes {
                let mut input = repo_input(repo)?;
                input
                    .as_object_mut()
                    .unwrap()
                    .insert("size_quota_bytes".into(), json!(bytes));
                return invoke(g, rt, "repo.quota.set", input, output::generic).await;
            }
            invoke(g, rt, "repo.quota.get", repo_input(repo)?, output::generic).await
        }

        // ── issues ──────────────────────────────────────────────────
        Command::IssueList { repo, filters } => {
            invoke(
                g,
                rt,
                "issue.list",
                list_input(repo, filters)?,
                output::issue_list,
            )
            .await
        }
        Command::IssueView { repo, number } => {
            invoke(
                g,
                rt,
                "issue.get",
                ref_input(repo, *number)?,
                output::issue_view,
            )
            .await
        }
        Command::IssueCreate { repo, title, body } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("title".into(), json!(title));
            opt(m, "body", body_text(body, "issue body")?);
            invoke(g, rt, "issue.create", input, output::issue_view).await
        }
        Command::IssueEdit {
            repo,
            number,
            title,
            body,
            labels,
            assignees,
        } => {
            let mut last: Option<Out> = None;
            if title.is_some() || body.body.is_some() || body.body_file.is_some() {
                let mut input = ref_input(repo, *number)?;
                let m = input.as_object_mut().unwrap();
                opt(m, "title", title.clone());
                opt(m, "body", body_text(body, "issue body")?);
                last = Some(invoke(g, rt, "issue.update", input, output::issue_view).await?);
            }
            if !labels.is_empty() {
                let ids = resolve_label_ids(g, rt, repo, labels).await?;
                let mut input = ref_input(repo, *number)?;
                input.as_object_mut().unwrap().insert(
                    "labelIds".into(),
                    Value::Array(ids.into_iter().map(Value::String).collect()),
                );
                last = Some(invoke(g, rt, "issue.labels.set", input, output::generic).await?);
            }
            if !assignees.is_empty() {
                let ids = resolve_assignee_ids(g, rt, repo, assignees).await?;
                let mut input = ref_input(repo, *number)?;
                input.as_object_mut().unwrap().insert(
                    "userIds".into(),
                    Value::Array(ids.into_iter().map(Value::String).collect()),
                );
                last = Some(invoke(g, rt, "issue.assignees.set", input, output::generic).await?);
            }
            match last {
                Some(out) => Ok(out),
                // Bare `issue edit` — show the issue (gh does the same).
                None => {
                    invoke(
                        g,
                        rt,
                        "issue.get",
                        ref_input(repo, *number)?,
                        output::issue_view,
                    )
                    .await
                }
            }
        }
        Command::IssueClose { repo, number } => {
            invoke(
                g,
                rt,
                "issue.close",
                ref_input(repo, *number)?,
                output::issue_view,
            )
            .await
        }
        Command::IssueReopen { repo, number } => {
            invoke(
                g,
                rt,
                "issue.reopen",
                ref_input(repo, *number)?,
                output::issue_view,
            )
            .await
        }
        Command::IssueDelete { repo, number } => {
            invoke(
                g,
                rt,
                "issue.delete",
                ref_input(repo, *number)?,
                output::generic,
            )
            .await
        }
        Command::IssueComment { repo, number, body } => {
            let mut input = ref_input(repo, *number)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("body".into(), json!(require_body(body, "comment")?));
            invoke(g, rt, "issue.comments.create", input, output::generic).await
        }

        // ── pull requests ───────────────────────────────────────────
        Command::PrList { repo, filters } => {
            invoke(
                g,
                rt,
                "pull.list",
                list_input(repo, filters)?,
                output::pull_list,
            )
            .await
        }
        Command::PrView { repo, number } => {
            invoke(
                g,
                rt,
                "pull.get",
                ref_input(repo, *number)?,
                output::pull_view,
            )
            .await
        }
        Command::PrChecks { repo, number } => pr_checks(g, rt, repo, *number).await,
        Command::PrCreate {
            repo,
            title,
            body,
            base,
            head,
            head_owner,
            head_name,
            draft,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("title".into(), json!(title));
            m.insert("base_ref".into(), json!(base));
            m.insert("head_ref".into(), json!(head));
            opt(m, "body", body_text(body, "pr body")?);
            opt(m, "head_owner", head_owner.clone());
            opt(m, "head_name", head_name.clone());
            if *draft {
                m.insert("draft".into(), json!(true));
            }
            invoke(g, rt, "pull.create", input, output::pull_view).await
        }
        Command::PrMerge {
            repo,
            number,
            method,
            commit_title,
            commit_message,
            delete_branch,
        } => {
            let method = match method {
                Some(m) => m.clone(),
                None => default_merge_method(g, rt, repo).await?,
            };
            let mut input = ref_input(repo, *number)?;
            let m = input.as_object_mut().unwrap();
            m.insert("method".into(), json!(method));
            opt(m, "commit_title", commit_title.clone());
            opt(m, "commit_message", commit_message.clone());
            if *delete_branch {
                m.insert("delete_branch".into(), json!(true));
            }
            invoke(g, rt, "pull.merge", input, output::generic).await
        }
        Command::PrDiff { repo, number } => {
            invoke(g, rt, "pull.files", ref_input(repo, *number)?, output::diff).await
        }
        Command::PrFiles { repo, number } => {
            invoke(
                g,
                rt,
                "pull.files",
                ref_input(repo, *number)?,
                output::file_list,
            )
            .await
        }
        Command::PrReview {
            repo,
            number,
            action,
            body,
            dismiss_id,
            reason,
            request,
            unrequest,
            list,
        } => {
            if *list {
                return invoke(
                    g,
                    rt,
                    "pull.reviews.list",
                    ref_input(repo, *number)?,
                    output::review_list,
                )
                .await;
            }
            if let Some(id) = dismiss_id {
                let mut input = ref_input(repo, *number)?;
                let m = input.as_object_mut().unwrap();
                m.insert("review_id".into(), json!(id));
                opt(m, "reason", reason.clone());
                return invoke(g, rt, "pull.reviews.dismiss", input, output::generic).await;
            }
            if let Some(u) = request {
                let mut input = ref_input(repo, *number)?;
                input
                    .as_object_mut()
                    .unwrap()
                    .insert("username".into(), json!(u));
                return invoke(g, rt, "pull.reviewRequests.add", input, output::generic).await;
            }
            if let Some(u) = unrequest {
                let mut input = ref_input(repo, *number)?;
                input
                    .as_object_mut()
                    .unwrap()
                    .insert("username".into(), json!(u));
                return invoke(g, rt, "pull.reviewRequests.remove", input, output::generic).await;
            }
            let state = match action {
                ReviewAction::Submit(s) => *s,
                ReviewAction::Comment => "commented",
            };
            let mut input = ref_input(repo, *number)?;
            let m = input.as_object_mut().unwrap();
            m.insert("state".into(), json!(state));
            opt(m, "body", body_text(body, "review body")?);
            invoke(g, rt, "pull.reviews.submit", input, output::generic).await
        }
        Command::PrComment { repo, number, body } => {
            let mut input = ref_input(repo, *number)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("body".into(), json!(require_body(body, "comment")?));
            invoke(g, rt, "pull.comments.create", input, output::generic).await
        }
        Command::PrReady { repo, number } => {
            let mut input = ref_input(repo, *number)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("draft".into(), json!(false));
            invoke(g, rt, "pull.update", input, output::generic).await
        }
        Command::PrUpdateBranch { repo, number } => {
            // Report ahead/behind first, then update.
            let status = call(g, rt, "pull.branchStatus", ref_input(repo, *number)?).await?;
            if output::s(&status, "status") == Some("up_to_date") {
                return Ok(Out::Report {
                    text: "branch already up to date".into(),
                    json: json!({"ok": true, "data": status}),
                });
            }
            invoke(
                g,
                rt,
                "pull.updateBranch",
                ref_input(repo, *number)?,
                output::generic,
            )
            .await
        }
        Command::PrClose { repo, number } => {
            invoke(
                g,
                rt,
                "pull.close",
                ref_input(repo, *number)?,
                output::pull_view,
            )
            .await
        }
        Command::PrReopen { repo, number } => {
            invoke(
                g,
                rt,
                "pull.reopen",
                ref_input(repo, *number)?,
                output::pull_view,
            )
            .await
        }
        Command::PrCheckout { repo, number } => {
            let pull = call(g, rt, "pull.get", ref_input(repo, *number)?).await?;
            let head_ref = output::s(&pull, "head_ref").unwrap_or("head").to_string();
            let head_owner = output::s(&pull, "head_owner").unwrap_or_default();
            let head_name = output::s(&pull, "head_name").unwrap_or_default();
            let instance = resolve_instance(g, &rt.config)?;
            let remote = format!("{instance}/{head_owner}/{head_name}.git");
            let branch = if head_ref.is_empty() {
                format!("pr-{number}")
            } else {
                head_ref.to_string()
            };
            run_git(&["fetch", remote.as_str(), head_ref.as_str()])?;
            run_git(&["checkout", "-b", branch.as_str(), "FETCH_HEAD"])?;
            Ok(Out::Report {
                text: format!("checked out {remote} {head_ref} → {branch}"),
                json: json!({"ok": true, "data": {"branch": branch, "remote": remote}}),
            })
        }

        // ── actions ─────────────────────────────────────────────────
        Command::RunList {
            repo,
            page,
            per_page,
            status,
            event,
            branch,
            workflow,
            actor,
            query,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            opt(m, "page", *page);
            opt(m, "per_page", *per_page);
            opt(m, "status", status.clone());
            opt(m, "event", event.clone());
            opt(m, "branch", branch.clone());
            opt(m, "workflow", workflow.clone());
            opt(m, "actor", actor.clone());
            opt(m, "query", query.clone());
            invoke(g, rt, "repo.actions.listRuns", input, output::run_list).await
        }
        Command::RunView {
            repo,
            run_id,
            job,
            log,
        } => {
            let (owner, name) = repo_parts(repo)?;
            let base = json!({ "owner": owner, "name": name, "run_id": run_id });
            if !*log {
                return invoke(g, rt, "repo.actions.getRun", base, output::run_view).await;
            }
            // --log needs a job id; --job wins, else the run's first job.
            let job_id = match job {
                Some(j) => j.clone(),
                None => {
                    let data = call(g, rt, "repo.actions.getRun", base.clone()).await?;
                    data.get("jobs")
                        .and_then(Value::as_array)
                        .and_then(|j| j.first())
                        .and_then(|j| output::s(j, "id"))
                        .map(str::to_string)
                        .ok_or_else(|| {
                            CliError::Usage(format!("run {run_id} has no jobs to log"))
                        })?
                }
            };
            let mut input = base;
            input
                .as_object_mut()
                .unwrap()
                .insert("job_id".into(), json!(job_id));
            invoke(g, rt, "repo.actions.getJobLog", input, output::job_log).await
        }
        Command::RunRerun { repo, run_id, m } => {
            let mut input = repo_input(repo)?;
            let mm = input.as_object_mut().unwrap();
            mm.insert("run_id".into(), json!(run_id));
            opt(mm, "job_id", m.job_id.clone());
            if m.failed_only {
                mm.insert("failed_only".into(), json!(true));
            }
            invoke(g, rt, "repo.actions.rerunRun", input, output::generic).await
        }
        Command::RunCancel { repo, run_id, m } => {
            let mut input = repo_input(repo)?;
            let mm = input.as_object_mut().unwrap();
            mm.insert("run_id".into(), json!(run_id));
            opt(mm, "job_id", m.job_id.clone());
            invoke(g, rt, "repo.actions.cancelRun", input, output::generic).await
        }
        Command::RunWatch {
            repo,
            run_id,
            interval,
            timeout,
            exit_status,
        } => {
            let (owner, name) = repo_parts(repo)?;
            let input = json!({ "owner": owner, "name": name, "run_id": run_id });
            let every = std::time::Duration::from_secs(interval.unwrap_or(5).max(1));
            let deadline =
                timeout.map(|t| std::time::Instant::now() + std::time::Duration::from_secs(t));
            let mut last_status = String::new();
            loop {
                let data = call(g, rt, "repo.actions.getRun", input.clone()).await?;
                let status = data
                    .pointer("/run/status")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !status.is_empty() && status != last_status {
                    eprintln!("run {run_id}: {status}");
                    last_status = status.to_string();
                }
                if matches!(status, "success" | "failure" | "cancelled") {
                    if *exit_status && status != "success" {
                        return Err(CliError::Api(AppError::new(
                            "run.failed",
                            format!("run {run_id} ended {status}"),
                        )));
                    }
                    return invoke(g, rt, "repo.actions.getRun", input, output::run_view).await;
                }
                if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                    return Err(CliError::Usage(format!(
                        "timed out waiting for run {run_id} ({status})"
                    )));
                }
                tokio::time::sleep(every).await;
            }
        }
        Command::WorkflowList { repo, git_ref } => {
            let mut input = repo_input(repo)?;
            opt(input.as_object_mut().unwrap(), "git_ref", git_ref.clone());
            invoke(
                g,
                rt,
                "repo.actions.listWorkflows",
                input,
                output::workflow_list,
            )
            .await
        }
        Command::WorkflowView {
            repo,
            path,
            git_ref,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("path".into(), json!(path));
            // --ref defaults to the repo's default branch.
            let git_ref = match git_ref {
                Some(r) => r.clone(),
                None => {
                    let data = call(g, rt, "repo.get", repo_input(repo)?).await?;
                    output::s(&data, "default_branch")
                        .unwrap_or("main")
                        .to_string()
                }
            };
            m.insert("ref".into(), json!(git_ref));
            invoke(g, rt, "repo.blob", input, output::blob).await
        }
        Command::WorkflowRun {
            repo,
            workflow_id,
            git_ref,
        } => {
            let git_ref = match git_ref {
                Some(r) => r.clone(),
                None => {
                    let data = call(g, rt, "repo.get", repo_input(repo)?).await?;
                    output::s(&data, "default_branch")
                        .unwrap_or("main")
                        .to_string()
                }
            };
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("workflow_id".into(), json!(workflow_id));
            m.insert("git_ref".into(), json!(git_ref));
            invoke(
                g,
                rt,
                "repo.actions.dispatchWorkflow",
                input,
                output::generic,
            )
            .await
        }
        Command::SecretList { repo } => {
            invoke(
                g,
                rt,
                "repo.actions.secrets.list",
                repo_input(repo)?,
                output::secret_list,
            )
            .await
        }
        Command::SecretSet { repo, name, body } => {
            let value = require_body(body, "secret value")?;
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("secret_name".into(), json!(name));
            m.insert("value".into(), json!(value));
            invoke(g, rt, "repo.actions.secrets.put", input, output::generic).await
        }
        Command::SecretDelete { repo, name } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("secret_name".into(), json!(name));
            invoke(g, rt, "repo.actions.secrets.delete", input, output::generic).await
        }

        // ── releases & packages ─────────────────────────────────────
        Command::ReleaseList { repo } => {
            invoke(
                g,
                rt,
                "release.list",
                repo_input(repo)?,
                output::release_list,
            )
            .await
        }
        Command::ReleaseView { repo, tag } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("tag_name".into(), json!(tag));
            invoke(g, rt, "release.get", input, output::release_view).await
        }
        Command::ReleaseCreate {
            repo,
            tag,
            title,
            body,
            draft,
            prerelease,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("tag_name".into(), json!(tag));
            opt(m, "title", title.clone());
            opt(m, "body", body_text(body, "release notes")?);
            if *draft {
                m.insert("draft".into(), json!(true));
            }
            if *prerelease {
                m.insert("prerelease".into(), json!(true));
            }
            invoke(g, rt, "release.create", input, output::release_view).await
        }
        Command::ReleaseEdit {
            repo,
            tag,
            title,
            body,
            draft,
            prerelease,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("tag_name".into(), json!(tag));
            opt(m, "title", title.clone());
            opt(m, "body", body_text(body, "release notes")?);
            opt(m, "draft", *draft);
            opt(m, "prerelease", *prerelease);
            invoke(g, rt, "release.update", input, output::release_view).await
        }
        Command::ReleaseDelete { repo, tag } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("tag_name".into(), json!(tag));
            invoke(g, rt, "release.delete", input, output::generic).await
        }
        Command::ReleaseUpload { repo, tag, files } => {
            release_upload(g, rt, repo, tag, files).await
        }
        Command::ReleaseDownload {
            repo,
            tag,
            pattern,
            dir,
            clobber,
        } => release_download(g, rt, repo, tag, pattern.as_ref(), dir.as_ref(), *clobber).await,
        Command::ReleaseDeleteAsset {
            repo,
            tag,
            filename,
        } => {
            let rel = release_by_tag(g, rt, repo, tag).await?;
            let asset_id = rel
                .get("assets")
                .and_then(Value::as_array)
                .and_then(|a| {
                    a.iter()
                        .find(|x| output::s(x, "filename") == Some(filename.as_str()))
                })
                .and_then(|x| output::s(x, "id"))
                .map(str::to_string)
                .ok_or_else(|| {
                    CliError::Usage(format!("no asset named '{filename}' on release {tag}"))
                })?;
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("asset_id".into(), json!(asset_id));
            invoke(g, rt, "release.deleteAsset", input, output::generic).await
        }
        Command::PkgList { owner } => {
            let input = match owner {
                Some(owner) => json!({ "owner": owner }),
                None => json!({}),
            };
            invoke(g, rt, "packages.list", input, output::pkg_list).await
        }

        // ── misc / navigation ───────────────────────────────────────
        Command::Search { q, types, limit } => {
            let mut input = json!({ "q": q });
            let m = input.as_object_mut().unwrap();
            if !types.is_empty() {
                let mapped: Vec<String> = types
                    .iter()
                    .map(|t| match t.as_str() {
                        "repo" | "repos" => "repositories",
                        "issue" | "issues" => "issues",
                        "pr" | "pull" | "pulls" => "pulls",
                        "user" | "users" => "users",
                        "org" | "orgs" | "organizations" => "organizations",
                        "commit" | "commits" => "commits",
                        "code" => "code",
                        other => other,
                    })
                    .map(str::to_string)
                    .collect();
                m.insert(
                    "types".into(),
                    Value::Array(mapped.into_iter().map(Value::String).collect()),
                );
            }
            opt(m, "limit", *limit);
            invoke(g, rt, "search.global", input, output::search).await
        }
        Command::Browse { repo, print } => {
            let instance = resolve_instance(g, &rt.config)?;
            let url = match repo {
                Some(r) => format!("{instance}/{r}"),
                None => instance.clone(),
            };
            if *print {
                return Ok(Out::Report {
                    text: url.clone(),
                    json: json!({"ok": true, "data": {"url": url}}),
                });
            }
            open_url(&url)?;
            Ok(Out::Report {
                text: format!("opened {url}"),
                json: json!({"ok": true, "data": {"url": url}}),
            })
        }
        Command::StatusCreate {
            repo,
            sha,
            context,
            state,
            description,
            target_url,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("sha".into(), json!(sha));
            m.insert("context".into(), json!(context));
            m.insert("state".into(), json!(state));
            opt(m, "description", description.clone());
            opt(m, "target_url", target_url.clone());
            invoke(g, rt, "repo.commitStatus.create", input, output::generic).await
        }

        // ── orgs / users / notifications ────────────────────────────
        Command::OrgList => invoke(g, rt, "org.listMine", json!({}), output::org_list).await,
        Command::OrgView { slug } => {
            invoke(g, rt, "org.get", json!({ "slug": slug }), output::generic).await
        }
        Command::OrgCreate { slug, name } => {
            let mut input = json!({ "slug": slug });
            opt(input.as_object_mut().unwrap(), "display_name", name.clone());
            invoke(g, rt, "org.create", input, output::generic).await
        }
        Command::OrgMembers { slug } => {
            invoke(
                g,
                rt,
                "org.members.list",
                json!({ "slug": slug }),
                output::member_list,
            )
            .await
        }
        Command::OrgMemberAdd {
            slug,
            username,
            role,
        } => {
            invoke(
                g,
                rt,
                "org.members.add",
                json!({ "slug": slug, "username": username, "role": role }),
                output::generic,
            )
            .await
        }
        Command::OrgMemberRemove { slug, username } => {
            let user_id = resolve_member_id(g, rt, slug, username).await?;
            invoke(
                g,
                rt,
                "org.members.remove",
                json!({ "slug": slug, "user_id": user_id }),
                output::generic,
            )
            .await
        }
        Command::OrgMemberRole {
            slug,
            username,
            role,
        } => {
            let user_id = resolve_member_id(g, rt, slug, username).await?;
            invoke(
                g,
                rt,
                "org.members.updateRole",
                json!({ "slug": slug, "user_id": user_id, "role": role }),
                output::generic,
            )
            .await
        }
        Command::OrgInviteList { slug } => {
            invoke(
                g,
                rt,
                "org.invites.list",
                json!({ "slug": slug }),
                output::invite_list,
            )
            .await
        }
        Command::OrgInviteCreate { slug, emails, role } => {
            invoke(
                g,
                rt,
                "org.invites.create",
                json!({
                    "slug": slug,
                    "emails": emails,
                    "role": role,
                }),
                output::invite_create_result,
            )
            .await
        }
        Command::OrgInviteLink {
            slug,
            role,
            expires_at,
            max_uses,
        } => {
            let mut input = json!({ "slug": slug, "role": role });
            let m = input.as_object_mut().unwrap();
            opt(m, "expires_at", expires_at.clone());
            opt(m, "max_uses", *max_uses);
            invoke(
                g,
                rt,
                "org.invites.createLink",
                input,
                output::invite_link_result,
            )
            .await
        }
        Command::OrgInviteRevoke { slug, id } => {
            invoke(
                g,
                rt,
                "org.invites.revoke",
                json!({ "slug": slug, "invite_id": id }),
                output::generic,
            )
            .await
        }
        Command::NotificationList { all, limit, offset } => {
            let mut input = json!({});
            let m = input.as_object_mut().unwrap();
            m.insert("filter".into(), json!(if *all { "all" } else { "unread" }));
            opt(m, "limit", *limit);
            opt(m, "offset", *offset);
            invoke(g, rt, "notification.list", input, output::notification_list).await
        }
        Command::NotificationRead { ids, all } => {
            if *all || ids.is_empty() {
                return invoke(
                    g,
                    rt,
                    "notification.markAllRead",
                    json!({}),
                    output::generic,
                )
                .await;
            }
            invoke(
                g,
                rt,
                "notification.markRead",
                json!({ "ids": ids }),
                output::generic,
            )
            .await
        }
        Command::NotificationUnread => {
            invoke(
                g,
                rt,
                "notification.unreadCount",
                json!({}),
                output::unread_count,
            )
            .await
        }
        Command::UserView { username } => {
            invoke(
                g,
                rt,
                "user.getPublicProfile",
                json!({ "username": username }),
                output::user_view,
            )
            .await
        }
        Command::UserLookup { prefix } => {
            invoke(
                g,
                rt,
                "user.lookup",
                json!({ "prefix": prefix }),
                output::user_list,
            )
            .await
        }
        Command::UserFollow { username, follow } => {
            invoke(
                g,
                rt,
                if *follow {
                    "user.follow"
                } else {
                    "user.unfollow"
                },
                json!({ "username": username }),
                output::generic,
            )
            .await
        }
        Command::UserStarred => {
            invoke(g, rt, "user.listStarred", json!({}), output::starred_list).await
        }
        Command::UserWatched => {
            invoke(g, rt, "user.listWatched", json!({}), output::repo_list).await
        }

        // ── account: keys, emails, PATs ─────────────────────────────
        Command::SshKeyList => invoke(g, rt, "sshKey.list", json!({}), output::ssh_key_list).await,
        Command::SshKeyAdd {
            title,
            key_file,
            can_auth,
            can_sign,
        } => {
            let public_key = read_body_source(key_file)?;
            invoke(
                g,
                rt,
                "sshKey.add",
                json!({
                    "title": title,
                    "public_key": public_key.trim(),
                    "can_authenticate": can_auth,
                    "can_sign": can_sign,
                }),
                output::generic,
            )
            .await
        }
        Command::SshKeyDelete { id } => {
            invoke(g, rt, "sshKey.delete", json!({ "id": id }), output::generic).await
        }
        Command::GpgKeyList => invoke(g, rt, "gpgKey.list", json!({}), output::gpg_key_list).await,
        Command::GpgKeyAdd { title, key_file } => {
            let armored = read_body_source(key_file)?;
            invoke(
                g,
                rt,
                "gpgKey.add",
                json!({ "title": title, "armored_public_key": armored }),
                output::generic,
            )
            .await
        }
        Command::GpgKeyDelete { id } => {
            invoke(g, rt, "gpgKey.delete", json!({ "id": id }), output::generic).await
        }
        Command::EmailList => invoke(g, rt, "email.list", json!({}), output::email_list).await,
        Command::EmailAdd { email } => {
            invoke(
                g,
                rt,
                "email.add",
                json!({ "email": email }),
                output::generic,
            )
            .await
        }
        Command::EmailRemove { id } => {
            invoke(g, rt, "email.remove", json!({ "id": id }), output::generic).await
        }
        Command::EmailSetPrimary { id } => {
            invoke(
                g,
                rt,
                "email.setPrimary",
                json!({ "id": id }),
                output::generic,
            )
            .await
        }
        Command::EmailResend { id } => {
            invoke(
                g,
                rt,
                "email.resendVerify",
                json!({ "id": id }),
                output::generic,
            )
            .await
        }
        // ── labels / webhooks ───────────────────────────────────────
        Command::LabelList {
            repo,
            org,
            include_hidden,
        } => {
            let (proc, mut input) = label_scope_input(repo.as_deref(), org.as_deref())?;
            if *include_hidden {
                input
                    .as_object_mut()
                    .unwrap()
                    .insert("includeHidden".into(), json!(true));
            }
            invoke(g, rt, proc, input, output::label_list).await
        }
        Command::LabelCreate {
            repo,
            org,
            name,
            color,
            description,
        } => {
            let mut input = label_write_input(repo.as_deref(), org.as_deref())?;
            let m = input.as_object_mut().unwrap();
            m.insert("name".into(), json!(name));
            m.insert("color".into(), json!(color));
            opt(m, "description", description.clone());
            invoke(g, rt, "label.create", input, output::generic).await
        }
        Command::LabelEdit {
            repo,
            org,
            id,
            name,
            color,
            description,
            hidden,
        } => {
            let mut input = label_write_input(repo.as_deref(), org.as_deref())?;
            let m = input.as_object_mut().unwrap();
            m.insert("id".into(), json!(id));
            opt(m, "name", name.clone());
            opt(m, "color", color.clone());
            opt(m, "description", description.clone());
            opt(m, "hidden", *hidden);
            invoke(g, rt, "label.update", input, output::generic).await
        }
        Command::LabelDelete { repo, org, id } => {
            let mut input = label_write_input(repo.as_deref(), org.as_deref())?;
            input
                .as_object_mut()
                .unwrap()
                .insert("id".into(), json!(id));
            invoke(g, rt, "label.delete", input, output::generic).await
        }
        Command::WebhookList { repo } => {
            invoke(
                g,
                rt,
                "webhook.list",
                repo_input(repo)?,
                output::webhook_list,
            )
            .await
        }
        Command::WebhookCreate {
            repo,
            url,
            secret,
            events,
            active,
            description,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("url".into(), json!(url));
            m.insert("secret".into(), json!(secret));
            m.insert(
                "events".into(),
                Value::Array(events.iter().cloned().map(Value::String).collect()),
            );
            m.insert("active".into(), json!(active));
            opt(m, "description", description.clone());
            invoke(g, rt, "webhook.create", input, output::generic).await
        }
        Command::WebhookEdit {
            repo,
            id,
            url,
            secret,
            events,
            active,
            description,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("id".into(), json!(id));
            opt(m, "url", url.clone());
            opt(m, "secret", secret.clone());
            if let Some(e) = events {
                m.insert(
                    "events".into(),
                    Value::Array(e.iter().cloned().map(Value::String).collect()),
                );
            }
            opt(m, "active", *active);
            opt(m, "description", description.clone());
            invoke(g, rt, "webhook.update", input, output::generic).await
        }
        Command::WebhookDelete { repo, id } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("id".into(), json!(id));
            invoke(g, rt, "webhook.delete", input, output::generic).await
        }
        Command::WebhookPing { repo, id } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("id".into(), json!(id));
            invoke(g, rt, "webhook.ping", input, output::generic).await
        }
        Command::WebhookDeliveries {
            repo,
            hook_id,
            limit,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("webhook_id".into(), json!(hook_id));
            opt(m, "limit", *limit);
            invoke(
                g,
                rt,
                "webhook.deliveries.list",
                input,
                output::delivery_list,
            )
            .await
        }
        Command::WebhookDelivery {
            repo,
            hook_id,
            delivery_id,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("webhook_id".into(), json!(hook_id));
            m.insert("delivery_id".into(), json!(delivery_id));
            invoke(g, rt, "webhook.deliveries.get", input, output::generic).await
        }
        Command::WebhookRedeliver {
            repo,
            hook_id,
            delivery_id,
        } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("webhook_id".into(), json!(hook_id));
            m.insert("delivery_id".into(), json!(delivery_id));
            invoke(g, rt, "webhook.redeliver", input, output::generic).await
        }

        Command::SkillPrint => Ok(crate::skill::print_skill()),
        Command::SkillInstall {
            harnesses,
            project,
            global,
            upgrade,
            detected,
        } => crate::skill::install(g, rt, harnesses, *project, *global, *upgrade, *detected),
        Command::McpPrint { harness } => crate::mcp::print_cfg(g, rt, harness.as_deref()),
        Command::McpInstall {
            harnesses,
            project,
            global,
            detected,
            token,
            token_stdin,
            web,
            anonymous,
            upgrade,
        } => {
            crate::mcp::install(
                g,
                rt,
                harnesses,
                *detected,
                *project,
                *global,
                token.clone(),
                *token_stdin,
                *web,
                *anonymous,
                *upgrade,
            )
            .await
        }
        Command::McpStatus => crate::mcp::status(g, rt).await,
        Command::McpRemove {
            harnesses,
            project,
            global,
            detected,
        } => crate::mcp::remove(g, rt, harnesses, *detected, *project, *global),
        Command::SelfUpdate {
            no_skill_refresh,
            no_mcp_refresh,
        } => crate::self_update::run(g, rt, *no_skill_refresh, *no_mcp_refresh).await,
        Command::Api { procedure, input } => {
            invoke(g, rt, procedure, input.clone(), |data| {
                serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string())
            })
            .await
        }
    }
}

/// Non-RPC client (release assets ride multipart/byte-stream routes).
fn http_client(global: &Global, rt: &Runtime) -> Result<crate::http::HttpClient, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
    let token = config::resolve_token(config::env_token().as_deref(), &rt.config, &instance);
    Ok(crate::http::HttpClient::new(&instance, token))
}

/// `release.get` by tag — shared by the asset verbs to resolve release id
/// and enumerate assets.
async fn release_by_tag(
    global: &Global,
    rt: &Runtime,
    repo: &str,
    tag: &str,
) -> Result<Value, CliError> {
    let mut input = repo_input(repo)?;
    input
        .as_object_mut()
        .unwrap()
        .insert("tag_name".into(), json!(tag));
    call(global, rt, "release.get", input).await
}

async fn release_upload(
    global: &Global,
    rt: &Runtime,
    repo: &str,
    tag: &str,
    files: &[String],
) -> Result<Out, CliError> {
    let rel = release_by_tag(global, rt, repo, tag).await?;
    let release_id = output::s(&rel, "id")
        .ok_or_else(|| CliError::Transport("release.get returned no id".into()))?;
    let (owner, name) = repo_parts(repo)?;
    let http = http_client(global, rt)?;
    let mut lines = Vec::new();
    let mut uploaded = Vec::new();
    for file in files {
        let path = std::path::Path::new(file);
        let data = http
            .upload_release_asset(&owner, &name, release_id, path)
            .await?;
        let fname = data
            .get("asset")
            .and_then(|a| output::s(a, "filename"))
            .or_else(|| output::s(&data, "filename"))
            .unwrap_or(file.as_str())
            .to_string();
        lines.push(format!("uploaded {fname}"));
        uploaded.push(fname);
    }
    Ok(Out::Report {
        text: lines.join("\n"),
        json: json!({"ok": true, "data": {"uploaded": uploaded}}),
    })
}

/// Tiny `*` glob — prefix/suffix/contains matching for `--pattern`.
fn glob_match(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return name == pattern;
    }
    let mut rest = name;
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() {
            continue;
        }
        match rest.find(p) {
            Some(idx) if i == 0 && !pattern.starts_with('*') && idx == 0 => {
                rest = &rest[idx + p.len()..];
            }
            Some(idx) if i > 0 => {
                rest = &rest[idx + p.len()..];
            }
            _ => return false,
        }
    }
    pattern.ends_with('*') || parts.last().map(|p| p.is_empty()).unwrap_or(false) || rest.is_empty()
}

async fn release_download(
    global: &Global,
    rt: &Runtime,
    repo: &str,
    tag: &str,
    pattern: Option<&String>,
    dir: Option<&String>,
    clobber: bool,
) -> Result<Out, CliError> {
    let rel = release_by_tag(global, rt, repo, tag).await?;
    let assets = rel
        .get("assets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let selected: Vec<&Value> = assets
        .iter()
        .filter(|a| match pattern {
            Some(p) => output::s(a, "filename")
                .map(|f| glob_match(p, f))
                .unwrap_or(false),
            None => true,
        })
        .collect();
    if selected.is_empty() {
        return Err(CliError::Usage(format!(
            "release {tag} has no matching assets"
        )));
    }
    let out_dir = dir.cloned().unwrap_or_else(|| ".".to_string());
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| CliError::Transport(format!("creating {out_dir}: {e}")))?;
    let http = http_client(global, rt)?;
    let mut lines = Vec::new();
    let mut saved = Vec::new();
    for asset in selected {
        let id = output::s(asset, "id").unwrap_or_default();
        let (filename, bytes) = http.download_release_asset(id).await?;
        let dest = std::path::Path::new(&out_dir).join(&filename);
        if dest.exists() && !clobber {
            return Err(CliError::Usage(format!(
                "{filename} exists — pass --clobber to overwrite"
            )));
        }
        std::fs::write(&dest, &bytes)
            .map_err(|e| CliError::Transport(format!("writing {}: {e}", dest.display())))?;
        lines.push(format!("saved {}", dest.display()));
        saved.push(filename);
    }
    Ok(Out::Report {
        text: lines.join("\n"),
        json: json!({"ok": true, "data": {"downloaded": saved}}),
    })
}

/// Classic scopes covering the full `ox` command surface — the default set
/// `--web` login and `auth refresh` mint. Deliberately broad (gh's CLI tokens
/// are similarly broad); `--scopes` narrows.
pub const CLI_DEFAULT_SCOPES: &str = concat!(
    "repo workflow notifications user write:org delete_repo ",
    "package:read package:write package:delete admin:repo_hook admin:public_key admin:gpg_key"
);

/// Normalize a `--scopes` value into a space-delimited, validated scope
/// string. Comma and whitespace both split; unknown names are rejected here
/// (the server validates again, but failing before the browser opens is the
/// better error).
fn scope_list(raw: &str) -> Result<String, CliError> {
    let mut out = Vec::new();
    for part in raw.split([',', ' ', '\t']).filter(|s| !s.is_empty()) {
        match oxidean_core::ClassicPatScope::parse(part) {
            Ok(_) => out.push(part.to_string()),
            Err(_) => {
                let valid = oxidean_core::CLASSIC_SCOPES
                    .iter()
                    .map(|m| m.scope.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(CliError::Usage(format!(
                    "unknown scope '{part}' — valid scopes: {valid}"
                )));
            }
        }
    }
    if out.is_empty() {
        return Err(CliError::Usage("empty --scopes list".into()));
    }
    Ok(out.join(" "))
}

/// `auth login` — verify reachability, store the token, mark the instance
/// default. `--token` → `OXIDEAN_TOKEN` → stdin (explicit `--token-stdin`, or
/// implicitly when stdin is a pipe — the CI path). Bare on a TTY: interactive
/// picker between browser sign-in and pasting a token (gh parity).
async fn auth_login(
    global: &Global,
    rt: &Runtime,
    token_flag: Option<&str>,
    token_stdin: bool,
) -> Result<Out, CliError> {
    let instance = resolve_instance(global, &rt.config)?;

    // Interactive picker — no auth flags and nothing piped in.
    if token_flag.is_none()
        && !token_stdin
        && config::env_token().is_none()
        && crate::term::can_prompt()
    {
        let choice = crate::prompt::select(
            "How do you want to sign in?",
            &[
                "Sign in via browser (device flow)".to_string(),
                "Paste a personal access token".to_string(),
            ],
        )
        .map_err(|e| CliError::Usage(format!("prompt failed: {e}")))?;
        if choice == 0 {
            return auth_login_web(global, rt, None).await;
        }
        let token = crate::prompt::password("Personal access token")
            .map_err(|e| CliError::Usage(format!("prompt failed: {e}")))?;
        return auth_login_with_token(global, rt, &instance, token.trim()).await;
    }

    let token = login_token(token_flag, token_stdin)?;
    auth_login_with_token(global, rt, &instance, &token).await
}

/// Shared tail of `auth login` once a token exists: probe, verify, persist.
async fn auth_login_with_token(
    _global: &Global,
    rt: &Runtime,
    instance: &str,
    token: &str,
) -> Result<Out, CliError> {
    let instance = instance.to_string();
    let token = token.to_string();

    // Reachability: system.health is unauthenticated and allowed pre-bootstrap.
    let probe = Client::new(&instance, None);
    match probe.call("system.health", json!({})).await {
        Ok(resp) if rpc::is_ok(&resp) => {}
        Ok(resp) => {
            let err = rpc::error(&resp)
                .cloned()
                .unwrap_or_else(|| AppError::new("rpc.unknown", "unexpected response"));
            return Err(CliError::Api(err));
        }
        Err(e) => return Err(CliError::Transport(format!("cannot reach {instance}: {e}"))),
    }

    // Best-effort verification: Bearer PATs on /api/rpc land with API-02; on a
    // cookie-only instance auth.me returns auth.unauthenticated and we still
    // store the token so the config is ready when the server catches up.
    let authed = Client::new(&instance, Some(token.clone()));
    let username = match authed.call("auth.me", json!({})).await {
        Ok(RpcResponse::Ok { data, .. }) => output::s(&data, "username").map(str::to_string),
        _ => None,
    };

    let entry = keystore::persist(&instance, &token);
    let mut cfg = rt.config.clone();
    cfg.instances.insert(instance.clone(), entry);
    cfg.default_instance = Some(instance.clone());
    config::save(&rt.config_path, &cfg).map_err(CliError::Config)?;

    let text = match &username {
        Some(user) => format!("Logged in to {instance} as {user}"),
        None => format!(
            "Logged in to {instance} — token stored, but auth.me could not verify it \
             (this instance may not accept Bearer tokens on /api/rpc yet)"
        ),
    };
    Ok(Out::Report {
        text,
        json: json!({
            "ok": true,
            "data": { "instance": instance, "authenticated_as": username }
        }),
    })
}

/// `auth login --web` — RFC 8628 device grant: request a code, show it, open
/// the approval page, poll until the user approves/denies, then store the
/// minted PAT exactly like a pasted token.
async fn auth_login_web(
    global: &Global,
    rt: &Runtime,
    scopes: Option<&str>,
) -> Result<Out, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
    let scopes = match scopes {
        Some(raw) => scope_list(raw)?,
        None => CLI_DEFAULT_SCOPES.to_string(),
    };

    // Same reachability probe as token login.
    let probe = Client::new(&instance, None);
    match probe.call("system.health", json!({})).await {
        Ok(resp) if rpc::is_ok(&resp) => {}
        Ok(resp) => {
            let err = rpc::error(&resp)
                .cloned()
                .unwrap_or_else(|| AppError::new("rpc.unknown", "unexpected response"));
            return Err(CliError::Api(err));
        }
        Err(e) => return Err(CliError::Transport(format!("cannot reach {instance}: {e}"))),
    }

    let token = device::run_grant(&instance, &scopes).await?;

    // Verify + store, mirroring auth_login.
    let authed = Client::new(&instance, Some(token.clone()));
    let username = match authed.call("auth.me", json!({})).await {
        Ok(RpcResponse::Ok { data, .. }) => output::s(&data, "username").map(str::to_string),
        _ => None,
    };
    let entry = keystore::persist(&instance, &token);
    let mut cfg = rt.config.clone();
    cfg.instances.insert(instance.clone(), entry);
    cfg.default_instance = Some(instance.clone());
    config::save(&rt.config_path, &cfg).map_err(CliError::Config)?;

    Ok(Out::Report {
        text: match &username {
            Some(user) => format!("Logged in to {instance} as {user}"),
            None => format!("Logged in to {instance}"),
        },
        json: json!({
            "ok": true,
            "data": { "instance": instance, "authenticated_as": username }
        }),
    })
}

async fn auth_status(global: &Global, rt: &Runtime) -> Result<Out, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
    let me = call(global, rt, "auth.me", json!({})).await?;

    // Best-effort PAT introspection — session-authed calls and older servers
    // simply skip the token block.
    let pat_info = match client(global, rt)?.call("pat.self", json!({})).await {
        Ok(RpcResponse::Ok { data, .. }) => Some(data),
        _ => None,
    };

    let store_label = rt
        .config
        .instances
        .get(&instance)
        .map(|e| keystore::store_used(&instance, e).to_string())
        .unwrap_or_else(|| "none".to_string());
    let env_token = config::env_token().is_some();

    let text = {
        let mut out = output::auth_status(&me, &instance);
        if env_token {
            out.push_str("\n  token: OXIDEAN_TOKEN (env)");
        } else if let Some(pat) = &pat_info {
            let name = output::s(pat, "name").unwrap_or("token");
            let prefix = output::s(pat, "token_prefix").unwrap_or("");
            let kind = output::s(pat, "kind").unwrap_or("");
            let expiry = output::s(pat, "expires_at").unwrap_or("never");
            out.push_str(&format!(
                "\n  token: {name} ({prefix}) {kind} · expires {expiry} · stored: {store_label}"
            ));
            if let Some(scopes) = pat.get("scopes").and_then(Value::as_array) {
                if !scopes.is_empty() {
                    let names: Vec<&str> = scopes.iter().filter_map(Value::as_str).collect();
                    out.push_str(&format!("\n  scopes: {}", names.join(" ")));
                }
            }
        } else {
            out.push_str(&format!("\n  token: stored: {store_label}"));
        }
        out
    };

    Ok(Out::Report {
        text,
        json: json!({
            "ok": true,
            "data": {
                "instance": instance,
                "user": me,
                "token": pat_info.unwrap_or(Value::Null),
                "token_source": if env_token { "env" } else { "stored" },
                "secret_store": store_label,
            }
        }),
    })
}

/// `auth switch` — change the default instance; arg or TTY picker over the
/// logged-in set. Config-file edit only, no network call.
fn auth_switch(rt: &Runtime, instance: Option<&str>) -> Result<Out, CliError> {
    use std::io::IsTerminal;

    if rt.config.instances.is_empty() {
        return Err(CliError::Usage(
            "no logged-in instances — run `ox auth login`".into(),
        ));
    }
    let chosen = match instance {
        Some(raw) => {
            let url = config::normalize_instance(raw).map_err(CliError::Usage)?;
            if !rt.config.instances.contains_key(&url) {
                return Err(CliError::Usage(format!(
                    "not logged in to {url} — known: {}",
                    rt.config
                        .instances
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            url
        }
        None => {
            if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
                return Err(CliError::Usage(
                    "non-interactive: pass the instance URL — `ox auth switch <url>`".into(),
                ));
            }
            let keys: Vec<String> = rt.config.instances.keys().cloned().collect();
            eprintln!("Default instance?");
            for (i, k) in keys.iter().enumerate() {
                let mark = if rt.config.default_instance.as_ref() == Some(k) {
                    " (current)"
                } else {
                    ""
                };
                eprintln!("  {:2}. {}{}", i + 1, k, mark);
            }
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .map_err(|e| CliError::Usage(format!("read selection: {e}")))?;
            let n: usize = line
                .trim()
                .parse()
                .map_err(|_| CliError::Usage(format!("invalid selection {:?}", line.trim())))?;
            keys.get(n.wrapping_sub(1))
                .cloned()
                .ok_or_else(|| CliError::Usage(format!("selection {n} out of range")))?
        }
    };
    let mut cfg = rt.config.clone();
    cfg.default_instance = Some(chosen.clone());
    config::save(&rt.config_path, &cfg).map_err(CliError::Config)?;
    Ok(Out::Report {
        text: format!("default instance: {chosen}"),
        json: json!({"ok": true, "data": {"default_instance": chosen}}),
    })
}

/// `config list` — the local config file. Stored tokens are never printed;
/// they report as `set`/`unset`.
fn config_list(rt: &Runtime) -> Result<Out, CliError> {
    let mut text = format!("config file: {}\n", rt.config_path.display());
    text.push_str(&format!(
        "default_instance: {}\n",
        rt.config.default_instance.as_deref().unwrap_or("(unset)")
    ));
    for (url, e) in &rt.config.instances {
        text.push_str(&format!(
            "{url}  token={}\n",
            if e.token.is_some() { "set" } else { "unset" }
        ));
    }
    let instances: Vec<Value> = rt
        .config
        .instances
        .iter()
        .map(|(url, e)| {
            json!({ "url": url, "token": if e.token.is_some() { "set" } else { "unset" } })
        })
        .collect();
    Ok(Out::Report {
        text: text.trim_end().to_string(),
        json: json!({"ok": true, "data": {
            "config_path": rt.config_path.display().to_string(),
            "default_instance": rt.config.default_instance,
            "instances": instances,
        }}),
    })
}

/// Split `instances.<url>.<field>` — the URL is normalized so lookups and
/// writes agree on one spelling.
fn config_instance_key(key: &str) -> Result<(String, String), CliError> {
    let rest = key.strip_prefix("instances.").ok_or_else(|| {
        CliError::Usage(format!(
            "unknown config key '{key}' — valid keys: default_instance, instances.<url>.token"
        ))
    })?;
    let (url, field) = rest.rsplit_once('.').ok_or_else(|| {
        CliError::Usage(format!(
            "unknown config key '{key}' — valid keys: default_instance, instances.<url>.token"
        ))
    })?;
    let url = config::normalize_instance(url).map_err(CliError::Usage)?;
    Ok((url, field.to_string()))
}

/// `config get` — `default_instance` prints its value; instance tokens
/// report `set`/`unset` rather than the secret.
fn config_get(rt: &Runtime, key: &str) -> Result<Out, CliError> {
    if key == "default_instance" {
        let v = rt.config.default_instance.clone();
        return Ok(Out::Report {
            text: v.clone().unwrap_or_default(),
            json: json!({"ok": true, "data": {"default_instance": v}}),
        });
    }
    let (url, field) = config_instance_key(key)?;
    match field.as_str() {
        "token" => {
            let state = match rt.config.instances.get(&url).and_then(|e| e.token.as_ref()) {
                Some(_) => "set",
                None => "unset",
            };
            Ok(Out::Report {
                text: state.to_string(),
                json: json!({"ok": true, "data": {"url": url, "token": state}}),
            })
        }
        _ => Err(CliError::Usage(format!(
            "unknown config key '{key}' — for instances only .token is stored"
        ))),
    }
}

/// `config set` — `default_instance` must name a known (logged-in)
/// instance; `instances.<url>.token` upserts the entry.
fn config_set(rt: &Runtime, key: &str, value: &str) -> Result<Out, CliError> {
    if key == "default_instance" {
        let url = config::normalize_instance(value).map_err(CliError::Usage)?;
        if !rt.config.instances.contains_key(&url) {
            return Err(CliError::Usage(format!(
                "not logged in to {url} — known: {}",
                rt.config
                    .instances
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        let mut cfg = rt.config.clone();
        cfg.default_instance = Some(url.clone());
        config::save(&rt.config_path, &cfg).map_err(CliError::Config)?;
        return Ok(Out::Report {
            text: format!("default_instance = {url}"),
            json: json!({"ok": true, "data": {"default_instance": url}}),
        });
    }
    let (url, field) = config_instance_key(key)?;
    match field.as_str() {
        "token" => {
            if value.is_empty() {
                return Err(CliError::Usage(
                    "empty token — use `ox auth logout` to remove a credential".into(),
                ));
            }
            let mut cfg = rt.config.clone();
            cfg.instances.entry(url.clone()).or_default().token = Some(value.to_string());
            config::save(&rt.config_path, &cfg).map_err(CliError::Config)?;
            Ok(Out::Report {
                text: format!("{url}: token set"),
                json: json!({"ok": true, "data": {"url": url, "token": "set"}}),
            })
        }
        _ => Err(CliError::Usage(format!(
            "unknown config key '{key}' — for instances only .token is stored"
        ))),
    }
}

/// `auth logout` — drop the stored token for the resolved instance (keyring
/// copy first, then the config entry).
fn auth_logout(global: &Global, rt: &Runtime) -> Result<Out, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
    keystore::remove(&instance);
    let mut cfg = rt.config.clone();
    let had = cfg.instances.remove(&instance).is_some();
    if !had {
        return Err(CliError::Usage(format!(
            "no stored token for {instance} — nothing to remove"
        )));
    }
    if cfg.default_instance.as_deref() == Some(instance.as_str()) {
        cfg.default_instance = cfg.instances.keys().next().cloned();
    }
    config::save(&rt.config_path, &cfg).map_err(CliError::Config)?;
    Ok(Out::Report {
        text: format!("logged out of {instance}"),
        json: json!({"ok": true, "data": {"instance": instance}}),
    })
}

/// `auth token` — print the token `ox` would send (scripts: `$(ox auth token)`).
fn auth_token(global: &Global, rt: &Runtime) -> Result<Out, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
    let token = config::resolve_token(config::env_token().as_deref(), &rt.config, &instance)
        .ok_or_else(|| {
            CliError::Auth(format!(
                "no token for {instance} — run `ox auth login` or set OXIDEAN_TOKEN"
            ))
        })?;
    Ok(Out::Report {
        text: token.clone(),
        json: json!({"ok": true, "data": {"token": token}}),
    })
}

/// Token for `auth login`: `--token` → `OXIDEAN_TOKEN` → stdin (explicit
/// `--token-stdin`, or implicitly when stdin is a pipe — the CI path).
fn login_token(flag: Option<&str>, token_stdin: bool) -> Result<String, CliError> {
    if let Some(t) = flag
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .or_else(config::env_token)
    {
        return Ok(t);
    }
    use std::io::IsTerminal;
    if !token_stdin && std::io::stdin().is_terminal() {
        return Err(CliError::Usage(
            "no token — pass --token, --web, set OXIDEAN_TOKEN, or pipe a token on stdin"
                .to_string(),
        ));
    }
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::Transport(format!("reading token from stdin: {e}")))?;
    let token = buf.trim();
    if token.is_empty() {
        return Err(CliError::Usage("empty token on stdin".to_string()));
    }
    Ok(token.to_string())
}

/// org member username → user_id (members.remove/updateRole take ids).
async fn resolve_member_id(
    global: &Global,
    rt: &Runtime,
    slug: &str,
    username: &str,
) -> Result<String, CliError> {
    let data = call(global, rt, "org.members.list", json!({ "slug": slug })).await?;
    data.get("members")
        .and_then(Value::as_array)
        .and_then(|ms| {
            ms.iter().find(|m| {
                output::s(m, "username")
                    .map(|u| u.eq_ignore_ascii_case(username))
                    .unwrap_or(false)
            })
        })
        .and_then(|m| output::s(m, "user_id"))
        .map(str::to_string)
        .ok_or_else(|| CliError::Usage(format!("'{username}' is not a member of {slug}")))
}

/// label scope plumbing — `--org` vs repo arg → `{scope, owner, repo?}` or
/// `{org}` inputs depending on the procedure family.
fn label_scope_input(
    repo: Option<&str>,
    org: Option<&str>,
) -> Result<(&'static str, Value), CliError> {
    match (repo, org) {
        (Some(r), None) => {
            let (owner, name) = repo_parts(r)?;
            Ok(("label.listForRepo", json!({ "owner": owner, "name": name })))
        }
        (None, Some(o)) => Ok(("label.listForOrg", json!({ "owner": o }))),
        (None, None) => Err(CliError::Usage(
            "label commands need <owner/name> or --org <slug>".into(),
        )),
        (Some(_), Some(_)) => Err(CliError::Usage(
            "label: pass either <owner/name> or --org, not both".into(),
        )),
    }
}

/// Stored credential only — `auth refresh`/`revoke` manage the login's own
/// token, never an `OXIDEAN_TOKEN` env override (env credentials are owned by
/// the caller's shell, refreshing them would leak a swapped secret into the
/// config).
fn stored_token(rt: &Runtime, instance: &str) -> Result<String, CliError> {
    if config::env_token().is_some() {
        return Err(CliError::Usage(
            "OXIDEAN_TOKEN is set — env-supplied tokens can't be refreshed or \
             revoked here; manage that token at {instance}/settings/tokens"
                .replace("{instance}", instance),
        ));
    }
    rt.config
        .instances
        .get(instance)
        .and_then(|e| crate::keystore::resolve(instance, e))
        .ok_or_else(|| {
            CliError::Auth(format!(
                "no stored token for {instance} — run `ox auth login`"
            ))
        })
}

/// `auth refresh` — mint a fresh CLI token through the device grant and
/// self-revoke the old one. Scopes: `--scopes` wins, `--reset-scopes` returns
/// to `CLI_DEFAULT_SCOPES`, otherwise the old token's own scopes carry over
/// (read via `pat.self` before the swap).
async fn auth_refresh(
    global: &Global,
    rt: &Runtime,
    scopes: Option<&str>,
    reset_scopes: bool,
) -> Result<Out, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
    let old_token = stored_token(rt, &instance)?;

    // What the old token carries — id for self-revoke, scopes for the default.
    let old_client = Client::new(&instance, Some(old_token.clone()));
    let old_info = match old_client.call("pat.self", json!({})).await {
        Ok(RpcResponse::Ok { data, .. }) => Some(data),
        Ok(RpcResponse::Err { error, .. }) => {
            return Err(CliError::Api(error));
        }
        Err(e) => return Err(CliError::from(e)),
    };

    let scopes = match (scopes, reset_scopes) {
        (Some(raw), _) => scope_list(raw)?,
        (None, true) => CLI_DEFAULT_SCOPES.to_string(),
        (None, false) => old_info
            .as_ref()
            .and_then(|i| i.get("scopes"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| CLI_DEFAULT_SCOPES.to_string()),
    };

    // Mint → verify → persist (atomic swap: the old credential is only
    // replaced once the new one is proven usable).
    let new_token = device::run_grant(&instance, &scopes).await?;
    let authed = Client::new(&instance, Some(new_token.clone()));
    let username = match authed.call("auth.me", json!({})).await {
        Ok(RpcResponse::Ok { data, .. }) => output::s(&data, "username").map(str::to_string),
        Ok(RpcResponse::Err { error, .. }) => return Err(CliError::Api(error)),
        Err(e) => return Err(CliError::from(e)),
    };
    let entry = keystore::persist(&instance, &new_token);
    let mut cfg = rt.config.clone();
    cfg.instances.insert(instance.clone(), entry);
    config::save(&rt.config_path, &cfg).map_err(CliError::Config)?;

    // Best-effort self-revoke of the old token (server rejects anything but
    // self-revocation, so this must run under the old credential).
    let revoked = if let Some(id) = old_info.as_ref().and_then(|i| output::s(i, "id")) {
        matches!(
            old_client.call("pat.revoke", json!({ "id": id })).await,
            Ok(RpcResponse::Ok { .. })
        )
    } else {
        false
    };

    let mut text = format!("Refreshed token on {instance}");
    if let Some(user) = &username {
        text.push_str(&format!(" as {user}"));
    }
    text.push_str(&format!(" — scopes: {scopes}"));
    if !revoked {
        text.push_str("\n(old token could not be revoked — it may already be expired/revoked)");
    }
    Ok(Out::Report {
        text,
        json: json!({
            "ok": true,
            "data": {
                "instance": instance,
                "authenticated_as": username,
                "scopes": scopes,
                "old_token_revoked": revoked,
            }
        }),
    })
}

/// `auth revoke` — self-revoke the stored token server-side, then drop the
/// local credential (keyring + config entry).
async fn auth_revoke(global: &Global, rt: &Runtime) -> Result<Out, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
    let token = stored_token(rt, &instance)?;
    let c = Client::new(&instance, Some(token));
    let info = match c.call("pat.self", json!({})).await {
        Ok(RpcResponse::Ok { data, .. }) => data,
        Ok(RpcResponse::Err { error, .. }) => return Err(CliError::Api(error)),
        Err(e) => return Err(CliError::from(e)),
    };
    let id = output::s(&info, "id")
        .ok_or_else(|| CliError::Transport("pat.self returned no id".to_string()))?;
    match c.call("pat.revoke", json!({ "id": id })).await {
        Ok(RpcResponse::Ok { .. }) => {}
        Ok(RpcResponse::Err { error, .. }) => return Err(CliError::Api(error)),
        Err(e) => return Err(CliError::from(e)),
    }
    keystore::remove(&instance);
    let mut cfg = rt.config.clone();
    cfg.instances.remove(&instance);
    if cfg.default_instance.as_deref() == Some(instance.as_str()) {
        cfg.default_instance = cfg.instances.keys().next().cloned();
    }
    config::save(&rt.config_path, &cfg).map_err(CliError::Config)?;
    Ok(Out::Report {
        text: format!("Revoked token {id} on {instance} and logged out"),
        json: json!({"ok": true, "data": {"instance": instance, "revoked": id}}),
    })
}

/// `auth setup-git` — register `ox` as the git credential helper for the
/// resolved instance's host (gh `auth setup-git` shape).
fn auth_setup_git(global: &Global, rt: &Runtime) -> Result<Out, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
    let host = instance
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let key = format!("credential.https://{host}.helper");
    // gh-style: clear any existing helpers for the host, then register ours.
    let _ = std::process::Command::new("git")
        .args(["config", "--global", "--unset-all", &key])
        .status();
    let status = std::process::Command::new("git")
        .args(["config", "--global", &key, "!ox auth git-credential"])
        .status()
        .map_err(|e| CliError::Transport(format!("cannot run git config: {e}")))?;
    if !status.success() {
        return Err(CliError::Transport("git config failed".into()));
    }
    Ok(Out::Report {
        text: format!("git credential helper registered for {host} (`ox auth git-credential`)"),
        json: json!({"ok": true, "data": {"host": host, "helper": "ox auth git-credential"}}),
    })
}

/// `auth git-credential` — git credential-helper protocol on stdio:
/// `get` answers with the stored token when the request targets this
/// instance's host; `store`/`erase` update the stored credential.
fn auth_git_credential(global: &Global, rt: &Runtime, op: &str) -> Result<Out, CliError> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::Transport(format!("reading credential request: {e}")))?;
    let mut kv = std::collections::BTreeMap::new();
    for line in buf.lines() {
        if let Some((k, v)) = line.split_once('=') {
            kv.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    let instance = resolve_instance(global, &rt.config)?;
    let req_host = kv.get("host").cloned().unwrap_or_default();
    let instance_host = instance
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string();

    let empty = Out::Report {
        text: String::new(),
        json: json!({"ok": true, "data": {}}),
    };

    match op {
        "get" => {
            if req_host.eq_ignore_ascii_case(&instance_host) {
                if let Some(t) =
                    config::resolve_token(config::env_token().as_deref(), &rt.config, &instance)
                {
                    print!("username=oxidean\npassword={t}\n\n");
                }
            }
            Ok(empty)
        }
        "store" => {
            if req_host.eq_ignore_ascii_case(&instance_host) {
                if let Some(password) = kv.get("password") {
                    let entry = keystore::persist(&instance, password);
                    let mut cfg = rt.config.clone();
                    cfg.instances.insert(instance.clone(), entry);
                    config::save(&rt.config_path, &cfg).map_err(CliError::Config)?;
                }
            }
            Ok(empty)
        }
        "erase" => {
            if req_host.eq_ignore_ascii_case(&instance_host) {
                keystore::remove(&instance);
                let mut cfg = rt.config.clone();
                cfg.instances.remove(&instance);
                if cfg.default_instance.as_deref() == Some(instance.as_str()) {
                    cfg.default_instance = cfg.instances.keys().next().cloned();
                }
                config::save(&rt.config_path, &cfg).map_err(CliError::Config)?;
            }
            Ok(empty)
        }
        _ => Err(CliError::Usage(format!(
            "unknown git-credential operation '{op}' — get|store|erase"
        ))),
    }
}

/// label.create/update/delete take `{scope, owner, repo?, …}`.
fn label_write_input(repo: Option<&str>, org: Option<&str>) -> Result<Value, CliError> {
    match (repo, org) {
        (Some(r), None) => {
            let (owner, name) = repo_parts(r)?;
            Ok(json!({ "scope": "repo", "owner": owner, "repo": name }))
        }
        (None, Some(o)) => Ok(json!({ "scope": "org", "owner": o })),
        (None, None) => Err(CliError::Usage(
            "label commands need <owner/name> or --org <slug>".into(),
        )),
        (Some(_), Some(_)) => Err(CliError::Usage(
            "label: pass either <owner/name> or --org, not both".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_input_omits_unset_filters() {
        let input = list_input("o/n", &ListFilters::default()).unwrap();
        assert_eq!(input, json!({"owner": "o", "name": "n"}));
    }

    #[test]
    fn list_input_maps_filters() {
        let input = list_input(
            "o/n",
            &ListFilters {
                state: Some("all".into()),
                author: Some("me".into()),
                assignee: None,
                label: Some("bug".into()),
                q: Some("crash".into()),
                limit: Some(50),
            },
        )
        .unwrap();
        assert_eq!(
            input,
            json!({
                "owner": "o", "name": "n", "state": "all", "author": "me",
                "label": "bug", "q": "crash", "limit": 50
            })
        );
    }

    #[test]
    fn ref_input_shape() {
        assert_eq!(
            ref_input("o/n", 4).unwrap(),
            json!({"owner": "o", "name": "n", "number": 4})
        );
    }

    #[test]
    fn cli_error_exit_codes() {
        assert_eq!(CliError::Usage("x".into()).exit_code(), 2);
        assert_eq!(CliError::Unsupported("x".into()).exit_code(), 2);
        assert_eq!(CliError::Auth("x".into()).exit_code(), 3);
        assert_eq!(CliError::Transport("x".into()).exit_code(), 1);
        assert_eq!(
            CliError::Api(AppError::new("auth.unauthenticated", "x")).exit_code(),
            3
        );
        assert_eq!(
            CliError::Api(AppError::new("repo.not_found", "x")).exit_code(),
            1
        );
        assert_eq!(
            CliError::from(CallError::Http {
                status: 403,
                body: "no".into()
            })
            .exit_code(),
            3
        );
    }

    #[test]
    fn required_procedures_map_commands() {
        assert_eq!(
            required_procedures(&Command::PrChecks {
                repo: "o/n".into(),
                number: 1
            }),
            vec!["pull.get".to_string(), "repo.commitStatus.list".to_string()]
        );
        assert_eq!(
            required_procedures(&Command::RepoList { owner: None }),
            vec!["repo.listMine".to_string()]
        );
        assert_eq!(
            required_procedures(&Command::RepoList {
                owner: Some("org".into())
            }),
            vec!["repo.listByOwner".to_string()]
        );
        assert_eq!(
            required_procedures(&Command::Api {
                procedure: "repo.explore".into(),
                input: json!({})
            }),
            vec!["repo.explore".to_string()]
        );
        assert!(required_procedures(&Command::Help).is_empty());
        assert!(required_procedures(&Command::Version).is_empty());
        assert_eq!(
            required_procedures(&Command::PrMerge {
                repo: "o/n".into(),
                number: 1,
                method: None,
                commit_title: None,
                commit_message: None,
                delete_branch: false,
            }),
            vec![
                "repo.mergeSettings.get".to_string(),
                "pull.merge".to_string()
            ]
        );
        assert_eq!(
            required_procedures(&Command::IssueEdit {
                repo: "o/n".into(),
                number: 1,
                title: None,
                body: Default::default(),
                labels: vec!["bug".into()],
                assignees: vec![],
            }),
            vec![
                "label.listForRepo".to_string(),
                "issue.labels.set".to_string()
            ]
        );
    }
}
