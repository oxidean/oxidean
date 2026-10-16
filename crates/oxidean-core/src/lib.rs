//! Shared domain types for Oxidean.

pub mod action_types;
pub mod auth_types;
pub mod deploy_key_types;
pub mod email_types;
pub mod gpg_key_types;
pub mod issue_types;
pub mod languages;
pub mod mcp_types;
pub mod mirror_types;
pub mod notification_types;
pub mod oauth_types;
pub mod org_types;
pub mod package_types;
pub mod pat_types;
pub mod proc_classes;
pub mod protection_types;
pub mod pull_types;
pub mod release_types;
pub mod repo_types;
pub mod scope_catalog;
pub mod search_types;
pub mod two_factor_types;
pub mod ssh_key_types;
pub mod webhook_types;

pub use action_types::*;
pub use auth_types::*;
pub use deploy_key_types::*;
pub use email_types::*;
pub use gpg_key_types::*;
pub use issue_types::*;
pub use languages::*;
pub use mcp_types::*;
pub use mirror_types::*;
pub use notification_types::*;
pub use oauth_types::*;
pub use org_types::*;
pub use package_types::*;
pub use pat_types::*;
pub use proc_classes::*;
pub use protection_types::*;
pub use pull_types::*;
pub use release_types::*;
pub use repo_types::*;
pub use scope_catalog::*;
pub use search_types::*;
pub use two_factor_types::*;
pub use ssh_key_types::*;
pub use webhook_types::*;

use serde::{Deserialize, Serialize};

pub fn crate_name() -> &'static str {
    "oxidean-core"
}

/// Legacy RPC protocol ordinal. Frozen at `1`: `system.manifest` keeps emitting
/// it as the `protocol_version` alias while `WIRE_VERSION` remains `0.x`, and
/// the `Oxidean-RPC-Version` header value `"1"` is accepted as the legacy
/// alias for the `0.1.x` wire contract.
pub const RPC_PROTOCOL_VERSION: u32 = 1;
/// SemVer wire-contract version (registry entry `wire` in `versioning.toml`).
/// Sent on `Oxidean-RPC-Version`, surfaced in `system.manifest`, and rendered
/// into the OpenAPI `info.version`.
pub const WIRE_VERSION: &str = "0.1.0";
/// The wire contract `Oxidean-RPC-Version: 1` maps to during the 0.x
/// transition window.
pub const LEGACY_WIRE_VERSION: &str = "0.1.0";
/// MCP protocol revisions this implementation negotiates, oldest → newest.
/// External spec dates — not Oxidean SemVer. The generated TS client re-emits
/// this list; every other pin site must read it from here.
pub const MCP_PROTOCOL_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];
pub const ECHO_MAX_BYTES: usize = 8192;

/// Parses `major.minor.patch` (optional `-prerelease`/`+build` ignored for
/// compatibility purposes). Returns `(major, minor)`.
fn semver_major_minor(version: &str) -> Option<(u64, u64)> {
    let core = version
        .trim()
        .split('+')
        .next()?
        .split('-')
        .next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor))
}

/// Shared SemVer compatibility rule for the wire contract (BR6.1):
/// below `1.0.0`, versions are compatible when the minor matches
/// (`0.MINOR.x` ↔ `0.MINOR.y`); at or above `1.0.0`, when the major matches.
/// Unparseable input is incompatible.
pub fn wire_compatible(a: &str, b: &str) -> bool {
    match (semver_major_minor(a), semver_major_minor(b)) {
        (Some((am, an)), Some((bm, bn))) if am == 0 && bm == 0 => an == bn,
        (Some((am, _)), Some((bm, _))) => am != 0 && am == bm,
        _ => false,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    /// `"ok"`, `"error"`, or `"skipped"` when no DATABASE_URL / ping not attempted.
    pub database: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DbProbeResponse {
    pub dialect: String,
    pub probe_count: i64,
    pub probed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EchoRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EchoResponse {
    pub message: String,
}

/// Optional-instance-feature flags advertised by `system.manifest` (CLI-02).
/// Fields default to `false` so older manifests deserialize cleanly.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ManifestCapabilities {
    /// `/api/mcp` agent surface.
    #[serde(default)]
    pub mcp: bool,
    /// `/api/v1` typed REST facade.
    #[serde(default)]
    pub rest: bool,
    /// `/oauth/*` provider endpoints (incl. RFC 8628 device flow).
    #[serde(default)]
    pub oauth: bool,
    /// `/cli/*` binary distribution, install scripts, and agent SKILL.md.
    #[serde(default)]
    pub cli_dist: bool,
}

/// `system.manifest` response — the server-driven compatibility contract that
/// lets clients (the `ox` CLI, third-party tools) feature-gate on the exact
/// procedure set this instance's dispatch table knows.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ManifestResponse {
    /// Legacy wire ordinal — frozen at `1` while `WIRE_VERSION` is `0.x`.
    /// New clients read `wire_version`; this field exists for old ones.
    pub protocol_version: u32,
    /// SemVer wire-contract version (`WIRE_VERSION`).
    pub wire_version: String,
    /// Server build version (`oxidean-api` crate version).
    pub server_version: String,
    /// Every procedure the dispatch table handles, mapped to `true`.
    pub procedures: std::collections::BTreeMap<String, bool>,
    pub capabilities: ManifestCapabilities,
    /// Oldest `ox` CLI version this instance guarantees compatibility with.
    pub min_cli_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum RpcInput {
    Empty(serde_json::Value),
    Echo(EchoRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RpcRequest {
    pub procedure: String,
    #[serde(default = "default_input")]
    pub input: serde_json::Value,
}

fn default_input() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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

    #[test]
    fn wire_compat_0x_shares_minor() {
        assert!(wire_compatible("0.1.0", "0.1.5"));
        assert!(wire_compatible("0.1.0", "0.1.0"));
        assert!(!wire_compatible("0.1.0", "0.2.0"));
        assert!(!wire_compatible("0.2.0", "0.1.0"));
    }

    #[test]
    fn wire_compat_stable_shares_major() {
        assert!(wire_compatible("1.0.0", "1.4.2"));
        assert!(wire_compatible("2.0.0", "2.9.9"));
        assert!(!wire_compatible("1.9.0", "2.0.0"));
        assert!(!wire_compatible("2.0.0", "1.9.9"));
    }

    #[test]
    fn wire_compat_rejects_crossing_and_garbage() {
        assert!(!wire_compatible("0.9.9", "1.0.0"));
        assert!(!wire_compatible("1.0.0", "0.9.9"));
        assert!(!wire_compatible("garbage", "0.1.0"));
        assert!(!wire_compatible("0.1.0", "x"));
        assert!(!wire_compatible("", "0.1.0"));
        assert!(!wire_compatible("0.1", "0.1.0"));
        assert!(!wire_compatible("0.1.0.1", "0.1.0"));
    }

    #[test]
    fn wire_compat_ignores_prerelease_and_build() {
        assert!(wire_compatible("0.1.0-beta.1", "0.1.0"));
        assert!(wire_compatible("0.1.0", "0.1.2+build5"));
    }

    #[test]
    fn manifest_response_carries_wire_version() {
        let manifest = ManifestResponse {
            protocol_version: RPC_PROTOCOL_VERSION,
            wire_version: WIRE_VERSION.to_string(),
            server_version: "0.1.0".to_string(),
            procedures: std::collections::BTreeMap::new(),
            capabilities: ManifestCapabilities::default(),
            min_cli_version: "0.1.0".to_string(),
        };
        let json = serde_json::to_value(&manifest).unwrap();
        assert_eq!(json["wire_version"], "0.1.0");
        assert_eq!(json["protocol_version"], 1);
        let back: ManifestResponse = serde_json::from_value(json).unwrap();
        assert_eq!(back.wire_version, WIRE_VERSION);
    }

    #[test]
    fn mcp_pins_are_external_dates() {
        assert_eq!(MCP_PROTOCOL_VERSIONS.last(), Some(&"2025-06-18"));
        assert!(MCP_PROTOCOL_VERSIONS.contains(&"2024-11-05"));
    }
}
