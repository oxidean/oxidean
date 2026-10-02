//! Parse SSH `exec` pack commands, ACL, and spawn system git pack helpers (no shell).

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context, Poll};

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
            }
        }
        PackAction::Push => {
            if !meets(capability, Capability::Write) {
                return AuthzDecision::Deny {
                    message: "ERROR: Permission denied to this repository.\n".into(),
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
            }
        }
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
        // Dropping ChildStdin closes the pipe — `receive-pack` waits for stdin
        // EOF before finalizing updates, and `shutdown()` alone does not
        // deliver it on tokio (the fd closes on drop). Without this, a client
        // that leaves the channel open deadlocks the join below, and the
        // PullRefGate's truncated-EOF deny cannot terminate the child.
        drop(child_stdin);
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

// --- refs/pull/* push gate (API-06) ------------------------------------------

/// Upper bound on bytes buffered while scanning the receive-pack command
/// prefix. Larger pushes (thousands of ref updates) fall back to pass-through;
/// the installed `hooks/update` remains the backstop where wired (D-PKG-01).
const PULL_REF_SCAN_MAX: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PullRefScan {
    /// Still consuming the pkt-line command section.
    Scanning,
    /// Command section complete (or unparseable) — pass everything through.
    Open,
    /// A command targets `refs/pull/*` — present EOF to receive-pack.
    Forbidden,
}

/// [`tokio::io::AsyncRead`] wrapper for `receive-pack` client input that denies
/// pushes to the synthesized `refs/pull/*` namespace (API-06).
///
/// The first bytes of a receive-pack request are pkt-line commands
/// (`<old> <new> <refname>`). The gate buffers and scans them transparently:
/// once the command list ends (flush-pkt) it stops scanning and passes the rest
/// of the stream (pack data) straight through. When a command targets
/// `refs/pull/` it returns EOF instead, so receive-pack aborts before git ever
/// applies a ref update — git only writes refs after the complete command list
/// and pack are read, so the truncation is always safe.
///
/// Bytes already forwarded before detection may contain a truncated final
/// command line — receive-pack treats that EOF as a client abort and writes
/// nothing. [`PullRefGate::forbidden_ref`] reports the denied refname so the
/// caller can emit a human-readable message on the stderr channel.
pub struct PullRefGate<R> {
    inner: R,
    /// Bytes read from `inner` but not yet yielded to the child.
    pending: VecDeque<u8>,
    /// Everything read while [`PullRefScan::Scanning`] — the parse source.
    scan: Vec<u8>,
    /// Pkt-line cursor into `scan`.
    cursor: usize,
    state: PullRefScan,
    forbidden: Option<String>,
}

impl<R> PullRefGate<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            pending: VecDeque::new(),
            scan: Vec::new(),
            cursor: 0,
            state: PullRefScan::Scanning,
            forbidden: None,
        }
    }

    /// The `refs/pull/*` target that triggered the deny, once detected.
    pub fn forbidden_ref(&self) -> Option<&str> {
        self.forbidden.as_deref()
    }

    fn advance_scan(&mut self) {
        if self.state != PullRefScan::Scanning {
            return;
        }
        loop {
            let rest = &self.scan[self.cursor..];
            if rest.len() < 4 {
                break; // incomplete pkt header — wait for more bytes
            }
            let len = match std::str::from_utf8(&rest[..4])
                .ok()
                .and_then(|h| usize::from_str_radix(h, 16).ok())
            {
                Some(l) => l,
                // Not a pkt-line stream (e.g. raw pack) — pass through.
                None => {
                    self.state = PullRefScan::Open;
                    break;
                }
            };
            if len == 0 {
                // flush-pkt: end of the command list.
                self.state = PullRefScan::Open;
                break;
            }
            if len == 1 {
                // delim-pkt (push-options) — command list already ended.
                self.state = PullRefScan::Open;
                break;
            }
            if len < 4 || self.cursor + len > self.scan.len() {
                if len >= 4 {
                    break; // incomplete pkt payload — wait for more bytes
                }
                self.state = PullRefScan::Open; // malformed — pass through
                break;
            }
            let pkt = &self.scan[self.cursor + 4..self.cursor + len];
            self.cursor += len;
            let line = String::from_utf8_lossy(pkt);
            let line = line.split('\0').next().unwrap_or("").trim();
            let parts: Vec<&str> = line.split_whitespace().collect();
            // Command lines are `<old-sha> <new-sha> <refname>`; other lines
            // (push-options, commands we don't recognize) are skipped.
            if parts.len() >= 3 && parts[2].starts_with("refs/") {
                if crate::pull::refs::is_pull_ref(parts[2]) {
                    self.forbidden = Some(parts[2].to_string());
                    self.state = PullRefScan::Forbidden;
                    return;
                }
            }
        }
        if self.scan.len() > PULL_REF_SCAN_MAX {
            self.state = PullRefScan::Open;
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for PullRefGate<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.state == PullRefScan::Forbidden {
            // EOF — receive-pack aborts without applying any update.
            return Poll::Ready(Ok(()));
        }
        if self.pending.is_empty() {
            if self.state == PullRefScan::Open {
                return Pin::new(&mut self.inner).poll_read(cx, buf);
            }
            let mut tmp = [0u8; 8192];
            let mut rb = tokio::io::ReadBuf::new(&mut tmp);
            match Pin::new(&mut self.inner).poll_read(cx, &mut rb) {
                Poll::Ready(Ok(())) => {
                    let filled = rb.filled();
                    if filled.is_empty() {
                        // Client EOF mid-scan — nothing more can arrive.
                        self.state = PullRefScan::Open;
                        return Poll::Ready(Ok(()));
                    }
                    self.pending.extend(filled.iter().copied());
                    self.scan.extend_from_slice(filled);
                    self.advance_scan();
                }
                other => return other,
            }
        }
        if self.state == PullRefScan::Forbidden {
            self.pending.clear();
            self.scan.clear();
            return Poll::Ready(Ok(()));
        }
        let n = buf.remaining().min(self.pending.len());
        if n == 0 {
            // Caller passed a zero-capacity buffer — a no-op read.
            return Poll::Ready(Ok(()));
        }
        let chunk: Vec<u8> = self.pending.drain(..n).collect();
        buf.put_slice(&chunk);
        Poll::Ready(Ok(()))
    }
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

    /// Build a receive-pack command stream: `cmd` lines as pkt-lines + flush + pack.
    fn cmd_stream(cmds: &[&str], pack: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for c in cmds {
            let line = format!("{c}\n");
            out.extend_from_slice(format!("{:04x}", line.len() + 4).as_bytes());
            out.extend_from_slice(line.as_bytes());
        }
        out.extend_from_slice(b"0000");
        out.extend_from_slice(pack);
        out
    }

    #[tokio::test]
    async fn pull_ref_gate_denies_pull_namespace_eof() {
        use tokio::io::AsyncReadExt;
        let sha_a = "a".repeat(40);
        let sha_b = "b".repeat(40);
        let stream = cmd_stream(
            &[
                &format!("{sha_a} {sha_b} refs/heads/topic"),
                &format!("{sha_a} {sha_b} refs/pull/1/head"),
            ],
            b"PACKDATA",
        );
        let mut gate = PullRefGate::new(std::io::Cursor::new(stream));
        let mut out = Vec::new();
        gate.read_to_end(&mut out).await.unwrap();
        assert_eq!(gate.forbidden_ref(), Some("refs/pull/1/head"));
        // The forbidden command (and anything after) never reaches git.
        assert!(
            !String::from_utf8_lossy(&out).contains("refs/pull/1/head"),
            "denied ref must not be forwarded"
        );
        assert!(
            !String::from_utf8_lossy(&out).contains("PACKDATA"),
            "pack bytes must not be forwarded after a denied command"
        );
    }

    #[tokio::test]
    async fn pull_ref_gate_passes_benign_commands() {
        use tokio::io::AsyncReadExt;
        let sha_a = "a".repeat(40);
        let sha_b = "b".repeat(40);
        let stream = cmd_stream(
            &[&format!("{sha_a} {sha_b} refs/heads/topic")],
            b"PACKDATA-PAYLOAD",
        );
        let expect = stream.clone();
        let mut gate = PullRefGate::new(std::io::Cursor::new(stream));
        let mut out = Vec::new();
        gate.read_to_end(&mut out).await.unwrap();
        assert_eq!(gate.forbidden_ref(), None);
        assert_eq!(out, expect, "benign stream must pass through unchanged");
    }

    #[tokio::test]
    async fn pull_ref_gate_detects_refname_split_across_reads() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        // Drive the gate through a duplex channel so we can feed it one byte at
        // a time — the parser must buffer until the full pkt-line arrives.
        let (mut tx, rx) = tokio::io::duplex(64);
        let sha_a = "a".repeat(40);
        let sha_b = "b".repeat(40);
        let stream = cmd_stream(&[&format!("{sha_a} {sha_b} refs/pull/9/merge")], b"PACK");
        let writer = tokio::spawn(async move {
            for b in stream.chunks(1) {
                tx.write_all(b).await.unwrap();
            }
            drop(tx);
        });
        let mut gate = PullRefGate::new(rx);
        let mut out = Vec::new();
        gate.read_to_end(&mut out).await.unwrap();
        writer.await.unwrap();
        assert_eq!(gate.forbidden_ref(), Some("refs/pull/9/merge"));
        assert!(
            !String::from_utf8_lossy(&out).contains("refs/pull/9/merge"),
            "split pkt-line must still be denied"
        );
    }

    #[tokio::test]
    async fn pull_ref_gate_against_real_receive_pack_exits() {
        // End-to-end at the pack layer: spawn `git receive-pack` with the gate
        // on stdin, send one refs/pull/* command, assert the child exits
        // without applying the ref.
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("r.git");
        let st = std::process::Command::new("git")
            .args(["init", "--bare"])
            .arg(&bare)
            .status()
            .unwrap();
        assert!(st.success());
        let sha = "a".repeat(40);
        let zero = "0".repeat(40);
        let cmd = format!("{zero} {sha} refs/pull/1/head\n");
        let stream = format!("{:04x}{cmd}0000", cmd.len() + 4).into_bytes();
        let mut gate = PullRefGate::new(std::io::Cursor::new(stream));
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            run_pack_command("receive-pack", &bare, &mut gate, &mut out, &mut err, None),
        )
        .await
        .expect("receive-pack must not hang")
        .expect("run");
        assert_ne!(code, 0, "receive-pack must fail on truncated stdin");
        assert_eq!(gate.forbidden_ref(), Some("refs/pull/1/head"));
        // Nothing applied.
        let chk = std::process::Command::new("git")
            .args(["-C"])
            .arg(&bare)
            .args(["rev-parse", "--verify", "refs/pull/1/head"])
            .output()
            .unwrap();
        assert!(
            !chk.status.success(),
            "refs/pull/1/head must not be created by the denied push"
        );
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
