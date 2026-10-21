//! Wiki RPC types — the git-backed wiki domain. Pages are Markdown files in
//! the repository's `<name>.wiki.git` sibling bare (GitHub/Gitea layout):
//! `wiki.pages.*` read/write the files, `wiki.revisions.list` reads git log.
//! `gollum` lifecycle + webhook events fire on page create/update.

use serde::{Deserialize, Serialize};

/// A wiki page entry (file name without `.md`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiPagePublic {
    /// GitHub `page_name` — the file stem (`Home`, `Page-Name`, `_Sidebar`).
    pub page_name: String,
    /// `/owner/repo/wiki/{page_name}` web route.
    pub html_url: String,
    /// `_Sidebar` / `_Footer` / `_Header` — GitHub's reserved meta pages.
    pub reserved: bool,
    /// Tip commit touching the page (empty on a fresh create response before
    /// history exists — create/update responses always carry it).
    #[serde(default)]
    pub sha: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Present on `wiki.pages.get` — raw Markdown body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// A wiki page revision from git history.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiRevisionPublic {
    pub sha: String,
    pub message: String,
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub author_username: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiPagesListRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiPagesListResponse {
    pub pages: Vec<WikiPagePublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiPageGetRequest {
    pub owner: String,
    pub name: String,
    /// Page name — the `.md` file stem (`Home`, `_Sidebar`, …).
    pub page: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiPageCreateRequest {
    pub owner: String,
    pub name: String,
    /// Display title — becomes the file name (`My Page` → `My-Page.md`).
    pub title: String,
    pub content: String,
    /// Commit message — optional; defaults to GitHub's `Created {title}`.
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiPageUpdateRequest {
    pub owner: String,
    pub name: String,
    /// Page name to edit (file stem).
    pub page: String,
    pub content: String,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiPageDeleteRequest {
    pub owner: String,
    pub name: String,
    pub page: String,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiPageWriteResponse {
    pub page: WikiPagePublic,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiPageDeleteResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiRevisionsListRequest {
    pub owner: String,
    pub name: String,
    pub page: String,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WikiRevisionsListResponse {
    pub revisions: Vec<WikiRevisionPublic>,
}
