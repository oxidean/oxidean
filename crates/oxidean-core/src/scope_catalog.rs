//! Authoritative classic-PAT scope + fine-grained permission catalog.
//!
//! Single source of truth for every surface that enumerates, validates, or
//! describes token capabilities: `pat.createClassic`/`createFineGrained`
//! validation, `pat::bearer` procedure authorization, OAuth authorize/device
//! consent (`scope_details`), the web PAT form (via `pat.catalog`), and
//! `ox auth login --web --scopes` validation. Wire names are stable and
//! strictly validated — unknown names are rejected, never dropped.

use serde::{Deserialize, Serialize};

use crate::pat_types::ClassicPatScope;

/// `const`-friendly catalog row. `implies` holds *direct* implication edges;
/// [`classic_scope_implies`] resolves the transitive closure (e.g.
/// `admin:public_key` → `write:public_key` → `read:public_key`).
#[derive(Debug, Clone, Copy)]
pub struct ScopeMeta {
    pub scope: ClassicPatScope,
    /// UI grouping on token forms and consent screens.
    pub group: &'static str,
    /// One-line description of what the scope unlocks.
    pub description: &'static str,
    /// Least-privilege guidance shown alongside the picker.
    pub hint: &'static str,
    /// Scopes this scope satisfies directly.
    pub implies: &'static [ClassicPatScope],
}

use ClassicPatScope as S;

/// The full classic scope catalog — every scope `ClassicPatScope::parse`
/// accepts, in UI order. Keep `as_str`/`parse` and this table in lockstep;
/// the unit test below enforces coverage.
pub const CLASSIC_SCOPES: &[ScopeMeta] = &[
    // ── Repository ──────────────────────────────────────────────────
    ScopeMeta {
        scope: S::Repo,
        group: "Repository",
        description: "Full control of private repositories",
        hint: "Required for git clone, fetch, and push over HTTPS.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::PublicRepo,
        group: "Repository",
        description: "Public repositories only",
        hint: "Read and collaborate on public repositories; no private access.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::RepoStatus,
        group: "Repository",
        description: "Commit statuses",
        hint: "Read and create commit status checks.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::RepoInvite,
        group: "Repository",
        description: "Repository invitations",
        hint: "Invite collaborators and manage repository invites.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::Workflow,
        group: "Repository",
        description: "Actions runs",
        hint: "Dispatch, rerun, and cancel actions workflows.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::SecurityEvents,
        group: "Repository",
        description: "Security events",
        hint: "Reserved for the security-events surface.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::DeleteRepo,
        group: "Repository",
        description: "Delete repositories",
        hint: "Not implied by repo; required to soft-delete a repository.",
        implies: &[],
    },
    // ── Packages ────────────────────────────────────────────────────
    ScopeMeta {
        scope: S::PackageRead,
        group: "Packages",
        description: "Download packages",
        hint: "Registry download only; does not include repository access by itself.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::PackageWrite,
        group: "Packages",
        description: "Publish packages",
        hint: "Implies package:read. Publish and overwrite package versions.",
        implies: &[S::PackageRead],
    },
    ScopeMeta {
        scope: S::PackageDelete,
        group: "Packages",
        description: "Delete package versions",
        hint: "Not implied by package:write.",
        implies: &[],
    },
    // ── Account ─────────────────────────────────────────────────────
    ScopeMeta {
        scope: S::Notifications,
        group: "Account",
        description: "Notifications",
        hint: "Read and manage your notifications.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::User,
        group: "Account",
        description: "Full profile access",
        hint: "Implies read:user, user:email, and user:follow.",
        implies: &[S::ReadUser, S::UserEmail, S::UserFollow],
    },
    ScopeMeta {
        scope: S::ReadUser,
        group: "Account",
        description: "Read profile data",
        hint: "Read your profile and starred/watched lists.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::UserEmail,
        group: "Account",
        description: "Read email addresses",
        hint: "List your account email addresses.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::UserFollow,
        group: "Account",
        description: "Follow users",
        hint: "Follow and unfollow other users.",
        implies: &[],
    },
    // ── Organizations ───────────────────────────────────────────────
    ScopeMeta {
        scope: S::ReadOrg,
        group: "Organizations",
        description: "Read org data",
        hint: "Read org profiles and membership.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::WriteOrg,
        group: "Organizations",
        description: "Administer orgs",
        hint: "Implies read:org. Create orgs and manage membership.",
        implies: &[S::ReadOrg],
    },
    // ── Webhooks and keys ───────────────────────────────────────────
    ScopeMeta {
        scope: S::AdminRepoHook,
        group: "Webhooks and keys",
        description: "Repository webhooks",
        hint: "Full control of repository webhooks.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::ReadPublicKey,
        group: "Webhooks and keys",
        description: "List SSH keys",
        hint: "Read your SSH public keys.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::WritePublicKey,
        group: "Webhooks and keys",
        description: "Manage SSH keys",
        hint: "Implies read:public_key. Add and revoke SSH keys.",
        implies: &[S::ReadPublicKey],
    },
    ScopeMeta {
        scope: S::AdminPublicKey,
        group: "Webhooks and keys",
        description: "Full SSH key control",
        hint: "Implies write:public_key.",
        implies: &[S::WritePublicKey],
    },
    ScopeMeta {
        scope: S::ReadGpgKey,
        group: "Webhooks and keys",
        description: "List GPG keys",
        hint: "Read your GPG public keys.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::WriteGpgKey,
        group: "Webhooks and keys",
        description: "Manage GPG keys",
        hint: "Implies read:gpg_key. Add and revoke GPG keys.",
        implies: &[S::ReadGpgKey],
    },
    ScopeMeta {
        scope: S::AdminGpgKey,
        group: "Webhooks and keys",
        description: "Full GPG key control",
        hint: "Implies write:gpg_key.",
        implies: &[S::WriteGpgKey],
    },
    // ── Projects ────────────────────────────────────────────────────
    ScopeMeta {
        scope: S::Project,
        group: "Projects",
        description: "Projects",
        hint: "Reserved for project boards.",
        implies: &[],
    },
    ScopeMeta {
        scope: S::ReadProject,
        group: "Projects",
        description: "Read projects",
        hint: "Reserved for project boards.",
        implies: &[],
    },
];

/// `const`-friendly fine-grained permission family row.
#[derive(Debug, Clone, Copy)]
pub struct FgPermMeta {
    /// `FgPerms` field name on the wire (`issues`, `pull_requests`, …).
    pub family: &'static str,
    /// `true` = acts on the token owner's account (no repo binding);
    /// `false` = repo-bound, requires a covered repository.
    pub account_level: bool,
    /// Picker label.
    pub label: &'static str,
    /// What the family gates.
    pub hint: &'static str,
}

/// The fine-grained `perms` catalog — mirrors `FgPerms`' fields.
/// `contents` and `packages` are dedicated request fields, not `perms`
/// families, so they are intentionally absent.
pub const FG_PERMS: &[FgPermMeta] = &[
    // ── repo-bound ──────────────────────────────────────────────────
    FgPermMeta {
        family: "issues",
        account_level: false,
        label: "Issues",
        hint: "Read and write issues, comments, and labels.",
    },
    FgPermMeta {
        family: "pull_requests",
        account_level: false,
        label: "Pull requests",
        hint: "Read and write pull requests, reviews, and merges.",
    },
    FgPermMeta {
        family: "actions",
        account_level: false,
        label: "Actions",
        hint: "List, dispatch, rerun, and cancel workflow runs.",
    },
    FgPermMeta {
        family: "commit_statuses",
        account_level: false,
        label: "Commit statuses",
        hint: "Read and create commit status checks.",
    },
    FgPermMeta {
        family: "secrets",
        account_level: false,
        label: "Secrets",
        hint: "Manage actions secrets on repositories.",
    },
    FgPermMeta {
        family: "variables",
        account_level: false,
        label: "Variables",
        hint: "Reserved for actions variables.",
    },
    FgPermMeta {
        family: "environments",
        account_level: false,
        label: "Environments",
        hint: "Reserved for deployment environments.",
    },
    FgPermMeta {
        family: "deployments",
        account_level: false,
        label: "Deployments",
        hint: "Reserved for deployments.",
    },
    FgPermMeta {
        family: "webhooks",
        account_level: false,
        label: "Webhooks",
        hint: "Manage repository webhooks and deliveries.",
    },
    FgPermMeta {
        family: "administration",
        account_level: false,
        label: "Administration",
        hint: "Collaborators, protection rules, mirrors, settings, deletion.",
    },
    FgPermMeta {
        family: "metadata",
        account_level: false,
        label: "Metadata",
        hint: "Repository metadata reads; an alternative to contents for read-only tokens.",
    },
    FgPermMeta {
        family: "pages",
        account_level: false,
        label: "Pages",
        hint: "Reserved for pages.",
    },
    FgPermMeta {
        family: "releases",
        account_level: false,
        label: "Releases",
        hint: "Manage releases and assets. Read also rides the contents permission.",
    },
    // ── account-level ───────────────────────────────────────────────
    FgPermMeta {
        family: "emails",
        account_level: true,
        label: "Email addresses",
        hint: "Read and manage account emails.",
    },
    FgPermMeta {
        family: "notifications",
        account_level: true,
        label: "Notifications",
        hint: "Read and manage notifications.",
    },
    FgPermMeta {
        family: "profile",
        account_level: true,
        label: "Profile",
        hint: "Update your public profile.",
    },
    FgPermMeta {
        family: "ssh_keys",
        account_level: true,
        label: "SSH keys",
        hint: "Read and manage SSH public keys.",
    },
    FgPermMeta {
        family: "gpg_keys",
        account_level: true,
        label: "GPG keys",
        hint: "Read and manage GPG public keys.",
    },
    FgPermMeta {
        family: "starring",
        account_level: true,
        label: "Starring",
        hint: "Star and unstar repositories.",
    },
    FgPermMeta {
        family: "watching",
        account_level: true,
        label: "Watching",
        hint: "Watch and unwatch repositories.",
    },
    FgPermMeta {
        family: "followers",
        account_level: true,
        label: "Followers",
        hint: "Follow users and list followers.",
    },
];

/// Catalog row for `scope`.
pub fn scope_meta(scope: ClassicPatScope) -> &'static ScopeMeta {
    CLASSIC_SCOPES
        .iter()
        .find(|m| m.scope == scope)
        .expect("CLASSIC_SCOPES covers every ClassicPatScope")
}

/// Every scope `held` implies — transitively flattened, deduplicated.
pub fn classic_scope_implies(held: ClassicPatScope) -> Vec<ClassicPatScope> {
    let mut seen = Vec::new();
    let mut stack: Vec<ClassicPatScope> = scope_meta(held).implies.to_vec();
    while let Some(s) = stack.pop() {
        if seen.contains(&s) {
            continue;
        }
        seen.push(s);
        stack.extend_from_slice(scope_meta(s).implies);
    }
    seen
}

/// `held` satisfies `required` — exact match or transitive implication.
pub fn classic_scope_satisfies(held: ClassicPatScope, required: ClassicPatScope) -> bool {
    held == required || classic_scope_implies(held).contains(&required)
}

/// True when any held scope satisfies `required`.
pub fn classic_has_scope(held: &[ClassicPatScope], required: ClassicPatScope) -> bool {
    held.iter().any(|s| classic_scope_satisfies(*s, required))
}

// ---------------------------------------------------------------------------
// Public (serializable) catalog DTOs — `pat.catalog` + OAuth `scope_details`.
// ---------------------------------------------------------------------------

/// One classic scope, presentation-ready.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PatScopePublic {
    /// Wire scope name (`repo`, `admin:repo_hook`, …).
    pub name: String,
    pub group: String,
    pub description: String,
    pub hint: String,
    /// Scope names this scope satisfies (transitively flattened).
    pub implies: Vec<String>,
}

/// One fine-grained permission family, presentation-ready.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FgPermPublic {
    pub family: String,
    /// `true` = account-level (no repository binding); `false` = repo-bound.
    pub account_level: bool,
    pub label: String,
    pub hint: String,
}

/// `pat.catalog` response — the full mintable capability surface.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PatCatalogPublic {
    pub classic_scopes: Vec<PatScopePublic>,
    pub fg_perms: Vec<FgPermPublic>,
}

/// Compact scope descriptor for OAuth consent screens (`scope_details`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OAuthScopeDetail {
    pub name: String,
    pub description: String,
}

impl From<&ScopeMeta> for PatScopePublic {
    fn from(m: &ScopeMeta) -> Self {
        Self {
            name: m.scope.as_str().to_string(),
            group: m.group.to_string(),
            description: m.description.to_string(),
            hint: m.hint.to_string(),
            implies: classic_scope_implies(m.scope)
                .iter()
                .map(|s| s.as_str().to_string())
                .collect(),
        }
    }
}

impl From<&FgPermMeta> for FgPermPublic {
    fn from(m: &FgPermMeta) -> Self {
        Self {
            family: m.family.to_string(),
            account_level: m.account_level,
            label: m.label.to_string(),
            hint: m.hint.to_string(),
        }
    }
}

/// The full catalog as the public DTO (`pat.catalog` response payload).
pub fn public_catalog() -> PatCatalogPublic {
    PatCatalogPublic {
        classic_scopes: CLASSIC_SCOPES.iter().map(PatScopePublic::from).collect(),
        fg_perms: FG_PERMS.iter().map(FgPermPublic::from).collect(),
    }
}

/// `name`/`description` pairs for a parsed scope list — consent-screen rows.
/// Unknown names are skipped (callers validate strictly at parse time).
pub fn scope_details(scopes: &[ClassicPatScope]) -> Vec<OAuthScopeDetail> {
    scopes
        .iter()
        .map(|s| OAuthScopeDetail {
            name: s.as_str().to_string(),
            description: scope_meta(*s).description.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_every_scope() {
        let mut listed: Vec<ClassicPatScope> = CLASSIC_SCOPES.iter().map(|m| m.scope).collect();
        listed.dedup();
        assert_eq!(listed.len(), CLASSIC_SCOPES.len(), "duplicate catalog rows");
        // Every name parse accepts must appear in the catalog — and nothing extra.
        for name in [
            "repo",
            "repo:status",
            "repo:invite",
            "public_repo",
            "security_events",
            "workflow",
            "package:read",
            "package:write",
            "package:delete",
            "notifications",
            "read:user",
            "user",
            "user:email",
            "user:follow",
            "read:org",
            "write:org",
            "admin:repo_hook",
            "read:public_key",
            "write:public_key",
            "admin:public_key",
            "read:gpg_key",
            "write:gpg_key",
            "admin:gpg_key",
            "delete_repo",
            "project",
            "read:project",
        ] {
            let s = ClassicPatScope::parse(name).unwrap();
            assert!(listed.contains(&s), "catalog missing {name}");
        }
        assert_eq!(listed.len(), 26);
    }

    #[test]
    fn implications_match_web_table() {
        assert!(classic_scope_satisfies(S::User, S::ReadUser));
        assert!(classic_scope_satisfies(S::User, S::UserEmail));
        assert!(classic_scope_satisfies(S::User, S::UserFollow));
        assert!(classic_scope_satisfies(S::WriteOrg, S::ReadOrg));
        assert!(classic_scope_satisfies(S::WritePublicKey, S::ReadPublicKey));
        // Transitive: admin → write → read.
        assert!(classic_scope_satisfies(S::AdminPublicKey, S::ReadPublicKey));
        assert!(classic_scope_satisfies(S::AdminPublicKey, S::WritePublicKey));
        assert!(classic_scope_satisfies(S::AdminGpgKey, S::ReadGpgKey));
        assert!(classic_scope_satisfies(S::PackageWrite, S::PackageRead));
        // Deliberate non-implications (GitHub parity).
        assert!(!classic_scope_satisfies(S::Repo, S::DeleteRepo));
        assert!(!classic_scope_satisfies(S::PackageWrite, S::PackageDelete));
        assert!(!classic_scope_satisfies(S::ReadUser, S::UserEmail));
    }

    #[test]
    fn public_catalog_serializes() {
        let v = serde_json::to_value(public_catalog()).unwrap();
        assert!(v["classic_scopes"].as_array().unwrap().len() == 26);
        assert!(v["fg_perms"].as_array().unwrap().len() == 21);
        let admin = v["classic_scopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "admin:public_key")
            .unwrap();
        assert!(admin["implies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s == "read:public_key"));
    }
}
