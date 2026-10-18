//! Milestone RPC types — repo-scoped `#N` milestone domain (issue linkage,
//! `milestone`/`issues.milestoned` Actions events).

use serde::{Deserialize, Serialize};

/// `Option<Option<T>>` wire helper — distinguishes *absent* (keep current
/// value, deserialize_with isn't invoked thanks to `serde(default)`) from
/// explicit `null` (clear). Plain `Option<Option<T>>` collapses both to
/// `None` and `null`-clears can never be expressed.
pub fn deserialize_some<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Public milestone shape (list/detail/issue embed). Counts follow GitHub's
/// milestone list: `open_issues` + `closed_issues`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MilestonePublic {
    pub id: String,
    pub repo_id: String,
    pub number: i64,
    pub title: String,
    pub description: String,
    pub state: String,
    /// `YYYY-MM-DD` when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    #[serde(default)]
    pub creator_username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub open_issues: i64,
    #[serde(default)]
    pub closed_issues: i64,
}

/// `milestone.list` — repo path + optional state filter (`open` default).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MilestoneListRequest {
    pub owner: String,
    pub name: String,
    /// `open` | `closed` | `all` — default open.
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MilestoneListResponse {
    pub milestones: Vec<MilestonePublic>,
}

/// `milestone.get` — repo path + `#N`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MilestoneRefRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
}

/// `milestone.create` — Write+.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateMilestoneRequest {
    pub owner: String,
    pub name: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(default)]
    pub due_on: Option<String>,
}

/// `milestone.update` — Write+. Omitted fields keep current values;
/// `due_on: null` clears the due date.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateMilestoneRequest {
    pub owner: String,
    pub name: String,
    pub number: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// `YYYY-MM-DD`; explicit `null` clears.
    #[serde(default, deserialize_with = "deserialize_some")]
    pub due_on: Option<Option<String>>,
    /// `open` | `closed` — transitions fire `opened`/`closed` events.
    #[serde(default)]
    pub state: Option<String>,
}

/// `milestone.delete` — Write+; issues keep their data, the link clears.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteMilestoneResponse {
    pub ok: bool,
}

/// `issue.milestone.set` — Write+. `milestone` is the milestone `#N`;
/// `null`/absent clears.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetIssueMilestoneRequest {
    pub owner: String,
    pub name: String,
    /// Issue `#N`.
    pub number: i64,
    /// Milestone `#N`; `null` clears the issue's milestone.
    #[serde(default)]
    pub milestone: Option<i64>,
}
