//! Per-repo deploy key DTOs for admin RPC (GIT-23).
//!
//! Deploy keys are transport-only credentials for Git over SSH: they authorize
//! pack operations on one repository and never resolve to an account identity,
//! so they grant no session, RPC, or web access. List items may include the
//! public key material (not a secret); no one-time reveal token exists.

use serde::{Deserialize, Serialize};

/// `repo.deployKey.create` input — admin-gated.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeployKeyCreateRequest {
    pub owner: String,
    pub name: String,
    /// Required title/note shown in settings.
    pub title: String,
    /// OpenSSH authorized_keys line (`ssh-ed25519 AAAA… comment`).
    pub public_key: String,
    /// `false` (default) → read-only (upload-pack only); `true` → read/write.
    #[serde(default)]
    pub can_write: bool,
}

/// `repo.deployKey.delete` input — admin-gated.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeployKeyDeleteRequest {
    pub owner: String,
    pub name: String,
    pub id: String,
}

/// List / detail item — public key optional; no secret fields.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeployKeyPublic {
    pub id: String,
    pub repo_id: String,
    pub title: String,
    pub fingerprint: String,
    pub key_type: String,
    pub can_write: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_ip: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

/// `repo.deployKey.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeployKeyListResponse {
    pub keys: Vec<DeployKeyPublic>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_key_public_has_no_token_or_secret_field() {
        let item = DeployKeyPublic {
            id: "dk1".into(),
            repo_id: "r1".into(),
            title: "ci".into(),
            fingerprint: "SHA256:deadbeef".into(),
            key_type: "ssh-ed25519".into(),
            can_write: false,
            public_key: Some("ssh-ed25519 AAAA ci".into()),
            last_used_at: None,
            last_used_ip: None,
            created_by: "u1".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let v = serde_json::to_value(&item).unwrap();
        assert!(v.get("token").is_none());
        assert!(v.get("secret").is_none());
        assert_eq!(v["fingerprint"], "SHA256:deadbeef");
        assert_eq!(v["can_write"], false);
    }

    #[test]
    fn create_request_defaults_read_only() {
        let req: DeployKeyCreateRequest = serde_json::from_value(serde_json::json!({
            "owner": "ada",
            "name": "demo",
            "title": "ci",
            "public_key": "ssh-ed25519 AAAA ci"
        }))
        .unwrap();
        assert!(!req.can_write, "can_write must default to read-only");
    }
}
