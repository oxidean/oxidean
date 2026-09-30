//! Organization domain DTOs (ORG-01). RPC handlers land in plan 10-13.

use serde::{Deserialize, Serialize};

/// Org membership role (D-ORG-02a). Serialized lowercase: `owner` | `admin` | `member`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgPublic {
    pub id: String,
    pub slug: String,
    pub display_name: String,
    pub member_base_permission: MemberBasePermission,
    pub created_at: String,
    pub updated_at: String,
}

/// `org.create` input — slug + optional display name (plan 10-13).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrgRequest {
    pub slug: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// `org.get` / slug-scoped org RPCs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgSlugRequest {
    pub slug: String,
}

/// `org.updateSettings` — Admin+ (D-ORG-02b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgUpdateSettingsRequest {
    pub slug: String,
    #[serde(default)]
    pub member_base_permission: Option<MemberBasePermission>,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// One org in `org.listMine` — includes caller's membership role.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgListMineResponse {
    pub orgs: Vec<OrgMineEntry>,
}

/// Public membership row — no email (ORG-01).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgMemberPublic {
    pub user_id: String,
    pub username: String,
    pub role: OrgRole,
    pub created_at: String,
}

/// `org.members.list` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgMembersListResponse {
    pub members: Vec<OrgMemberPublic>,
}

/// `org.members.add` — existing instance user by username (D-ORG-03).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgMembersAddRequest {
    pub slug: String,
    pub username: String,
    pub role: OrgRole,
}

/// `org.members.updateRole`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgMembersUpdateRoleRequest {
    pub slug: String,
    pub user_id: String,
    pub role: OrgRole,
}

/// `org.members.remove`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgMembersRemoveRequest {
    pub slug: String,
    pub user_id: String,
}

/// Public pending invite — never includes token or token_hash (ORG-01 / T-10-11).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgInvitePublic {
    pub id: String,
    pub email: String,
    pub role: OrgRole,
    pub expires_at: String,
    pub invited_by: String,
    pub created_at: String,
}

/// `org.invites.list` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgInvitesListResponse {
    pub invites: Vec<OrgInvitePublic>,
}

/// `org.invites.create`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgInvitesCreateRequest {
    pub slug: String,
    pub email: String,
    pub role: OrgRole,
}

/// `org.invites.revoke`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgInvitesRevokeRequest {
    pub slug: String,
    pub invite_id: String,
}

/// `org.invites.accept` — token from email link; username/password when provisioning (A2).
/// Same shape as [`crate::InvitesAcceptRequest`] (unified `invites.accept`).
pub type OrgInvitesAcceptRequest = crate::InvitesAcceptRequest;

/// `org.invites.accept` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgInvitesAcceptResponse {
    pub org: OrgPublic,
    pub member: OrgMemberPublic,
}
