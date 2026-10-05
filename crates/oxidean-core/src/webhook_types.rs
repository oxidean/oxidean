//! Webhook domain DTOs for `webhook.*` RPC (Phase 18 / HOOK-01..03).

use serde::{Deserialize, Serialize};

/// Public webhook config — secret is always masked except create/rotate reveal.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebhookPublic {
    pub id: String,
    pub repo_id: String,
    pub url: String,
    /// Masked secret (never full value) — D-HOOK-16.
    pub secret_masked: String,
    pub active: bool,
    pub events: Vec<String>,
    pub name: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    /// One-time plaintext secret on create/rotate only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateWebhookRequest {
    pub owner: String,
    pub name: String,
    pub url: String,
    pub secret: String,
    pub events: Vec<String>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateWebhookRequest {
    pub owner: String,
    pub name: String,
    pub id: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub events: Option<Vec<String>>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebhookIdRequest {
    pub owner: String,
    pub name: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebhookListRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebhookListResponse {
    pub webhooks: Vec<WebhookPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteWebhookResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebhookDeliveryPublic {
    pub id: String,
    pub webhook_id: String,
    pub delivery_guid: String,
    pub event: String,
    pub action: String,
    pub status: String,
    pub created_at: String,
    #[serde(default)]
    pub http_status: Option<i32>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub attempt_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebhookDeliveriesListRequest {
    pub owner: String,
    pub name: String,
    pub webhook_id: String,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebhookDeliveriesListResponse {
    pub deliveries: Vec<WebhookDeliveryPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebhookDeliveryGetRequest {
    pub owner: String,
    pub name: String,
    pub webhook_id: String,
    pub delivery_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebhookRedeliverRequest {
    pub owner: String,
    pub name: String,
    pub webhook_id: String,
    pub delivery_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WebhookPingResponse {
    pub delivery_id: String,
    pub delivery_guid: String,
}
