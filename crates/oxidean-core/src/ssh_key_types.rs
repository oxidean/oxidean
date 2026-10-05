//! SSH public key DTOs for session RPC (GIT-04, D-SSH-05).
//!
//! List/response items may include the public key material (not a secret) but
//! never invent a one-time reveal token field.

use serde::{Deserialize, Serialize};

/// `sshKey.add` input — title/note required; public key is the OpenSSH one-line form.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddSshKeyRequest {
    /// Required title/note (D-SSH-05).
    pub title: String,
    /// OpenSSH authorized_keys line (`ssh-ed25519 AAAA… comment`).
    pub public_key: String,
    /// When omitted, defaults to true (Git SSH auth).
    #[serde(default = "default_true")]
    pub can_authenticate: bool,
    /// When omitted, defaults to true (commit signature verify).
    #[serde(default = "default_true")]
    pub can_sign: bool,
}

fn default_true() -> bool {
    true
}

/// List / metadata item — public key optional; no one-time secret field (D-SSH-05).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SshKeyListItem {
    pub id: String,
    pub title: String,
    pub fingerprint: String,
    pub key_type: String,
    pub can_authenticate: bool,
    pub can_sign: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_ip: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_key_list_item_has_no_token_or_secret_field() {
        let item = SshKeyListItem {
            id: "k1".into(),
            title: "laptop".into(),
            fingerprint: "SHA256:deadbeef".into(),
            key_type: "ssh-ed25519".into(),
            can_authenticate: true,
            can_sign: true,
            public_key: Some("ssh-ed25519 AAAA laptop".into()),
            last_used_at: None,
            last_used_ip: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let v = serde_json::to_value(&item).unwrap();
        assert!(v.get("token").is_none());
        assert!(v.get("secret").is_none());
        assert_eq!(v["fingerprint"], "SHA256:deadbeef");
        assert_eq!(v["title"], "laptop");
        assert_eq!(v["can_sign"], true);
    }

    #[test]
    fn add_request_defaults_usage_flags() {
        let req: AddSshKeyRequest = serde_json::from_value(serde_json::json!({
            "title": "ci",
            "public_key": "ssh-ed25519 AAAA ci"
        }))
        .unwrap();
        assert!(req.can_authenticate);
        assert!(req.can_sign);
    }
}
