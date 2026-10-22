//! Fork network helpers for Phase 12 PR heads (D-SOC-17 / D-PR-01…03).
//!
//! Phase 12 compare/open must call [`head_valid_for_base`]: a head is valid for a base
//! when it is the same repository **or** `head.fork_network_id == base.id` (network root).

/// Whether `head_repo_id` may be used as a PR head against `base_repo_id`.
///
/// `head_fork_network_id` is the head row's `fork_network_id` (roots: equals own id).
pub fn head_valid_for_base(
    base_repo_id: &str,
    head_repo_id: &str,
    head_fork_network_id: Option<&str>,
) -> bool {
    if head_repo_id == base_repo_id {
        return true;
    }
    match head_fork_network_id {
        Some(net) => net == base_repo_id,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_repo_ok() {
        assert!(head_valid_for_base("a", "a", Some("a")));
    }

    #[test]
    fn fork_of_base_ok() {
        assert!(head_valid_for_base("base", "fork", Some("base")));
    }

    #[test]
    fn unrelated_rejected() {
        assert!(!head_valid_for_base("base", "other", Some("other-root")));
        assert!(!head_valid_for_base("base", "other", None));
    }
}
