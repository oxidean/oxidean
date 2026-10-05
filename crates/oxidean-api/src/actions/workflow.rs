//! Discover `.github/workflows/*.{yml,yaml}` at a commit (D-ACT-01).

use std::path::{Component, Path, PathBuf};

use oxidean_git::{GitBackend, GitError, TreeEntryKind};

use crate::actions::parse::{parse_workflow_yaml, ParseError, WorkflowDocument};

/// Soft cap on a single workflow file (T-19-06).
pub const MAX_WORKFLOW_BYTES: usize = 1_048_576;

#[derive(Debug, Clone)]
pub struct DiscoveredWorkflow {
    pub path: String,
    pub document: WorkflowDocument,
}

/// A workflow file that was skipped during discovery (bad YAML, too large,
/// or a disallowed path) — recorded so callers can surface it.
#[derive(Debug, Clone)]
pub struct DiscoveryFileError {
    pub path: String,
    pub message: String,
}

/// Discovery outcome: usable workflows plus per-file errors for skipped
/// files. Only git-level failures produce `Err` — a broken workflow file
/// must not take the repo's remaining workflows down with it.
#[derive(Debug, Default)]
pub struct WorkflowDiscovery {
    pub workflows: Vec<DiscoveredWorkflow>,
    pub errors: Vec<DiscoveryFileError>,
}

#[derive(Debug)]
pub enum DiscoverError {
    Git(GitError),
    Parse { path: String, error: ParseError },
    TooLarge { path: String, size: usize },
    PathEscape(String),
}

impl std::fmt::Display for DiscoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git(e) => write!(f, "git: {e}"),
            Self::Parse { path, error } => write!(f, "{path}: {error}"),
            Self::TooLarge { path, size } => {
                write!(f, "{path}: workflow exceeds {MAX_WORKFLOW_BYTES} bytes ({size})")
            }
            Self::PathEscape(p) => write!(f, "path escape rejected: {p}"),
        }
    }
}

impl std::error::Error for DiscoverError {}

impl From<GitError> for DiscoverError {
    fn from(value: GitError) -> Self {
        Self::Git(value)
    }
}

fn is_workflow_filename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".yml") || lower.ends_with(".yaml")
}

/// Reject `..` and absolute components; require under `.github/workflows/`.
fn confine_workflow_path(path: &str) -> Result<String, DiscoverError> {
    let p = Path::new(path);
    if p.is_absolute() {
        return Err(DiscoverError::PathEscape(path.into()));
    }
    for c in p.components() {
        match c {
            Component::Normal(_) => {}
            _ => return Err(DiscoverError::PathEscape(path.into())),
        }
    }
    let norm = path.replace('\\', "/");
    if !norm.starts_with(".github/workflows/") {
        return Err(DiscoverError::PathEscape(path.into()));
    }
    let rest = &norm[".github/workflows/".len()..];
    if rest.is_empty() || rest.contains('/') {
        // Only flat files under workflows/ in v1 (no nested dirs).
        return Err(DiscoverError::PathEscape(path.into()));
    }
    if !is_workflow_filename(rest) {
        return Err(DiscoverError::PathEscape(path.into()));
    }
    Ok(norm)
}

/// List and parse workflow files at `treeish` in a bare (or worktree) repo.
///
/// Does **not** execute steps (D-ACT-03 / ACT-07).
pub async fn discover_workflows(
    git: &dyn GitBackend,
    repo: &Path,
    treeish: &str,
) -> Result<WorkflowDiscovery, DiscoverError> {
    let entries = git.ls_tree(repo, treeish, ".github/workflows").await?;
    let mut out = WorkflowDiscovery::default();
    for entry in entries {
        if entry.kind != TreeEntryKind::Blob {
            continue;
        }
        if !is_workflow_filename(&entry.name) {
            continue;
        }
        let path = format!(".github/workflows/{}", entry.name);
        let path = match confine_workflow_path(&path) {
            Ok(p) => p,
            Err(e) => {
                out.errors.push(DiscoveryFileError {
                    path,
                    message: e.to_string(),
                });
                continue;
            }
        };
        let bytes = git.cat_blob(repo, treeish, &path).await?;
        if bytes.len() > MAX_WORKFLOW_BYTES {
            out.errors.push(DiscoveryFileError {
                path,
                message: format!(
                    "workflow exceeds {MAX_WORKFLOW_BYTES} bytes ({})",
                    bytes.len()
                ),
            });
            continue;
        }
        match parse_workflow_yaml(&bytes) {
            Ok(document) => out.workflows.push(DiscoveredWorkflow { path, document }),
            Err(error) => {
                out.errors.push(DiscoveryFileError {
                    path,
                    message: error.to_string(),
                });
            }
        }
    }
    out.workflows.sort_by(|a, b| a.path.cmp(&b.path));
    out.errors.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Helper retained for callers that want PathBuf joining without escape.
pub fn workflows_dir() -> PathBuf {
    PathBuf::from(".github").join("workflows")
}
