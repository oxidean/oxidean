//! GPG / OpenPGP public key DTOs for session RPC (commit signing).

use serde::{Deserialize, Serialize};

/// `gpgKey.add` input — armored public key only (never a private key).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddGpgKeyRequest {
    /// Required title/note.
    pub title: String,
    /// ASCII-armored OpenPGP public key block.
    pub armored_public_key: String,
}

/// List / metadata item for a registered GPG public key.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GpgKeyListItem {
    pub id: String,
    pub title: String,
    pub fingerprint: String,
    pub key_id: String,
    /// Email addresses parsed from key UIDs.
    pub uid_emails: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armored_public_key: Option<String>,
    pub created_at: String,
}

/// `gpgKey.revoke` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RevokeGpgKeyRequest {
    pub id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpg_list_item_shape() {
        let item = GpgKeyListItem {
            id: "g1".into(),
            title: "laptop".into(),
            fingerprint: "ABCD".into(),
            key_id: "ABCD1234".into(),
            uid_emails: vec!["a@example.com".into()],
            armored_public_key: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let v = serde_json::to_value(&item).unwrap();
        assert!(v.get("secret").is_none());
        assert_eq!(v["key_id"], "ABCD1234");
    }
}
