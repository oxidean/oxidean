//! `search.global` types — sitewide grouped search across entity kinds (DEBT-03).
//!
//! Unlike `repo.search` (single-repo, GIT-18), this aggregates across every
//! repository the viewer can read. Code/commits run a bounded per-repo git scan
//! (no cross-repo index yet — SRCH-01).

use serde::{Deserialize, Serialize};

/// One entity group selectable via `search.global` `types`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GlobalSearchKind {
    Repositories,
    Users,
    Organizations,
    Issues,
    Pulls,
    Commits,
    Code,
}

impl GlobalSearchKind {
    pub const ALL: [GlobalSearchKind; 7] = [
        Self::Repositories,
        Self::Users,
        Self::Organizations,
        Self::Issues,
        Self::Pulls,
        Self::Commits,
        Self::Code,
    ];
}

/// `search.global` input.
///
/// `types` absent/empty → all groups populated (bounded per-group `limit`).
/// `types` present → only those groups get `hits`; database-backed groups still
/// report `total` so callers can render per-kind counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchRequest {
    pub q: String,
    #[serde(default)]
    pub types: Option<Vec<GlobalSearchKind>>,
    /// Row offset applied to each requested database-backed group.
    #[serde(default)]
    pub offset: Option<i64>,
    /// Per-group hits cap (clamped server-side).
    #[serde(default)]
    pub limit: Option<i64>,
}

/// Repository hit — slim projection; not the full `RepoPublic` envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchRepoHit {
    /// Owner login — username or org slug (shared `/{owner}` namespace).
    pub owner: String,
    pub owner_type: String,
    pub name: String,
    pub description: String,
    pub visibility: String,
    pub star_count: i64,
    pub updated_at: String,
}

/// User hit — mirrors `user.lookup` public fields (never email).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchUserHit {
    pub username: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

/// Organization hit — public directory entry (mirrors `org.get` fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchOrgHit {
    pub slug: String,
    pub display_name: String,
}

/// Issue hit with repository context for sitewide results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchIssueHit {
    pub repo_owner: String,
    pub repo_name: String,
    pub number: i64,
    pub title: String,
    /// `open` | `closed`.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_username: Option<String>,
    pub comment_count: i64,
    pub updated_at: String,
}

/// Pull request hit with repository context for sitewide results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchPullHit {
    pub repo_owner: String,
    pub repo_name: String,
    pub number: i64,
    pub title: String,
    /// `open` | `closed` | `merged`.
    pub state: String,
    pub draft: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_username: Option<String>,
    pub comment_count: i64,
    pub updated_at: String,
}

/// Commit hit from the bounded cross-repo `git log --grep` scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchCommitHit {
    pub repo_owner: String,
    pub repo_name: String,
    pub sha: String,
    pub short_sha: String,
    pub subject: String,
    pub author_name: String,
    pub authored_at: String,
}

/// Code hit from the bounded cross-repo `git grep` scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchCodeHit {
    pub repo_owner: String,
    pub repo_name: String,
    /// Branch/ref the hit was found on (repo default branch today).
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub path: String,
    pub line: u32,
    pub content: String,
}

/// One result group: `hits` + `total` + `truncated`.
///
/// `total` is a real `COUNT(*)` for database-backed kinds; for the bounded git
/// scans (commits/code) it is the number of hits found within the scanned repo
/// window and `truncated` reports that coverage was partial.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchGroup<T> {
    pub hits: Vec<T>,
    pub total: i64,
    #[serde(default)]
    pub truncated: bool,
}

impl<T> Default for GlobalSearchGroup<T> {
    fn default() -> Self {
        Self {
            hits: Vec::new(),
            total: 0,
            truncated: false,
        }
    }
}

/// `search.global` response — every group is always present; unrequested
/// git-scan groups return empty hits with `total`/`truncated` zeroed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchResponse {
    pub q: String,
    pub repositories: GlobalSearchGroup<GlobalSearchRepoHit>,
    pub users: GlobalSearchGroup<GlobalSearchUserHit>,
    pub organizations: GlobalSearchGroup<GlobalSearchOrgHit>,
    pub issues: GlobalSearchGroup<GlobalSearchIssueHit>,
    pub pulls: GlobalSearchGroup<GlobalSearchPullHit>,
    pub commits: GlobalSearchGroup<GlobalSearchCommitHit>,
    pub code: GlobalSearchGroup<GlobalSearchCodeHit>,
}
