//! Project board RPC types — classic repo-scoped boards (GitHub Projects
//! API shape). A board owns ordered columns; a column owns ordered cards.
//! Cards are either free-text notes or links to a repo issue/pull
//! (`content_type` + `content_id`). `projects.*` mutations fire `project` /
//! `project_column` / `project_card` lifecycle + webhook events with the
//! GitHub activity-type vocabulary.

use serde::{Deserialize, Serialize};

/// A repo-scoped project board.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectPublic {
    pub id: String,
    pub repo_id: String,
    /// Per-repo sequential `#N` (same scheme as milestones/issues).
    pub number: i64,
    /// GitHub `name` — the board title.
    pub title: String,
    /// GitHub `body` — the board description.
    pub body: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_username: Option<String>,
    /// `/{owner}/{repo}/projects/{number}` web route.
    pub html_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A column inside a board.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectColumnPublic {
    pub id: String,
    pub project_id: String,
    pub name: String,
    /// Contiguous 0-based position inside the board.
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A card inside a column — a note or a linked issue/pull.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectCardPublic {
    pub id: String,
    pub column_id: String,
    /// Note text — `None` once the card is a linked entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// `issue` | `pull_request` — the linked entity kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Linked entity row id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
    /// Linked entity `#N` — enrichment for the UI (issue/pull number).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_number: Option<i64>,
    /// Linked entity title — enrichment for the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_title: Option<String>,
    /// Linked entity state — enrichment for the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_state: Option<String>,
    /// `/{owner}/{repo}/{issues|pulls}/{number}` web route for linked cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_url: Option<String>,
    /// Contiguous 0-based position inside the column.
    pub position: i64,
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_username: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectsListRequest {
    pub owner: String,
    pub name: String,
    /// `open` (default) | `closed` | `all`.
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectsListResponse {
    pub projects: Vec<ProjectPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectRefRequest {
    pub owner: String,
    pub name: String,
    /// The board's per-repo `#N`.
    pub project: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectCreateRequest {
    pub owner: String,
    pub name: String,
    /// Board title (GitHub `name`).
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectUpdateRequest {
    pub owner: String,
    pub name: String,
    pub project: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// `open` | `closed` — transitions fire `closed`/`reopened`.
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectDeleteResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectColumnsListRequest {
    pub owner: String,
    pub name: String,
    pub project: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectColumnsListResponse {
    pub columns: Vec<ProjectColumnPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectColumnCreateRequest {
    pub owner: String,
    pub name: String,
    pub project: i64,
    /// Column display name (GitHub `name`).
    pub column_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectColumnRefRequest {
    pub owner: String,
    pub name: String,
    /// The board's per-repo `#N` — scopes the column to the board.
    pub project: i64,
    /// Column id.
    pub column: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectColumnUpdateRequest {
    pub owner: String,
    pub name: String,
    pub project: i64,
    pub column: String,
    pub column_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectColumnMoveRequest {
    pub owner: String,
    pub name: String,
    pub project: i64,
    pub column: String,
    /// New 0-based position inside the board (clamped).
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectDeleteSimpleResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectCardsListRequest {
    pub owner: String,
    pub name: String,
    /// Column id the cards belong to.
    pub column: String,
    /// Include archived cards (GitHub hides them by default).
    #[serde(default)]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectCardsListResponse {
    pub cards: Vec<ProjectCardPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectCardCreateRequest {
    pub owner: String,
    pub name: String,
    /// Column id the card lands in (appended last).
    pub column: String,
    /// Note text — a note card when `content_type`/`content_id` are absent.
    #[serde(default)]
    pub note: Option<String>,
    /// `issue` | `pull_request` — link an existing entity instead.
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub content_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectCardRefRequest {
    pub owner: String,
    pub name: String,
    /// The board's per-repo `#N` — scopes the card to the board.
    pub project: i64,
    /// Card id.
    pub card: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectCardUpdateRequest {
    pub owner: String,
    pub name: String,
    pub project: i64,
    pub card: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectCardMoveRequest {
    pub owner: String,
    pub name: String,
    pub project: i64,
    pub card: String,
    /// Destination column (omit = same column reorder).
    #[serde(default)]
    pub column: Option<String>,
    /// New 0-based position inside the destination column (clamped).
    pub position: i64,
}
