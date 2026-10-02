//! Central Capability ACL (ORG-02 / ORG-04 / D-ORG-05).
//!
//! Shared *decision* helpers are reused by Smart HTTP; web vs git *status mapping*
//! stays separate (web → `repo.not_found`; git private unauth → 401, D-21).
//! Callers map HTTP status — this module never embeds 401.

use oxidean_core::AppError;
use oxidean_db::{Database, RepositoryRow};
use chrono::Utc;

use crate::rpc::RpcCtx;

/// Identical error for missing repos and unauthorized private access (anti-enumeration).
pub fn not_found() -> AppError {
    AppError::new("repo.not_found", "Repository not found")
}

/// Whether visibility is private (case-insensitive).
pub fn is_private_visibility(visibility: &str) -> bool {
    visibility.eq_ignore_ascii_case("private")
}

/// Effective forge permission ladder (D-ORG-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    Read = 1,
    Write = 2,
    Admin = 3,
}

/// Org membership role (D-ORG-02a) — Collaborator is per-repo, not an org role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrgRole {
    Owner,
    Admin,
    Member,
}

/// Org-level Member base on private org repos (D-ORG-02b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberBasePermission {
    None,
    Read,
    Write,
}

/// Whether `have` satisfies `need` (inclusive ladder).
pub fn meets(have: Option<Capability>, need: Capability) -> bool {
    have.map(|h| h >= need).unwrap_or(false)
}

/// Highest-wins among applicable sources (D-ORG-05 / ASSUME A1).
/// Collaborator raises but cannot lower Owner/Admin; Member uses member_base.
pub fn coalesce(
    personal_owner: bool,
    org_role: Option<OrgRole>,
    member_base: MemberBasePermission,
    collaborator: Option<Capability>,
    public_repo: bool,
) -> Option<Capability> {
    let mut best: Option<Capability> = None;
    let bump = |b: &mut Option<Capability>, c: Capability| {
        *b = Some(b.map(|x| x.max(c)).unwrap_or(c));
    };
    if personal_owner {
        bump(&mut best, Capability::Admin);
    }
    match org_role {
        Some(OrgRole::Owner) | Some(OrgRole::Admin) => bump(&mut best, Capability::Admin),
        Some(OrgRole::Member) => match member_base {
            MemberBasePermission::None => {}
            MemberBasePermission::Read => bump(&mut best, Capability::Read),
            MemberBasePermission::Write => bump(&mut best, Capability::Write),
        },
        None => {}
    }
    if let Some(c) = collaborator {
        bump(&mut best, c);
    }
    if public_repo {
        bump(&mut best, Capability::Read);
    }
    best
}

fn parse_org_role(role: &str) -> Option<OrgRole> {
    match role.trim().to_ascii_lowercase().as_str() {
        "owner" => Some(OrgRole::Owner),
        "admin" => Some(OrgRole::Admin),
        "member" => Some(OrgRole::Member),
        _ => None,
    }
}

fn parse_member_base(perm: &str) -> MemberBasePermission {
    match perm.trim().to_ascii_lowercase().as_str() {
        "read" => MemberBasePermission::Read,
        "write" => MemberBasePermission::Write,
        _ => MemberBasePermission::None,
    }
}

fn parse_collaborator_capability(perm: &str) -> Option<Capability> {
    match perm.trim().to_ascii_lowercase().as_str() {
        "read" => Some(Capability::Read),
        "write" => Some(Capability::Write),
        "admin" => Some(Capability::Admin),
        _ => None,
    }
}

/// Resolve [`OwnerRef`] from a repository's polymorphic owner columns.
pub async fn owner_ref_for_repo(
    db: &Database,
    repo: &RepositoryRow,
) -> Result<Option<OwnerRef>, String> {
    match repo.owner_type.trim().to_ascii_lowercase().as_str() {
        "user" => {
            let Some(u) = db.find_user_by_id(&repo.owner_id).await? else {
                return Ok(None);
            };
            Ok(Some(OwnerRef::User {
                id: u.id,
                username: u.username,
            }))
        }
        "org" => {
            let Some(o) = db.find_organization_by_id(&repo.owner_id).await? else {
                return Ok(None);
            };
            Ok(Some(OwnerRef::Org {
                id: o.id,
                slug: o.slug,
            }))
        }
        _ => Ok(None),
    }
}

/// FG All coverage (ASSUME A4): personal-owned repos + org repos where the
/// subject is Org Owner or Admin. Collaborator-only subjects must use Selected.
pub async fn fg_all_covers_repo(
    db: &Database,
    user_id: &str,
    owner: &OwnerRef,
) -> Result<bool, String> {
    match owner {
        OwnerRef::User { id, .. } => Ok(user_id == id),
        OwnerRef::Org { id, .. } => match db.find_org_member_role(id, user_id).await? {
            Some(role) => {
                let r = role.trim().to_ascii_lowercase();
                Ok(r == "owner" || r == "admin")
            }
            None => Ok(false),
        },
    }
}

/// Legacy personal-owner equality helper retained for any remaining call sites.
/// Smart HTTP and mutate gates use [`effective_capability`] + [`meets`] instead.
pub fn can_read_as_owner(caller_user_id: Option<&str>, owner_id: &str) -> bool {
    caller_user_id == Some(owner_id)
}

/// Polymorphic owner resolved from a shared slug (D-ORG-01 / RESEARCH Pattern 3).
#[derive(Debug, Clone)]
pub enum OwnerRef {
    User { id: String, username: String },
    Org { id: String, slug: String },
}

impl OwnerRef {
    pub fn id(&self) -> &str {
        match self {
            Self::User { id, .. } | Self::Org { id, .. } => id,
        }
    }

    /// Public URL / disk path segment (username or org slug).
    pub fn slug(&self) -> &str {
        match self {
            Self::User { username, .. } => username,
            Self::Org { slug, .. } => slug,
        }
    }

    pub fn owner_type(&self) -> &'static str {
        match self {
            Self::User { .. } => "user",
            Self::Org { .. } => "org",
        }
    }
}

/// Resolve shared slug → user (first) or organization. Missing → `Ok(None)`.
pub async fn resolve_owner_slug(
    db: &Database,
    slug: &str,
) -> Result<Option<OwnerRef>, String> {
    let slug = slug.trim();
    if slug.is_empty() {
        return Ok(None);
    }
    if let Some(u) = db.find_user_by_username(slug).await? {
        return Ok(Some(OwnerRef::User {
            id: u.id,
            username: u.username,
        }));
    }
    if let Some(o) = db.find_organization_by_slug(slug).await? {
        return Ok(Some(OwnerRef::Org {
            id: o.id,
            slug: o.slug,
        }));
    }
    Ok(None)
}

/// True when `slug` is already a username or organization slug (D-ORG-01 shared namespace).
pub async fn login_slug_taken(db: &Database, slug: &str) -> Result<bool, String> {
    let slug = slug.trim();
    if slug.is_empty() {
        return Ok(false);
    }
    if db.find_user_by_username(slug).await?.is_some() {
        return Ok(true);
    }
    if db.find_organization_by_slug(slug).await?.is_some() {
        return Ok(true);
    }
    Ok(false)
}

/// Load ACL sources and coalesce (D-ORG-05). Does not map HTTP status.
pub async fn effective_capability(
    db: &Database,
    caller_user_id: Option<&str>,
    repo: &RepositoryRow,
    owner: &OwnerRef,
) -> Result<Option<Capability>, String> {
    let personal_owner = matches!(owner, OwnerRef::User { .. })
        && caller_user_id.is_some_and(|id| id == owner.id());

    let mut org_role: Option<OrgRole> = None;
    let mut member_base = MemberBasePermission::None;
    if matches!(owner, OwnerRef::Org { .. }) {
        if let Some(caller) = caller_user_id {
            if let Some(role) = db.find_org_member_role(owner.id(), caller).await? {
                org_role = parse_org_role(&role);
            }
        }
        if let Some(base) = db.find_org_member_base_permission(owner.id()).await? {
            member_base = parse_member_base(&base);
        }
    }

    let collaborator = if let Some(caller) = caller_user_id {
        match db.find_repo_collaborator(&repo.id, caller).await? {
            Some(row) => parse_collaborator_capability(&row.permission),
            None => None,
        }
    } else {
        None
    };

    let public_repo = !is_private_visibility(&repo.visibility);
    Ok(coalesce(
        personal_owner,
        org_role,
        member_base,
        collaborator,
        public_repo,
    ))
}

/// Repo row the caller is allowed to read, plus resolved owner username/slug.
pub struct AccessibleRepo {
    pub row: RepositoryRow,
    pub owner_username: String,
    /// Effective capability after coalesce (None only if somehow granted without a tier).
    pub capability: Option<Capability>,
}

fn redirect_expired(expires_at: &str) -> bool {
    let trimmed = expires_at.trim();
    let when = chrono::DateTime::parse_from_rfc3339(trimmed)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%SZ")
                .ok()
                .map(|n| chrono::DateTime::from_naive_utc_and_offset(n, Utc))
        })
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|n| chrono::DateTime::from_naive_utc_and_offset(n, Utc))
        });
    match when {
        Some(exp) => Utc::now() >= exp,
        None => true,
    }
}

/// Live repo at `owner`/`name`, or unexpired redirect → current row + owner (D-REL-08).
/// Live path always wins over redirects.
pub async fn lookup_repo_row_or_redirect(
    db: &Database,
    owner_slug: &str,
    name: &str,
) -> Result<Option<(RepositoryRow, OwnerRef)>, String> {
    let owner_slug = owner_slug.trim();
    let name = name.trim();
    if owner_slug.is_empty() || name.is_empty() {
        return Ok(None);
    }

    if let Some(owner_ref) = resolve_owner_slug(db, owner_slug).await? {
        if let Some(row) = db
            .find_repository_by_owner_name(owner_ref.id(), name)
            .await?
        {
            return Ok(Some((row, owner_ref)));
        }
    }

    let Some(redir) = db.find_repository_redirect(owner_slug, name).await? else {
        return Ok(None);
    };
    if redirect_expired(&redir.expires_at) {
        return Ok(None);
    }
    let Some(row) = db.find_repository_by_id(&redir.repo_id).await? else {
        return Ok(None);
    };
    let Some(owner_ref) = owner_ref_for_repo(db, &row).await? else {
        return Ok(None);
    };
    Ok(Some((row, owner_ref)))
}

/// Resolve `owner`/`name` for read. Missing OR unauthorized private → identical [`not_found`].
/// Honors unexpired repository redirects (D-REL-08).
pub async fn resolve_repo_for_read(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let owner = owner.trim();
    let name = name.trim();
    if owner.is_empty() || name.is_empty() {
        return Err(not_found());
    }

    let pair = match lookup_repo_row_or_redirect(&ctx.db, owner, name).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "lookup_repo_row_or_redirect failed");
            return Err(AppError::new("repo.internal", "repository operation failed"));
        }
    };
    let Some((row, owner_ref)) = pair else {
        return Err(not_found());
    };

    let caller_id = ctx.session.as_ref().map(|s| s.user_id.as_str());
    let capability = match effective_capability(&ctx.db, caller_id, &row, &owner_ref).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "effective_capability failed");
            return Err(AppError::new("repo.internal", "repository operation failed"));
        }
    };

    if !meets(capability, Capability::Read) {
        return Err(not_found());
    }

    Ok(AccessibleRepo {
        row,
        owner_username: owner_ref.slug().to_string(),
        capability,
    })
}

/// GIT-20: archived repositories reject content writes with `repo.archived`.
/// Reads, clone/fetch, and Admin settings (including unarchive) stay open.
pub fn archived_error() -> AppError {
    AppError::new("repo.archived", "Repository is archived (read-only)")
}

/// Reject a content write when the resolved repository is archived (GIT-20).
pub fn ensure_not_archived(repo: &AccessibleRepo) -> Result<(), AppError> {
    if repo.row.archived {
        return Err(archived_error());
    }
    Ok(())
}

#[cfg(test)]
mod coalesce_tests {
    //! ORG-02 / D-ORG-05: highest-wins coalesce matrix.

    use super::{coalesce, Capability, MemberBasePermission, OrgRole};

    #[test]
    fn coalesce_personal_owner_is_admin() {
        assert_eq!(
            coalesce(true, None, MemberBasePermission::None, None, false),
            Some(Capability::Admin)
        );
    }

    #[test]
    fn coalesce_org_owner_is_admin() {
        assert_eq!(
            coalesce(
                false,
                Some(OrgRole::Owner),
                MemberBasePermission::None,
                None,
                false
            ),
            Some(Capability::Admin)
        );
    }

    #[test]
    fn coalesce_org_admin_is_admin() {
        assert_eq!(
            coalesce(
                false,
                Some(OrgRole::Admin),
                MemberBasePermission::Read,
                None,
                false
            ),
            Some(Capability::Admin)
        );
    }

    #[test]
    fn coalesce_member_base_none_yields_none() {
        assert_eq!(
            coalesce(
                false,
                Some(OrgRole::Member),
                MemberBasePermission::None,
                None,
                false
            ),
            None
        );
    }

    #[test]
    fn coalesce_member_base_read_is_read() {
        assert_eq!(
            coalesce(
                false,
                Some(OrgRole::Member),
                MemberBasePermission::Read,
                None,
                false
            ),
            Some(Capability::Read)
        );
    }

    #[test]
    fn coalesce_member_base_write_is_write() {
        assert_eq!(
            coalesce(
                false,
                Some(OrgRole::Member),
                MemberBasePermission::Write,
                None,
                false
            ),
            Some(Capability::Write)
        );
    }

    #[test]
    fn coalesce_collaborator_raises_member_base_none() {
        assert_eq!(
            coalesce(
                false,
                Some(OrgRole::Member),
                MemberBasePermission::None,
                Some(Capability::Write),
                false
            ),
            Some(Capability::Write)
        );
    }

    #[test]
    fn coalesce_public_repo_grants_read() {
        assert_eq!(
            coalesce(false, None, MemberBasePermission::None, None, true),
            Some(Capability::Read)
        );
    }

    #[test]
    fn coalesce_anonymous_private_is_none() {
        assert_eq!(
            coalesce(false, None, MemberBasePermission::None, None, false),
            None
        );
    }
}
