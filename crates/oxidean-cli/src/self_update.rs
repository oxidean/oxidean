//! `ox self-update` — pull the latest `ox` binary the configured instance
//! serves under `/cli/*` and replace the running executable in place.
//!
//! Flow: `GET {instance}/cli/latest` → compare `version` to the build version
//! → `GET /cli/bin/ox-{target}` (+ published `.sha256`) → verify → atomic swap
//! next to `current_exe`. Update locksteps the CLI to the instance it talks
//! to; when the instance has no binary for this platform the command reports
//! where to get one instead of guessing.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::args::Global;
use crate::commands::{CliError, Out, Runtime};

/// Rust target triple for the running binary — `std::env::consts` values map
/// 1:1 onto the `ox-{triple}` names the server dist dir uses.
pub fn current_target() -> String {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        "arm" => "armv7",
        other => other,
    };
    let os = match std::env::consts::OS {
        "linux" => "unknown-linux-gnu",
        "macos" => "apple-darwin",
        "windows" => "pc-windows-msvc",
        "freebsd" => "unknown-freebsd",
        other => other,
    };
    format!("{arch}-{os}")
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(64);
    for b in Sha256::digest(bytes) {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

struct Latest {
    version: String,
    /// target → expected sha256
    targets: Vec<(String, String)>,
}

async fn get_bytes(http: &reqwest::Client, url: &str) -> Result<Vec<u8>, CliError> {
    let res = http
        .get(url)
        .header("user-agent", concat!("ox/", env!("CARGO_PKG_VERSION")))
        .send()
        .await
        .map_err(|e| CliError::Transport(format!("request to {url} failed: {e}")))?;
    if !res.status().is_success() {
        return Err(CliError::Transport(format!(
            "GET {url} → HTTP {}",
            res.status().as_u16()
        )));
    }
    res.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| CliError::Transport(format!("reading {url} failed: {e}")))
}

async fn fetch_latest(http: &reqwest::Client, instance: &str) -> Result<Latest, CliError> {
    let url = format!("{instance}/cli/latest");
    let body = get_bytes(http, &url).await?;
    let json: Value = serde_json::from_slice(&body)
        .map_err(|e| CliError::Transport(format!("invalid /cli/latest response: {e}")))?;
    let version = json
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::Transport("/cli/latest missing version".into()))?
        .to_string();
    let mut targets = Vec::new();
    for t in json
        .get("targets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(name), Some(sha)) = (
            t.get("target").and_then(Value::as_str),
            t.get("sha256").and_then(Value::as_str),
        ) else {
            continue;
        };
        targets.push((name.to_string(), sha.to_string()));
    }
    Ok(Latest { version, targets })
}

/// Swap `exe` for `bytes`: rename-over on unix; on platforms where a running
/// image can't be replaced (Windows) the current exe is first moved aside.
/// Returns the path that was written.
pub fn apply_binary(exe: &Path, bytes: &[u8]) -> Result<PathBuf, CliError> {
    let dir = exe
        .parent()
        .ok_or_else(|| CliError::Transport("cannot resolve executable directory".into()))?;
    let tmp = dir.join(format!(".ox-update-{}", std::process::id()));
    fs::write(&tmp, bytes)
        .map_err(|e| CliError::Transport(format!("writing {}: {e}", tmp.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755));
    }
    match fs::rename(&tmp, exe) {
        Ok(()) => Ok(exe.to_path_buf()),
        Err(first) => {
            // Windows: a running exe can be renamed but not replaced — move it
            // aside, then move the new binary in.
            let backup = exe.with_extension("ox-old");
            fs::rename(exe, &backup)
                .and_then(|_| fs::rename(&tmp, exe))
                .map_err(|e| {
                    let _ = fs::remove_file(&tmp);
                    CliError::Transport(format!(
                        "replacing {}: {e} (rename failed earlier: {first})",
                        exe.display()
                    ))
                })?;
            let _ = fs::remove_file(&backup);
            Ok(exe.to_path_buf())
        }
    }
}

pub async fn run(global: &Global, rt: &Runtime) -> Result<Out, CliError> {
    let instance = crate::commands::resolve_instance(global, &rt.config)?;
    let http = reqwest::Client::new();
    let latest = fetch_latest(&http, &instance).await?;
    let current = env!("CARGO_PKG_VERSION");

    if latest.version == current {
        return Ok(Out::Report {
            text: format!("ox {current} is up to date ({instance})"),
            json: json!({"ok": true, "data": {"version": current, "updated": false}}),
        });
    }

    let target = current_target();
    let Some((_, expected_sha)) = latest.targets.iter().find(|(t, _)| *t == target) else {
        return Err(CliError::Transport(format!(
            "instance has no ox binary for {target} — update via \
             `curl -fsSL {instance}/cli/install.sh | sh` or `cargo install`"
        )));
    };

    let url = format!("{instance}/cli/bin/ox-{target}");
    let bytes = get_bytes(&http, &url).await?;
    let actual = sha256_hex(&bytes);
    if &actual != expected_sha {
        return Err(CliError::Transport(format!(
            "checksum mismatch for ox-{target} (expected {expected_sha}, got {actual}) — aborting"
        )));
    }

    let exe = std::env::current_exe()
        .map_err(|e| CliError::Transport(format!("cannot locate current executable: {e}")))?;
    apply_binary(&exe, &bytes)?;

    Ok(Out::Report {
        text: format!(
            "updated ox {current} → {} ({})",
            latest.version,
            exe.display()
        ),
        json: json!({"ok": true, "data": {
            "from": current,
            "to": latest.version,
            "path": exe,
            "updated": true,
        }}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_triple_looks_like_triple() {
        let t = current_target();
        assert!(t.contains('-'), "{t}");
    }

    #[test]
    fn apply_binary_replaces_file() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("ox");
        fs::write(&exe, b"old-binary").unwrap();
        apply_binary(&exe, b"new-binary").unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"new-binary");
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        // sha256("") — well-known test vector.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
