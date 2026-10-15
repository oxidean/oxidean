//! Serving version for self-reported surfaces (manifest, health, MCP
//! serverInfo).
//!
//! Resolution order:
//!   1. `/usr/local/share/oxidean/VERSION` — written into the image by the
//!      Dockerfile `VERSION` build-arg / `docker/Dockerfile.release-layer`,
//!      so a `chore(release):` bump commit ships as a layer on the previous
//!      image and still reports the released version.
//!   2. `OXIDEAN_VERSION` env — escape hatch for non-image deployments that
//!      can't write the file.
//!   3. `env!("CARGO_PKG_VERSION")` — the manifest version (self-host,
//!      Compose, and cargo-run builds; always correct at compile time).
//!
//! The file wins over the env so a stray variable can never mis-report what
//! the deployed artifact actually is.

use std::sync::OnceLock;

const VERSION_FILE: &str = "/usr/local/share/oxidean/VERSION";

/// The version this binary reports on serving paths. Resolved once and
/// cached — file and env are read at first use.
pub fn version() -> &'static str {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| {
        if let Ok(raw) = std::fs::read_to_string(VERSION_FILE) {
            let v = raw.trim();
            if !v.is_empty() {
                return v.to_string();
            }
        }
        if let Ok(raw) = std::env::var("OXIDEAN_VERSION") {
            let v = raw.trim();
            if !v.is_empty() {
                return v.to_string();
            }
        }
        env!("CARGO_PKG_VERSION").to_string()
    })
    .as_str()
}
