//! Hand-rolled argv parsing — the workspace's small binaries don't pull in
//! clap (see crates/oxidean-api/src/bin/protection_hook.rs and
//! crates/oxidean-runner), so `ox` keeps the same minimal dependency shape.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// `ox` command summary, printed for `ox`, `ox --help`, and usage errors.
pub const USAGE: &str = "\
ox — companion CLI for Oxidean forge instances

USAGE
  ox <command> [flags]

AUTH
  ox auth login --instance <url> [--token <pat> | --token-stdin]
  ox auth status

REPOSITORIES
  ox repo list [--owner <user|org>]
  ox repo view <owner/name>

ISSUES
  ox issue list <owner/name> [--state <s>] [--author <u>] [--assignee <u>]
                [--label <l>] [--search <q>] [--limit <n>]
  ox issue view <owner/name> <number>
  ox issue create <owner/name> --title <t> [--body <b>]

PULL REQUESTS
  ox pr list <owner/name> [--state <s>] [--author <u>] [--assignee <u>]
             [--label <l>] [--search <q>] [--limit <n>]
  ox pr view <owner/name> <number>
  ox pr checks <owner/name> <number>

ACTIONS
  ox run list <owner/name> [--limit <n>] [--page <n>]
  ox run view <owner/name> <run-id>

PACKAGES
  ox pkg list [--owner <user|org>]

MAINTENANCE
  ox self-update     download the latest ox binary served by the instance and
                     replace this executable in place

ESCAPE HATCH
  ox api <procedure> [-f field=value]... [-F field=<json>]...
     -f sends the value as a string; -F parses it as JSON.

GLOBAL FLAGS
  --instance <url>   instance base URL (else $OXIDEAN_INSTANCE, else login default)
  --json             print the raw {ok, data|error} RPC envelope
  -R, --repo <o/n>   repository in owner/name form (alternative to positional)
  -h, --help         show this help
  --version          show version

ENVIRONMENT
  OXIDEAN_INSTANCE    default instance URL
  OXIDEAN_TOKEN       token override for any command
  OXIDEAN_CONFIG_DIR  config directory override (default: ~/.config/ox)

EXIT CODES
  0 ok · 1 api error · 2 usage · 3 auth";

/// argv parse failure — process exit code 2.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct UsageError(pub String);

fn usage(msg: impl Into<String>) -> UsageError {
    UsageError(msg.into())
}

/// Flags shared by every command (extracted before subcommand parsing).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Global {
    /// `--instance` — highest-priority instance URL override.
    pub instance: Option<String>,
    /// `--json` — emit the raw RPC response envelope.
    pub json: bool,
    /// `-h` / `--help`.
    pub help: bool,
    /// `--version`.
    pub version: bool,
}

/// List filters that map 1:1 onto `issue.list` / `pull.list` request fields.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ListFilters {
    pub state: Option<String>,
    pub author: Option<String>,
    pub assignee: Option<String>,
    pub label: Option<String>,
    pub q: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    AuthLogin {
        token: Option<String>,
        token_stdin: bool,
    },
    AuthStatus,
    RepoList {
        owner: Option<String>,
    },
    RepoView {
        repo: String,
    },
    IssueList {
        repo: String,
        filters: ListFilters,
    },
    IssueView {
        repo: String,
        number: u64,
    },
    IssueCreate {
        repo: String,
        title: String,
        body: Option<String>,
    },
    PrList {
        repo: String,
        filters: ListFilters,
    },
    PrView {
        repo: String,
        number: u64,
    },
    PrChecks {
        repo: String,
        number: u64,
    },
    RunList {
        repo: String,
        page: Option<u64>,
        per_page: Option<u64>,
    },
    RunView {
        repo: String,
        run_id: String,
    },
    PkgList {
        owner: Option<String>,
    },
    SelfUpdate,
    Api {
        procedure: String,
        input: Value,
    },
    Help,
    Version,
}

#[derive(Debug)]
pub struct Invocation {
    pub global: Global,
    pub command: Command,
}

/// Scanned subcommand tail: positionals plus recognized flags.
#[derive(Debug, Default)]
struct Flags {
    positionals: Vec<String>,
    values: HashMap<String, String>,
    multi: Vec<(String, String)>,
    bools: HashSet<String>,
}

/// Scan a subcommand tail. `value_flags` take a following (or `=`-attached)
/// value, `bool_flags` are bare switches, `multi_flags` are repeatable
/// value flags collected in order (`-f` / `-F`). `--` ends flag parsing.
fn scan(
    args: &[String],
    value_flags: &[&str],
    bool_flags: &[&str],
    multi_flags: &[&str],
) -> Result<Flags, UsageError> {
    let mut out = Flags::default();
    let mut positional_only = false;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if positional_only {
            out.positionals.push(arg.to_string());
            i += 1;
            continue;
        }
        if arg == "--" {
            positional_only = true;
            i += 1;
            continue;
        }
        if let Some(eq) = arg.find('=') {
            let (name, inline) = (&arg[..eq], &arg[eq + 1..]);
            if value_flags.contains(&name) {
                out.values.insert(name.to_string(), inline.to_string());
                i += 1;
                continue;
            }
            if multi_flags.contains(&name) {
                out.multi.push((name.to_string(), inline.to_string()));
                i += 1;
                continue;
            }
            if name.starts_with('-') {
                return Err(usage(format!("unknown flag: {name}")));
            }
            // `=` in a bare positional — keep it as a positional.
        }
        if bool_flags.contains(&arg) {
            out.bools.insert(arg.to_string());
            i += 1;
            continue;
        }
        if value_flags.contains(&arg) {
            let value = args
                .get(i + 1)
                .ok_or_else(|| usage(format!("{arg} requires a value")))?;
            out.values.insert(arg.to_string(), value.clone());
            i += 2;
            continue;
        }
        if multi_flags.contains(&arg) {
            let value = args
                .get(i + 1)
                .ok_or_else(|| usage(format!("{arg} requires a value")))?;
            out.multi.push((arg.to_string(), value.clone()));
            i += 2;
            continue;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            return Err(usage(format!("unknown flag: {arg}")));
        }
        out.positionals.push(arg.to_string());
        i += 1;
    }
    Ok(out)
}

/// Validate `owner/name`, returning the split parts.
pub fn parse_repo(spec: &str) -> Result<(String, String), UsageError> {
    let bad = || usage(format!("invalid repo '{spec}' — expected <owner>/<name>"));
    let (owner, name) = spec.split_once('/').ok_or_else(bad)?;
    if owner.is_empty()
        || name.is_empty()
        || name.contains('/')
        || owner.trim() != owner
        || name.trim() != name
    {
        return Err(bad());
    }
    Ok((owner.to_string(), name.to_string()))
}

/// `<owner/name>` from the first positional or `-R`/`--repo`.
fn repo_spec(f: &Flags, usage_str: &str) -> Result<String, UsageError> {
    let raw = f
        .positionals
        .first()
        .or_else(|| f.values.get("-R"))
        .or_else(|| f.values.get("--repo"))
        .ok_or_else(|| usage(format!("usage: ox {usage_str} <owner/name>")))?;
    parse_repo(raw)?;
    Ok(raw.clone())
}

/// Repo spec plus one trailing positional (issue number, run id, …). When
/// `-R`/`--repo` supplies the repo, positionals[0] is the trailing arg.
fn repo_and_arg(
    f: &Flags,
    usage_str: &str,
    arg_name: &str,
) -> Result<(String, String), UsageError> {
    let want = || usage(format!("usage: ox {usage_str} <owner/name> <{arg_name}>"));
    if f.values.contains_key("-R") || f.values.contains_key("--repo") {
        let repo = repo_spec(f, usage_str)?;
        let arg = f.positionals.first().ok_or_else(want)?;
        Ok((repo, arg.clone()))
    } else {
        let repo = f.positionals.first().ok_or_else(want)?;
        parse_repo(repo)?;
        let arg = f.positionals.get(1).ok_or_else(want)?;
        Ok((repo.clone(), arg.clone()))
    }
}

fn repo_and_number(f: &Flags, usage_str: &str) -> Result<(String, u64), UsageError> {
    let (repo, raw) = repo_and_arg(f, usage_str, "number")?;
    let number = raw
        .parse::<u64>()
        .map_err(|_| usage(format!("invalid {usage_str} number: {raw}")))?;
    Ok((repo, number))
}

/// Reject stray positionals on commands that take none.
fn expect_no_positionals(f: &Flags, usage_str: &str) -> Result<(), UsageError> {
    if let Some(extra) = f.positionals.first() {
        return Err(usage(format!(
            "ox {usage_str}: unexpected argument '{extra}'"
        )));
    }
    Ok(())
}

fn list_filters(f: &Flags) -> Result<ListFilters, UsageError> {
    let limit = match f.values.get("--limit") {
        Some(v) => Some(
            v.parse::<u64>()
                .map_err(|_| usage(format!("invalid --limit: {v}")))?,
        ),
        None => None,
    };
    Ok(ListFilters {
        state: f.values.get("--state").cloned(),
        author: f.values.get("--author").cloned(),
        assignee: f.values.get("--assignee").cloned(),
        label: f.values.get("--label").cloned(),
        q: f.values.get("--search").cloned(),
        limit,
    })
}

fn opt_u64(f: &Flags, name: &str) -> Result<Option<u64>, UsageError> {
    match f.values.get(name) {
        Some(v) => Ok(Some(
            v.parse::<u64>()
                .map_err(|_| usage(format!("invalid {name}: {v}")))?,
        )),
        None => Ok(None),
    }
}

/// `-f` values are strings; `-F` values are parsed as JSON. Flat keys only.
fn api_input(f: &Flags) -> Result<Value, UsageError> {
    let mut input = serde_json::Map::new();
    for (flag, kv) in &f.multi {
        let (key, raw) = kv
            .split_once('=')
            .ok_or_else(|| usage(format!("{flag} expects field=value, got '{kv}'")))?;
        if key.is_empty() {
            return Err(usage(format!("{flag} expects field=value, got '{kv}'")));
        }
        let value = if flag == "-F" {
            serde_json::from_str(raw)
                .map_err(|e| usage(format!("{flag} {key}: invalid JSON value: {e}")))?
        } else {
            Value::String(raw.to_string())
        };
        input.insert(key.to_string(), value);
    }
    Ok(Value::Object(input))
}

const REPO_FLAGS: &[&str] = &["-R", "--repo"];
const LIST_FLAGS: &[&str] = &[
    "-R",
    "--repo",
    "--state",
    "--author",
    "--assignee",
    "--label",
    "--search",
    "--limit",
];

/// Split argv into globals + command. Global flags may appear anywhere;
/// everything else keeps its order and is parsed per-subcommand.
pub fn parse(argv: &[String]) -> Result<Invocation, UsageError> {
    let mut global = Global::default();
    let mut rest: Vec<String> = Vec::new();
    let mut positional_only = false;
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        if !positional_only {
            match arg {
                // Fall through to `rest` so the subcommand scanner sees the
                // end-of-flags marker too.
                "--" => positional_only = true,
                "--json" => {
                    global.json = true;
                    i += 1;
                    continue;
                }
                "-h" | "--help" => {
                    global.help = true;
                    i += 1;
                    continue;
                }
                "--version" => {
                    global.version = true;
                    i += 1;
                    continue;
                }
                "--instance" => {
                    let v = argv
                        .get(i + 1)
                        .ok_or_else(|| usage("--instance requires a value"))?;
                    global.instance = Some(v.clone());
                    i += 2;
                    continue;
                }
                _ => {
                    if let Some(v) = arg.strip_prefix("--instance=") {
                        if v.is_empty() {
                            return Err(usage("--instance requires a value"));
                        }
                        global.instance = Some(v.to_string());
                        i += 1;
                        continue;
                    }
                }
            }
        }
        rest.push(argv[i].clone());
        i += 1;
    }

    if global.help {
        return Ok(Invocation {
            global,
            command: Command::Help,
        });
    }
    if global.version {
        return Ok(Invocation {
            global,
            command: Command::Version,
        });
    }
    let Some(group) = rest.first().map(String::as_str) else {
        return Ok(Invocation {
            global,
            command: Command::Help,
        });
    };
    let sub = rest.get(1).map(String::as_str).unwrap_or("");
    let tail = &rest[rest.len().min(2)..];

    let command = match (group, sub) {
        ("auth", "login") => {
            let f = scan(tail, &["--token"], &["--token-stdin"], &[])?;
            expect_no_positionals(&f, "auth login")?;
            Command::AuthLogin {
                token: f.values.get("--token").cloned(),
                token_stdin: f.bools.contains("--token-stdin"),
            }
        }
        ("auth", "status") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "auth status")?;
            Command::AuthStatus
        }
        ("repo", "list") => {
            let f = scan(tail, &["--owner"], &[], &[])?;
            expect_no_positionals(&f, "repo list")?;
            Command::RepoList {
                owner: f.values.get("--owner").cloned(),
            }
        }
        ("repo", "view") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::RepoView {
                repo: repo_spec(&f, "repo view")?,
            }
        }
        ("issue", "list") => {
            let f = scan(tail, LIST_FLAGS, &[], &[])?;
            Command::IssueList {
                repo: repo_spec(&f, "issue list")?,
                filters: list_filters(&f)?,
            }
        }
        ("issue", "view") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "issue view")?;
            Command::IssueView { repo, number }
        }
        ("issue", "create") => {
            let f = scan(tail, &["-R", "--repo", "--title", "--body"], &[], &[])?;
            let repo = repo_spec(&f, "issue create")?;
            let title = f
                .values
                .get("--title")
                .cloned()
                .ok_or_else(|| usage("issue create requires --title"))?;
            Command::IssueCreate {
                repo,
                title,
                body: f.values.get("--body").cloned(),
            }
        }
        ("pr", "list") => {
            let f = scan(tail, LIST_FLAGS, &[], &[])?;
            Command::PrList {
                repo: repo_spec(&f, "pr list")?,
                filters: list_filters(&f)?,
            }
        }
        ("pr", "view") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr view")?;
            Command::PrView { repo, number }
        }
        ("pr", "checks") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr checks")?;
            Command::PrChecks { repo, number }
        }
        ("run", "list") => {
            let f = scan(tail, &["-R", "--repo", "--limit", "--page"], &[], &[])?;
            Command::RunList {
                repo: repo_spec(&f, "run list")?,
                page: opt_u64(&f, "--page")?,
                per_page: opt_u64(&f, "--limit")?,
            }
        }
        ("run", "view") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, run_id) = repo_and_arg(&f, "run view", "run-id")?;
            Command::RunView { repo, run_id }
        }
        ("pkg", "list") => {
            let f = scan(tail, &["--owner"], &[], &[])?;
            expect_no_positionals(&f, "pkg list")?;
            Command::PkgList {
                owner: f.values.get("--owner").cloned(),
            }
        }
        ("self-update", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "self-update")?;
            Command::SelfUpdate
        }
        ("api", proc) if !proc.is_empty() => {
            let f = scan(tail, &[], &[], &["-f", "-F"])?;
            expect_no_positionals(&f, "api")?;
            Command::Api {
                procedure: proc.to_string(),
                input: api_input(&f)?,
            }
        }
        ("api", _) => {
            return Err(usage("usage: ox api <procedure> [-f field=value]..."));
        }
        ("auth", "") | ("repo", "") | ("issue", "") | ("pr", "") | ("run", "") | ("pkg", "") => {
            return Err(usage(format!(
                "ox {group}: missing subcommand — see `ox --help`"
            )));
        }
        (g, s) => {
            let what = if s.is_empty() {
                g.to_string()
            } else {
                format!("{g} {s}")
            };
            return Err(usage(format!("unknown command: {what} — see `ox --help`")));
        }
    };
    Ok(Invocation { global, command })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn self_update_parses() {
        let inv = parse(&argv(&["self-update"])).unwrap();
        assert_eq!(inv.command, Command::SelfUpdate);
        let err = parse(&argv(&["self-update", "extra"])).unwrap_err();
        assert!(err.0.contains("unknown command"), "{err}");
    }

    #[test]
    fn empty_argv_is_help() {
        let inv = parse(&argv(&[])).unwrap();
        assert!(matches!(inv.command, Command::Help));
    }

    #[test]
    fn repo_view_positional() {
        let inv = parse(&argv(&["repo", "view", "octo/demo"])).unwrap();
        assert_eq!(
            inv.command,
            Command::RepoView {
                repo: "octo/demo".into()
            }
        );
    }

    #[test]
    fn repo_view_dash_r_flag() {
        let inv = parse(&argv(&["repo", "view", "-R", "octo/demo"])).unwrap();
        assert_eq!(
            inv.command,
            Command::RepoView {
                repo: "octo/demo".into()
            }
        );
    }

    #[test]
    fn repo_view_rejects_missing_slash() {
        let err = parse(&argv(&["repo", "view", "demo"])).unwrap_err();
        assert!(err.0.contains("invalid repo"), "{err}");
    }

    #[test]
    fn globals_extracted_anywhere() {
        let inv = parse(&argv(&[
            "issue",
            "list",
            "o/n",
            "--json",
            "--instance",
            "https://f.example.com",
        ]))
        .unwrap();
        assert!(inv.global.json);
        assert_eq!(
            inv.global.instance.as_deref(),
            Some("https://f.example.com")
        );
        match inv.command {
            Command::IssueList { repo, .. } => assert_eq!(repo, "o/n"),
            other => panic!("expected issue list, got {other:?}"),
        }
    }

    #[test]
    fn instance_equals_form() {
        let inv = parse(&argv(&[
            "--instance=https://f.example.com/",
            "repo",
            "list",
        ]))
        .unwrap();
        assert_eq!(
            inv.global.instance.as_deref(),
            Some("https://f.example.com/")
        );
    }

    #[test]
    fn issue_view_parses_number() {
        let inv = parse(&argv(&["issue", "view", "o/n", "12"])).unwrap();
        assert_eq!(
            inv.command,
            Command::IssueView {
                repo: "o/n".into(),
                number: 12
            }
        );
    }

    #[test]
    fn issue_view_rejects_non_number() {
        let err = parse(&argv(&["issue", "view", "o/n", "x"])).unwrap_err();
        assert!(err.0.contains("number"), "{err}");
    }

    #[test]
    fn issue_create_requires_title() {
        let err = parse(&argv(&["issue", "create", "o/n"])).unwrap_err();
        assert!(err.0.contains("--title"), "{err}");
    }

    #[test]
    fn issue_create_full() {
        let inv = parse(&argv(&[
            "issue", "create", "o/n", "--title", "t", "--body", "b",
        ]))
        .unwrap();
        assert_eq!(
            inv.command,
            Command::IssueCreate {
                repo: "o/n".into(),
                title: "t".into(),
                body: Some("b".into()),
            }
        );
    }

    #[test]
    fn list_filters_parse() {
        let inv = parse(&argv(&[
            "pr", "list", "o/n", "--state", "all", "--author", "me", "--limit", "10",
        ]))
        .unwrap();
        match inv.command {
            Command::PrList { filters, .. } => {
                assert_eq!(filters.state.as_deref(), Some("all"));
                assert_eq!(filters.author.as_deref(), Some("me"));
                assert_eq!(filters.limit, Some(10));
            }
            other => panic!("expected pr list, got {other:?}"),
        }
    }

    #[test]
    fn api_fields_string_and_typed() {
        let inv = parse(&argv(&[
            "api",
            "issue.get",
            "-f",
            "owner=o",
            "-f",
            "name=n",
            "-F",
            "number=4",
            "-F",
            "flag=true",
        ]))
        .unwrap();
        match inv.command {
            Command::Api { procedure, input } => {
                assert_eq!(procedure, "issue.get");
                assert_eq!(
                    input,
                    json!({"owner": "o", "name": "n", "number": 4, "flag": true})
                );
            }
            other => panic!("expected api, got {other:?}"),
        }
    }

    #[test]
    fn api_field_without_equals_is_usage_error() {
        let err = parse(&argv(&["api", "system.health", "-f", "novalue"])).unwrap_err();
        assert!(err.0.contains("field=value"), "{err}");
    }

    #[test]
    fn api_requires_procedure() {
        let err = parse(&argv(&["api"])).unwrap_err();
        assert!(err.0.contains("usage"), "{err}");
    }

    #[test]
    fn unknown_flag_is_usage_error() {
        let err = parse(&argv(&["repo", "list", "--bogus"])).unwrap_err();
        assert!(err.0.contains("unknown flag"), "{err}");
    }

    #[test]
    fn unknown_command_is_usage_error() {
        let err = parse(&argv(&["frobnicate"])).unwrap_err();
        assert!(err.0.contains("unknown command"), "{err}");
    }

    #[test]
    fn bare_group_is_usage_error() {
        let err = parse(&argv(&["issue"])).unwrap_err();
        assert!(err.0.contains("missing subcommand"), "{err}");
    }

    #[test]
    fn auth_login_variants() {
        let inv = parse(&argv(&["auth", "login", "--token", "oxidean_pat_x"])).unwrap();
        match inv.command {
            Command::AuthLogin { token, token_stdin } => {
                assert_eq!(token.as_deref(), Some("oxidean_pat_x"));
                assert!(!token_stdin);
            }
            other => panic!("expected auth login, got {other:?}"),
        }
        let inv = parse(&argv(&["auth", "login", "--token-stdin"])).unwrap();
        match inv.command {
            Command::AuthLogin { token, token_stdin } => {
                assert_eq!(token, None);
                assert!(token_stdin);
            }
            other => panic!("expected auth login, got {other:?}"),
        }
    }

    #[test]
    fn run_view_parses_run_id() {
        let inv = parse(&argv(&["run", "view", "o/n", "run-123"])).unwrap();
        assert_eq!(
            inv.command,
            Command::RunView {
                repo: "o/n".into(),
                run_id: "run-123".into()
            }
        );
    }

    #[test]
    fn positional_after_double_dash() {
        let inv = parse(&argv(&["repo", "view", "--", "-R"])).unwrap_err();
        // "-R" alone is not a valid owner/name spec.
        assert!(inv.0.contains("invalid repo"), "{inv}");
    }

    #[test]
    fn help_and_version() {
        assert!(matches!(
            parse(&argv(&["--help"])).unwrap().command,
            Command::Help
        ));
        assert!(matches!(
            parse(&argv(&["repo", "list", "--version"]))
                .unwrap()
                .command,
            Command::Version
        ));
    }

    #[test]
    fn parse_repo_validation() {
        assert!(parse_repo("o/n").is_ok());
        assert!(parse_repo("o/n/x").is_err());
        assert!(parse_repo("/n").is_err());
        assert!(parse_repo("o/").is_err());
        assert!(parse_repo("o").is_err());
    }
}
