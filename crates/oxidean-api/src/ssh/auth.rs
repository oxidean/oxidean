//! SSH pubkey → fingerprint → registered user (D-SSH-03).

use oxidean_db::{Database, DeployKeyRow, SshKeyRow};
use ssh_key::{HashAlg, PublicKey};

/// OpenSSH-display fingerprint (`SHA256:…`) for a russh/ssh-key public key.
pub fn fingerprint_of(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Look up a registered SSH key by fingerprint (authentication usage only).
/// Rejects keys belonging to soft-banned users.
pub async fn find_registered_key(
    db: &Database,
    key: &PublicKey,
) -> Result<Option<SshKeyRow>, String> {
    let fp = fingerprint_of(key);
    match db.find_ssh_key_by_fingerprint(&fp).await? {
        Some(row) if row.can_authenticate => {
            match db.find_user_by_id(&row.user_id).await? {
                Some(u) if u.banned_at.is_some() => Ok(None),
                Some(_) => Ok(Some(row)),
                None => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

/// Look up a repo deploy key by fingerprint (GIT-23).
///
/// Any attached row accepts the handshake — the repo binding (`repo_id`) is
/// enforced per pack exec via `authorize_deploy_key_pack`, because SSH auth
/// completes before the target repository is known. Deploy keys never resolve
/// to an account identity: they are transport-only credentials (no session,
/// no RPC, no web access).
pub async fn find_deploy_key(
    db: &Database,
    key: &PublicKey,
) -> Result<Option<DeployKeyRow>, String> {
    let fp = fingerprint_of(key);
    db.find_deploy_key_by_fingerprint(&fp).await
}
