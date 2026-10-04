//! Parse SSH `exec` pack commands, ACL, and spawn system git pack helpers (no shell).

use std::path::{Path, PathBuf};
use std::process::Stdio;

use oxidean_db::Database;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::process::Command;

use crate::git::bare_repo_path;
use crate::repo::{
    effective_capability, is_private_visibility, lookup_repo_row_or_redirect, meets, Capability,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackCommand {
    UploadPack { owner: String, name: String },
    ReceivePack { owner: String, name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackAction {
    Fetch,
    Push,
}

#[derive(Debug)]
pub enum AuthzDecision {
    Allow {
        bare: PathBuf,
        repo_id: String,
        owner_slug: String,
        repo_name: String,
        is_push: bool,
        /// Forge capability for `OXIDEAN_ACTOR_CAPABILITY` on receive-pack (D-PKG-01).
        capability: Option<Capability>,
        /// User the push is attributed to in post-receive hooks (webhooks,
        /// PR sync, Actions). Account path: the authenticated user. Deploy-key
        /// path (GIT-23): the admin who attached the authorizing key row.
        actor_user_id: String,
    },
    Deny { message: String },
}

/// Parse `git-upload-pack 'owner/name.git'` / `git-receive-pack "owner/name.git"`.
pub fn parse_pack_exec(data: &[u8]) -> Option<PackCommand> {
    let s = std::str::from_utf8(data).ok()?.trim();
    let (kind, rest) = if let Some(r) = s.strip_prefix("git-upload-pack") {
        ("upload", r.trim())
    } else if let Some(r) = s.strip_prefix("git-receive-pack") {
        ("receive", r.trim())
    } else {
        return None;
    };

    let path = strip_quotes(rest)?;
    let path = path.trim_start_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return None;
    }
    match kind {
        "upload" => Some(PackCommand::UploadPack {
            owner: owner.to_string(),
            name: name.to_string(),
        }),
        "receive" => Some(PackCommand::ReceivePack {
            owner: owner.to_string(),
            name: name.to_string(),
        }),
        _ => None,
    }
}

fn strip_quotes(s: &str) -> Option<&str> {
    let s = s.trim();
    if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
        Some(&s[1..s.len() - 1])
    } else if !s.is_empty() {
        Some(s)
    } else {
        None
    }
}

pub fn resolve_bare(repos_dir: &Path, owner: &str, name: &str) -> Result<PathBuf, String> {
    bare_repo_path(repos_dir, owner, name).map_err(|e| e.message)
}

/// Authorize pack access using the same owner/visibility helpers as Smart HTTP (D-SSH-04).
pub async fn authorize_pack(
    db: &Database,
    repos_dir: &Path,
    caller_user_id: &str,
    cmd: &PackCommand,
) -> AuthzDecision {
    match db.find_user_by_id(caller_user_id).await {
        Ok(Some(u)) if u.banned_at.is_some() => {
            return AuthzDecision::Deny {
                message: "ERROR: Permission denied to this repository.\n".into(),
            };
        }
        Ok(Some(_)) => {}
        _ => {
            return AuthzDecision::Deny {
                message: "ERROR: Permission denied to this repository.\n".into(),
            };
        }
    }

    let (owner, name, action) = match cmd {
        PackCommand::UploadPack { owner, name } => (owner.as_str(), name.as_str(), PackAction::Fetch),
        PackCommand::ReceivePack { owner, name } => (owner.as_str(), name.as_str(), PackAction::Push),
    };

    let pair = match lookup_repo_row_or_redirect(db, owner, name).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return AuthzDecision::Deny {
                message: "ERROR: Repository not found.\n".into(),
            };
        }
        Err(_) => {
            return AuthzDecision::Deny {
                message: "ERROR: Internal error.\n".into(),
            };
        }
    };
    let (row, owner_ref) = pair;
    let disk_owner = owner_ref.slug();
    let disk_name = row.name.as_str();

    let bare = match resolve_bare(repos_dir, disk_owner, disk_name) {
        Ok(p) if p.exists() => p,
        _ => {
            return AuthzDecision::Deny {
                message: "ERROR: Repository not found.\n".into(),
            };
        }
    };

    // Match Smart HTTP: Capability coalesce (personal owner / org role / collaborator / public).
    let capability = match effective_capability(db, Some(caller_user_id), &row, &owner_ref).await {
        Ok(c) => c,
        Err(_) => {
            return AuthzDecision::Deny {
                message: "ERROR: Internal error.\n".into(),
            };
        }
    };

    match action {
        PackAction::Fetch => {
            if is_private_visibility(&row.visibility) && !meets(capability, Capability::Read) {
                return AuthzDecision::Deny {
                    message: "ERROR: Permission denied to this repository.\n".into(),
                };
            }
            AuthzDecision::Allow {
                bare,
                repo_id: row.id.clone(),
                owner_slug: disk_owner.to_string(),
                repo_name: disk_name.to_string(),
                is_push: false,
                capability,
                actor_user_id: caller_user_id.to_string(),
            }
        }
        PackAction::Push => {
            if !meets(capability, Capability::Write) {
                return AuthzDecision::Deny {
                    message: "ERROR: Permission denied to this repository.\n".into(),
                };
            }
            // GIT-20: archived repositories are read-only — fetch stays open, push denied.
            if row.archived {
                return AuthzDecision::Deny {
                    message: "ERROR: Repository is archived (read-only).\n".into(),
                };
            }
            let caller = match db.find_user_by_id(caller_user_id).await {
                Ok(Some(u)) if u.banned_at.is_none() => u,
                _ => {
                    return AuthzDecision::Deny {
                        message: "ERROR: Permission denied to this repository.\n".into(),
                    };
                }
            };
            if caller
                .email_verified_at
                .as_deref()
                .filter(|s| !s.is_empty())
                .is_none()
            {
                return AuthzDecision::Deny {
                    message: "ERROR: Email verification required to push.\n".into(),
                };
            }
            AuthzDecision::Allow {
                bare,
                repo_id: row.id.clone(),
                owner_slug: disk_owner.to_string(),
                repo_name: disk_name.to_string(),
                is_push: true,
                capability,
                actor_user_id: caller_user_id.to_string(),
            }
        }
    }
}

/// Authorize pack access for a deploy-key principal (GIT-23).
///
/// The fingerprint authenticated at handshake time; the repo binding is
/// enforced here per exec because auth completes before the target repo is
/// known. A deploy key grants exactly the repos it is attached to — nothing
/// else (not even public read on other repos) — and never a user identity:
/// no email-verified / collaborator / org checks apply. `can_write` gates
/// receive-pack; upload-pack is allowed for any attached key. Push still runs
/// the receive-pack protection env (`OXIDEAN_ACTOR_CAPABILITY=write`), so
/// branch protection hooks apply unchanged.
///
/// `actor_user_id` on Allow is the attaching admin (`created_by`) — used for
/// post-push webhook/PR-sync/Actions attribution only.
pub async fn authorize_deploy_key_pack(
    db: &Database,
    repos_dir: &Path,
    fingerprint: &str,
    cmd: &PackCommand,
    client_ip: &str,
) -> AuthzDecision {
    let (owner, name, action) = match cmd {
        PackCommand::UploadPack { owner, name } => (owner.as_str(), name.as_str(), PackAction::Fetch),
        PackCommand::ReceivePack { owner, name } => (owner.as_str(), name.as_str(), PackAction::Push),
    };

    let pair = match lookup_repo_row_or_redirect(db, owner, name).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return AuthzDecision::Deny {
                message: "ERROR: Repository not found.\n".into(),
            };
        }
        Err(_) => {
            return AuthzDecision::Deny {
                message: "ERROR: Internal error.\n".into(),
            };
        }
    };
    let (row, owner_ref) = pair;
    let disk_owner = owner_ref.slug();
    let disk_name = row.name.as_str();

    let bare = match resolve_bare(repos_dir, disk_owner, disk_name) {
        Ok(p) if p.exists() => p,
        _ => {
            return AuthzDecision::Deny {
                message: "ERROR: Repository not found.\n".into(),
            };
        }
    };

    // Fresh per-exec scope check — a key detached between auth and exec,
    // or attached to a different repo under the same fingerprint, must not pass.
    let key = match db.find_deploy_key_for_repo(&row.id, fingerprint).await {
        Ok(Some(k)) => k,
        Ok(None) => {
            return AuthzDecision::Deny {
                message: "ERROR: Permission denied to this repository.\n".into(),
            };
        }
        Err(_) => {
            return AuthzDecision::Deny {
                message: "ERROR: Internal error.\n".into(),
            };
        }
    };

    if action == PackAction::Push && !key.can_write {
        return AuthzDecision::Deny {
            message: "ERROR: Permission denied to this repository.\n".into(),
        };
    }

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let _ = db
        .touch_deploy_key_last_used(&key.id, &now, Some(client_ip))
        .await;

    AuthzDecision::Allow {
        bare,
        repo_id: row.id.clone(),
        owner_slug: disk_owner.to_string(),
        repo_name: disk_name.to_string(),
        is_push: action == PackAction::Push,
        capability: Some(if key.can_write {
            Capability::Write
        } else {
            Capability::Read
        }),
        actor_user_id: key.created_by.clone(),
    }
}

/// Spawn `git-upload-pack` or `git-receive-pack` with argv only (no shell) and bridge stdio.
///
/// When `protection_env` is `Some` (receive-pack), inject helper/DB/repos/capability/ENV
/// so bare-repo update hooks can evaluate branch protection (D-PKG-01). Upload-pack
/// callers pass `None`.
pub async fn run_pack_command<R, W, E>(
    program: &str,
    bare: &Path,
    mut stdin_rx: R,
    mut stdout_tx: W,
    mut stderr_tx: E,
    protection_env: Option<&[(String, String)]>,
) -> Result<i32, String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    E: AsyncWrite + Unpin,
{
    let mut cmd = Command::new("git");
    cmd.arg(program)
        .arg(bare)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(pairs) = protection_env {
        for (k, v) in pairs {
            cmd.env(k, v);
        }
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn git {program}: {e}"))?;

    let mut child_stdin = child.stdin.take().ok_or("missing stdin")?;
    let mut child_stdout = child.stdout.take().ok_or("missing stdout")?;
    let mut child_stderr = child.stderr.take().ok_or("missing stderr")?;

    let stdin_pipe = async {
        let _ = tokio::io::copy(&mut stdin_rx, &mut child_stdin).await;
        let _ = child_stdin.shutdown().await;
    };
    let stdout_pipe = async {
        let _ = tokio::io::copy(&mut child_stdout, &mut stdout_tx).await;
        let _ = stdout_tx.flush().await;
    };
    let stderr_pipe = async {
        let _ = tokio::io::copy(&mut child_stderr, &mut stderr_tx).await;
        let _ = stderr_tx.flush().await;
    };

    tokio::join!(stdin_pipe, stdout_pipe, stderr_pipe);

    let status = child
        .wait()
        .await
        .map_err(|e| format!("wait git {program}: {e}"))?;
    Ok(status.code().unwrap_or(1))
}

/// Write a denial message to the channel as git stderr (extended data 1) then close.
pub async fn write_git_stderr_deny(
    handle: &russh::server::Handle,
    channel: russh::ChannelId,
    message: &str,
) {
    let _ = handle
        .extended_data(channel, 1, message.as_bytes().to_vec())
        .await;
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;
}

/// Map forge capability to `OXIDEAN_ACTOR_CAPABILITY` (D-PKG-01 / Smart HTTP).
pub fn capability_env_label(capability: Option<Capability>) -> &'static str {
    match capability {
        Some(Capability::Admin) => "admin",
        Some(Capability::Write) => "write",
        Some(Capability::Read) | None => "read",
    }
}

/// Env pairs injected into `git receive-pack` for protection hooks (D-PKG-01).
/// Upload-pack must not require these — callers pass `None` into [`run_pack_command`].
pub fn receive_pack_protection_env(
    database_url: &str,
    repos_dir: &Path,
    actor_capability: &str,
    helper: Option<&str>,
    oxidean_env: Option<&str>,
) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(5);
    out.push((
        "OXIDEAN_DATABASE_URL".into(),
        database_url.to_string(),
    ));
    out.push((
        "OXIDEAN_REPOS_DIR".into(),
        repos_dir.display().to_string(),
    ));
    out.push((
        "OXIDEAN_ACTOR_CAPABILITY".into(),
        actor_capability.to_string(),
    ));
    if let Some(h) = helper.filter(|s| !s.is_empty()) {
        out.push(("OXIDEAN_PROTECTION_HELPER".into(), h.to_string()));
    }
    if let Some(env_name) = oxidean_env.filter(|s| !s.is_empty()) {
        out.push(("OXIDEAN_ENV".into(), env_name.to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn capability_env_label_maps_admin_write_read() {
        assert_eq!(
            capability_env_label(Some(Capability::Admin)),
            "admin",
            "Admin must map to OXIDEAN_ACTOR_CAPABILITY=admin (D-PKG-01)"
        );
        assert_eq!(capability_env_label(Some(Capability::Write)), "write");
        assert_eq!(capability_env_label(Some(Capability::Read)), "read");
        assert_eq!(capability_env_label(None), "read");
    }

    #[test]
    fn receive_pack_protection_env_sets_all_five_keys() {
        let vars = receive_pack_protection_env(
            "sqlite:/tmp/oxidean.db",
            Path::new("/var/repos"),
            "admin",
            Some("/usr/local/bin/oxidean-protection-hook"),
            Some("compose"),
        );
        let map: HashMap<&str, &str> = vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            map.get("OXIDEAN_PROTECTION_HELPER").copied(),
            Some("/usr/local/bin/oxidean-protection-hook"),
            "receive-pack must set OXIDEAN_PROTECTION_HELPER"
        );
        assert_eq!(
            map.get("OXIDEAN_DATABASE_URL").copied(),
            Some("sqlite:/tmp/oxidean.db")
        );
        assert_eq!(
            map.get("OXIDEAN_REPOS_DIR").copied(),
            Some("/var/repos")
        );
        assert_eq!(
            map.get("OXIDEAN_ACTOR_CAPABILITY").copied(),
            Some("admin")
        );
        assert_eq!(map.get("OXIDEAN_ENV").copied(), Some("compose"));
    }

    #[test]
    fn receive_pack_protection_env_omits_helper_when_none() {
        let vars = receive_pack_protection_env(
            "postgres://x",
            Path::new("/repos"),
            "write",
            None,
            Some("production"),
        );
        let map: HashMap<&str, &str> = vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert!(
            !map.contains_key("OXIDEAN_PROTECTION_HELPER"),
            "helper key omitted when unresolved (mirror Smart HTTP)"
        );
        assert_eq!(map.get("OXIDEAN_ACTOR_CAPABILITY").copied(), Some("write"));
        assert_eq!(map.get("OXIDEAN_ENV").copied(), Some("production"));
        assert_eq!(map.get("OXIDEAN_DATABASE_URL").copied(), Some("postgres://x"));
        assert_eq!(map.get("OXIDEAN_REPOS_DIR").copied(), Some("/repos"));
    }

    #[test]
    fn authz_allow_carries_capability_for_actor_env() {
        // Structural contract: Allow must expose capability for OXIDEAN_ACTOR_CAPABILITY.
        let allow = AuthzDecision::Allow {
            bare: PathBuf::from("/tmp/r.git"),
            repo_id: "rid".into(),
            owner_slug: "o".into(),
            repo_name: "n".into(),
            is_push: true,
            capability: Some(Capability::Admin),
            actor_user_id: "u1".into(),
        };
        match allow {
            AuthzDecision::Allow { capability, .. } => {
                assert_eq!(
                    capability_env_label(capability),
                    "admin",
                    "Allow.capability must feed actor capability env"
                );
            }
            AuthzDecision::Deny { .. } => panic!("expected Allow"),
        }
    }
}
