//! MCP endpoint admin settings types (AGT-03).

use serde::{Deserialize, Serialize};

/// `admin.mcp.getSettings` — effective MCP endpoint state (no secrets).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminMcpSettingsPublic {
    /// Effective state: stored admin override, else the `OXIDEAN_MCP_ENABLED`
    /// environment default (true when unset).
    pub enabled: bool,
    /// True when an admin override row is stored; false means the env default
    /// is in force (same env-default/override split as `admin.lfs.*`).
    pub enabled_overridden: bool,
}

/// `admin.mcp.updateSettings` — set or clear the MCP enable override.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminMcpUpdateSettingsRequest {
    /// `Some(bool)` stores an explicit override; omitted/null keeps the stored
    /// value (mirrors `admin.lfs.updateSettings` semantics).
    #[serde(default)]
    pub enabled: Option<bool>,
    /// When true, clear the override so the env default applies again.
    #[serde(default)]
    pub clear_overrides: bool,
}
