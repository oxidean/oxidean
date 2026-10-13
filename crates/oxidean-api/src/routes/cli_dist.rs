//! Companion CLI (`ox`) distribution — install script + platform binaries.
//!
//! Routes:
//!   GET /cli/install.sh       POSIX installer with the public origin baked in
//!   GET /cli/install.ps1      PowerShell installer (native Windows)
//!   GET /cli/skill.md         consumer-facing agent skill (embedded single source)
//!   GET /cli/latest           JSON { version, targets: [{ target, sha256, bytes }],
//!                                   dist_commit? }
//!   GET /cli/bin/{*file}      `ox-{target}` binary, or `ox-{target}.sha256` digest
//!
//! Binaries live in `OXIDEAN_CLI_DIST_DIR` (default `/usr/local/share/oxidean-cli`)
//! as `ox-{rust-target-triple}` — the release image ships the server's own
//! platform; self-hosters can drop additional targets into the same directory.
//! Responses are unauthenticated: `ox self-update` and `curl | sh` must work
//! before `ox auth login` exists.

use std::path::PathBuf;

use axum::Json;
use axum::extract::Path as AxumPath;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use crate::public_origin::resolve_public_origin;

const DEFAULT_DIST_DIR: &str = "/usr/local/share/oxidean-cli";
const BINARY_PREFIX: &str = "ox-";
const MAX_BINARY_BYTES: u64 = 256 * 1024 * 1024;

fn dist_dir() -> PathBuf {
    std::env::var("OXIDEAN_CLI_DIST_DIR")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DIST_DIR))
}

/// `ox-{target}` names: triples are `[a-z0-9._-]` — reject anything else so the
/// wildcard path can never traverse out of the dist dir.
fn valid_binary_name(name: &str) -> bool {
    let Some(target) = name.strip_prefix(BINARY_PREFIX) else {
        return false;
    };
    !target.is_empty()
        && !target.starts_with('.')
        && target
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn not_found(msg: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "ok": false,
            "error": { "code": "cli.binary_not_found", "message": msg }
        })),
    )
        .into_response()
}

struct TargetEntry {
    target: String,
    sha256: String,
    bytes: u64,
}

/// `ox-{target}` files present in the dist dir (sorted for stable output).
fn list_targets() -> Result<Vec<TargetEntry>, std::io::Error> {
    let dir = dist_dir();
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e),
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !valid_binary_name(name) {
            continue;
        }
        let meta = entry.metadata()?;
        if !meta.is_file() || meta.len() > MAX_BINARY_BYTES {
            continue;
        }
        let bytes = std::fs::read(entry.path())?;
        out.push(TargetEntry {
            target: name[BINARY_PREFIX.len()..].to_string(),
            sha256: bytes_to_hex(&Sha256::digest(&bytes)),
            bytes: meta.len(),
        });
    }
    out.sort_by(|a, b| a.target.cmp(&b.target));
    Ok(out)
}

/// Optional git SHA written by the ox-dist publisher / Dockerfile staging
/// (`SOURCE_COMMIT` in the dist dir). Not an `ox-*` binary — provenance only.
fn dist_commit() -> Option<String> {
    let path = dist_dir().join("SOURCE_COMMIT");
    let raw = std::fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_hexdigit())
    {
        return None;
    }
    Some(trimmed.to_string())
}

pub async fn latest() -> Response {
    match list_targets() {
        Ok(targets) => {
            let mut body = serde_json::json!({
                "ok": true,
                "version": env!("CARGO_PKG_VERSION"),
                "targets": targets.iter().map(|t| serde_json::json!({
                    "target": t.target,
                    "sha256": t.sha256,
                    "bytes": t.bytes,
                })).collect::<Vec<_>>(),
            });
            if let Some(commit) = dist_commit() {
                body["dist_commit"] = serde_json::json!(commit);
            }
            Json(body).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": { "code": "cli.dist_unavailable", "message": format!("cannot read CLI dist dir: {e}") }
            })),
        )
            .into_response(),
    }
}

/// `/cli/bin/ox-x86_64-unknown-linux-gnu` or `….sha256` (sha256sum-format text,
/// verifiable with `sha256sum -c` / `shasum -a 256 -c`).
pub async fn binary(AxumPath(file): AxumPath<String>) -> Response {
    let (name, want_digest) = match file.strip_suffix(".sha256") {
        Some(base) => (base.to_string(), true),
        None => (file, false),
    };
    if !valid_binary_name(&name) {
        return not_found("unknown CLI artifact");
    }
    let path = dist_dir().join(&name);
    let bytes = match std::fs::read(&path) {
        Ok(b) if b.len() as u64 <= MAX_BINARY_BYTES => b,
        _ => return not_found(&format!("no CLI binary for {name}")),
    };
    if want_digest {
        let hex = bytes_to_hex(&Sha256::digest(&bytes));
        return (
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("{hex}  {name}\n"),
        )
            .into_response();
    }
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"ox\""),
        ],
        bytes,
    )
        .into_response()
}

/// `install.sh` with the resolved public origin substituted.
pub async fn install_script() -> Response {
    let origin = resolve_public_origin();
    let script = INSTALL_SCRIPT.replace("@OXIDEAN_ORIGIN@", &origin);
    (
        [
            (header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=\"install.sh\"",
            ),
        ],
        script,
    )
        .into_response()
}

/// `install.ps1` with the resolved public origin substituted.
pub async fn install_ps1() -> Response {
    let origin = resolve_public_origin();
    let script = INSTALL_PS1.replace("@OXIDEAN_ORIGIN@", &origin);
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=\"install.ps1\"",
            ),
        ],
        script,
    )
        .into_response()
}

/// POSIX installer — detect the target triple, download `ox-{target}` plus its
/// published digest, verify, and place the binary. Falls back to `cargo
/// install` instructions when the instance has no prebuilt artifact.
const INSTALL_SCRIPT: &str = include_str!("../../assets/cli/install.sh");

/// PowerShell installer for native Windows (`irm …/cli/install.ps1 | iex`).
/// Same contract: detect arch → `ox-{target}` + `.sha256` → `Get-FileHash`
/// verify → `%LOCALAPPDATA%\Programs\ox\ox.exe` → PATH hint.
const INSTALL_PS1: &str = include_str!("../../assets/cli/install.ps1");

/// Consumer-facing agent SKILL.md — single source of truth lives in the CLI
/// crate (`crates/oxidean-cli/skill/SKILL.md`) and is embedded into `ox` for
/// `ox skill print`/`install`; serving it here versions the doc with the
/// server so fetched copies match the RPC surface they document.
const SKILL_MD: &str = include_str!("../../../oxidean-cli/skill/SKILL.md");

/// `GET /cli/skill.md` — the agent skill document, unauthenticated like the
/// install scripts.
pub async fn skill_md() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=\"SKILL.md\"",
            ),
        ],
        SKILL_MD,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_name_validation() {
        assert!(valid_binary_name("ox-x86_64-unknown-linux-gnu"));
        assert!(valid_binary_name("ox-aarch64-apple-darwin"));
        assert!(!valid_binary_name("ox-"));
        assert!(!valid_binary_name("ox-../etc/passwd"));
        assert!(!valid_binary_name("ox-a/b"));
        assert!(!valid_binary_name("other"));
        assert!(!valid_binary_name("ox-.hidden"));
    }

    #[test]
    fn install_script_substitutes_origin() {
        let out = INSTALL_SCRIPT.replace("@OXIDEAN_ORIGIN@", "https://forge.example.com");
        assert!(out.contains("BASE=\"https://forge.example.com\""));
        assert!(!out.contains("@OXIDEAN_ORIGIN@"));
        assert!(out.starts_with("#!/bin/sh"));
    }

    #[test]
    fn install_script_covers_shipped_targets() {
        // Target mapping contract: every triple the build matrix ships has an
        // install.sh path — including musl (Alpine) and the corrected armv7
        // ABI-suffixed triples.
        for needle in [
            "unknown-linux-musl",
            "apple-darwin",
            "unknown-freebsd",
            "pc-windows-msvc",
            "armv7-unknown-linux-gnueabihf",
            "armv7-unknown-linux-musleabihf",
        ] {
            assert!(INSTALL_SCRIPT.contains(needle), "install.sh missing {needle}");
        }
    }

    #[test]
    fn skill_doc_is_the_embedded_source() {
        assert!(SKILL_MD.starts_with("---\nname: ox"));
        assert!(SKILL_MD.contains("ox api"));
        assert!(SKILL_MD.contains("--json"));
    }

    #[test]
    fn install_ps1_substitutes_origin() {
        let out = INSTALL_PS1.replace("@OXIDEAN_ORIGIN@", "https://forge.example.com");
        assert!(out.contains("$Base = 'https://forge.example.com'"));
        assert!(!out.contains("@OXIDEAN_ORIGIN@"));
        assert!(out.contains("pc-windows-msvc"));
        assert!(out.contains("Get-FileHash"));
    }
}
