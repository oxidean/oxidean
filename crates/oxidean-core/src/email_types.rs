//! Account email address DTOs (`email.list` / `add` / `remove` / `setPrimary` / `resendVerify`).

use serde::{Deserialize, Serialize};

/// `email.add` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddEmailRequest {
    pub email: String,
}

/// `email.remove` / `email.setPrimary` / `email.resendVerify` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmailIdRequest {
    pub id: String,
}

/// One account email row for settings UI.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmailListItem {
    pub id: String,
    pub email: String,
    pub is_primary: bool,
    pub verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    pub created_at: String,
}
