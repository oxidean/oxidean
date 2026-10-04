//! Shared domain types for Oxidean.

pub mod action_types;
pub mod auth_types;
pub mod issue_types;
pub mod languages;
pub mod mirror_types;
pub mod notification_types;
pub mod org_types;
pub mod package_types;
pub mod pat_types;
pub mod protection_types;
pub mod pull_types;
pub mod release_types;
pub mod repo_types;
pub mod search_types;
pub mod deploy_key_types;
pub mod ssh_key_types;
pub mod gpg_key_types;
pub mod email_types;
pub mod webhook_types;

pub use action_types::*;
pub use auth_types::*;
pub use issue_types::*;
pub use languages::*;
pub use mirror_types::*;
pub use notification_types::*;
pub use org_types::*;
pub use package_types::*;
pub use pat_types::*;
pub use protection_types::*;
pub use pull_types::*;
pub use release_types::*;
pub use repo_types::*;
pub use search_types::*;
pub use deploy_key_types::*;
pub use ssh_key_types::*;
pub use gpg_key_types::*;
pub use email_types::*;
pub use webhook_types::*;

use serde::{Deserialize, Serialize};

pub fn crate_name() -> &'static str {
    "oxidean-core"
}

pub const RPC_PROTOCOL_VERSION: u32 = 1;
pub const ECHO_MAX_BYTES: usize = 8192;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    /// `"ok"`, `"error"`, or `"skipped"` when no DATABASE_URL / ping not attempted.
    pub database: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbProbeResponse {
    pub dialect: String,
    pub probe_count: i64,
    pub probed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoResponse {
    pub message: String,
}

/// Optional-instance-feature flags advertised by `system.manifest` (CLI-02).
/// All `false` today — they flip on as the corresponding surfaces land
/// (MCP: AGT-01, a typed REST surface, instance-as-OAuth-provider: API-03).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManifestCapabilities {
    pub mcp: bool,
    pub rest: bool,
    pub oauth: bool,
}

/// `system.manifest` response — the server-driven compatibility contract that
/// lets clients (the `ox` CLI, third-party tools) feature-gate on the exact
/// procedure set this instance's dispatch table knows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestResponse {
    /// RPC wire protocol version (`RPC_PROTOCOL_VERSION`).
    pub protocol_version: u32,
    /// Server build version (`oxidean-api` crate version).
    pub server_version: String,
    /// Every procedure the dispatch table handles, mapped to `true`.
    pub procedures: std::collections::BTreeMap<String, bool>,
    pub capabilities: ManifestCapabilities,
    /// Oldest `ox` CLI version this instance guarantees compatibility with.
    pub min_cli_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcInput {
    Empty(serde_json::Value),
    Echo(EchoRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub procedure: String,
    #[serde(default = "default_input")]
    pub input: serde_json::Value,
}

fn default_input() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcResponse {
    Ok { ok: bool, data: serde_json::Value },
    Err { ok: bool, error: AppError },
}

impl RpcResponse {
    pub fn ok(data: impl Serialize) -> Self {
        Self::Ok {
            ok: true,
            data: serde_json::to_value(data).unwrap_or(serde_json::json!({})),
        }
    }

    pub fn err(error: AppError) -> Self {
        Self::Err { ok: false, error }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name() {
        assert_eq!(crate_name(), "oxidean-core");
    }
}
