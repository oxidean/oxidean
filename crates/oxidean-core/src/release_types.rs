//! Release domain DTOs for `release.*` RPC (Phase 15 / GIT-14).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReleasePublic {
    pub id: String,
    pub repo_id: String,
    pub tag_name: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
    pub prerelease: bool,
    pub author_id: String,
    pub author_username: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub assets: Vec<ReleaseAssetPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReleaseAssetPublic {
    pub id: String,
    pub release_id: String,
    pub filename: String,
    pub content_type: String,
    pub byte_size: i64,
    pub uploader_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateReleaseRequest {
    pub owner: String,
    pub name: String,
    pub tag_name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReleaseListRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReleaseListResponse {
    pub releases: Vec<ReleasePublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReleaseGetRequest {
    pub owner: String,
    pub name: String,
    pub tag_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateReleaseRequest {
    pub owner: String,
    pub name: String,
    pub tag_name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub draft: Option<bool>,
    #[serde(default)]
    pub prerelease: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteReleaseRequest {
    pub owner: String,
    pub name: String,
    pub tag_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteReleaseResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteReleaseAssetRequest {
    pub owner: String,
    pub name: String,
    pub asset_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteReleaseAssetResponse {
    pub ok: bool,
}
