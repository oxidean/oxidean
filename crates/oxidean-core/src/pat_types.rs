//! Personal access token DTOs and scope enums (D-03, D-06, D-08, D-15).
//!
//! List/response items never carry plaintext secrets — only `CreatePatResponse.token`
//! holds the one-time reveal value.

use serde::{Deserialize, Serialize};

/// Classic PAT string prefix (D-08 locked `oxidean_prefixes`).
pub const CLASSIC_PAT_PREFIX: &str = "oxidean_pat_";

/// Fine-grained PAT string prefix (D-08 locked `oxidean_prefixes`).
pub const FINE_GRAINED_PAT_PREFIX: &str = "oxidean_fg_";

/// Token kind. Serialized as `classic` | `fine_grained`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PatKind {
    Classic,
    FineGrained,
}

impl PatKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::FineGrained => "fine_grained",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "classic" => Ok(Self::Classic),
            "fine_grained" => Ok(Self::FineGrained),
            other => Err(format!("invalid pat kind: {other}")),
        }
    }

    pub const fn token_prefix(self) -> &'static str {
        match self {
            Self::Classic => CLASSIC_PAT_PREFIX,
            Self::FineGrained => FINE_GRAINED_PAT_PREFIX,
        }
    }
}

/// Classic scope catalog (GitHub-classic parity where a procedure family
/// exists). `repo` is the umbrella repo-domain scope; the narrower scopes
/// gate single families (`pat::bearer::classify` maps procedures → gates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ClassicPatScope {
    #[serde(rename = "repo")]
    Repo,
    /// Commit statuses only (`repo.commitStatus.*`).
    #[serde(rename = "repo:status")]
    RepoStatus,
    /// Repository invitations (`repo.invites.*`).
    #[serde(rename = "repo:invite")]
    RepoInvite,
    /// Public-repo read/write only (no private repos, no admin surface).
    #[serde(rename = "public_repo")]
    PublicRepo,
    /// Security-events surface — mintable; no procedures map yet.
    #[serde(rename = "security_events")]
    SecurityEvents,
    /// Actions runs/workflows (dispatch, rerun, cancel).
    #[serde(rename = "workflow")]
    Workflow,
    #[serde(rename = "package:read")]
    PackageRead,
    #[serde(rename = "package:write")]
    PackageWrite,
    /// Package version deletion (held back from `package:write`, GH parity).
    #[serde(rename = "package:delete")]
    PackageDelete,
    #[serde(rename = "notifications")]
    Notifications,
    /// Profile + public user lookups.
    #[serde(rename = "read:user")]
    ReadUser,
    /// Full user profile: profile writes + email management + follow.
    #[serde(rename = "user")]
    User,
    /// `email.list` (read own addresses).
    #[serde(rename = "user:email")]
    UserEmail,
    /// `user.follow` / `user.unfollow`.
    #[serde(rename = "user:follow")]
    UserFollow,
    /// Org profile/membership reads.
    #[serde(rename = "read:org")]
    ReadOrg,
    /// Org creation/settings/membership writes (implies `read:org`).
    #[serde(rename = "write:org")]
    WriteOrg,
    /// Repository webhooks (`webhook.*` read + write).
    #[serde(rename = "admin:repo_hook")]
    AdminRepoHook,
    #[serde(rename = "read:public_key")]
    ReadPublicKey,
    #[serde(rename = "write:public_key")]
    WritePublicKey,
    #[serde(rename = "admin:public_key")]
    AdminPublicKey,
    #[serde(rename = "read:gpg_key")]
    ReadGpgKey,
    #[serde(rename = "write:gpg_key")]
    WriteGpgKey,
    #[serde(rename = "admin:gpg_key")]
    AdminGpgKey,
    /// `repo.softDelete` — deliberately NOT implied by `repo` (GH parity).
    #[serde(rename = "delete_repo")]
    DeleteRepo,
    /// Project boards — mintable forward-compat; no procedures map yet.
    #[serde(rename = "project")]
    Project,
    #[serde(rename = "read:project")]
    ReadProject,
}

impl ClassicPatScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Repo => "repo",
            Self::RepoStatus => "repo:status",
            Self::RepoInvite => "repo:invite",
            Self::PublicRepo => "public_repo",
            Self::SecurityEvents => "security_events",
            Self::Workflow => "workflow",
            Self::PackageRead => "package:read",
            Self::PackageWrite => "package:write",
            Self::PackageDelete => "package:delete",
            Self::Notifications => "notifications",
            Self::ReadUser => "read:user",
            Self::User => "user",
            Self::UserEmail => "user:email",
            Self::UserFollow => "user:follow",
            Self::ReadOrg => "read:org",
            Self::WriteOrg => "write:org",
            Self::AdminRepoHook => "admin:repo_hook",
            Self::ReadPublicKey => "read:public_key",
            Self::WritePublicKey => "write:public_key",
            Self::AdminPublicKey => "admin:public_key",
            Self::ReadGpgKey => "read:gpg_key",
            Self::WriteGpgKey => "write:gpg_key",
            Self::AdminGpgKey => "admin:gpg_key",
            Self::DeleteRepo => "delete_repo",
            Self::Project => "project",
            Self::ReadProject => "read:project",
        }
    }

    /// Full parse table — accepts every `as_str` wire name.
    pub fn parse(s: &str) -> Result<Self, String> {
        Ok(match s.trim() {
            "repo" => Self::Repo,
            "repo:status" => Self::RepoStatus,
            "repo:invite" => Self::RepoInvite,
            "public_repo" => Self::PublicRepo,
            "security_events" => Self::SecurityEvents,
            "workflow" => Self::Workflow,
            "package:read" => Self::PackageRead,
            "package:write" => Self::PackageWrite,
            "package:delete" => Self::PackageDelete,
            "notifications" => Self::Notifications,
            "read:user" => Self::ReadUser,
            "user" => Self::User,
            "user:email" => Self::UserEmail,
            "user:follow" => Self::UserFollow,
            "read:org" => Self::ReadOrg,
            "write:org" => Self::WriteOrg,
            "admin:repo_hook" => Self::AdminRepoHook,
            "read:public_key" => Self::ReadPublicKey,
            "write:public_key" => Self::WritePublicKey,
            "admin:public_key" => Self::AdminPublicKey,
            "read:gpg_key" => Self::ReadGpgKey,
            "write:gpg_key" => Self::WriteGpgKey,
            "admin:gpg_key" => Self::AdminGpgKey,
            "delete_repo" => Self::DeleteRepo,
            "project" => Self::Project,
            "read:project" => Self::ReadProject,
            other => return Err(format!("invalid classic scope: {other}")),
        })
    }
}

/// Fine-grained Packages permission (D-PKG-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PackagesPerm {
    Read,
    Write,
}

impl PackagesPerm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            other => Err(format!("invalid packages_perm: {other}")),
        }
    }
}

/// Fine-grained repository access (D-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FgRepoAccess {
    Selected,
    All,
}

impl FgRepoAccess {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::All => "all",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "selected" => Ok(Self::Selected),
            "all" => Ok(Self::All),
            other => Err(format!("invalid repo_access: {other}")),
        }
    }
}

/// Fine-grained Contents permission — `read` = upload-pack; `write` = receive-pack + read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ContentsPerm {
    Read,
    Write,
}

impl ContentsPerm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            other => Err(format!("invalid contents_perm: {other}")),
        }
    }
}

/// Fine-grained permission level shared by every family — `read` | `write`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum FgPermLevel {
    Read,
    Write,
}

impl FgPermLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            other => Err(format!("invalid permission level: {other}")),
        }
    }

    /// `write` implies `read` (GH parity).
    pub const fn meets(self, required: FgPermLevel) -> bool {
        self as u8 >= required as u8
    }
}

/// Fine-grained permission families (GitHub FG parity). Absent field = no
/// grant. `contents` stays on `CreateFineGrainedPatRequest.contents`
/// (required, existing column); `packages` keeps its dedicated field —
/// `perms` holds every *other* family, persisted as JSON on the token row.
///
/// Repo-bound families require the procedure to name a covered repository;
/// account-level families (`emails`, `notifications`, `profile`, `ssh_keys`,
/// `gpg_keys`, `starring`, `watching`, `followers`) act on the token owner's
/// account directly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FgPerms {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issues: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_requests: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_statuses: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secrets: Option<FgPermLevel>,
    /// Actions variables — mintable forward-compat; no procedures map yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<FgPermLevel>,
    /// Environments — mintable forward-compat; no procedures map yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environments: Option<FgPermLevel>,
    /// Deployments — mintable forward-compat; no procedures map yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployments: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhooks: Option<FgPermLevel>,
    /// Repository administration (collaborators, protection rules, mirrors,
    /// LFS/quota/settings writes, deploy keys, repo delete).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub administration: Option<FgPermLevel>,
    /// Repo metadata reads — satisfies the `RepoRead` procedure class without
    /// `contents` (GH grants Metadata:read implicitly).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<FgPermLevel>,
    /// Pages — mintable forward-compat; no procedures map yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub releases: Option<FgPermLevel>,
    // --- account-level ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emails: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notifications: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_keys: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpg_keys: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starring: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watching: Option<FgPermLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub followers: Option<FgPermLevel>,
}

impl FgPerms {
    /// True when no family is granted (useful for "pick at least one" checks).
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// `pat.createClassic` input (RPC wired in 08-04+).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateClassicPatRequest {
    /// Required note/name (D-16).
    pub name: String,
    /// Must include `repo` in Phase 8.
    pub scopes: Vec<ClassicPatScope>,
    /// Optional expiry; omit / null = no expiration (D-07).
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// `pat.createFineGrained` input (RPC wired in 08-04+).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateFineGrainedPatRequest {
    /// Required note/name (D-16).
    pub name: String,
    pub repo_access: FgRepoAccess,
    /// Required when `repo_access` is `selected`.
    #[serde(default)]
    pub repository_ids: Vec<String>,
    pub contents: ContentsPerm,
    /// Optional packages permission for FG tokens (D-PKG-04).
    #[serde(default)]
    pub packages: Option<PackagesPerm>,
    /// Additional permission families (issues/pull_requests/actions/…).
    #[serde(default)]
    pub perms: FgPerms,
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// List / metadata item — **no** plaintext token field (D-15, T-08-01).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PatListItem {
    pub id: String,
    pub kind: PatKind,
    pub name: String,
    pub token_prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<ClassicPatScope>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<ContentsPerm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packages: Option<PackagesPerm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_access: Option<FgRepoAccess>,
    /// Extra FG permission families (fine-grained only).
    #[serde(default, skip_serializing_if = "FgPerms::is_empty")]
    pub perms: FgPerms,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repository_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_ip: Option<String>,
    pub created_at: String,
}

/// One-time create response — plaintext `token` only here (D-15).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatePatResponse {
    pub token: String,
    pub item: PatListItem,
}

/// `pat.self` — introspection of the bearer PAT making the call. Lets `ox`
/// show token metadata (expiry, scopes, origin) without a session cookie.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PatSelfInfo {
    /// `personal_access_tokens.id` of the presented token.
    pub id: String,
    pub name: String,
    /// Display fingerprint (`oxidean_pat_xxxxxxxx` prefix — never the secret).
    pub token_prefix: String,
    /// `classic` | `fine_grained`.
    pub kind: String,
    /// Classic scope names carried by the token (empty for fine-grained).
    pub scopes: Vec<String>,
    /// RFC3339 expiry, `null` for non-expiring tokens.
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

/// `pat.policy` — the instance's effective PAT expiration policy. Public data
/// (defaults + bounds), safe to expose to any authenticated caller so token
/// forms and `ox` can render/validate expiry choices.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PatPolicyPublic {
    /// Lifetime applied when a create request omits `expires_at` (days).
    pub default_ttl_days: u32,
    /// Upper bound for an explicit `expires_at` (days from creation).
    pub max_ttl_days: u32,
    /// Whether `"never"` is an accepted `expires_at` value on this instance.
    pub allow_never_expire: bool,
}

/// `admin.pat.getSettings` — effective values plus per-field override flags
/// (same env-default/override split as `admin.lfs.*` / `admin.mcp.*`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminPatSettingsPublic {
    pub default_ttl_days: u32,
    pub default_ttl_days_overridden: bool,
    pub max_ttl_days: u32,
    pub max_ttl_days_overridden: bool,
    pub allow_never_expire: bool,
    pub allow_never_expire_overridden: bool,
}

/// `admin.pat.updateSettings` — omitted fields keep the stored override;
/// `clear_overrides` reverts all three to the env defaults.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminPatUpdateSettingsRequest {
    #[serde(default)]
    pub default_ttl_days: Option<u32>,
    #[serde(default)]
    pub max_ttl_days: Option<u32>,
    #[serde(default)]
    pub allow_never_expire: Option<bool>,
    /// When true, clear all stored overrides so env defaults apply again.
    #[serde(default)]
    pub clear_overrides: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_are_oxidean_brand_not_ona() {
        assert_eq!(CLASSIC_PAT_PREFIX, "oxidean_pat_");
        assert_eq!(FINE_GRAINED_PAT_PREFIX, "oxidean_fg_");
        assert_eq!(PatKind::Classic.token_prefix(), "oxidean_pat_");
        assert_eq!(PatKind::FineGrained.token_prefix(), "oxidean_fg_");
        assert!(!CLASSIC_PAT_PREFIX.starts_with("ona_"));
        assert!(!FINE_GRAINED_PAT_PREFIX.starts_with("ona_"));
        assert!(!CLASSIC_PAT_PREFIX.contains("github"));
        assert!(!CLASSIC_PAT_PREFIX.starts_with("gh"));
    }

    #[test]
    fn pat_kind_serde_snake_case() {
        let json = serde_json::to_string(&PatKind::FineGrained).unwrap();
        assert_eq!(json, "\"fine_grained\"");
        let kind: PatKind = serde_json::from_str("\"classic\"").unwrap();
        assert_eq!(kind, PatKind::Classic);
    }

    #[test]
    fn pat_list_item_json_has_no_token_field() {
        let item = PatListItem {
            id: "p1".into(),
            kind: PatKind::Classic,
            name: "laptop".into(),
            token_prefix: CLASSIC_PAT_PREFIX.into(),
            scopes: Some(vec![ClassicPatScope::Repo]),
            contents: None,
            packages: None,
            repo_access: None,
            perms: FgPerms::default(),
            repository_ids: vec![],
            expires_at: None,
            last_used_at: None,
            last_used_ip: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let v = serde_json::to_value(&item).unwrap();
        assert!(v.get("token").is_none());
        assert_eq!(v["token_prefix"], CLASSIC_PAT_PREFIX);
    }

    #[test]
    fn create_pat_response_includes_token_once() {
        let resp = CreatePatResponse {
            token: format!("{CLASSIC_PAT_PREFIX}deadbeef"),
            item: PatListItem {
                id: "p1".into(),
                kind: PatKind::Classic,
                name: "ci".into(),
                token_prefix: CLASSIC_PAT_PREFIX.into(),
                scopes: Some(vec![ClassicPatScope::Repo]),
                contents: None,
                packages: None,
                perms: FgPerms::default(),
                repo_access: None,
                repository_ids: vec![],
                expires_at: None,
                last_used_at: None,
                last_used_ip: None,
                created_at: "2026-01-01T00:00:00Z".into(),
            },
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert!(v["token"].as_str().unwrap().starts_with(CLASSIC_PAT_PREFIX));
        assert!(v["item"].get("token").is_none());
    }
}
