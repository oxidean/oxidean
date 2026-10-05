//! Command → procedure orchestration: config resolution, client wiring, and
//! the `pull.get` → `repo.commitStatus.list` chain behind `pr checks`.

use std::io::Read;
use std::path::PathBuf;

use oxidean_core::{AppError, RpcResponse};
use serde_json::{json, Value};

use crate::args::{parse_repo, Command, Global, Invocation, ListFilters};
use crate::config::{self, Config, InstanceEntry};
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
            "no token — pass --token, set OXIDEAN_TOKEN, or pipe a token on stdin".to_string(),
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

async fn auth_status(global: &Global, rt: &Runtime) -> Result<Out, CliError> {
    let instance = resolve_instance(global, &rt.config)?;
    invoke(global, rt, "auth.me", json!({}), move |data| {
        output::auth_status(data, &instance)
    })
    .await
}

/// `pull.get` for the head sha, then `repo.commitStatus.list`. When the pull
/// lookup fails its envelope is the one reported (and `--json` echoes it).
async fn pr_checks(
    global: &Global,
    rt: &Runtime,
    repo: &str,
    number: u64,
) -> Result<Out, CliError> {
    let (owner, name) = repo_parts(repo)?;
    let client = client(global, rt)?;
    let pull = client
        .call(
            "pull.get",
            json!({ "owner": owner, "name": name, "number": number }),
        )
        .await?;
    let RpcResponse::Ok {
        data: pull_data, ..
    } = pull
    else {
        return Ok(Out::Rpc {
            envelope: pull,
            pretty: String::new(),
        });
    };
    let sha = output::s(&pull_data, "head_sha").unwrap_or_default();
    if sha.is_empty() {
        return Err(CliError::Transport(
            "pull.get response had no head_sha".to_string(),
        ));
    }
    let resp = client
        .call(
            "repo.commitStatus.list",
            json!({ "owner": owner, "name": name, "sha": sha }),
        )
        .await?;
    let sha_short = output::truncate(sha, 7);
    let pretty = match &resp {
        RpcResponse::Ok { data, .. } => {
            format!(
                "checks for #{number} @ {sha_short}\n{}",
                output::checks(data)
            )
        }
        RpcResponse::Err { .. } => String::new(),
    };
    Ok(Out::Rpc {
        envelope: resp,
        pretty,
    })
}

/// Procedures a command will call — the manifest gates on exactly these.
/// `Help`/`Version` are local-only (empty list → no fetch, no instance needed).
fn required_procedures(command: &Command) -> Vec<String> {
    match command {
        Command::AuthLogin { .. } => vec!["system.health".into(), "auth.me".into()],
        Command::AuthStatus => vec!["auth.me".into()],
        Command::RepoList { owner } => vec![if owner.is_some() {
            "repo.listByOwner"
        } else {
            "repo.listMine"
        }
        .into()],
        Command::RepoView { .. } => vec!["repo.get".into()],
        Command::IssueList { .. } => vec!["issue.list".into()],
        Command::IssueView { .. } => vec!["issue.get".into()],
        Command::IssueCreate { .. } => vec!["issue.create".into()],
        Command::PrList { .. } => vec!["pull.list".into()],
        Command::PrView { .. } => vec!["pull.get".into()],
        Command::PrChecks { .. } => vec!["pull.get".into(), "repo.commitStatus.list".into()],
        Command::RunList { .. } => vec!["repo.actions.listRuns".into()],
        Command::RunView { .. } => vec!["repo.actions.getRun".into()],
        Command::PkgList { .. } => vec!["packages.list".into()],
        Command::Api { procedure, .. } => vec![procedure.clone()],
        // self-update talks to /cli/* HTTP, not /api/rpc — no manifest gate.
        Command::SelfUpdate | Command::Help | Command::Version => vec![],
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
        Command::Version => Ok(Out::Report {
            text: format!("ox {}", env!("CARGO_PKG_VERSION")),
            json: json!({"ok": true, "data": {"version": env!("CARGO_PKG_VERSION")}}),
        }),
        Command::AuthLogin { token, token_stdin } => {
            auth_login(g, rt, token.as_deref(), *token_stdin).await
        }
        Command::AuthStatus => auth_status(g, rt).await,
        Command::RepoList { owner } => {
            let (procedure, input) = match owner {
                Some(owner) => ("repo.listByOwner", json!({ "owner": owner })),
                None => ("repo.listMine", json!({})),
            };
            invoke(g, rt, procedure, input, output::repo_list).await
        }
        Command::RepoView { repo } => {
            let (owner, name) = repo_parts(repo)?;
            invoke(
                g,
                rt,
                "repo.get",
                json!({ "owner": owner, "name": name }),
                output::repo_view,
            )
            .await
        }
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
            let (owner, name) = repo_parts(repo)?;
            let mut input = json!({ "owner": owner, "name": name, "title": title });
            if let (Some(map), Some(body)) = (input.as_object_mut(), body) {
                map.insert("body".into(), json!(body));
            }
            invoke(g, rt, "issue.create", input, output::issue_view).await
        }
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
        Command::RunList {
            repo,
            page,
            per_page,
        } => {
            let (owner, name) = repo_parts(repo)?;
            let mut input = json!({ "owner": owner, "name": name });
            if let Some(map) = input.as_object_mut() {
                if let Some(p) = page {
                    map.insert("page".into(), json!(p));
                }
                if let Some(pp) = per_page {
                    map.insert("per_page".into(), json!(pp));
                }
            }
            invoke(g, rt, "repo.actions.listRuns", input, output::run_list).await
        }
        Command::RunView { repo, run_id } => {
            let (owner, name) = repo_parts(repo)?;
            invoke(
                g,
                rt,
                "repo.actions.getRun",
                json!({ "owner": owner, "name": name, "run_id": run_id }),
                output::run_view,
            )
            .await
        }
        Command::PkgList { owner } => {
            let input = match owner {
                Some(owner) => json!({ "owner": owner }),
                None => json!({}),
            };
            invoke(g, rt, "packages.list", input, output::pkg_list).await
        }
        Command::SelfUpdate => crate::self_update::run(g, rt).await,
        Command::Api { procedure, input } => {
            invoke(g, rt, procedure, input.clone(), |data| {
                serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string())
            })
            .await
        }
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
    }
}
