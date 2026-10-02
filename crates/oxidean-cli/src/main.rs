//! `ox` binary — thin shell over `oxidean_cli`: parse argv, load config,
//! dispatch, print, and map errors to exit codes.

use std::process::ExitCode;

use oxidean_cli::args::{self, Command};
use oxidean_cli::commands::{self, Out};
use oxidean_cli::rpc;
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
            if inv.global.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json).unwrap_or_default()
                );
            } else {
                println!("{text}");
            }
            ExitCode::SUCCESS
        }
        Ok(Out::Rpc { envelope, pretty }) => match &envelope {
            RpcResponse::Ok { data, .. } => {
                if inv.global.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&envelope).unwrap_or_default()
                    );
                } else if !pretty.is_empty() {
                    println!("{pretty}");
                } else {
                    println!("{}", serde_json::to_string_pretty(data).unwrap_or_default());
                }
                ExitCode::SUCCESS
            }
            RpcResponse::Err { error, .. } => {
                if inv.global.json {
                    eprintln!(
                        "{}",
                        serde_json::to_string_pretty(&envelope).unwrap_or_default()
                    );
                } else {
                    eprintln!("ox: {}: {}", error.code, error.message);
                }
                if error.code == "auth.unauthenticated" {
                    eprintln!("hint: run `ox auth login` or set OXIDEAN_TOKEN");
                }
                ExitCode::from(rpc::error_exit_code(error))
            }
        },
        Err(e) => {
            eprintln!("ox: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}
