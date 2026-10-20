//! GitHub Pages RPC types — per-repo site config (`repo_pages`, existence =
//! enabled) and append-only `pages_builds` history. `build_type` is
//! `legacy` (serve `branch:path`, rebuilt on push) or `workflow` (Actions
//! deploy seam). Completed builds fire `page_build` lifecycle + webhook
//! events; serving resolves the newest `built` build's materialized tree.

use serde::{Deserialize, Serialize};

/// The serving subtree — GitHub `source` (`branch` + `path`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PagesSourcePublic {
    pub branch: String,
    /// `/` (repo root) or `/docs`.
    pub path: String,
}

/// A Pages site — mirrors GitHub `GET /repos/{o}/{r}/pages`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoPagesPublic {
    pub repo_id: String,
    /// `legacy` | `workflow`.
    pub build_type: String,
    pub source: PagesSourcePublic,
    /// `built` | `building` | `errored` | `None` — the newest build's state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub cname: String,
    pub custom_404: bool,
    pub https_enforced: bool,
    /// `/pages/{owner}/{repo}/` — the served site root.
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A Pages build — mirrors GitHub `GET /repos/{o}/{r}/pages/builds/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PagesBuildPublic {
    pub id: String,
    pub repo_id: String,
    /// `building` | `built` | `errored`.
    pub status: String,
    /// `legacy` | `workflow`.
    pub build_type: String,
    pub commit: String,
    pub branch: String,
    pub path: String,
    /// Served site root (`built` builds only; `""` otherwise).
    pub url: String,
    /// Build failure detail (`errored` builds only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub duration_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pusher_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pusher_username: Option<String>,
    /// Actions run id for `workflow` builds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// -- repo.pages.* -----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoPagesGetRequest {
    pub owner: String,
    pub name: String,
}

/// `repo.pages.enable` (POST /pages) — creates the site.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoPagesEnableRequest {
    pub owner: String,
    pub name: String,
    /// `legacy` (default) | `workflow`.
    #[serde(default)]
    pub build_type: Option<String>,
    pub source: Option<PagesSourcePublic>,
}

/// `repo.pages.update` (PUT /pages) — replaces the source/cname flags. All
/// fields optional; omitted fields keep their current value.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoPagesUpdateRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub build_type: Option<String>,
    #[serde(default)]
    pub source: Option<PagesSourcePublic>,
    #[serde(default)]
    pub cname: Option<String>,
    #[serde(default)]
    pub https_enforced: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoPagesDeleteResponse {
    pub ok: bool,
}

// -- repo.pages.builds.* ----------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PagesBuildsListRequest {
    pub owner: String,
    pub name: String,
    /// `built` | `errored` | `building` filter.
    #[serde(default)]
    pub status: Option<String>,
    /// Page size cap (server clamps to 100, default 30 like GitHub).
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PagesBuildsListResponse {
    pub builds: Vec<PagesBuildPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PagesBuildGetRequest {
    pub owner: String,
    pub name: String,
    /// Build id, or `"latest"` for the newest build of any status.
    pub build: String,
}

/// `repo.pages.builds.request` (POST /pages/builds) — queues a rebuild of
/// the configured source. `legacy` sites only (GitHub 422s on `workflow`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PagesBuildRequestRequest {
    pub owner: String,
    pub name: String,
}

// -- pages.deploys.* (job-token seam) ---------------------------------------

/// `pages.deploys.create` — the server-side half of `actions/deploy-pages`:
/// the job token minted with `pages: write` + `environment: github-pages`
/// calls this to publish a `github-pages` artifact as a `workflow` build.
/// Job/run/repository are all derived from the calling token's row — the
/// caller cannot point the deploy at another job's artifacts. Requires the
/// site configured with `build_type: workflow`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PagesDeployCreateRequest {
    pub owner: String,
    pub name: String,
    /// Artifact name to publish (default `github-pages`).
    #[serde(default)]
    pub artifact: Option<String>,
    /// Artifact row id — wins over `artifact` when set. The GitHub-shaped
    /// REST `pages/deployment` route translates `artifact_url` into this.
    #[serde(default)]
    pub artifact_id: Option<String>,
}
