//! Package registry RPC DTOs (PKG-05).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackagesListRequest {
    /// Owner username or org slug.
    #[serde(default)]
    pub owner: Option<String>,
    /// Filter to packages linked to this repository id.
    #[serde(default)]
    pub repository_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackageVersionPublic {
    /// Row id — the `{version_id}` the GitHub-shaped REST routes address.
    #[serde(default)]
    pub id: String,
    pub version: String,
    pub digest: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackagePublic {
    pub id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub name: String,
    pub format: String,
    pub visibility: String,
    pub repository_id: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    pub versions: Vec<PackageVersionPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackagesListResponse {
    pub packages: Vec<PackagePublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackagesDeleteVersionRequest {
    pub package_id: String,
    pub version: String,
    /// Must equal `{name}@{version}` (D-PKG-12).
    pub confirm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackagesDeleteVersionResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackagesDeleteRequest {
    pub package_id: String,
    /// Must equal the package name (same safety gate as deleteVersion).
    pub confirm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackagesDeleteResponse {
    pub ok: bool,
}

/// Admin: package storage usage for an owner (D-PKG-09).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackagesAdminUsageRequest {
    /// Owner username or org slug.
    pub owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackageUsageByFormat {
    pub format: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackageUsageRow {
    pub package_id: String,
    pub name: String,
    pub format: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackagesAdminUsageResponse {
    pub owner_type: String,
    pub owner_id: String,
    pub used_bytes: u64,
    pub quota_bytes: u64,
    pub default_quota_bytes: u64,
    pub by_format: Vec<PackageUsageByFormat>,
    pub packages: Vec<PackageUsageRow>,
}

/// Admin: set per-owner package quota override (D-PKG-09).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackagesAdminSetQuotaRequest {
    pub owner: String,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackagesAdminSetQuotaResponse {
    pub ok: bool,
    pub max_bytes: u64,
}
