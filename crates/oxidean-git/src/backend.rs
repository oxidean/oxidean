//! `GitBackend` seam — all forge git ops go through this trait (GIT-09 / GIT-10 / D-32).
//!
//! Phase 7 ships only [`crate::cli::CliGitBackend`] (system `git` CLI ≥ 2.5).
//! A future `GixGitBackend` (gitoxide) should implement this same trait when
//! coverage reaches create/browse/branch/archive/gc parity — do not call `gix`
//! from API handlers.

use std::path::Path;

use thiserror::Error;

/// How to authenticate to an external git remote (two-way mirroring).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteAuthKind {
    /// HTTPS Basic with token/password (PAT, app password, oauth2 token).
    HttpsToken,
    /// SSH private key + known_hosts (deploy key).
    SshKey,
}

/// Ephemeral credentials for one outbound git URL operation.
///
/// Callers own secret lifetime; the CLI adapter materializes temp askpass /
/// key / known_hosts files for the duration of a single command.
#[derive(Debug, Clone)]
pub struct RemoteCredentials {
    pub kind: RemoteAuthKind,
    /// HTTPS username (often a token username or `oauth2` / `git`).
    pub username: Option<String>,
    /// HTTPS password/token, or SSH private key PEM (OpenSSH format).
    pub secret: String,
    /// OpenSSH `known_hosts` file contents (required for SSH).
    pub known_hosts: Option<String>,
}

/// Validate a remote git URL for mirroring (reject dangerous schemes/paths).
pub fn validate_remote_url(url: &str) -> Result<&str, GitError> {
    let u = url.trim();
    if u.is_empty() {
        return Err(GitError::InvalidArg("remote URL is required".into()));
    }
    if u.contains('\0') || u.contains('\n') || u.contains('\r') {
        return Err(GitError::InvalidArg("remote URL contains invalid characters".into()));
    }
    let lower = u.to_ascii_lowercase();
    if lower.starts_with("file:")
        || lower.starts_with("ext::")
        || lower.starts_with("fd:")
        || Path::new(u).is_absolute()
        || u.starts_with('.')
        || u.starts_with('/')
    {
        return Err(GitError::InvalidArg(
            "remote URL must be https://, http://, ssh://, or git@host:path".into(),
        ));
    }
    let ok = lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("ssh://")
        || lower.starts_with("git@");
    if !ok {
        return Err(GitError::InvalidArg(
            "remote URL must be https://, http://, ssh://, or git@host:path".into(),
        ));
    }
    Ok(u)
}

/// Extract SSH hostname from an SSH-style remote URL for `ssh-keyscan`.
/// Accepts `git@host:path` and `ssh://[user@]host[:port]/path`. Rejects HTTPS/HTTP.
pub fn ssh_host_from_remote_url(url: &str) -> Result<String, GitError> {
    let u = validate_remote_url(url)?;
    let lower = u.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return Err(GitError::InvalidArg(
            "fetch host key requires an SSH remote (git@host:path or ssh://…)".into(),
        ));
    }
    if let Some(rest) = u.strip_prefix("ssh://") {
        // ssh://[user@]host[:port]/path
        let after_auth = rest.split_once('@').map(|(_, h)| h).unwrap_or(rest);
        let hostport = after_auth.split('/').next().unwrap_or(after_auth);
        let host = hostport
            .rsplit_once(':')
            .and_then(|(h, port)| {
                if port.chars().all(|c| c.is_ascii_digit()) {
                    Some(h)
                } else {
                    None
                }
            })
            .unwrap_or(hostport);
        let host = host.trim().trim_start_matches('[').trim_end_matches(']');
        if host.is_empty() {
            return Err(GitError::InvalidArg("could not parse SSH host from URL".into()));
        }
        return Ok(host.to_string());
    }
    // git@host:path
    if let Some(rest) = u.strip_prefix("git@") {
        let host = rest.split_once(':').map(|(h, _)| h).unwrap_or(rest).trim();
        if host.is_empty() {
            return Err(GitError::InvalidArg("could not parse SSH host from URL".into()));
        }
        return Ok(host.to_string());
    }
    Err(GitError::InvalidArg(
        "fetch host key requires an SSH remote (git@host:path or ssh://…)".into(),
    ))
}

/// Errors from git backend operations (CLI or future gitoxide adapter).
#[derive(Debug, Error)]
pub enum GitError {
    #[error("git process failed: {0}")]
    Process(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid argument: {0}")]
    InvalidArg(String),
    /// Object/path/ref missing (empty repo, bad path, etc.).
    #[error("not found: {0}")]
    NotFound(String),
    /// Ref tip moved between read and update — caller should re-read and retry
    /// (CAS / fast-forward race).
    #[error("conflict: {0}")]
    Conflict(String),
    /// The update was refused by a repository hook (e.g. branch protection
    /// `hooks/update` decline) after transport-level checks passed.
    #[error("denied: {0}")]
    Denied(String),
}

/// Kind of a tree entry (`git ls-tree` object type).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeEntryKind {
    Blob,
    Tree,
    /// Gitlink / submodule (`160000`).
    Commit,
}

impl TreeEntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blob => "blob",
            Self::Tree => "tree",
            Self::Commit => "commit",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "blob" => Some(Self::Blob),
            "tree" => Some(Self::Tree),
            "commit" => Some(Self::Commit),
            _ => None,
        }
    }
}

/// One entry from `git ls-tree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub mode: String,
    pub kind: TreeEntryKind,
    pub oid: String,
    pub name: String,
}

/// A ref from `git for-each-ref` / `show-ref`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRef {
    pub name: String,
    pub oid: String,
    /// Tip commit author name (`%(authorname)`); absent for annotated tag objects.
    pub tip_author_name: Option<String>,
    /// Tip commit committer date, ISO-strict (`%(committerdate:iso-strict)`).
    pub tip_committed_at: Option<String>,
}

/// One commit from `git log` (paged history).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub sha: String,
    pub short_sha: String,
    pub subject: String,
    pub author_name: String,
    pub author_email: String,
    /// Author date as ISO-8601 (`%aI`).
    pub authored_at: String,
    /// Committer email (`%ce`) — used for signature verification policy.
    pub committer_email: String,
    /// `none` | `valid` | `invalid` | `unknown` (from `%G?` + optional allowedSigners).
    pub signature_status: String,
    /// `ssh` | `gpg` | empty when unsigned / unknown.
    pub signature_kind: String,
}

/// Aggregated author from `git shortlog` (issue #23 contributors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributorSummary {
    pub name: String,
    pub email: String,
    pub commit_count: i64,
}

/// Blob path + byte size from `git ls-tree -r -l` (About language stats).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizedBlobEntry {
    pub path: String,
    pub size: u64,
}

/// One file in a commit or compare diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFile {
    pub path: String,
    /// `added` | `modified` | `deleted` | `renamed` | `copied` | `unknown`
    pub status: String,
    /// Unified diff hunk text (may be truncated by soft caps).
    pub patch: String,
}

/// Full commit detail for `/commit/{sha}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDetail {
    pub sha: String,
    pub short_sha: String,
    pub subject: String,
    pub body: String,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
    pub committer_email: String,
    pub parents: Vec<String>,
    pub files: Vec<DiffFile>,
    /// True when patch payload was soft-capped (D-20 / T-07-18).
    pub truncated: bool,
    pub signature_status: String,
    pub signature_kind: String,
}

/// Compare `base...head` (or empty when identical).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffResult {
    pub base: String,
    pub head: String,
    pub files: Vec<DiffFile>,
    pub empty: bool,
    pub truncated: bool,
}

/// One `git grep -n` hit (Phase 16 / GIT-18 / D-SRCH-06).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepHit {
    pub path: String,
    pub line: u32,
    pub content: String,
}

/// Aggregated grep results with soft truncation flag (D-SRCH-08).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepResult {
    pub hits: Vec<GrepHit>,
    pub truncated: bool,
}

/// One blame line (text files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    pub sha: String,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
    pub line_number: u32,
    pub content: String,
}

/// Blame for a path at a ref.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameFile {
    pub path: String,
    pub ref_name: String,
    pub lines: Vec<BlameLine>,
    pub truncated: bool,
}

/// Soft cap for unified patch bytes in show/diff responses (D-20 / T-07-18).
pub const DIFF_SOFT_MAX_BYTES: usize = 1_048_576;
/// Soft cap for blame line count (D-20).
pub const BLAME_SOFT_MAX_LINES: usize = 10_000;

/// Soft timeout for `git archive` (T-07-22).
pub const ARCHIVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Forge web-flow identity used as committer (and default author) on
/// forge-authored seed commits. It is not a registered account address — the
/// instance signs these commits with the web-flow SSH key under this principal.
pub const FORGE_NOREPLY_EMAIL: &str = "noreply@oxidean.local";

/// One file mutation inside a [`GitBackend::commit_files`] changeset (GIT-19).
///
/// Paths are repository-relative, normalized, and must not escape the repo
/// (no `..`, no absolute/prefix components, no `.git` segment, no NUL).
/// Directories are never passed explicitly — Git tracks files only; deleting
/// `dir` removes every blob under `dir/` (see [`FileChange::Delete`]).
#[derive(Debug, Clone)]
pub enum FileChange {
    /// Create or overwrite `path` with `content` bytes (mode `100644`).
    /// An empty `content` creates an empty blob — a legitimate empty file.
    Upsert { path: String, content: Vec<u8> },
    /// Stage an existing blob `oid` (full hex) at `path` — renames preserve
    /// blob identity so history/diffs stay clean.
    UpsertOid { path: String, oid: String },
    /// Remove `path` — the file itself, or every blob under `path/` when it
    /// names a directory. Missing path → [`GitError::NotFound`].
    Delete { path: String },
}

/// Result of [`GitBackend::commit_files`].
#[derive(Debug, Clone)]
pub struct FilesCommit {
    /// New commit SHA.
    pub sha: String,
    /// Tip of `base` the commit was built on (`None` = root commit on an
    /// unborn base — first commit of an empty repository).
    pub base_sha: Option<String>,
    /// Branch (short name) that received the commit.
    pub branch: String,
}

/// Source archive format for [`GitBackend::archive`] (GIT-07 / D-29).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
}

impl ArchiveFormat {
    pub fn as_git_format(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Self::Zip => "application/zip",
            Self::TarGz => "application/gzip",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
        }
    }
}

/// Async forge git operations. API/RPC never shell out directly.
#[async_trait::async_trait]
pub trait GitBackend: Send + Sync {
    /// Create a bare repository at `path` and point unborn `HEAD` at
    /// `refs/heads/{initial_branch}` (Git 2.5-compatible; no `--initial-branch`).
    async fn init_bare(&self, path: &Path, initial_branch: &str) -> Result<(), GitError>;

    /// Write `files` (relative path → bytes) as a single commit on `branch` in an
    /// existing bare repo (temp worktree + push). No-op when `files` is empty.
    /// Uses Oxidean noreply identity (tests / system seeds).
    async fn seed_commit(
        &self,
        bare_path: &Path,
        branch: &str,
        message: &str,
        files: &[(String, Vec<u8>)],
    ) -> Result<(), GitError>;

    /// Like [`seed_commit`], but author is `author_name`/`author_email`, committer is
    /// Oxidean noreply, and optionally SSH-signs with `signing_key_path` (`gpg.format=ssh`).
    async fn seed_commit_authored(
        &self,
        bare_path: &Path,
        branch: &str,
        message: &str,
        files: &[(String, Vec<u8>)],
        author_name: &str,
        author_email: &str,
        signing_key_path: Option<&Path>,
    ) -> Result<(), GitError>;

    /// Apply `changes` as a single commit on `branch` (GIT-19 web file editing).
    ///
    /// `base` is the branch whose tip seeds the starting tree and parents the
    /// commit — for a plain in-place commit pass `base == branch`; to create
    /// `branch` from another tip pass the source branch. A missing `base`
    /// produces a root commit (first commit of an empty repository).
    /// `branch` must not already exist when `branch != base`.
    ///
    /// Implementations apply changes via index plumbing (no worktree
    /// checkout), then publish the commit through a push so bare
    /// `hooks/update` (branch protection) still runs — non-fast-forward races
    /// surface as [`GitError::Conflict`], hook declines as
    /// [`GitError::Denied`]. Author is `author_name`/`author_email`; committer
    /// is the forge identity; `signing_key_path` SSH-signs (`gpg.format=ssh`)
    /// when set. A changeset that produces a tree identical to `base`'s is a
    /// [`GitError::InvalidArg`] no-op rather than an empty commit.
    ///
    /// `actor_capability` is the pusher's forge capability label
    /// (`admin`/`write`/`read`) exported to `hooks/update` as
    /// `OXIDEAN_ACTOR_CAPABILITY` — pass the real actor (or `admin` for
    /// system-initiated commits that already cleared API-layer policy).
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
    ) -> Result<FilesCommit, GitError>;

    /// `git ls-tree <treeish> -- <path>` — the entry for `path` itself (blob /
    /// tree / commit gitlink), not its children. `Ok(None)` when `treeish` or
    /// `path` does not resolve (missing path, unborn ref).
    async fn ls_tree_entry(
        &self,
        repo: &Path,
        treeish: &str,
        path: &str,
    ) -> Result<Option<TreeEntry>, GitError>;

    /// List tree entries at `path` under `treeish` (branch/tag/sha). Empty repo /
    /// unborn HEAD → `Ok(vec![])` (not an error).
    async fn ls_tree(
        &self,
        repo: &Path,
        treeish: &str,
        path: &str,
    ) -> Result<Vec<TreeEntry>, GitError>;

    /// Read blob bytes at `path` for `treeish`. Missing path → [`GitError::NotFound`].
    async fn cat_blob(
        &self,
        repo: &Path,
        treeish: &str,
        path: &str,
    ) -> Result<Vec<u8>, GitError>;

    /// List refs under `refs/heads` and `refs/tags` (name + oid). Empty → `Ok(vec![])`.
    async fn list_refs(&self, repo: &Path) -> Result<Vec<GitRef>, GitError>;

    /// Paged `git log` for `refname` (`skip` / `limit`). Empty history → `Ok(vec![])`.
    /// When `allowed_signers` is set, configures `gpg.ssh.allowedSignersFile` for `%G?`.
    /// When `gpg_home` is set, sets `GNUPGHOME` + `gpg.trustModel=always` for OpenPGP verify.
    async fn log(
        &self,
        repo: &Path,
        refname: &str,
        skip: u32,
        limit: u32,
        allowed_signers: Option<&Path>,
        gpg_home: Option<&Path>,
    ) -> Result<Vec<CommitSummary>, GitError>;

    /// Commit metadata + per-file unified patches (`git show`).
    async fn show_commit(
        &self,
        repo: &Path,
        sha: &str,
        allowed_signers: Option<&Path>,
        gpg_home: Option<&Path>,
    ) -> Result<CommitDetail, GitError>;

    /// Unified diff `base...head`. Identical trees → `empty: true` (not an error).
    async fn diff(
        &self,
        repo: &Path,
        base: &str,
        head: &str,
    ) -> Result<DiffResult, GitError>;

    /// Per-line blame for a text file (`git blame --line-porcelain`).
    async fn blame(
        &self,
        repo: &Path,
        refname: &str,
        path: &str,
    ) -> Result<BlameFile, GitError>;

    /// Create branch `name` pointing at `start` (branch/tag/sha).
    async fn branch_create(
        &self,
        repo: &Path,
        name: &str,
        start: &str,
    ) -> Result<(), GitError>;

    /// Rename local branch `from` → `to`.
    async fn branch_rename(&self, repo: &Path, from: &str, to: &str) -> Result<(), GitError>;

    /// Delete local branch `name` (`git branch -D`).
    async fn branch_delete(&self, repo: &Path, name: &str) -> Result<(), GitError>;

    /// Build a source archive (`git archive`) for `treeish` with `--prefix={prefix}/`.
    /// Empty / unborn refs → [`GitError::NotFound`] (not a panic).
    async fn archive(
        &self,
        repo: &Path,
        treeish: &str,
        format: ArchiveFormat,
        prefix: &str,
    ) -> Result<Vec<u8>, GitError>;

    /// Run `git gc` on a bare (or worktree) repository (D-37).
    async fn gc(&self, repo: &Path) -> Result<(), GitError>;

    /// Create a merge commit of `head_sha` into `base_ref` on a bare repo.
    /// Returns the resulting tip SHA on `base_ref`. Conflicts → [`GitError::Process`].
    async fn merge_commit(
        &self,
        repo: &Path,
        base_ref: &str,
        head_sha: &str,
        message: &str,
    ) -> Result<String, GitError>;

    /// Squash `head_sha` onto `base_ref` as a single commit.
    async fn squash_merge(
        &self,
        repo: &Path,
        base_ref: &str,
        head_sha: &str,
        message: &str,
    ) -> Result<String, GitError>;

    /// Rebase commits reachable from `head_sha` (not in `base_ref`) onto `base_ref`,
    /// then fast-forward `base_ref` to the rebased tip.
    async fn rebase_merge(
        &self,
        repo: &Path,
        base_ref: &str,
        head_sha: &str,
    ) -> Result<String, GitError>;

    /// Fetch objects for `refname` from another bare repo into `dest` (fork heads).
    async fn fetch_ref_from(
        &self,
        dest: &Path,
        source: &Path,
        refname: &str,
    ) -> Result<String, GitError>;

    /// Clone `source` bare repo into a new bare `dest` (minimal fork).
    async fn clone_bare(&self, source: &Path, dest: &Path) -> Result<(), GitError>;

    /// Search file contents with `git grep -n -I` on `treeish` (D-SRCH-06 / D-SRCH-08).
    /// Empty pattern or no matches → empty `hits` (not an error). Exit code 1 from git
    /// grep (no match) is mapped to empty. Soft-caps at `max_matches` and sets `truncated`.
    /// `pathspecs` OR together after `--` (git pathspec magic like `:(icase)` allowed);
    /// empty slice searches the whole tree.
    async fn grep(
        &self,
        repo: &Path,
        treeish: &str,
        pattern: &str,
        pathspecs: &[String],
        max_matches: u32,
    ) -> Result<GrepResult, GitError>;

    /// Search commits via `git log --grep` / `--author` (D-SRCH-07). Empty → `Ok(vec![])`.
    async fn log_search(
        &self,
        repo: &Path,
        refname: &str,
        grep: Option<&str>,
        author: Option<&str>,
        skip: u32,
        limit: u32,
    ) -> Result<Vec<CommitSummary>, GitError>;

    /// Last commit touching `path` under `refname` (`git log -1 -- <path>`).
    /// Missing/empty history → `Ok(vec![])`.
    async fn log_path(
        &self,
        repo: &Path,
        refname: &str,
        path: &str,
        limit: u32,
    ) -> Result<Vec<CommitSummary>, GitError>;

    /// Last commit per directory entry name (batched; concurrency-capped).
    /// Keys are bare entry names (not full paths). Missing entries omitted.
    async fn path_last_commits(
        &self,
        repo: &Path,
        refname: &str,
        dir_path: &str,
        entry_names: &[String],
    ) -> Result<std::collections::HashMap<String, CommitSummary>, GitError>;

    /// `git rev-list --count <refname>`. Unborn → 0.
    async fn rev_list_count(&self, repo: &Path, refname: &str) -> Result<u64, GitError>;

    /// `git shortlog -sn -e` contributors for `refname`, capped to `limit` (max 100).
    async fn shortlog(
        &self,
        repo: &Path,
        refname: &str,
        limit: u32,
    ) -> Result<Vec<ContributorSummary>, GitError>;

    /// Recursive `git ls-tree -r -l` blob paths + sizes for language stats.
    /// Empty / unborn → `Ok(vec![])`. Soft-capped by `max_entries`.
    async fn ls_tree_sized_blobs(
        &self,
        repo: &Path,
        treeish: &str,
        max_entries: u32,
    ) -> Result<Vec<SizedBlobEntry>, GitError>;

    /// `git ls-remote --heads --tags <url>` — remote tips only (no objects).
    async fn ls_remote_url(
        &self,
        url: &str,
        credentials: &RemoteCredentials,
    ) -> Result<Vec<GitRef>, GitError>;

    /// Fetch heads+tags from `url` into `refs/oxidean/mirror/{heads,tags}/*` on bare `dest`.
    /// Never uses `--force` / `--mirror` on push; fetch refspecs overwrite only the mirror namespace.
    async fn fetch_from_url(
        &self,
        dest: &Path,
        url: &str,
        credentials: &RemoteCredentials,
    ) -> Result<(), GitError>;

    /// Push an explicit refspec to `url` (**no** `--force` / `--mirror`).
    /// `local_ref` and `remote_ref` are full ref names (e.g. `refs/heads/main`).
    async fn push_to_url(
        &self,
        repo: &Path,
        url: &str,
        credentials: &RemoteCredentials,
        local_ref: &str,
        remote_ref: &str,
    ) -> Result<(), GitError>;

    /// Push with force (`+local:remote`) or delete (`:remote` when `local_ref` is empty).
    /// Exact-sync only — never used by merge mode.
    async fn push_to_url_force(
        &self,
        repo: &Path,
        url: &str,
        credentials: &RemoteCredentials,
        local_ref: &str,
        remote_ref: &str,
    ) -> Result<(), GitError>;

    /// Force-update (or create) a local ref to `target_sha` via worktree push with `+`.
    /// Heads use branch push; tags use force tag push.
    async fn force_update_ref(
        &self,
        repo: &Path,
        refname: &str,
        target_sha: &str,
    ) -> Result<(), GitError>;

    /// Committer unix timestamp (`git log -1 --format=%ct`) for LWW tip comparison.
    async fn committer_unix_time(&self, repo: &Path, sha: &str) -> Result<i64, GitError>;

    /// Bare clone from a remote URL into a new bare `dest` (first import).
    async fn clone_bare_url(
        &self,
        url: &str,
        dest: &Path,
        credentials: &RemoteCredentials,
    ) -> Result<(), GitError>;

    /// True when `maybe_ancestor` is an ancestor of `tip` (`git merge-base --is-ancestor`).
    async fn is_ancestor(
        &self,
        repo: &Path,
        maybe_ancestor: &str,
        tip: &str,
    ) -> Result<bool, GitError>;

    /// Fast-forward (or create) `refname` to `target_sha` via a worktree push so
    /// bare `hooks/update` runs. Non-FF → [`GitError::Process`].
    async fn fast_forward_ref(
        &self,
        repo: &Path,
        refname: &str,
        target_sha: &str,
    ) -> Result<(), GitError>;

    /// Resolve a ref / SHA to a commit OID (`git rev-parse`).
    async fn rev_parse(&self, repo: &Path, rev: &str) -> Result<String, GitError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_host_from_scp_style() {
        assert_eq!(
            ssh_host_from_remote_url("git@github.com:Org/repo.git").unwrap(),
            "github.com"
        );
    }

    #[test]
    fn ssh_host_from_ssh_url_with_port() {
        assert_eq!(
            ssh_host_from_remote_url("ssh://git@gitlab.example:2222/org/repo.git").unwrap(),
            "gitlab.example"
        );
    }

    #[test]
    fn ssh_host_rejects_https() {
        assert!(ssh_host_from_remote_url("https://github.com/Org/repo.git").is_err());
    }
}
