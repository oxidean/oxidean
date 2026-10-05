//! Issue domain DTOs and enums for later `issue.*` / `label.*` RPC (Phase 11).
//! No handlers here — schema door + shared types only (11-02).

use serde::{Deserialize, Serialize};

/// Issue lifecycle state (D-ISS-02): open ↔ closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
}

impl IssueState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            other => Err(format!("invalid issue state: {other}")),
        }
    }
}

/// The eight reaction contents (D-ISS-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ReactionContent {
    #[serde(rename = "+1")]
    PlusOne,
    #[serde(rename = "-1")]
    MinusOne,
    #[serde(rename = "laugh")]
    Laugh,
    #[serde(rename = "confused")]
    Confused,
    #[serde(rename = "heart")]
    Heart,
    #[serde(rename = "hooray")]
    Hooray,
    #[serde(rename = "rocket")]
    Rocket,
    #[serde(rename = "eyes")]
    Eyes,
}

impl ReactionContent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlusOne => "+1",
            Self::MinusOne => "-1",
            Self::Laugh => "laugh",
            Self::Confused => "confused",
            Self::Heart => "heart",
            Self::Hooray => "hooray",
            Self::Rocket => "rocket",
            Self::Eyes => "eyes",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "+1" => Ok(Self::PlusOne),
            "-1" => Ok(Self::MinusOne),
            "laugh" => Ok(Self::Laugh),
            "confused" => Ok(Self::Confused),
            "heart" => Ok(Self::Heart),
            "hooray" => Ok(Self::Hooray),
            "rocket" => Ok(Self::Rocket),
            "eyes" => Ok(Self::Eyes),
            other => Err(format!("invalid reaction content: {other}")),
        }
    }
}

/// Label definition scope (D-ISS-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LabelScope {
    Org,
    Repo,
}

/// Linked issue / PR stub / real PR kind (D-ISS-13 / D-PR-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IssueLinkKind {
    Issue,
    PrStub,
    Pr,
}

impl IssueLinkKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::PrStub => "pr_stub",
            Self::Pr => "pr",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "issue" => Ok(Self::Issue),
            "pr_stub" => Ok(Self::PrStub),
            "pr" => Ok(Self::Pr),
            other => Err(format!("invalid issue link kind: {other}")),
        }
    }
}

/// Public issue metadata (list/detail).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssuePublic {
    pub id: String,
    pub repo_id: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: IssueState,
    pub author_id: String,
    pub author_username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub labels: Vec<LabelPublic>,
    #[serde(default)]
    pub assignees: Vec<IssueAssigneePublic>,
    /// Aggregated reaction counts (D-ISS-11).
    #[serde(default)]
    pub reactions: Vec<ReactionGroupPublic>,
    /// Number of comments on the issue (shown on list rows).
    #[serde(default)]
    pub comment_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueAssigneePublic {
    pub user_id: String,
    pub username: String,
    #[serde(default)]
    pub display_name: String,
}

/// Label definition returned over RPC.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LabelPublic {
    pub id: String,
    pub name: String,
    pub color: String,
    pub description: String,
    pub scope: LabelScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    /// True when this org label is hidden for the current repo effective set.
    #[serde(default)]
    pub hidden: bool,
}

/// Create org or repo label definition (Admin — D-ISS-07).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateLabelRequest {
    pub scope: LabelScope,
    /// Org slug (scope=org) or repo owner slug/username (scope=repo).
    pub owner: String,
    /// Repo name when `scope` is `repo`.
    #[serde(default)]
    pub repo: Option<String>,
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Update label fields and/or repo hide override for an org label.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateLabelRequest {
    pub id: String,
    /// Org slug or repo owner (auth context).
    pub owner: String,
    /// Repo name when mutating a repo-local label or toggling hide.
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Hide/unhide an org label for a repo (requires `repo`).
    #[serde(default)]
    pub hidden: Option<bool>,
}

/// Delete a label definition (Admin).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteLabelRequest {
    pub id: String,
    pub owner: String,
    #[serde(default)]
    pub repo: Option<String>,
}

/// List effective (or include-hidden) labels for a repo.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListLabelsForRepoRequest {
    pub owner: String,
    pub name: String,
    /// When true (Admin), include hidden org labels with `hidden: true`.
    #[serde(default, rename = "includeHidden", alias = "include_hidden")]
    pub include_hidden: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LabelsListResponse {
    pub labels: Vec<LabelPublic>,
}

/// Replace issue label set by id (Write+ — D-ISS-07).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetIssueLabelsRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    #[serde(rename = "labelIds", alias = "label_ids")]
    pub label_ids: Vec<String>,
}

/// Replace issue assignees by user id (Write+ — D-ISS-06 / D-ISS-07 / D-ISS-08).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetIssueAssigneesRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    #[serde(rename = "userIds", alias = "user_ids")]
    pub user_ids: Vec<String>,
}

/// List Read+-eligible assignee candidates for a repo (Write+ picker — D-ISS-08).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AssigneeCandidatesRequest {
    pub owner: String,
    pub name: String,
    /// Optional username prefix filter (case-insensitive).
    #[serde(default)]
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AssigneeCandidatesResponse {
    pub users: Vec<IssueAssigneePublic>,
}

/// Create-issue input (RPC wired in 11-03).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateIssueRequest {
    pub owner: String,
    pub name: String,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
}

/// Issue get / mutate path — owner + repo name + `#N`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueRefRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
}

/// Update title and/or body (D-ISS-03). Omitted fields keep current values.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateIssueRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

/// Admin hard-delete with typed confirm (D-ISS-02).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteIssueRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    #[serde(rename = "confirmNumber", alias = "confirm_number")]
    pub confirm_number: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteIssueResponse {
    pub number: i64,
}

/// One prior title/body snapshot (D-ISS-04).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueRevisionPublic {
    pub id: String,
    pub issue_id: String,
    pub editor_id: String,
    pub editor_username: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueHistoryResponse {
    pub revisions: Vec<IssueRevisionPublic>,
}

/// List filters (D-ISS-16..18).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueListRequest {
    pub owner: String,
    pub name: String,
    /// `open` | `closed` | `all` — default open.
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueListResponse {
    pub issues: Vec<IssuePublic>,
    pub total: i64,
}

/// Reaction target for `issue.reactions.toggle` (D-ISS-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ReactionTarget {
    Issue,
    Comment,
}

/// Toggle a reaction on an issue or comment (Write+ / D-ISS-20).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToggleReactionRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    pub target: ReactionTarget,
    #[serde(default, rename = "commentId", alias = "comment_id")]
    pub comment_id: Option<String>,
    pub content: ReactionContent,
}

/// Aggregated count for one reaction content value.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReactionGroupPublic {
    pub content: String,
    pub count: i64,
    #[serde(rename = "viewerHasReacted", alias = "viewer_has_reacted")]
    pub viewer_has_reacted: bool,
}

/// Result of `issue.reactions.toggle`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToggleReactionResponse {
    pub reactions: Vec<ReactionGroupPublic>,
    /// Whether the caller now has this content reaction after the toggle.
    pub reacted: bool,
}

/// Comment on an issue (ISS-02).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueCommentPublic {
    pub id: String,
    pub issue_id: String,
    pub author_id: String,
    pub author_username: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    /// Aggregated reaction counts (D-ISS-11).
    #[serde(default)]
    pub reactions: Vec<ReactionGroupPublic>,
}

/// Create comment on an issue (Write+).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateIssueCommentRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    pub body: String,
}

/// Update / delete / history for a comment on an issue.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueCommentRefRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    #[serde(rename = "commentId", alias = "comment_id")]
    pub comment_id: String,
}

/// Update comment body (author only; D-ISS-09).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateIssueCommentRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    #[serde(rename = "commentId", alias = "comment_id")]
    pub comment_id: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueCommentsListResponse {
    pub comments: Vec<IssueCommentPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteIssueCommentResponse {
    pub ok: bool,
}

/// One prior comment body snapshot (D-ISS-12).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommentRevisionPublic {
    pub id: String,
    pub comment_id: String,
    pub editor_id: String,
    pub editor_username: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommentHistoryResponse {
    pub revisions: Vec<CommentRevisionPublic>,
}

/// Linked issue / PR stub row (D-ISS-13).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueLinkPublic {
    pub id: String,
    pub kind: IssueLinkKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_repo_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_number: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_opaque_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub created_at: String,
}

/// Manual add link / PR stub (D-ISS-14).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddIssueLinkRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    pub kind: IssueLinkKind,
    #[serde(default, rename = "targetNumber", alias = "target_number")]
    pub target_number: Option<i64>,
    #[serde(default, rename = "targetRepoId", alias = "target_repo_id")]
    pub target_repo_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// Manual remove link by opaque id (D-ISS-14).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RemoveIssueLinkRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    #[serde(rename = "linkId", alias = "link_id")]
    pub link_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IssueLinksListResponse {
    pub links: Vec<IssueLinkPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RemoveIssueLinkResponse {
    pub ok: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_state_roundtrip() {
        assert_eq!(IssueState::parse("open").unwrap(), IssueState::Open);
        assert_eq!(IssueState::Closed.as_str(), "closed");
    }

    #[test]
    fn reaction_content_eight() {
        assert_eq!(ReactionContent::parse("+1").unwrap().as_str(), "+1");
        assert_eq!(ReactionContent::parse("eyes").unwrap().as_str(), "eyes");
        assert!(ReactionContent::parse("thumbs").is_err());
    }

    #[test]
    fn link_kind_pr_stub() {
        assert_eq!(
            IssueLinkKind::parse("pr_stub").unwrap(),
            IssueLinkKind::PrStub
        );
    }
}
