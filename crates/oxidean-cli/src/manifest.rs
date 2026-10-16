//! `system.manifest` compatibility gate (CLI-02).
//!
//! Every RPC-backed command first fetches the instance's capability manifest
//! and feature-gates on its `procedures` map, so a server API change surfaces
//! as "command not supported" instead of a hard failure. The fetch always
//! **fails open**: a pre-manifest server (`rpc.unknown_procedure`), an
//! unreachable instance, a timeout, or a malformed payload prints a stderr
//! note and the command proceeds ungated — compatibility must never be the
//! reason a call breaks.

use std::collections::BTreeMap;
use std::time::Duration;

use oxidean_core::{
    wire_compatible, RpcResponse, LEGACY_WIRE_VERSION, RPC_PROTOCOL_VERSION, WIRE_VERSION,
};
use serde::Deserialize;
use serde_json::json;

use crate::rpc::Client;

/// Manifest call budget — a hung instance must not stall the command.
const FETCH_TIMEOUT: Duration = Duration::from_secs(3);

/// Client-side view of `system.manifest`. Tolerant on purpose (`serde`
/// defaults + unknown fields ignored): only `procedures` is required — a
/// payload without it isn't a manifest and is treated as fail-open.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    /// Legacy wire ordinal — `1` on every pre-SemVer-wire server.
    #[serde(default)]
    pub protocol_version: u32,
    /// SemVer wire contract (`WIRE_VERSION`). Absent on pre-wire manifests —
    /// `protocol_version: 1` then means the legacy `0.1.x` contract (BR6.4).
    #[serde(default)]
    pub wire_version: Option<String>,
    #[serde(default)]
    pub server_version: String,
    /// Required — the whole point of the contract.
    pub procedures: BTreeMap<String, bool>,
    #[serde(default)]
    pub min_cli_version: String,
}

impl Manifest {
    /// `true` only when the manifest explicitly lists `proc` as `true`.
    pub fn supports(&self, procedure: &str) -> bool {
        self.procedures.get(procedure).copied().unwrap_or(false)
    }
}

/// Fetch `system.manifest` once per process. `None` means "run ungated" —
/// the reason was already reported on stderr (never stdout, so `--json`
/// pipelines stay clean).
pub async fn fetch(client: &Client) -> Option<Manifest> {
    let call = client.call("system.manifest", json!({}));
    let resp = match tokio::time::timeout(FETCH_TIMEOUT, call).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            eprintln!("ox: note: cannot fetch instance manifest ({e}) — running ungated");
            return None;
        }
        Err(_) => {
            eprintln!(
                "ox: note: system.manifest timed out after {}s — running ungated",
                FETCH_TIMEOUT.as_secs()
            );
            return None;
        }
    };
    match resp {
        RpcResponse::Ok { data, .. } => match serde_json::from_value::<Manifest>(data) {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("ox: note: malformed system.manifest ({e}) — running ungated");
                None
            }
        },
        RpcResponse::Err { error, .. } => {
            if error.code == "rpc.unknown_procedure" {
                eprintln!("ox: note: instance predates system.manifest — running ungated");
            } else {
                eprintln!(
                    "ox: note: system.manifest failed ({}: {}) — running ungated",
                    error.code, error.message
                );
            }
            None
        }
    }
}

/// Advisory stderr notices: the instance demands a newer `ox`, or speaks a
/// newer wire protocol. Never blocks — the caller prints each line.
pub fn compat_notices(manifest: &Manifest, cli_version: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let (Some(min), Some(cur)) = (
        semver_tuple(&manifest.min_cli_version),
        semver_tuple(cli_version),
    ) {
        if min > cur {
            out.push(format!(
                "warning: instance requires ox >= {} (installed {cli_version}) — upgrade ox",
                manifest.min_cli_version
            ));
        }
    }
    // Wire compatibility (BR6.4): `wire_version` when the server advertises
    // it; a bare `protocol_version: 1` means the legacy 0.1.x contract; both
    // absent → compatibility is unknown, warn only.
    match &manifest.wire_version {
        Some(v) => {
            if !wire_compatible(v, WIRE_VERSION) {
                out.push(format!(
                    "warning: instance wire version {v} is incompatible with \
                     installed ox (wire {WIRE_VERSION}) — upgrade ox"
                ));
            }
        }
        None if manifest.protocol_version == RPC_PROTOCOL_VERSION => {
            if !wire_compatible(LEGACY_WIRE_VERSION, WIRE_VERSION) {
                out.push(format!(
                    "warning: instance speaks the legacy wire contract \
                     ({LEGACY_WIRE_VERSION}), installed ox uses wire \
                     {WIRE_VERSION} — upgrade ox"
                ));
            }
        }
        None => {
            out.push(
                "note: instance did not advertise a wire version — \
                 compatibility unchecked"
                    .into(),
            );
        }
    }
    out
}

/// `x.y.z` → tuple; tolerates a `v` prefix and `-pre`/`+build` suffixes.
/// `None` on anything unparseable (callers treat it as "cannot compare").
fn semver_tuple(version: &str) -> Option<(u64, u64, u64)> {
    let v = version.trim().trim_start_matches('v');
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(procs: &[(&str, bool)], min_cli: &str) -> Manifest {
        Manifest {
            protocol_version: 1,
            wire_version: Some(WIRE_VERSION.into()),
            server_version: "9.9.9".into(),
            procedures: procs.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            min_cli_version: min_cli.into(),
        }
    }

    #[test]
    fn supports_only_explicit_true() {
        let m = manifest(&[("repo.get", true), ("issue.list", false)], "0.1.0");
        assert!(m.supports("repo.get"));
        assert!(!m.supports("issue.list"));
        assert!(!m.supports("pull.get"));
    }

    #[test]
    fn deserialize_requires_procedures_map() {
        let ok: Manifest = serde_json::from_value(json!({
            "protocol_version": 1,
            "server_version": "0.1.0",
            "procedures": {"repo.get": true},
            "capabilities": {"mcp": false, "rest": false, "oauth": false},
            "min_cli_version": "0.1.0",
            "future_field": {"ignored": true}
        }))
        .unwrap();
        assert!(ok.supports("repo.get"));
        assert!(serde_json::from_value::<Manifest>(json!({"protocol_version": 1})).is_err());
    }

    #[test]
    fn semver_parsing() {
        assert_eq!(semver_tuple("0.1.0"), Some((0, 1, 0)));
        assert_eq!(semver_tuple("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(semver_tuple("1.2.3-rc.1"), Some((1, 2, 3)));
        assert_eq!(semver_tuple("1.2"), Some((1, 2, 0)));
        assert_eq!(semver_tuple("garbage"), None);
        assert_eq!(semver_tuple("1.2.3.4"), None);
    }

    #[test]
    fn notices_when_instance_needs_newer_cli() {
        let m = manifest(&[], "99.0.0");
        let notices = compat_notices(&m, "0.1.0");
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("99.0.0"), "{notices:?}");
        assert!(compat_notices(&m, "99.0.0").is_empty());
    }

    #[test]
    fn notices_for_incompatible_wire() {
        let mut m = manifest(&[], "0.1.0");
        m.wire_version = Some("0.2.0".into());
        let notices = compat_notices(&m, "0.1.0");
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("0.2.0"), "{notices:?}");
        m.wire_version = Some(WIRE_VERSION.into());
        assert!(compat_notices(&m, "0.1.0").is_empty());
    }

    /// BR6.4: a manifest with no `wire_version` but `protocol_version: 1` is
    /// the legacy 0.1.x contract — compatible while `WIRE_VERSION` is 0.1.x.
    #[test]
    fn legacy_manifest_is_compatible() {
        let mut m = manifest(&[], "0.1.0");
        m.wire_version = None;
        assert!(compat_notices(&m, "0.1.0").is_empty());
    }

    /// BR6.4: neither field → warn-and-continue, never a hard refusal.
    #[test]
    fn wireless_manifest_warns_only() {
        let mut m = manifest(&[], "0.1.0");
        m.wire_version = None;
        m.protocol_version = 0;
        let notices = compat_notices(&m, "0.1.0");
        assert_eq!(notices.len(), 1);
        assert!(notices[0].starts_with("note:"), "{notices:?}");
    }

    #[test]
    fn unparseable_min_version_is_quiet() {
        let m = manifest(&[], "not-a-version");
        assert!(compat_notices(&m, "0.1.0").is_empty());
    }
}
