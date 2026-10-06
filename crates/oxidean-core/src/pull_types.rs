//! Pull request domain DTOs and enums for `pull.*` RPC (Phase 12).

use serde::{Deserialize, Serialize};

/// PR lifecycle: open ↔ closed; merged is terminal for reopen-as-open of same merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PullState {
    Open,
    Closed,
    Merged,
}

impl PullState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            "merged" => Ok(Self::Merged),
            other => Err(format!("invalid pull state: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "merge" => Ok(Self::Merge),
            "squash" => Ok(Self::Squash),
            "rebase" => Ok(Self::Rebase),
            other => Err(format!("invalid merge method: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PullReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
}

impl PullReviewState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::ChangesRequested => "changes_requested",
            Self::Commented => "commented",
            Self::Dismissed => "dismissed",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "approved" => Ok(Self::Approved),
            "changes_requested" => Ok(Self::ChangesRequested),
            "commented" => Ok(Self::Commented),
            "dismissed" => Ok(Self::Dismissed),
            other => Err(format!("invalid review state: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullPublic {
    pub id: String,
    pub repo_id: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: PullState,
    pub draft: bool,
    pub author_id: String,
    pub author_username: String,
    pub base_ref: String,
    pub base_sha: String,
    pub head_repo_id: String,
    pub head_owner: String,
    pub head_name: String,
    pub head_ref: String,
    pub head_sha: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_commit_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_method: Option<MergeMethod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub comment_count: i64,
    #[serde(default)]
    pub assignees: Vec<crate::IssueAssigneePublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatePullRequest {
    pub owner: String,
    pub name: String,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    pub base_ref: String,
    pub head_ref: String,
    /// Optional fork head owner (defaults to base owner / same-repo).
    #[serde(default)]
    pub head_owner: Option<String>,
    #[serde(default)]
    pub head_name: Option<String>,
    #[serde(default)]
    pub draft: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullRefRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullListRequest {
    pub owner: String,
    pub name: String,
    /// `open` (default) | `closed` | `merged` | `all`
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    /// Title/body substring search (same semantics as issue.list `q`).
    #[serde(default)]
    pub q: Option<String>,
    /// `approved` | `changes_requested` | `review_required` | …
    #[serde(default, alias = "review")]
    pub review_state: Option<String>,
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullListResponse {
    pub pulls: Vec<PullPublic>,
    pub total: i64,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdatePullRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub base_ref: Option<String>,
    #[serde(default)]
    pub draft: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergePullRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    pub method: MergeMethod,
    #[serde(default)]
    pub commit_title: Option<String>,
    #[serde(default)]
    pub commit_message: Option<String>,
    #[serde(default)]
    pub delete_branch: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergePullResponse {
    pub pull: PullPublic,
    pub merge_commit_sha: String,
}

/// `pull.branchStatus` response — head vs live base tip freshness (GIT-24).
/// `PullRefRequest` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullBranchStatusResponse {
    /// `up_to_date` when the base tip is already reachable from the head,
    /// otherwise `behind` (the base branch has commits the head lacks).
    pub status: String,
    /// Commits on the head branch the base lacks (the PR's own commits).
    pub ahead_count: i64,
    /// Commits on the base branch the head lacks.
    pub behind_count: i64,
    /// Live base branch tip.
    pub base_sha: String,
    /// Live head branch tip.
    pub head_sha: String,
    /// Caller can update the head branch (Write on base repo or head repo).
    #[serde(default)]
    pub can_update: bool,
}

/// `pull.updateBranch` response (GIT-24). `PullRefRequest` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdatePullBranchResponse {
    pub pull: PullPublic,
    /// `up_to_date` (no-op) | `updated` (merge commit created on the head).
    pub status: String,
    /// Merge commit SHA when `status == "updated"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_commit_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMergeSettings {
    pub allow_merge_commit: bool,
    pub allow_squash_merge: bool,
    pub allow_rebase_merge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateRepoMergeSettingsRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub allow_merge_commit: Option<bool>,
    #[serde(default)]
    pub allow_squash_merge: Option<bool>,
    #[serde(default)]
    pub allow_rebase_merge: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullCommentPublic {
    pub id: String,
    pub pull_id: String,
    pub author_id: String,
    pub author_username: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    pub outdated: bool,
    pub resolved: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatePullCommentRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    pub body: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub side: Option<String>,
    #[serde(default)]
    pub line: Option<i64>,
    #[serde(default)]
    pub start_line: Option<i64>,
    #[serde(default)]
    pub commit_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullCommentsListResponse {
    pub comments: Vec<PullCommentPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ResolvePullCommentRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    pub comment_id: String,
    pub resolved: bool,
}

/// Update / delete / history for a comment on a pull request.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullCommentRefRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    #[serde(rename = "commentId", alias = "comment_id")]
    pub comment_id: String,
}

/// Update comment body (author only; mirrors `issue.comments.update`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdatePullCommentRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    #[serde(rename = "commentId", alias = "comment_id")]
    pub comment_id: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeletePullCommentResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullReviewPublic {
    pub id: String,
    pub pull_id: String,
    pub author_id: String,
    pub author_username: String,
    pub state: PullReviewState,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    pub submitted_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismissed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismiss_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SubmitPullReviewRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    /// `approved` | `changes_requested` | `commented`
    pub state: String,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DismissPullReviewRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    pub review_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullReviewsListResponse {
    pub reviews: Vec<PullReviewPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullReviewRequestMutate {
    pub owner: String,
    pub name: String,
    pub number: i64,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullReviewRequestsListResponse {
    pub usernames: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullFilesResponse {
    pub files: Vec<PullDiffFile>,
    pub empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullDiffFile {
    pub path: String,
    pub status: String,
    pub patch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullCommitsResponse {
    pub commits: Vec<PullCommitSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PullCommitSummary {
    pub sha: String,
    pub short_sha: String,
    pub subject: String,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_avatar_url: Option<String>,
    #[serde(default = "default_sig_none")]
    pub signature_status: String,
    #[serde(default)]
    pub signature_kind: String,
}

fn default_sig_none() -> String {
    "none".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ForkRepoRequest {
    pub owner: String,
    pub name: String,
    /// Destination owner (user username). Defaults to session user.
    #[serde(default)]
    pub into_owner: Option<String>,
    #[serde(default)]
    pub into_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_state_roundtrip() {
        assert_eq!(PullState::parse("merged").unwrap(), PullState::Merged);
        assert_eq!(MergeMethod::parse("squash").unwrap().as_str(), "squash");
        assert_eq!(
            PullReviewState::parse("changes_requested")
                .unwrap()
                .as_str(),
            "changes_requested"
        );
    }
}
