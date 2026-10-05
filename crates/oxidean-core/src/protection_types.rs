//! Classic branch protection + commit status RPC types (Phase 13 / D-26).

use serde::{Deserialize, Serialize};

/// Commit status state (D-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CommitStatusState {
    Pending,
    Success,
    Failure,
    Error,
}

impl CommitStatusState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "pending" => Ok(Self::Pending),
            "success" => Ok(Self::Success),
            "failure" => Ok(Self::Failure),
            "error" => Ok(Self::Error),
            other => Err(format!("invalid commit status state: {other}")),
        }
    }

    /// Pass for required-check evaluation (D-12); neutral/skipped aliases accepted if stored later.
    pub fn is_passing(self) -> bool {
        matches!(self, Self::Success)
    }

    pub fn is_passing_str(s: &str) -> bool {
        matches!(s, "success" | "neutral" | "skipped")
    }
}

/// Public branch protection rule (ORG-05).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BranchProtectionRulePublic {
    pub id: String,
    pub repo_id: String,
    pub pattern: String,
    pub require_reviews: bool,
    pub required_approving_review_count: i32,
    pub dismiss_stale_reviews: bool,
    pub require_conversation_resolution: bool,
    pub require_last_push_approval: bool,
    pub required_status_contexts: Vec<String>,
    pub strict_status_checks: bool,
    pub allow_force_pushes: bool,
    pub allow_deletions: bool,
    pub enforce_admins: bool,
    pub required_linear_history: bool,
    pub lock_branch: bool,
    /// GIT-22: pushes introducing unsigned commits are denied by the update hook.
    pub require_signed_commits: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// `repo.branchProtection.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BranchProtectionListResponse {
    pub rules: Vec<BranchProtectionRulePublic>,
}

/// Shared fields for create/update (D-05..18).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BranchProtectionRuleInput {
    pub owner: String,
    pub name: String,
    pub pattern: String,
    #[serde(default)]
    pub require_reviews: bool,
    #[serde(default = "default_review_count")]
    pub required_approving_review_count: i32,
    #[serde(default)]
    pub dismiss_stale_reviews: bool,
    #[serde(default)]
    pub require_conversation_resolution: bool,
    #[serde(default)]
    pub require_last_push_approval: bool,
    #[serde(default)]
    pub required_status_contexts: Vec<String>,
    #[serde(default)]
    pub strict_status_checks: bool,
    #[serde(default)]
    pub allow_force_pushes: bool,
    #[serde(default)]
    pub allow_deletions: bool,
    #[serde(default)]
    pub enforce_admins: bool,
    #[serde(default)]
    pub required_linear_history: bool,
    #[serde(default)]
    pub lock_branch: bool,
    /// GIT-22: deny pushes introducing commits without a forge-verified signature.
    #[serde(default)]
    pub require_signed_commits: bool,
}

fn default_review_count() -> i32 {
    1
}

/// `repo.branchProtection.update` / delete — includes rule id.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BranchProtectionUpdateRequest {
    pub owner: String,
    pub name: String,
    pub id: String,
    pub pattern: String,
    #[serde(default)]
    pub require_reviews: bool,
    #[serde(default = "default_review_count")]
    pub required_approving_review_count: i32,
    #[serde(default)]
    pub dismiss_stale_reviews: bool,
    #[serde(default)]
    pub require_conversation_resolution: bool,
    #[serde(default)]
    pub require_last_push_approval: bool,
    #[serde(default)]
    pub required_status_contexts: Vec<String>,
    #[serde(default)]
    pub strict_status_checks: bool,
    #[serde(default)]
    pub allow_force_pushes: bool,
    #[serde(default)]
    pub allow_deletions: bool,
    #[serde(default)]
    pub enforce_admins: bool,
    #[serde(default)]
    pub required_linear_history: bool,
    #[serde(default)]
    pub lock_branch: bool,
    /// GIT-22: deny pushes introducing commits without a forge-verified signature.
    #[serde(default)]
    pub require_signed_commits: bool,
}

/// `repo.branchProtection.delete`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BranchProtectionDeleteRequest {
    pub owner: String,
    pub name: String,
    pub id: String,
}

/// Public tag protection rule (GIT-21) — restricts create/update/delete on
/// matching `refs/tags/*` for non-bypass actors.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TagProtectionRulePublic {
    pub id: String,
    pub repo_id: String,
    pub pattern: String,
    pub allow_create: bool,
    pub allow_update: bool,
    pub allow_delete: bool,
    pub enforce_admins: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// `repo.tagProtection.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TagProtectionListResponse {
    pub rules: Vec<TagProtectionRulePublic>,
}

/// Shared fields for tag rule create/update.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TagProtectionRuleInput {
    pub owner: String,
    pub name: String,
    pub pattern: String,
    #[serde(default)]
    pub allow_create: bool,
    #[serde(default)]
    pub allow_update: bool,
    #[serde(default)]
    pub allow_delete: bool,
    #[serde(default)]
    pub enforce_admins: bool,
}

/// `repo.tagProtection.update` — includes rule id.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TagProtectionUpdateRequest {
    pub owner: String,
    pub name: String,
    pub id: String,
    pub pattern: String,
    #[serde(default)]
    pub allow_create: bool,
    #[serde(default)]
    pub allow_update: bool,
    #[serde(default)]
    pub allow_delete: bool,
    #[serde(default)]
    pub enforce_admins: bool,
}

/// `repo.tagProtection.delete`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TagProtectionDeleteRequest {
    pub owner: String,
    pub name: String,
    pub id: String,
}

/// Public commit status (D-11).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommitStatusPublic {
    pub id: String,
    pub repo_id: String,
    pub sha: String,
    pub context: String,
    pub state: CommitStatusState,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// `repo.commitStatus.create`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommitStatusCreateRequest {
    pub owner: String,
    pub name: String,
    pub sha: String,
    pub context: String,
    pub state: CommitStatusState,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub target_url: Option<String>,
}

/// `repo.commitStatus.list`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommitStatusListRequest {
    pub owner: String,
    pub name: String,
    pub sha: String,
}

/// `repo.commitStatus.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommitStatusListResponse {
    pub statuses: Vec<CommitStatusPublic>,
}

/// Structured merge-block reasons (D-22 / D-24).
#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ProtectionBlockReasons {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_approving_review_count: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approving_review_count: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_status_contexts: Vec<String>,
    /// GIT-22: commits newly introduced by the push lacking a verified signature.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsigned_commits: Vec<String>,
}
