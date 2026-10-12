//! Package ACL helpers (PKG-04 / D-PKG-04..06).

use oxidean_core::{ClassicPatScope, PackagesPerm};
use oxidean_db::{Database, PackageRow, RepositoryRow};

use crate::repo::{
    coalesce, effective_capability, meets, owner_ref_for_repo, Capability, MemberBasePermission,
    OrgRole,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageAction {
    Pull,
    Publish,
    Delete,
}

pub fn effective_visibility(
    package: &PackageRow,
    linked_repo: Option<&RepositoryRow>,
) -> String {
    if let Some(repo) = linked_repo {
        return repo.visibility.clone();
    }
    package.visibility.clone()
}

pub fn is_public(visibility: &str) -> bool {
    visibility.eq_ignore_ascii_case("public")
}

pub fn pat_allows_packages(
    classic_scopes: Option<&[ClassicPatScope]>,
    fg_packages: Option<PackagesPerm>,
    action: PackageAction,
) -> bool {
    if let Some(scopes) = classic_scopes {
        return match action {
            // `package:delete` is standalone — `package:write` does not imply
            // it (same rule as RPC `packages.deleteVersion`).
            PackageAction::Delete => scopes
                .iter()
                .any(|s| matches!(s, ClassicPatScope::PackageDelete)),
            PackageAction::Publish => scopes
                .iter()
                .any(|s| matches!(s, ClassicPatScope::PackageWrite)),
            PackageAction::Pull => scopes.iter().any(|s| {
                matches!(s, ClassicPatScope::PackageRead | ClassicPatScope::PackageWrite)
            }),
        };
    }
    match (fg_packages, action) {
        // FG tokens carry `package:read`/`package:write` only — no delete grant.
        (_, PackageAction::Delete) => false,
        (Some(PackagesPerm::Write), _) => true,
        (Some(PackagesPerm::Read), PackageAction::Pull) => true,
        _ => false,
    }
}

pub fn capability_needed(action: PackageAction) -> Capability {
    match action {
        PackageAction::Pull => Capability::Read,
        PackageAction::Publish => Capability::Write,
        PackageAction::Delete => Capability::Admin,
    }
}

/// Resolve Capability for a caller against a package.
pub async fn owner_capability(
    db: &Database,
    user_id: Option<&str>,
    package: &PackageRow,
    linked_repo: Option<&RepositoryRow>,
) -> Result<Option<Capability>, String> {
    if let Some(repo) = linked_repo {
        let owner = match owner_ref_for_repo(db, repo).await? {
            Some(o) => o,
            None => return Err("package owner missing".into()),
        };
        return effective_capability(db, user_id, repo, &owner).await;
    }

    let Some(uid) = user_id else {
        let public = is_public(&package.visibility);
        return Ok(coalesce(
            false,
            None,
            MemberBasePermission::None,
            None,
            public,
        ));
    };

    if package.owner_type == "user" {
        let personal_owner = package.owner_id == uid;
        let public = is_public(&package.visibility);
        return Ok(coalesce(
            personal_owner,
            None,
            MemberBasePermission::None,
            None,
            public,
        ));
    }

    // org-owned unlinked
    let mut org_role = None;
    if let Some(role) = db.find_org_member_role(&package.owner_id, uid).await? {
        org_role = match role.as_str() {
            "owner" => Some(OrgRole::Owner),
            "admin" => Some(OrgRole::Admin),
            "member" => Some(OrgRole::Member),
            _ => None,
        };
    }
    let member_base = match db
        .find_org_member_base_permission(&package.owner_id)
        .await?
        .as_deref()
    {
        Some("read") => MemberBasePermission::Read,
        Some("write") => MemberBasePermission::Write,
        _ => MemberBasePermission::None,
    };
    let public = is_public(&package.visibility);
    Ok(coalesce(false, org_role, member_base, None, public))
}

pub fn authorize(
    visibility: &str,
    have: Option<Capability>,
    pat_ok: bool,
    action: PackageAction,
) -> bool {
    if matches!(action, PackageAction::Pull) && is_public(visibility) {
        return true;
    }
    let need = capability_needed(action);
    meets(have, need) && pat_ok
}

pub fn classic_repo_alone_denied(scopes: &[ClassicPatScope]) -> bool {
    !scopes.iter().any(|s| {
        matches!(
            s,
            ClassicPatScope::PackageRead | ClassicPatScope::PackageWrite
        )
    }) && scopes.contains(&ClassicPatScope::Repo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_acl_classic_repo_alone_denied() {
        assert!(!pat_allows_packages(
            Some(&[ClassicPatScope::Repo]),
            None,
            PackageAction::Pull
        ));
        assert!(classic_repo_alone_denied(&[ClassicPatScope::Repo]));
    }

    #[test]
    fn package_acl_anonymous_public_pull_ok() {
        assert!(authorize("public", None, false, PackageAction::Pull));
        assert!(!authorize("private", None, false, PackageAction::Pull));
    }

    #[test]
    fn package_acl_publish_needs_write_and_package_write() {
        assert!(!authorize(
            "private",
            Some(Capability::Write),
            false,
            PackageAction::Publish
        ));
        assert!(authorize(
            "private",
            Some(Capability::Write),
            true,
            PackageAction::Publish
        ));
    }

    #[test]
    fn package_acl_delete_needs_admin_and_package_write() {
        assert!(!authorize(
            "private",
            Some(Capability::Write),
            true,
            PackageAction::Delete
        ));
        assert!(authorize(
            "private",
            Some(Capability::Admin),
            true,
            PackageAction::Delete
        ));
    }

    #[test]
    fn package_acl_private_pull_needs_read_and_package_read() {
        assert!(!authorize(
            "private",
            Some(Capability::Read),
            false,
            PackageAction::Pull
        ));
        assert!(authorize(
            "private",
            Some(Capability::Read),
            true,
            PackageAction::Pull
        ));
        assert!(pat_allows_packages(
            Some(&[ClassicPatScope::PackageRead]),
            None,
            PackageAction::Pull
        ));
    }
}
