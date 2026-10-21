//! Action supply-chain policy (Phase 8) — instance-configured gates applied
//! to remote `uses:` references at enqueue. Violations flow through the same
//! loud-failure path as unsupported workflow features (a failed
//! `__validation` job), never a silent skip.
//!
//! Env knobs (all optional, backward-compatible defaults):
//! - `OXIDEAN_ACTIONS_ALLOWED_SOURCES` — comma-separated owner allowlist for
//!   remote `owner/repo@ref` actions. Entries are `owner` (any repo under the
//!   owner) or `owner/repo`. Empty/unset allows every remote source
//!   (GitHub parity default). `docker://` and `./local` uses are unaffected.
//! - `OXIDEAN_ACTIONS_REQUIRE_SHA_PIN` — `1`/`true`: every remote `uses:` ref
//!   must pin a full 40-hex commit SHA.
//! - `OXIDEAN_ACTIONS_MIRROR` — read by the runner, not here: replaces the
//!   public-github clone fallback with the instance mirror base.

use crate::actions::parse::WorkflowDocument;

pub struct ActionSourcePolicy {
    /// `owner` or `owner/repo` allowlist entries; empty = allow all.
    pub allowlist: Vec<String>,
    pub require_sha_pin: bool,
}

pub fn policy_from_env() -> ActionSourcePolicy {
    let allowlist = std::env::var("OXIDEAN_ACTIONS_ALLOWED_SOURCES")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let require_sha_pin = matches!(
        std::env::var("OXIDEAN_ACTIONS_REQUIRE_SHA_PIN")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    );
    ActionSourcePolicy {
        allowlist,
        require_sha_pin,
    }
}

/// A remote `uses:` decomposition — `owner/repo[/path]@ref`.
struct RemoteUses<'a> {
    owner: &'a str,
    repo: &'a str,
    git_ref: &'a str,
}

/// Decompose a remote `owner/repo@ref` spec; `None` for `./local`,
/// `docker://`, or malformed refs (the parser reports those separately).
fn remote_uses(uses: &str) -> Option<RemoteUses<'_>> {
    if uses.starts_with("docker://") || uses.starts_with("./") {
        return None;
    }
    let (path_part, git_ref) = uses.rsplit_once('@')?;
    if git_ref.is_empty() {
        return None;
    }
    let mut segs = path_part.split('/');
    let owner = segs.next()?;
    let repo = segs.next()?;
    Some(RemoteUses {
        owner,
        repo,
        git_ref,
    })
}

fn is_full_sha(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// One `uses:` violation message, if any.
pub fn check_uses(uses: &str, policy: &ActionSourcePolicy) -> Option<String> {
    let t = remote_uses(uses)?;
    if !policy.allowlist.is_empty() {
        let owner = t.owner.to_ascii_lowercase();
        let repo = format!("{}/{}", owner, t.repo.to_ascii_lowercase());
        let allowed = policy
            .allowlist
            .iter()
            .any(|e| e == &owner || e == &repo || e == "*");
        if !allowed {
            return Some(format!(
                "action '{uses}' is not in the instance action-source allowlist"
            ));
        }
    }
    if policy.require_sha_pin && !is_full_sha(t.git_ref) {
        return Some(format!(
            "action '{uses}' must pin a full commit SHA (OXIDEAN_ACTIONS_REQUIRE_SHA_PIN)"
        ));
    }
    None
}

/// All policy violations across a workflow's jobs — one line per offending
/// `uses:`, in document order.
pub fn check_doc(doc: &WorkflowDocument, policy: &ActionSourcePolicy) -> Vec<String> {
    let mut out = Vec::new();
    for job in &doc.jobs {
        for step in &job.steps {
            if let Some(uses) = step.uses.as_deref() {
                if let Some(v) = check_uses(uses, policy) {
                    out.push(format!("jobs.{}: {v}", job.id));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(list: &str, pin: bool) -> ActionSourcePolicy {
        ActionSourcePolicy {
            allowlist: list
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            require_sha_pin: pin,
        }
    }

    #[test]
    fn allowlist_enforces_owner_and_repo() {
        let p = policy("actions, oxidean/toolkit", false);
        assert!(check_uses("actions/checkout@v4", &p).is_none());
        assert!(check_uses("actions/setup-node@v4", &p).is_none());
        assert!(check_uses("oxidean/toolkit/build@main", &p).is_none());
        assert!(check_uses("oxidean/other@main", &p).is_some());
        assert!(check_uses("evil/malware@v1", &p).is_some());
        // Non-remote forms pass through untouched.
        assert!(check_uses("./tools/my-action", &p).is_none());
        assert!(check_uses("docker://alpine:3.19", &p).is_none());
        // Empty allowlist = allow all.
        assert!(check_uses("anything/at@all", &policy("", false)).is_none());
        // '*' permits everything.
        assert!(check_uses("anything/at@all", &policy("*", false)).is_none());
    }

    #[test]
    fn sha_pin_enforcement() {
        let p = policy("", true);
        assert!(check_uses("actions/checkout@v4", &p).is_some());
        assert!(check_uses(
            "actions/checkout@8f4b7f84864484a7bf31766abe9204da3cbe65b3",
            &p
        )
        .is_none());
        // Case-insensitive owner match + pin combo.
        let p2 = policy("Actions", true);
        assert!(check_uses("actions/checkout@v4", &p2).is_some()); // pin violation wins
        assert!(check_uses("evil/x@8f4b7f84864484a7bf31766abe9204da3cbe65b3", &p2).is_some());
    }
}
