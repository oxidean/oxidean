//! Saved-reply RPC types — per-user comment templates (GitHub "saved
//! replies"). User-scoped in v1: every procedure operates on the caller's
//! own rows; there is no repo/org binding.

use serde::{Deserialize, Serialize};

/// A saved reply as returned to its owner.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SavedReplyPublic {
    pub id: String,
    /// Picker label (GitHub `name`).
    pub name: String,
    /// Markdown body inserted into a composer.
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SavedRepliesListResponse {
    pub replies: Vec<SavedReplyPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SavedReplyCreateRequest {
    /// Picker label — 1..=80 chars after trim.
    pub name: String,
    /// Markdown body — non-empty after trim.
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SavedReplyUpdateRequest {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SavedReplyRefRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SavedReplyDeleteResponse {
    pub ok: bool,
}
