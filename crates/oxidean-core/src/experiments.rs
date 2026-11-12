//! Per-user experiment flags (WI-0001): registry + RPC types.
//!
//! The registry is the single source of truth — `experiments.list` serves it,
//! `experiments.set` validates keys against it, the web experiments settings
//! page renders it, and `docs/FEATURE-FLAGS.md` is CI-checked against it.
//! Enabled state lives server-side in `user_experiments` (one row per
//! (user, flag)); a missing row means off.

use serde::{Deserialize, Serialize};

/// Maturity bucket for a flag — surfaced on the settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    /// Early, may change or be removed without notice.
    Experimental,
    /// Feature-complete, soaking before default-on.
    Beta,
}

/// Static definition of one experiment flag.
#[derive(Debug, Clone, Copy)]
pub struct ExperimentDef {
    /// Stable wire/storage key (kebab-case). Never reuse a retired key.
    pub key: &'static str,
    /// Settings page label.
    pub title: &'static str,
    /// Settings page blurb.
    pub description: &'static str,
    pub status: ExperimentStatus,
    /// Release version the flag first shipped in (`"unreleased"` until then).
    pub since_version: &'static str,
}

/// All known experiment flags, in settings-page display order.
pub const EXPERIMENTS: &[ExperimentDef] = &[ExperimentDef {
    key: "pr-new-ui",
    title: "New pull request experience",
    description: "GitHub-style pull request layout: real per-tab URLs, file \
                  tree, viewed state, and pending reviews.",
    status: ExperimentStatus::Experimental,
    since_version: "unreleased",
}];

/// Registry lookup — `experiments.set` rejects keys not present here.
pub fn experiment(key: &str) -> Option<&'static ExperimentDef> {
    EXPERIMENTS.iter().find(|d| d.key == key)
}

/// `experiments.list` element — one registry entry plus the caller's state.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExperimentPublic {
    pub key: String,
    pub title: String,
    pub description: String,
    pub status: ExperimentStatus,
    pub since_version: String,
    /// Whether the calling user has this flag on (anonymous → always false).
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExperimentsListResponse {
    pub experiments: Vec<ExperimentPublic>,
}

/// `experiments.set` input — toggle one flag for the calling user.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExperimentSetRequest {
    pub key: String,
    pub enabled: bool,
}
