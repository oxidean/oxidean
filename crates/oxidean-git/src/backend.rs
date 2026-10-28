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

/// Egress-validated DNS answers for an HTTP(S) remote — pins git's HTTP
/// transport (`http.curloptResolve`) to the checked addresses so it cannot
/// re-resolve to an unvalidated host between egress validation and connect
/// (DNS-rebind TOCTOU). SSH remotes resolve inside OpenSSH and cannot be
/// pinned here; that residual bound is documented in CONFIGURATION.md.
#[derive(Debug, Clone, Default)]
pub struct RemoteDnsPin {
    /// Remote hostname — always a domain; literal IPs have no DNS to pin.
    pub host: String,
    /// Remote port.
    pub port: u16,
    /// Validated answers.
    pub addrs: Vec<std::net::IpAddr>,
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
    /// Optional egress pin — set by callers that already resolved and
    /// validated the remote's DNS answers (HTTP(S) only).
    pub dns_pin: Option<RemoteDnsPin>,
}

/// Validate a remote git URL for mirroring (reject dangerous schemes/paths).
pub fn validate_remote_url(url: &str) -> Result<&str, GitError> {
    let u = url.trim();
    if u.is_empty() {
        return Err(GitError::InvalidArg("remote URL is required".into()));
    }
    if u.contains('\0') || u.contains('\n') || u.contains('\r') {
        return Err(GitError::InvalidArg(
            "remote URL contains invalid characters".into(),
        ));
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
            return Err(GitError::InvalidArg(
                "could not parse SSH host from URL".into(),
            ));
        }
        return Ok(host.to_string());
    }
    // git@host:path
    if let Some(rest) = u.strip_prefix("git@") {
        let host = rest.split_once(':').map(|(h, _)| h).unwrap_or(rest).trim();
        if host.is_empty() {
            return Err(GitError::InvalidArg(
                "could not parse SSH host from URL".into(),
            ));
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

/// Per-author aggregate from [`GitBackend::contributor_scan`] (GIT-26 insights).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributorStat {
    /// Author name from the newest scanned commit (`%aN`, mailmap-aware).
    pub name: String,
    /// Author email from the newest scanned commit (`%aE`, mailmap-aware).
    pub email: String,
    pub commit_count: i64,
    /// Oldest scanned commit by this author.
    pub first_commit_sha: String,
    /// Oldest scanned commit committer unix timestamp (`%ct`).
    pub first_commit_unix: i64,
    /// Newest scanned commit by this author.
    pub last_commit_sha: String,
    /// Newest scanned commit committer unix timestamp (`%ct`).
    pub last_commit_unix: i64,
}

/// Bounded contributor scan over a ref's history (GIT-26 insights).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributorScan {
    /// Top authors by commit count within the scanned window.
    pub contributors: Vec<ContributorStat>,
    /// Commits actually walked (`min(total_commits, max_commits)`).
    pub scanned_commits: u64,
    /// True when the ref's history exceeds `max_commits` and the scan was clipped.
    pub truncated: bool,
}

/// Bounded committer-timestamp walk for weekly commit-activity buckets (GIT-26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitTimesResult {
    /// Unix committer timestamps (`%ct`), newest first, capped at `max_commits`.
    pub times: Vec<i64>,
    /// True when the window held more commits than `max_commits`.
    pub truncated: bool,
}

/// Blob path + byte size from `git ls-tree -r -l` (About language stats).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizedBlobEntry {
    pub path: String,
    pub size: u64,
}

/// One entry of a push's commit list — metadata plus the per-file change
/// sets Actions `push` event payloads carry (`commits[].added/removed/modified`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushEventCommit {
    pub sha: String,
    pub subject: String,
    pub author_name: String,
    pub author_email: String,
    /// ISO-8601 author date (`%aI`).
    pub authored_at: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
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

// ---------------------------------------------------------------------------
// VCS git-interop surface (u6) — plumbing for the side-by-side changeset
// protocol's import/export path. Byte-tolerant: git permits non-UTF8 ident
// and message bytes, and round-trip fidelity requires preserving them.
// ---------------------------------------------------------------------------

/// A raw `author`/`committer` line decoded from a commit object. Byte-exact —
/// name/email may hold non-UTF8 or angle-bracket bytes git tolerates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitIdent {
    pub name: Vec<u8>,
    pub email: Vec<u8>,
    pub unix: i64,
    /// Timezone as written (`+0200`, `-0330`, `Z`); re-emitted verbatim.
    pub tz: String,
}

/// A parsed `git cat-file commit` object — folded continuation headers are
/// flattened away; everything after the blank line is the message verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCommit {
    pub tree: String,
    pub parents: Vec<String>,
    pub author: GitIdent,
    pub committer: GitIdent,
    pub message: Vec<u8>,
}

/// `git log --name-status` change kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameStatusKind {
    Add,
    Modify,
    Delete,
    Rename,
    TypeChange,
}

impl NameStatusKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Modify => "modify",
            Self::Delete => "delete",
            Self::Rename => "rename",
            Self::TypeChange => "typechange",
        }
    }
}

/// One row of a `diff-tree --name-status -z` read. Rename rows carry both
/// paths — `old_path` is the `-z` first record (source).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameStatusRow {
    pub status: NameStatusKind,
    pub path: String,
    pub old_path: Option<String>,
}

/// One leaf of a `ls-tree -r -l -z --full-tree` walk — mode+kind+size per
/// entry so callers can apply byte caps before touching blobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatTreeEntry {
    /// Octal mode as git writes it (`100644`, `100755`, `120000`, `160000`).
    pub mode: String,
    pub kind: TreeEntryKind,
    pub oid: String,
    pub path: String,
    /// `0` for non-blob kinds (trees/gitlinks report `-`).
    pub size: u64,
}

/// One input row for `mktree -z` — full-snapshot semantics; `mode` is the
/// zero-padded octal git accepts (`{:06o}` form).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeWriteEntry {
    pub mode: String,
    pub oid: String,
    pub path: String,
}

/// `rev-list` page — `oids` arrive PARENTS-FIRST (the adapter reverses
/// `--topo-order` output); `truncated` reports whether `max` cut the walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevList {
    pub oids: Vec<String>,
    pub truncated: bool,
}

#[allow(clippy::too_many_arguments)]
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

    /// Like [`Self::seed_commit`], but author is `author_name`/`author_email`, committer is
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
    async fn cat_blob(&self, repo: &Path, treeish: &str, path: &str) -> Result<Vec<u8>, GitError>;

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
    async fn diff(&self, repo: &Path, base: &str, head: &str) -> Result<DiffResult, GitError>;

    /// Per-line blame for a text file (`git blame --line-porcelain`).
    async fn blame(&self, repo: &Path, refname: &str, path: &str) -> Result<BlameFile, GitError>;

    /// Create branch `name` pointing at `start` (branch/tag/sha).
    async fn branch_create(&self, repo: &Path, name: &str, start: &str) -> Result<(), GitError>;

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

    /// Build a test-merge commit of `head_sha` into `base_sha` inside `repo`
    /// without moving any branch — `merge-tree --write-tree` + `commit-tree`
    /// with parents `(base_sha, head_sha)`. Conflicts → [`GitError::Process`].
    /// Callers publish the result via [`update_ref`](Self::update_ref)
    /// (e.g. `refs/pull/N/merge`).
    async fn test_merge_commit(
        &self,
        repo: &Path,
        base_sha: &str,
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

    /// `git log --name-status <new> --not <old>` — commits a push introduces,
    /// each with the per-file added/removed/modified sets Actions payloads and
    /// `paths` filters need. `old` empty/all-zeros → full history up to `new`.
    /// Bounded by `limit` (GitHub caps `push.commits` at 2048).
    async fn push_event_commits(
        &self,
        repo: &Path,
        old: &str,
        new: &str,
        limit: u32,
    ) -> Result<Vec<PushEventCommit>, GitError>;

    /// `git rev-list --count <refname>`. Unborn → 0.
    async fn rev_list_count(&self, repo: &Path, refname: &str) -> Result<u64, GitError>;

    /// `git shortlog -sn -e` contributors for `refname`, capped to `limit` (max 100).
    async fn shortlog(
        &self,
        repo: &Path,
        refname: &str,
        limit: u32,
    ) -> Result<Vec<ContributorSummary>, GitError>;

    /// Per-author commit stats over at most `max_commits` commits of `refname`
    /// (`git log` author fields aggregated per author email) with truncation
    /// metadata for the Insights page (GIT-26). Sorted by commit count desc,
    /// then most-recent commit. Unborn/missing ref -> empty scan.
    async fn contributor_scan(
        &self,
        repo: &Path,
        refname: &str,
        limit: u32,
        max_commits: u64,
    ) -> Result<ContributorScan, GitError>;

    /// Committer timestamps (`git log --format=%ct`) for `refname`, newest
    /// first, bounded by `max_commits`. A positive `since_unix` adds a
    /// `--since` floor (commit date); `<= 0` scans the full capped history.
    /// Unborn/missing ref -> empty result.
    async fn commit_times(
        &self,
        repo: &Path,
        refname: &str,
        since_unix: Option<i64>,
        max_commits: u64,
    ) -> Result<CommitTimesResult, GitError>;

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
    /// Heads use branch push; tags use force tag push. `actor_capability`
    /// threads into `OXIDEAN_ACTOR_CAPABILITY` for the protection hook —
    /// system ref moves pass `"admin"` (they already passed API-side policy).
    async fn force_update_ref(
        &self,
        repo: &Path,
        refname: &str,
        target_sha: &str,
        actor_capability: &str,
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

    /// `(ahead, behind)` commit counts between `head` and `base`
    /// (`git rev-list --left-right --count head...base`): commits reachable
    /// from `head` but not `base`, then commits reachable from `base` but
    /// not `head`. Both revs must resolve to commits in `repo` (fetch first
    /// for cross-repo comparisons).
    async fn ahead_behind(
        &self,
        repo: &Path,
        head: &str,
        base: &str,
    ) -> Result<(u64, u64), GitError>;

    /// Fast-forward (or create) `refname` to `target_sha` via a worktree push so
    /// bare `hooks/update` runs. Non-FF → [`GitError::Conflict`]; hook decline →
    /// [`GitError::Denied`]. `actor_capability` threads into
    /// `OXIDEAN_ACTOR_CAPABILITY` — system ref moves pass `"admin"`.
    async fn fast_forward_ref(
        &self,
        repo: &Path,
        refname: &str,
        target_sha: &str,
        actor_capability: &str,
    ) -> Result<(), GitError>;

    /// Resolve a ref / SHA to a commit OID (`git rev-parse`).
    async fn rev_parse(&self, repo: &Path, rev: &str) -> Result<String, GitError>;

    /// Point `refname` at `sha` via `git update-ref` — a direct write that does
    /// **not** run `hooks/update`. Reserved for forge-synthesized namespaces
    /// (e.g. `refs/pull/*` in API-06) that no push path may write; callers must
    /// ensure `sha` resolves to an object already present in `repo`.
    async fn update_ref(&self, repo: &Path, refname: &str, sha: &str) -> Result<(), GitError>;

    /// Repoint the bare `HEAD` symref at `refs/heads/<branch>`
    /// (`git symbolic-ref HEAD …`) — used when a migration discovers the
    /// remote's default branch differs from the one chosen at `init_bare`.
    async fn set_head_branch(&self, repo: &Path, branch: &str) -> Result<(), GitError>;

    // ------------------------------------------------------------------
    // VCS git-interop plumbing (u6 / TD6.2)
    // ------------------------------------------------------------------

    /// Resolve `rev` to a commit oid — `Ok(Some)` peels annotated tags and
    /// accepts raw oids, `Ok(None)` when `rev` is missing or resolves to a
    /// non-commit object (tree/blob). Never errors on an unresolvable rev.
    async fn verify_commit_oid(&self, repo: &Path, rev: &str) -> Result<Option<String>, GitError>;

    /// `git check-ref-format` validity for a fully-qualified `refs/heads/*`
    /// refname — `Ok(true)` legal, `Ok(false)` rejected.
    async fn check_refname(&self, repo: &Path, refname: &str) -> Result<bool, GitError>;

    /// Raw `cat-file commit` parse. `sha` must verify as a commit first
    /// ([`GitBackend::verify_commit_oid`]); malformed objects → `Process`.
    async fn cat_commit(&self, repo: &Path, sha: &str) -> Result<RawCommit, GitError>;

    /// `diff-tree --name-status -z -M` for `sha` against `base` (`None` diffs
    /// against the empty tree — a root import). Bounded stdout: over
    /// `max_bytes` → `Process("oxidean:stdout-bound-exceeded")`.
    async fn commit_name_status(
        &self,
        repo: &Path,
        base: Option<&str>,
        sha: &str,
        max_bytes: usize,
    ) -> Result<Vec<NameStatusRow>, GitError>;

    /// `ls-tree -r -l -z --full-tree` at `sha` — every leaf with mode/kind/
    /// oid/size. Bounded stdout: over `max_bytes` → the stdout-bound sentinel.
    async fn tree_flat(
        &self,
        repo: &Path,
        sha: &str,
        max_bytes: usize,
    ) -> Result<Vec<FlatTreeEntry>, GitError>;

    /// `rev-list --topo-order` over `tips` (children-first), reversed in-crate
    /// to parents-first. `max` bounds both the walk and the returned page;
    /// `truncated` reports the cut.
    async fn rev_list(&self, repo: &Path, tips: &[String], max: usize)
        -> Result<RevList, GitError>;

    /// `hash-object -t blob -w --stdin` — mints a blob, returns its oid.
    async fn write_object(&self, repo: &Path, bytes: &[u8]) -> Result<String, GitError>;

    /// Mint a commit deterministically: `entries` fold into nested trees via
    /// `mktree -z` (full snapshot, dirs synthesized), then `commit-tree` with
    /// the supplied raw idents and message — no signing, no ref movement.
    async fn write_commit(
        &self,
        repo: &Path,
        entries: &[TreeWriteEntry],
        parents: &[String],
        author: &GitIdent,
        committer: &GitIdent,
        message: &[u8],
    ) -> Result<String, GitError>;
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
