//! Companion CLI (`ox`) distribution — install script + platform binaries.
//!
//! Routes:
//!   GET /cli/install.sh       POSIX installer with the public origin baked in
//!   GET /cli/install.ps1      PowerShell installer (native Windows)
//!   GET /cli/skill.md         consumer-facing agent skill (embedded single source)
//!   GET /cli/latest           JSON { version, targets: [{ target, sha256, bytes }],
//!                                   dist_commit? } — served from the release-written
//!                                   `latest.json`; `{version: null, targets: []}`
//!                                   when no release has published one.
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

/// `latest.json` as written by the `ox-v*` release lane (C5): the published
/// CLI component version, the commit the dist matrix was built from, and one
/// record per shipped target.
#[derive(serde::Deserialize)]
struct LatestManifest {
    schema_version: Option<u64>,
    version: serde_json::Value,
    dist_commit: Option<String>,
    targets: Option<Vec<LatestTarget>>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct LatestTarget {
    target: String,
    sha256: String,
    bytes: u64,
}

fn manifest_is_valid(m: &LatestManifest) -> bool {
    // schema_version is the C5 marker: absent tolerates hand-written files,
    // anything but 1 is a format this handler doesn't know how to read.
    m.version.as_str().is_some_and(|v| !v.is_empty())
        && m.targets.is_some()
        && m.schema_version.is_none_or(|v| v == 1)
}

/// Reads and validates `<dir>/latest.json`. `None` covers every unpublished
/// shape — absent, unreadable, malformed JSON, or missing `version`/`targets`.
fn load_latest_manifest(dir: &std::path::Path) -> Option<LatestManifest> {
    let manifest_path = dir.join("latest.json");
    match std::fs::read_to_string(&manifest_path) {
        Ok(text) => match serde_json::from_str::<LatestManifest>(&text) {
            Ok(m) if manifest_is_valid(&m) => Some(m),
            Ok(_) => {
                tracing::warn!(path = %manifest_path.display(), "latest.json missing version/targets — reporting unpublished");
                None
            }
            Err(e) => {
                tracing::warn!(path = %manifest_path.display(), error = %e, "latest.json is corrupt — reporting unpublished");
                None
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            tracing::warn!(path = %manifest_path.display(), error = %e, "cannot read latest.json — reporting unpublished");
            None
        }
    }
}

/// `/cli/latest` reads the release-owned `latest.json` — the published `ox`
/// version is distribution state, never the API crate version (BR6.5).
/// Absent or corrupt manifests answer with the unpublished marker so
/// `ox self-update` fails honestly instead of chasing a server version.
pub async fn latest() -> Response {
    let manifest = load_latest_manifest(&dist_dir());

    let mut body = serde_json::json!({
        "ok": true,
        "version": manifest.as_ref().map(|m| &m.version).cloned().unwrap_or(serde_json::Value::Null),
        "targets": manifest.as_ref().and_then(|m| m.targets.clone()).map(|targets| {
            targets.iter().map(|t| serde_json::json!({
                "target": t.target,
                "sha256": t.sha256,
                "bytes": t.bytes,
            })).collect::<Vec<_>>()
        }).unwrap_or_default(),
    });
    let commit = manifest
        .and_then(|m| m.dist_commit.filter(|c| !c.is_empty()))
        .or_else(dist_commit);
    if let Some(commit) = commit {
        body["dist_commit"] = serde_json::json!(commit);
    }
    Json(body).into_response()
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

    #[test]
    fn latest_manifest_absent_is_unpublished() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_latest_manifest(dir.path()).is_none());
    }

    #[test]
    fn latest_manifest_corrupt_is_unpublished() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("latest.json"), b"{not json").unwrap();
        assert!(load_latest_manifest(dir.path()).is_none());
    }

    #[test]
    fn latest_manifest_missing_version_is_unpublished() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("latest.json"),
            r#"{"schema_version": 1, "targets": []}"#,
        )
        .unwrap();
        assert!(load_latest_manifest(dir.path()).is_none());
    }

    #[test]
    fn latest_manifest_valid_reads_version_and_targets() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("latest.json"),
            r#"{
                "schema_version": 1,
                "version": "0.2.3",
                "dist_commit": "0123456789abcdef0123456789abcdef01234567",
                "targets": [
                    {"target": "x86_64-unknown-linux-gnu", "sha256": "ab", "bytes": 12}
                ]
            }"#,
        )
        .unwrap();
        let m = load_latest_manifest(dir.path()).unwrap();
        assert_eq!(m.version, "0.2.3");
        let targets = m.targets.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target, "x86_64-unknown-linux-gnu");
        assert_eq!(
            m.dist_commit.as_deref(),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
    }
}
