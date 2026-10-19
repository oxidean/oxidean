//! Merge queue RPC types — the per-`base_ref` FIFO train
//! (`merge_queue_entries`) and the tested batches (`merge_groups`) that back
//! GitHub's "Require merge queue" branch protection.
//!
//! Enqueue forms a `checking` group spanning base_tip + every queued PR in
//! order, materialized as merge-commit chain on
//! `refs/heads/gh-readonly-queue/{base}/group-{id}`. `merge_group`
//! (`checks_requested`) fires so Actions validates the combined tree; on
//! success `base_ref` fast-forwards to the group's `head_sha` and each PR is
//! marked merged — the tested commits become history, GitHub parity.

use serde::{Deserialize, Serialize};

/// A queued PR — position in its train + the group it currently rides.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueueEntryPublic {
    pub id: String,
    pub repo_id: String,
    /// PR repo-visible number (queue entries always resolve by number).
    pub pull_number: i64,
    /// Bare branch name the train targets (`main`, not `refs/heads/main`).
    pub base_ref: String,
    /// 1-based enqueue order within the train.
    pub position: i64,
    /// `queued` | `merged` | `dequeued`.
    pub state: String,
    /// The `checking` group the entry currently rides (`None` once resolved).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// User id that added the pull to the queue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enqueued_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enqueued_by_username: Option<String>,
    /// Why the entry left the queue — `manual` | `failed_checks` |
    /// `merge_conflict` | `head_changed` | `base_changed` | `closed`
    /// (`""` while queued/merged).
    pub dequeue_reason: String,
    /// Minimal pull embed (GitHub embeds `pullRequest` on queue entries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull: Option<MergeQueuePullEmbed>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueuePullEmbed {
    pub number: i64,
    pub title: String,
    pub state: String,
    pub head_ref: String,
    pub head_sha: String,
    pub base_ref: String,
}

/// A tested batch — base_tip + its entries merged in order; `head_sha` is
/// what Actions `on: merge_group` runs check out.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeGroupPublic {
    pub id: String,
    pub repo_id: String,
    pub base_ref: String,
    pub base_sha: String,
    pub head_ref: String,
    pub head_sha: String,
    /// `checking` | `merged` | `destroyed`.
    pub state: String,
    /// Pull numbers the group carries, in merge order.
    pub pull_numbers: Vec<i64>,
    pub created_at: String,
    pub updated_at: String,
}

// -- merge_queue.* -----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueueListRequest {
    pub owner: String,
    pub name: String,
    /// Filter to one train (bare branch name); empty lists every queued
    /// train in the repo.
    #[serde(default)]
    pub base_ref: Option<String>,
    /// `queued` (default) | `merged` | `dequeued` | `all`.
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueueListResponse {
    pub entries: Vec<MergeQueueEntryPublic>,
}

/// The PR's queue slot + groups it rode — GitHub's "merge queue" section on
/// the PR page reads this shape.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueueGetRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueueGetResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<MergeQueueEntryPublic>,
    /// The `checking` group the entry rides (`None` when not queued).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<MergeGroupPublic>,
    /// Whether the base branch's protection requires the queue — the merge
    /// button renders "Merge when ready" from this.
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueueEnqueueRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueueEnqueueResponse {
    pub entry: MergeQueueEntryPublic,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<MergeGroupPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueueDequeueRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueueDequeueResponse {
    pub entry: MergeQueueEntryPublic,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueueGroupsRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub base_ref: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeQueueGroupsResponse {
    pub groups: Vec<MergeGroupPublic>,
}
