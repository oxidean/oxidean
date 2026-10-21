//! SSH host key load/generate (Ed25519) under `OXIDEAN_SSH_HOST_KEY_DIR`.

use std::path::Path;

use russh::keys::{Algorithm, PrivateKey};

pub use oxidean_api_core::ssh::host_keys::host_key_dir;

const HOST_KEY_FILE: &str = "ssh_host_ed25519_key";

/// Load existing Ed25519 host key or generate + persist one.
pub async fn load_or_generate(dir: &Path) -> Result<PrivateKey, String> {
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("create host key dir: {e}"))?;
    let path = dir.join(HOST_KEY_FILE);
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| format!("read host key: {e}"))?;
        let pem = String::from_utf8(bytes).map_err(|e| format!("host key utf8: {e}"))?;
        return PrivateKey::from_openssh(&pem).map_err(|e| format!("parse host key: {e}"));
    }

    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
        .map_err(|e| format!("generate host key: {e}"))?;
    let pem = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .map_err(|e| format!("encode host key: {e}"))?;
    tokio::fs::write(&path, pem.as_bytes())
        .await
        .map_err(|e| format!("write host key: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).await;
    }
    Ok(key)
}
