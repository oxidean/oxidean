//! Auth-related DTOs and username validation shared across API and clients.

use serde::{Deserialize, Serialize};

/// Auth provider mode (D-05, D-06). Serialized as lowercase: `local` | `workos` | `oidc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProviderMode {
    #[default]
    Local,
    Workos,
    Oidc,
}

/// Outbound email adapter kind. Serialized as lowercase: `log` | `smtp` | `resend`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmailProviderKind {
    Log,
    Smtp,
    Resend,
}

/// Instance-level user role. Serialized as kebab-case: `user` | `admin` | `sys-admin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    #[default]
    User,
    Admin,
    SysAdmin,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Admin => "admin",
            Self::SysAdmin => "sys-admin",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "user" => Ok(Self::User),
            "admin" => Ok(Self::Admin),
            "sys-admin" => Ok(Self::SysAdmin),
            other => Err(format!("invalid role: {other}")),
        }
    }

    /// Instance Auth settings / `admin.auth.*`.
    pub const fn is_sys_admin(self) -> bool {
        matches!(self, Self::SysAdmin)
    }

    /// Elevated staff (admin or sys-admin).
    pub const fn is_staff(self) -> bool {
        matches!(self, Self::Admin | Self::SysAdmin)
    }
}

/// Public user profile returned over RPC (no password hash).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserPublic {
    pub id: String,
    pub email: String,
    pub username: String,
    pub display_name: String,
    pub bio: String,
    /// Public URL path (e.g. `/uploads/avatars/{id}.webp`), not a filesystem path.
    pub avatar_url: Option<String>,
    pub role: Role,
    /// True when username needs completion (e.g. after SSO with placeholder handle).
    pub profile_incomplete: bool,
    /// True when the account email has been verified (D-13); wired in `user_to_public` in 05-02.
    pub email_verified: bool,
    /// True when ENV-seeded admin must confirm credentials before normal use (AUTH-06).
    pub must_change_credentials: bool,
    /// Default branch name for new repositories (D-09); defaults to `main`.
    #[serde(default = "default_branch_main")]
    pub default_branch: String,
}

fn default_branch_main() -> String {
    "main".into()
}

/// Empty-instance bootstrap status (AUTH-07).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BootstrapStatus {
    /// True when `users` is empty and `OXIDEAN_ADMIN_*` ENV seed is not configured.
    pub needs_setup: bool,
}

/// One-time setup wizard input — creates the first `sys-admin` (AUTH-07).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BootstrapSetupRequest {
    pub email: String,
    pub username: String,
    pub password: String,
    /// Post-bootstrap local signup policy (D-05/D-07); defaults false when omitted.
    #[serde(default)]
    pub allow_signup: bool,
    /// Auth stack for this instance (defaults to local).
    #[serde(default)]
    pub provider_mode: ProviderMode,
    #[serde(default)]
    pub oidc_issuer: Option<String>,
    #[serde(default)]
    pub oidc_client_id: Option<String>,
    #[serde(default)]
    pub workos_client_id: Option<String>,
}

/// Factory reset disk/DB scope (D-34). Default keeps repository files on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FactoryResetScope {
    /// Wipe users/sessions/auth; keep bare repos under `OXIDEAN_REPOS_DIR`.
    #[default]
    DatabaseOnly,
    /// Wipe DB and delete repo files under `OXIDEAN_REPOS_DIR`.
    DatabaseAndRepositories,
}

/// Sys-admin factory reset — wipe users/sessions and restore empty-instance setup.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FactoryResetRequest {
    /// Must equal `RESET` (case-sensitive) to proceed.
    pub confirmation: String,
    /// Reset scope; omit → [`FactoryResetScope::DatabaseOnly`] (D-34).
    #[serde(default)]
    pub scope: FactoryResetScope,
}

/// Result of a successful factory reset.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FactoryResetResponse {
    pub ok: bool,
    pub needs_setup: bool,
}

/// Forced credential confirm for ENV-seeded admins (AUTH-06, D-16/D-17).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConfirmAdminCredentialsRequest {
    pub username: String,
    /// Optional; omit or empty keeps the current (ENV) email.
    #[serde(default)]
    pub email: Option<String>,
    /// Required when `keep_password` is false.
    #[serde(default)]
    pub password: Option<String>,
    /// When true, leave the existing password hash unchanged (D-17 Keep Switch).
    #[serde(default)]
    pub keep_password: bool,
}

/// Local signup input (D-01).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SignupRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

/// Local login input (D-02). `identifier` is email or username.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LoginRequest {
    pub identifier: String,
    pub password: String,
    pub remember_me: bool,
}

/// Public provider config for `auth.provider_config`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProviderConfigPublic {
    pub mode: ProviderMode,
    /// When false, local signup is closed (D-05/D-06); fail closed if unset at read.
    pub allow_signup: bool,
}

/// Profile update fields (D-18). Bio max length enforced in API (160).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateProfileRequest {
    pub display_name: String,
    pub username: String,
    pub bio: String,
    /// When set, updates the account default branch for new repos (D-09).
    #[serde(default)]
    pub default_branch: Option<String>,
}

/// `user.lookup` autocomplete input (ORG-01 / D-ORG-03). Prefix is matched on username only.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserLookupRequest {
    pub prefix: String,
    /// Optional scope for exclude/rank (PR-B contextual autocomplete).
    #[serde(default)]
    pub context: Option<UserLookupContext>,
}

/// Scope for contextual `user.lookup` ranking / exclusion.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UserLookupContext {
    Instance,
    Org { slug: String },
    Repo { owner: String, name: String },
}

/// Public autocomplete hit — never includes email (T-10-03).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserLookupHit {
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

/// `user.lookup` response — at most 10 hits.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserLookupResponse {
    pub users: Vec<UserLookupHit>,
}

/// Admin user row for `admin.users.list` (includes email + ban state).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUserPublic {
    pub id: String,
    pub email: String,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub email_verified: bool,
    pub banned_at: Option<String>,
    pub created_at: String,
}

/// `admin.users.list` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUsersListRequest {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// `admin.users.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUsersListResponse {
    pub users: Vec<AdminUserPublic>,
    pub total: i64,
}

/// `admin.users.updateRole` — only `user` ↔ `sys-admin`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUsersUpdateRoleRequest {
    pub user_id: String,
    pub role: Role,
}

/// Shared `{ user_id }` for ban / unban / revokeSessions.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUsersUserIdRequest {
    pub user_id: String,
}

/// `admin.users.delete` — confirmation must equal the target username.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUsersDeleteRequest {
    pub user_id: String,
    pub confirmation: String,
    /// Opt-in required when a sole-owner org still has other members —
    /// deleting the account would delete that org and its repositories.
    #[serde(default)]
    pub delete_orgs: Option<bool>,
}

/// `admin.users.delete` summary for the confirmation UI.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUsersDeleteResponse {
    pub ok: bool,
    pub deleted_repos: i64,
    pub deleted_orgs: i64,
}

/// Public pending instance invite — never includes token or token_hash.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InstanceInvitePublic {
    pub id: String,
    /// `None` = shareable link (not bound to a recipient email).
    pub email: Option<String>,
    /// `None` = never expires.
    pub expires_at: Option<String>,
    pub invited_by: String,
    pub created_at: String,
    /// `None` = unlimited seats.
    pub max_uses: Option<i64>,
    pub use_count: i64,
}

/// `admin.invites.create` — bulk: one email-bound invite per address.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminInvitesCreateRequest {
    pub emails: Vec<String>,
}

/// Per-recipient outcome of a bulk invite create.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminInvitesCreateItemResult {
    pub email: String,
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub invite: Option<InstanceInvitePublic>,
    /// One-time copyable URL (present only when `ok`).
    #[serde(default)]
    pub invite_url: Option<String>,
}

/// `admin.invites.create` response — one entry per submitted email.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminInvitesCreateResponse {
    pub results: Vec<AdminInvitesCreateItemResult>,
}

/// `admin.invites.createLink` — shareable link, no bound email.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminInvitesCreateLinkRequest {
    /// ISO-8601 UTC; `None`/absent = never expires.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// `None`/absent = unlimited seats.
    #[serde(default)]
    pub max_uses: Option<i64>,
}

/// `admin.invites.createLink` — invite metadata + one-time copyable URL.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminInvitesCreateLinkResponse {
    pub invite: InstanceInvitePublic,
    pub invite_url: String,
}

/// `admin.invites.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminInvitesListResponse {
    pub invites: Vec<InstanceInvitePublic>,
}

/// `admin.invites.revoke`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminInvitesRevokeRequest {
    pub invite_id: String,
}

/// `invites.get` — anonymous-safe invite preview (accept page).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InvitesGetRequest {
    pub token: String,
}

/// Invite preview — scope + constraints; never token internals.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InvitesGetResponse {
    /// `"instance" | "org" | "repo"`.
    pub kind: String,
    /// `Some` when the invite is bound to a recipient email (enforced on accept).
    pub email: Option<String>,
    /// `None` = never expires.
    pub expires_at: Option<String>,
    /// Seats remaining; `None` = unlimited.
    pub seats_remaining: Option<i64>,
    /// Org invite target (kind == "org").
    #[serde(default)]
    pub org_slug: Option<String>,
    #[serde(default)]
    pub org_display_name: Option<String>,
    /// Repo invite target (kind == "repo").
    #[serde(default)]
    pub repo_owner: Option<String>,
    #[serde(default)]
    pub repo_name: Option<String>,
    /// Role/permission granted on accept (org/repo invites).
    #[serde(default)]
    pub grant: Option<String>,
    /// `false` when expired, revoked, or out of seats.
    pub acceptable: bool,
    /// `"expired" | "revoked" | "exhausted"` — set when `acceptable` is false.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Unified `invites.accept` / `org.invites.accept` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InvitesAcceptRequest {
    pub token: String,
    /// Account email — required for anonymous accept of unbound link invites.
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Unified `invites.accept` response — instance signup, org membership, or repo collaborator.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InvitesAcceptResponse {
    Instance,
    Org {
        org: crate::OrgPublic,
        member: crate::OrgMemberPublic,
    },
    Repo {
        owner: String,
        name: String,
        permission: crate::CollaboratorPermission,
    },
}

/// Org membership summary for `admin.users.getAccess`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUserAccessOrg {
    pub slug: String,
    pub display_name: String,
    pub role: crate::OrgRole,
}

/// Repo collaborator grant summary for `admin.users.getAccess`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUserAccessRepo {
    pub owner: String,
    pub name: String,
    pub permission: crate::CollaboratorPermission,
}

/// `admin.users.getAccess` response — read-only memberships / grants.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUsersGetAccessResponse {
    pub orgs: Vec<AdminUserAccessOrg>,
    pub repos: Vec<AdminUserAccessRepo>,
}

/// Session row for the admin user view — never includes token or token_hash.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminSessionPublic {
    pub id: String,
    pub created_at: String,
    pub last_seen_at: String,
    pub expires_at: String,
    pub remember_me: bool,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

/// `admin.users.listSessions` response (input is the shared `{ user_id }` shape).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUsersListSessionsResponse {
    pub sessions: Vec<AdminSessionPublic>,
}

/// `admin.users.getActivity` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUsersGetActivityRequest {
    pub user_id: String,
    /// Source filter: `"audit"`, `"repository"`, or absent for both.
    #[serde(default)]
    pub source: Option<String>,
    /// Exact event-type filter (audit `event_type` / repo `push_type`).
    #[serde(default)]
    pub event_type: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// One row in the merged user activity feed (audit events + repo activity).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUserActivityItem {
    pub id: String,
    /// `"audit"` or `"repository"`.
    pub source: String,
    /// Audit `event_type` or repo `push_type`.
    pub event_type: String,
    pub created_at: String,
    // Audit fields.
    #[serde(default)]
    pub target_type: Option<String>,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    // Repository-activity fields.
    #[serde(default)]
    pub repo_owner: Option<String>,
    #[serde(default)]
    pub repo_name: Option<String>,
    #[serde(default)]
    pub ref_name: Option<String>,
    #[serde(default)]
    pub commits_count: Option<i64>,
    #[serde(default)]
    pub commit_message: Option<String>,
    #[serde(default)]
    pub pr_number: Option<i64>,
}

/// `admin.users.getActivity` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminUsersGetActivityResponse {
    pub items: Vec<AdminUserActivityItem>,
    /// Distinct event types seen for this user (filter UI).
    pub event_types: Vec<String>,
}

/// Instance auth settings for admin UI — secrets never returned; ENV badges only (D-09, T-04-22).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AuthSettingsPublic {
    pub provider_mode: ProviderMode,
    pub email_provider: EmailProviderKind,
    pub from_address: Option<String>,
    /// Non-secret WorkOS client id (display); may also come from ENV when DB empty.
    pub workos_client_id: Option<String>,
    pub oidc_issuer: Option<String>,
    pub oidc_client_id: Option<String>,
    pub smtp_configured: bool,
    pub resend_configured: bool,
    pub workos_api_key_configured: bool,
    pub oidc_client_secret_configured: bool,
    pub allow_signup: bool,
    /// Instance default visibility for new repos (D-08).
    #[serde(default = "default_visibility_public")]
    pub default_visibility: crate::RepoVisibility,
}

fn default_visibility_public() -> crate::RepoVisibility {
    crate::RepoVisibility::Public
}

/// Admin update payload — non-secret fields only.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateAuthSettingsRequest {
    pub provider_mode: ProviderMode,
    pub email_provider: EmailProviderKind,
    pub from_address: Option<String>,
    pub oidc_issuer: Option<String>,
    pub oidc_client_id: Option<String>,
    pub workos_client_id: Option<String>,
    #[serde(default)]
    pub allow_signup: bool,
    /// Instance default visibility for new repos (D-08).
    #[serde(default = "default_visibility_public")]
    pub default_visibility: crate::RepoVisibility,
}

const RESERVED_USERNAMES: &[&str] = &[
    "admin",
    "api",
    "settings",
    "login",
    "signup",
    "logout",
    "status",
    "dashboard",
    "explore",
    "orgs",
    "org",
    "help",
    "support",
    "www",
    "root",
    "system",
    "null",
    "undefined",
    "oxidean",
    "assets",
    "static",
    "uploads",
    "health",
    "rpc",
    "auth",
    "account",
    "profile",
    "robots",
    "favicon",
    "verify",
    "reset-password",
    "setup",
    "system-administrator",
    // Phase 7 flat routes (D-14) — must not collide with /{owner}/{repo}
    "new",
    "commits",
    "branches",
    "tags",
    "compare",
    "blame",
    "tree",
    "blob",
    "raw",
    // Phase 8 Basic-auth username aliases (D-10) — must not collide with accounts
    "git",
    "token",
    "oauth2",
    // Phase 20 registry path prefixes (D-PKG-01) — must not collide with owners
    "v2",
    "npm",
    "generic",
    // Static-asset dirs under apps/web/dist — an owner named e.g. `brand`
    // would claim `/brand/*` asset space and its own dotted repos would 404
    // (`/{owner}/{repo}` prefers existing dist entries). `_astro` needs no
    // entry — `_` is illegal in usernames.
    "brand",
    "emoji",
    "fonts",
    "icons",
];

/// Returns true if `u` matches a reserved username (case-insensitive).
pub fn is_reserved_username(u: &str) -> bool {
    let lower = u.to_ascii_lowercase();
    RESERVED_USERNAMES.contains(&lower.as_str())
}

/// Username rules (D-03): 1–39 chars, ascii alphanumeric + hyphen,
/// no leading/trailing hyphen, not reserved.
pub fn validate_username(raw: &str) -> Result<(), String> {
    let u = raw.trim();
    if u.is_empty() || u.len() > 39 {
        return Err("username must be 1–39 characters".into());
    }
    if u.starts_with('-') || u.ends_with('-') {
        return Err("username cannot start or end with a hyphen".into());
    }
    if !u.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("username must be alphanumeric or hyphen".into());
    }
    if is_reserved_username(u) {
        return Err("username is reserved".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_username_accepts_short_ok() {
        assert!(validate_username("ab").is_ok());
    }

    #[test]
    fn validate_username_rejects_leading_hyphen() {
        assert!(validate_username("-ab").is_err());
    }

    #[test]
    fn validate_username_rejects_trailing_hyphen() {
        assert!(validate_username("ab-").is_err());
    }

    #[test]
    fn validate_username_rejects_reserved() {
        let err = validate_username("admin").unwrap_err();
        assert!(err.contains("reserved"));
        // dist/ asset dirs — an owner sharing the name would shadow assets
        // and its own dotted repos would 404 at the web tier.
        for name in ["brand", "Brand", "EMOJI", "fonts", "icons"] {
            assert!(
                validate_username(name).is_err(),
                "{name} must be reserved (dist/ asset dir)"
            );
        }
    }

    #[test]
    fn validate_username_rejects_verify_and_reset_password() {
        assert!(is_reserved_username("verify"));
        assert!(is_reserved_username("Verify"));
        assert!(is_reserved_username("reset-password"));
        assert!(validate_username("verify").is_err());
        assert!(validate_username("reset-password").is_err());
    }

    #[test]
    fn validate_username_rejects_too_long() {
        let long = "a".repeat(40);
        assert!(validate_username(&long).is_err());
    }

    #[test]
    fn provider_mode_serde_lowercase() {
        let json = serde_json::to_string(&ProviderMode::Local).unwrap();
        assert_eq!(json, "\"local\"");
        let mode: ProviderMode = serde_json::from_str("\"workos\"").unwrap();
        assert_eq!(mode, ProviderMode::Workos);
    }

    /// Wave 06-01: ENV seed username must stay reserved for signup (Pitfall 5 / A4).
    #[test]
    fn system_administrator_is_reserved() {
        assert!(is_reserved_username("system-administrator"));
        assert!(is_reserved_username("System-Administrator"));
        assert!(validate_username("system-administrator").is_err());
    }

    /// Wave 07-02 / D-14: flat `/new` must stay off the owner slug namespace.
    #[test]
    fn new_is_reserved_for_flat_routes() {
        assert!(is_reserved_username("new"));
        assert!(is_reserved_username("New"));
        assert!(validate_username("new").is_err());
    }

    /// Wave 08-03 / D-10: Basic-auth username aliases must not be claimable accounts.
    #[test]
    fn git_token_oauth2_are_reserved_for_basic_aliases() {
        for name in ["git", "token", "oauth2", "Git", "TOKEN", "OAuth2"] {
            assert!(
                is_reserved_username(name),
                "{name} must be reserved (D-10 aliases)"
            );
            assert!(validate_username(name).is_err());
        }
    }

    #[test]
    fn user_public_json_includes_must_change_credentials() {
        let json = serde_json::json!({
            "id": "1",
            "email": "a@b.co",
            "username": "u",
            "display_name": "d",
            "bio": "",
            "avatar_url": null,
            "role": "user",
            "profile_incomplete": false,
            "email_verified": false,
            "must_change_credentials": true
        });
        let user: UserPublic = serde_json::from_value(json).unwrap();
        let back = serde_json::to_value(&user).unwrap();
        assert_eq!(back["must_change_credentials"], true);
    }

    #[test]
    fn provider_config_and_bootstrap_setup_include_allow_signup() {
        let cfg_json = serde_json::json!({ "mode": "local", "allow_signup": true });
        let cfg: ProviderConfigPublic = serde_json::from_value(cfg_json).unwrap();
        let cfg_back = serde_json::to_value(&cfg).unwrap();
        assert_eq!(cfg_back["allow_signup"], true);

        let setup_json = serde_json::json!({
            "email": "a@b.co",
            "username": "alice",
            "password": "password1",
            "allow_signup": true
        });
        let setup: BootstrapSetupRequest = serde_json::from_value(setup_json).unwrap();
        let setup_back = serde_json::to_value(&setup).unwrap();
        assert_eq!(setup_back["allow_signup"], true);
    }

    #[test]
    fn auth_settings_dtos_include_allow_signup() {
        let pub_json = serde_json::json!({
            "provider_mode": "local",
            "email_provider": "log",
            "from_address": null,
            "workos_client_id": null,
            "oidc_issuer": null,
            "oidc_client_id": null,
            "smtp_configured": false,
            "resend_configured": false,
            "workos_api_key_configured": false,
            "oidc_client_secret_configured": false,
            "allow_signup": true
        });
        let settings: AuthSettingsPublic = serde_json::from_value(pub_json).unwrap();
        assert_eq!(serde_json::to_value(&settings).unwrap()["allow_signup"], true);

        let upd_json = serde_json::json!({
            "provider_mode": "local",
            "email_provider": "log",
            "from_address": null,
            "oidc_issuer": null,
            "oidc_client_id": null,
            "workos_client_id": null,
            "allow_signup": false
        });
        let upd: UpdateAuthSettingsRequest = serde_json::from_value(upd_json).unwrap();
        assert_eq!(serde_json::to_value(&upd).unwrap()["allow_signup"], false);
    }
}
