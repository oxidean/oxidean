//! SSH host key directory resolution (`OXIDEAN_SSH_HOST_KEY_DIR`) — key
//! load/generate itself stays in the `oxidean-api` shell (russh).

use std::path::PathBuf;

/// Resolve host-key directory (`OXIDEAN_SSH_HOST_KEY_DIR`, default `var/ssh`).
pub fn host_key_dir() -> PathBuf {
    std::env::var("OXIDEAN_SSH_HOST_KEY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/ssh"))
}
