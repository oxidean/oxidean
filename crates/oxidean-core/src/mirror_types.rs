//! Two-way repository mirror RPC types (GIT-V2-01).

use serde::{Deserialize, Serialize};

/// Public mirror settings (secrets never returned in plaintext).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorPublic {
    pub id: String,
    pub repository_id: String,
    pub remote_url: String,
    /// `https_token` | `ssh_key`
    pub auth_kind: String,
    pub username: String,
    pub has_secret: bool,
    pub ssh_public_key: String,
    pub known_hosts: String,
    pub webhook_url: String,
    pub webhook_secret_masked: String,
    pub poll_interval_secs: i64,
    pub enabled: bool,
    /// `merge` | `exact`
    pub sync_mode: String,
    pub last_synced_at: Option<String>,
    /// `never` | `ok` | `error` | `conflict` | `running`
    pub last_status: String,
    pub last_error: String,
    pub created_at: String,
    pub updated_at: String,
    pub ref_results: Vec<RepoMirrorRefResultPublic>,
    /// Present only on create / rotateWebhookSecret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorRefResultPublic {
    pub refname: String,
    pub outcome: String,
    pub local_oid: String,
    pub remote_oid: String,
    pub detail: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorGetRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorGetResponse {
    pub mirror: Option<RepoMirrorPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorUpsertRequest {
    pub owner: String,
    pub name: String,
    pub remote_url: String,
    /// `https_token` | `ssh_key`
    pub auth_kind: String,
    #[serde(default)]
    pub username: Option<String>,
    /// Omit or empty to keep existing secret.
    #[serde(default)]
    pub secret: Option<String>,
    /// SSH public key (optional display override after generate).
    #[serde(default)]
    pub ssh_public_key: Option<String>,
    #[serde(default)]
    pub known_hosts: Option<String>,
    #[serde(default)]
    pub poll_interval_secs: Option<i64>,
    #[serde(default)]
    pub enabled: Option<bool>,
    /// `merge` | `exact` — omit to keep existing / default `merge` on create.
    #[serde(default)]
    pub sync_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorDeleteRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorDeleteResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorSyncNowRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorSyncNowResponse {
    pub enqueued: bool,
    pub mirror: RepoMirrorPublic,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorGenerateSshKeyRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorGenerateSshKeyResponse {
    pub ssh_public_key: String,
    pub mirror: RepoMirrorPublic,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorRotateWebhookSecretRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorRotateWebhookSecretResponse {
    pub webhook_secret: String,
    pub mirror: RepoMirrorPublic,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorFetchHostKeyRequest {
    pub owner: String,
    pub name: String,
    /// SSH remote URL (`git@host:path` or `ssh://…`). Host is scanned via ssh-keyscan.
    pub remote_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoMirrorFetchHostKeyResponse {
    pub host: String,
    pub known_hosts: String,
}
