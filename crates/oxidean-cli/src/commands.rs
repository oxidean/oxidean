//! Command → procedure orchestration: config resolution, client wiring,
//! dispatch for every `ox` verb, and the multi-call chains behind
//! `pr checks` / `pr merge` / `issue edit`.

use std::io::Read;
use std::path::PathBuf;

use oxidean_core::{AppError, RpcResponse};
use serde_json::{Value, json};

use crate::args::{
    BodyArg, Command, Global, Invocation, ListFilters, ReviewAction, parse_repo,
};
use crate::config::{self, Config, InstanceEntry};
use crate::device;
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

/// Valid `pat create --fine-grained --perms` families (mirrors `FgPerms`).
const FG_PERM_FAMILIES: &[&str] = &[
    "issues",
    "pull_requests",
    "actions",
    "commit_statuses",
    "secrets",
    "variables",
    "environments",
    "deployments",
    "webhooks",
    "administration",
    "metadata",
    "pages",
    "releases",
    "emails",
    "notifications",
    "profile",
    "ssh_keys",
    "gpg_keys",
    "starring",
    "watching",
    "followers",
];

/// Parse `--perms issues=write,pull_requests=read` into the RPC `perms` object.
fn parse_fg_perms(pairs: &[String]) -> Result<Value, CliError> {
    let mut out = serde_json::Map::new();
    for pair in pairs {
        let (key, level) = pair.split_once('=').ok_or_else(|| {
            CliError::Usage(format!("--perms entries need family=level, got `{pair}`"))
        })?;
        if !FG_PERM_FAMILIES.contains(&key) {
            return Err(CliError::Usage(format!(
                "unknown permission family `{key}` (one of: {})",
                FG_PERM_FAMILIES.join(", ")
            )));
        }
        if level != "read" && level != "write" {
            return Err(CliError::Usage(format!(
                "permission level for `{key}` must be read or write, got `{level}`"
            )));
        }
        out.insert(key.to_string(), json!(level));
    }
    Ok(Value::Object(out))
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
    body_text(b, what)?.ok_or_else(|| {
        CliError::Usage(format!("{what} needs --body, --body-file, or piped stdin"))
    })
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
    std::fs::read_to_string(path)
        .map_err(|e| CliError::Transport(format!("reading {path}: {e}")))
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
async fn default_merge_method(global: &Global, rt: &Runtime, repo: &str) -> Result<String, CliError> {
    let data = call(global, rt, "repo.mergeSettings.get", repo_input(repo)?).await?;
    let settings = data.get("settings").unwrap_or(&data);
    for (field, method) in [
        ("allow_merge_commit", "merge"),
        ("allow_squash_merge", "squash"),
        ("allow_rebase_merge", "rebase"),
    ] {
        if settings.get(field).and_then(Value::as_bool).unwrap_or(false) {
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
        .ok_or_else(|| {
            CliError::Usage(format!("'{username}' is not a collaborator on {repo}"))
        })
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
        Command::AuthLogin { .. } | Command::AuthToken | Command::AuthLogout => vec![],
        Command::AuthStatus => one("auth.me"),
        Command::RepoList { owner } => vec![if owner.is_some() {
            "repo.listByOwner"
        } else {
            "repo.listMine"
        }
        .into()],
        Command::RepoView { .. } => one("repo.get"),
        Command::RepoCreate { .. } => one("repo.create"),
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
        Command::RepoWatch { watch, .. } => {
            one(if *watch { "repo.watch" } else { "repo.unwatch" })
        }
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
        Command::InviteAccept { .. } => one("invites.accept"),
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
            labels, assignees, title, body, ..
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
        Command::OrgMemberRemove { .. } => vec![
            "org.members.list".into(),
            "org.members.remove".into(),
        ],
        Command::OrgMemberRole { .. } => vec![
            "org.members.list".into(),
            "org.members.updateRole".into(),
        ],
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
        Command::UserFollow { follow, .. } => {
            one(if *follow { "user.follow" } else { "user.unfollow" })
        }
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
        Command::PatList => one("pat.list"),
        Command::PatCreate { fine_grained, .. } => one(if *fine_grained {
            "pat.createFineGrained"
        } else {
            "pat.createClassic"
        }),
        Command::PatRevoke { .. } => one("pat.revoke"),
        Command::LabelList { org, .. } => {
            one(if org.is_some() {
                "label.listForOrg"
            } else {
                "label.listForRepo"
            })
        }
        Command::LabelCreate { org, .. } | Command::LabelEdit { org, .. } | Command::LabelDelete { org, .. } => {
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
        Command::AdminUsersList { .. } => one("admin.users.list"),
        Command::AdminUserBan { ban, .. } => one(if *ban {
            "admin.users.ban"
        } else {
            "admin.users.unban"
        }),
        Command::AdminUserDelete { .. } => one("admin.users.delete"),
        Command::AdminUserRole { .. } => one("admin.users.updateRole"),
        Command::AdminUserSessions { .. } => one("admin.users.listSessions"),
        Command::AdminUserRevokeSessions { .. } => one("admin.users.revokeSessions"),
        Command::AdminUserAccess { .. } => one("admin.users.getAccess"),
        Command::AdminUserActivity { .. } => one("admin.users.getActivity"),
        Command::AdminInviteList => one("admin.invites.list"),
        Command::AdminInviteCreate { .. } => one("admin.invites.create"),
        Command::AdminInviteLink { .. } => one("admin.invites.createLink"),
        Command::AdminInviteRevoke { .. } => one("admin.invites.revoke"),
        Command::AdminRunners => one("admin.actions.listRunners"),
        Command::AdminRunnerToken => one("admin.actions.createRegistrationToken"),
        Command::AdminReposGc => one("admin.repos.gc"),
        Command::AdminLfs { set, .. } => one(if *set {
            "admin.lfs.updateSettings"
        } else {
            "admin.lfs.getSettings"
        }),
        Command::AdminGit { set, .. } => one(if *set {
            "admin.git.updateSettings"
        } else {
            "admin.git.getSettings"
        }),
        Command::AdminMcp { set, .. } => one(if *set {
            "admin.mcp.updateSettings"
        } else {
            "admin.mcp.getSettings"
        }),
        Command::AdminTemplatesList => one("admin.templates.list"),
        Command::AdminTemplatesSetEnabled { .. } => one("admin.templates.setEnabled"),
        Command::AdminTemplatesDelete { .. } => one("admin.templates.delete"),
        Command::PkgList { .. } => one("packages.list"),
        Command::Api { procedure, .. } => vec![procedure.clone()],
        // self-update talks to /cli/* HTTP, not /api/rpc — no manifest gate.
        // skill commands are fully local.
        Command::SelfUpdate
        | Command::SkillPrint
        | Command::SkillInstall { .. }
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
        } => {
            if *web {
                auth_login_web(g, rt).await
            } else {
                auth_login(g, rt, token.as_deref(), *token_stdin).await
            }
        }
        Command::AuthStatus => auth_status(g, rt).await,
        Command::AuthLogout => auth_logout(g, rt),
        Command::AuthToken => auth_token(g, rt),

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
        } => {
            let mut input = json!({ "name": name });
            let m = input.as_object_mut().unwrap();
            opt(m, "owner", owner.clone());
            opt(m, "description", description.clone());
            opt(m, "visibility", visibility.clone());
            opt(m, "license_id", license.clone());
            opt(m, "gitignore_id", gitignore.clone());
            invoke(g, rt, "repo.create", input, output::repo_view).await
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
        Command::InviteAccept {
            token,
            email,
            username,
            password,
        } => {
            let mut input = json!({ "token": token });
            let m = input.as_object_mut().unwrap();
            opt(m, "email", email.clone());
            opt(m, "username", username.clone());
            opt(m, "password", password.clone());
            invoke(g, rt, "invites.accept", input, output::generic).await
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
        Command::BranchProtectionSet { repo, pattern, opts } => {
            let mut input = repo_input(repo)?;
            let m = input.as_object_mut().unwrap();
            m.insert("pattern".into(), json!(pattern));
            if let Some(o) = opts.as_object() {
                for (k, v) in o {
                    m.insert(k.clone(), v.clone());
                }
            }
            invoke(g, rt, "repo.branchProtection.create", input, output::generic).await
        }
        Command::BranchProtectionDelete { repo, id } => {
            let mut input = repo_input(repo)?;
            input
                .as_object_mut()
                .unwrap()
                .insert("id".into(), json!(id));
            invoke(g, rt, "repo.branchProtection.delete", input, output::generic).await
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
        Command::TagProtectionSet { repo, pattern, opts } => {
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
            invoke(g, rt, "repo.mirror.syncNow", repo_input(repo)?, output::generic).await
        }
        Command::MirrorDelete { repo } => {
            invoke(g, rt, "repo.mirror.delete", repo_input(repo)?, output::generic).await
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
            invoke(g, rt, "repo.mergeSettings.update", input, output::merge_settings).await
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
            invoke(g, rt, "issue.list", list_input(repo, filters)?, output::issue_list).await
        }
        Command::IssueView { repo, number } => {
            invoke(g, rt, "issue.get", ref_input(repo, *number)?, output::issue_view).await
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
                    invoke(g, rt, "issue.get", ref_input(repo, *number)?, output::issue_view).await
                }
            }
        }
        Command::IssueClose { repo, number } => {
            invoke(g, rt, "issue.close", ref_input(repo, *number)?, output::issue_view).await
        }
        Command::IssueReopen { repo, number } => {
            invoke(g, rt, "issue.reopen", ref_input(repo, *number)?, output::issue_view).await
        }
        Command::IssueDelete { repo, number } => {
            invoke(g, rt, "issue.delete", ref_input(repo, *number)?, output::generic).await
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
            invoke(g, rt, "pull.list", list_input(repo, filters)?, output::pull_list).await
        }
        Command::PrView { repo, number } => {
            invoke(g, rt, "pull.get", ref_input(repo, *number)?, output::pull_view).await
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
            invoke(g, rt, "pull.files", ref_input(repo, *number)?, output::file_list).await
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
            invoke(g, rt, "pull.updateBranch", ref_input(repo, *number)?, output::generic).await
        }
        Command::PrClose { repo, number } => {
            invoke(g, rt, "pull.close", ref_input(repo, *number)?, output::pull_view).await
        }
        Command::PrReopen { repo, number } => {
            invoke(g, rt, "pull.reopen", ref_input(repo, *number)?, output::pull_view).await
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
            invoke(g, rt, "repo.actions.dispatchWorkflow", input, output::generic).await
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
            invoke(g, rt, "release.list", repo_input(repo)?, output::release_list).await
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
            m.insert(
                "filter".into(),
                json!(if *all { "all" } else { "unread" }),
            );
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
                if *follow { "user.follow" } else { "user.unfollow" },
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
        Command::SshKeyList => {
            invoke(g, rt, "sshKey.list", json!({}), output::ssh_key_list).await
        }
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
        Command::GpgKeyList => {
            invoke(g, rt, "gpgKey.list", json!({}), output::gpg_key_list).await
        }
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
        Command::EmailList => {
            invoke(g, rt, "email.list", json!({}), output::email_list).await
        }
        Command::EmailAdd { email } => {
            invoke(g, rt, "email.add", json!({ "email": email }), output::generic).await
        }
        Command::EmailRemove { id } => {
            invoke(g, rt, "email.remove", json!({ "id": id }), output::generic).await
        }
        Command::EmailSetPrimary { id } => {
            invoke(g, rt, "email.setPrimary", json!({ "id": id }), output::generic).await
        }
        Command::EmailResend { id } => {
            invoke(g, rt, "email.resendVerify", json!({ "id": id }), output::generic).await
        }
        Command::PatList => invoke(g, rt, "pat.list", json!({}), output::pat_list).await,
        Command::PatCreate {
            name,
            scopes,
            fine_grained,
            contents,
            repo_access,
            repo_ids,
            perms,
            expires_at,
        } => {
            let input = if *fine_grained {
                let mut i = json!({
                    "name": name,
                    "contents": contents.clone().unwrap_or_else(|| "read".to_string()),
                });
                let m = i.as_object_mut().unwrap();
                opt(m, "repo_access", repo_access.clone());
                if !repo_ids.is_empty() {
                    m.insert(
                        "repository_ids".into(),
                        Value::Array(repo_ids.iter().cloned().map(Value::String).collect()),
                    );
                }
                if !perms.is_empty() {
                    m.insert("perms".into(), parse_fg_perms(perms)?);
                }
                opt(m, "expires_at", expires_at.clone());
                i
            } else {
                if !perms.is_empty() {
                    return Err(CliError::Usage(
                        "--perms only applies to --fine-grained tokens".into(),
                    ));
                }
                if scopes.is_empty() {
                    return Err(CliError::Usage(
                        "pat create needs --scopes repo[,package:read,...] or --fine-grained".into(),
                    ));
                }
                let mut i = json!({ "name": name, "scopes": scopes });
                opt(i.as_object_mut().unwrap(), "expires_at", expires_at.clone());
                i
            };
            let proc = if *fine_grained {
                "pat.createFineGrained"
            } else {
                "pat.createClassic"
            };
            invoke(g, rt, proc, input, output::pat_created).await
        }
        Command::PatRevoke { id } => {
            invoke(g, rt, "pat.revoke", json!({ "id": id }), output::generic).await
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

        // ── instance admin ──────────────────────────────────────────
        Command::AdminUsersList {
            query,
            limit,
            offset,
        } => {
            let mut input = json!({});
            let m = input.as_object_mut().unwrap();
            opt(m, "query", query.clone());
            opt(m, "limit", *limit);
            opt(m, "offset", *offset);
            invoke(g, rt, "admin.users.list", input, output::admin_users).await
        }
        Command::AdminUserBan { id, ban } => {
            invoke(
                g,
                rt,
                if *ban { "admin.users.ban" } else { "admin.users.unban" },
                json!({ "user_id": id }),
                output::generic,
            )
            .await
        }
        Command::AdminUserDelete {
            id,
            confirm,
            delete_orgs,
        } => {
            invoke(
                g,
                rt,
                "admin.users.delete",
                json!({
                    "user_id": id,
                    "confirmation": confirm,
                    "delete_orgs": delete_orgs,
                }),
                output::generic,
            )
            .await
        }
        Command::AdminUserRole { id, role } => {
            invoke(
                g,
                rt,
                "admin.users.updateRole",
                json!({ "user_id": id, "role": role }),
                output::generic,
            )
            .await
        }
        Command::AdminUserSessions { id } => {
            invoke(
                g,
                rt,
                "admin.users.listSessions",
                json!({ "user_id": id }),
                output::admin_sessions,
            )
            .await
        }
        Command::AdminUserRevokeSessions { id } => {
            invoke(
                g,
                rt,
                "admin.users.revokeSessions",
                json!({ "user_id": id }),
                output::generic,
            )
            .await
        }
        Command::AdminUserAccess { id } => {
            invoke(
                g,
                rt,
                "admin.users.getAccess",
                json!({ "user_id": id }),
                output::admin_access,
            )
            .await
        }
        Command::AdminUserActivity {
            id,
            source,
            event_type,
            limit,
        } => {
            let mut input = json!({ "user_id": id });
            let m = input.as_object_mut().unwrap();
            opt(m, "source", source.clone());
            opt(m, "event_type", event_type.clone());
            opt(m, "limit", *limit);
            invoke(g, rt, "admin.users.getActivity", input, output::admin_activity).await
        }
        Command::AdminInviteList => {
            invoke(
                g,
                rt,
                "admin.invites.list",
                json!({}),
                output::invite_list,
            )
            .await
        }
        Command::AdminInviteCreate { emails } => {
            invoke(
                g,
                rt,
                "admin.invites.create",
                json!({ "emails": emails }),
                output::invite_create_result,
            )
            .await
        }
        Command::AdminInviteLink {
            expires_at,
            max_uses,
        } => {
            let mut input = json!({});
            let m = input.as_object_mut().unwrap();
            opt(m, "expires_at", expires_at.clone());
            opt(m, "max_uses", *max_uses);
            invoke(
                g,
                rt,
                "admin.invites.createLink",
                input,
                output::invite_link_result,
            )
            .await
        }
        Command::AdminInviteRevoke { id } => {
            invoke(
                g,
                rt,
                "admin.invites.revoke",
                json!({ "invite_id": id }),
                output::generic,
            )
            .await
        }
        Command::AdminRunners => {
            invoke(
                g,
                rt,
                "admin.actions.listRunners",
                json!({}),
                output::runner_list,
            )
            .await
        }
        Command::AdminRunnerToken => {
            invoke(
                g,
                rt,
                "admin.actions.createRegistrationToken",
                json!({}),
                output::generic,
            )
            .await
        }
        Command::AdminReposGc => {
            invoke(g, rt, "admin.repos.gc", json!({}), output::generic).await
        }
        Command::AdminLfs {
            set,
            max_object_bytes,
            quota_repo_bytes,
            quota_user_bytes,
            clear,
        } => {
            if !*set {
                return invoke(g, rt, "admin.lfs.getSettings", json!({}), output::generic).await;
            }
            let mut input = json!({});
            let m = input.as_object_mut().unwrap();
            opt(m, "max_object_bytes", *max_object_bytes);
            opt(m, "quota_repo_bytes", *quota_repo_bytes);
            opt(m, "quota_user_bytes", *quota_user_bytes);
            if *clear {
                m.insert("clear_overrides".into(), json!(true));
            }
            invoke(g, rt, "admin.lfs.updateSettings", input, output::generic).await
        }
        Command::AdminGit {
            set,
            repo_quota_bytes,
            clear,
        } => {
            if !*set {
                return invoke(g, rt, "admin.git.getSettings", json!({}), output::generic).await;
            }
            let mut input = json!({});
            let m = input.as_object_mut().unwrap();
            opt(m, "repo_quota_bytes", *repo_quota_bytes);
            if *clear {
                m.insert("clear_overrides".into(), json!(true));
            }
            invoke(g, rt, "admin.git.updateSettings", input, output::generic).await
        }
        Command::AdminMcp { set, enable, clear } => {
            if !*set {
                return invoke(g, rt, "admin.mcp.getSettings", json!({}), output::generic).await;
            }
            let mut input = json!({});
            let m = input.as_object_mut().unwrap();
            opt(m, "enabled", *enable);
            if *clear {
                m.insert("clear_overrides".into(), json!(true));
            }
            invoke(g, rt, "admin.mcp.updateSettings", input, output::generic).await
        }
        Command::AdminTemplatesList => {
            invoke(g, rt, "admin.templates.list", json!({}), output::generic).await
        }
        Command::AdminTemplatesSetEnabled { id, enabled } => {
            invoke(
                g,
                rt,
                "admin.templates.setEnabled",
                json!({ "id": id, "enabled": enabled }),
                output::generic,
            )
            .await
        }
        Command::AdminTemplatesDelete { id } => {
            invoke(
                g,
                rt,
                "admin.templates.delete",
                json!({ "id": id }),
                output::generic,
            )
            .await
        }

        Command::SkillPrint => Ok(crate::skill::print_skill()),
        Command::SkillInstall {
            harnesses,
            project,
            global,
            upgrade,
            detected,
        } => crate::skill::install(g, rt, harnesses, *project, *global, *upgrade, *detected),
        Command::SelfUpdate => crate::self_update::run(g, rt).await,
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
    pattern.ends_with('*')
        || parts.last().map(|p| p.is_empty()).unwrap_or(false)
        || rest.is_empty()
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
        .filter(|a| {
            match pattern {
                Some(p) => output::s(a, "filename")
                    .map(|f| glob_match(p, f))
                    .unwrap_or(false),
                None => true,
            }
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

/// `auth login` — verify reachability, store the token, mark the instance
/// default. `--token` → `OXIDEAN_TOKEN` → stdin (explicit `--token-stdin`, or
/// implicitly when stdin is a pipe — the CI path).
async fn auth_login(
    global: &Global,
    rt: &Runtime,
    token_flag: Option<&str>,
    token_stdin: bool,
) -> Result<Out, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
    let token = login_token(token_flag, token_stdin)?;

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

    let mut cfg = rt.config.clone();
    cfg.instances
        .insert(instance.clone(), InstanceEntry { token: Some(token) });
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
async fn auth_login_web(global: &Global, rt: &Runtime) -> Result<Out, CliError> {
    use std::io::IsTerminal;

    let instance = resolve_instance(global, &rt.config)?;

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

    // Classic scopes covering the full CLI surface (repo domain + registries).
    let grant = device::request(&instance, "repo package:read package:write").await?;

    let url = grant
        .verification_uri_complete
        .clone()
        .unwrap_or(grant.verification_uri.clone());
    eprintln!("! First copy your one-time code: {}", grant.user_code);
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if tty {
        eprintln!("Press Enter to open {url} in your browser…");
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        device::open_browser(&url);
    } else {
        eprintln!("Open {url} in your browser to approve this sign-in.");
    }
    eprintln!("Waiting for approval…");

    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(grant.expires_in.max(60) as u64);
    let mut interval = grant.interval.max(1) as u64;
    let token = loop {
        if std::time::Instant::now() >= deadline {
            return Err(CliError::Auth("device code expired before approval".into()));
        }
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        match device::poll(&instance, &grant.device_code, interval as i64).await? {
            device::Poll::Pending => {}
            device::Poll::SlowDown(next) => interval = next.max(1) as u64,
            device::Poll::Failed(msg) => return Err(CliError::Auth(msg)),
            device::Poll::Token(t) => break t,
        }
    };

    // Verify + store, mirroring auth_login.
    let authed = Client::new(&instance, Some(token.clone()));
    let username = match authed.call("auth.me", json!({})).await {
        Ok(RpcResponse::Ok { data, .. }) => output::s(&data, "username").map(str::to_string),
        _ => None,
    };
    let mut cfg = rt.config.clone();
    cfg.instances
        .insert(instance.clone(), InstanceEntry { token: Some(token) });
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
    invoke(global, rt, "auth.me", json!({}), move |data| {
        output::auth_status(data, &instance)
    })
    .await
}

/// `auth logout` — drop the stored token for the resolved instance.
fn auth_logout(global: &Global, rt: &Runtime) -> Result<Out, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
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
fn label_scope_input(repo: Option<&str>, org: Option<&str>) -> Result<(&'static str, Value), CliError> {
    match (repo, org) {
        (Some(r), None) => {
            let (owner, name) = repo_parts(r)?;
            Ok((
                "label.listForRepo",
                json!({ "owner": owner, "name": name }),
            ))
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
    fn parse_fg_perms_validates_family_and_level() {
        let v = parse_fg_perms(&[
            "issues=write".to_string(),
            "pull_requests=read".to_string(),
        ])
        .unwrap();
        assert_eq!(v, json!({"issues": "write", "pull_requests": "read"}));
        assert!(parse_fg_perms(&["bogus=read".to_string()]).is_err());
        assert!(parse_fg_perms(&["issues=admin".to_string()]).is_err());
        assert!(parse_fg_perms(&["issues".to_string()]).is_err());
    }

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
