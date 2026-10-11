//! CLI-backed [`GitBackend`] via `tokio::process::Command` argv arrays (never `sh -c`).

use std::path::{Component, Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;

use crate::backend::{
    validate_remote_url, ArchiveFormat, BlameFile, BlameLine, CommitDetail, CommitSummary,
    CommitTimesResult, ContributorScan, ContributorStat, ContributorSummary, DiffFile, DiffResult,
    FileChange, FilesCommit, GitBackend, GitError, GitRef, GrepHit, GrepResult, RemoteAuthKind,
    RemoteCredentials, SizedBlobEntry, TreeEntry, TreeEntryKind, ARCHIVE_TIMEOUT,
    BLAME_SOFT_MAX_LINES, DIFF_SOFT_MAX_BYTES, FORGE_NOREPLY_EMAIL,
};

/// System `git` CLI adapter (D-32). Only backend registered in Phase 7.
#[derive(Debug, Default, Clone)]
pub struct CliGitBackend;

impl CliGitBackend {
    pub fn new() -> Self {
        Self
    }
}

/// Install bare-repo `hooks/update` for ref protection — branches (Phase 13 /
/// D-19) and tags (GIT-21). Idempotent — overwrites with the known-good script.
pub async fn install_protection_hooks(bare: &Path) -> Result<(), GitError> {
    let hooks = bare.join("hooks");
    tokio::fs::create_dir_all(&hooks).await?;
    let update = hooks.join("update");
    // D-PKG-02: fail-closed when helper missing in production|cloud
    // (mirrors webhook deliver.rs env signal); fail-open for compose/dev.
    let script = r#"#!/bin/sh
# Oxidean ref protection update hook (Phase 13 / D-19; D-PKG-02; GIT-21)
refname="$1"
oldrev="$2"
newrev="$3"
helper="${OXIDEAN_PROTECTION_HELPER:-}"
if [ -z "$helper" ] || [ ! -x "$helper" ]; then
  env_name="${OXIDEAN_ENV:-development}"
  case "$env_name" in
    production|cloud)
      echo "oxidean: protection helper missing or not executable (OXIDEAN_ENV=$env_name)" >&2
      exit 1
      ;;
    *)
      exit 0
      ;;
  esac
fi
exec "$helper" update "$refname" "$oldrev" "$newrev"
"#;
    tokio::fs::write(&update, script).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&update).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&update, perms).await?;
    }
    Ok(())
}

/// Ensure protection hooks exist (lazy reconcile).
pub async fn reconcile_protection_hooks(bare: &Path) -> Result<(), GitError> {
    let update = bare.join("hooks").join("update");
    if tokio::fs::metadata(&update).await.is_ok() {
        return Ok(());
    }
    install_protection_hooks(bare).await
}

/// One commit a push introduces, with identity + signature verdict (GIT-22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushedCommit {
    pub sha: String,
    pub committer_email: String,
    pub author_email: String,
    /// `none` | `valid` | `invalid` | `unknown` (from `%G?` + optional keyring).
    pub signature_status: String,
    /// `ssh` | `gpg` | empty when unsigned / unknown.
    pub signature_kind: String,
}

/// `git log <new_sha> --not <exclude>` — commits the push introduces.
///
/// `exclude` selection (GIT-22):
/// - updates / force-pushes (`old_sha` non-zero): `old..new` — commits newly
///   reachable on *this* branch, even if they already exist on another ref;
/// - creates (`old_sha` zero/empty): `--all` — commits no existing ref reaches,
///   so branching off already-present history does not re-check it.
///
/// `allowed_signers` / `gpg_home` feed `%G?` verification exactly like
/// [`CliGitBackend::log`] (allowedSignersFile + `GNUPGHOME` /
/// `gpg.trustModel=always`). Pass `None` for a no-keyring probe (emails +
/// unsigned detection only).
///
/// Runs inside `hooks/update` too — quarantine `GIT_OBJECT_DIRECTORY` env is
/// inherited by the spawned `git` process.
pub async fn pushed_commits(
    repo: &Path,
    old_sha: &str,
    new_sha: &str,
    allowed_signers: Option<&Path>,
    gpg_home: Option<&Path>,
) -> Result<Vec<PushedCommit>, GitError> {
    let new_sha = validate_treeish(new_sha)?;
    let repo_s = repo_str(repo)?;

    let mut args: Vec<String> = vec!["-C".into(), repo_s.into()];
    let mut extra_env: Vec<(String, String)> = Vec::new();
    push_signature_verify_config(&mut args, &mut extra_env, allowed_signers, gpg_home)?;
    args.push("log".into());
    args.push("--format=%H%x00%ce%x00%ae%x00%G?".into());
    args.push(new_sha.to_string());
    args.push("--not".into());
    let old_sha = old_sha.trim();
    if old_sha.is_empty() || old_sha.bytes().all(|b| b == b'0') {
        args.push("--all".into());
    } else {
        args.push(validate_treeish(old_sha)?.to_string());
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let env_refs: Vec<(&str, &str)> = extra_env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let stdout = run_git_stdout_env(&arg_refs, &env_refs).await?;
    let text = String::from_utf8_lossy(&stdout);

    let mut out = Vec::new();
    for record in text.split('\n') {
        let record = record.trim_end_matches('\r');
        if record.is_empty() {
            continue;
        }
        let parts: Vec<&str> = record.split('\0').collect();
        if parts.len() < 4 {
            continue;
        }
        let sha = parts[0].trim();
        if sha.is_empty() {
            continue;
        }
        let signature_status = map_signature_status(parts[3]);
        let mut signature_kind = String::new();
        if signature_status != "none" {
            // Best-effort kind from the raw commit object (same as `log`).
            if let Ok(raw) = run_git_stdout(&["-C", repo_s, "cat-file", "-p", sha]).await {
                signature_kind = signature_kind_from_commit(&String::from_utf8_lossy(&raw));
            }
        }
        out.push(PushedCommit {
            sha: sha.to_string(),
            committer_email: parts[1].trim().to_string(),
            author_email: parts[2].trim().to_string(),
            signature_status,
            signature_kind,
        });
    }
    Ok(out)
}

/// Disk usage of a repository directory in bytes — `du`-equivalent without
/// shelling out (GIT-25). Sums apparent file sizes (`Metadata::len`), counts a
/// hardlinked inode once per walk on unix, and never follows symlinks. Runs
/// inside `spawn_blocking` so callers stay on the async scheduler.
///
/// Called from `hooks/update` while receive-pack quarantine objects still live
/// under `GIT_DIR`, so the result includes the incoming pack — callers can
/// treat it as the projected post-push size.
pub async fn repo_disk_usage(dir: &Path) -> Result<u64, GitError> {
    let dir = dir.to_path_buf();
    tokio::task::spawn_blocking(move || dir_disk_usage_sync(&dir))
        .await
        .map_err(|e| GitError::Process(format!("disk usage task join: {e}")))?
}

fn dir_disk_usage_sync(dir: &Path) -> Result<u64, GitError> {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    // (dev, ino) dedup so hardlinked objects (local clones / alternates) count once.
    #[cfg(unix)]
    let mut seen_inodes = std::collections::HashSet::new();
    while let Some(path) = stack.pop() {
        let entries = match std::fs::read_dir(&path) {
            Ok(e) => e,
            Err(e) if path == *dir => return Err(GitError::Io(e)),
            // Unreadable subdir (race / permissions): skip, do not fail the walk.
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            // file_type does not traverse symlinks: links are counted (their own
            // dirent size via metadata below) but never followed.
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            // symlink_metadata so symlinks count their own dirent size and never
            // drag in (or double-count via nlink>1 targets) an outside tree.
            let meta = match std::fs::symlink_metadata(entry.path()) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if ft.is_dir() {
                stack.push(entry.path());
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if meta.nlink() > 1 && !seen_inodes.insert((meta.dev(), meta.ino())) {
                    continue;
                }
            }
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}

async fn run_git(args: &[&str]) -> Result<(), GitError> {
    let _ = run_git_stdout(args).await?;
    Ok(())
}

async fn run_git_with_env(args: &[&str], extra_env: &[(&str, &str)]) -> Result<(), GitError> {
    let _ = run_git_stdout_env(args, extra_env).await?;
    Ok(())
}

/// Env for `git push` into a forge bare repo so `hooks/update` can find the
/// protection helper (D-PKG-01/02). Prefer `OXIDEAN_PROTECTION_HELPER`; else a
/// sibling `oxidean-protection-hook` next to the current executable (API image).
///
/// `actor_capability` lands in `OXIDEAN_ACTOR_CAPABILITY` for the hook.
/// System ref updates (mirror FF / internal sync) pass `"admin"` — they already
/// passed API-layer policy, and production fail-closed hooks would otherwise
/// deny with default capability `read`. User-attributed commits (GIT-19
/// `commit_files`) pass the actor's real capability so a protection rule
/// created between the API-side check and the push still bites.
fn protection_hook_push_env(actor_capability: &str) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(2);
    let helper = std::env::var("OXIDEAN_PROTECTION_HELPER")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::current_exe().ok().and_then(|p| {
                let sibling = p.parent()?.join("oxidean-protection-hook");
                sibling.is_file().then(|| sibling.display().to_string())
            })
        });
    if let Some(h) = helper {
        out.push(("OXIDEAN_PROTECTION_HELPER".into(), h));
    }
    out.push((
        "OXIDEAN_ACTOR_CAPABILITY".into(),
        actor_capability.to_string(),
    ));
    out
}

async fn run_git_stdout(args: &[&str]) -> Result<Vec<u8>, GitError> {
    run_git_stdout_env(args, &[]).await
}

async fn run_git_stdout_env(args: &[&str], extra_env: &[(&str, &str)]) -> Result<Vec<u8>, GitError> {
    // Tests / CI often have no global git identity; env overrides avoid
    // "Author identity unknown" without mutating the runner's ~/.gitconfig.
    let mut cmd = Command::new("git");
    cmd.args(args)
        .env("GIT_AUTHOR_NAME", "Oxidean")
        .env("GIT_AUTHOR_EMAIL", FORGE_NOREPLY_EMAIL)
        .env("GIT_COMMITTER_NAME", "Oxidean")
        .env("GIT_COMMITTER_EMAIL", FORGE_NOREPLY_EMAIL)
        // Never prompt interactively for credentials during mirror ops.
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;

    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}\n{stdout}");
    if looks_like_missing_ssh(&combined) {
        return Err(GitError::Process(
            "OpenSSH client is not installed on this Oxidean API host (install openssh-client / ensure `ssh` is on PATH)".into(),
        ));
    }
    Err(GitError::Process(format!(
        "git {} failed (status {:?}): {}{}",
        args.join(" "),
        output.status.code(),
        stderr.trim(),
        if stdout.trim().is_empty() {
            String::new()
        } else {
            format!(" | {}", stdout.trim())
        }
    )))
}

/// `run_git_stdout_env` variant that pipes `stdin_bytes` to the child —
/// for `hash-object -w --stdin` and `update-index --index-info`, where the
/// payload may contain NULs and cannot ride on argv (GIT-19).
async fn run_git_stdout_env_stdin(
    args: &[&str],
    extra_env: &[(&str, &str)],
    stdin_bytes: &[u8],
) -> Result<Vec<u8>, GitError> {
    use tokio::io::AsyncWriteExt;
    let mut cmd = Command::new("git");
    cmd.args(args)
        .env("GIT_AUTHOR_NAME", "Oxidean")
        .env("GIT_AUTHOR_EMAIL", FORGE_NOREPLY_EMAIL)
        .env("GIT_COMMITTER_NAME", "Oxidean")
        .env("GIT_COMMITTER_EMAIL", FORGE_NOREPLY_EMAIL)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| GitError::Process("failed to open git stdin".into()))?;
    stdin.write_all(stdin_bytes).await.map_err(GitError::Io)?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| GitError::Process(format!("failed to wait on git: {e}")))?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(GitError::Process(format!(
        "git {} failed (status {:?}): {}{}",
        args.join(" "),
        output.status.code(),
        stderr.trim(),
        if stdout.trim().is_empty() {
            String::new()
        } else {
            format!(" | {}", stdout.trim())
        }
    )))
}

fn looks_like_missing_ssh(combined: &str) -> bool {
    let lower = combined.to_ascii_lowercase();
    lower.contains("cannot run ssh")
        || lower.contains("error: cannot run ssh")
        || (lower.contains("ssh:") && lower.contains("no such file or directory"))
}

/// Materialize askpass / SSH key / known_hosts for one outbound URL op.
struct RemoteAuthFiles {
    _dir: tempfile::TempDir,
    env: Vec<(String, String)>,
}

impl RemoteAuthFiles {
    fn prepare(credentials: &RemoteCredentials) -> Result<Self, GitError> {
        let dir = tempfile::tempdir().map_err(GitError::Io)?;
        let mut env = Vec::new();
        match credentials.kind {
            RemoteAuthKind::HttpsToken => {
                let user = credentials.username.as_deref().unwrap_or("git").trim();
                if user.is_empty() {
                    return Err(GitError::InvalidArg("HTTPS username is required".into()));
                }
                if credentials.secret.contains('\0') || credentials.secret.contains('\n') {
                    return Err(GitError::InvalidArg(
                        "HTTPS token contains invalid characters".into(),
                    ));
                }
                let askpass = dir.path().join("askpass.sh");
                // Echo password for any credential prompt; username via URL or
                // GIT_ASKPASS first call — we inject via http.extraHeader instead
                // so askpass only needs the token.
                let script = format!(
                    "#!/bin/sh\necho '{}'\n",
                    credentials.secret.replace('\'', "'\\''")
                );
                std::fs::write(&askpass, script).map_err(GitError::Io)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = std::fs::metadata(&askpass).map_err(GitError::Io)?.permissions();
                    perms.set_mode(0o700);
                    std::fs::set_permissions(&askpass, perms).map_err(GitError::Io)?;
                }
                let auth_b64 = {
                    use std::fmt::Write as _;
                    let raw = format!("{user}:{}", credentials.secret);
                    let mut out = String::new();
                    // Manual base64 (stdlib) — avoid new dependency in oxidean-git.
                    const T: &[u8] =
                        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
                    let bytes = raw.as_bytes();
                    let mut i = 0;
                    while i < bytes.len() {
                        let b0 = bytes[i] as u32;
                        let b1 = if i + 1 < bytes.len() {
                            bytes[i + 1] as u32
                        } else {
                            0
                        };
                        let b2 = if i + 2 < bytes.len() {
                            bytes[i + 2] as u32
                        } else {
                            0
                        };
                        let n = (b0 << 16) | (b1 << 8) | b2;
                        let _ = write!(
                            out,
                            "{}{}{}{}",
                            T[((n >> 18) & 63) as usize] as char,
                            T[((n >> 12) & 63) as usize] as char,
                            if i + 1 < bytes.len() {
                                T[((n >> 6) & 63) as usize] as char
                            } else {
                                '='
                            },
                            if i + 2 < bytes.len() {
                                T[(n & 63) as usize] as char
                            } else {
                                '='
                            }
                        );
                        i += 3;
                    }
                    out
                };
                env.push((
                    "GIT_CONFIG_COUNT".into(),
                    "1".into(),
                ));
                env.push((
                    "GIT_CONFIG_KEY_0".into(),
                    "http.extraHeader".into(),
                ));
                env.push((
                    "GIT_CONFIG_VALUE_0".into(),
                    format!("Authorization: Basic {auth_b64}"),
                ));
                let _ = askpass; // reserved if we switch to ASKPASS later
            }
            RemoteAuthKind::SshKey => {
                let known = credentials.known_hosts.as_deref().unwrap_or("").trim();
                if known.is_empty() {
                    return Err(GitError::InvalidArg(
                        "SSH known_hosts is required for outbound SSH remotes".into(),
                    ));
                }
                if credentials.secret.contains('\0') {
                    return Err(GitError::InvalidArg("SSH private key contains NUL".into()));
                }
                let key_path = dir.path().join("id_mirror");
                let kh_path = dir.path().join("known_hosts");
                let mut key = credentials.secret.clone();
                if !key.ends_with('\n') {
                    key.push('\n');
                }
                std::fs::write(&key_path, key).map_err(GitError::Io)?;
                std::fs::write(&kh_path, format!("{known}\n")).map_err(GitError::Io)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = std::fs::metadata(&key_path).map_err(GitError::Io)?.permissions();
                    perms.set_mode(0o600);
                    std::fs::set_permissions(&key_path, perms).map_err(GitError::Io)?;
                }
                let key_s = key_path
                    .to_str()
                    .ok_or_else(|| GitError::InvalidArg("non-utf8 key path".into()))?;
                let kh_s = kh_path
                    .to_str()
                    .ok_or_else(|| GitError::InvalidArg("non-utf8 known_hosts path".into()))?;
                // IdentitiesOnly + pinned known_hosts; no agent / default keys.
                let ssh_cmd = format!(
                    "ssh -i {key_s} -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile={kh_s} -o GlobalKnownHostsFile=/dev/null"
                );
                env.push(("GIT_SSH_COMMAND".into(), ssh_cmd));
            }
        }
        Ok(Self { _dir: dir, env })
    }

    fn as_pairs(&self) -> Vec<(&str, &str)> {
        self.env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }
}

async fn run_git_remote(
    args: &[&str],
    credentials: &RemoteCredentials,
) -> Result<Vec<u8>, GitError> {
    let files = RemoteAuthFiles::prepare(credentials)?;
    let pairs = files.as_pairs();
    run_git_stdout_env(args, &pairs).await
}

/// Parse `git shortlog -sn -e` output into [`ContributorSummary`] rows
/// (count, name, email), capped at `limit` authors.
fn parse_shortlog_stdout(stdout: &[u8], limit: u32) -> Vec<ContributorSummary> {
    let text = String::from_utf8_lossy(stdout);
    let mut out = Vec::new();
    for line in text.split('\n') {
        if out.len() as u32 >= limit {
            break;
        }
        let line = line.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }
        // "    42\tName <email@x>" or spaces then count then name <email>
        let rest = line.trim_start();
        let (count_s, after) = rest
            .split_once(|c: char| c.is_whitespace())
            .unwrap_or((rest, ""));
        let count: i64 = count_s.trim().parse().unwrap_or(0);
        if count <= 0 {
            continue;
        }
        let after = after.trim();
        let (name, email) = if let Some((n, e)) = after.rsplit_once(" <") {
            let email = e.trim().trim_end_matches('>').trim().to_string();
            (n.trim().to_string(), email)
        } else {
            (after.to_string(), String::new())
        };
        if name.is_empty() {
            continue;
        }
        out.push(ContributorSummary {
            name,
            email,
            commit_count: count,
        });
    }
    out
}

/// Aggregate `git log --format=%aN%x00%aE%x00%ct%x00%H` output (one line per
/// commit, newest first) into per-author [`ContributorStat`] rows.
///
/// Authors are keyed by lowercased email, falling back to lowercased name when
/// the email is empty. `name`/`email`/`last_commit_*` come from the author's
/// newest scanned commit (first line seen); `first_commit_*` from the oldest
/// (last line seen). Sorted by commit count desc, then newest commit, then
/// name; capped at `limit` authors.
fn aggregate_author_log(stdout: &[u8], limit: u32) -> Vec<ContributorStat> {
    struct Acc {
        name: String,
        email: String,
        commit_count: i64,
        first_sha: String,
        first_unix: i64,
        last_sha: String,
        last_unix: i64,
    }
    let text = String::from_utf8_lossy(stdout);
    let mut by_author: std::collections::HashMap<String, Acc> = std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\x00');
        let (Some(name), Some(email), Some(ct), Some(sha)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let name = name.trim().to_string();
        let email = email.trim().to_string();
        let unix = ct.trim().parse::<i64>().unwrap_or(0);
        let sha = sha.trim().to_string();
        if name.is_empty() && email.is_empty() {
            continue;
        }
        let key = if email.is_empty() {
            name.to_ascii_lowercase()
        } else {
            email.to_ascii_lowercase()
        };
        match by_author.get_mut(&key) {
            Some(acc) => {
                acc.commit_count += 1;
                // Walking newest-first: every later line is an older commit.
                acc.first_sha = sha;
                acc.first_unix = unix;
            }
            None => {
                order.push(key.clone());
                by_author.insert(
                    key,
                    Acc {
                        name,
                        email,
                        commit_count: 1,
                        first_sha: sha.clone(),
                        first_unix: unix,
                        last_sha: sha,
                        last_unix: unix,
                    },
                );
            }
        }
    }
    let mut out: Vec<ContributorStat> = order
        .into_iter()
        .filter_map(|k| by_author.remove(&k))
        .map(|a| ContributorStat {
            name: a.name,
            email: a.email,
            commit_count: a.commit_count,
            first_commit_sha: a.first_sha,
            first_commit_unix: a.first_unix,
            last_commit_sha: a.last_sha,
            last_commit_unix: a.last_unix,
        })
        .collect();
    out.sort_by(|a, b| {
        b.commit_count
            .cmp(&a.commit_count)
            .then_with(|| b.last_commit_unix.cmp(&a.last_commit_unix))
            .then_with(|| a.name.cmp(&b.name))
    });
    out.truncate(limit as usize);
    out
}

/// Reject NUL / `..` / leading `-` / absolute-looking refs (T-07-15 / T-07-17 / CR-02).
fn validate_treeish(treeish: &str) -> Result<&str, GitError> {
    let t = treeish.trim();
    if t.is_empty() || t.contains('\0') || t.contains("..") || t.starts_with('-') {
        return Err(GitError::InvalidArg(format!("invalid treeish: {treeish}")));
    }
    // Allow branch/tag/sha characters; reject shell metacharacters.
    if t.chars().any(|c| {
        matches!(
            c,
            ';' | '|' | '&' | '`' | '$' | '(' | ')' | '<' | '>' | '\n' | '\r' | ' '
        )
    }) {
        return Err(GitError::InvalidArg(format!("invalid treeish: {treeish}")));
    }
    Ok(t)
}

/// Reject path traversal in tree/blob paths (T-07-15). Empty path = repo root.
fn validate_repo_rel_path(path: &str) -> Result<String, GitError> {
    let rel = path.trim().trim_start_matches('/');
    if rel.contains('\0') {
        return Err(GitError::InvalidArg("path contains NUL".into()));
    }
    if rel.is_empty() {
        return Ok(String::new());
    }
    let candidate = Path::new(rel);
    if candidate.is_absolute()
        || candidate.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(GitError::InvalidArg(format!("path escapes repo: {path}")));
    }
    Ok(rel.to_string())
}

/// Stricter repo-relative path validation for file *writes* (GIT-19):
/// [`validate_repo_rel_path`] traversal/NUL/absolute checks plus non-empty,
/// no empty segments (`a//b`, trailing `/`), no `.` segments, and no `.git`
/// component (reserved by git checkout).
fn validate_edit_path(path: &str) -> Result<String, GitError> {
    // Writes reject absolute paths outright — the read-path helper tolerates a
    // leading `/` for URL-derived inputs, but a file mutation must name an
    // unambiguous repo-relative path.
    if path.trim().starts_with('/') {
        return Err(GitError::InvalidArg(format!(
            "absolute paths are not allowed: {path}"
        )));
    }
    let rel = validate_repo_rel_path(path)?;
    if rel.is_empty() {
        return Err(GitError::InvalidArg("path is required".into()));
    }
    if rel
        .split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == ".git")
    {
        return Err(GitError::InvalidArg(format!("invalid repo path: {path}")));
    }
    Ok(rel)
}

fn parse_ls_tree_line(line: &str) -> Option<TreeEntry> {
    // <mode> SP <type> SP <object> TAB <file>
    let (meta, name) = line.split_once('\t')?;
    let mut parts = meta.split_whitespace();
    let mode = parts.next()?.to_string();
    let kind = TreeEntryKind::parse(parts.next()?)?;
    let oid = parts.next()?.to_string();
    if name.is_empty() || name.contains('\0') {
        return None;
    }
    Some(TreeEntry {
        mode,
        kind,
        oid,
        name: name.to_string(),
    })
}

fn repo_str(repo: &Path) -> Result<&str, GitError> {
    repo.to_str()
        .ok_or_else(|| GitError::InvalidArg(format!("non-utf8 repo path: {}", repo.display())))
}

fn push_signature_verify_config(
    args: &mut Vec<String>,
    extra_env: &mut Vec<(String, String)>,
    allowed_signers: Option<&Path>,
    gpg_home: Option<&Path>,
) -> Result<(), GitError> {
    if let Some(p) = allowed_signers {
        let ps = p.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 allowedSigners path: {}", p.display()))
        })?;
        // Do not force gpg.format=ssh — that blocks OpenPGP %G? when gpg_home is set.
        // SSH signatures still verify via allowedSignersFile.
        if gpg_home.is_none() {
            args.push("-c".into());
            args.push("gpg.format=ssh".into());
        }
        args.push("-c".into());
        args.push(format!("gpg.ssh.allowedSignersFile={ps}"));
    }
    if let Some(home) = gpg_home {
        let hs = home.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 GNUPGHOME path: {}", home.display()))
        })?;
        extra_env.push(("GNUPGHOME".into(), hs.to_string()));
        args.push("-c".into());
        args.push("gpg.trustModel=always".into());
    }
    Ok(())
}

fn map_signature_status(g: &str) -> String {
    match g.trim() {
        "N" | "" => "none".into(),
        // G = good; U = good but untrusted (forge keyring uses trustModel=always).
        "G" | "U" => "valid".into(),
        "B" => "invalid".into(),
        _ => "unknown".into(),
    }
}

fn signature_kind_from_commit(raw: &str) -> String {
    // SSH signatures use "gpgsig -----BEGIN SSH SIGNATURE-----"
    if raw.contains("BEGIN SSH SIGNATURE") {
        "ssh".into()
    } else if raw.contains("BEGIN PGP SIGNATURE") || raw.contains("BEGIN SIGNATURE") {
        "gpg".into()
    } else {
        String::new()
    }
}

/// Whether the commit object carries a signature header — mirrors what git's
/// signature_check scans for (`gpgsig`, plus the legacy `openpgp` header).
/// Header block ends at the first blank line.
fn commit_has_signature(raw: &str) -> bool {
    let headers = raw.split("\n\n").next().unwrap_or(raw);
    headers.starts_with("gpgsig ")
        || headers.starts_with("openpgp ")
        || headers.contains("\ngpgsig ")
        || headers.contains("\nopenpgp ")
}

/// Result of a batched raw-object scan for one commit.
pub(crate) struct SignatureScan {
    pub has_signature: bool,
    pub kind: String,
}

/// Signature presence + kind for many commits via a single
/// `git cat-file --batch` process (replaces a `cat-file -p` spawn per commit).
/// `None` on spawn/protocol failure so callers can fall back — the same
/// silent best-effort as the per-commit calls. Every requested sha is present
/// in the map on success.
async fn signature_scan_batch(
    repo_s: &str,
    shas: &[String],
) -> Option<std::collections::HashMap<String, SignatureScan>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut child = Command::new("git")
        .args(["-C", repo_s, "cat-file", "--batch"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let (Some(mut stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return None;
    };
    let feed = {
        let mut s = shas.join("\n");
        s.push('\n');
        s
    };
    // Feed stdin on a task so a large request list can't deadlock on the
    // pipe buffer while we drain stdout.
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(feed.as_bytes()).await;
        let _ = stdin.shutdown().await;
    });
    let mut buf = Vec::new();
    let _ = stdout.read_to_end(&mut buf).await;
    let _ = writer.await;
    let _ = child.wait().await;

    let mut out = std::collections::HashMap::new();
    // Response per object: "<sha> <type> <size>\n<content>\n" — or
    // "<sha> missing\n". Order follows the request order.
    let mut cursor: &[u8] = buf.as_slice();
    for sha in shas {
        let nl = cursor.iter().position(|&b| b == b'\n')?;
        let header = String::from_utf8_lossy(&cursor[..nl]).to_string();
        cursor = &cursor[nl + 1..];
        let mut parts = header.split_whitespace();
        let _obj = parts.next();
        let obj_type = parts.next().unwrap_or("");
        if obj_type == "missing" {
            out.insert(
                sha.clone(),
                SignatureScan {
                    has_signature: false,
                    kind: String::new(),
                },
            );
            continue;
        }
        let size: usize = parts.next().and_then(|s| s.parse().ok())?;
        if cursor.len() < size + 1 {
            return None;
        }
        let body = String::from_utf8_lossy(&cursor[..size]);
        out.insert(
            sha.clone(),
            SignatureScan {
                has_signature: commit_has_signature(&body),
                kind: signature_kind_from_commit(&body),
            },
        );
        cursor = &cursor[size + 1..];
    }
    Some(out)
}

/// True when no OpenPGP signature in this repo could ever verify: no ambient
/// `gpg.*` config (signers files, program shims, trust/format overrides) and
/// an empty effective OpenPGP keyring (`gpg_home` when given, else the
/// ambient GNUPGHOME/~/.gnupg git would verify against). Under those
/// conditions `%G?` deterministically reports `E` (→ "unknown") for every
/// OpenPGP-signed commit, so verification spawns are pure cost. Any doubt →
/// false → callers run the real %G? pass.
async fn sig_verify_impossible(repo_s: &str, gpg_home: Option<&Path>) -> bool {
    sig_verify_impossible_env(repo_s, gpg_home, &[]).await
}

/// `extra_env` overrides for the probes (tests point GNUPGHOME / git config
/// at controlled paths).
async fn sig_verify_impossible_env(
    repo_s: &str,
    gpg_home: Option<&Path>,
    extra_env: &[(&str, &str)],
) -> bool {
    match Command::new("git")
        .args(["-C", repo_s, "config", "--get-regexp", "^gpg\\."])
        .envs(extra_env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
    {
        // Match(es) found — some gpg.* config exists.
        Ok(o) if o.status.success() => return false,
        // Exit 1 = "no matches" → no gpg.* config. Anything else is a real
        // error — treat as not provable.
        Ok(o) if o.status.code() == Some(1) => {}
        _ => return false,
    }
    // Probe the effective OpenPGP keyring: the passed gpg_home when present,
    // else the ambient home git's verifier would use.
    let mut gpg = Command::new("gpg");
    gpg.arg("--batch");
    let home;
    if let Some(h) = gpg_home {
        home = h.to_str().map(str::to_string);
        match &home {
            Some(h) => {
                gpg.args(["--homedir", h]);
            }
            None => return false,
        }
    }
    match gpg
        .arg("--list-keys")
        .envs(extra_env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
    {
        Ok(o) => o.status.success() && o.stdout.is_empty(),
        Err(_) => false,
    }
}

/// Real `%G?` verification for a subset of commits — one `git show` process
/// (git still spawns its verifier per signed commit, but only for these).
/// Output maps full sha → mapped status; `None` on spawn/parse failure so
/// callers fall back to the whole-page `%G?` pass.
async fn verify_status_batch(
    prefix_args: &[String],
    env: &[(&str, &str)],
    shas: &[String],
) -> Option<std::collections::HashMap<String, String>> {
    if shas.is_empty() {
        return Some(std::collections::HashMap::new());
    }
    let mut args: Vec<&str> = prefix_args.iter().map(String::as_str).collect();
    args.extend(["show", "-s", "--format=%H%x00%G?"]);
    args.extend(shas.iter().map(String::as_str));
    let stdout = run_git_stdout_env(&args, env).await.ok()?;
    let text = String::from_utf8_lossy(&stdout);
    let mut map = std::collections::HashMap::new();
    for line in text.lines() {
        let mut it = line.split('\0');
        let (Some(sha), Some(code)) = (it.next(), it.next()) else {
            return None;
        };
        map.insert(sha.trim().to_string(), map_signature_status(code.trim()));
    }
    if map.len() != shas.len() {
        return None;
    }
    Some(map)
}

/// Whole-page `%G?` re-run — the pre-optimization listing path, kept as the
/// fallback whenever object scans or subset verifies can't prove/produce the
/// statuses. `scan` reuses the batched signature scan for kinds when present.
async fn log_page_verify(
    prefix_args: &[String],
    env: &[(&str, &str)],
    repo_s: &str,
    skip_s: &str,
    limit_s: &str,
    refname: &str,
    scan: Option<std::collections::HashMap<String, SignatureScan>>,
) -> Result<Vec<CommitSummary>, GitError> {
    let skip = format!("--skip={skip_s}");
    let limit = format!("--max-count={limit_s}");
    let mut args: Vec<&str> = prefix_args.iter().map(String::as_str).collect();
    args.extend([
        "log",
        &skip,
        &limit,
        "--format=%H%x00%h%x00%s%x00%an%x00%ae%x00%aI%x00%ce%x00%G?",
        refname,
    ]);
    let stdout = run_git_stdout_env(&args, env).await?;
    let text = String::from_utf8_lossy(&stdout);
    let mut out = Vec::new();
    for record in text.split('\n') {
        let record = record.trim_end_matches('\r');
        if record.is_empty() {
            continue;
        }
        if let Some(summary) = parse_commit_summary_record(record, true) {
            out.push(summary);
        }
    }
    apply_signature_kinds(repo_s, &mut out, scan).await;
    Ok(out)
}

/// Single-commit `cat-file -p` fallback used only when the batch scan fails.
async fn signature_kind_for_sha(repo_s: &str, sha: &str) -> String {
    match run_git_stdout(&["-C", repo_s, "cat-file", "-p", sha]).await {
        Ok(raw) => signature_kind_from_commit(&String::from_utf8_lossy(&raw)),
        Err(_) => String::new(),
    }
}

/// Attach `signature_kind` to non-"none" commits — batched when the scan
/// succeeded, per-commit cat-file as the (rare) fallback. "none" commits are
/// skipped exactly like the old per-commit loop: a signature git itself
/// reported 'N' (e.g. an SSH sig with no signers file) carries no kind.
async fn apply_signature_kinds(
    repo_s: &str,
    out: &mut [CommitSummary],
    scan: Option<std::collections::HashMap<String, SignatureScan>>,
) {
    match scan {
        Some(scan) => {
            for s in out.iter_mut() {
                if s.signature_status == "none" {
                    continue;
                }
                if let Some(sc) = scan.get(&s.sha) {
                    if !sc.kind.is_empty() {
                        s.signature_kind = sc.kind.clone();
                    }
                }
            }
        }
        None => {
            for s in out.iter_mut() {
                if s.signature_status == "none" {
                    continue;
                }
                let kind = signature_kind_for_sha(repo_s, &s.sha).await;
                if !kind.is_empty() {
                    s.signature_kind = kind;
                }
            }
        }
    }
}

/// `expect_sig` says whether the record's format appended a %G? field — the
/// log path uses formats without %G? for unverified listings.
fn parse_commit_summary_record(record: &str, expect_sig: bool) -> Option<CommitSummary> {
    let parts: Vec<&str> = record.split('\0').collect();
    // sha, short, subject, author_name, author_email, authored_at, committer_email, %G?
    if parts.len() < 6 {
        return None;
    }
    let sha = parts[0].trim();
    if sha.is_empty() {
        return None;
    }
    let committer_email = if parts.len() >= 7 {
        parts[6].trim().to_string()
    } else {
        String::new()
    };
    let g = if expect_sig {
        if parts.len() >= 8 {
            parts[7]
        } else if parts.len() >= 7 {
            // Back-compat if committer missing.
            parts[6]
        } else {
            "N"
        }
    } else {
        "N"
    };
    Some(CommitSummary {
        sha: sha.to_string(),
        short_sha: parts[1].trim().to_string(),
        subject: parts[2].to_string(),
        author_name: parts[3].to_string(),
        author_email: parts[4].to_string(),
        authored_at: parts[5].trim().to_string(),
        committer_email,
        signature_status: map_signature_status(g),
        signature_kind: String::new(),
    })
}

fn status_from_diff_header(header: &str) -> String {
    if header.contains("new file mode") {
        "added".into()
    } else if header.contains("deleted file mode") {
        "deleted".into()
    } else if header.contains("rename from") || header.contains("similarity index") {
        "renamed".into()
    } else if header.contains("copy from") {
        "copied".into()
    } else {
        "modified".into()
    }
}

fn path_from_diff_git_line(line: &str) -> String {
    // diff --git a/path b/path  (paths may include spaces when quoted)
    let rest = line.strip_prefix("diff --git ").unwrap_or(line);
    let mut parts = rest.split_whitespace();
    let _a = parts.next().unwrap_or("");
    let b = parts.next().unwrap_or("");
    let path = b.strip_prefix("b/").unwrap_or(b);
    if path.starts_with('"') {
        path.trim_matches('"').to_string()
    } else {
        path.to_string()
    }
}

/// Split unified `git show` / `git diff` patch output into per-file hunks.
fn parse_unified_diff_files(patch: &str, soft_max: usize) -> (Vec<DiffFile>, bool) {
    let mut files = Vec::new();
    let mut truncated = false;
    let mut total = 0usize;
    let mut current_header = String::new();
    let mut current_body = String::new();
    let mut current_path = String::new();

    let flush = |header: &str,
                 body: &str,
                 path: &str,
                 files: &mut Vec<DiffFile>,
                 total: &mut usize,
                 truncated: &mut bool| {
        if path.is_empty() && header.is_empty() && body.is_empty() {
            return;
        }
        let chunk = if header.is_empty() {
            body.to_string()
        } else if body.is_empty() {
            header.to_string()
        } else {
            format!("{header}{body}")
        };
        let take = if *total >= soft_max {
            *truncated = true;
            String::new()
        } else if *total + chunk.len() > soft_max {
            *truncated = true;
            let remain = soft_max - *total;
            chunk.chars().take(remain).collect()
        } else {
            chunk
        };
        *total += take.len();
        if path.is_empty() && take.is_empty() {
            return;
        }
        files.push(DiffFile {
            path: if path.is_empty() {
                "(unknown)".into()
            } else {
                path.to_string()
            },
            status: status_from_diff_header(header),
            patch: take,
        });
    };

    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            flush(
                &current_header,
                &current_body,
                &current_path,
                &mut files,
                &mut total,
                &mut truncated,
            );
            current_path = path_from_diff_git_line(line);
            current_header = format!("{line}\n");
            current_body.clear();
        } else if current_path.is_empty() && current_header.is_empty() {
            // Skip commit metadata preamble before first diff.
            continue;
        } else if line.starts_with("@@")
            || line.starts_with('+')
            || line.starts_with('-')
            || line.starts_with(' ')
            || line == "\\ No newline at end of file"
        {
            current_body.push_str(line);
            current_body.push('\n');
        } else {
            current_header.push_str(line);
            current_header.push('\n');
        }
        if truncated {
            break;
        }
    }
    flush(
        &current_header,
        &current_body,
        &current_path,
        &mut files,
        &mut total,
        &mut truncated,
    );
    (files, truncated)
}

fn parse_blame_porcelain(text: &str, soft_max_lines: usize) -> (Vec<BlameLine>, bool) {
    let mut lines = Vec::new();
    let mut truncated = false;
    let mut sha = String::new();
    let mut author = String::new();
    let mut author_email = String::new();
    let mut authored_at = String::new();
    let mut line_number = 0u32;

    for raw in text.lines() {
        if let Some(content) = raw.strip_prefix('\t') {
            if lines.len() >= soft_max_lines {
                truncated = true;
                break;
            }
            lines.push(BlameLine {
                sha: sha.clone(),
                author_name: author.clone(),
                author_email: author_email.clone(),
                authored_at: authored_at.clone(),
                line_number,
                content: content.to_string(),
            });
            continue;
        }
        if raw.is_empty() {
            continue;
        }
        let mut parts = raw.split_whitespace();
        let first = parts.next().unwrap_or("");
        if first.len() >= 40 && first.chars().all(|c| c.is_ascii_hexdigit()) {
            sha = first.to_string();
            let _orig = parts.next();
            if let Some(final_no) = parts.next() {
                line_number = final_no.parse().unwrap_or(0);
            }
        } else if first == "author" {
            author = raw.strip_prefix("author ").unwrap_or("").to_string();
        } else if first == "author-mail" {
            let mail = raw.strip_prefix("author-mail ").unwrap_or("").trim();
            author_email = mail
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();
        } else if first == "author-time" {
            let secs: i64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            authored_at = chrono_like_iso(secs);
        }
    }
    (lines, truncated)
}

fn chrono_like_iso(secs: i64) -> String {
    if secs < 0 {
        return String::new();
    }
    // Howard Hinnant civil-from-days (proleptic Gregorian), UTC.
    let z = (secs / 86_400) + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let tod = (secs % 86_400) as u32;
    let hour = tod / 3600;
    let min = (tod % 3600) / 60;
    let sec = tod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z")
}

#[async_trait::async_trait]
impl GitBackend for CliGitBackend {
    async fn init_bare(&self, path: &Path, initial_branch: &str) -> Result<(), GitError> {
        let branch = initial_branch.trim();
        if branch.is_empty()
            || branch.contains('/')
            || branch.contains('\0')
            || branch.contains("..")
        {
            return Err(GitError::InvalidArg(format!(
                "invalid initial branch: {initial_branch}"
            )));
        }

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let path_str = path.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 repo path: {}", path.display()))
        })?;

        // Pattern 2 (RESEARCH): init --bare then symbolic-ref (no `--initial-branch`; Git 2.5+).
        run_git(&["init", "--bare", path_str]).await?;
        let head_ref = format!("refs/heads/{branch}");
        run_git(&["-C", path_str, "symbolic-ref", "HEAD", &head_ref]).await?;
        // Phase 13 / D-19: install branch-protection update hook (reconcile-safe).
        install_protection_hooks(path).await?;
        Ok(())
    }

    async fn seed_commit(
        &self,
        bare_path: &Path,
        branch: &str,
        message: &str,
        files: &[(String, Vec<u8>)],
    ) -> Result<(), GitError> {
        self.seed_commit_authored(
            bare_path,
            branch,
            message,
            files,
            "Oxidean",
            FORGE_NOREPLY_EMAIL,
            None,
        )
        .await
    }

    async fn seed_commit_authored(
        &self,
        bare_path: &Path,
        branch: &str,
        message: &str,
        files: &[(String, Vec<u8>)],
        author_name: &str,
        author_email: &str,
        signing_key_path: Option<&Path>,
    ) -> Result<(), GitError> {
        if files.is_empty() {
            return Ok(());
        }

        let branch = branch.trim();
        if branch.is_empty()
            || branch.contains('/')
            || branch.contains('\0')
            || branch.contains("..")
        {
            return Err(GitError::InvalidArg(format!(
                "invalid branch for seed: {branch}"
            )));
        }
        if message.contains('\0') {
            return Err(GitError::InvalidArg("commit message contains NUL".into()));
        }
        let author_name = author_name.trim();
        let author_email = author_email.trim();
        if author_name.is_empty() || author_email.is_empty() {
            return Err(GitError::InvalidArg(
                "author name and email are required".into(),
            ));
        }

        let bare_abs = absolute_path(bare_path)?;
        let bare_str = bare_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 bare path: {}", bare_abs.display()))
        })?;

        let tmp = tempfile::tempdir().map_err(GitError::Io)?;
        let work = tmp.path();
        let work_str = work
            .to_str()
            .ok_or_else(|| GitError::InvalidArg("non-utf8 temp worktree path".into()))?;

        run_git(&["init", work_str]).await?;
        run_git(&[
            "-C",
            work_str,
            "symbolic-ref",
            "HEAD",
            &format!("refs/heads/{branch}"),
        ])
        .await?;

        for (rel, content) in files {
            let dest = safe_worktree_path(work, rel)?;
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&dest, content).await?;
        }

        // Committer is always the forge web-flow identity; author is the acting user.
        run_git(&["-C", work_str, "config", "user.email", FORGE_NOREPLY_EMAIL]).await?;
        run_git(&["-C", work_str, "config", "user.name", "Oxidean"]).await?;
        run_git(&["-C", work_str, "add", "-A"]).await?;

        let commit_env: Vec<(&str, String)> = vec![
            ("GIT_AUTHOR_NAME", author_name.to_string()),
            ("GIT_AUTHOR_EMAIL", author_email.to_string()),
            ("GIT_COMMITTER_NAME", "Oxidean".into()),
            ("GIT_COMMITTER_EMAIL", FORGE_NOREPLY_EMAIL.into()),
        ];
        let mut commit_args: Vec<String> = vec!["-C".into(), work_str.into()];
        if let Some(key) = signing_key_path {
            // Absolute: `git -C <work>` would otherwise resolve a relative key
            // under the temp worktree.
            let key_abs = absolute_path(key)?;
            let key_s = key_abs.to_str().ok_or_else(|| {
                GitError::InvalidArg(format!("non-utf8 signing key path: {}", key_abs.display()))
            })?;
            commit_args.push("-c".into());
            commit_args.push("gpg.format=ssh".into());
            commit_args.push("-c".into());
            commit_args.push(format!("user.signingkey={key_s}"));
            commit_args.push("commit".into());
            commit_args.push("-S".into());
            commit_args.push("-m".into());
            commit_args.push(message.to_string());
        } else {
            // Disable ambient commit.gpgsign so unsigned seeds stay unsigned.
            commit_args.push("-c".into());
            commit_args.push("commit.gpgsign=false".into());
            commit_args.push("commit".into());
            commit_args.push("-m".into());
            commit_args.push(message.to_string());
        }
        let commit_refs: Vec<&str> = commit_args.iter().map(String::as_str).collect();
        let env_pairs: Vec<(&str, &str)> = commit_env
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect();
        run_git_stdout_env(&commit_refs, &env_pairs).await?;

        run_git(&["-C", work_str, "remote", "add", "origin", bare_str]).await?;
        let refspec = format!("HEAD:refs/heads/{branch}");
        run_git(&["-C", work_str, "push", "origin", &refspec]).await?;
        Ok(())
    }

    async fn commit_files(
        &self,
        bare_path: &Path,
        branch: &str,
        base: &str,
        message: &str,
        changes: &[FileChange],
        author_name: &str,
        author_email: &str,
        signing_key_path: Option<&Path>,
        actor_capability: &str,
    ) -> Result<FilesCommit, GitError> {
        let branch = validate_treeish(branch)?;
        let base = validate_treeish(base)?;
        if changes.is_empty() {
            return Err(GitError::InvalidArg("no file changes to commit".into()));
        }
        if message.trim().is_empty() || message.contains('\0') {
            return Err(GitError::InvalidArg("commit message is required".into()));
        }
        let author_name = author_name.trim();
        let author_email = author_email.trim();
        if author_name.is_empty() || author_email.is_empty() {
            return Err(GitError::InvalidArg(
                "author name and email are required".into(),
            ));
        }
        // Normalize every path up front so a bad entry fails before any
        // object writes.
        let normalized: Vec<FileChange> = changes
            .iter()
            .map(|c| match c {
                FileChange::Upsert { path, content } => Ok(FileChange::Upsert {
                    path: validate_edit_path(path)?,
                    content: content.clone(),
                }),
                FileChange::UpsertOid { path, oid } => {
                    let oid = oid.trim();
                    if oid.len() != 40 || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err(GitError::InvalidArg(format!("invalid blob oid: {oid}")));
                    }
                    Ok(FileChange::UpsertOid {
                        path: validate_edit_path(path)?,
                        oid: oid.to_string(),
                    })
                }
                FileChange::Delete { path } => Ok(FileChange::Delete {
                    path: validate_edit_path(path)?,
                }),
            })
            .collect::<Result<_, GitError>>()?;

        let bare_abs = absolute_path(bare_path)?;
        let bare_str = bare_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 bare path: {}", bare_abs.display()))
        })?;

        // New-branch commits must not silently land on an existing head.
        if branch != base {
            let existing = run_git_stdout(&[
                "-C",
                bare_str,
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ])
            .await;
            if let Ok(out) = existing {
                if !out.is_empty() {
                    return Err(GitError::Conflict(format!(
                        "branch already exists: {branch}"
                    )));
                }
            }
        }

        // Scratch repo — all work happens on index plumbing (no checkout), and
        // the commit ships back through `push` so bare `hooks/update` runs.
        let tmp = tempfile::tempdir().map_err(GitError::Io)?;
        let work = tmp.path();
        let work_str = work
            .to_str()
            .ok_or_else(|| GitError::InvalidArg("non-utf8 temp worktree path".into()))?;
        run_git(&["init", work_str]).await?;

        // Pull the base tip (shallow, single ref) so its tree/blob objects are
        // local for read-tree / hash-object / commit-tree. Absent base → the
        // changeset builds a root commit (empty repository).
        let base_ref = format!("refs/heads/{base}");
        let base_tip = run_git_stdout(&[
            "-C",
            bare_str,
            "rev-parse",
            "--verify",
            "--quiet",
            &base_ref,
        ])
        .await
        .ok()
        .map(|o| String::from_utf8_lossy(&o).trim().to_string())
        .filter(|s| !s.is_empty());
        let base_sha: Option<String> = if base_tip.is_some() {
            run_git(&["-C", work_str, "fetch", "--depth=1", bare_str, &base_ref]).await?;
            let out = run_git_stdout(&["-C", work_str, "rev-parse", "FETCH_HEAD"]).await?;
            Some(String::from_utf8_lossy(&out).trim().to_string())
        } else {
            None
        };

        // Scratch index file — real repo index is never touched.
        let index = work.join("oxidean-index");
        let index_s = index
            .to_str()
            .ok_or_else(|| GitError::InvalidArg("non-utf8 index path".into()))?;
        let idx_env: Vec<(&str, &str)> = vec![("GIT_INDEX_FILE", index_s)];
        match &base_sha {
            Some(sha) => run_git_with_env(&["-C", work_str, "read-tree", sha], &idx_env).await?,
            None => run_git_with_env(&["-C", work_str, "read-tree", "--empty"], &idx_env).await?,
        }

        // Stage the changeset via `update-index --index-info`: deletions are
        // emitted first (mode 0 records) so a delete+upsert of the same path
        // resolves to the upsert.
        let mut batch: Vec<u8> = Vec::new();
        for change in &normalized {
            match change {
                FileChange::Delete { path } => {
                    // `:(literal)` — literal pathspec; `path` matches the file
                    // itself and every blob under `path/` (directory delete).
                    let spec = format!(":(literal){path}");
                    let matched = run_git_stdout_env(
                        &["-C", work_str, "ls-files", "-z", "--", &spec],
                        &idx_env,
                    )
                    .await?;
                    if matched.is_empty() {
                        return Err(GitError::NotFound(format!("path not found: {path}")));
                    }
                    for entry in matched.split(|b| *b == 0) {
                        if entry.is_empty() {
                            continue;
                        }
                        batch.extend_from_slice(b"0 0000000000000000000000000000000000000000\t");
                        batch.extend_from_slice(entry);
                        batch.push(0);
                    }
                }
                FileChange::Upsert { path, content } => {
                    let oid = run_git_stdout_env_stdin(
                        &["-C", work_str, "hash-object", "-w", "--stdin"],
                        &idx_env,
                        content,
                    )
                    .await?;
                    let oid = String::from_utf8_lossy(&oid).trim().to_string();
                    batch.extend_from_slice(format!("100644 {oid}\t").as_bytes());
                    batch.extend_from_slice(path.as_bytes());
                    batch.push(0);
                }
                FileChange::UpsertOid { path, oid } => {
                    run_git(&["-C", work_str, "cat-file", "-e", oid])
                        .await
                        .map_err(|_| GitError::NotFound(format!("unknown blob oid: {oid}")))?;
                    batch.extend_from_slice(format!("100644 {oid}\t").as_bytes());
                    batch.extend_from_slice(path.as_bytes());
                    batch.push(0);
                }
            }
        }
        run_git_stdout_env_stdin(
            &["-C", work_str, "update-index", "-z", "--index-info"],
            &idx_env,
            &batch,
        )
        .await?;

        let new_tree = {
            let out = run_git_stdout_env(&["-C", work_str, "write-tree"], &idx_env).await?;
            String::from_utf8_lossy(&out).trim().to_string()
        };
        if let Some(base) = &base_sha {
            let base_tree = {
                let out = run_git_stdout(&[
                    "-C",
                    work_str,
                    "rev-parse",
                    "--verify",
                    &format!("{base}^{{tree}}"),
                ])
                .await?;
                String::from_utf8_lossy(&out).trim().to_string()
            };
            if base_tree == new_tree {
                return Err(GitError::InvalidArg("changeset produces no changes".into()));
            }
        }

        // commit-tree in the scratch repo; committer = forge identity, author
        // = acting user, optional web-flow SSH signature (D-signing parity
        // with seed_commit_authored).
        let mut commit_args: Vec<String> = vec!["-C".into(), work_str.into()];
        if let Some(key) = signing_key_path {
            let key_abs = absolute_path(key)?;
            let key_s = key_abs.to_str().ok_or_else(|| {
                GitError::InvalidArg(format!("non-utf8 signing key path: {}", key_abs.display()))
            })?;
            commit_args.push("-c".into());
            commit_args.push("gpg.format=ssh".into());
            commit_args.push("-c".into());
            commit_args.push(format!("user.signingkey={key_s}"));
            commit_args.push("commit-tree".into());
            commit_args.push("-S".into());
        } else {
            commit_args.push("-c".into());
            commit_args.push("commit.gpgsign=false".into());
            commit_args.push("commit-tree".into());
        }
        commit_args.push("-m".into());
        commit_args.push(message.to_string());
        commit_args.push(new_tree);
        if let Some(base) = &base_sha {
            commit_args.push("-p".into());
            commit_args.push(base.clone());
        }
        let commit_refs: Vec<&str> = commit_args.iter().map(String::as_str).collect();
        let commit_env: Vec<(&str, String)> = vec![
            ("GIT_AUTHOR_NAME", author_name.to_string()),
            ("GIT_AUTHOR_EMAIL", author_email.to_string()),
            ("GIT_COMMITTER_NAME", "Oxidean".into()),
            ("GIT_COMMITTER_EMAIL", FORGE_NOREPLY_EMAIL.into()),
        ];
        let env_pairs: Vec<(&str, &str)> =
            commit_env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let sha = {
            let out = run_git_stdout_env(&commit_refs, &env_pairs).await?;
            String::from_utf8_lossy(&out).trim().to_string()
        };

        // Publish through a real push: hooks/update runs (D-19) and
        // receive-pack enforces fast-forward — a moved tip surfaces as
        // Conflict, a protection-hook decline as Denied.
        let hook_env = protection_hook_push_env(actor_capability);
        let hook_pairs: Vec<(&str, &str)> = hook_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let refspec = format!("{sha}:refs/heads/{branch}");
        match run_git_with_env(&["-C", work_str, "push", bare_str, &refspec], &hook_pairs).await {
            Ok(()) => Ok(FilesCommit {
                sha,
                base_sha,
                branch: branch.to_string(),
            }),
            Err(GitError::Process(msg)) => {
                if msg.contains("non-fast-forward")
                    || msg.contains("fetch first")
                    || msg.contains("stale info")
                {
                    Err(GitError::Conflict(
                        "branch tip moved while committing — retry the change".into(),
                    ))
                } else if msg.contains("hook declined") || msg.contains("protected") {
                    Err(GitError::Denied(msg))
                } else {
                    Err(GitError::Process(msg))
                }
            }
            Err(other) => Err(other),
        }
    }

    async fn ls_tree_entry(
        &self,
        repo: &Path,
        treeish: &str,
        path: &str,
    ) -> Result<Option<TreeEntry>, GitError> {
        let treeish = validate_treeish(treeish)?;
        let path = validate_edit_path(path)?;
        let repo_str = repo.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 repo path: {}", repo.display()))
        })?;
        match run_git_stdout(&["-C", repo_str, "ls-tree", "-z", treeish, "--", &path]).await {
            Ok(bytes) => {
                for chunk in bytes.split(|b| *b == 0) {
                    if chunk.is_empty() {
                        continue;
                    }
                    let line = String::from_utf8_lossy(chunk);
                    if let Some(entry) = parse_ls_tree_line(&line) {
                        if entry.name == path {
                            return Ok(Some(entry));
                        }
                    }
                }
                Ok(None)
            }
            Err(GitError::Process(msg))
                if msg.contains("Not a valid object name")
                    || msg.contains("does not exist")
                    || msg.contains("not exist") =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    async fn ls_tree(
        &self,
        repo: &Path,
        treeish: &str,
        path: &str,
    ) -> Result<Vec<TreeEntry>, GitError> {
        let treeish = validate_treeish(treeish)?;
        let path = validate_repo_rel_path(path)?;
        let repo_str = repo.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 repo path: {}", repo.display()))
        })?;

        // Unborn HEAD / empty repo: rev-parse fails → structured empty.
        let rev = Command::new("git")
            .args(["-C", repo_str, "rev-parse", "--verify", &format!("{treeish}^{{commit}}")])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
        if !rev.status.success() {
            return Ok(Vec::new());
        }

        let mut args: Vec<String> = vec!["-C".into(), repo_str.into(), "ls-tree".into()];
        if path.is_empty() {
            args.push(treeish.to_string());
        } else {
            // List children of the path tree (not the tree entry itself).
            args.push(format!("{treeish}:{path}"));
        }

        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let stdout = match run_git_stdout(&arg_refs).await {
            Ok(b) => b,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("Not a valid object name")
                    || msg.contains("does not exist")
                    || msg.contains("not exist")
                    || msg.contains("bad file")
                {
                    return Ok(Vec::new());
                }
                return Err(e);
            }
        };
        let text = String::from_utf8_lossy(&stdout);
        let mut entries = Vec::new();
        for line in text.lines() {
            if line.is_empty() {
                continue;
            }
            if let Some(entry) = parse_ls_tree_line(line) {
                // When listing a subdirectory, git prints the basename only if
                // we pass path/; with path as tree, entries are children.
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    async fn cat_blob(&self, repo: &Path, treeish: &str, path: &str) -> Result<Vec<u8>, GitError> {
        let treeish = validate_treeish(treeish)?;
        let path = validate_repo_rel_path(path)?;
        if path.is_empty() {
            return Err(GitError::InvalidArg("blob path required".into()));
        }
        let repo_str = repo.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 repo path: {}", repo.display()))
        })?;

        let spec = format!("{treeish}:{path}");
        let output = Command::new("git")
            .args(["-C", repo_str, "cat-file", "-p", &spec])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;

        if output.status.success() {
            return Ok(output.stdout);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("Not a valid object name")
            || stderr.contains("does not exist")
            || stderr.contains("exists on disk, but not in")
            || stderr.contains("bad file")
        {
            return Err(GitError::NotFound(format!("blob {spec}")));
        }
        Err(GitError::Process(format!(
            "git cat-file failed: {}",
            stderr.trim()
        )))
    }

    async fn list_refs(&self, repo: &Path) -> Result<Vec<GitRef>, GitError> {
        let repo_str = repo.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 repo path: {}", repo.display()))
        })?;

        let output = Command::new("git")
            .args([
                "-C",
                repo_str,
                "for-each-ref",
                "--format=%(objectname) %(refname) %(committerdate:iso-strict) %(authorname)",
                "refs/heads",
                "refs/tags",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Empty bare repo may still succeed with empty stdout.
            if output.stdout.is_empty() {
                return Ok(Vec::new());
            }
            return Err(GitError::Process(format!(
                "git for-each-ref failed: {}",
                stderr.trim()
            )));
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let mut refs = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // oid, refname and iso-strict date are space-free; author name may
            // contain spaces so it stays the remainder of the line.
            let mut parts = line.splitn(4, ' ');
            let (Some(oid), Some(name)) = (parts.next(), parts.next()) else {
                continue;
            };
            let date = parts.next().unwrap_or_default().trim();
            let author = parts.next().unwrap_or_default().trim();
            refs.push(GitRef {
                name: name.to_string(),
                oid: oid.to_string(),
                tip_committed_at: (!date.is_empty()).then(|| date.to_string()),
                tip_author_name: (!author.is_empty()).then(|| author.to_string()),
            });
        }
        Ok(refs)
    }

    async fn log(
        &self,
        repo: &Path,
        refname: &str,
        skip: u32,
        limit: u32,
        allowed_signers: Option<&Path>,
        gpg_home: Option<&Path>,
    ) -> Result<Vec<CommitSummary>, GitError> {
        let refname = validate_treeish(refname)?;
        let repo_s = repo_str(repo)?;
        let limit = limit.clamp(1, 100);
        let skip_s = skip.to_string();
        let limit_s = limit.to_string();

        let rev = Command::new("git")
            .args([
                "-C",
                repo_s,
                "rev-parse",
                "--verify",
                &format!("{refname}^{{commit}}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
        if !rev.status.success() {
            return Ok(Vec::new());
        }

        let mut args: Vec<String> = vec!["-C".into(), repo_s.into()];
        let mut extra_env: Vec<(String, String)> = Vec::new();
        push_signature_verify_config(&mut args, &mut extra_env, allowed_signers, gpg_home)?;
        // The config prefix (-C repo, -c pairs) doubles as the argv for any
        // later %G? verify pass — same keyring/signers configuration.
        let verify_prefix_len = args.len();
        args.push("log".into());
        args.push(format!("--skip={skip_s}"));
        args.push(format!("--max-count={limit_s}"));
        // %G? makes git spawn a verifier subprocess per signed commit. The
        // listing runs without it: signatures are detected from the raw
        // objects below and only commits whose signature could actually
        // produce a real status get a %G? pass.
        args.push("--format=%H%x00%h%x00%s%x00%an%x00%ae%x00%aI%x00%ce".into());
        args.push(refname.to_string());
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let env_refs: Vec<(&str, &str)> = extra_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let stdout = run_git_stdout_env(&arg_refs, &env_refs).await?;
        let text = String::from_utf8_lossy(&stdout);
        let mut out = Vec::new();
        for record in text.split('\n') {
            let record = record.trim_end_matches('\r');
            if record.is_empty() {
                continue;
            }
            if let Some(summary) = parse_commit_summary_record(record, false) {
                out.push(summary);
            }
        }
        if out.is_empty() {
            return Ok(out);
        }
        let shas: Vec<String> = out.iter().map(|s| s.sha.clone()).collect();
        let Some(scan) = signature_scan_batch(repo_s, &shas).await else {
            // Can't inspect the objects — whole-page %G? like before.
            return log_page_verify(
                &args[..verify_prefix_len],
                &env_refs,
                repo_s,
                &skip_s,
                &limit_s,
                refname,
                None,
            )
            .await;
        };
        // Partition signed commits: OpenPGP sigs on a provably-empty
        // keyring are deterministic 'E' candidates; everything else needs a
        // real %G?.
        let mut keyring_empty: Option<bool> = None;
        let mut pgp_candidates: Vec<String> = Vec::new();
        let mut verify_set: Vec<String> = Vec::new();
        for s in &out {
            let Some(e) = scan.get(&s.sha) else {
                verify_set.push(s.sha.clone());
                continue;
            };
            if !e.has_signature {
                continue;
            }
            if e.kind == "gpg" {
                if keyring_empty.is_none() {
                    keyring_empty = Some(sig_verify_impossible(repo_s, gpg_home).await);
                }
                if keyring_empty.unwrap_or(false) {
                    pgp_candidates.push(s.sha.clone());
                    continue;
                }
            }
            verify_set.push(s.sha.clone());
        }
        // Verify the first PGP candidate along with the rest: a uniform
        // failure status ("unknown"/"none") means every candidate on this
        // keyring reports identically; a real result (e.g. a verifier that
        // handles PGP sigs under gpg.format=ssh differently) means the
        // others need individual %G? answers.
        if let Some(first) = pgp_candidates.first() {
            verify_set.push(first.clone());
        }
        if !verify_set.is_empty() {
            let Some(statuses) =
                verify_status_batch(&args[..verify_prefix_len], &env_refs, &verify_set).await
            else {
                return log_page_verify(
                    &args[..verify_prefix_len],
                    &env_refs,
                    repo_s,
                    &skip_s,
                    &limit_s,
                    refname,
                    Some(scan),
                )
                .await;
            };
            for s in out.iter_mut() {
                if let Some(st) = statuses.get(&s.sha) {
                    s.signature_status = st.clone();
                }
            }
            let probe = pgp_candidates.first().and_then(|f| statuses.get(f));
            if matches!(probe.map(String::as_str), Some("unknown") | Some("none")) {
                let st = probe.unwrap().clone();
                for sha in &pgp_candidates[1..] {
                    if let Some(s) = out.iter_mut().find(|c| &c.sha == sha) {
                        s.signature_status = st.clone();
                    }
                }
            } else if pgp_candidates.len() > 1 {
                let rest: Vec<String> = pgp_candidates[1..].to_vec();
                let Some(rest_statuses) =
                    verify_status_batch(&args[..verify_prefix_len], &env_refs, &rest).await
                else {
                    return log_page_verify(
                        &args[..verify_prefix_len],
                        &env_refs,
                        repo_s,
                        &skip_s,
                        &limit_s,
                        refname,
                        Some(scan),
                    )
                    .await;
                };
                for s in out.iter_mut() {
                    if let Some(st) = rest_statuses.get(&s.sha) {
                        s.signature_status = st.clone();
                    }
                }
            }
        }
        apply_signature_kinds(repo_s, &mut out, Some(scan)).await;
        Ok(out)
    }

    async fn show_commit(
        &self,
        repo: &Path,
        sha: &str,
        allowed_signers: Option<&Path>,
        gpg_home: Option<&Path>,
    ) -> Result<CommitDetail, GitError> {
        let sha = validate_treeish(sha)?;
        let repo_s = repo_str(repo)?;

        let configured = allowed_signers.is_some() || gpg_home.is_some();
        let mut meta_args: Vec<String> = vec!["-C".into(), repo_s.into()];
        let mut extra_env: Vec<(String, String)> = Vec::new();
        push_signature_verify_config(&mut meta_args, &mut extra_env, allowed_signers, gpg_home)?;
        meta_args.extend(["show".into(), "-s".into()]);
        // Same %G? policy as `log`: required for real statuses under a forge
        // keyring, deferred for the unconfigured pass.
        if configured {
            meta_args.push(
                "--format=%H%x00%h%x00%s%x00%b%x00%an%x00%ae%x00%aI%x00%P%x00%ce%x00%G?".into(),
            );
        } else {
            meta_args.push(
                "--format=%H%x00%h%x00%s%x00%b%x00%an%x00%ae%x00%aI%x00%P%x00%ce".into(),
            );
        }
        meta_args.push(sha.to_string());
        let meta_refs: Vec<&str> = meta_args.iter().map(String::as_str).collect();
        let env_refs: Vec<(&str, &str)> = extra_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let meta = run_git_stdout_env(&meta_refs, &env_refs).await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("unknown revision")
                || msg.contains("bad object")
                || msg.contains("invalid object")
                || msg.contains("Not a valid object")
            {
                GitError::NotFound(format!("commit {sha}"))
            } else {
                e
            }
        })?;
        let meta_text = String::from_utf8_lossy(&meta);
        let meta_line = meta_text.trim();
        let parts: Vec<&str> = meta_line.split('\0').collect();
        if parts.len() < 8 {
            return Err(GitError::Process(format!(
                "unexpected git show format for {sha}"
            )));
        }
        let full_sha = parts[0].trim().to_string();
        let short_sha = parts[1].trim().to_string();
        let subject = parts[2].to_string();
        let body = parts[3].trim_end().to_string();
        let author_name = parts[4].to_string();
        let author_email = parts[5].to_string();
        let authored_at = parts[6].trim().to_string();
        let parents: Vec<String> = parts[7]
            .split_whitespace()
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .collect();
        let committer_email = if parts.len() >= 9 {
            parts[8].trim().to_string()
        } else {
            String::new()
        };
        let g = if parts.len() >= 10 { parts[9] } else { "N" };
        let mut signature_status = map_signature_status(g);
        let mut signature_kind = String::new();
        if configured {
            if signature_status != "none" {
                signature_kind = signature_kind_for_sha(repo_s, &full_sha).await;
            }
        } else {
            // Unconfigured: inspect the raw object, then verify only when it
            // could produce a real status — same policy as `log`.
            let scan = signature_scan_batch(repo_s, std::slice::from_ref(&full_sha)).await;
            let entry = scan.as_ref().and_then(|m| m.get(&full_sha));
            let has_sig = entry.map(|e| e.has_signature).unwrap_or(true);
            if has_sig {
                // Unconfigured ⇒ gpg_home is None — probe the ambient keyring.
                let provable_e = entry.map(|e| e.kind == "gpg").unwrap_or(false)
                    && sig_verify_impossible(repo_s, None).await;
                if provable_e {
                    // OpenPGP sig on a provably-empty ambient keyring → 'E'.
                    signature_status = "unknown".into();
                    signature_kind = entry
                        .map(|e| e.kind.clone())
                        .unwrap_or_else(|| "gpg".into());
                } else {
                    // Real %G? for this commit — ssh/unknown sig kinds and
                    // verifiable keyrings need git's own answer.
                    if let Ok(g) =
                        run_git_stdout(&["-C", repo_s, "show", "-s", "--format=%G?", &full_sha])
                            .await
                    {
                        signature_status = map_signature_status(String::from_utf8_lossy(&g).trim());
                    }
                    if signature_status != "none" {
                        signature_kind = match &entry {
                            Some(e) if !e.kind.is_empty() => e.kind.clone(),
                            _ => signature_kind_for_sha(repo_s, &full_sha).await,
                        };
                    }
                }
            }
        }

        let patch_bytes = run_git_stdout(&[
            "-C",
            repo_s,
            "show",
            "--format=",
            "--patch",
            "--find-renames",
            &full_sha,
        ])
        .await?;
        let patch_text = String::from_utf8_lossy(&patch_bytes);
        let (files, truncated) = parse_unified_diff_files(&patch_text, DIFF_SOFT_MAX_BYTES);

        Ok(CommitDetail {
            sha: full_sha,
            short_sha,
            subject,
            body,
            author_name,
            author_email,
            authored_at,
            committer_email,
            parents,
            files,
            truncated,
            signature_status,
            signature_kind,
        })
    }

    async fn diff(&self, repo: &Path, base: &str, head: &str) -> Result<DiffResult, GitError> {
        let base = validate_treeish(base)?;
        let head = validate_treeish(head)?;
        let repo_s = repo_str(repo)?;

        // Resolve both ends; identical trees → empty (not 500).
        for tip in [base, head] {
            let rev = Command::new("git")
                .args(["-C", repo_s, "rev-parse", "--verify", &format!("{tip}^{{commit}}")])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
            if !rev.status.success() {
                return Err(GitError::NotFound(format!("ref {tip}")));
            }
        }

        let range = format!("{base}...{head}");
        let output = Command::new("git")
            .args(["-C", repo_s, "diff", "--find-renames", "--patch", &range])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Empty / nothing to compare still often exits 0 with empty stdout.
            if output.stdout.is_empty()
                && (stderr.contains("unknown revision")
                    || stderr.contains("bad revision")
                    || stderr.contains("ambiguous"))
            {
                return Err(GitError::NotFound(format!("diff {range}")));
            }
            if output.stdout.is_empty() {
                return Ok(DiffResult {
                    base: base.to_string(),
                    head: head.to_string(),
                    files: Vec::new(),
                    empty: true,
                    truncated: false,
                });
            }
            return Err(GitError::Process(format!(
                "git diff failed: {}",
                stderr.trim()
            )));
        }

        if output.stdout.is_empty() {
            return Ok(DiffResult {
                base: base.to_string(),
                head: head.to_string(),
                files: Vec::new(),
                empty: true,
                truncated: false,
            });
        }

        let patch_text = String::from_utf8_lossy(&output.stdout);
        let (files, truncated) = parse_unified_diff_files(&patch_text, DIFF_SOFT_MAX_BYTES);
        let empty = files.is_empty();
        Ok(DiffResult {
            base: base.to_string(),
            head: head.to_string(),
            files,
            empty,
            truncated,
        })
    }

    async fn blame(&self, repo: &Path, refname: &str, path: &str) -> Result<BlameFile, GitError> {
        let refname = validate_treeish(refname)?;
        let path = validate_repo_rel_path(path)?;
        if path.is_empty() {
            return Err(GitError::InvalidArg("blame path required".into()));
        }
        let repo_s = repo_str(repo)?;

        let output = Command::new("git")
            .args([
                "-C",
                repo_s,
                "blame",
                "--line-porcelain",
                refname,
                "--",
                &path,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("no such path")
                || stderr.contains("cannot find")
                || stderr.contains("Not a valid object")
                || stderr.contains("no such ref")
                || stderr.contains("fatal:")
            {
                return Err(GitError::NotFound(format!("blame {refname}:{path}")));
            }
            return Err(GitError::Process(format!(
                "git blame failed: {}",
                stderr.trim()
            )));
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let (lines, truncated) = parse_blame_porcelain(&text, BLAME_SOFT_MAX_LINES);
        Ok(BlameFile {
            path,
            ref_name: refname.to_string(),
            lines,
            truncated,
        })
    }

    async fn branch_create(&self, repo: &Path, name: &str, start: &str) -> Result<(), GitError> {
        let name = validate_treeish(name)?;
        let start = validate_treeish(start)?;
        let repo_s = repo_str(repo)?;
        // End-of-options so option-like names cannot become switches (CR-02).
        run_git(&["-C", repo_s, "branch", "--", name, start]).await?;
        Ok(())
    }

    async fn branch_rename(&self, repo: &Path, from: &str, to: &str) -> Result<(), GitError> {
        let from = validate_treeish(from)?;
        let to = validate_treeish(to)?;
        let repo_s = repo_str(repo)?;
        // Known flag -m, then end-of-options before user operands (CR-02).
        run_git(&["-C", repo_s, "branch", "-m", "--", from, to]).await?;
        Ok(())
    }

    async fn branch_delete(&self, repo: &Path, name: &str) -> Result<(), GitError> {
        let name = validate_treeish(name)?;
        let repo_s = repo_str(repo)?;
        // Force delete: forge UI confirms; bare repos have no "unmerged" worktree concept.
        // Known flag -D, then end-of-options before user operand (CR-02).
        run_git(&["-C", repo_s, "branch", "-D", "--", name]).await?;
        Ok(())
    }

    async fn archive(
        &self,
        repo: &Path,
        treeish: &str,
        format: ArchiveFormat,
        prefix: &str,
    ) -> Result<Vec<u8>, GitError> {
        let treeish = validate_treeish(treeish)?;
        let repo_s = repo_str(repo)?;
        let prefix = prefix.trim().trim_matches('/');
        if prefix.is_empty() || prefix.contains('\0') || prefix.contains("..") {
            return Err(GitError::InvalidArg(format!("invalid archive prefix: {prefix}")));
        }
        if prefix.chars().any(|c| matches!(c, ';' | '|' | '&' | '`' | '$' | '\n' | '\r')) {
            return Err(GitError::InvalidArg(format!("invalid archive prefix: {prefix}")));
        }
        let prefix_arg = format!("--prefix={prefix}/");
        let format_arg = format!("--format={}", format.as_git_format());
        // Known format/prefix flags, then end-of-options before user revision (CR-01).
        let args = [
            "-C",
            repo_s,
            "archive",
            format_arg.as_str(),
            prefix_arg.as_str(),
            "--",
            treeish,
        ];

        let result = tokio::time::timeout(ARCHIVE_TIMEOUT, run_git_stdout(&args))
            .await
            .map_err(|_| GitError::Process("git archive timed out".into()))?;

        match result {
            Ok(bytes) => Ok(bytes),
            Err(GitError::Process(msg)) => {
                let lower = msg.to_lowercase();
                if lower.contains("not a valid object")
                    || lower.contains("bad revision")
                    || lower.contains("unknown revision")
                    || lower.contains("does not exist")
                    || lower.contains("ambiguous argument")
                {
                    Err(GitError::NotFound(format!(
                        "archive unavailable for ref {treeish}"
                    )))
                } else {
                    Err(GitError::Process(msg))
                }
            }
            Err(e) => Err(e),
        }
    }

    async fn gc(&self, repo: &Path) -> Result<(), GitError> {
        let repo_s = repo_str(repo)?;
        run_git(&["-C", repo_s, "gc", "--auto"]).await?;
        Ok(())
    }

    async fn merge_commit(
        &self,
        repo: &Path,
        base_ref: &str,
        head_sha: &str,
        message: &str,
    ) -> Result<String, GitError> {
        merge_via_worktree(repo, base_ref, head_sha, message, MergeMode::MergeCommit).await
    }

    async fn squash_merge(
        &self,
        repo: &Path,
        base_ref: &str,
        head_sha: &str,
        message: &str,
    ) -> Result<String, GitError> {
        merge_via_worktree(repo, base_ref, head_sha, message, MergeMode::Squash).await
    }

    async fn rebase_merge(
        &self,
        repo: &Path,
        base_ref: &str,
        head_sha: &str,
    ) -> Result<String, GitError> {
        merge_via_worktree(repo, base_ref, head_sha, "rebase", MergeMode::Rebase).await
    }

    async fn fetch_ref_from(
        &self,
        dest: &Path,
        source: &Path,
        refname: &str,
    ) -> Result<String, GitError> {
        let dest_abs = absolute_path(dest)?;
        let source_abs = absolute_path(source)?;
        let dest_s = dest_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 dest: {}", dest_abs.display()))
        })?;
        let source_s = source_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 source: {}", source_abs.display()))
        })?;
        let refname = validate_treeish(refname)?;
        // Fetch into a temporary ref then resolve SHA.
        let tmp_ref = format!("refs/oxidean/fetch-tmp/{}", uuid_like());
        let refspec = format!("+{refname}:{tmp_ref}");
        run_git(&["-C", dest_s, "fetch", source_s, &refspec]).await?;
        let sha = run_git_stdout(&["-C", dest_s, "rev-parse", &tmp_ref]).await?;
        let sha = String::from_utf8_lossy(&sha).trim().to_string();
        let _ = run_git(&["-C", dest_s, "update-ref", "-d", &tmp_ref]).await;
        Ok(sha)
    }

    async fn clone_bare(&self, source: &Path, dest: &Path) -> Result<(), GitError> {
        let source_abs = absolute_path(source)?;
        let dest_abs = absolute_path(dest)?;
        let source_s = source_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 source: {}", source_abs.display()))
        })?;
        let dest_s = dest_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 dest: {}", dest_abs.display()))
        })?;
        if dest_abs.exists() {
            return Err(GitError::InvalidArg(format!(
                "dest already exists: {}",
                dest_abs.display()
            )));
        }
        if let Some(parent) = dest_abs.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        run_git(&["clone", "--bare", source_s, dest_s]).await?;
        // D-FORK-02: every bare copy gets protection hooks (same as init_bare).
        // Failure propagates so fork RPC can compensate (D-FORK-04).
        install_protection_hooks(&dest_abs).await?;
        Ok(())
    }

    async fn grep(
        &self,
        repo: &Path,
        treeish: &str,
        pattern: &str,
        pathspecs: &[String],
        max_matches: u32,
    ) -> Result<GrepResult, GitError> {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return Ok(GrepResult {
                hits: Vec::new(),
                truncated: false,
            });
        }
        if pattern.contains('\0') {
            return Err(GitError::InvalidArg("grep pattern contains NUL".into()));
        }
        let treeish = validate_treeish(treeish)?;
        let repo_s = repo_str(repo)?;
        let max_matches = max_matches.clamp(1, 10_000);
        let mut path_owned = Vec::new();
        for p in pathspecs {
            if !p.trim().is_empty() {
                path_owned.push(validate_repo_rel_path(p)?);
            }
        }

        // Empty / unborn → empty hits.
        let rev = Command::new("git")
            .args([
                "-C",
                repo_s,
                "rev-parse",
                "--verify",
                &format!("{treeish}^{{commit}}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
        if !rev.status.success() {
            return Ok(GrepResult {
                hits: Vec::new(),
                truncated: false,
            });
        }

        // -n line numbers, -I skip binary (D-SRCH-08), -e pattern as arg (no shell).
        // Tree-ish must NOT follow `--` or git treats it as a pathspec (work-tree error on bare).
        let mut cmd = Command::new("git");
        cmd.args(["-C", repo_s, "grep", "-n", "-I", "-e", pattern, treeish]);
        if !path_owned.is_empty() {
            cmd.arg("--");
            cmd.args(&path_owned);
        }
        let output = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git grep: {e}")))?;

        match output.status.code() {
            Some(0) | Some(1) => {}
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(GitError::Process(format!(
                    "git grep failed (status {:?}): {}",
                    output.status.code(),
                    stderr.trim()
                )));
            }
        }

        // git grep prefixes matches with `treeish:` when searching a revision.
        // Format: `<treeish>:<path>:<line>:<content>`
        let text = String::from_utf8_lossy(&output.stdout);
        let mut hits = Vec::new();
        let mut truncated = false;
        for raw in text.split('\n') {
            let raw = raw.trim_end_matches('\r');
            if raw.is_empty() {
                continue;
            }
            if let Some(hit) = parse_grep_line(raw, treeish) {
                if hits.len() as u32 >= max_matches {
                    truncated = true;
                    break;
                }
                hits.push(hit);
            }
        }
        Ok(GrepResult { hits, truncated })
    }

    async fn log_search(
        &self,
        repo: &Path,
        refname: &str,
        grep: Option<&str>,
        author: Option<&str>,
        skip: u32,
        limit: u32,
    ) -> Result<Vec<CommitSummary>, GitError> {
        let refname = validate_treeish(refname)?;
        let repo_s = repo_str(repo)?;
        let limit = limit.clamp(1, 100);
        let skip_s = skip.to_string();
        let limit_s = limit.to_string();

        let grep = grep.map(str::trim).filter(|s| !s.is_empty());
        let author = author.map(str::trim).filter(|s| !s.is_empty());
        if grep.is_none() && author.is_none() {
            return Ok(Vec::new());
        }
        if grep.is_some_and(|s| s.contains('\0')) || author.is_some_and(|s| s.contains('\0')) {
            return Err(GitError::InvalidArg("log_search filter contains NUL".into()));
        }

        let rev = Command::new("git")
            .args([
                "-C",
                repo_s,
                "rev-parse",
                "--verify",
                &format!("{refname}^{{commit}}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
        if !rev.status.success() {
            return Ok(Vec::new());
        }

        let mut args: Vec<String> = vec![
            "-C".into(),
            repo_s.to_string(),
            "log".into(),
            format!("--skip={skip_s}"),
            format!("--max-count={limit_s}"),
            "--regexp-ignore-case".into(),
            "--format=%H%x00%h%x00%s%x00%an%x00%ae%x00%aI%x00%G?".into(),
        ];
        if let Some(g) = grep {
            args.push(format!("--grep={g}"));
        }
        if let Some(a) = author {
            args.push(format!("--author={a}"));
        }
        args.push(refname.to_string());
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

        let stdout = run_git_stdout(&arg_refs).await?;
        let text = String::from_utf8_lossy(&stdout);
        let mut out = Vec::new();
        for record in text.split('\n') {
            let record = record.trim_end_matches('\r');
            if record.is_empty() {
                continue;
            }
            if let Some(summary) = parse_commit_summary_record(record, true) {
                out.push(summary);
            }
        }
        Ok(out)
    }

    async fn log_path(
        &self,
        repo: &Path,
        refname: &str,
        path: &str,
        limit: u32,
    ) -> Result<Vec<CommitSummary>, GitError> {
        let refname = validate_treeish(refname)?;
        let repo_s = repo_str(repo)?;
        let path = path.trim().trim_start_matches('/');
        if path.is_empty() || path.contains('\0') {
            return Err(GitError::InvalidArg("log_path requires a non-empty path".into()));
        }
        if path
            .split('/')
            .any(|seg| seg.is_empty() || seg == "." || seg == "..")
        {
            return Err(GitError::InvalidArg("log_path path has invalid segments".into()));
        }
        let limit = limit.clamp(1, 100);
        let limit_s = limit.to_string();

        let rev = Command::new("git")
            .args([
                "-C",
                repo_s,
                "rev-parse",
                "--verify",
                &format!("{refname}^{{commit}}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
        if !rev.status.success() {
            return Ok(Vec::new());
        }

        let stdout = match run_git_stdout(&[
            "-C",
            repo_s,
            "log",
            &format!("--max-count={limit_s}"),
            "--format=%H%x00%h%x00%s%x00%an%x00%ae%x00%aI%x00%G?",
            refname,
            "--",
            path,
        ])
        .await
        {
            Ok(b) => b,
            Err(_) => return Ok(Vec::new()),
        };
        let text = String::from_utf8_lossy(&stdout);
        let mut out = Vec::new();
        for record in text.split('\n') {
            let record = record.trim_end_matches('\r');
            if record.is_empty() {
                continue;
            }
            if let Some(summary) = parse_commit_summary_record(record, true) {
                out.push(summary);
            }
        }
        Ok(out)
    }

    async fn path_last_commits(
        &self,
        repo: &Path,
        refname: &str,
        dir_path: &str,
        entry_names: &[String],
    ) -> Result<std::collections::HashMap<String, CommitSummary>, GitError> {
        let refname = validate_treeish(refname)?;
        let dir = dir_path.trim().trim_start_matches('/').trim_end_matches('/');
        if dir.contains('\0')
            || dir.split('/').any(|seg| seg == "." || seg == "..")
        {
            return Err(GitError::InvalidArg(
                "path_last_commits dir_path has invalid segments".into(),
            ));
        }

        let mut out = std::collections::HashMap::new();
        if entry_names.is_empty() {
            return Ok(out);
        }

        // Concurrency-capped parallel `log -1 -- path` (max 8).
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(8));
        let mut joins = Vec::with_capacity(entry_names.len());
        for name in entry_names {
            let name = name.trim().to_string();
            if name.is_empty() || name.contains('/') || name.contains('\0') || name == ".." {
                continue;
            }
            let full = if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            let repo = repo.to_path_buf();
            let refname = refname.to_string();
            let sem = sem.clone();
            let backend = self.clone();
            joins.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.ok()?;
                let commits = backend.log_path(&repo, &refname, &full, 1).await.ok()?;
                commits.into_iter().next().map(|c| (name, c))
            }));
        }
        for j in joins {
            if let Ok(Some((name, commit))) = j.await {
                out.insert(name, commit);
            }
        }
        Ok(out)
    }

    async fn rev_list_count(&self, repo: &Path, refname: &str) -> Result<u64, GitError> {
        let refname = validate_treeish(refname)?;
        let repo_s = repo_str(repo)?;
        let rev = Command::new("git")
            .args([
                "-C",
                repo_s,
                "rev-parse",
                "--verify",
                &format!("{refname}^{{commit}}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
        if !rev.status.success() {
            return Ok(0);
        }
        let stdout = run_git_stdout(&["-C", repo_s, "rev-list", "--count", refname]).await?;
        let text = String::from_utf8_lossy(&stdout);
        let n = text.trim().parse::<u64>().unwrap_or(0);
        Ok(n)
    }

    async fn shortlog(
        &self,
        repo: &Path,
        refname: &str,
        limit: u32,
    ) -> Result<Vec<ContributorSummary>, GitError> {
        let refname = validate_treeish(refname)?;
        let repo_s = repo_str(repo)?;
        let limit = limit.clamp(1, 100);

        let rev = Command::new("git")
            .args([
                "-C",
                repo_s,
                "rev-parse",
                "--verify",
                &format!("{refname}^{{commit}}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
        if !rev.status.success() {
            return Ok(Vec::new());
        }

        let stdout = run_git_stdout(&["-C", repo_s, "shortlog", "-sn", "-e", refname]).await?;
        Ok(parse_shortlog_stdout(&stdout, limit))
    }

    async fn contributor_scan(
        &self,
        repo: &Path,
        refname: &str,
        limit: u32,
        max_commits: u64,
    ) -> Result<ContributorScan, GitError> {
        let refname = validate_treeish(refname)?;
        let repo_s = repo_str(repo)?;
        let limit = limit.clamp(1, 100);
        let max_commits = max_commits.clamp(1, 1_000_000);

        let rev = Command::new("git")
            .args([
                "-C",
                repo_s,
                "rev-parse",
                "--verify",
                &format!("{refname}^{{commit}}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
        if !rev.status.success() {
            return Ok(ContributorScan {
                contributors: Vec::new(),
                scanned_commits: 0,
                truncated: false,
            });
        }

        // A `max + 1` rev-list count detects a clipped walk without counting
        // the full history on huge repositories.
        let cap_probe = max_commits.saturating_add(1);
        let counted = run_git_stdout(&[
            "-C",
            repo_s,
            "rev-list",
            "--count",
            &format!("--max-count={cap_probe}"),
            refname,
        ])
        .await?;
        let walked = String::from_utf8_lossy(&counted)
            .trim()
            .parse::<u64>()
            .unwrap_or(0);
        let truncated = walked > max_commits;
        let scanned_commits = walked.min(max_commits);

        // One bounded `git log` pass yields count + first/last commit per
        // author — `git shortlog` cannot emit per-author timestamps.
        let stdout = run_git_stdout(&[
            "-C",
            repo_s,
            "log",
            "--format=%aN%x00%aE%x00%ct%x00%H",
            &format!("--max-count={max_commits}"),
            refname,
        ])
        .await?;
        Ok(ContributorScan {
            contributors: aggregate_author_log(&stdout, limit),
            scanned_commits,
            truncated,
        })
    }

    async fn commit_times(
        &self,
        repo: &Path,
        refname: &str,
        since_unix: Option<i64>,
        max_commits: u64,
    ) -> Result<CommitTimesResult, GitError> {
        let refname = validate_treeish(refname)?;
        let repo_s = repo_str(repo)?;
        let max_commits = max_commits.clamp(1, 1_000_000);

        let rev = Command::new("git")
            .args([
                "-C",
                repo_s,
                "rev-parse",
                "--verify",
                &format!("{refname}^{{commit}}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
        if !rev.status.success() {
            return Ok(CommitTimesResult {
                times: Vec::new(),
                truncated: false,
            });
        }

        // `@<epoch>` only parses for plausible dates (`@0` is not a real
        // cutoff), so a non-positive bound simply skips the filter.
        let since_arg = since_unix.and_then(|s| (s > 0).then(|| format!("--since=@{s}")));
        let max_count_arg = format!("--max-count={}", max_commits.saturating_add(1));
        let mut args = vec!["-C", repo_s, "log", "--format=%ct", &max_count_arg];
        if let Some(arg) = &since_arg {
            args.push(arg);
        }
        args.push(refname);
        let stdout = run_git_stdout(&args).await?;
        let mut times: Vec<i64> = String::from_utf8_lossy(&stdout)
            .split('\n')
            .filter_map(|line| line.trim().parse::<i64>().ok())
            .collect();
        let truncated = times.len() as u64 > max_commits;
        times.truncate(max_commits as usize);
        Ok(CommitTimesResult { times, truncated })
    }

    async fn ls_tree_sized_blobs(
        &self,
        repo: &Path,
        treeish: &str,
        max_entries: u32,
    ) -> Result<Vec<SizedBlobEntry>, GitError> {
        let treeish = validate_treeish(treeish)?;
        let repo_s = repo_str(repo)?;
        let max_entries = max_entries.clamp(1, 200_000);

        let rev = Command::new("git")
            .args([
                "-C",
                repo_s,
                "rev-parse",
                "--verify",
                &format!("{treeish}^{{commit}}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
        if !rev.status.success() {
            return Ok(Vec::new());
        }

        // `-l` adds blob size after the OID; `-r` walks the full tree.
        let stdout =
            match run_git_stdout(&["-C", repo_s, "ls-tree", "-r", "-l", "--full-tree", treeish])
                .await
            {
                Ok(b) => b,
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("Not a valid object name")
                        || msg.contains("does not exist")
                        || msg.contains("not exist")
                    {
                        return Ok(Vec::new());
                    }
                    return Err(e);
                }
            };
        let text = String::from_utf8_lossy(&stdout);
        let mut out = Vec::new();
        for line in text.lines() {
            if out.len() as u32 >= max_entries {
                break;
            }
            if let Some(entry) = parse_ls_tree_sized_blob_line(line) {
                out.push(entry);
            }
        }
        Ok(out)
    }

    async fn ls_remote_url(
        &self,
        url: &str,
        credentials: &RemoteCredentials,
    ) -> Result<Vec<GitRef>, GitError> {
        let url = validate_remote_url(url)?;
        let stdout = run_git_remote(&["ls-remote", "--heads", "--tags", url], credentials).await?;
        let text = String::from_utf8_lossy(&stdout);
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let oid = parts.next().unwrap_or("").to_string();
            let name = parts.next().unwrap_or("").to_string();
            if oid.is_empty() || name.is_empty() {
                continue;
            }
            // Skip peeled tags (`^{}`).
            if name.ends_with("^{}") {
                continue;
            }
            out.push(GitRef {
                name,
                oid,
                tip_author_name: None,
                tip_committed_at: None,
            });
        }
        Ok(out)
    }

    async fn fetch_from_url(
        &self,
        dest: &Path,
        url: &str,
        credentials: &RemoteCredentials,
    ) -> Result<(), GitError> {
        let url = validate_remote_url(url)?;
        let dest_abs = absolute_path(dest)?;
        let dest_s = dest_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 dest: {}", dest_abs.display()))
        })?;
        // Overwrite only the mirror tracking namespace (not local heads).
        let _ = run_git_remote(
            &[
                "-C",
                dest_s,
                "fetch",
                "--prune",
                url,
                "+refs/heads/*:refs/oxidean/mirror/heads/*",
                "+refs/tags/*:refs/oxidean/mirror/tags/*",
            ],
            credentials,
        )
        .await?;
        Ok(())
    }

    async fn push_to_url(
        &self,
        repo: &Path,
        url: &str,
        credentials: &RemoteCredentials,
        local_ref: &str,
        remote_ref: &str,
    ) -> Result<(), GitError> {
        let url = validate_remote_url(url)?;
        let local_ref = validate_treeish(local_ref)?;
        let remote_ref = validate_treeish(remote_ref)?;
        if local_ref.contains(':') || remote_ref.contains(':') {
            return Err(GitError::InvalidArg("ref names must not contain ':'".into()));
        }
        let repo_abs = absolute_path(repo)?;
        let repo_s = repo_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 repo: {}", repo_abs.display()))
        })?;
        // Explicit refspec only — never --force / --mirror.
        let refspec = format!("{local_ref}:{remote_ref}");
        let _ = run_git_remote(&["-C", repo_s, "push", url, &refspec], credentials).await?;
        Ok(())
    }

    async fn push_to_url_force(
        &self,
        repo: &Path,
        url: &str,
        credentials: &RemoteCredentials,
        local_ref: &str,
        remote_ref: &str,
    ) -> Result<(), GitError> {
        let url = validate_remote_url(url)?;
        let remote_ref = validate_treeish(remote_ref)?;
        if remote_ref.contains(':') {
            return Err(GitError::InvalidArg("ref names must not contain ':'".into()));
        }
        let refspec = if local_ref.is_empty() {
            // Delete remote ref.
            format!(":{remote_ref}")
        } else {
            let local_ref = validate_treeish(local_ref)?;
            if local_ref.contains(':') {
                return Err(GitError::InvalidArg("ref names must not contain ':'".into()));
            }
            format!("+{local_ref}:{remote_ref}")
        };
        let repo_abs = absolute_path(repo)?;
        let repo_s = repo_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 repo: {}", repo_abs.display()))
        })?;
        let _ = run_git_remote(&["-C", repo_s, "push", url, &refspec], credentials).await?;
        Ok(())
    }

    async fn force_update_ref(
        &self,
        repo: &Path,
        refname: &str,
        target_sha: &str,
    ) -> Result<(), GitError> {
        let refname = validate_treeish(refname)?;
        let target_sha = validate_treeish(target_sha)?;
        let bare_abs = absolute_path(repo)?;
        let bare_s = bare_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 bare path: {}", bare_abs.display()))
        })?;

        let _ = run_git_stdout(&[
            "-C",
            bare_s,
            "cat-file",
            "-e",
            &format!("{target_sha}^{{commit}}"),
        ])
        .await?;

        let tmp = tempfile::tempdir().map_err(GitError::Io)?;
        let work = tmp.path();
        let work_s = work
            .to_str()
            .ok_or_else(|| GitError::InvalidArg("non-utf8 temp worktree".into()))?;
        run_git(&["clone", bare_s, work_s]).await?;
        run_git(&["-C", work_s, "checkout", "--detach", target_sha]).await?;

        let hook_env = protection_hook_push_env("admin");
        let hook_env_refs: Vec<(&str, &str)> = hook_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        if let Some(branch) = refname.strip_prefix("refs/heads/") {
            let refspec = format!("+HEAD:refs/heads/{branch}");
            run_git_with_env(&["-C", work_s, "push", "origin", &refspec], &hook_env_refs).await?;
            return Ok(());
        }
        if let Some(tag) = refname.strip_prefix("refs/tags/") {
            // Force-move tag on bare via worktree.
            let _ = run_git(&["-C", work_s, "tag", "-f", tag, target_sha]).await;
            let refspec = format!("+refs/tags/{tag}:refs/tags/{tag}");
            run_git_with_env(&["-C", work_s, "push", "origin", &refspec], &hook_env_refs).await?;
            return Ok(());
        }
        Err(GitError::InvalidArg(format!(
            "force_update_ref supports heads/tags only: {refname}"
        )))
    }

    async fn committer_unix_time(&self, repo: &Path, sha: &str) -> Result<i64, GitError> {
        let sha = validate_treeish(sha)?;
        let repo_s = repo_str(repo)?;
        let stdout = run_git_stdout(&["-C", repo_s, "log", "-1", "--format=%ct", sha]).await?;
        let s = String::from_utf8_lossy(&stdout).trim().to_string();
        s.parse::<i64>()
            .map_err(|e| GitError::Process(format!("invalid committer time for {sha}: {e}")))
    }

    async fn clone_bare_url(
        &self,
        url: &str,
        dest: &Path,
        credentials: &RemoteCredentials,
    ) -> Result<(), GitError> {
        let url = validate_remote_url(url)?;
        let dest_abs = absolute_path(dest)?;
        let dest_s = dest_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 dest: {}", dest_abs.display()))
        })?;
        if dest_abs.exists() {
            return Err(GitError::InvalidArg(format!(
                "dest already exists: {}",
                dest_abs.display()
            )));
        }
        if let Some(parent) = dest_abs.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let _ = run_git_remote(&["clone", "--bare", url, dest_s], credentials).await?;
        install_protection_hooks(&dest_abs).await?;
        Ok(())
    }

    async fn is_ancestor(
        &self,
        repo: &Path,
        maybe_ancestor: &str,
        tip: &str,
    ) -> Result<bool, GitError> {
        let maybe_ancestor = validate_treeish(maybe_ancestor)?;
        let tip = validate_treeish(tip)?;
        let repo_s = repo_str(repo)?;
        let output = Command::new("git")
            .args([
                "-C",
                repo_s,
                "merge-base",
                "--is-ancestor",
                maybe_ancestor,
                tip,
            ])
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| GitError::Process(format!("failed to spawn git: {e}")))?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(GitError::Process(format!(
                    "git merge-base --is-ancestor failed: {}",
                    stderr.trim()
                )))
            }
        }
    }

    async fn ahead_behind(
        &self,
        repo: &Path,
        head: &str,
        base: &str,
    ) -> Result<(u64, u64), GitError> {
        let head = validate_treeish(head)?;
        let base = validate_treeish(base)?;
        let repo_s = repo_str(repo)?;
        let range = format!("{head}...{base}");
        let stdout =
            run_git_stdout(&["-C", repo_s, "rev-list", "--left-right", "--count", &range]).await?;
        let text = String::from_utf8_lossy(&stdout);
        let mut parts = text.split_whitespace();
        let ahead = parts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| {
                GitError::Process(format!("unexpected rev-list output: {}", text.trim()))
            })?;
        let behind = parts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| {
                GitError::Process(format!("unexpected rev-list output: {}", text.trim()))
            })?;
        Ok((ahead, behind))
    }

    async fn fast_forward_ref(
        &self,
        repo: &Path,
        refname: &str,
        target_sha: &str,
    ) -> Result<(), GitError> {
        let refname = validate_treeish(refname)?;
        let target_sha = validate_treeish(target_sha)?;
        let bare_abs = absolute_path(repo)?;
        let bare_s = bare_abs.to_str().ok_or_else(|| {
            GitError::InvalidArg(format!("non-utf8 bare path: {}", bare_abs.display()))
        })?;

        // Ensure the object exists locally.
        let _ = run_git_stdout(&["-C", bare_s, "cat-file", "-e", &format!("{target_sha}^{{commit}}")])
            .await?;

        // Push through a worktree so hooks/update runs (same pattern as merge).
        let tmp = tempfile::tempdir().map_err(GitError::Io)?;
        let work = tmp.path();
        let work_s = work
            .to_str()
            .ok_or_else(|| GitError::InvalidArg("non-utf8 temp worktree".into()))?;
        run_git(&["clone", bare_s, work_s]).await?;
        let branch = refname.strip_prefix("refs/heads/").unwrap_or(refname);
        // Detached checkout of target, then push to branch (FF only — no +).
        run_git(&["-C", work_s, "checkout", "--detach", target_sha]).await?;
        let refspec = format!("HEAD:refs/heads/{branch}");
        let hook_env = protection_hook_push_env("admin");
        let hook_env_refs: Vec<(&str, &str)> = hook_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        run_git_with_env(&["-C", work_s, "push", "origin", &refspec], &hook_env_refs).await?;
        Ok(())
    }

    async fn rev_parse(&self, repo: &Path, rev: &str) -> Result<String, GitError> {
        let rev = validate_treeish(rev)?;
        let repo_s = repo_str(repo)?;
        let stdout = run_git_stdout(&["-C", repo_s, "rev-parse", rev]).await?;
        Ok(String::from_utf8_lossy(&stdout).trim().to_string())
    }

    async fn update_ref(&self, repo: &Path, refname: &str, sha: &str) -> Result<(), GitError> {
        let refname = validate_treeish(refname)?;
        if !refname.starts_with("refs/") {
            return Err(GitError::InvalidArg(format!(
                "update_ref requires a fully-qualified refs/* name: {refname}"
            )));
        }
        let sha = validate_treeish(sha)?;
        if !(7..=64).contains(&sha.len()) || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(GitError::InvalidArg(format!(
                "update_ref target must be a sha: {sha}"
            )));
        }
        let repo_s = repo_str(repo)?;
        run_git(&["-C", repo_s, "update-ref", refname, sha]).await
    }
}

/// Parse `git ls-tree -r -l` lines: `mode type oid size\tpath` (blobs only).
fn parse_ls_tree_sized_blob_line(line: &str) -> Option<SizedBlobEntry> {
    let (meta, path) = line.split_once('\t')?;
    if path.is_empty() || path.contains('\0') {
        return None;
    }
    let mut parts = meta.split_whitespace();
    let _mode = parts.next()?;
    let kind = parts.next()?;
    if kind != "blob" {
        return None;
    }
    let _oid = parts.next()?;
    let size_s = parts.next()?;
    // Submodules / missing size show as `-`.
    if size_s == "-" {
        return None;
    }
    let size: u64 = size_s.parse().ok()?;
    Some(SizedBlobEntry {
        path: path.to_string(),
        size,
    })
}

fn parse_grep_line(line: &str, treeish: &str) -> Option<GrepHit> {
    let rest = line.strip_prefix(&format!("{treeish}:")).unwrap_or(line);
    let (path, after_path) = rest.split_once(':')?;
    let (line_s, content) = after_path.split_once(':')?;
    let line_no: u32 = line_s.parse().ok()?;
    if path.is_empty() {
        return None;
    }
    Some(GrepHit {
        path: path.to_string(),
        line: line_no,
        content: content.to_string(),
    })
}

#[derive(Clone, Copy)]
enum MergeMode {
    MergeCommit,
    Squash,
    Rebase,
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{n:x}")
}

async fn merge_via_worktree(
    bare: &Path,
    base_ref: &str,
    head_sha: &str,
    message: &str,
    mode: MergeMode,
) -> Result<String, GitError> {
    let base_ref = validate_treeish(base_ref)?;
    let head_sha = validate_treeish(head_sha)?;
    if message.contains('\0') {
        return Err(GitError::InvalidArg("commit message contains NUL".into()));
    }
    let bare_abs = absolute_path(bare)?;
    let bare_s = bare_abs.to_str().ok_or_else(|| {
        GitError::InvalidArg(format!("non-utf8 bare path: {}", bare_abs.display()))
    })?;

    let tmp = tempfile::tempdir().map_err(GitError::Io)?;
    let work = tmp.path();
    let work_s = work
        .to_str()
        .ok_or_else(|| GitError::InvalidArg("non-utf8 temp worktree".into()))?;

    run_git(&["clone", bare_s, work_s]).await?;
    run_git(&["-C", work_s, "config", "user.email", FORGE_NOREPLY_EMAIL]).await?;
    run_git(&["-C", work_s, "config", "user.name", "Oxidean"]).await?;
    if run_git(&[
        "-C",
        work_s,
        "checkout",
        "-B",
        base_ref,
        &format!("origin/{base_ref}"),
    ])
    .await
    .is_err()
    {
        run_git(&["-C", work_s, "checkout", "-B", base_ref, base_ref]).await?;
    }

    // Ensure head object is present (same-repo SHA already is).
    let _ = run_git(&["-C", work_s, "fetch", "origin", head_sha]).await;

    match mode {
        MergeMode::MergeCommit => {
            if let Err(e) = run_git(&[
                "-C",
                work_s,
                "merge",
                "--no-ff",
                "-m",
                message,
                head_sha,
            ])
            .await
            {
                let msg = e.to_string();
                if msg.to_lowercase().contains("conflict") {
                    return Err(GitError::Process(format!("merge conflict: {msg}")));
                }
                return Err(e);
            }
        }
        MergeMode::Squash => {
            if let Err(e) = run_git(&["-C", work_s, "merge", "--squash", head_sha]).await {
                let msg = e.to_string();
                if msg.to_lowercase().contains("conflict") {
                    return Err(GitError::Process(format!("merge conflict: {msg}")));
                }
                return Err(e);
            }
            run_git(&["-C", work_s, "commit", "-m", message]).await?;
        }
        MergeMode::Rebase => {
            run_git(&["-C", work_s, "checkout", "-B", "oxidean-rebase-head", head_sha])
                .await?;
            if let Err(e) = run_git(&["-C", work_s, "rebase", base_ref]).await {
                let _ = run_git(&["-C", work_s, "rebase", "--abort"]).await;
                let msg = e.to_string();
                if msg.to_lowercase().contains("conflict") {
                    return Err(GitError::Process(format!("merge conflict: {msg}")));
                }
                return Err(e);
            }
            run_git(&["-C", work_s, "checkout", base_ref]).await?;
            run_git(&["-C", work_s, "merge", "--ff-only", "oxidean-rebase-head"]).await?;
        }
    }

    let sha_bytes = run_git_stdout(&["-C", work_s, "rev-parse", "HEAD"]).await?;
    let sha = String::from_utf8_lossy(&sha_bytes).trim().to_string();
    let refspec = format!("HEAD:refs/heads/{base_ref}");
    let hook_env = protection_hook_push_env("admin");
    let hook_env_refs: Vec<(&str, &str)> = hook_env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    run_git_with_env(&["-C", work_s, "push", "origin", &refspec], &hook_env_refs).await?;
    Ok(sha)
}

/// Resolve `path` against the process cwd when relative (seed push remote safety).
fn absolute_path(path: &Path) -> Result<PathBuf, GitError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(GitError::Io)?;
    Ok(cwd.join(path))
}

/// Reject absolute paths and `..` components (T-07-09).
fn safe_worktree_path(work: &Path, rel: &str) -> Result<PathBuf, GitError> {
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() || rel.contains('\0') {
        return Err(GitError::InvalidArg(format!("invalid seed path: {rel}")));
    }
    let candidate = Path::new(rel);
    if candidate.is_absolute()
        || candidate.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(GitError::InvalidArg(format!(
            "path escapes worktree: {rel}"
        )));
    }
    let dest = work.join(candidate);
    let work_canon = work;
    if !dest.starts_with(work_canon) {
        return Err(GitError::InvalidArg(format!(
            "path escapes worktree: {rel}"
        )));
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{ArchiveFormat, GitBackend, GitError, TreeEntryKind};

    #[test]
    fn absolute_path_keeps_absolute_and_joins_relative() {
        let abs = PathBuf::from("/var/repos/x.git");
        assert_eq!(absolute_path(&abs).unwrap(), abs);
        let rel = PathBuf::from("var/repos/x.git");
        let joined = absolute_path(&rel).unwrap();
        assert!(joined.is_absolute());
        assert!(joined.ends_with("var/repos/x.git"));
    }

    #[test]
    fn missing_ssh_binary_error_is_detected() {
        assert!(looks_like_missing_ssh(
            "error: cannot run ssh: No such file or directory\nfatal: unable to fork"
        ));
        assert!(!looks_like_missing_ssh("Permission denied (publickey)."));
    }

    #[tokio::test]
    async fn seed_commit_works_with_relative_bare_path() {
        let root = tempfile::tempdir().unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(root.path()).unwrap();
        let bare = PathBuf::from("relative-bare.git");
        let git = CliGitBackend::new();
        let result = async {
            git.init_bare(&bare, "main").await?;
            git.seed_commit(
                &bare,
                "main",
                "seed",
                &[("README.md".into(), b"# hi\n".to_vec())],
            )
            .await
        }
        .await;
        let _ = std::env::set_current_dir(&prev);
        result.expect("relative bare seed");
        assert!(root.path().join("relative-bare.git").join("HEAD").exists());
    }

    #[tokio::test]
    async fn ls_tree_returns_dirs_files_and_gitlink_modes() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("modes.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "seed",
            &[
                ("README.md".into(), b"# hi\n".to_vec()),
                ("src/lib.rs".into(), b"fn x() {}\n".to_vec()),
            ],
        )
        .await
        .unwrap();

        // Add gitlink via plumbing on a worktree push.
        let wt = tempfile::tempdir().unwrap();
        let wt_s = wt.path().to_str().unwrap();
        let bare_s = bare.to_str().unwrap();
        run_git(&["clone", bare_s, wt_s]).await.unwrap();
        let index_info = b"160000 commit 0123456789abcdef0123456789abcdef01234567\tvendor/dep\n";
        let mut child = Command::new("git")
            .args(["-C", wt_s, "update-index", "--add", "--index-info"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        use tokio::io::AsyncWriteExt;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(index_info)
            .await
            .unwrap();
        let out = child.wait_with_output().await.unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        run_git(&["-C", wt_s, "commit", "-m", "gitlink"]).await.unwrap();
        run_git(&["-C", wt_s, "push", "origin", "HEAD:main"]).await.unwrap();

        let entries = git.ls_tree(&bare, "main", "").await.expect("ls_tree root");
        let readme = entries.iter().find(|e| e.name == "README.md").expect("blob");
        assert_eq!(readme.mode, "100644");
        assert_eq!(readme.kind, TreeEntryKind::Blob);
        let src = entries.iter().find(|e| e.name == "src").expect("tree");
        assert_eq!(src.mode, "040000");
        assert_eq!(src.kind, TreeEntryKind::Tree);
        let vendor = entries.iter().find(|e| e.name == "vendor").expect("vendor");
        assert_eq!(vendor.kind, TreeEntryKind::Tree);

        let nested = git.ls_tree(&bare, "main", "vendor").await.expect("vendor");
        let dep = nested.iter().find(|e| e.name == "dep").expect("gitlink");
        assert_eq!(dep.mode, "160000");
        assert_eq!(dep.kind, TreeEntryKind::Commit);
    }

    #[tokio::test]
    async fn cat_blob_returns_seeded_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("blob.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "c",
            &[("a.txt".into(), b"hello-blob\n".to_vec())],
        )
        .await
        .unwrap();
        let bytes = git.cat_blob(&bare, "main", "a.txt").await.unwrap();
        assert_eq!(bytes, b"hello-blob\n");
    }

    #[tokio::test]
    async fn grep_finds_seeded_line_and_empty_on_miss() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("grep.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "c",
            &[("src/a.txt".into(), b"alpha\nUNIQUE_GREP_TOKEN\nbeta\n".to_vec())],
        )
        .await
        .unwrap();
        let hit = git
            .grep(&bare, "main", "UNIQUE_GREP_TOKEN", &[], 50)
            .await
            .expect("grep");
        assert_eq!(hit.hits.len(), 1);
        assert_eq!(hit.hits[0].path, "src/a.txt");
        assert_eq!(hit.hits[0].line, 2);
        assert!(hit.hits[0].content.contains("UNIQUE_GREP_TOKEN"));
        assert!(!hit.truncated);

        let miss = git
            .grep(&bare, "main", "no_such_token_zzz", &[], 50)
            .await
            .expect("grep miss");
        assert!(miss.hits.is_empty());
        assert!(!miss.truncated);
    }

    #[tokio::test]
    async fn grep_skips_binary_with_i_and_truncates() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("grep_cap.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        let mut text = String::new();
        for i in 0..20 {
            text.push_str(&format!("CAP_TOKEN line {i}\n"));
        }
        // Binary-ish file with NUL — git grep -I should skip.
        let mut bin = b"CAP_TOKEN\0binary".to_vec();
        bin.extend_from_slice(&[0u8; 8]);
        git.seed_commit(
            &bare,
            "main",
            "c",
            &[
                ("text.txt".into(), text.into_bytes()),
                ("bin.dat".into(), bin),
            ],
        )
        .await
        .unwrap();
        let capped = git
            .grep(&bare, "main", "CAP_TOKEN", &[], 5)
            .await
            .expect("grep cap");
        assert_eq!(capped.hits.len(), 5);
        assert!(capped.truncated);
        assert!(
            capped.hits.iter().all(|h| h.path != "bin.dat"),
            "binary file should be skipped with -I"
        );
    }

    #[tokio::test]
    async fn grep_with_multiple_pathspecs_and_icase_glob() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("grep_paths.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "c",
            &[
                (
                    "SRC/A.RS".into(),
                    b"fn TOK() {}
"
                    .to_vec(),
                ),
                (
                    "src/b.ts".into(),
                    b"const TOK = 1;
"
                    .to_vec(),
                ),
                (
                    "src/readme.md".into(),
                    b"TOK docs
"
                    .to_vec(),
                ),
            ],
        )
        .await
        .unwrap();
        // Language-style pathspecs: `:(icase)` extension glob OR'd across specs.
        let specs = vec![":(icase)*.rs".to_string(), ":(icase)*.ts".to_string()];
        let hits = git
            .grep(&bare, "main", "TOK", &specs, 50)
            .await
            .expect("grep pathspecs");
        let mut paths: Vec<&str> = hits.hits.iter().map(|h| h.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, ["SRC/A.RS", "src/b.ts"]);

        // `:(icase,glob)` prefix + filename matcher (`language:` filename rows).
        let specs = vec![":(icase,glob)src/**/readme.md".to_string()];
        let hits = git
            .grep(&bare, "main", "TOK", &specs, 50)
            .await
            .expect("grep glob filename");
        assert_eq!(hits.hits.len(), 1);
        assert_eq!(hits.hits[0].path, "src/readme.md");

        // `..` traversal and NUL injection pathspecs are rejected outright.
        // (A leading `/` is normalized like the `path:` qualifier, so an
        // absolute-looking spec can't escape the repo — git only sees
        // repo-relative pathspecs.)
        let bad = vec!["../escape.txt".to_string()];
        assert!(git.grep(&bare, "main", "TOK", &bad, 50).await.is_err());
        let bad = vec!["a\0b".to_string()];
        assert!(git.grep(&bare, "main", "TOK", &bad, 50).await.is_err());
    }

    #[tokio::test]
    async fn log_search_matches_message_and_author() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("log_search.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "UNIQUE_COMMIT_MSG_TOKEN",
            &[("a.txt".into(), b"one\n".to_vec())],
        )
        .await
        .unwrap();
        let by_msg = git
            .log_search(&bare, "main", Some("UNIQUE_COMMIT_MSG_TOKEN"), None, 0, 10)
            .await
            .expect("log_search msg");
        assert_eq!(by_msg.len(), 1);
        assert!(by_msg[0].subject.contains("UNIQUE_COMMIT_MSG_TOKEN"));

        let by_author = git
            .log_search(&bare, "main", None, Some("Oxidean"), 0, 10)
            .await
            .expect("log_search author");
        assert!(!by_author.is_empty());

        let miss = git
            .log_search(&bare, "main", Some("zzz_no_msg"), None, 0, 10)
            .await
            .expect("log_search miss");
        assert!(miss.is_empty());
    }

    #[tokio::test]
    async fn ls_tree_empty_repo_returns_empty_vec() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("empty.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        let entries = git.ls_tree(&bare, "main", "").await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn log_returns_paged_commit_summaries_for_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("log.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "first commit",
            &[("a.txt".into(), b"one\n".to_vec())],
        )
        .await
        .unwrap();
        // Second commit via clone + push.
        let wt = tempfile::tempdir().unwrap();
        let wt_s = wt.path().to_str().unwrap();
        let bare_s = bare.to_str().unwrap();
        run_git(&["clone", bare_s, wt_s]).await.unwrap();
        tokio::fs::write(wt.path().join("a.txt"), b"two\n")
            .await
            .unwrap();
        run_git(&["-C", wt_s, "add", "a.txt"]).await.unwrap();
        run_git(&["-C", wt_s, "commit", "-m", "second commit"])
            .await
            .unwrap();
        run_git(&["-C", wt_s, "push", "origin", "HEAD:main"])
            .await
            .unwrap();

        let page = git.log(&bare, "main", 0, 10, None, None).await.expect("log");
        assert!(
            page.len() >= 2,
            "expected at least 2 commits, got {}",
            page.len()
        );
        assert_eq!(page[0].subject, "second commit");
        assert!(!page[0].sha.is_empty());
        assert!(!page[0].short_sha.is_empty());
        assert!(!page[0].author_name.is_empty());
        assert!(!page[0].authored_at.is_empty());

        let skipped = git.log(&bare, "main", 1, 1, None, None).await.expect("log skip");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].subject, "first commit");
    }

    #[tokio::test]
    async fn show_commit_returns_files_and_unified_patch() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("show.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "seed subject",
            &[("hello.txt".into(), b"hello\nworld\n".to_vec())],
        )
        .await
        .unwrap();
        let bare_s = bare.to_str().unwrap();
        let sha_bytes = run_git_stdout(&["-C", bare_s, "rev-parse", "main"])
            .await
            .unwrap();
        let sha = String::from_utf8_lossy(&sha_bytes).trim().to_string();

        let detail = git.show_commit(&bare, &sha, None, None).await.expect("show_commit");
        assert_eq!(detail.sha, sha);
        assert_eq!(detail.subject, "seed subject");
        assert!(
            !detail.files.is_empty(),
            "expected at least one file in commit"
        );
        let hello = detail
            .files
            .iter()
            .find(|f| f.path == "hello.txt")
            .expect("hello.txt");
        assert!(
            hello.patch.contains("hello") || hello.patch.contains("+++"),
            "expected unified patch content, got: {}",
            hello.patch
        );
    }

    #[tokio::test]
    async fn diff_identical_refs_returns_empty_not_error() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("diff.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "only",
            &[("x.txt".into(), b"x\n".to_vec())],
        )
        .await
        .unwrap();

        let result = git
            .diff(&bare, "main", "main")
            .await
            .expect("diff identical must not 500");
        assert!(result.empty, "identical refs should be empty");
        assert!(result.files.is_empty());
    }

    #[tokio::test]
    async fn blame_returns_per_line_meta_for_text_file() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("blame.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "blame me",
            &[("lines.txt".into(), b"alpha\nbeta\n".to_vec())],
        )
        .await
        .unwrap();

        let blame = git.blame(&bare, "main", "lines.txt").await.expect("blame");
        assert_eq!(blame.path, "lines.txt");
        assert!(
            blame.lines.len() >= 2,
            "expected >=2 blame lines, got {}",
            blame.lines.len()
        );
        assert!(!blame.lines[0].sha.is_empty());
        assert!(!blame.lines[0].author_name.is_empty());
        assert_eq!(blame.lines[0].line_number, 1);
        assert!(blame.lines[0].content.contains("alpha"));
    }

    /// Filter: `git_archive_formats` — zip + tar.gz via CliGitBackend (GIT-07).
    #[tokio::test]
    async fn git_archive_formats_zip_and_tar_gz() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("arch.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "archive seed",
            &[("hello.txt".into(), b"hello archive\n".to_vec())],
        )
        .await
        .unwrap();

        let zip = git
            .archive(&bare, "main", ArchiveFormat::Zip, "arch")
            .await
            .expect("zip archive");
        assert!(!zip.is_empty(), "zip archive bytes must be non-empty");
        assert_eq!(&zip[0..2], b"PK", "zip should start with PK magic");

        let tar_gz = git
            .archive(&bare, "main", ArchiveFormat::TarGz, "arch")
            .await
            .expect("tar.gz archive");
        assert!(!tar_gz.is_empty(), "tar.gz archive bytes must be non-empty");
        // gzip magic 1f 8b
        assert_eq!(&tar_gz[0..2], &[0x1f, 0x8b], "tar.gz should be gzip");
    }

    #[tokio::test]
    async fn git_archive_empty_repo_returns_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("empty-arch.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();

        let err = git
            .archive(&bare, "main", ArchiveFormat::Zip, "empty-arch")
            .await
            .expect_err("empty repo archive must fail");
        match err {
            GitError::NotFound(_) => {}
            other => panic!("expected NotFound for empty archive, got {other}"),
        }
    }

    #[tokio::test]
    async fn gc_runs_on_bare_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("gc.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "gc seed",
            &[("a.txt".into(), b"a\n".to_vec())],
        )
        .await
        .unwrap();
        git.gc(&bare).await.expect("gc");
    }

    async fn push_branch_with_file(
        bare: &Path,
        branch: &str,
        from: &str,
        file: &str,
        content: &[u8],
        message: &str,
    ) {
        let wt = tempfile::tempdir().unwrap();
        let wt_s = wt.path().to_str().unwrap();
        let bare_s = bare.to_str().unwrap();
        run_git(&["clone", bare_s, wt_s]).await.unwrap();
        run_git(&["-C", wt_s, "checkout", "-B", branch, from])
            .await
            .unwrap();
        tokio::fs::write(wt.path().join(file), content)
            .await
            .unwrap();
        run_git(&["-C", wt_s, "add", file]).await.unwrap();
        run_git(&["-C", wt_s, "commit", "-m", message])
            .await
            .unwrap();
        let refspec = format!("HEAD:refs/heads/{branch}");
        run_git(&["-C", wt_s, "push", "origin", &refspec])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn merge_commit_creates_merge_on_base() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("merge.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "seed",
            &[("a.txt".into(), b"base\n".to_vec())],
        )
        .await
        .unwrap();
        push_branch_with_file(&bare, "feature", "main", "b.txt", b"feat\n", "feat").await;
        let head = String::from_utf8_lossy(
            &run_git_stdout(&[
                "-C",
                bare.to_str().unwrap(),
                "rev-parse",
                "refs/heads/feature",
            ])
            .await
            .unwrap(),
        )
        .trim()
        .to_string();
        let sha = git
            .merge_commit(&bare, "main", &head, "Merge feature")
            .await
            .expect("merge_commit");
        assert_eq!(sha.len(), 40);
        let tip = String::from_utf8_lossy(
            &run_git_stdout(&["-C", bare.to_str().unwrap(), "rev-parse", "refs/heads/main"])
                .await
                .unwrap(),
        )
        .trim()
        .to_string();
        assert_eq!(tip, sha);
    }

    #[tokio::test]
    async fn squash_merge_single_commit_on_base() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("squash.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "seed",
            &[("a.txt".into(), b"base\n".to_vec())],
        )
        .await
        .unwrap();
        push_branch_with_file(&bare, "feature", "main", "c.txt", b"sq\n", "sq").await;
        let head = String::from_utf8_lossy(
            &run_git_stdout(&[
                "-C",
                bare.to_str().unwrap(),
                "rev-parse",
                "refs/heads/feature",
            ])
            .await
            .unwrap(),
        )
        .trim()
        .to_string();
        let sha = git
            .squash_merge(&bare, "main", &head, "Squash feature")
            .await
            .expect("squash_merge");
        assert_eq!(sha.len(), 40);
    }

    #[tokio::test]
    async fn rebase_merge_fast_forwards_base() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("rebase.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "seed",
            &[("a.txt".into(), b"base\n".to_vec())],
        )
        .await
        .unwrap();
        push_branch_with_file(&bare, "feature", "main", "d.txt", b"rb\n", "rb").await;
        let head = String::from_utf8_lossy(
            &run_git_stdout(&[
                "-C",
                bare.to_str().unwrap(),
                "rev-parse",
                "refs/heads/feature",
            ])
            .await
            .unwrap(),
        )
        .trim()
        .to_string();
        let sha = git
            .rebase_merge(&bare, "main", &head)
            .await
            .expect("rebase_merge");
        assert_eq!(sha.len(), 40);
    }

    #[tokio::test]
    async fn merge_commit_conflict_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("conflict.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "seed",
            &[("clash.txt".into(), b"base\n".to_vec())],
        )
        .await
        .unwrap();
        // Divergent edits on same file.
        push_branch_with_file(
            &bare,
            "feature",
            "main",
            "clash.txt",
            b"feature\n",
            "feat clash",
        )
        .await;
        // Advance main with conflicting content.
        push_branch_with_file(&bare, "main", "main", "clash.txt", b"mainline\n", "main clash")
            .await;
        let head = String::from_utf8_lossy(
            &run_git_stdout(&[
                "-C",
                bare.to_str().unwrap(),
                "rev-parse",
                "refs/heads/feature",
            ])
            .await
            .unwrap(),
        )
        .trim()
        .to_string();
        let err = git
            .merge_commit(&bare, "main", &head, "Merge conflict")
            .await
            .expect_err("expected conflict");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("conflict") || msg.contains("failed"),
            "unexpected err: {msg}"
        );
    }

    #[tokio::test]
    async fn ahead_behind_counts_between_refs() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("ab.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "seed",
            &[("a.txt".into(), b"base\n".to_vec())],
        )
        .await
        .unwrap();
        push_branch_with_file(&bare, "feature", "main", "b.txt", b"feat\n", "feat").await;
        // feature ahead by 1, not behind.
        let (ahead, behind) = git
            .ahead_behind(&bare, "refs/heads/feature", "refs/heads/main")
            .await
            .expect("ahead_behind");
        assert_eq!((ahead, behind), (1, 0));
        // Reverse: main is behind feature by 1.
        let (ahead, behind) = git
            .ahead_behind(&bare, "refs/heads/main", "refs/heads/feature")
            .await
            .expect("ahead_behind reverse");
        assert_eq!((ahead, behind), (0, 1));
        // Advance main once: now diverged 1 / 1.
        push_branch_with_file(&bare, "main", "main", "c.txt", b"mainline\n", "main").await;
        let (ahead, behind) = git
            .ahead_behind(&bare, "refs/heads/feature", "refs/heads/main")
            .await
            .expect("ahead_behind diverged");
        assert_eq!((ahead, behind), (1, 1));
    }

    /// D-FORK-02/03: clone_bare must install hooks/update (same as init_bare).
    #[tokio::test]
    async fn clone_bare_installs_protection_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("src.git");
        let dest = tmp.path().join("dest.git");
        let git = CliGitBackend::new();
        git.init_bare(&source, "main").await.unwrap();
        git.seed_commit(
            &source,
            "main",
            "seed",
            &[("README.md".into(), b"hi\n".to_vec())],
        )
        .await
        .unwrap();
        // Strip hooks so clone cannot inherit a valid update file from source.
        let _ = tokio::fs::remove_file(source.join("hooks").join("update")).await;

        git.clone_bare(&source, &dest).await.expect("clone_bare");
        let update = dest.join("hooks").join("update");
        let meta = tokio::fs::metadata(&update)
            .await
            .expect("hooks/update must exist after clone_bare");
        assert!(meta.is_file(), "hooks/update must be a file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                meta.permissions().mode() & 0o111,
                0o111,
                "hooks/update must be executable"
            );
        }
        let body = tokio::fs::read_to_string(&update).await.unwrap();
        assert!(
            body.contains("OXIDEAN_ENV") && body.contains("production|cloud"),
            "cloned hook script must include D-PKG-02 gate"
        );
    }

    #[test]
    fn protection_hook_push_env_sets_admin_and_optional_helper() {
        let prev = std::env::var_os("OXIDEAN_PROTECTION_HELPER");
        std::env::set_var(
            "OXIDEAN_PROTECTION_HELPER",
            "/usr/local/bin/oxidean-protection-hook",
        );
        let env = protection_hook_push_env("admin");
        match prev {
            Some(v) => std::env::set_var("OXIDEAN_PROTECTION_HELPER", v),
            None => std::env::remove_var("OXIDEAN_PROTECTION_HELPER"),
        }
        let map: std::collections::HashMap<_, _> = env.into_iter().collect();
        assert_eq!(
            map.get("OXIDEAN_ACTOR_CAPABILITY").map(String::as_str),
            Some("admin"),
            "system pushes must use admin capability for hook evaluation"
        );
        assert_eq!(
            map.get("OXIDEAN_PROTECTION_HELPER").map(String::as_str),
            Some("/usr/local/bin/oxidean-protection-hook")
        );
    }

    /// D-FORK-04: hook install failure after clone propagates as Err.
    /// Uses a template where `hooks` is a file so create_dir_all fails
    /// (bare clone installs from GIT_TEMPLATE_DIR, not source hooks).
    #[tokio::test]
    async fn clone_bare_fails_when_hook_install_blocked() {
        let tmp = tempfile::tempdir().unwrap();
        let template = tmp.path().join("tmpl");
        std::fs::create_dir_all(&template).unwrap();
        std::fs::write(template.join("hooks"), b"not-a-directory\n").unwrap();

        let source = tmp.path().join("src.git");
        let dest = tmp.path().join("dest.git");
        let git = CliGitBackend::new();
        git.init_bare(&source, "main").await.unwrap();
        git.seed_commit(
            &source,
            "main",
            "seed",
            &[("README.md".into(), b"hi\n".to_vec())],
        )
        .await
        .unwrap();

        let prev = std::env::var_os("GIT_TEMPLATE_DIR");
        // nextest runs each test in its own process — safe to set for this call.
        std::env::set_var("GIT_TEMPLATE_DIR", &template);
        let result = git.clone_bare(&source, &dest).await;
        match prev {
            Some(v) => std::env::set_var("GIT_TEMPLATE_DIR", v),
            None => std::env::remove_var("GIT_TEMPLATE_DIR"),
        }

        let err = result.expect_err("hook install must fail");
        assert!(
            matches!(err, GitError::Io(_)),
            "unexpected error variant: {err:?}"
        );
    }

    fn file_url(path: &Path) -> String {
        let abs = absolute_path(path).unwrap();
        format!("file://{}", abs.display())
    }

    fn https_cred() -> RemoteCredentials {
        RemoteCredentials {
            kind: RemoteAuthKind::HttpsToken,
            username: Some("git".into()),
            secret: "token".into(),
            known_hosts: None,
        }
    }

    #[test]
    fn validate_remote_url_rejects_file_and_relative() {
        assert!(validate_remote_url("file:///tmp/x.git").is_err());
        assert!(validate_remote_url("/tmp/x.git").is_err());
        assert!(validate_remote_url("./x.git").is_err());
        assert!(validate_remote_url("https://github.com/a/b.git").is_ok());
        assert!(validate_remote_url("git@github.com:a/b.git").is_ok());
        assert!(validate_remote_url("ssh://git@host/a/b.git").is_ok());
    }

    #[tokio::test]
    async fn fetch_push_ff_and_merge_between_bares_via_path_url_rejected() {
        // Path / file:// URLs are rejected — use two local bares via fetch_ref_from
        // pattern covered elsewhere. This asserts URL validation on fetch_from_url.
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.git");
        let git = CliGitBackend::new();
        git.init_bare(&a, "main").await.unwrap();
        let cred = https_cred();
        let err = git
            .fetch_from_url(&a, &file_url(&a), &cred)
            .await
            .expect_err("file:// must be rejected");
        assert!(err.to_string().contains("remote URL"));
    }

    #[tokio::test]
    async fn two_way_ff_and_merge_on_diverge_local_path_engine() {
        // Simulate the mirror engine algorithm between two local bares
        // (URL remotes use the same ancestry/merge/push primitives).
        let tmp = tempfile::tempdir().unwrap();
        let local = tmp.path().join("local.git");
        let remote = tmp.path().join("remote.git");
        let git = CliGitBackend::new();
        git.init_bare(&local, "main").await.unwrap();
        git.seed_commit(
            &local,
            "main",
            "seed",
            &[("a.txt".into(), b"base\n".to_vec())],
        )
        .await
        .unwrap();
        // remote starts as a clone of local
        git.clone_bare(&local, &remote).await.unwrap();

        // Advance local only → should FF-push to remote.
        push_branch_with_file(&local, "main", "main", "b.txt", b"local\n", "local only").await;
        let local_tip = git.rev_parse(&local, "refs/heads/main").await.unwrap();
        // Fetch remote tip into local mirror namespace via path fetch.
        let remote_tip_before = git.rev_parse(&remote, "refs/heads/main").await.unwrap();
        assert!(
            git.is_ancestor(&local, &remote_tip_before, &local_tip)
                .await
                .unwrap()
        );
        // Push local → remote (path URL not allowed; use fetch_ref_from inverse:
        // clone objects by fetching from local into remote via path).
        let fetched = git
            .fetch_ref_from(&remote, &local, "refs/heads/main")
            .await
            .unwrap();
        assert_eq!(fetched, local_tip);
        git.fast_forward_ref(&remote, "refs/heads/main", &local_tip)
            .await
            .unwrap();
        assert_eq!(
            git.rev_parse(&remote, "refs/heads/main").await.unwrap(),
            local_tip
        );

        // Diverge both sides → merge_commit then FF both.
        push_branch_with_file(
            &local,
            "main",
            "main",
            "l.txt",
            b"L\n",
            "local diverge",
        )
        .await;
        push_branch_with_file(
            &remote,
            "main",
            "main",
            "r.txt",
            b"R\n",
            "remote diverge",
        )
        .await;
        let local_sha = git.rev_parse(&local, "refs/heads/main").await.unwrap();
        let remote_sha = git
            .fetch_ref_from(&local, &remote, "refs/heads/main")
            .await
            .unwrap();
        assert_ne!(local_sha, remote_sha);
        assert!(
            !git.is_ancestor(&local, &remote_sha, &local_sha)
                .await
                .unwrap()
        );
        assert!(
            !git.is_ancestor(&local, &local_sha, &remote_sha)
                .await
                .unwrap()
        );
        let merged = git
            .merge_commit(&local, "main", &remote_sha, "Mirror merge")
            .await
            .expect("merge on diverge");
        let fetched_m = git
            .fetch_ref_from(&remote, &local, "refs/heads/main")
            .await
            .unwrap();
        assert_eq!(fetched_m, merged);
        git.fast_forward_ref(&remote, "refs/heads/main", &merged)
            .await
            .unwrap();
        assert_eq!(
            git.rev_parse(&remote, "refs/heads/main").await.unwrap(),
            merged
        );
    }

    #[tokio::test]
    async fn merge_conflict_does_not_force() {
        let tmp = tempfile::tempdir().unwrap();
        let local = tmp.path().join("local.git");
        let remote = tmp.path().join("remote.git");
        let git = CliGitBackend::new();
        git.init_bare(&local, "main").await.unwrap();
        git.seed_commit(
            &local,
            "main",
            "seed",
            &[("clash.txt".into(), b"base\n".to_vec())],
        )
        .await
        .unwrap();
        git.clone_bare(&local, &remote).await.unwrap();
        push_branch_with_file(
            &local,
            "main",
            "main",
            "clash.txt",
            b"local\n",
            "local clash",
        )
        .await;
        push_branch_with_file(
            &remote,
            "main",
            "main",
            "clash.txt",
            b"remote\n",
            "remote clash",
        )
        .await;
        let before = git.rev_parse(&local, "refs/heads/main").await.unwrap();
        let remote_sha = git
            .fetch_ref_from(&local, &remote, "refs/heads/main")
            .await
            .unwrap();
        let err = git
            .merge_commit(&local, "main", &remote_sha, "Mirror merge")
            .await
            .expect_err("conflict");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("conflict") || msg.contains("failed") || msg.contains("process"),
            "unexpected err: {msg}"
        );
        // Local tip unchanged (no force).
        assert_eq!(
            git.rev_parse(&local, "refs/heads/main").await.unwrap(),
            before
        );
    }

    #[tokio::test]
    async fn force_update_ref_moves_non_ff_tip() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("force.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "first",
            &[("a.txt".into(), b"1\n".to_vec())],
        )
        .await
        .unwrap();
        let first = git.rev_parse(&bare, "refs/heads/main").await.unwrap();
        push_branch_with_file(&bare, "main", "main", "b.txt", b"2\n", "second").await;
        let second = git.rev_parse(&bare, "refs/heads/main").await.unwrap();
        assert_ne!(first, second);
        // Force main back to the older tip (non-FF).
        assert!(
            git.fast_forward_ref(&bare, "refs/heads/main", &first)
                .await
                .is_err(),
            "non-FF must fail without force"
        );
        git.force_update_ref(&bare, "refs/heads/main", &first)
            .await
            .unwrap();
        assert_eq!(
            git.rev_parse(&bare, "refs/heads/main").await.unwrap(),
            first
        );
        let ct = git.committer_unix_time(&bare, &first).await.unwrap();
        assert!(ct > 0, "committer unix time should be positive");
    }

    #[tokio::test]
    async fn seed_commit_authored_sets_author_email() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("authored.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit_authored(
            &bare,
            "main",
            "authored seed",
            &[("README.md".into(), b"hi\n".to_vec())],
            "Ada Lovelace",
            "ada@example.com",
            None,
        )
        .await
        .expect("seed_commit_authored");
        let page = git.log(&bare, "main", 0, 1, None, None).await.expect("log");
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].author_name, "Ada Lovelace");
        assert_eq!(page[0].author_email, "ada@example.com");
        assert_eq!(page[0].signature_status, "none");
    }

    #[tokio::test]
    async fn seed_commit_authored_ssh_signed_verifies_with_allowed_signers() {
        let tmp = tempfile::tempdir().unwrap();
        let key_dir = tmp.path().join("keys");
        std::fs::create_dir_all(&key_dir).unwrap();
        let key_path = key_dir.join("web-flow");
        let key_s = key_path.to_str().unwrap();
        let gen = std::process::Command::new("ssh-keygen")
            .args([
                "-t",
                "ed25519",
                "-N",
                "",
                "-f",
                key_s,
                "-C",
                "oxidean-web-flow",
            ])
            .output()
            .expect("ssh-keygen");
        assert!(
            gen.status.success(),
            "ssh-keygen failed: {}",
            String::from_utf8_lossy(&gen.stderr)
        );

        let pub_line = std::fs::read_to_string(key_dir.join("web-flow.pub")).unwrap();
        let mut parts = pub_line.split_whitespace();
        let key_type = parts.next().unwrap();
        let key_b64 = parts.next().unwrap();
        let allowed = tmp.path().join("allowed_signers");
        // Principal must match the committer email used by seed_commit_authored.
        std::fs::write(
            &allowed,
            format!("{FORGE_NOREPLY_EMAIL} namespaces=\"git\" {key_type} {key_b64}\n"),
        )
        .unwrap();

        let bare = tmp.path().join("signed.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit_authored(
            &bare,
            "main",
            "signed seed",
            &[("README.md".into(), b"signed\n".to_vec())],
            "Ada Lovelace",
            "ada@example.com",
            Some(&key_path),
        )
        .await
        .expect("signed seed");

        let page = git
            .log(&bare, "main", 0, 1, Some(allowed.as_path()), None)
            .await
            .expect("log with allowed_signers");
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].author_email, "ada@example.com");
        assert_eq!(
            page[0].signature_status, "valid",
            "expected valid SSH signature, got status={} kind={}",
            page[0].signature_status, page[0].signature_kind
        );
        assert_eq!(page[0].signature_kind, "ssh");
    }

    #[tokio::test]
    async fn gpg_signed_commit_verifies_with_gpg_home() {
        let gpg_ok = std::process::Command::new("gpg")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !gpg_ok {
            eprintln!("skipping: gpg not available");
            return;
        }

        let tmp = tempfile::tempdir().unwrap();
        let sign_home = tmp.path().join("gnupg-sign");
        let verify_home = tmp.path().join("gnupg-verify");
        std::fs::create_dir_all(&sign_home).unwrap();
        std::fs::create_dir_all(&verify_home).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sign_home, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::set_permissions(&verify_home, std::fs::Permissions::from_mode(0o700)).unwrap();
        }

        let email = "gpg-signer@example.com";
        let gen = std::process::Command::new("gpg")
            .args([
                "--batch",
                "--passphrase",
                "",
                "--quick-generate-key",
                &format!("GPG Signer <{email}>"),
                "ed25519",
                "default",
                "never",
            ])
            .env("GNUPGHOME", &sign_home)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("gpg generate");
        assert!(gen.success(), "gpg --quick-generate-key failed");

        let export = std::process::Command::new("gpg")
            .args(["--armor", "--export", email])
            .env("GNUPGHOME", &sign_home)
            .output()
            .expect("gpg export");
        assert!(export.status.success(), "gpg --export failed");
        let armor = String::from_utf8(export.stdout).expect("utf8 armor");

        let mut import = std::process::Command::new("gpg")
            .args(["--batch", "--yes", "--import"])
            .env("GNUPGHOME", &verify_home)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("gpg import spawn");
        {
            use std::io::Write;
            import
                .stdin
                .as_mut()
                .unwrap()
                .write_all(armor.as_bytes())
                .unwrap();
        }
        assert!(import.wait().unwrap().success(), "gpg --import failed");

        let bare = tmp.path().join("gpg-signed.git");
        let work = tmp.path().join("work");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();

        let bare_s = bare.to_str().unwrap();
        let work_s = work.to_str().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["clone", bare_s, work_s])
                .status()
                .unwrap()
                .success()
        );
        for (k, v) in [
            ("user.name", "GPG Signer"),
            ("user.email", email),
            ("commit.gpgsign", "true"),
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(["-C", work_s, "config", k, v])
                    .status()
                    .unwrap()
                    .success()
            );
        }

        // Resolve the freshly generated key id and pin signingkey so ambient
        // ~/.gitconfig user.signingkey cannot steal the sign attempt.
        let list_keys = std::process::Command::new("gpg")
            .args(["--list-secret-keys", "--with-colons"])
            .env("GNUPGHOME", &sign_home)
            .output()
            .expect("list secret keys");
        assert!(list_keys.status.success());
        let list_out = String::from_utf8_lossy(&list_keys.stdout);
        let key_id = list_out
            .lines()
            .find_map(|line| {
                let mut parts = line.split(':');
                if parts.next()? != "sec" {
                    return None;
                }
                // sec:…:…:…:…:<keyid>:…
                parts.nth(3).map(|s| s.to_string())
            })
            .expect("secret key id");
        assert!(
            std::process::Command::new("git")
                .args(["-C", work_s, "config", "user.signingkey", &key_id])
                .status()
                .unwrap()
                .success()
        );

        std::fs::write(work.join("README.md"), "gpg signed\n").unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["-C", work_s, "add", "README.md"])
                .status()
                .unwrap()
                .success()
        );
        let commit = std::process::Command::new("git")
            .args([
                "-C",
                work_s,
                "-c",
                "gpg.program=gpg",
                "-c",
                &format!("user.signingkey={key_id}"),
                "-c",
                "commit.gpgsign=true",
                "commit",
                "-m",
                "gpg seed",
            ])
            .env("GNUPGHOME", &sign_home)
            .env_remove("GPG_TTY")
            .output()
            .expect("git commit");
        assert!(
            commit.status.success(),
            "signed commit failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        );
        assert!(
            std::process::Command::new("git")
                .args(["-C", work_s, "push", "origin", "main"])
                .status()
                .unwrap()
                .success()
        );

        let page = git
            .log(&bare, "main", 0, 1, None, Some(verify_home.as_path()))
            .await
            .expect("log with gpg_home");
        assert_eq!(page.len(), 1);
        assert_eq!(
            page[0].signature_status, "valid",
            "expected valid GPG signature, got status={} kind={}",
            page[0].signature_status, page[0].signature_kind
        );
        assert_eq!(page[0].signature_kind, "gpg");
    }

    /// SSH-sign a seed commit in `bare` (fresh ed25519 key). Returns the key
    /// path and the allowed_signers file for verifying it.
    async fn seed_ssh_signed(
        git: &CliGitBackend,
        tmp: &tempfile::TempDir,
        bare: &Path,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let key_dir = tmp.path().join("keys");
        std::fs::create_dir_all(&key_dir).unwrap();
        let key_path = key_dir.join("web-flow");
        let gen = std::process::Command::new("ssh-keygen")
            .args([
                "-t",
                "ed25519",
                "-N",
                "",
                "-f",
                key_path.to_str().unwrap(),
                "-C",
                "oxidean-web-flow",
            ])
            .output()
            .expect("ssh-keygen");
        assert!(
            gen.status.success(),
            "ssh-keygen failed: {}",
            String::from_utf8_lossy(&gen.stderr)
        );
        let pub_line = std::fs::read_to_string(key_dir.join("web-flow.pub")).unwrap();
        let mut parts = pub_line.split_whitespace();
        let key_type = parts.next().unwrap().to_string();
        let key_b64 = parts.next().unwrap().to_string();
        let allowed = tmp.path().join("allowed_signers");
        std::fs::write(
            &allowed,
            format!("{FORGE_NOREPLY_EMAIL} namespaces=\"git\" {key_type} {key_b64}\n"),
        )
        .unwrap();
        git.init_bare(bare, "main").await.unwrap();
        git.seed_commit_authored(
            bare,
            "main",
            "signed seed",
            &[("README.md".into(), b"signed\n".to_vec())],
            "Ada Lovelace",
            "ada@example.com",
            Some(&key_path),
        )
        .await
        .expect("signed seed");
        (key_path, allowed)
    }

    #[tokio::test]
    async fn unconfigured_log_matches_real_gq_statuses() {
        // Equivalence contract: with no forge keyring the unconfigured log
        // must report exactly what `git log %G?` reports — the fast path and
        // the verifying path are interchangeable.
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("signed.git");
        let git = CliGitBackend::new();
        let (_key, _allowed) = seed_ssh_signed(&git, &tmp, &bare).await;
        // Unsigned follow-up via a real clone — seed_commit would push a
        // second root and get rejected.
        let work = tmp.path().join("work");
        let bare_s = bare.to_str().unwrap();
        let work_s = work.to_str().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["clone", bare_s, work_s])
                .status()
                .unwrap()
                .success()
        );
        for (k, v) in [
            ("commit.gpgsign", "false"),
            ("user.email", "t@example.com"),
            ("user.name", "T"),
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(["-C", work_s, "config", k, v])
                    .status()
                    .unwrap()
                    .success()
            );
        }
        std::fs::write(work.join("b.txt"), "v2\n").unwrap();
        for args in [
            vec!["add", "b.txt"],
            vec!["commit", "-qm", "unsigned follow-up"],
            vec!["push", "-q", "origin", "main"],
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(["-C", work_s])
                    .args(&args)
                    .status()
                    .unwrap()
                    .success()
            );
        }

        let page = git
            .log(&bare, "main", 0, 10, None, None)
            .await
            .expect("unconfigured log");
        assert_eq!(page.len(), 2);

        // Ground truth straight from git.
        let raw = run_git_stdout(&[
            "-C",
            bare.to_str().unwrap(),
            "log",
            "--format=%H%x00%G?",
            "main",
        ])
        .await
        .expect("git log %G?");
        let expected: std::collections::HashMap<String, String> = String::from_utf8_lossy(&raw)
            .lines()
            .filter_map(|l| {
                let (sha, g) = l.split_once('\0')?;
                Some((sha.trim().to_string(), map_signature_status(g)))
            })
            .collect();

        for s in &page {
            let want = expected
                .get(&s.sha)
                .unwrap_or_else(|| panic!("missing ground truth for {}", s.sha));
            assert_eq!(
                &s.signature_status, want,
                "sha {} status mismatch (kind={})",
                s.sha, s.signature_kind
            );
            if s.signature_status == "none" {
                assert_eq!(s.signature_kind, "");
            } else {
                assert_eq!(s.signature_kind, "ssh");
            }
        }
    }

    /// Generate a throwaway OpenPGP key in `sign_home`, returning its key id.
    /// `None` when gpg is unavailable.
    fn gpg_test_key_id(sign_home: &Path, email: &str) -> Option<String> {
        std::fs::create_dir_all(sign_home).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(sign_home, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let gen = std::process::Command::new("gpg")
            .args([
                "--batch",
                "--passphrase",
                "",
                "--quick-generate-key",
                &format!("GPG Signer <{email}>"),
                "ed25519",
                "default",
                "never",
            ])
            .env("GNUPGHOME", sign_home)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if !gen.map(|s| s.success()).unwrap_or(false) {
            return None;
        }
        let list_keys = std::process::Command::new("gpg")
            .args(["--list-secret-keys", "--with-colons"])
            .env("GNUPGHOME", sign_home)
            .output()
            .expect("list secret keys");
        let list_out = String::from_utf8_lossy(&list_keys.stdout);
        list_out.lines().find_map(|line| {
            let mut parts = line.split(':');
            if parts.next()? != "sec" {
                return None;
            }
            parts.nth(3).map(|s| s.to_string())
        })
    }

    /// Create a bare repo with one OpenPGP-signed commit signed by a fresh
    /// key living in `sign_home` (never imported into the ambient keyring).
    /// Returns `false` when gpg is unavailable.
    async fn gpg_sign_seed(
        tmp: &tempfile::TempDir,
        bare: &Path,
        sign_home: &Path,
        email: &str,
    ) -> bool {
        let Some(key_id) = gpg_test_key_id(sign_home, email) else {
            return false;
        };
        let git = CliGitBackend::new();
        git.init_bare(bare, "main").await.unwrap();
        let bare_s = bare.to_str().unwrap();
        let work = tmp.path().join("gpg-work");
        let work_s = work.to_str().unwrap();
        let ok = std::process::Command::new("git")
            .args(["clone", bare_s, work_s])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok);
        std::fs::write(work.join("README.md"), "gpg signed\n").unwrap();
        let env = [("GNUPGHOME", sign_home.to_str().unwrap())];
        for args in [
            vec!["config", "user.email", email],
            vec!["config", "user.name", "GPG Signer"],
            vec!["add", "README.md"],
            vec![
                "-c",
                &format!("user.signingkey={key_id}"),
                "-c",
                "commit.gpgsign=true",
                "commit",
                "-qm",
                "gpg seed",
            ],
            vec!["push", "-q", "origin", "main"],
        ] {
            let status = std::process::Command::new("git")
                .args(["-C", work_s])
                .args(&args)
                .envs(env)
                .env_remove("GPG_TTY")
                .status()
                .unwrap();
            assert!(status.success(), "step failed: {args:?}");
        }
        true
    }

    #[tokio::test]
    async fn unconfigured_log_empty_keyring_fast_path_reports_unknown() {
        // Provable-E fast path: empty ambient keyring + no gpg.* config →
        // signed commits report "unknown" without any verify subprocess.
        let gpg_ok = std::process::Command::new("gpg")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !gpg_ok {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let empty_home = tmp.path().join("empty-gnupg");
        std::fs::create_dir_all(&empty_home).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&empty_home, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let empty_gitconfig = tmp.path().join("empty-gitconfig");
        std::fs::write(&empty_gitconfig, "").unwrap();

        let bare = tmp.path().join("signed.git");
        let sign_home = tmp.path().join("sign-gnupg");
        if !gpg_sign_seed(&tmp, &bare, &sign_home, "gpg-signer@example.com").await {
            eprintln!("skipping: gpg keygen failed");
            return;
        }
        let git = CliGitBackend::new();

        // Probe must agree the ambient keyring cannot verify.
        assert!(
            sig_verify_impossible_env(
                bare.to_str().unwrap(),
                None,
                &[
                    ("GNUPGHOME", empty_home.to_str().unwrap()),
                    ("GIT_CONFIG_GLOBAL", empty_gitconfig.to_str().unwrap()),
                    ("GIT_CONFIG_SYSTEM", "/dev/null"),
                ],
            )
            .await,
            "expected provable-unverifiable ambient keyring"
        );

        // Route the whole log call's ambient env the same way — nextest runs
        // each test in its own process; restore for cargo test anyway.
        let saved: Vec<(&str, Option<std::ffi::OsString>)> =
            ["GNUPGHOME", "GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM"]
                .iter()
                .map(|k| (*k, std::env::var_os(k)))
                .collect();
        std::env::set_var("GNUPGHOME", &empty_home);
        std::env::set_var("GIT_CONFIG_GLOBAL", &empty_gitconfig);
        std::env::set_var("GIT_CONFIG_SYSTEM", "/dev/null");

        let page = git.log(&bare, "main", 0, 10, None, None).await;

        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }

        let page = page.expect("unconfigured log on empty keyring");
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].signature_status, "unknown");
        assert_eq!(page[0].signature_kind, "gpg");
    }

    #[tokio::test]
    async fn unconfigured_log_ssh_signed_falls_back_to_gq_status() {
        // SSH-signed commits take the real %G? pass — their unverifiable
        // outcome is git-version/config dependent, never shortcut.
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("signed.git");
        let git = CliGitBackend::new();
        let (_key, _allowed) = seed_ssh_signed(&git, &tmp, &bare).await;

        let page = git
            .log(&bare, "main", 0, 1, None, None)
            .await
            .expect("unconfigured log");
        assert_eq!(page.len(), 1);
        // Whatever %G? itself reports must be mirrored.
        let raw = run_git_stdout(&[
            "-C",
            bare.to_str().unwrap(),
            "log",
            "--format=%G?",
            "main",
        ])
        .await
        .unwrap();
        let want = map_signature_status(String::from_utf8_lossy(&raw).trim());
        assert_eq!(page[0].signature_status, want);
        if want == "none" {
            assert_eq!(page[0].signature_kind, "");
        } else {
            assert_eq!(page[0].signature_kind, "ssh");
        }
    }

    #[tokio::test]
    async fn default_sig_verify_impossible_false_with_key_or_gpg_config() {
        let gpg_ok = std::process::Command::new("gpg")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !gpg_ok {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("r.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        let repo_s = bare.to_str().unwrap();
        let empty_gitconfig = tmp.path().join("empty-gitconfig");
        std::fs::write(&empty_gitconfig, "").unwrap();
        let cfg_env = [
            ("GIT_CONFIG_GLOBAL", empty_gitconfig.to_str().unwrap()),
            ("GIT_CONFIG_SYSTEM", "/dev/null"),
        ];

        // Empty keyring → provable.
        let empty_home = tmp.path().join("empty-gnupg");
        std::fs::create_dir_all(&empty_home).unwrap();
        let env: Vec<(&str, &str)> = cfg_env
            .iter()
            .copied()
            .chain([("GNUPGHOME", empty_home.to_str().unwrap())])
            .collect();
        assert!(sig_verify_impossible_env(repo_s, None, &env).await);

        // Keyring with a key → not provable (a 'G' outcome is possible).
        let key_home = tmp.path().join("key-gnupg");
        std::fs::create_dir_all(&key_home).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key_home, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let gen = std::process::Command::new("gpg")
            .args([
                "--batch",
                "--passphrase",
                "",
                "--quick-generate-key",
                "Probe <probe@example.com>",
                "ed25519",
                "default",
                "never",
            ])
            .env("GNUPGHOME", &key_home)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("gpg gen");
        assert!(gen.success());
        let env: Vec<(&str, &str)> = cfg_env
            .iter()
            .copied()
            .chain([("GNUPGHOME", key_home.to_str().unwrap())])
            .collect();
        assert!(!sig_verify_impossible_env(repo_s, None, &env).await);

        // Repo-local gpg.* config → not provable even on an empty keyring.
        assert!(
            std::process::Command::new("git")
                .args(["-C", repo_s, "config", "gpg.ssh.allowedSignersFile", "/tmp/x"])
                .status()
                .unwrap()
                .success()
        );
        let env: Vec<(&str, &str)> = cfg_env
            .iter()
            .copied()
            .chain([("GNUPGHOME", empty_home.to_str().unwrap())])
            .collect();
        assert!(!sig_verify_impossible_env(repo_s, None, &env).await);
    }

    #[tokio::test]
    async fn configured_log_mixed_sigs_verifies_only_verifiable_subset() {
        // The forge-shaped call: allowed_signers for ssh sigs, no gpg_home.
        // The SSH-signed commit must verify for real; the PGP-signed commit
        // on a provably-empty ambient keyring collapses to "unknown" after
        // the probe candidate reports 'E'.
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("mixed.git");
        let git = CliGitBackend::new();
        let (_key, allowed) = seed_ssh_signed(&git, &tmp, &bare).await;

        // Stack a PGP-signed commit on top — its key lives only in sign_home.
        let sign_home = tmp.path().join("sign-gnupg");
        let email = "pgp-signer@example.com";
        let Some(key_id) = gpg_test_key_id(&sign_home, email) else {
            eprintln!("skipping: gpg keygen failed");
            return;
        };
        let bare_s = bare.to_str().unwrap();
        let work = tmp.path().join("mixed-work");
        let work_s = work.to_str().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["clone", bare_s, work_s])
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(work.join("pgp.txt"), "pgp\n").unwrap();
        let env = [("GNUPGHOME", sign_home.to_str().unwrap())];
        for args in [
            vec!["config", "user.email", email],
            vec!["config", "user.name", "PGP Signer"],
            vec!["add", "pgp.txt"],
            vec![
                "-c",
                &format!("user.signingkey={key_id}"),
                "-c",
                "commit.gpgsign=true",
                "commit",
                "-qm",
                "pgp on top",
            ],
            vec!["push", "-q", "origin", "main"],
        ] {
            let status = std::process::Command::new("git")
                .args(["-C", work_s])
                .args(&args)
                .envs(env)
                .env_remove("GPG_TTY")
                .status()
                .unwrap();
            assert!(status.success(), "step failed: {args:?}");
        }

        // Point the call's ambient env at a provably-empty keyring + clean
        // git config so the PGP candidate path is exercised.
        let empty_home = tmp.path().join("empty-gnupg");
        std::fs::create_dir_all(&empty_home).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&empty_home, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let empty_gitconfig = tmp.path().join("empty-gitconfig");
        std::fs::write(&empty_gitconfig, "").unwrap();
        let saved: Vec<(&str, Option<std::ffi::OsString>)> =
            ["GNUPGHOME", "GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM"]
                .iter()
                .map(|k| (*k, std::env::var_os(k)))
                .collect();
        std::env::set_var("GNUPGHOME", &empty_home);
        std::env::set_var("GIT_CONFIG_GLOBAL", &empty_gitconfig);
        std::env::set_var("GIT_CONFIG_SYSTEM", "/dev/null");

        let page = git
            .log(&bare, "main", 0, 10, Some(allowed.as_path()), None)
            .await;

        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }

        let page = page.expect("configured log");
        assert_eq!(page.len(), 2);
        let pgp = page
            .iter()
            .find(|c| c.subject == "pgp on top")
            .expect("pgp commit");
        assert_eq!(
            pgp.signature_status, "unknown",
            "expected probe-collapsed 'unknown', got {}",
            pgp.signature_status
        );
        assert_eq!(pgp.signature_kind, "gpg");
        let ssh = page
            .iter()
            .find(|c| c.subject == "signed seed")
            .expect("ssh commit");
        assert_eq!(
            ssh.signature_status, "valid",
            "expected real verification 'valid', got {}",
            ssh.signature_status
        );
        assert_eq!(ssh.signature_kind, "ssh");
    }

    /// GIT-19 helpers — temp bare repo seeded with two files on `main`.
    async fn seeded_bare() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("r.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        git.seed_commit(
            &bare,
            "main",
            "seed",
            &[
                ("README.md".into(), b"hi\n".to_vec()),
                ("docs/a.md".into(), b"doc\n".to_vec()),
            ],
        )
        .await
        .unwrap();
        (dir, bare)
    }

    async fn show_file(bare: &Path, path: &str) -> String {
        let out = Command::new("git")
            .args([
                "-C",
                bare.to_str().unwrap(),
                "show",
                &format!("main:{path}"),
            ])
            .output()
            .await
            .unwrap();
        assert!(out.status.success(), "show {path} failed");
        String::from_utf8(out.stdout).unwrap()
    }

    #[tokio::test]
    async fn commit_files_creates_updates_and_deletes() {
        let (_d, bare) = seeded_bare().await;
        let git = CliGitBackend::new();
        let res = git
            .commit_files(
                &bare,
                "main",
                "main",
                "web edit",
                &[
                    FileChange::Upsert {
                        path: "src/new.txt".into(),
                        content: b"hello".to_vec(),
                    },
                    FileChange::Upsert {
                        path: "README.md".into(),
                        content: b"updated".to_vec(),
                    },
                    FileChange::Delete {
                        path: "docs".into(), // directory delete
                    },
                ],
                "Web User",
                "web@example.com",
                None,
                "admin",
            )
            .await
            .expect("commit_files");
        assert_eq!(res.branch, "main");
        assert!(res.base_sha.is_some());
        assert_eq!(show_file(&bare, "src/new.txt").await, "hello");
        assert_eq!(show_file(&bare, "README.md").await, "updated");
        // Directory delete removed its child; the dir itself is gone.
        assert_eq!(
            git.ls_tree_entry(&bare, "main", "docs").await.expect("ls"),
            None
        );
        // Commit author/committer metadata.
        let log = git.log(&bare, "main", 0, 1, None, None).await.unwrap();
        assert_eq!(log[0].author_name, "Web User");
        assert_eq!(log[0].author_email, "web@example.com");
        assert_eq!(log[0].committer_email, FORGE_NOREPLY_EMAIL);
    }

    #[tokio::test]
    async fn commit_files_empty_file_and_unborn_repo() {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("empty.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        // Root commit on unborn base — empty file included.
        git.commit_files(
            &bare,
            "main",
            "main",
            "first",
            &[
                FileChange::Upsert {
                    path: "blank.txt".into(),
                    content: Vec::new(),
                },
                FileChange::Upsert {
                    path: "dir/.gitkeep".into(),
                    content: Vec::new(),
                },
            ],
            "Oxidean",
            FORGE_NOREPLY_EMAIL,
            None,
            "admin",
        )
        .await
        .expect("root commit");
        assert_eq!(show_file(&bare, "blank.txt").await, "");
        assert_eq!(show_file(&bare, "dir/.gitkeep").await, "");
    }

    #[tokio::test]
    async fn commit_files_rename_via_oid_and_traversal_rejects() {
        let (_d, bare) = seeded_bare().await;
        let git = CliGitBackend::new();
        let entry = git
            .ls_tree_entry(&bare, "main", "README.md")
            .await
            .unwrap()
            .expect("entry");
        assert_eq!(entry.kind, TreeEntryKind::Blob);
        git.commit_files(
            &bare,
            "main",
            "main",
            "mv",
            &[
                FileChange::Delete {
                    path: "README.md".into(),
                },
                FileChange::UpsertOid {
                    path: "docs/README-moved.md".into(),
                    oid: entry.oid.clone(),
                },
            ],
            "Oxidean",
            FORGE_NOREPLY_EMAIL,
            None,
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(show_file(&bare, "docs/README-moved.md").await, "hi\n");
        // blob object id preserved across the rename
        let moved = git
            .ls_tree_entry(&bare, "main", "docs/README-moved.md")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(moved.oid, entry.oid);

        // Write paths reject leading `/` outright (unlike the read-path
        // helper, which normalizes URL-derived inputs).
        assert!(validate_edit_path("/abs/path").is_err());
        for bad in [
            "../etc",
            "a/../../b",
            "a/../b",
            ".git/hook",
            "a//b",
            "x/./y",
            "trailing/",
            "/abs/path",
            "",
        ] {
            let err = git
                .commit_files(
                    &bare,
                    "main",
                    "main",
                    "bad",
                    &[FileChange::Upsert {
                        path: bad.into(),
                        content: b"x".to_vec(),
                    }],
                    "Oxidean",
                    FORGE_NOREPLY_EMAIL,
                    None,
                    "admin",
                )
                .await
                .expect_err("traversal must reject");
            assert!(matches!(err, GitError::InvalidArg(_)), "{bad} → {err:?}");
        }
    }

    #[tokio::test]
    async fn commit_files_new_branch_and_noop_and_race() {
        let (_d, bare) = seeded_bare().await;
        let git = CliGitBackend::new();
        // new branch off main
        let res = git
            .commit_files(
                &bare,
                "feat/x",
                "main",
                "branch commit",
                &[FileChange::Upsert {
                    path: "feat.txt".into(),
                    content: b"f".to_vec(),
                }],
                "Oxidean",
                FORGE_NOREPLY_EMAIL,
                None,
                "admin",
            )
            .await
            .unwrap();
        assert!(res.base_sha.is_some());
        assert_eq!(res.branch, "feat/x");
        // branch exists now
        let e = git
            .commit_files(
                &bare,
                "feat/x",
                "main",
                "again",
                &[FileChange::Upsert {
                    path: "z.txt".into(),
                    content: b"z".to_vec(),
                }],
                "Oxidean",
                FORGE_NOREPLY_EMAIL,
                None,
                "admin",
            )
            .await
            .expect_err("existing branch as new must fail");
        assert!(matches!(e, GitError::Conflict(_)), "{e:?}");

        // no-op changeset (same content) → error, no commit
        let err = git
            .commit_files(
                &bare,
                "main",
                "main",
                "noop",
                &[FileChange::Upsert {
                    path: "README.md".into(),
                    content: b"hi\n".to_vec(),
                }],
                "Oxidean",
                FORGE_NOREPLY_EMAIL,
                None,
                "admin",
            )
            .await
            .expect_err("no-op must error");
        assert!(matches!(err, GitError::InvalidArg(_)), "{err:?}");

        // delete of missing path → NotFound
        let err = git
            .commit_files(
                &bare,
                "main",
                "main",
                "rm missing",
                &[FileChange::Delete {
                    path: "nope.txt".into(),
                }],
                "Oxidean",
                FORGE_NOREPLY_EMAIL,
                None,
                "admin",
            )
            .await
            .expect_err("missing delete must fail");
        assert!(matches!(err, GitError::NotFound(_)), "{err:?}");
    }

    #[tokio::test]
    async fn commit_files_hook_decline_maps_denied() {
        let (_d, bare) = seeded_bare().await;
        let git = CliGitBackend::new();
        // Install a deny-all update hook on the bare repo.
        let hook = bare.join("hooks/update");
        std::fs::write(&hook, "#!/bin/sh\necho denied >&2\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(&hook).unwrap().permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(&hook, p).unwrap();
        }
        let err = git
            .commit_files(
                &bare,
                "main",
                "main",
                "blocked",
                &[FileChange::Upsert {
                    path: "x.txt".into(),
                    content: b"x".to_vec(),
                }],
                "Oxidean",
                FORGE_NOREPLY_EMAIL,
                None,
                "admin",
            )
            .await
            .expect_err("hook decline must surface");
        assert!(matches!(err, GitError::Denied(_)), "{err:?}");
    }

    /// Commit `(message, author_name, author_email, committer_unix_date)` rows
    /// onto `branch` in `bare`, in order, via a scratch clone (seed_commit
    /// creates root commits only, so a real history needs a shared worktree).
    /// An empty committer date uses the wall clock.
    async fn seed_history(bare: &Path, branch: &str, commits: &[(&str, &str, &str, &str)]) {
        let wt = tempfile::tempdir().unwrap();
        let wt_s = wt.path().to_str().unwrap().to_string();
        let bare_s = bare.to_str().unwrap().to_string();
        run_git(&["clone", &bare_s, &wt_s]).await.unwrap();
        run_git(&["-C", &wt_s, "checkout", "-B", branch])
            .await
            .unwrap();
        for (i, &(msg, name, email, date)) in commits.iter().enumerate() {
            let file = wt.path().join(format!("f{i}.txt"));
            std::fs::write(&file, format!("{i}\n")).unwrap();
            run_git(&["-C", &wt_s, "add", "-A"]).await.unwrap();
            let mut env: Vec<(&str, &str)> = vec![
                ("GIT_AUTHOR_NAME", name),
                ("GIT_AUTHOR_EMAIL", email),
                ("GIT_COMMITTER_NAME", "Oxidean"),
                ("GIT_COMMITTER_EMAIL", FORGE_NOREPLY_EMAIL),
            ];
            if !date.is_empty() {
                env.push(("GIT_AUTHOR_DATE", date));
                env.push(("GIT_COMMITTER_DATE", date));
            }
            run_git_stdout_env(&["-C", &wt_s, "commit", "-m", msg], &env)
                .await
                .unwrap();
        }
        run_git(&["-C", &wt_s, "push", "origin", &format!("HEAD:{branch}")])
            .await
            .unwrap();
    }

    fn now_ts() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[tokio::test]
    async fn contributor_scan_aggregates_and_reports_truncation() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("scan.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        seed_history(
            &bare,
            "main",
            &[
                ("a1", "Alice", "alice@ex.com", ""),
                ("a2", "Alice", "alice@ex.com", ""),
                ("a3", "Alice", "alice@ex.com", ""),
                ("b1", "Bob", "bob@ex.com", ""),
            ],
        )
        .await;

        let full = git
            .contributor_scan(&bare, "main", 10, 100)
            .await
            .expect("full scan");
        assert_eq!(full.scanned_commits, 4);
        assert!(!full.truncated);
        let alice = full
            .contributors
            .iter()
            .find(|c| c.email == "alice@ex.com")
            .expect("alice row");
        assert_eq!(alice.name, "Alice");
        assert_eq!(alice.commit_count, 3);
        // Three Alice commits: first/last differ and timestamps are ordered.
        assert_ne!(alice.first_commit_sha, alice.last_commit_sha);
        assert!(alice.first_commit_unix <= alice.last_commit_unix);
        assert!(alice.first_commit_unix > 0);
        let bob = full
            .contributors
            .iter()
            .find(|c| c.email == "bob@ex.com")
            .expect("bob row");
        assert_eq!(bob.commit_count, 1);
        // Single-commit author: first == last.
        assert_eq!(bob.first_commit_sha, bob.last_commit_sha);
        assert_eq!(bob.first_commit_unix, bob.last_commit_unix);

        // Only the two newest commits are walked when the cap clips history.
        let clipped = git
            .contributor_scan(&bare, "main", 10, 2)
            .await
            .expect("clipped scan");
        assert_eq!(clipped.scanned_commits, 2);
        assert!(clipped.truncated);
        let scanned: i64 = clipped.contributors.iter().map(|c| c.commit_count).sum();
        assert_eq!(scanned, 2);

        // Unborn / missing ref -> empty scan.
        let empty = git
            .contributor_scan(&bare, "refs/heads/nope", 10, 100)
            .await
            .expect("missing ref");
        assert!(empty.contributors.is_empty());
        assert_eq!(empty.scanned_commits, 0);
        assert!(!empty.truncated);
    }

    #[tokio::test]
    async fn commit_times_returns_bounded_newest_first_timestamps() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("times.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        seed_history(
            &bare,
            "main",
            &[
                ("c1", "Alice", "alice@ex.com", "@1700000000"),
                ("c2", "Alice", "alice@ex.com", ""),
                ("c3", "Alice", "alice@ex.com", ""),
            ],
        )
        .await;

        let res = git
            .commit_times(&bare, "main", Some(0), 100)
            .await
            .expect("commit_times");
        assert_eq!(res.times.len(), 3);
        assert!(!res.truncated);
        assert!(res.times.iter().all(|t| *t > 0));
        // Newest-first order.
        assert!(res.times.windows(2).all(|w| w[0] >= w[1]));

        // `--since` between commits excludes the older history. (git still
        // emits the boundary tip when `since` exceeds it — the API layer
        // buckets times itself, so a leaked tip is harmless.)
        let windowed = git
            .commit_times(&bare, "main", Some(now_ts() - 60), 100)
            .await
            .expect("windowed since");
        assert_eq!(windowed.times.len(), 2, "{:?}", windowed.times);

        // Cap clips the oldest commits out and marks truncation.
        let capped = git
            .commit_times(&bare, "main", Some(0), 2)
            .await
            .expect("capped");
        assert_eq!(capped.times.len(), 2);
        assert!(capped.truncated);

        // Missing ref -> empty, not an error.
        let missing = git
            .commit_times(&bare, "refs/heads/nope", None, 100)
            .await
            .expect("missing ref");
        assert!(missing.times.is_empty());
        assert!(!missing.truncated);
    }
}
