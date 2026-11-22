//! Organization domain DTOs (ORG-01). RPC handlers land in plan 10-13.

use serde::{Deserialize, Serialize};

/// Org membership role (D-ORG-02a). Serialized lowercase: `owner` | `admin` | `member`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OrgRole {
    Owner,
    Admin,
    Member,
}

impl OrgRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            other => Err(format!("invalid org role: {other}")),
        }
    }
}

/// Default Member permission on org private repos (D-ORG-02b).
/// Serialized lowercase: `none` | `read` | `write`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum MemberBasePermission {
    #[default]
    None,
    Read,
    Write,
}

impl MemberBasePermission {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "none" => Ok(Self::None),
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            other => Err(format!("invalid member_base_permission: {other}")),
        }
    }
}

/// Polymorphic repository owner discriminant (D-ORG-01).
/// Serialized lowercase: `user` | `org`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OwnerType {
    User,
    Org,
}

impl OwnerType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Org => "org",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "user" => Ok(Self::User),
            "org" => Ok(Self::Org),
            other => Err(format!("invalid owner_type: {other}")),
        }
    }
}

/// Per-repo collaborator permission ladder (D-ORG-02c).
/// Serialized lowercase: `read` | `write` | `admin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CollaboratorPermission {
    Read,
    Write,
    Admin,
}

impl CollaboratorPermission {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "admin",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            "admin" => Ok(Self::Admin),
            other => Err(format!("invalid collaborator permission: {other}")),
        }
    }
}

/// Public organization profile returned over RPC (ORG-01).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgPublic {
    pub id: String,
    pub slug: String,
    pub display_name: String,
    pub member_base_permission: MemberBasePermission,
    /// Members must enroll a 2FA factor to use the instance (#130).
    #[serde(default)]
    pub require_two_factor: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// `org.create` input — slug + optional display name (plan 10-13).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateOrgRequest {
    pub slug: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// `org.get` / slug-scoped org RPCs.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgSlugRequest {
    pub slug: String,
}

/// `org.setPinnedRepos` — org-owned repo ids in display order, capped at
/// [`crate::MAX_PINNED_REPOS`] (COL-04 org half). Response reuses
/// `RepoListMineResponse`; `org.listPinnedRepos` reuses `OrgSlugRequest`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetOrgPinnedReposRequest {
    pub slug: String,
    pub repo_ids: Vec<String>,
}

/// `org.updateSettings` — Admin+ (D-ORG-02b).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgUpdateSettingsRequest {
    pub slug: String,
    #[serde(default)]
    pub member_base_permission: Option<MemberBasePermission>,
    #[serde(default)]
    pub display_name: Option<String>,
    /// Set/clear the org's "members must have 2FA" policy (#130).
    #[serde(default)]
    pub require_two_factor: Option<bool>,
}

/// One org in `org.listMine` — includes caller's membership role.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgMineEntry {
    pub id: String,
    pub slug: String,
    pub display_name: String,
    pub member_base_permission: MemberBasePermission,
    pub role: OrgRole,
    pub created_at: String,
    pub updated_at: String,
}

/// `org.listMine` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgListMineResponse {
    pub orgs: Vec<OrgMineEntry>,
}

/// Public membership row — no email (ORG-01).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgMemberPublic {
    pub user_id: String,
    pub username: String,
    pub role: OrgRole,
    pub created_at: String,
}

/// `org.members.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgMembersListResponse {
    pub members: Vec<OrgMemberPublic>,
}

/// `org.members.add` — existing instance user by username (D-ORG-03).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgMembersAddRequest {
    pub slug: String,
    pub username: String,
    pub role: OrgRole,
}

/// `org.members.updateRole`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgMembersUpdateRoleRequest {
    pub slug: String,
    pub user_id: String,
    pub role: OrgRole,
}

/// `org.members.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgMembersRemoveRequest {
    pub slug: String,
    pub user_id: String,
}

/// Public pending invite — never includes token or token_hash (ORG-01 / T-10-11).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgInvitePublic {
    pub id: String,
    /// `None` = shareable link (not bound to a recipient email).
    pub email: Option<String>,
    pub role: OrgRole,
    /// `None` = never expires.
    pub expires_at: Option<String>,
    pub invited_by: String,
    pub created_at: String,
    /// `None` = unlimited seats.
    pub max_uses: Option<i64>,
    pub use_count: i64,
}

/// `org.invites.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgInvitesListResponse {
    pub invites: Vec<OrgInvitePublic>,
}

/// `org.invites.create` — bulk: one email-bound invite per address.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgInvitesCreateRequest {
    pub slug: String,
    pub emails: Vec<String>,
    pub role: OrgRole,
}

/// Per-recipient outcome of a bulk `org.invites.create`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgInvitesCreateItemResult {
    pub email: String,
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub invite: Option<OrgInvitePublic>,
    /// One-time copyable URL (present only when `ok`).
    #[serde(default)]
    pub invite_url: Option<String>,
}

/// `org.invites.create` response — one entry per submitted email.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgInvitesCreateResponse {
    pub results: Vec<OrgInvitesCreateItemResult>,
}

/// `org.invites.createLink` — shareable link, no bound email.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgInvitesCreateLinkRequest {
    pub slug: String,
    pub role: OrgRole,
    /// ISO-8601 UTC; `None`/absent = never expires.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// `None`/absent = unlimited seats.
    #[serde(default)]
    pub max_uses: Option<i64>,
}

/// `org.invites.createLink` — invite metadata + one-time copyable URL.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgInvitesCreateLinkResponse {
    pub invite: OrgInvitePublic,
    pub invite_url: String,
}

/// `org.invites.revoke`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgInvitesRevokeRequest {
    pub slug: String,
    pub invite_id: String,
}

/// `org.invites.accept` — token from email link; username/password when provisioning (A2).
/// Same shape as [`crate::InvitesAcceptRequest`] (unified `invites.accept`).
pub type OrgInvitesAcceptRequest = crate::InvitesAcceptRequest;

/// `org.invites.accept` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgInvitesAcceptResponse {
    pub org: OrgPublic,
    pub member: OrgMemberPublic,
}

// ---------------------------------------------------------------------------
// Org teams (COL-05) — org-scoped groups holding read|write|admin grants on
// org-owned repos. Membership is org-members-only; effective repo access is
// highest-wins across all grant sources (see api-core::repo::acl).
// ---------------------------------------------------------------------------

/// Team-scoped role (COL-05). `Maintainer` may manage members/repos of the
/// team without full org Admin; ACL access still comes only from repo grants.
/// Serialized lowercase: `member` | `maintainer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    Member,
    Maintainer,
}

impl TeamRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Maintainer => "maintainer",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "member" => Ok(Self::Member),
            "maintainer" => Ok(Self::Maintainer),
            other => Err(format!("invalid team role: {other}")),
        }
    }
}

/// Public org team row — id, identity, aggregate counts (COL-05).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamPublic {
    pub id: String,
    pub org_id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub member_count: i64,
    pub repo_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// `org.teams.list` response (input: [`OrgSlugRequest`]).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamsListResponse {
    pub teams: Vec<OrgTeamPublic>,
}

/// Team-scoped org RPC input — org slug + team slug.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamSlugRequest {
    pub slug: String,
    pub team_slug: String,
}

/// `org.teams.create` — `team_slug` optional; derived from `name` when absent.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamsCreateRequest {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub team_slug: Option<String>,
}

/// `org.teams.update` — Admin+ or team maintainer.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamsUpdateRequest {
    pub slug: String,
    pub team_slug: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub new_slug: Option<String>,
}

/// `org.teams.delete` — Admin+.
pub type OrgTeamsDeleteRequest = OrgTeamSlugRequest;
/// `org.teams.get` — org member.
pub type OrgTeamsGetRequest = OrgTeamSlugRequest;
/// `org.teams.members.list` / `org.teams.repos.list` — org member.
pub type OrgTeamsMembersListRequest = OrgTeamSlugRequest;
pub type OrgTeamsReposListRequest = OrgTeamSlugRequest;

/// Team member row — username only, no email (ORG-01 parity).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamMemberPublic {
    pub user_id: String,
    pub username: String,
    pub role: TeamRole,
    pub created_at: String,
}

/// `org.teams.members.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamsMembersListResponse {
    pub members: Vec<OrgTeamMemberPublic>,
}

/// `org.teams.members.add` — existing user, must already be an org member.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamsMembersAddRequest {
    pub slug: String,
    pub team_slug: String,
    pub username: String,
    #[serde(default)]
    pub role: Option<TeamRole>,
}

/// `org.teams.members.remove` / `set_role` — target by user_id.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamsMembersRemoveRequest {
    pub slug: String,
    pub team_slug: String,
    pub user_id: String,
}

/// `org.teams.members.set_role`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamsMembersSetRoleRequest {
    pub slug: String,
    pub team_slug: String,
    pub user_id: String,
    pub role: TeamRole,
}

/// Team's grant on one repo (repo identified by name within the org).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamRepoPublic {
    pub team_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub permission: CollaboratorPermission,
    pub created_at: String,
}

/// `org.teams.repos.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamsReposListResponse {
    pub repos: Vec<OrgTeamRepoPublic>,
}

/// `org.teams.repos.add` — upsert grant; `repo` is the repo name inside the org.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamsReposAddRequest {
    pub slug: String,
    pub team_slug: String,
    pub repo: String,
    pub permission: CollaboratorPermission,
}

/// `org.teams.repos.update`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamsReposUpdateRequest {
    pub slug: String,
    pub team_slug: String,
    pub repo: String,
    pub permission: CollaboratorPermission,
}

/// `org.teams.repos.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgTeamsReposRemoveRequest {
    pub slug: String,
    pub team_slug: String,
    pub repo: String,
}

/// Team holding a grant on a repo — `repo.teams.list` row.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTeamPublic {
    pub team_id: String,
    pub slug: String,
    pub name: String,
    pub permission: CollaboratorPermission,
    pub created_at: String,
}

/// `repo.teams.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTeamsListResponse {
    pub teams: Vec<RepoTeamPublic>,
}

/// Repo-scoped team RPCs — owner slug + repo name + team slug (COL-05).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTeamsAddRequest {
    pub owner: String,
    pub name: String,
    pub team_slug: String,
    pub permission: CollaboratorPermission,
}

/// `repo.teams.update`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTeamsUpdateRequest {
    pub owner: String,
    pub name: String,
    pub team_slug: String,
    pub permission: CollaboratorPermission,
}

/// `repo.teams.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoTeamsRemoveRequest {
    pub owner: String,
    pub name: String,
    pub team_slug: String,
}
