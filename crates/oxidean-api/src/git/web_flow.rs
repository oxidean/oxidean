//! Instance “web-flow” Ed25519 SSH signing key for forge-authored seed commits.
//!
//! Key pair lives next to the SSH host key under `OXIDEAN_SSH_HOST_KEY_DIR`
//! (default `var/ssh`): private `web-flow`, public `web-flow.pub`.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use russh::keys::PrivateKey;
use tokio::process::Command;

use crate::ssh::host_keys;

const WEB_FLOW_KEY_BASENAME: &str = "web-flow";
const WEB_FLOW_KEY_COMMENT: &str = "oxidean-web-flow";
/// Optional provisioned secret: OpenSSH private key (PEM, or single-line
/// base64-encoded PEM). When set, `web-flow`/`web-flow.pub` are materialized
/// from it — deterministic signing identity across fresh volumes (Railway PR
/// environments, replaced mounts). `production`/`cloud` require either this
/// variable or pre-provisioned keypair files.
const WEB_FLOW_PRIVATE_KEY_ENV: &str = "OXIDEAN_WEB_FLOW_PRIVATE_KEY";

/// Directory for the web-flow key (same as SSH host keys).
pub fn web_flow_dir() -> PathBuf {
    host_keys::host_key_dir()
}

/// Private key path: `{ssh_host_key_dir}/web-flow`.
pub fn private_key_path() -> PathBuf {
    web_flow_dir().join(WEB_FLOW_KEY_BASENAME)
}

/// Public key path: `{ssh_host_key_dir}/web-flow.pub`.
pub fn public_key_path() -> PathBuf {
    web_flow_dir().join(format!("{WEB_FLOW_KEY_BASENAME}.pub"))
}

fn oxidean_env() -> String {
    std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into())
}

/// Environments that must provision the keypair — `OXIDEAN_WEB_FLOW_PRIVATE_KEY`
/// or `web-flow`/`web-flow.pub` files under `OXIDEAN_SSH_HOST_KEY_DIR`. Every
/// other env (development, compose, test, Railway `preview`/`staging` and the PR
/// environments that inherit them) may auto-generate on first use. Mirrors the
/// `production|cloud` gate used by webhook URL policy and the update hook.
fn requires_provisioned_key(env: &str) -> bool {
    matches!(env.to_ascii_lowercase().as_str(), "production" | "cloud")
}

/// Resolve against process cwd when relative.
///
/// Seed commits run `git -C <tmpdir> … -c user.signingkey=…`; a relative key
/// path would resolve under the temp worktree, not the package cwd.
fn absolute_key_path(path: PathBuf) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path);
    }
    let cwd = std::env::current_dir().map_err(|e| format!("cwd for web-flow key: {e}"))?;
    Ok(cwd.join(path))
}

/// Tiny base64 decoder — same shape as `routes::git_smart_http::base64_lite`
/// (no new crates.io dep).
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|&b| !b.is_ascii_whitespace())
        .collect();
    if bytes.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: i32 = 0;
    for &b in &bytes {
        if b == b'=' {
            break;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Decode `OXIDEAN_WEB_FLOW_PRIVATE_KEY` material: raw OpenSSH PEM passes
/// through; anything else is treated as base64-encoded PEM.
fn decode_private_key_pem(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(format!("{WEB_FLOW_PRIVATE_KEY_ENV} is empty"));
    }
    if trimmed.contains("-----BEGIN") {
        return Ok(trimmed.to_string());
    }
    let bytes = base64_decode(trimmed)
        .ok_or_else(|| format!("{WEB_FLOW_PRIVATE_KEY_ENV} is neither PEM nor valid base64"))?;
    let pem = String::from_utf8(bytes)
        .map_err(|_| format!("{WEB_FLOW_PRIVATE_KEY_ENV} base64 did not decode to utf8 PEM"))?;
    if !pem.contains("-----BEGIN") {
        return Err(format!(
            "{WEB_FLOW_PRIVATE_KEY_ENV} did not decode to an OpenSSH PEM"
        ));
    }
    Ok(pem.trim().to_string())
}

/// Persist a provisioned private key as `web-flow`/`web-flow.pub` (normalized
/// OpenSSH, `web-flow.pub` carrying the standard comment). The env value is the
/// source of truth when set — files are rewritten so rotation = changing the
/// variable.
async fn materialize_env_key(dir: &Path, pem: &str) -> Result<PathBuf, String> {
    let key = PrivateKey::from_openssh(pem)
        .map_err(|e| format!("{WEB_FLOW_PRIVATE_KEY_ENV} parse: {e}"))?;
    let normalized = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .map_err(|e| format!("{WEB_FLOW_PRIVATE_KEY_ENV} encode: {e}"))?;
    let mut public = key.public_key().clone();
    public.set_comment(WEB_FLOW_KEY_COMMENT);
    let pub_line = public
        .to_openssh()
        .map_err(|e| format!("{WEB_FLOW_PRIVATE_KEY_ENV} public key: {e}"))?;

    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("create web-flow key dir: {e}"))?;
    let priv_path = dir.join(WEB_FLOW_KEY_BASENAME);
    let pub_path = dir.join(format!("{WEB_FLOW_KEY_BASENAME}.pub"));
    tokio::fs::write(&priv_path, normalized.as_bytes())
        .await
        .map_err(|e| format!("write web-flow key: {e}"))?;
    tokio::fs::write(&pub_path, format!("{pub_line}\n"))
        .await
        .map_err(|e| format!("write web-flow.pub: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ =
            tokio::fs::set_permissions(&priv_path, std::fs::Permissions::from_mode(0o600)).await;
    }

    Ok(priv_path)
}

/// True when `web-flow` and `web-flow.pub` exist **and** the public key is the
/// counterpart of the private key. Parallel first-run generation can otherwise
/// persist a split pair (priv from one `ssh-keygen`, pub from another) that
/// `try_exists` alone would accept forever — `git commit -S` then fails with
/// "public key doesn't match private" on every sign.
async fn keypair_matches(priv_path: &Path, pub_path: &Path) -> bool {
    let (priv_bytes, pub_bytes) = match (
        tokio::fs::read(priv_path).await,
        tokio::fs::read(pub_path).await,
    ) {
        (Ok(p), Ok(u)) => (p, u),
        _ => return false,
    };
    let key = match PrivateKey::from_openssh(&priv_bytes) {
        Ok(k) => k,
        Err(_) => return false,
    };
    let derived = match key.public_key().to_openssh() {
        Ok(l) => l,
        Err(_) => return false,
    };
    // Compare the first two fields (type + blob); the trailing comment differs
    // between `ssh-keygen -C` output and re-serialized keys.
    let pub_text = String::from_utf8_lossy(&pub_bytes).into_owned();
    let mut want = derived.split_whitespace().take(2);
    let mut got = pub_text.split_whitespace().take(2);
    matches!(
        (want.next(), want.next(), got.next(), got.next()),
        (Some(a), Some(b), Some(c), Some(d)) if a == c && b == d
    )
}

/// Wait for another process to finish generating the pair, up to
/// `GENERATE_WAIT`. Returns `true` once a *valid* pair is on disk.
async fn wait_for_pair(priv_path: &Path, pub_path: &Path) -> bool {
    const GENERATE_WAIT: std::time::Duration = std::time::Duration::from_secs(15);
    let deadline = std::time::Instant::now() + GENERATE_WAIT;
    while std::time::Instant::now() < deadline {
        if keypair_matches(priv_path, pub_path).await {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    keypair_matches(priv_path, pub_path).await
}

/// Ensure the Ed25519 web-flow key exists.
///
/// Returns an **absolute** private-key path so `git -C <tmpdir> commit -S` can
/// load it (relative paths resolve under the worktree).
///
/// Precedence:
/// 1. `OXIDEAN_WEB_FLOW_PRIVATE_KEY` set → materialize files from it.
/// 2. Existing **matching** `web-flow`/`web-flow.pub` on disk → use them.
/// 3. `production`/`cloud` → fail closed (must provision).
/// 4. Anything else (development, compose, test, Railway `preview`/`staging`,
///    PR environments) → generate with `ssh-keygen`, serialized through a
///    `web-flow.lock` sentinel so concurrent first runs cannot split the pair.
pub async fn ensure_web_flow_key() -> Result<PathBuf, String> {
    let dir = absolute_key_path(web_flow_dir())?;
    let priv_path = dir.join(WEB_FLOW_KEY_BASENAME);
    let pub_path = dir.join(format!("{WEB_FLOW_KEY_BASENAME}.pub"));

    if let Some(raw) = std::env::var(WEB_FLOW_PRIVATE_KEY_ENV)
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        let pem = decode_private_key_pem(&raw)?;
        return materialize_env_key(&dir, &pem).await;
    }

    if keypair_matches(&priv_path, &pub_path).await {
        return Ok(priv_path);
    }

    if requires_provisioned_key(&oxidean_env()) {
        return Err(format!(
            "web-flow signing key missing or mismatched at {} (and {}.pub); set {} or provision the keypair under OXIDEAN_SSH_HOST_KEY_DIR",
            priv_path.display(),
            WEB_FLOW_KEY_BASENAME,
            WEB_FLOW_PRIVATE_KEY_ENV
        ));
    }

    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("create web-flow key dir: {e}"))?;

    // Cross-process lock: only one `ssh-keygen` may write the pair. `create_new`
    // is atomic on every platform we target.
    let lock_path = dir.join(format!("{WEB_FLOW_KEY_BASENAME}.lock"));
    let mut attempts = 0u8;
    let _guard = loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => break GenerationLock(lock_path.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if wait_for_pair(&priv_path, &pub_path).await {
                    return Ok(priv_path);
                }
                attempts += 1;
                if attempts > 1 {
                    return Err(
                        "web-flow signing key still absent after waiting for concurrent generation"
                            .to_string(),
                    );
                }
                // Stale lock from a crashed generator — take over once.
                let _ = std::fs::remove_file(&lock_path);
            }
            Err(e) => return Err(format!("web-flow lock {}: {e}", lock_path.display())),
        }
    };

    // Remove partial files before generating.
    let _ = tokio::fs::remove_file(&priv_path).await;
    let _ = tokio::fs::remove_file(&pub_path).await;

    let out = Command::new("ssh-keygen")
        .args([
            "-t",
            "ed25519",
            "-N",
            "",
            "-f",
            priv_path
                .to_str()
                .ok_or_else(|| "non-utf8 web-flow key path".to_string())?,
            "-C",
            WEB_FLOW_KEY_COMMENT,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("failed to spawn ssh-keygen: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("ssh-keygen failed: {}", stderr.trim()));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ =
            tokio::fs::set_permissions(&priv_path, std::fs::Permissions::from_mode(0o600)).await;
    }

    Ok(priv_path)
}

/// Removes the `web-flow.lock` sentinel on drop.
struct GenerationLock(PathBuf);

impl Drop for GenerationLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Read the OpenSSH public key line (`ssh-ed25519 AAAA… comment`), if present.
pub async fn public_key_line() -> Result<Option<String>, String> {
    let path = public_key_path();
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Ok(None);
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("read web-flow.pub: {e}"))?;
    let line = String::from_utf8(bytes)
        .map_err(|e| format!("web-flow.pub utf8: {e}"))?
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if line.is_empty() {
        Ok(None)
    } else {
        Ok(Some(line))
    }
}

/// Public key bytes (OpenSSH `.pub` file contents), if present.
pub async fn public_key_bytes() -> Result<Option<Vec<u8>>, String> {
    let path = public_key_path();
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Ok(None);
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("read web-flow.pub: {e}"))?;
    Ok(Some(bytes))
}

/// True when both private and public key files exist.
pub async fn key_present() -> bool {
    let priv_ok = tokio::fs::try_exists(private_key_path())
        .await
        .unwrap_or(false);
    let pub_ok = tokio::fs::try_exists(public_key_path())
        .await
        .unwrap_or(false);
    priv_ok && pub_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::Algorithm;
    use tokio::sync::Mutex;

    /// Serialize tests that mutate process env (`OXIDEAN_ENV`,
    /// `OXIDEAN_SSH_HOST_KEY_DIR`, `OXIDEAN_WEB_FLOW_PRIVATE_KEY`).
    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    fn fresh_ed25519_pem() -> String {
        let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("random key");
        key.to_openssh(russh::keys::ssh_key::LineEnding::LF)
            .expect("encode pem")
            .to_string()
    }

    fn b64_encode(bytes: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(T[(n >> 18) as usize & 63] as char);
            out.push(T[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 {
                T[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                T[n as usize & 63] as char
            } else {
                '='
            });
        }
        out
    }

    #[test]
    fn provisioned_key_required_only_for_prod_envs() {
        for env in ["production", "cloud", "PRODUCTION", "Cloud"] {
            assert!(requires_provisioned_key(env), "{env} must provision");
        }
        for env in [
            "development",
            "dev",
            "compose",
            "test",
            "preview",
            "staging",
            "pr-123",
            "",
        ] {
            assert!(!requires_provisioned_key(env), "{env} may auto-generate");
        }
    }

    #[test]
    fn decode_accepts_raw_pem_and_base64_pem() {
        let pem = fresh_ed25519_pem();
        assert_eq!(decode_private_key_pem(&pem).unwrap(), pem.trim());
        let b64 = b64_encode(pem.as_bytes());
        assert_eq!(decode_private_key_pem(&b64).unwrap(), pem.trim());
        assert!(decode_private_key_pem("   ").is_err());
        assert!(decode_private_key_pem("not-a-key-not-b64!!!").is_err());
        assert!(decode_private_key_pem(&b64_encode(b"plain text")).is_err());
    }

    #[tokio::test]
    async fn materialize_env_key_writes_pair() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pem = fresh_ed25519_pem();
        let priv_path = materialize_env_key(dir.path(), &pem)
            .await
            .expect("materialize");
        assert_eq!(priv_path, dir.path().join(WEB_FLOW_KEY_BASENAME));
        let saved = std::fs::read_to_string(&priv_path).expect("read priv");
        PrivateKey::from_openssh(&saved).expect("persisted priv parses");
        let pub_line = std::fs::read_to_string(dir.path().join("web-flow.pub")).expect("read pub");
        assert!(pub_line.starts_with("ssh-ed25519 "), "pub: {pub_line}");
        assert!(pub_line.contains(WEB_FLOW_KEY_COMMENT), "pub: {pub_line}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&priv_path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[tokio::test]
    async fn ensure_uses_env_key_even_on_production() {
        let _g = ENV_LOCK.lock().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let pem = fresh_ed25519_pem();

        let prev_dir = std::env::var_os("OXIDEAN_SSH_HOST_KEY_DIR");
        let prev_env = std::env::var_os("OXIDEAN_ENV");
        let prev_key = std::env::var_os(WEB_FLOW_PRIVATE_KEY_ENV);
        std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", dir.path());
        std::env::set_var("OXIDEAN_ENV", "production");
        std::env::set_var(WEB_FLOW_PRIVATE_KEY_ENV, &pem);

        let res = ensure_web_flow_key().await;

        match prev_dir {
            Some(v) => std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", v),
            None => std::env::remove_var("OXIDEAN_SSH_HOST_KEY_DIR"),
        }
        match prev_env {
            Some(v) => std::env::set_var("OXIDEAN_ENV", v),
            None => std::env::remove_var("OXIDEAN_ENV"),
        }
        match prev_key {
            Some(v) => std::env::set_var(WEB_FLOW_PRIVATE_KEY_ENV, v),
            None => std::env::remove_var(WEB_FLOW_PRIVATE_KEY_ENV),
        }

        let p = res.expect("ensure with env key");
        assert_eq!(p, dir.path().join(WEB_FLOW_KEY_BASENAME));
        assert!(dir.path().join("web-flow.pub").is_file());
    }

    #[tokio::test]
    async fn ensure_fails_closed_on_production_without_key() {
        let _g = ENV_LOCK.lock().await;
        let dir = tempfile::tempdir().expect("tempdir");

        let prev_dir = std::env::var_os("OXIDEAN_SSH_HOST_KEY_DIR");
        let prev_env = std::env::var_os("OXIDEAN_ENV");
        let prev_key = std::env::var_os(WEB_FLOW_PRIVATE_KEY_ENV);
        std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", dir.path());
        std::env::set_var("OXIDEAN_ENV", "production");
        std::env::remove_var(WEB_FLOW_PRIVATE_KEY_ENV);

        let res = ensure_web_flow_key().await;

        match prev_dir {
            Some(v) => std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", v),
            None => std::env::remove_var("OXIDEAN_SSH_HOST_KEY_DIR"),
        }
        match prev_env {
            Some(v) => std::env::set_var("OXIDEAN_ENV", v),
            None => std::env::remove_var("OXIDEAN_ENV"),
        }
        match prev_key {
            Some(v) => std::env::set_var(WEB_FLOW_PRIVATE_KEY_ENV, v),
            None => std::env::remove_var(WEB_FLOW_PRIVATE_KEY_ENV),
        }

        let err = res.expect_err("production without key must fail closed");
        assert!(
            err.contains(WEB_FLOW_PRIVATE_KEY_ENV),
            "error must name the env var: {err}"
        );
    }

    #[tokio::test]
    async fn ensure_concurrent_generators_share_one_pair() {
        let _g = ENV_LOCK.lock().await;
        let ssh_keygen_ok = std::process::Command::new("ssh-keygen")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok();
        if !ssh_keygen_ok {
            eprintln!("skipping: ssh-keygen not available");
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");

        let prev_dir = std::env::var_os("OXIDEAN_SSH_HOST_KEY_DIR");
        let prev_env = std::env::var_os("OXIDEAN_ENV");
        let prev_key = std::env::var_os(WEB_FLOW_PRIVATE_KEY_ENV);
        std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", dir.path());
        std::env::set_var("OXIDEAN_ENV", "preview");
        std::env::remove_var(WEB_FLOW_PRIVATE_KEY_ENV);

        // Cold cache, 6 callers racing — without the lock this splits the
        // priv/pub pair across interleaved ssh-keygen runs.
        let mut handles = Vec::new();
        for _ in 0..6 {
            handles.push(tokio::spawn(ensure_web_flow_key()));
        }
        let mut results = Vec::new();
        for h in handles {
            results.push(h.await.expect("join"));
        }

        match prev_dir {
            Some(v) => std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", v),
            None => std::env::remove_var("OXIDEAN_SSH_HOST_KEY_DIR"),
        }
        match prev_env {
            Some(v) => std::env::set_var("OXIDEAN_ENV", v),
            None => std::env::remove_var("OXIDEAN_ENV"),
        }
        match prev_key {
            Some(v) => std::env::set_var(WEB_FLOW_PRIVATE_KEY_ENV, v),
            None => std::env::remove_var(WEB_FLOW_PRIVATE_KEY_ENV),
        }

        for r in &results {
            let p = r.as_ref().expect("every caller gets a key");
            assert_eq!(p, &dir.path().join(WEB_FLOW_KEY_BASENAME));
        }
        assert!(
            keypair_matches(
                &dir.path().join(WEB_FLOW_KEY_BASENAME),
                &dir.path().join(format!("{WEB_FLOW_KEY_BASENAME}.pub")),
            )
            .await
        );
        assert!(!dir.path().join("web-flow.lock").exists(), "lock released");
    }

    #[tokio::test]
    async fn ensure_autogenerates_on_preview_env() {
        let _g = ENV_LOCK.lock().await;
        // Spawn-only check — ssh-keygen has no --version; exit code is irrelevant.
        let ssh_keygen_ok = std::process::Command::new("ssh-keygen")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok();
        if !ssh_keygen_ok {
            eprintln!("skipping: ssh-keygen not available");
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");

        let prev_dir = std::env::var_os("OXIDEAN_SSH_HOST_KEY_DIR");
        let prev_env = std::env::var_os("OXIDEAN_ENV");
        let prev_key = std::env::var_os(WEB_FLOW_PRIVATE_KEY_ENV);
        std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", dir.path());
        std::env::set_var("OXIDEAN_ENV", "preview");
        std::env::remove_var(WEB_FLOW_PRIVATE_KEY_ENV);

        let res = ensure_web_flow_key().await;

        match prev_dir {
            Some(v) => std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", v),
            None => std::env::remove_var("OXIDEAN_SSH_HOST_KEY_DIR"),
        }
        match prev_env {
            Some(v) => std::env::set_var("OXIDEAN_ENV", v),
            None => std::env::remove_var("OXIDEAN_ENV"),
        }
        match prev_key {
            Some(v) => std::env::set_var(WEB_FLOW_PRIVATE_KEY_ENV, v),
            None => std::env::remove_var(WEB_FLOW_PRIVATE_KEY_ENV),
        }

        let p = res.expect("preview env should auto-generate");
        assert_eq!(p, dir.path().join(WEB_FLOW_KEY_BASENAME));
        let pub_line = std::fs::read_to_string(dir.path().join("web-flow.pub")).expect("read pub");
        assert!(pub_line.starts_with("ssh-ed25519 "), "pub: {pub_line}");
    }
}
