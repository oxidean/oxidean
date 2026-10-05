//! In-app notification DTOs (NOTF-01 / NOTF-02 / D-12 / D-15).

use serde::{Deserialize, Serialize};

/// Subject kind for deep links (D-05, DEBT-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationSubjectKind {
    Issue,
    PullRequest,
    Release,
    WorkflowRun,
    Push,
}

impl NotificationSubjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::PullRequest => "pull_request",
            Self::Release => "release",
            Self::WorkflowRun => "workflow_run",
            Self::Push => "push",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "issue" => Ok(Self::Issue),
            "pull_request" => Ok(Self::PullRequest),
            "release" => Ok(Self::Release),
            "workflow_run" => Ok(Self::WorkflowRun),
            "push" => Ok(Self::Push),
            other => Err(format!("unknown notification subject_kind: {other}")),
        }
    }
}

/// Public notification row for list UI + deep links (D-05 / D-09 / D-10).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NotificationPublic {
    pub id: String,
    pub reason: String,
    pub subject_kind: String,
    pub subject_repo_id: String,
    pub owner: String,
    pub repo: String,
    pub subject_number: i64,
    pub subject_title: String,
    /// Deep-link ref for non-numbered subjects (release tag, run id; DEBT-06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<String>,
    pub actor_id: String,
    pub actor_username: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
}

/// List filter: unread only or all (D-09 / D-12).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NotificationListRequest {
    /// `unread` | `all` — default `unread`.
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NotificationListResponse {
    pub notifications: Vec<NotificationPublic>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NotificationUnreadCountResponse {
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NotificationMarkReadRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NotificationMarkReadResponse {
    pub marked: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NotificationMarkAllReadResponse {
    pub marked: i64,
}
