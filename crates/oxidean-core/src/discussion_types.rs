//! Discussion forum RPC types — repo-scoped categories, discussions, and
//! threaded comments (GitHub Discussions shape). Discussion numbers share
//! the issue/PR `issue_counters` sequence so `#N` never collides across
//! kinds. `discussions.*` mutations fire `discussion` /
//! `discussion_comment` lifecycle + webhook events with the GitHub
//! activity-type vocabulary.

use serde::{Deserialize, Serialize};

/// A discussion category — the `is_answerable` flag marks GitHub's Q&A
/// format (accepts an "answer" comment); other categories are open threads.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCategoryPublic {
    pub id: String,
    pub repo_id: String,
    pub name: String,
    /// URL-safe unique key (GitHub `slug`) — stable across renames.
    pub slug: String,
    pub emoji: String,
    pub description: String,
    /// GitHub `is_answerable` — Q&A categories only.
    pub is_answerable: bool,
    /// Contiguous 0-based position inside the repo's category list.
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A discussion thread.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionPublic {
    pub id: String,
    pub repo_id: String,
    /// Shared `#N` with issues/PRs (GitHub uses one sequence for all three).
    pub number: i64,
    pub title: String,
    pub body: String,
    /// Category embed — `None` when uncategorized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<DiscussionCategoryPublic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_username: Option<String>,
    /// Comment id the author marked as the answer (answerable categories).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_comment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_by_username: Option<String>,
    pub locked: bool,
    pub pinned: bool,
    /// Repo label ids attached to the thread.
    pub label_ids: Vec<String>,
    /// `/{owner}/{repo}/discussions/{number}` web route.
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A comment on a discussion — `parent_id` threads replies one level deep
/// (GitHub's model: replies attach to a top-level comment).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCommentPublic {
    pub id: String,
    pub discussion_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_username: Option<String>,
    pub body: String,
    /// `/{owner}/{repo}/discussions/{number}#comment-{id}` web route.
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCategoriesListRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCategoriesListResponse {
    pub categories: Vec<DiscussionCategoryPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCategoryCreateRequest {
    pub owner: String,
    pub name: String,
    /// Display name (slug derived server-side).
    pub category_name: String,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_answerable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCategoryUpdateRequest {
    pub owner: String,
    pub name: String,
    /// Category id.
    pub category: String,
    #[serde(default)]
    pub category_name: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_answerable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCategoryRefRequest {
    pub owner: String,
    pub name: String,
    /// Category id.
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionDeleteResponse {
    pub ok: bool,
}

// ---------------------------------------------------------------------------
// Discussions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionsListRequest {
    pub owner: String,
    pub name: String,
    /// Filter by category id.
    #[serde(default)]
    pub category: Option<String>,
    /// `true` = answered only, `false` = unanswered only.
    #[serde(default)]
    pub answered: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionsListResponse {
    pub discussions: Vec<DiscussionPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionRefRequest {
    pub owner: String,
    pub name: String,
    /// The thread's shared `#N` (issue/PR/discussion sequence).
    pub discussion: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCreateRequest {
    pub owner: String,
    pub name: String,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    /// Category id — must belong to this repo.
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub label_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionUpdateRequest {
    pub owner: String,
    pub name: String,
    pub discussion: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// Category id — explicit null clears (uncategorized).
    #[serde(
        default,
        deserialize_with = "crate::milestone_types::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub category: Option<Option<String>>,
}

/// Pin/unpin — separate from `update` so the event vocabulary matches
/// GitHub (`pinned`/`unpinned` activity types).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionPinRequest {
    pub owner: String,
    pub name: String,
    pub discussion: i64,
    pub pinned: bool,
}

/// Lock/unlock — `locked`/`unlocked` activity types.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionLockRequest {
    pub owner: String,
    pub name: String,
    pub discussion: i64,
    pub locked: bool,
}

/// Move the thread to another repository (`transferred` activity type —
/// emitted on the destination repo like GitHub).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionTransferRequest {
    pub owner: String,
    pub name: String,
    pub discussion: i64,
    /// Destination repo owner (user or org).
    pub target_owner: String,
    /// Destination repo name.
    pub target_name: String,
}

/// Replace the label set (`labeled`/`unlabeled` per label diff).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionLabelsSetRequest {
    pub owner: String,
    pub name: String,
    pub discussion: i64,
    pub label_ids: Vec<String>,
}

/// Mark a comment as the answer (`answered`) — answerable categories only.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionAnswerSetRequest {
    pub owner: String,
    pub name: String,
    pub discussion: i64,
    /// Comment id belonging to this discussion.
    pub comment: String,
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCommentsListRequest {
    pub owner: String,
    pub name: String,
    pub discussion: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCommentsListResponse {
    pub comments: Vec<DiscussionCommentPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCommentCreateRequest {
    pub owner: String,
    pub name: String,
    pub discussion: i64,
    pub body: String,
    /// Top-level comment id to reply under (one level of threading).
    #[serde(default)]
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCommentUpdateRequest {
    pub owner: String,
    pub name: String,
    pub discussion: i64,
    /// Comment id.
    pub comment: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionCommentRefRequest {
    pub owner: String,
    pub name: String,
    pub discussion: i64,
    /// Comment id.
    pub comment: String,
}
