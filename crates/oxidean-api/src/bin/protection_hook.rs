//! Ref-protection update-hook helper (Phase 13 / D-19; GIT-21 tags).
//!
//! Invoked from bare-repo `hooks/update` with:
//! `oxidean-protection-hook update <ref> <oldsha> <newsha>`
//!
//! Env:
//! - `OXIDEAN_DATABASE_URL` (required)
//! - `OXIDEAN_REPOS_DIR` (required)
//! - `OXIDEAN_ACTOR_CAPABILITY` = admin|write|read (default read)
//! - `GIT_DIR` set by git

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use oxidean_api::protection::{capability_from_env, check_ref_update};
use oxidean_db::Database;

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    if cmd != "update" {
        eprintln!("oxidean-protection-hook: usage: update <ref> <oldsha> <newsha>");
        return ExitCode::from(2);
    }
    let git_ref = args.next().unwrap_or_default();
    let old_sha = args.next().unwrap_or_default();
    let new_sha = args.next().unwrap_or_default();
    if git_ref.is_empty() || old_sha.is_empty() || new_sha.is_empty() {
        eprintln!("oxidean-protection-hook: missing args");
        return ExitCode::from(2);
    }

    let db_url = match env::var("OXIDEAN_DATABASE_URL").or_else(|_| env::var("DATABASE_URL")) {
        Ok(u) if !u.is_empty() => u,
        _ => {
            eprintln!("oxidean-protection-hook: OXIDEAN_DATABASE_URL not set");
            return ExitCode::from(1);
        }
    };
    let repos_dir = match env::var("OXIDEAN_REPOS_DIR") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            eprintln!("oxidean-protection-hook: OXIDEAN_REPOS_DIR not set");
            return ExitCode::from(1);
        }
    };
    let git_dir = match env::var("GIT_DIR") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            eprintln!("oxidean-protection-hook: GIT_DIR not set");
            return ExitCode::from(1);
        }
    };
    let capability = capability_from_env(
        &env::var("OXIDEAN_ACTOR_CAPABILITY").unwrap_or_else(|_| "read".into()),
    );

    let db = match Database::connect(&db_url).await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("oxidean-protection-hook: db connect failed: {e}");
            return ExitCode::from(1);
        }
    };

    match check_ref_update(
        &db,
        &repos_dir,
        &git_dir,
        &git_ref,
        &old_sha,
        &new_sha,
        capability,
    )
    .await
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("oxidean-protection: {} ({})", e.message, e.code);
            ExitCode::from(1)
        }
    }
}
