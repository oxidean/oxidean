//! `ox` binary — thin shell over `oxidean_cli`: parse argv, load config,
//! dispatch, print, and map errors to exit codes.

use std::process::ExitCode;

use oxidean_cli::args::{self, Command};
use oxidean_cli::commands::{self, Out};
use oxidean_cli::{jq, pager, rpc};
use oxidean_core::RpcResponse;

#[tokio::main]
async fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let inv = match args::parse(&argv) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("ox: {e}");
            eprintln!("run `ox --help` for usage");
            return ExitCode::from(commands::EXIT_USAGE);
        }
    };
    if inv.global.help || matches!(inv.command, Command::Help) {
        println!("{}", args::USAGE);
        return ExitCode::SUCCESS;
    }
    if matches!(inv.command, Command::Version) {
        println!("ox {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let rt = match commands::runtime() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("ox: {e}");
            return ExitCode::from(e.exit_code());
        }
    };
    match commands::dispatch(&inv, &rt).await {
        Ok(Out::Report { text, json }) => {
            if let Some(filter) = &inv.global.jq {
                return jq_out(filter, &json["data"]);
            }
            if inv.global.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json).unwrap_or_default()
                );
            } else {
                pager::emit(&text);
            }
            ExitCode::SUCCESS
        }
        Ok(Out::Rpc { envelope, pretty }) => match &envelope {
            RpcResponse::Ok { data, .. } => {
                if let Some(filter) = &inv.global.jq {
                    return jq_out(filter, data);
                }
                if inv.global.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&envelope).unwrap_or_default()
                    );
                } else if !pretty.is_empty() {
                    pager::emit(&pretty);
                } else {
                    println!("{}", serde_json::to_string_pretty(data).unwrap_or_default());
                }
                ExitCode::SUCCESS
            }
            RpcResponse::Err { .. } => rpc_err(&inv, &rt, &envelope),
        },
        // Raw-bytes RPC results (`vcs cat`) — the blob goes to stdout
        // verbatim; error envelopes behave exactly like `Out::Rpc`.
        Ok(Out::Bytes { envelope, bytes }) => match &envelope {
            RpcResponse::Ok { data, .. } => {
                if let Some(filter) = &inv.global.jq {
                    return jq_out(filter, data);
                }
                if inv.global.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&envelope).unwrap_or_default()
                    );
                } else {
                    use std::io::Write;
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(&bytes);
                }
                ExitCode::SUCCESS
            }
            RpcResponse::Err { .. } => rpc_err(&inv, &rt, &envelope),
        },
        Err(e) => {
            eprintln!("ox: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

/// `RpcResponse::Err` handling shared by `Out::Rpc`/`Out::Bytes` — print the
/// error + auth hints on stderr, exit via `error_exit_code`.
fn rpc_err(inv: &args::Invocation, rt: &commands::Runtime, envelope: &RpcResponse) -> ExitCode {
    let RpcResponse::Err { error, .. } = envelope else {
        return ExitCode::SUCCESS;
    };
    if inv.global.json {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(envelope).unwrap_or_default()
        );
    } else {
        eprintln!("ox: {}: {}", error.code, error.message);
    }
    let instance = oxidean_cli::config::resolve_instance(
        inv.global.instance.as_deref(),
        oxidean_cli::config::env_instance().as_deref(),
        &rt.config,
    )
    .ok()
    .flatten()
    .unwrap_or_else(|| "<instance>".to_string());
    for hint in auth_hints(&error.code, &instance) {
        eprintln!("hint: {hint}");
    }
    ExitCode::from(rpc::error_exit_code(error))
}

/// `--jq` — filter `data` and print each result (strings bare like jq).
fn jq_out(filter: &str, data: &serde_json::Value) -> ExitCode {
    match jq::apply(filter, data) {
        Ok(results) => {
            println!("{}", jq::render(&results));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ox: {e}");
            ExitCode::from(commands::EXIT_USAGE)
        }
    }
}

/// Actionable recovery hints per auth error code — the `auth.*` taxonomy the
/// server emits. Printed to stderr so piped stdout stays clean.
fn auth_hints(code: &str, instance: &str) -> Vec<String> {
    let env_note = if std::env::var("OXIDEAN_TOKEN").is_ok() {
        " (OXIDEAN_TOKEN is set — the env token, not the stored one, was used)"
    } else {
        ""
    };
    match code {
        "auth.unauthenticated" | "auth.token_invalid" => vec![format!(
            "run `ox auth login` or set OXIDEAN_TOKEN{env_note}"
        )],
        "auth.token_expired" => vec![
            format!("token expired — mint a new one{env_note}"),
            format!("`ox auth login --web` to sign in again, or create a PAT at {instance}/settings/tokens"),
        ],
        "auth.token_revoked" => vec![
            format!("token was revoked — mint a new one{env_note}"),
            format!("`ox auth login --web` to sign in again, or create a PAT at {instance}/settings/tokens"),
        ],
        "auth.pat_scope" => vec![
            "token lacks a required scope — `ox auth status` shows current scopes"
                .to_string(),
            format!("mint a new PAT with the needed scope at {instance}/settings/tokens, or `ox auth login --web --scopes <list>`"),
        ],
        "auth.banned" => vec!["account is banned — contact the instance admin".to_string()],
        "vcs.unauthenticated" => vec![format!(
            "run `ox auth login` or set OXIDEAN_TOKEN{env_note}"
        )],
        "vcs.forbidden" => vec![
            "token lacks access to this repository — check repo permissions".to_string()
        ],
        _ => Vec::new(),
    }
}
